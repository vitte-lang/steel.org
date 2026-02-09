// C:\Users\gogin\Documents\GitHub\steel\src\vms_progname.rs
//
// Steel — VMS (Virtual Steel System) utilities
// Program name resolution (human-friendly progname) for diagnostics/logging.
//
// Goals:
// - Stable, cross-platform progname derivation.
// - Prefer explicit override (env / provided argv[0]).
// - Fallback to current executable name.
// - Sanitize to avoid empty / weird values.
// - Keep this module dependency-light and deterministic.

#![allow(dead_code)]

use std::borrow::Cow;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Default environment variable used to override progname.
pub const PROGNAME_ENV: &str = "MUFFIN_PROGNAME";

/// Additional env overrides (in priority order) that can be checked if desired.
pub const PROGNAME_ENV_FALLBACKS: &[&str] = &["MUFFIN_APPNAME", "MUFFIN_NAME"];

/// A resolved, display-ready program name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProgName {
    /// Stable printable name.
    pub display: String,
    /// Where the name came from.
    pub source: ProgNameSource,
}

/// Source of the progname.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgNameSource {
    /// Explicit override via environment variable.
    Env,
    /// Derived from argv[0] / provided invocation path.
    Argv0,
    /// Derived from std::env::current_exe().
    CurrentExe,
    /// Final fallback.
    Default,
}

/// Options controlling progname resolution.
#[derive(Debug, Clone)]
pub struct ProgNameOptions {
    /// Primary environment variable to check first (default: MUFFIN_PROGNAME).
    pub env_key: &'static str,
    /// Optional fallback env vars to check next.
    pub env_fallbacks: &'static [&'static str],
    /// Default name if everything fails.
    pub default_name: &'static str,
    /// If true, lowercases the final name (useful for uniform CLI output).
    pub force_lowercase: bool,
    /// If true, trims extension (".exe") from executable file name.
    pub strip_exe_extension: bool,
    /// If true, collapses whitespace in the derived name.
    pub normalize_whitespace: bool,
    /// Maximum length of display name (hard cap to avoid log spam).
    pub max_len: usize,
}

impl Default for ProgNameOptions {
    fn default() -> Self {
        Self {
            env_key: PROGNAME_ENV,
            env_fallbacks: PROGNAME_ENV_FALLBACKS,
            default_name: "steel",
            force_lowercase: false,
            strip_exe_extension: true,
            normalize_whitespace: true,
            max_len: 64,
        }
    }
}

/// Resolve program name using the process environment and argv[0] (if available).
///
/// Priority:
/// 1) env override (MUFFIN_PROGNAME, then fallbacks)
/// 2) argv0 basename
/// 3) current_exe basename
/// 4) default
pub fn resolve_progname(argv0: Option<&OsStr>) -> ProgName {
    resolve_progname_with(env::vars_os().collect::<Vec<_>>().as_slice(), argv0, &ProgNameOptions::default())
}

/// Same as `resolve_progname` but caller can provide env snapshot and options.
pub fn resolve_progname_with_env(env_kv: &[(OsString, OsString)], argv0: Option<&OsStr>) -> ProgName {
    resolve_progname_with(env_kv, argv0, &ProgNameOptions::default())
}

/// Full control entrypoint: env snapshot + argv0 + options.
pub fn resolve_progname_with(
    env_kv: &[(OsString, OsString)],
    argv0: Option<&OsStr>,
    opts: &ProgNameOptions,
) -> ProgName {
    // 1) Environment override
    if let Some(v) = get_env_value(env_kv, opts.env_key)
        .or_else(|| get_env_value_any(env_kv, opts.env_fallbacks))
    {
        if let Some(s) = os_to_clean_string(&v, opts) {
            return ProgName {
                display: s,
                source: ProgNameSource::Env,
            };
        }
    }

    // 2) argv0 basename
    if let Some(a0) = argv0 {
        if let Some(s) = argv0_to_progname(a0, opts) {
            return ProgName {
                display: s,
                source: ProgNameSource::Argv0,
            };
        }
    }

    // 3) current_exe basename
    if let Ok(exe) = env::current_exe() {
        if let Some(s) = path_to_progname(&exe, opts) {
            return ProgName {
                display: s,
                source: ProgNameSource::CurrentExe,
            };
        }
    }

    // 4) default fallback
    ProgName {
        display: finalize_string(opts.default_name.into(), opts),
        source: ProgNameSource::Default,
    }
}

/// Resolve progname using a path (useful when embedding Steel as a lib).
pub fn resolve_progname_from_path(path: &Path, opts: &ProgNameOptions) -> ProgName {
    if let Some(s) = path_to_progname(path, opts) {
        ProgName {
            display: s,
            source: ProgNameSource::CurrentExe,
        }
    } else {
        ProgName {
            display: finalize_string(opts.default_name.into(), opts),
            source: ProgNameSource::Default,
        }
    }
}

