//! BSD platform helpers (bsd.rs) — MAX (std-only).
//!
//! This module provides BSD-oriented platform utilities used by Steel.
//! Targets:
//! - FreeBSD
//! - OpenBSD
//! - NetBSD
//! - DragonFlyBSD
//! - (optionally) macOS/Darwin can share some POSIX helpers, but keep separate
//!
//! Scope:
//! - OS identification (compile-time + runtime best-effort)
//! - default paths (temp, cache, config, data)
//! - filesystem quirks helpers (symlink, executable bit detection)
//! - process helpers (shell selection, env defaults)
//! - feature probes (best-effort, std-only)
//!
//! Notes:
//! - std-only: no sysctl crate, no libc crate. Runtime probing is best-effort.
//! - For richer info (sysctl kern.ostype, etc.), add an optional `libc` feature.
//! - Keep API deterministic: any runtime probing must be explicitly requested.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsdKind {
    FreeBsd,
    OpenBsd,
    NetBsd,
    DragonFly,
    Unknown,
}

impl BsdKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BsdKind::FreeBsd => "freebsd",
            BsdKind::OpenBsd => "openbsd",
            BsdKind::NetBsd => "netbsd",
            BsdKind::DragonFly => "dragonfly",
            BsdKind::Unknown => "bsd-unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BsdInfo {
    /// Compile-time best guess.
    pub kind: BsdKind,
    /// Compile-time target triple-ish information.
    pub target_os: &'static str,
    pub target_arch: &'static str,

    /// Runtime best-effort fields (may be None if not available).
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub shell: Option<PathBuf>,
}

impl BsdInfo {
    pub fn gather() -> Self {
        let kind = detect_compile_time_kind();
        Self {
            kind,
            target_os: option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown"),
            target_arch: option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown"),
            hostname: hostname_best_effort(),
            username: username_best_effort(),
            home_dir: home_dir_best_effort(),
            shell: shell_best_effort(),
        }
    }
}

/* ---------------------------- OS detection ---------------------------- */

pub fn detect_compile_time_kind() -> BsdKind {
    // Prefer actual BSD targets
    #[cfg(target_os = "freebsd")]
    {
        return BsdKind::FreeBsd;
    }
    #[cfg(target_os = "openbsd")]
    {
        return BsdKind::OpenBsd;
    }
    #[cfg(target_os = "netbsd")]
    {
        return BsdKind::NetBsd;
    }
    #[cfg(target_os = "dragonfly")]
    {
        return BsdKind::DragonFly;
    }

    // Not a BSD target at compile time
    BsdKind::Unknown
}

/// Runtime "is BSD" check based on compile-time kind.
pub fn is_bsd() -> bool {
    detect_compile_time_kind() != BsdKind::Unknown
}

/* ---------------------------- Default paths ---------------------------- */

/// Return typical config directory:
/// - `$XDG_CONFIG_HOME` if set
/// - `$HOME/.config`
/// - fallback: `/etc` (system-ish)
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

/// Return temp directory (std).
pub fn temp_dir() -> PathBuf {
    env::temp_dir()
}

/// Return default Steel config directory (e.g. for `steel/config.muf`).
pub fn steel_config_dir() -> PathBuf {
    config_dir().join("steel")
}

/// Return default Steel cache directory.
pub fn steel_cache_dir() -> PathBuf {
    cache_dir().join("steel")
}

/// Return default Steel data directory.
pub fn steel_data_dir() -> PathBuf {
    data_dir().join("steel")
}

/* ------------------------- Process / shell helpers ---------------------- */

/// Best-effort shell path:
/// - `$SHELL` if set and absolute
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

/// Returns a default env for running tools (minimal).
pub fn default_env() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();

    // Preserve PATH if present
    if let Ok(p) = env::var("PATH") {
        m.insert("PATH".into(), p);
    }

    // Common locale defaults (avoid surprising tool output)
    if env::var("LC_ALL").is_err() && env::var("LANG").is_err() {
        m.insert("LANG".into(), "C".into());
    }

    // Optional: Steel marker
    m.insert("MUFFIN_PLATFORM".into(), "bsd".into());
    m
}

/* ----------------------------- FS helpers ------------------------------- */

/// True if file has executable bit set (best-effort).
///
/// On Unix, std exposes mode bits via `PermissionsExt`.
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

/// Try to create a symlink. On BSD this is supported (unix).
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

/* --------------------------- Runtime probes ----------------------------- */

fn home_dir_best_effort() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn username_best_effort() -> Option<String> {
    // No libc; use env variables.
    env::var("USER")
        .ok()
        .or_else(|| env::var("LOGNAME").ok())
}

fn shell_best_effort() -> Option<PathBuf> {
    env::var_os("SHELL").map(PathBuf::from)
}

fn hostname_best_effort() -> Option<String> {
    // std-only: try HOSTNAME env, else None.
    env::var("HOSTNAME").ok()
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_are_non_empty() {
        assert!(!config_dir().as_os_str().is_empty());
        assert!(!cache_dir().as_os_str().is_empty());
        assert!(!data_dir().as_os_str().is_empty());
        assert!(!temp_dir().as_os_str().is_empty());
    }

    #[test]
    fn default_shell_is_abs() {
        let sh = default_shell();
        assert!(sh.is_absolute());
    }

    #[test]
    fn info_gather() {
        let info = BsdInfo::gather();
        assert!(!info.target_os.is_empty());
        assert!(!info.target_arch.is_empty());
    }
}
