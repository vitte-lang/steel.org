//! OS Abstraction Layer
//!
//! Provides a unified interface for OS-specific operations, with implementations
//! for Windows (modern and legacy), Unix-like systems, and a fallback pure Rust mode.

use std::path::PathBuf;
use std::process::{Child, Command};
use anyhow::Result;

/// Architecture types supported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X86_64,
    I686,
    ARM64,
    ARM,
    PowerPC,
    Unknown,
}

impl Architecture {
    pub fn current() -> Self {
        match std::env::consts::ARCH {
            "x86_64" => Architecture::X86_64,
            "x86" => Architecture::I686,
            "aarch64" => Architecture::ARM64,
            "arm" => Architecture::ARM,
            "powerpc64" => Architecture::PowerPC,
            _ => Architecture::Unknown,
        }
    }
}

/// OS Version information
#[derive(Debug, Clone, Copy)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Default for OsVersion {
    fn default() -> Self {
        OsVersion {
            major: 0,
            minor: 0,
            patch: 0,
        }
    }
}

/// Tier classification for feature support
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OsTier {
    Legacy,     // Windows XP, macOS 10.9, CentOS 6
    Compatible, // Windows 7+, macOS 10.14+, Ubuntu 16.04+
    Modern,     // Windows 10+, macOS 11+, Ubuntu 20.04+
    Current,    // Windows 11, macOS 13+, Ubuntu 22.04+
}

/// Abstract OS adapter trait
pub trait OsAdapter: Send + Sync + std::fmt::Debug {
    /// OS name (e.g., "Linux", "Windows", "macOS")
    fn name(&self) -> &'static str;

    /// OS version
    fn version(&self) -> OsVersion;

    /// Architecture
    fn arch(&self) -> Architecture;

    /// OS tier classification
    fn tier(&self) -> OsTier;

    // --- File operations ---

    /// Path component separator ('/' for Unix, '\\' for Windows)
    fn path_separator(&self) -> char;

    /// Whether symlinks are supported
    fn symlink_support(&self) -> bool;

    /// Whether hardlinks are supported
    fn hardlink_support(&self) -> bool;

    /// Get temporary directory
    fn temp_dir(&self) -> PathBuf;

    /// Get cache directory
    fn cache_dir(&self) -> PathBuf;

    // --- Process management ---

    /// Spawn a child process
    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child>;

    /// Get number of available CPUs
    fn cpu_count(&self) -> usize;

    /// Whether parallel jobs are supported
    fn supports_parallel_jobs(&self) -> bool;

    // --- Environment ---

    /// Get environment variable
    fn get_env(&self, key: &str) -> Option<String>;

    /// Set environment variable
    fn set_env(&self, key: &str, value: &str) -> Result<()>;

    // --- System capabilities ---

    /// Whether this adapter has all features available
    fn has_fallback(&self) -> bool {
        false
    }

    /// Diagnostic information
    fn diagnostic_info(&self) -> String {
        format!(
            "OS: {} v{}.{}.{}, Arch: {:?}, Tier: {:?}",
            self.name(),
            self.version().major,
            self.version().minor,
            self.version().patch,
            self.arch(),
            self.tier(),
        )
    }
}

/// Pure Rust fallback implementation (works everywhere)
#[derive(Debug)]
pub struct PureRustFallback;

impl OsAdapter for PureRustFallback {
    fn name(&self) -> &'static str {
        "Generic (Fallback Mode)"
    }

    fn version(&self) -> OsVersion {
        OsVersion::default()
    }

    fn arch(&self) -> Architecture {
        Architecture::current()
    }

    fn tier(&self) -> OsTier {
        OsTier::Legacy
    }

    fn path_separator(&self) -> char {
        #[cfg(windows)]
        {
            '\\'
        }
        #[cfg(not(windows))]
        {
            '/'
        }
    }

    fn symlink_support(&self) -> bool {
        false
    }

    fn hardlink_support(&self) -> bool {
        false
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn cache_dir(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from(format!(
                "{}/Library/Caches",
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            ))
        }
        #[cfg(target_os = "windows")]
        {
            PathBuf::from(
                std::env::var("APPDATA")
                    .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| "C:\\Temp".to_string())),
            )
        }
        #[cfg(target_os = "linux")]
        {
            PathBuf::from(format!(
                "{}/.cache",
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            ))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            std::env::temp_dir()
        }
    }

    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child> {
        let mut command = Command::new(cmd);
        command.args(args);
        Ok(command.spawn()?)
    }

    fn cpu_count(&self) -> usize {
        num_cpus::get()
    }

    fn supports_parallel_jobs(&self) -> bool {
        false // Safe default in fallback mode
    }

    fn get_env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set_env(&self, key: &str, value: &str) -> Result<()> {
        std::env::set_var(key, value);
        Ok(())
    }

    fn has_fallback(&self) -> bool {
        true
    }
}