/// Convert argv0 to progname (basenames, sanitize).
pub fn argv0_to_progname(argv0: &OsStr, opts: &ProgNameOptions) -> Option<String> {
    // argv0 might be:
    // - "steel"
    // - "./steel"
    // - "/usr/bin/steel"
    // - "C:\...\steel.exe"
    // - quoted or odd; we treat as path-like and take basename.
    let p = Path::new(argv0);
    let raw = p.as_os_str().to_string_lossy();
    if raw.contains('\\') {
        let file = raw.rsplit('\\').next().unwrap_or(raw.as_ref());
        let mut s = file.to_string();
        if opts.strip_exe_extension {
            s = strip_exe_ext(&s);
        }
        let s = finalize_string(s, opts);
        return if s.is_empty() { None } else { Some(s) };
    }
    if p.components().count() > 1 || raw.contains(std::path::MAIN_SEPARATOR) {
        path_to_progname(p, opts)
    } else {
        os_to_clean_string(argv0, opts)
    }
}

/// Convert a path to progname (basename, optional extension stripping).
pub fn path_to_progname(path: &Path, opts: &ProgNameOptions) -> Option<String> {
    let file = path.file_name().or_else(|| path.components().last().map(|c| c.as_os_str()))?;
    let mut s = file.to_string_lossy().into_owned();
    if s.is_empty() {
        return None;
    }
    if opts.strip_exe_extension {
        s = strip_exe_ext(&s);
    }
    let s = finalize_string(s, opts);
    if s.is_empty() { None } else { Some(s) }
}

/// Remove a trailing `.exe` or `.EXE` (Windows) without touching other extensions.
fn strip_exe_ext(s: &str) -> String {
    if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
        s[..s.len() - 4].to_string()
    } else {
        s.to_string()
    }
}

/// Fetch env value by key (case-sensitive).
fn get_env_value(env_kv: &[(OsString, OsString)], key: &str) -> Option<OsString> {
    let k = OsStr::new(key);
    for (ek, ev) in env_kv {
        if ek == k {
            return Some(ev.clone());
        }
    }
    None
}

fn get_env_value_any(env_kv: &[(OsString, OsString)], keys: &[&str]) -> Option<OsString> {
    for &k in keys {
        if let Some(v) = get_env_value(env_kv, k) {
            return Some(v);
        }
    }
    None
}

/// Converts an OsStr into a sanitized printable string according to options.
fn os_to_clean_string(os: &OsStr, opts: &ProgNameOptions) -> Option<String> {
    let raw: Cow<'_, str> = os.to_string_lossy();
    if raw.is_empty() {
        return None;
    }
    let s = finalize_string(raw.into_owned(), opts);
    if s.is_empty() { None } else { Some(s) }
}

/// Apply normalization rules to a string.
fn finalize_string(mut s: String, opts: &ProgNameOptions) -> String {
    // Trim outer whitespace
    s = s.trim().to_string();

    // Remove surrounding quotes (common when forwarded through wrappers)
    s = strip_surrounding_quotes(&s).to_string();

    // Collapse whitespace
    if opts.normalize_whitespace {
        s = collapse_whitespace(&s);
    }

    // Defensive: remove control chars (keep printable + spaces)
    s = s.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t') // keep minimal? then collapse later
        .collect::<String>();
    if opts.normalize_whitespace {
        s = collapse_whitespace(&s);
    }

    // Lowercase if asked
    if opts.force_lowercase {
        s = s.to_lowercase();
    }

    // Enforce max length (unicode-safe)
    if opts.max_len > 0 && s.chars().count() > opts.max_len {
        s = s.chars().take(opts.max_len).collect();
    }

    // Final trim
    s.trim().to_string()
}

/// Strip one pair of surrounding single or double quotes if present.
fn strip_surrounding_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..bytes.len() - 1];
        }
    }
    s
}

/// Collapse all whitespace runs to a single space.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

/// Convenience type used by other modules that store a "progname" field.
#[derive(Debug, Clone)]
pub struct ProgNameCache {
    resolved: ProgName,
}

impl ProgNameCache {
    /// Create a cache from argv0.
    pub fn new(argv0: Option<&OsStr>) -> Self {
        Self {
            resolved: resolve_progname(argv0),
        }
    }

    /// Create a cache with explicit options/env snapshot (deterministic in tests).
    pub fn new_with(env_kv: &[(OsString, OsString)], argv0: Option<&OsStr>, opts: &ProgNameOptions) -> Self {
        Self {
            resolved: resolve_progname_with(env_kv, argv0, opts),
        }
    }

