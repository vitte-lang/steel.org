//! Path canonicalization utilities (canon.rs) — MAX.
//!
//! Goals for Steel path handling:
//! - deterministic, cross-platform canonical representation for manifests and indices
//! - safe joining (no traversal outside base)
//! - normalization (slashes, '.' segments, '..' segments)
//! - optional filesystem canonicalization (resolve symlinks) where needed
//! - minimal allocations, std-only
//!
//! This module provides:
//! - `CanonPath`: an owned canonical path representation (portable string + PathBuf)
//! - normalization helpers (pure, no fs I/O)
//! - safe join helpers
//! - best-effort fs canonicalization with clear semantics
//!
//! Terminology:
//! - *normalize*: pure string/path normalization without touching the filesystem.
//! - *canonicalize_fs*: uses std::fs::canonicalize (resolves symlinks, requires fs).
//!
//! Notes:
//! - Windows: drive prefixes are preserved in `PathBuf` forms, but portable forms
//!   use forward slashes and no verbatim prefixes.
//! - For bundle paths (MFF), prefer `normalize_rel_unix()` outputs.

use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum CanonError {
    Invalid(&'static str),
    UnsafePath(String),
    Io(std::io::Error),
}

impl fmt::Display for CanonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonError::Invalid(s) => write!(f, "invalid: {s}"),
            CanonError::UnsafePath(p) => write!(f, "unsafe path: {p}"),
            CanonError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for CanonError {}

impl From<std::io::Error> for CanonError {
    fn from(e: std::io::Error) -> Self {
        CanonError::Io(e)
    }
}

fn unsafe_path(p: impl Into<String>) -> CanonError {
    CanonError::UnsafePath(p.into())
}

/// Canonical portable path representation.
///
/// `portable` is a forward-slash path suitable for stable hashing / manifests.
/// It can represent absolute paths (with a prefix) or relative paths.
///
/// Examples:
/// - Unix abs:  "/home/vince/proj"   -> "home/vince/proj" with `is_abs_unix=true`
/// - Windows:   "C:\a\b"             -> "C:/a/b"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonPath {
    portable: String,
    native: PathBuf,
}

impl CanonPath {
    /// Create from a native path, with normalization only (no fs I/O).
    pub fn new_normalized(p: impl AsRef<Path>) -> Result<Self, CanonError> {
        let p = p.as_ref();
        let portable = normalize_portable(p)?;
        let native = portable_to_native(&portable);
        Ok(Self { portable, native })
    }

    /// Create by filesystem canonicalization (resolves symlinks).
    /// This produces a stable absolute native path and a derived portable form.
    pub fn new_fs_canonical(p: impl AsRef<Path>) -> Result<Self, CanonError> {
        let abs = std::fs::canonicalize(p)?;
        let portable = normalize_portable(&abs)?;
        Ok(Self {
            portable,
            native: abs,
        })
    }

    /// Portable representation (stable, forward slashes).
    pub fn portable(&self) -> &str {
        &self.portable
    }

    /// Native path representation.
    pub fn native(&self) -> &Path {
        &self.native
    }

    /// Join a relative path onto this base, safely.
    /// Rejects absolute paths and traversal attempts.
    pub fn join_safe_rel(&self, rel: impl AsRef<Path>) -> Result<CanonPath, CanonError> {
        let rel = rel.as_ref();
        if rel.is_absolute() {
            return Err(unsafe_path(rel.display().to_string()));
        }
        let rel_norm = normalize_rel_unix(rel)?;
        if rel_norm.is_empty() {
            return Err(CanonError::Invalid("empty rel path"));
        }

        let joined_portable = if self.portable.is_empty() {
            rel_norm
        } else {
            format!("{}/{}", self.portable.trim_end_matches('/'), rel_norm)
        };

        Ok(CanonPath {
            portable: joined_portable.clone(),
            native: portable_to_native(&joined_portable),
        })
    }

    /// Convert to PathBuf (native).
    pub fn into_native(self) -> PathBuf {
        self.native
    }
}

/* -------------------------- Pure normalization --------------------------- */

