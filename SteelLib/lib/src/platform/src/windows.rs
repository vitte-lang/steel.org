//! Windows platform helpers (windows.rs) — MAX (std-only).
//!
//! Scope:
//! - compile-time identification
//! - default paths (APPDATA/LOCALAPPDATA/PROGRAMDATA/TEMP)
//! - executable detection (extensions + metadata best-effort)
//! - symlink creation (requires privileges; std-only best-effort)
//! - process helpers (ComSpec / PowerShell selection, env defaults)
//!
//! Notes:
//! - std-only: no winapi crate. Use env vars + std::fs.
//! - For robust Known Folders (FOLDERID_*), add a feature using `windows` crate.
//! - Be careful with path normalization: Windows paths can be verbatim, UNC, etc.
//! - Determinism: prefer returning explicit defaults if env is missing.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WindowsInfo {
    pub target_os: &'static str,
    pub target_arch: &'static str,

    // runtime best-effort:
    pub computer_name: Option<String>,
    pub username: Option<String>,
    pub user_profile: Option<PathBuf>,
    pub shell: Option<PathBuf>,

    // dirs:
    pub roaming_appdata: PathBuf,
    pub local_appdata: PathBuf,
    pub program_data: PathBuf,
    pub temp_dir: PathBuf,
}

impl WindowsInfo {
    pub fn gather() -> Self {
        Self {
            target_os: option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown"),
            target_arch: option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown"),
            computer_name: env::var("COMPUTERNAME").ok(),
            username: username_best_effort(),
            user_profile: env::var_os("USERPROFILE").map(PathBuf::from),
            shell: shell_best_effort(),
            roaming_appdata: roaming_appdata_dir(),
            local_appdata: local_appdata_dir(),
            program_data: program_data_dir(),
            temp_dir: temp_dir(),
        }
    }
}

pub fn is_windows() -> bool {
    cfg!(windows)
}

/* ---------------------------- Default paths ---------------------------- */

/// Roaming AppData:
/// - `%APPDATA%`
/// - fallback: `%USERPROFILE%\AppData\Roaming`
/// - fallback: `C:\Users\Default\AppData\Roaming` (last resort)
pub fn roaming_appdata_dir() -> PathBuf {
    if let Some(p) = env::var_os("APPDATA") {
        return PathBuf::from(p);
    }
    if let Some(up) = env::var_os("USERPROFILE") {
        return PathBuf::from(up).join("AppData").join("Roaming");
    }
    PathBuf::from(r"C:\Users\Default\AppData\Roaming")
}

/// Local AppData:
/// - `%LOCALAPPDATA%`
/// - fallback: `%USERPROFILE%\AppData\Local`
/// - fallback: `C:\Users\Default\AppData\Local`
pub fn local_appdata_dir() -> PathBuf {
    if let Some(p) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(p);
    }
    if let Some(up) = env::var_os("USERPROFILE") {
        return PathBuf::from(up).join("AppData").join("Local");
    }
    PathBuf::from(r"C:\Users\Default\AppData\Local")
}

/// ProgramData:
/// - `%PROGRAMDATA%`
/// - fallback: `C:\ProgramData`
pub fn program_data_dir() -> PathBuf {
    if let Some(p) = env::var_os("PROGRAMDATA") {
        return PathBuf::from(p);
    }
    PathBuf::from(r"C:\ProgramData")
}

pub fn temp_dir() -> PathBuf {
    env::temp_dir()
}

pub fn steel_config_dir() -> PathBuf {
    roaming_appdata_dir().join("Steel")
}

pub fn steel_cache_dir() -> PathBuf {
    local_appdata_dir().join("Steel").join("Cache")
}

pub fn steel_data_dir() -> PathBuf {
    local_appdata_dir().join("Steel").join("Data")
}

pub fn steel_state_dir() -> PathBuf {
    local_appdata_dir().join("Steel").join("State")
}

/* ---------------------------- Process helpers ---------------------------- */

