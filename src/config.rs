// /Users/vincent/Documents/Github/steel/src/config.rs
//! config — configuration coherence + invariants (std-only)
//!
//! Validator layer for Steel configuration objects.
//! This module is intentionally independent from any parser implementation:
//! it validates *shapes*, *paths*, and *selection invariants* (profile/target/toolchain)
//! and can be used both before and after resolution.
//!
//! Typical usage:
//! - validate a resolved config produced by `build_muf`
//! - validate workspace-level invariants (paths under root, distinct output dirs, etc.)
//!
//! Non-goals (by design):
//! - full semantic validation of the SteelConfig language
//! - dependency resolution correctness (owned by resolver layer)
//!
//! This module provides:
//! - `Diagnostic` / `ValidationReport`
//! - `validate_resolved_config()` for `build_muf::ResolvedConfig`
//! - small reusable validators (profile name, target triple, paths, toolchain)

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl Diagnostic {
    pub fn info(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code,
            message: msg.into(),
            path: None,
        }
    }

    pub fn warn(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: msg.into(),
            path: None,
        }
    }

    pub fn err(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: msg.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, p: impl Into<PathBuf>) -> Self {
        self.path = Some(p.into());
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn extend(&mut self, other: ValidationReport) {
        self.diagnostics.extend(other.diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.diagnostics {
            let sev = match d.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            };
            if let Some(p) = &d.path {
                writeln!(f, "[{sev}] {}: {} ({})", d.code, d.message, p.display())?;
            } else {
                writeln!(f, "[{sev}] {}: {}", d.code, d.message)?;
            }
        }
        Ok(())
    }
}

/// Minimal “policy” knobs for config validation.
#[derive(Debug, Clone)]
pub struct ConfigPolicy {
    /// Require build/dist/cache directories to be under project root.
    pub require_outputs_under_root: bool,
    /// Forbid output directories inside `.git` or `.hg` etc.
    pub forbid_vcs_dirs: bool,
    /// Require profile to match identifier rules.
    pub strict_profile_name: bool,
    /// Require target to look like a triple (>= 3 dash-separated atoms).
    pub strict_target_triple: bool,
    /// If true, warn when toolchain entries are missing.
    pub warn_missing_toolchain: bool,
}

impl Default for ConfigPolicy {
    fn default() -> Self {
        Self {
            require_outputs_under_root: true,
            forbid_vcs_dirs: true,
            strict_profile_name: true,
            strict_target_triple: true,
            warn_missing_toolchain: true,
        }
    }
}

/// Validate a resolved configuration produced by `build_muf`.
///
/// This function is best-effort: it accumulates diagnostics and never panics.
pub fn validate_resolved_config(cfg: &crate::build_muf::ResolvedConfig, policy: &ConfigPolicy) -> ValidationReport {
    let mut r = ValidationReport::default();

    // Root invariants
    if !cfg.project_root.is_dir() {
        r.push(
            Diagnostic::err("CFG_ROOT_NOT_DIR", "project root is not a directory")
                .with_path(cfg.project_root.clone()),
        );
    }

    // SteelConfig invariants
    if !cfg.steelfile_path.is_file() {
        r.push(
            Diagnostic::err("CFG_MUFFINFILE_MISSING", "SteelConfig path does not exist or is not a file")
                .with_path(cfg.steelfile_path.clone()),
        );
    }

    // Profile invariants
    r.extend(validate_profile(&cfg.profile, policy));

    // Target invariants
    r.extend(validate_target_triple(&cfg.target, policy));

    // Paths invariants
    r.extend(validate_output_paths(
        &cfg.project_root,
        &cfg.paths.build_dir,
        &cfg.paths.dist_dir,
        &cfg.paths.cache_dir,
        policy,
    ));

    // Toolchain invariants
    r.extend(validate_toolchain(&cfg.toolchain, policy));

    // Vars invariants
    r.extend(validate_vars(&cfg.vars));

    // Fingerprint invariants (format)
    r.extend(validate_fingerprint(&cfg.fingerprint));

    r
}