/// Normalize any path into a portable, forward-slash string.
///
/// Behavior:
/// - removes `.` segments
/// - resolves `..` segments *within the path string* by popping
/// - preserves Windows drive letter as prefix `C:` if present
/// - never emits backslashes
///
/// This is NOT a security function; for security, use `normalize_rel_unix`
/// + `join_under_base` or `reject_traversal`.
pub fn normalize_portable(p: &Path) -> Result<String, CanonError> {
    let mut out: Vec<String> = Vec::new();
    let mut prefix: Option<String> = None;
    let mut is_abs = false;

    for c in p.components() {
        match c {
            Component::Prefix(pr) => {
                // Windows prefix (C:, UNC, etc.)
                // Keep drive-letter form best-effort.
                prefix = Some(pr.as_os_str().to_string_lossy().to_string());
            }
            Component::RootDir => {
                is_abs = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_string_lossy().to_string()),
        }
    }

    let mut s = String::new();
    if let Some(pr) = prefix {
        // Convert "\" to "/" if any in prefix.
        let pr = pr.replace('\\', "/");
        s.push_str(&pr);
        // Ensure "C:" stays "C:" and join with "/"
        if !s.ends_with(':') {
            // UNC prefix etc: add separator
            if !s.ends_with('/') {
                s.push('/');
            }
        } else {
            s.push('/');
        }
    } else if is_abs {
        // unix absolute: keep leading "/" semantic, but portable form doesn't keep leading slash
        // (to avoid empty segment). We just mark by leaving it without special token.
        // If you need leading slash, store separately.
    }

    s.push_str(&out.join("/"));
    Ok(s.trim_matches('/').to_string())
}

/// Normalize a relative path into unix-style string, rejecting unsafe constructs.
///
/// Rejections:
/// - absolute paths
/// - windows prefixes
/// - any `..` that would traverse above base
///
/// Output:
/// - forward slashes
/// - no empty segments
pub fn normalize_rel_unix(p: &Path) -> Result<String, CanonError> {
    if p.is_absolute() {
        return Err(unsafe_path(p.display().to_string()));
    }

    let mut parts: Vec<String> = Vec::new();

    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                return Err(unsafe_path(p.display().to_string()));
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    // would traverse above base
                    return Err(unsafe_path(p.display().to_string()));
                }
            }
            Component::Normal(s) => {
                let seg = s.to_string_lossy().to_string();
                if seg.is_empty() || seg == "." || seg == ".." {
                    return Err(unsafe_path(p.display().to_string()));
                }
                parts.push(seg);
            }
        }
    }

    Ok(parts.join("/"))
}

/// Reject traversal for a bundle path string (already normalized-ish).
pub fn reject_traversal_str(s: &str) -> Result<(), CanonError> {
    if s.is_empty() {
        return Err(CanonError::Invalid("empty path"));
    }
    if s.starts_with('/') || s.starts_with('\\') {
        return Err(unsafe_path(s));
    }
    if s.contains(':') {
        return Err(unsafe_path(s)); // drive letters / schemes
    }
    if s.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return Err(unsafe_path(s));
    }
    Ok(())
}

/* --------------------------- Safe joining -------------------------------- */

/// Join a relative unix path (string) under a base directory (native Path).
/// Ensures the resulting path stays under base (lexically).
pub fn join_under_base(base: &Path, rel_unix: &str) -> Result<PathBuf, CanonError> {
    reject_traversal_str(rel_unix)?;
    let rel = portable_to_native(rel_unix);
    let joined = base.join(rel);

    // Lexical containment check (best effort): normalize joined and base and compare prefixes.
    // This does not resolve symlinks. For symlink-safe containment, use fs-canonicalization.
    let base_n = normalize_native_lex(base);
    let joined_n = normalize_native_lex(&joined);

    if !joined_n.starts_with(&base_n) {
        return Err(unsafe_path(rel_unix));
    }

    Ok(joined)
}

/// Lexically normalize a native path (no fs).
fn normalize_native_lex(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/* ---------------------- Portable/native conversion ----------------------- */

/// Convert a portable forward-slash path to a native PathBuf.
/// On Windows, `/` will be interpreted as separators by PathBuf join rules.
pub fn portable_to_native(portable: &str) -> PathBuf {
    // keep as-is; PathBuf will accept forward slashes.
    PathBuf::from(portable)
}

/* --------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rel_rejects_parent_escape() {
        assert!(normalize_rel_unix(Path::new("../x")).is_err());
        assert!(normalize_rel_unix(Path::new("a/../../x")).is_err());
    }

    #[test]
    fn normalize_rel_ok() {
        let s = normalize_rel_unix(Path::new("a/b/./c")).unwrap();
        assert_eq!(s, "a/b/c");
    }

    #[test]
    fn join_under_base_ok() {
        let base = Path::new("out");
        let p = join_under_base(base, "a/b.txt").unwrap();
        assert!(p.to_string_lossy().contains("out"));
    }

    #[test]
    fn reject_traversal_str_blocks_abs() {
        assert!(reject_traversal_str("/etc/passwd").is_err());
        assert!(reject_traversal_str("C:/x").is_err());
        assert!(reject_traversal_str("a/../b").is_err());
    }
}
