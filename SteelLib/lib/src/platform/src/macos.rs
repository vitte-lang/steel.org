//! macOS / Darwin platform helpers (macos.rs) — MAX (std-only).
//!
//! This module provides macOS-specific utilities used by Steel.
//! Scope:
//! - OS identification (compile-time + runtime best-effort)
//! - default paths (macOS conventions + XDG fallback)
//! - filesystem helpers (executable bit, symlink)
//! - process helpers (shell, env defaults)
//! - bundle/app support helpers (best-effort, std-only)
//!
//! Notes:
//! - std-only: no `libc`, no `core-foundation`, no `plist`. Use environment + well-known paths.
//! - Prefer deterministic output, and keep runtime probing optional.
//! - For richer platform integration, add feature-gated modules.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MacosInfo {
    pub target_os: &'static str,
    pub target_arch: &'static str,

    // runtime best-effort:
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub home_dir: Option<PathBuf>,
    pub shell: Option<PathBuf>,

    // app/bundle:
    pub app_bundle_path: Option<PathBuf>,
}

impl MacosInfo {
    pub fn gather() -> Self {
        let home = home_dir_best_effort();
        Self {
            target_os: option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown"),
            target_arch: option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown"),
            hostname: hostname_best_effort(),
            username: username_best_effort(),
            home_dir: home.clone(),
            shell: shell_best_effort(),
            app_bundle_path: current_app_bundle_best_effort(),
        }
    }
}

pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

/* ---------------------------- Default paths ---------------------------- */

/// macOS config dir heuristic:
/// - `$XDG_CONFIG_HOME` if set (respects XDG for CLI apps)
/// - `$HOME/Library/Application Support`
/// - fallback: `/Library/Application Support`
pub fn config_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join("Library").join("Application Support");
    }
    PathBuf::from("/Library/Application Support")
}

/// macOS cache dir heuristic:
/// - `$XDG_CACHE_HOME`
/// - `$HOME/Library/Caches`
/// - fallback: `/Library/Caches`
pub fn cache_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(x);
    }
    if let Some(home) = home_dir_best_effort() {
        return home.join("Library").join("Caches");
    }
    PathBuf::from("/Library/Caches")
}

/// macOS data dir heuristic (similar to config, but can be separate in your design):
/// - `$XDG_DATA_HOME`
/// - `$HOME/Library/Application Support`
/// - fallback: `/Library/Application Support`
pub fn data_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x);
    }
    config_dir()
}

/// macOS state dir heuristic:
/// - `$XDG_STATE_HOME`
/// - `$HOME/Library/Application Support` (no strict state dir convention)
pub fn state_dir() -> PathBuf {
    if let Some(x) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(x);
    }
    config_dir()
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
/// - fallback: `/bin/zsh` (modern macOS default), then `/bin/sh`
pub fn default_shell() -> PathBuf {
    if let Some(s) = env::var_os("SHELL") {
        let p = PathBuf::from(s);
        if p.is_absolute() {
            return p;
        }
    }
    let zsh = PathBuf::from("/bin/zsh");
    if zsh.exists() {
        return zsh;
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

    m.insert("MUFFIN_PLATFORM".into(), "macos".into());
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

/* ---------------------------- App bundle helpers ---------------------------- */

/// Best-effort current executable path.
pub fn current_exe_best_effort() -> Option<PathBuf> {
    env::current_exe().ok()
}

/// If the current executable is inside a `.app` bundle, return the bundle root path.
/// Typical layout:
///   MyApp.app/Contents/MacOS/MyApp
pub fn current_app_bundle_best_effort() -> Option<PathBuf> {
    let exe = current_exe_best_effort()?;
    let mut p = exe.as_path();

    // Walk up looking for "*.app/Contents/MacOS/*"
    // We detect "Contents" then check parent endswith ".app".
    while let Some(parent) = p.parent() {
        if parent.file_name().and_then(|s| s.to_str()) == Some("MacOS") {
            let contents = parent.parent()?;
            if contents.file_name().and_then(|s| s.to_str()) == Some("Contents") {
                let app = contents.parent()?;
                if app.extension().and_then(|s| s.to_str()) == Some("app") {
                    return Some(app.to_path_buf());
                }
            }
        }
        p = parent;
    }

    None
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
    fn app_bundle_probe_does_not_panic() {
        let _ = current_app_bundle_best_effort();
    }
}