/// Validate a profile name.
/// Default policy: must be a “config identifier”: `[A-Za-z_][A-Za-z0-9._-]*`
pub fn validate_profile(profile: &str, policy: &ConfigPolicy) -> ValidationReport {
    let mut r = ValidationReport::default();

    let p = profile.trim();
    if p.is_empty() {
        r.push(Diagnostic::err("CFG_PROFILE_EMPTY", "profile is empty"));
        return r;
    }

    if policy.strict_profile_name && !is_profile_ident(p) {
        r.push(Diagnostic::err(
            "CFG_PROFILE_INVALID",
            format!("invalid profile name: {p} (expected [A-Za-z_][A-Za-z0-9._-]*)"),
        ));
    }

    // common conventions
    if p.eq_ignore_ascii_case("debug") || p.eq_ignore_ascii_case("release") {
        // ok
    } else if p.len() > 64 {
        r.push(Diagnostic::warn(
            "CFG_PROFILE_LONG",
            format!("profile name is unusually long (len={}): {p}", p.len()),
        ));
    }

    r
}

/// Validate a target triple in a conservative way (std-only, no regex).
///
/// Rules (strict):
/// - at least 3 non-empty dash-separated components
/// - only `[A-Za-z0-9_+.]+` chars per component
pub fn validate_target_triple(triple: &str, policy: &ConfigPolicy) -> ValidationReport {
    let mut r = ValidationReport::default();

    let t = triple.trim();
    if t.is_empty() {
        r.push(Diagnostic::err("CFG_TARGET_EMPTY", "target triple is empty"));
        return r;
    }

    if policy.strict_target_triple {
        let parts: Vec<&str> = t.split('-').filter(|s| !s.is_empty()).collect();
        if parts.len() < 3 {
            r.push(Diagnostic::err(
                "CFG_TARGET_NOT_TRIPLE",
                format!("target does not look like a triple (need >=3 dash-separated atoms): {t}"),
            ));
            return r;
        }

        for atom in parts {
            if !is_triple_atom(atom) {
                r.push(Diagnostic::err(
                    "CFG_TARGET_BAD_ATOM",
                    format!("target triple contains invalid atom: {atom}"),
                ));
            }
        }
    }

    r
}

/// Validate output dirs build/dist/cache.
pub fn validate_output_paths(
    root: &Path,
    build_dir: &Path,
    dist_dir: &Path,
    cache_dir: &Path,
    policy: &ConfigPolicy,
) -> ValidationReport {
    let mut r = ValidationReport::default();

    // distinctness
    if build_dir == dist_dir {
        r.push(Diagnostic::err(
            "CFG_PATH_BUILD_EQ_DIST",
            "build_dir and dist_dir must be distinct",
        ));
    }
    if build_dir == cache_dir {
        r.push(Diagnostic::err(
            "CFG_PATH_BUILD_EQ_CACHE",
            "build_dir and cache_dir must be distinct",
        ));
    }
    if dist_dir == cache_dir {
        r.push(Diagnostic::err(
            "CFG_PATH_DIST_EQ_CACHE",
            "dist_dir and cache_dir must be distinct",
        ));
    }

    // under root (best effort)
    if policy.require_outputs_under_root {
        for (label, p) in [("build", build_dir), ("dist", dist_dir), ("cache", cache_dir)] {
            if p.is_absolute() {
                // ok; still enforce under root if possible
                if p.strip_prefix(root).is_err() {
                    r.push(Diagnostic::err(
                        "CFG_PATH_OUTSIDE_ROOT",
                        format!("{label} dir is outside project root"),
                    ).with_path(p.to_path_buf()));
                }
            } else {
                // relative path should resolve under root
                let joined = root.join(p);
                if joined.strip_prefix(root).is_err() {
                    r.push(Diagnostic::err(
                        "CFG_PATH_REL_ESCAPE",
                        format!("{label} dir escapes project root"),
                    ).with_path(joined));
                }
            }
        }
    }

    // forbid VCS dirs
    if policy.forbid_vcs_dirs {
        for (label, p) in [("build", build_dir), ("dist", dist_dir), ("cache", cache_dir)] {
            let full = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
            if contains_forbidden_segment(&full, &[".git", ".hg", ".svn"]) {
                r.push(
                    Diagnostic::err(
                        "CFG_PATH_IN_VCS",
                        format!("{label} dir must not be inside a VCS directory"),
                    )
                    .with_path(full),
                );
            }
        }
    }

    // hygiene warnings
    for (label, p) in [("build", build_dir), ("dist", dist_dir), ("cache", cache_dir)] {
        let s = p.to_string_lossy();
        if s.contains("//") || s.contains("\\\\") {
            r.push(Diagnostic::warn(
                "CFG_PATH_SUSPECT",
                format!("{label} dir contains repeated separators: {}", s),
            ));
        }
    }

    r
}

