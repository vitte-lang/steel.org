//! Solaris / illumos platform helpers (solaris.rs) — MAX (std-only).
//!
//! Targets:
//! - Oracle Solaris
//! - illumos (OpenIndiana, SmartOS, OmniOS, etc.)
//!
//! Scope:
//! - compile-time identification
//! - default paths (XDG-like best-effort + traditional /etc /var)
//! - filesystem helpers (exec bit, symlink)
//! - process helpers (shell, env defaults)
//! - runtime probes best-effort (env + file presence; no libc/sysconf)
//!
//! Notes:
//! - std-only: no `libc`, no `nix`, no `sysctl`.
//! - For deep integration (zones, SMF, pkg), add feature-gated modules.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolarisKind {
    Solaris,
    Illumos,
    Unknown,
}

impl SolarisKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SolarisKind::Solaris => "solaris",
            SolarisKind::Illumos => "illumos",
            SolarisKind::Unknown => "solaris-unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolarisInfo {
    pub kind: SolarisKind,
    pub target_os: &'static str,
    pub target_arch: &'static str,

    // runtime best-effort:
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub shell: Option<PathBuf>,

    // runtime "fingerprints" (best-effort):
    pub is_zone: Option<bool>,
    pub has_smf: bool,
}

impl SolarisInfo {
    pub fn gather() -> Self {
        Self {
            kind: detect_compile_time_kind(),
            target_os: option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown"),
            target_arch: option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown"),
            hostname: hostname_best_effort(),
            username: username_best_effort(),
            home_dir: home_dir_best_effort(),
            shell: shell_best_effort(),
            is_zone: zone_best_effort(),
            has_smf: Path::new("/lib/svc/bin/svc.startd").exists() || Path::new("/usr/sbin/svcadm").exists(),
        }
    }
}

/* ---------------------------- Compile-time detection ---------------------------- */

pub fn detect_compile_time_kind() -> SolarisKind {
    // Rust cfg for illumos exists; Solaris target is typically "solaris".
    #[cfg(target_os = "illumos")]
    {
        return SolarisKind::Illumos;
    }
    #[cfg(target_os = "solaris")]
    {
        return SolarisKind::Solaris;
    }
    SolarisKind::Unknown
}

pub fn is_solaris_family() -> bool {
    matches!(detect_compile_time_kind(), SolarisKind::Solaris | SolarisKind::Illumos)
}

/* ---------------------------- Default paths ---------------------------- */

/// Config directory heuristic:
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

/// Cache directory heuristic:
/// - `$XDG_CACHE_HOME`
/// - `$HOME/.cache`
/// - fallback: `/var/tmp`
pub fn cache_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".cache");
    }
    PathBuf::from("/var/tmp")
}

/// Data directory heuristic:
/// - `$XDG_DATA_HOME`
/// - `$HOME/.local/share`
/// - fallback: `/usr/share`
pub fn data_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".local").join("share");
    }
    PathBuf::from("/usr/share")
}

/// State directory heuristic:
/// - `$XDG_STATE_HOME`
/// - `$HOME/.local/state`
/// - fallback: `/var`
pub fn state_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join(".local").join("state");
    }
    PathBuf::from("/var")
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

/// Default shell:
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

/// Minimal environment defaults for tool execution.
pub fn default_env() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();

    if let Ok(p) = env::var("PATH") {
        m.insert("PATH".into(), p);
    }

    if env::var("LC_ALL").is_err() && env::var("LANG").is_err() {
        m.insert("LANG".into(), "C".into());
    }

    m.insert("MUFFIN_PLATFORM".into(), "solaris".into());
    m.insert("MUFFIN_SOLARIS_KIND".into(), detect_compile_time_kind().as_str().into());
    m
}

/* ---------------------------- FS helpers ---------------------------- */

/// True if file has any executable bit (unix mode & 0o111).
#[cfg(unix)]
pub fn is_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let md = std::fs::metadata(path)?;
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

/* ---------------------------- Runtime probes ---------------------------- */

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

/// Best-effort "zone" detection:
/// - `/etc/zones/index` exists on Solaris
/// - `zonename` command existence cannot be checked without spawning; so we just use file check.
/// Returns None if unknown.
fn zone_best_effort() -> Option<bool> {
    let idx = Path::new("/etc/zones/index");
    if idx.exists() {
        return Some(true);
    }
    // On illumos, zones also exist; absence of file doesn't prove not-a-zone.
    None
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_non_empty() {
        assert!(!config_dir().as_os_str().is_empty());
        assert!(!cache_dir().as_os_str().is_empty());
        assert!(!data_dir().as_os_str().is_empty());
        assert!(!state_dir().as_os_str().is_empty());
    }

    #[test]
    fn default_shell_abs() {
        let sh = default_shell();
        assert!(sh.is_absolute());
    }

    #[test]
    fn kind_str() {
        let _ = detect_compile_time_kind().as_str();
    }
}
