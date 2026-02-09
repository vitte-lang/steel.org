//! Linux platform helpers (linux.rs) — MAX (std-only).
//!
//! This module provides Linux-specific platform utilities used by Steel.
//! Scope:
//! - OS identification (compile-time + runtime best-effort)
//! - default paths (XDG + common Linux conventions)
//! - filesystem helpers (executable bit, symlink, /proc probes)
//! - process helpers (shell, env defaults)
//! - distro detection best-effort (os-release parsing, std-only)
//!
//! Notes:
//! - std-only: no `libc`, no `nix`. Use `/etc/os-release` parsing where possible.
//! - Any probing is best-effort and explicitly callable.
//! - Prefer deterministic outputs where possible.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LinuxInfo {
    pub target_os: &'static str,
    pub target_arch: &'static str,

    // runtime best-effort:
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub shell: Option<PathBuf>,

    // distro:
    pub distro: Option<DistroInfo>,
}

impl LinuxInfo {
    pub fn gather() -> Self {
        Self {
            target_os: option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown"),
            target_arch: option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown"),
            hostname: hostname_best_effort(),
            username: username_best_effort(),
            home_dir: home_dir_best_effort(),
            shell: shell_best_effort(),
            distro: read_os_release().ok(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistroInfo {
    pub id: Option<String>,
    pub id_like: Vec<String>,
    pub name: Option<String>,
    pub pretty_name: Option<String>,
    pub version: Option<String>,
    pub version_id: Option<String>,
}

impl DistroInfo {
    pub fn summary(&self) -> String {
        if let Some(p) = &self.pretty_name {
            return p.clone();
        }
        if let Some(n) = &self.name {
            return n.clone();
        }
        self.id.clone().unwrap_or_else(|| "linux".into())
    }
}

#[derive(Debug)]
pub enum LinuxError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for LinuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinuxError::Io(e) => write!(f, "io: {e}"),
            LinuxError::Parse(s) => write!(f, "parse: {s}"),
        }
    }
}

impl std::error::Error for LinuxError {}

impl From<std::io::Error> for LinuxError {
    fn from(e: std::io::Error) -> Self {
        LinuxError::Io(e)
    }
}

/* ---------------------------- Compile-time checks ---------------------------- */

pub fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

/* ---------------------------- Default paths (XDG) ---------------------------- */

/// Return typical config directory:
/// - `$XDG_CONFIG_HOME`
/// - `$HOME/.config`
/// - fallback: `/etc`
pub fn config_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".config");
    }
    PathBuf::from("/etc")
}

/// Return typical cache directory:
/// - `$XDG_CACHE_HOME`
/// - `$HOME/.cache`
/// - fallback: `/tmp`
pub fn cache_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".cache");
    }
    PathBuf::from("/tmp")
}

/// Return typical data directory:
/// - `$XDG_DATA_HOME`
/// - `$HOME/.local/share`
/// - fallback: `/usr/local/share`
pub fn data_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".local").join("share");
    }
    PathBuf::from("/usr/local/share")
}

/// Return typical state directory (XDG_STATE_HOME, systemd-ish):
/// - `$XDG_STATE_HOME`
/// - `$HOME/.local/state`
/// - fallback: `$HOME/.cache` or `/tmp`
pub fn state_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".local").join("state");
    }
    cache_dir()
}

pub fn temp_dir() -> PathBuf {
    env::temp_dir()
}

pub fn steel_config_dir() -> PathBuf {
    config_dir().join("steel")
}

pub fn steel_cache_dir() -> PathBuf {
    cache_dir().join("steel")
}

pub fn steel_data_dir() -> PathBuf {
    data_dir().join("steel")
}

pub fn steel_state_dir() -> PathBuf {
    state_dir().join("steel")
}

/* ---------------------------- Process helpers ---------------------------- */

/// Best-effort shell path:
/// - `$SHELL`
/// - fallback: `/bin/sh`
pub fn default_shell() -> PathBuf {
    if let Some(s) = env::var_os("SHELL") {
        let p = PathBuf::from(s);
        if p.is_absolute() {
            return p;
        }
    }
    PathBuf::from("/bin/sh")
}

/// Returns a default env map for running tools.
/// - preserve PATH
/// - set stable locale if none
pub fn default_env() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();

    if let Ok(p) = env::var("PATH") {
        m.insert("PATH".into(), p);
    }

    if env::var("LC_ALL").is_err() && env::var("LANG").is_err() {
        m.insert("LANG".into(), "C".into());
    }

    m.insert("MUFFIN_PLATFORM".into(), "linux".into());
    m
}