/// Validate toolchain entries.
/// In strict mode you likely want these to be resolved elsewhere; here we only check basic shape.
pub fn validate_toolchain(tc: &crate::build_muf::ToolchainInfo, policy: &ConfigPolicy) -> ValidationReport {
    let mut r = ValidationReport::default();

    // Basic “missing toolchain” heuristics
    if policy.warn_missing_toolchain {
        if tc.cc.is_none() && tc.cxx.is_none() && tc.rustc.is_none() {
            r.push(Diagnostic::warn(
                "CFG_TOOLCHAIN_EMPTY",
                "toolchain has no known compiler entries (CC/CXX/RUSTC)",
            ));
        }
    }

    // Validate tool strings (not empty, no NUL, etc.)
    for (k, v) in [
        ("cc", tc.cc.as_deref()),
        ("cxx", tc.cxx.as_deref()),
        ("ar", tc.ar.as_deref()),
        ("ld", tc.ld.as_deref()),
        ("rustc", tc.rustc.as_deref()),
    ] {
        if let Some(tool) = v {
            let t = tool.trim();
            if t.is_empty() {
                r.push(Diagnostic::err("CFG_TOOL_EMPTY", format!("{k} tool is empty")));
            }
            if t.contains('\0') {
                r.push(Diagnostic::err("CFG_TOOL_NUL", format!("{k} tool contains NUL byte")));
            }
            if looks_like_path_with_spaces(t) && !is_quoted(t) {
                r.push(Diagnostic::warn(
                    "CFG_TOOL_SPACES",
                    format!("{k} tool contains spaces; quoting may be required: {t}"),
                ));
            }
        }
    }

    // Versions map should be deterministic and trimmed.
    for (k, v) in &tc.versions {
        if k.trim().is_empty() {
            r.push(Diagnostic::warn("CFG_TOOLVER_KEY_EMPTY", "tool version key is empty"));
        }
        if v.trim().is_empty() {
            r.push(Diagnostic::warn(
                "CFG_TOOLVER_VAL_EMPTY",
                format!("tool version line is empty for key={k}"),
            ));
        }
        if v.contains('\n') {
            r.push(Diagnostic::warn(
                "CFG_TOOLVER_MULTILINE",
                format!("tool version line contains newline for key={k}"),
            ));
        }
    }

    r
}

/// Validate vars map: keys unique (guaranteed by map), basic shapes.
pub fn validate_vars(vars: &BTreeMap<String, String>) -> ValidationReport {
    let mut r = ValidationReport::default();

    for (k, v) in vars {
        let kt = k.trim();
        if kt.is_empty() {
            r.push(Diagnostic::err("CFG_VAR_KEY_EMPTY", "variable key is empty"));
        }
        if k.contains('\n') || k.contains('\0') {
            r.push(Diagnostic::err(
                "CFG_VAR_KEY_BAD",
                format!("variable key contains invalid characters: {k:?}"),
            ));
        }
        if v.contains('\0') {
            r.push(Diagnostic::err(
                "CFG_VAR_VAL_NUL",
                format!("variable value contains NUL for key={k}"),
            ));
        }
        if kt.len() > 256 {
            r.push(Diagnostic::warn(
                "CFG_VAR_KEY_LONG",
                format!("variable key is unusually long (len={}): {}", kt.len(), k),
            ));
        }
    }

    r
}

