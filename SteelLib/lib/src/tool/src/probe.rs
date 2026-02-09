//! Tool probing (probe.rs) — MAX (std-only).
//!
//! This module provides "tool discovery" and probing:
//! - search executables on PATH
//! - resolve absolute tool paths
//! - probe tool versions (best-effort) by running `--version` / `-v` / `version`
//! - normalize results into `ToolProbe`
//!
//! Integration:
//! - `tool/mod.rs` provides `ToolSpec` + `ToolRunner`
//! - this module builds `ToolSpec` for probing, and parses stdout/stderr
//!
//! Notes:
//! - std-only: no which crate, no semver crate
//! - Windows PATHEXT supported
//! - Keep probing deterministic: choose first match in PATH order.

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{ToolError, ToolRunner, ToolSpec, ToolStatus};

#[derive(Debug)]
pub enum ProbeError {
    Invalid(&'static str),
    Io(std::io::Error),
    Tool(ToolError),
    NotFound(String),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Invalid(s) => write!(f, "invalid: {s}"),
            ProbeError::Io(e) => write!(f, "io: {e}"),
            ProbeError::Tool(e) => write!(f, "tool: {e}"),
            ProbeError::NotFound(s) => write!(f, "not found: {s}"),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<std::io::Error> for ProbeError {
    fn from(e: std::io::Error) -> Self {
        ProbeError::Io(e)
    }
}

impl From<ToolError> for ProbeError {
    fn from(e: ToolError) -> Self {
        ProbeError::Tool(e)
    }
}

/// A discovered tool candidate.
#[derive(Debug, Clone)]
pub struct ToolCandidate {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ToolProbe {
    pub name: String,
    pub path: PathBuf,
    pub version_raw: Option<String>,
    pub ok: bool,
    pub notes: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl ToolProbe {
    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.name);
        s.push_str(": ");
        s.push_str(&self.path.to_string_lossy());
        if let Some(v) = &self.version_raw {
            s.push_str(" (");
            s.push_str(v);
            s.push(')');
        }
        if !self.ok {
            s.push_str(" [not ok]");
        }
        s
    }
}

/* ------------------------------ PATH search ------------------------------ */

/// Find an executable on PATH. Returns absolute path if found.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    // If name contains path separators, treat as direct path.
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return if is_executable_file(&p) { Some(p) } else { None };
    }

    let path_var = env::var_os("PATH")?;
    let paths = env::split_paths(&path_var);

    #[cfg(windows)]
    let exts = pathext_list();

    for dir in paths {
        if dir.as_os_str().is_empty() {
            continue;
        }
        #[cfg(not(windows))]
        {
            let cand = dir.join(name);
            if is_executable_file(&cand) {
                return Some(cand);
            }
        }

        #[cfg(windows)]
        {
            // If name already has extension, check directly first.
            let direct = dir.join(name);
            if is_executable_file(&direct) {
                return Some(direct);
            }
            for ext in &exts {
                let mut n = OsString::from(name);
                n.push(ext);
                let cand = dir.join(&n);
                if is_executable_file(&cand) {
                    return Some(cand);
                }
            }
        }
    }

    None
}