/* ---------------------------- FS helpers ---------------------------- */

/// True if file has any executable bit (unix mode & 0o111).
#[cfg(unix)]
pub fn is_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let md = fs::metadata(path)?;
    Ok((md.permissions().mode() & 0o111) != 0)
}

#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> std::io::Result<bool> {
    Ok(false)
}

/// Create a symlink (unix).
#[cfg(unix)]
pub fn symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(not(unix))]
pub fn symlink(_src: &Path, _dst: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlink unsupported on non-unix",
    ))
}

/// Returns true if `/proc` looks mounted.
pub fn has_procfs() -> bool {
    Path::new("/proc").is_dir() && Path::new("/proc/self").is_dir()
}

/// Return current process exe path (Linux: `/proc/self/exe`) best-effort.
/// Falls back to `std::env::current_exe()`.
pub fn current_exe_best_effort() -> Option<PathBuf> {
    if has_procfs() {
        if let Ok(p) = fs::read_link("/proc/self/exe") {
            return Some(p);
        }
    }
    std::env::current_exe().ok()
}

/* ---------------------------- Distro detection ---------------------------- */

/// Read `/etc/os-release` (or `/usr/lib/os-release`) best-effort.
///
/// Reference: os-release is standard on most modern distros.
/// We keep parsing minimal and robust.
pub fn read_os_release() -> Result<DistroInfo, LinuxError> {
    let p1 = Path::new("/etc/os-release");
    let p2 = Path::new("/usr/lib/os-release");

    let content = if p1.exists() {
        fs::read_to_string(p1)?
    } else if p2.exists() {
        fs::read_to_string(p2)?
    } else {
        return Err(LinuxError::Parse("os-release not found".into()));
    };

    parse_os_release(&content)
}

pub fn parse_os_release(s: &str) -> Result<DistroInfo, LinuxError> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();

    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_string();
        let mut val = v.trim().to_string();
        val = unquote_os_release(&val);
        map.insert(key, val);
    }

    let id_like = map
        .get("ID_LIKE")
        .map(|v| v.split_whitespace().map(|x| x.to_string()).collect())
        .unwrap_or_else(Vec::new);

    Ok(DistroInfo {
        id: map.get("ID").cloned(),
        id_like,
        name: map.get("NAME").cloned(),
        pretty_name: map.get("PRETTY_NAME").cloned(),
        version: map.get("VERSION").cloned(),
        version_id: map.get("VERSION_ID").cloned(),
    })
}

fn unquote_os_release(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"') || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')) {
        let inner = &s[1..s.len() - 1];
        // unescape minimal sequences: \" \\ \n \t
        let mut out = String::new();
        let mut it = inner.chars().peekable();
        while let Some(ch) = it.next() {
            if ch == '\\' {
                match it.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(x) => out.push(x),
                    None => {}
                }
            } else {
                out.push(ch);
            }
        }
        out
    } else {
        s.to_string()
    }
}

/* ---------------------------- Runtime best-effort ---------------------------- */

fn home_dir_best_effort() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn username_best_effort() -> Option<String> {
    env::var("USER")
        .ok()
        .or_else(|| env::var("LOGNAME").ok())
}

fn shell_best_effort() -> Option<PathBuf> {
    env::var_os("SHELL").map(PathBuf::from)
}

fn hostname_best_effort() -> Option<String> {
    env::var("HOSTNAME").ok()
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_dirs_non_empty() {
        assert!(!config_dir().as_os_str().is_empty());
        assert!(!cache_dir().as_os_str().is_empty());
        assert!(!data_dir().as_os_str().is_empty());
        assert!(!state_dir().as_os_str().is_empty());
    }

    #[test]
    fn parse_os_release_basic() {
        let s = r#"
            NAME="Debian GNU/Linux"
            ID=debian
            ID_LIKE="debian linux"
            PRETTY_NAME="Debian GNU/Linux 13 (trixie)"
            VERSION_ID="13"
        "#;
        let d = parse_os_release(s).unwrap();
        assert_eq!(d.id.as_deref(), Some("debian"));
        assert!(d.id_like.contains(&"linux".to_string()));
        assert_eq!(d.version_id.as_deref(), Some("13"));
    }

    #[test]
    fn proc_probe_ok() {
        // This test is environment-dependent; just ensure it doesn't panic.
        let _ = has_procfs();
        let _ = current_exe_best_effort();
    }
}