/// Validate fingerprint string emitted by build_muf.
/// Expected: `fnv1a64:<16 hex>`
pub fn validate_fingerprint(fp: &str) -> ValidationReport {
    let mut r = ValidationReport::default();

    let fpt = fp.trim();
    if fpt.is_empty() {
        r.push(Diagnostic::err("CFG_FP_EMPTY", "fingerprint is empty"));
        return r;
    }

    const PREFIX: &str = "fnv1a64:";
    if !fpt.starts_with(PREFIX) {
        r.push(Diagnostic::warn(
            "CFG_FP_PREFIX",
            format!("fingerprint does not start with {PREFIX}"),
        ));
        return r;
    }

    let hex = &fpt[PREFIX.len()..];
    if hex.len() != 16 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        r.push(Diagnostic::warn(
            "CFG_FP_SHAPE",
            format!("fingerprint hex part should be 16 hex chars, got: {hex}"),
        ));
    }

    r
}

fn is_profile_ident(s: &str) -> bool {
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' ))
}

fn is_triple_atom(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '.' ))
}

fn contains_forbidden_segment(p: &Path, forbidden: &[&str]) -> bool {
    p.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        forbidden.iter().any(|f| s == *f)
    })
}

fn looks_like_path_with_spaces(s: &str) -> bool {
    s.contains(' ') && (s.contains('/') || s.contains('\\') || s.contains(':'))
}

fn is_quoted(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))
}

/// Optional helper: normalize a path without `canonicalize()` (best-effort).
pub fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        use std::path::Component;
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Optional helper: check if a directory name should be ignored by policy.
/// This is useful for scanners and validators sharing a common ignore vocabulary.
pub fn is_ignored_dir_name(name: &OsStr) -> bool {
    matches!(
        name.to_string_lossy().as_ref(),
        ".git" | ".hg" | ".svn" | "target" | "node_modules" | "dist" | "build" | ".steel" | ".steel-cache"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_ident_rules() {
        assert!(is_profile_ident("debug"));
        assert!(is_profile_ident("_custom-1"));
        assert!(is_profile_ident("rel.v1"));
        assert!(!is_profile_ident("1bad"));
        assert!(!is_profile_ident("bad space"));
        assert!(!is_profile_ident(""));
    }

    #[test]
    fn triple_atom_rules() {
        assert!(is_triple_atom("x86_64"));
        assert!(is_triple_atom("unknown"));
        assert!(is_triple_atom("linux"));
        assert!(is_triple_atom("gnu.2"));
        assert!(is_triple_atom("msvc+crt"));
        assert!(!is_triple_atom(""));
        assert!(!is_triple_atom("bad-atom"));
        assert!(!is_triple_atom("bad atom"));
    }

    #[test]
    fn fingerprint_shape() {
        let r = validate_fingerprint("fnv1a64:0123456789abcdef");
        assert!(!r.has_errors());

        let r = validate_fingerprint("sha1:deadbeef");
        assert!(!r.has_errors()); // warning only

        let r = validate_fingerprint("");
        assert!(r.has_errors());
    }

    #[test]
    fn output_dirs_distinctness() {
        let policy = ConfigPolicy::default();
        let root = Path::new("/tmp/project");
        let r = validate_output_paths(root, Path::new("build"), Path::new("build"), Path::new(".steel-cache"), &policy);
        assert!(r.has_errors());
    }
}

pub struct Config {
    pub output: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // ...existing code...
        Ok(Config {
            output: "build/.mff".to_string(),
        })
    }
}
