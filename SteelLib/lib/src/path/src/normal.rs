//! Path normalization helpers (normal.rs) — MAX.
//!
//! This module provides small, focused normalization routines used throughout
//! Steel. It complements `canon.rs`:
//! - `canon.rs` focuses on canonical representations + safe join logic.
//! - `normal.rs` focuses on pure normalization transforms and tiny utilities.
//!
//! Provided:
//! - `normalize_slashes_*` (convert to '/' or platform separators)
//! - `clean_lexical_*` (resolve '.' and '..' lexically without fs I/O)
//! - `split/join` helpers for stable path keys
//! - `is_hidden`, `file_stem_utf8`, `ext_utf8`
//!
//! Design constraints:
//! - std-only
//! - deterministic output
//! - no allocation where reasonable (but returns `String` for stable keys)

use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum NormalError {
    Invalid(&'static str),
}

impl std::fmt::Display for NormalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NormalError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for NormalError {}

/// Convert all backslashes to forward slashes (string-level).
pub fn normalize_slashes_to_unix(s: &str) -> String {
    s.replace('\\', "/")
}

/// Convert forward slashes to platform separator (string-level).
pub fn normalize_slashes_to_native(s: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        s.to_string()
    } else {
        s.replace('/', &std::path::MAIN_SEPARATOR.to_string())
    }
}

/// Lexically clean a path:
/// - removes `.`
/// - resolves `..` by popping a segment when possible
/// - does not touch filesystem
///
/// If `keep_root` is true, preserves leading root/prefix components.
pub fn clean_lexical(p: &Path, keep_root: bool) -> PathBuf {
    let mut out = PathBuf::new();

    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::Normal(s) => out.push(s),
            Component::RootDir | Component::Prefix(_) => {
                if keep_root {
                    out.push(c.as_os_str());
                }
            }
        }
    }

    out
}

/// Clean lexical with `keep_root=true`.
pub fn clean_lexical_keep_root(p: &Path) -> PathBuf {
    clean_lexical(p, true)
}

/// Clean lexical with `keep_root=false`.
pub fn clean_lexical_drop_root(p: &Path) -> PathBuf {
    clean_lexical(p, false)
}

/// Return a stable, forward-slash key for a path, dropping drive/root prefixes.
///
/// Useful for maps/indices.
pub fn key_unix(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in clean_lexical_drop_root(p).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

/// Split a unix key (`a/b/c`) into segments.
pub fn split_key_unix(key: &str) -> Vec<&str> {
    key.split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect()
}

/// Join unix segments into a key.
pub fn join_key_unix<I, S>(segs: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for (i, s) in segs.into_iter().enumerate() {
        let s = s.as_ref();
        if s.is_empty() || s == "." {
            continue;
        }
        if i != 0 && !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(s.trim_matches('/'));
    }
    out
}

/// Returns true if path filename begins with '.' (unix-style hidden).
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// UTF-8 file stem, if valid.
pub fn file_stem_utf8(p: &Path) -> Option<&str> {
    p.file_stem().and_then(|s| s.to_str())
}

/// UTF-8 extension, if valid.
pub fn ext_utf8(p: &Path) -> Option<&str> {
    p.extension().and_then(|s| s.to_str())
}

/// Ensure a relative path does not contain traversal above base (lexical).
/// Returns a cleaned relative PathBuf.
/// Rejects absolute paths and windows prefixes.
pub fn clean_rel_no_escape(p: &Path) -> Result<PathBuf, NormalError> {
    if p.is_absolute() {
        return Err(NormalError::Invalid("absolute path"));
    }

    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => return Err(NormalError::Invalid("has prefix/root")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(NormalError::Invalid("escapes base"));
                }
            }
            Component::Normal(s) => out.push(s),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_norm() {
        assert_eq!(normalize_slashes_to_unix(r"a\b\c"), "a/b/c");
    }

    #[test]
    fn key_unix_basic() {
        let k = key_unix(Path::new("a/./b/../c"));
        assert_eq!(k, "a/c");
    }

    #[test]
    fn clean_rel_no_escape_ok() {
        let p = clean_rel_no_escape(Path::new("a/b/../c")).unwrap();
        assert_eq!(p, PathBuf::from("a/c"));
    }

    #[test]
    fn clean_rel_no_escape_reject() {
        assert!(clean_rel_no_escape(Path::new("../x")).is_err());
    }
}
