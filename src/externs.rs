// /Users/vincent/Documents/Github/steel/src/externs.rs
//! externs — external tool and environment detection (std-only)
//!
//! Provides best-effort discovery for common build tools and environment facts.
//! Intended for Steel (configuration) and the execution layer to share a stable
//! interface for:
//! - locating tools in PATH
//! - probing versions (best-effort, deterministic formatting)
//! - detecting host platform properties
//!
//! No external crates. All detection is conservative.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Externs {
    pub host: HostInfo,
    pub tools: BTreeMap<String, ToolInfo>,
}

impl Externs {
    pub fn new() -> Self {
        Self {
            host: HostInfo::detect(),
            tools: BTreeMap::new(),
        }
    }

    /// Probe a named tool (e.g. "cc") using a list of candidates and env overrides.
    ///
    /// - `env_var` if set (non-empty) is used as highest priority candidate.
    /// - Then `candidates` are tried in order.
    pub fn probe_tool(&mut self, logical_name: &str, env_var: Option<&str>, candidates: &[&str]) {
        let mut list: Vec<String> = Vec::new();

        if let Some(ev) = env_var {
            if let Ok(v) = env::var(ev) {
                let t = v.trim();
                if !t.is_empty() {
                    list.push(t.to_string());
                }
            }
        }

        for c in candidates {
            list.push((*c).to_string());
        }

        let (path, version) = probe_candidates(&list);
        let info = ToolInfo {
            name: logical_name.to_string(),
            path,
            version,
        };
        self.tools.insert(logical_name.to_string(), info);
    }

    /// Convenience: probe a standard C/C++ toolchain.
    pub fn probe_default_toolchain(&mut self) {
        self.probe_tool("cc", Some("CC"), &["cc", "clang", "gcc"]);
        self.probe_tool("cxx", Some("CXX"), &["c++", "clang++", "g++"]);
        self.probe_tool("ar", Some("AR"), &["ar", "llvm-ar"]);
        self.probe_tool("ld", Some("LD"), &["ld", "lld", "ld.lld"]);
        self.probe_tool("rustc", Some("RUSTC"), &["rustc"]);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub arch: String,
    pub os: String,
    pub family: String,
    pub triple_best_effort: String,

    pub is_ci: bool,
    pub env: BTreeMap<String, String>,
}

impl HostInfo {
    pub fn detect() -> Self {
        let arch = env::consts::ARCH.to_string();
        let os = env::consts::OS.to_string();
        let family = env::consts::FAMILY.to_string();
        let triple_best_effort = host_triple_best_effort(&arch, &os);

        let is_ci = env::var("CI").is_ok();

        let mut envmap = BTreeMap::new();
        for k in ["CC", "CXX", "AR", "LD", "RUSTC", "MUFFIN_PROFILE", "MUFFIN_TARGET"] {
            if let Ok(v) = env::var(k) {
                envmap.insert(k.to_string(), v);
            }
        }

        Self {
            arch,
            os,
            family,
            triple_best_effort,
            is_ci,
            env: envmap,
        }
    }
}

pub fn host_triple_best_effort(arch: &str, os: &str) -> String {
    let t = match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => "unknown-unknown-unknown",
    };
    t.to_string()
}

/// Probe tool candidates: locate in PATH (best-effort) and read first `--version` line.
pub fn probe_candidates(candidates: &[String]) -> (Option<PathBuf>, Option<String>) {
    for c in candidates {
        if c.trim().is_empty() {
            continue;
        }
        if let Some(path) = which(c) {
            let version = tool_version_line(c).or_else(|| tool_version_line_path(&path));
            return (Some(path), version);
        }

        // If candidate looks like a path, try it directly.
        let p = PathBuf::from(c);
        if p.is_file() {
            let version = tool_version_line_path(&p);
            return (Some(p), version);
        }
    }
    (None, None)
}