    /// Get the cached display name.
    pub fn display(&self) -> &str {
        &self.resolved.display
    }

    /// Get full cached object.
    pub fn get(&self) -> &ProgName {
        &self.resolved
    }
}

/// Minimal helper to format "progname: message" consistently.
pub fn format_prefixed(progname: &str, msg: &str) -> String {
    if progname.is_empty() {
        msg.to_string()
    } else if msg.is_empty() {
        progname.to_string()
    } else {
        format!("{progname}: {msg}")
    }
}

/// Helper to format "progname[component]: message" consistently.
pub fn format_component_prefixed(progname: &str, component: &str, msg: &str) -> String {
    let p = progname.trim();
    let c = component.trim();
    if p.is_empty() {
        if c.is_empty() {
            msg.to_string()
        } else {
            format!("[{c}] {msg}")
        }
    } else if c.is_empty() {
        format_prefixed(p, msg)
    } else {
        format!("{p}[{c}]: {msg}")
    }
}

/// Derive a friendly command name from a subcommand token (for nested CLIs).
pub fn derive_child_progname(parent: &str, child: &str, opts: &ProgNameOptions) -> String {
    let mut s = String::new();
    if !parent.trim().is_empty() {
        s.push_str(parent.trim());
    } else {
        s.push_str(opts.default_name);
    }
    if !child.trim().is_empty() {
        s.push(' ');
        s.push_str(child.trim());
    }
    finalize_string(s, opts)
}

/// Given a list of argv tokens, returns argv0 if present.
pub fn argv0_from_args(args: &[OsString]) -> Option<&OsStr> {
    args.get(0).map(|s| s.as_os_str())
}

/// Best-effort: returns the current executable path (if available).
pub fn current_exe_path() -> Option<PathBuf> {
    env::current_exe().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(k: &str, v: &str) -> (OsString, OsString) {
        (OsString::from(k), OsString::from(v))
    }

    #[test]
    fn env_override_wins() {
        let env_kv = vec![kv("MUFFIN_PROGNAME", "My Steel")];
        let pn = resolve_progname_with(&env_kv, Some(OsStr::new("ignored")), &ProgNameOptions::default());
        assert_eq!(pn.source, ProgNameSource::Env);
        assert_eq!(pn.display, "My Steel");
    }

    #[test]
    fn env_override_sanitizes_quotes_and_ws() {
        let env_kv = vec![kv("MUFFIN_PROGNAME", "  \"steel dev\"  ")];
        let pn = resolve_progname_with(&env_kv, None, &ProgNameOptions::default());
        assert_eq!(pn.display, "steel dev");
    }

    #[test]
    fn argv0_basename_unix() {
        let env_kv: Vec<(OsString, OsString)> = vec![];
        let pn = resolve_progname_with(&env_kv, Some(OsStr::new("/usr/bin/steel")), &ProgNameOptions::default());
        assert_eq!(pn.source, ProgNameSource::Argv0);
        assert_eq!(pn.display, "steel");
    }

    #[test]
    fn argv0_windows_exe_stripped() {
        let env_kv: Vec<(OsString, OsString)> = vec![];
        let pn = resolve_progname_with(
            &env_kv,
            Some(OsStr::new(r"C:\Tools\Steel.EXE")),
            &ProgNameOptions::default(),
        );
        assert_eq!(pn.display, "Steel");
    }

    #[test]
    fn strip_exe_ext_only() {
        assert_eq!(strip_exe_ext("steel.exe"), "steel");
        assert_eq!(strip_exe_ext("steel.EXE"), "steel");
        assert_eq!(strip_exe_ext("steel.exex"), "steel.exex");
        assert_eq!(strip_exe_ext("steel"), "steel");
    }

    #[test]
    fn finalize_enforces_max_len() {
        let mut opts = ProgNameOptions::default();
        opts.max_len = 5;
        let s = finalize_string("0123456789".to_string(), &opts);
        assert_eq!(s, "01234");
    }

    #[test]
    fn collapse_whitespace_basic() {
        assert_eq!(collapse_whitespace("a   b\tc\n\rd"), "a b c d");
    }

    #[test]
    fn format_prefix_helpers() {
        assert_eq!(format_prefixed("steel", "oops"), "steel: oops");
        assert_eq!(format_component_prefixed("steel", "vms", "oops"), "steel[vms]: oops");
        assert_eq!(format_component_prefixed("", "vms", "oops"), "[vms] oops");
    }

    #[test]
    fn derive_child_progname_basic() {
        let opts = ProgNameOptions::default();
        assert_eq!(derive_child_progname("steel", "build", &opts), "steel build");
        assert_eq!(derive_child_progname("", "build", &opts), "steel build");
    }
}