/// Get the current OS adapter based on runtime detection
pub fn get_current_os() -> Box<dyn OsAdapter> {
    #[cfg(unix)]
    {
        Box::new(UnixAdapter)
    }
    #[cfg(windows)]
    {
        Box::new(WindowsAdapter)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Box::new(PureRustFallback)
    }
}

// --- Unix/POSIX Implementation ---

#[cfg(unix)]
#[derive(Debug)]
pub struct UnixAdapter;

#[cfg(unix)]
impl OsAdapter for UnixAdapter {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "linux") {
            "Linux"
        } else if cfg!(target_os = "freebsd") {
            "FreeBSD"
        } else {
            "Unix"
        }
    }

    fn version(&self) -> OsVersion {
        detect_unix_version()
    }

    fn arch(&self) -> Architecture {
        Architecture::current()
    }

    fn tier(&self) -> OsTier {
        let version = self.version();

        #[cfg(target_os = "macos")]
        {
            if version.major < 10 || (version.major == 10 && version.minor < 14) {
                OsTier::Legacy
            } else if version.major < 11 {
                OsTier::Compatible
            } else if version.major < 13 {
                OsTier::Modern
            } else {
                OsTier::Current
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Simplified: use glibc version as proxy
            if version.major < 2 || (version.major == 2 && version.minor < 23) {
                OsTier::Legacy
            } else if version.major == 2 && version.minor < 29 {
                OsTier::Compatible
            } else if version.major == 2 && version.minor < 34 {
                OsTier::Modern
            } else {
                OsTier::Current
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            OsTier::Compatible
        }
    }

    fn path_separator(&self) -> char {
        '/'
    }

    fn symlink_support(&self) -> bool {
        true
    }

    fn hardlink_support(&self) -> bool {
        true
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn cache_dir(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from(format!(
                "{}/Library/Caches",
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            PathBuf::from(format!(
                "{}/.cache",
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            ))
        }
    }

    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child> {
        let mut command = Command::new(cmd);
        command.args(args);
        Ok(command.spawn()?)
    }

    fn cpu_count(&self) -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }

    fn supports_parallel_jobs(&self) -> bool {
        self.tier() >= OsTier::Modern
    }

    fn get_env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set_env(&self, key: &str, value: &str) -> Result<()> {
        std::env::set_var(key, value);
        Ok(())
    }
}

#[cfg(unix)]
fn detect_unix_version() -> OsVersion {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
        {
            if let Ok(version_str) = String::from_utf8(output.stdout) {
                let parts: Vec<&str> = version_str.trim().split('.').collect();
                return OsVersion {
                    major: parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0),
                    minor: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                    patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                };
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try libc
        let mut sysinfo: libc::sysinfo = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sysinfo(&mut sysinfo);
        }
        // sysinfo doesn't provide exact version; this is simplified
    }

    OsVersion::default()
}

// --- Windows Implementation ---

#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsAdapter;

#[cfg(windows)]
impl OsAdapter for WindowsAdapter {
    fn name(&self) -> &'static str {
        "Windows"
    }

    fn version(&self) -> OsVersion {
        detect_windows_version()
    }

    fn arch(&self) -> Architecture {
        Architecture::current()
    }

    fn tier(&self) -> OsTier {
        let version = self.version();
        if version.major < 7 {
            OsTier::Legacy
        } else if version.major < 10 {
            OsTier::Compatible
        } else if version.major == 10 {
            OsTier::Modern
        } else {
            OsTier::Current
        }
    }

    fn path_separator(&self) -> char {
        '\\'
    }

    fn symlink_support(&self) -> bool {
        self.tier() >= OsTier::Modern
    }

    fn hardlink_support(&self) -> bool {
        true
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn cache_dir(&self) -> PathBuf {
        PathBuf::from(
            std::env::var("APPDATA")
                .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| "C:\\Temp".to_string())),
        )
    }

    fn spawn_process(&self, cmd: &str, args: &[&str]) -> Result<Child> {
        let mut command = Command::new(cmd);
        command.args(args);
        Ok(command.spawn()?)
    }

    fn cpu_count(&self) -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }

    fn supports_parallel_jobs(&self) -> bool {
        self.tier() >= OsTier::Compatible
    }

    fn get_env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set_env(&self, key: &str, value: &str) -> Result<()> {
        std::env::set_var(key, value);
        Ok(())
    }
}

#[cfg(windows)]
fn detect_windows_version() -> OsVersion {
    // Simplified; actual implementation would use WinAPI
    OsVersion::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_os() {
        let os = get_current_os();
        assert!(!os.name().is_empty());
    }

    #[test]
    fn test_pure_rust_fallback() {
        let fallback = PureRustFallback;
        assert_eq!(fallback.tier(), OsTier::Legacy);
        assert!(fallback.has_fallback());
    }

    #[test]
    fn test_temp_dir_exists() {
        let os = get_current_os();
        let temp = os.temp_dir();
        assert!(!temp.as_os_str().is_empty());
    }

    #[test]
    fn test_cpu_count_positive() {
        let os = get_current_os();
        assert!(os.cpu_count() > 0);
    }
}