/// Locate executable in PATH (std-only).
pub fn which(cmd: &str) -> Option<PathBuf> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }

    // If already has a separator, treat as a path.
    if cmd.contains('/') || cmd.contains('\\') {
        let p = PathBuf::from(cmd);
        if is_executable(&p) {
            return Some(p);
        }
        return None;
    }

    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let cand = dir.join(cmd);
        if is_executable(&cand) {
            return Some(cand);
        }

        // Windows PATHEXT support
        #[cfg(windows)]
        {
            if let Some(pe) = env::var_os("PATHEXT") {
                for ext in env::split_paths(&pe) {
                    // split_paths on PATHEXT is imperfect; handle common formats too
                    let e = ext.to_string_lossy().to_string();
                    if e.is_empty() {
                        continue;
                    }
                    let e = if e.starts_with('.') { e } else { format!(".{e}") };
                    let cand2 = dir.join(format!("{cmd}{e}"));
                    if is_executable(&cand2) {
                        return Some(cand2);
                    }
                }
            }
        }
    }

    None
}

pub fn is_executable(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(p) {
            return (md.permissions().mode() & 0o111) != 0;
        }
        return false;
    }
    #[cfg(not(unix))]
    {
        // best-effort on non-unix
        return true;
    }
}

/// Best-effort: `tool --version`, first stdout line.
pub fn tool_version_line(tool: &str) -> Option<String> {
    let out = Command::new(tool).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?.trim();
    if line.is_empty() { None } else { Some(line.to_string()) }
}

/// Best-effort: `<path> --version`, first stdout line.
pub fn tool_version_line_path(path: &Path) -> Option<String> {
    let out = Command::new(path).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?.trim();
    if line.is_empty() { None } else { Some(line.to_string()) }
}

#[derive(Debug, Clone)]
pub struct ExternReport {
    pub missing: Vec<String>,
    pub present: Vec<String>,
}

impl fmt::Display for ExternReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "externs report")?;
        writeln!(f, "  present:")?;
        for p in &self.present {
            writeln!(f, "    - {p}")?;
        }
        writeln!(f, "  missing:")?;
        for m in &self.missing {
            writeln!(f, "    - {m}")?;
        }
        Ok(())
    }
}

/// Check that required logical tools exist (by key in externs.tools).
pub fn check_required(ext: &Externs, required: &[&str]) -> ExternReport {
    let mut missing = Vec::new();
    let mut present = Vec::new();

    for &r in required {
        match ext.tools.get(r) {
            Some(t) if t.path.is_some() => present.push(r.to_string()),
            _ => missing.push(r.to_string()),
        }
    }

    ExternReport { missing, present }
}

/// Simple env helpers.
pub fn env_flag(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

pub fn env_get(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    })
}

/// Normalize a path without canonicalize (lexical).
pub fn normalize_path(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    let mut out = PathBuf::new();
    for c in p.components() {
        use std::path::Component;
        match c {
            Component::CurDir => {}
            Component::ParentDir => { out.pop(); }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// Utility: join under root if relative.
pub fn join_under(root: impl AsRef<Path>, p: impl AsRef<Path>) -> PathBuf {
    let root = root.as_ref();
    let p = p.as_ref();
    if p.is_absolute() { p.to_path_buf() } else { root.join(p) }
}

/// Utility: read first non-empty line from a string (used for parsing tool output).
pub fn first_nonempty_line(s: &str) -> Option<&str> {
    s.lines().map(|l| l.trim()).find(|l| !l.is_empty())
}

/// Utility: split PATH-like env var into entries.
pub fn split_path_var(v: &OsStr) -> Vec<PathBuf> {
    env::split_paths(v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_detect() {
        let h = HostInfo::detect();
        assert!(!h.arch.is_empty());
        assert!(!h.os.is_empty());
        assert!(!h.triple_best_effort.is_empty());
    }

    #[test]
    fn which_empty() {
        assert!(which("").is_none());
    }

    #[test]
    fn first_nonempty() {
        assert_eq!(first_nonempty_line("\n\n a \n b"), Some("a"));
    }
}