/// Best-effort "default shell" on Windows.
/// - `%ComSpec%` typically points to `cmd.exe`
/// - fallback: `C:\Windows\System32\cmd.exe`
pub fn default_shell_cmd() -> PathBuf {
    if let Some(p) = env::var_os("ComSpec") {
        let pb = PathBuf::from(p);
        if pb.is_absolute() {
            return pb;
        }
    }
    PathBuf::from(r"C:\Windows\System32\cmd.exe")
}

/// Best-effort PowerShell path:
/// - Windows PowerShell: `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`
/// - PowerShell 7+: often `C:\Program Files\PowerShell\7\pwsh.exe`
///
/// This is heuristic (std-only). Prefer searching PATH in higher-level code if needed.
pub fn default_shell_powershell() -> PathBuf {
    let win_ps = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    if win_ps.exists() {
        return win_ps;
    }
    let pwsh7 = PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe");
    if pwsh7.exists() {
        return pwsh7;
    }
    default_shell_cmd()
}

/// Minimal environment defaults for tool execution.
pub fn default_env() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();

    if let Ok(p) = env::var("PATH") {
        m.insert("PATH".into(), p);
    }

    // Stable locale is not identical on Windows; keep as-is unless you want forced UTF-8:
    // - `chcp 65001` would be required for cmd.exe; do not set here.
    m.insert("MUFFIN_PLATFORM".into(), "windows".into());

    m
}

/* ---------------------------- FS helpers ---------------------------- */

/// Return true if a path looks like an executable on Windows.
/// - checks extension: .exe, .cmd, .bat, .com
/// - if no extension: best-effort false
pub fn is_executable_path(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "exe" | "cmd" | "bat" | "com")
}

/// Return true if the file exists and is executable by extension (best-effort).
pub fn is_executable_file(path: &Path) -> std::io::Result<bool> {
    let md = fs::metadata(path)?;
    if !md.is_file() {
        return Ok(false);
    }
    Ok(is_executable_path(path))
}

/// Try to create a symlink.
///
/// On Windows, symlink creation may require admin or Developer Mode.
/// `std::os::windows::fs::symlink_file/dir` exists; we choose based on metadata.
///
/// If `src` doesn't exist, you must pass `is_dir` hint via `SymlinkKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymlinkKind {
    File,
    Dir,
    Auto,
}

pub fn symlink(src: &Path, dst: &Path, kind: SymlinkKind) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::{symlink_dir, symlink_file};

        let k = match kind {
            SymlinkKind::File => SymlinkKind::File,
            SymlinkKind::Dir => SymlinkKind::Dir,
            SymlinkKind::Auto => {
                if let Ok(md) = fs::metadata(src) {
                    if md.is_dir() {
                        SymlinkKind::Dir
                    } else {
                        SymlinkKind::File
                    }
                } else {
                    // unknown -> assume file
                    SymlinkKind::File
                }
            }
        };

        match k {
            SymlinkKind::Dir => symlink_dir(src, dst),
            _ => symlink_file(src, dst),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (src, dst, kind);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlink requires windows target",
        ))
    }
}

/* ---------------------------- Runtime best-effort ---------------------------- */

fn username_best_effort() -> Option<String> {
    env::var("USERNAME").ok()
}

fn shell_best_effort() -> Option<PathBuf> {
    env::var_os("ComSpec").map(PathBuf::from)
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_non_empty() {
        assert!(!roaming_appdata_dir().as_os_str().is_empty());
        assert!(!local_appdata_dir().as_os_str().is_empty());
        assert!(!program_data_dir().as_os_str().is_empty());
        assert!(!temp_dir().as_os_str().is_empty());
    }

    #[test]
    fn exec_ext_detection() {
        assert!(is_executable_path(Path::new("a.exe")));
        assert!(is_executable_path(Path::new("a.CMD")));
        assert!(!is_executable_path(Path::new("a.txt")));
    }
}