/// Find all matches on PATH (ordered).
pub fn which_all(name: &str, limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if name.is_empty() {
        return out;
    }

    let Some(path_var) = env::var_os("PATH") else { return out };
    let paths = env::split_paths(&path_var);

    #[cfg(windows)]
    let exts = pathext_list();

    for dir in paths {
        if dir.as_os_str().is_empty() {
            continue;
        }
        #[cfg(not(windows))]
        {
            let cand = dir.join(name);
            if is_executable_file(&cand) {
                out.push(cand);
                if out.len() >= limit {
                    break;
                }
            }
        }

        #[cfg(windows)]
        {
            let direct = dir.join(name);
            if is_executable_file(&direct) {
                out.push(direct);
                if out.len() >= limit {
                    break;
                }
            } else {
                for ext in &exts {
                    let mut n = OsString::from(name);
                    n.push(ext);
                    let cand = dir.join(&n);
                    if is_executable_file(&cand) {
                        out.push(cand);
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
    }

    out
}

/* ------------------------------ Probing ------------------------------ */

/// Probe a tool by name (search on PATH) and run version query.
pub fn probe_tool(name: &str) -> Result<ToolProbe, ProbeError> {
    let path = which(name).ok_or_else(|| ProbeError::NotFound(name.to_string()))?;
    probe_tool_path(name, path)
}

/// Probe a tool at an explicit path and run version query.
pub fn probe_tool_path(name: &str, path: PathBuf) -> Result<ToolProbe, ProbeError> {
    if !is_executable_file(&path) {
        return Err(ProbeError::Invalid("path is not executable file"));
    }

    let runner = ToolRunner::new();
    let mut notes = Vec::new();

    // Common version flags to try in order.
    let attempts: &[&[&str]] = &[
        &["--version"],
        &["-V"],
        &["-v"],
        &["version"],
    ];

    let mut version_raw = None;
    let mut ok = false;

    for args in attempts {
        let spec = ToolSpec::new(&path)
            .args(args.iter().copied())
            .timeout(Duration::from_secs(2));

        match runner.run(&spec) {
            Ok(out) => {
                match out.status {
                    ToolStatus::Success { .. } | ToolStatus::Failed { .. } => {
                        // Many tools print version to stderr and exit non-zero; accept output if non-empty.
                        let s = best_effort_version_string(&out.stdout, &out.stderr);
                        if !s.trim().is_empty() {
                            version_raw = Some(first_line(&s));
                            ok = true;
                            break;
                        } else {
                            notes.push(format!("no version output for args {:?}", args));
                        }
                    }
                    ToolStatus::Timeout { .. } => {
                        notes.push(format!("timeout probing args {:?}", args));
                    }
                }
            }
            Err(e) => {
                notes.push(format!("probe error for args {:?}: {}", args, e));
            }
        }
    }

    if !ok {
        notes.push("could not extract version (tool may still be usable)".into());
    }

    Ok(ToolProbe {
        name: name.to_string(),
        path,
        version_raw,
        ok,
        notes,
        env: snapshot_env_minimal(),
    })
}

/// Probe multiple tools at once (best-effort, continues on errors).
pub fn probe_many(names: &[&str]) -> Vec<Result<ToolProbe, ProbeError>> {
    names.iter().map(|n| probe_tool(n)).collect()
}

/* ------------------------------ Helpers ------------------------------ */

fn is_executable_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(p) {
            return (md.permissions().mode() & 0o111) != 0;
        }
        false
    }
    #[cfg(windows)]
    {
        // On Windows, file extension determines executability; accept any file.
        // `which` already filtered by PATHEXT, so here just ensure file exists.
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

#[cfg(windows)]
fn pathext_list() -> Vec<OsString> {
    // PATHEXT like ".COM;.EXE;.BAT;.CMD;..."
    let v = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    v.to_string_lossy()
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            // ensure starts with dot
            if s.starts_with('.') {
                OsString::from(s)
            } else {
                let mut o = OsString::from(".");
                o.push(s);
                o
            }
        })
        .collect()
}

fn best_effort_version_string(stdout: &[u8], stderr: &[u8]) -> String {
    // Try stdout first, then stderr.
    let a = String::from_utf8_lossy(stdout).to_string();
    if !a.trim().is_empty() {
        return a;
    }
    String::from_utf8_lossy(stderr).to_string()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// Capture minimal env for traceability (no secrets filtering here; keep small).
fn snapshot_env_minimal() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for k in ["PATH", "HOME", "USERPROFILE", "SHELL", "ComSpec"] {
        if let Ok(v) = env::var(k) {
            m.insert(k.to_string(), v);
        }
    }
    m
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_empty_none() {
        assert!(which("").is_none());
    }

    #[test]
    fn which_all_limit() {
        let v = which_all("definitely-not-a-real-tool-name", 3);
        assert!(v.is_empty());
    }

    #[test]
    fn parse_pathext_non_empty() {
        #[cfg(windows)]
        {
            let v = pathext_list();
            assert!(!v.is_empty());
        }
    }
}
