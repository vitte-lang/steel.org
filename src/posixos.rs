//! OS Version Detection
//!
//! Runtime detection of operating system versions and capabilities.

use crate::os::{OsVersion, OsTier};
use anyhow::Result;
use std::process::Command;

/// Detect the current OS version
pub fn detect_os_version() -> Result<OsVersion> {
    #[cfg(target_os = "windows")]
    {
        detect_windows_version()
    }

    #[cfg(target_os = "macos")]
    {
        detect_macos_version()
    }

    #[cfg(target_os = "linux")]
    {
        detect_linux_version()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Ok(OsVersion::default())
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_version() -> Result<OsVersion> {
    // Try Get-ComputerInfo PowerShell command
    if let Ok(output) = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "[Environment]::OSVersion.Version | Select -ExpandProperty Major,Minor,Build",
        ])
        .output()
    {
        if let Ok(output_str) = String::from_utf8(output.stdout) {
            let parts: Vec<&str> = output_str.split_whitespace().collect();
            if parts.len() >= 3 {
                return Ok(OsVersion {
                    major: parts[0].parse().unwrap_or(0),
                    minor: parts[1].parse().unwrap_or(0),
                    patch: parts[2].parse().unwrap_or(0),
                });
            }
        }
    }

    // Fallback: try WMI (older systems)
    if let Ok(output) = Command::new("wmic")
        .args(&["os", "get", "version"])
        .output()
    {
        if let Ok(output_str) = String::from_utf8(output.stdout) {
            if let Some(line) = output_str.lines().nth(1) {
                let parts: Vec<&str> = line.split('.').collect();
                if parts.len() >= 2 {
                    return Ok(OsVersion {
                        major: parts[0].parse().unwrap_or(0),
                        minor: parts[1].parse().unwrap_or(0),
                        patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                    });
                }
            }
        }
    }

    // If all else fails, return default
    Ok(OsVersion::default())
}

#[cfg(target_os = "macos")]
fn detect_macos_version() -> Result<OsVersion> {
    if let Ok(output) = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
    {
        if let Ok(version_str) = String::from_utf8(output.stdout) {
            let parts: Vec<&str> = version_str.trim().split('.').collect();
            return Ok(OsVersion {
                major: parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0),
                minor: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
            });
        }
    }

    Ok(OsVersion::default())
}

#[cfg(target_os = "linux")]
fn detect_linux_version() -> Result<OsVersion> {
    // Try /etc/os-release (systemd standard)
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        return parse_os_release(&content);
    }

    // Try /etc/lsb-release (Ubuntu/Debian)
    if let Ok(content) = std::fs::read_to_string("/etc/lsb-release") {
        return parse_lsb_release(&content);
    }

    // Try /etc/redhat-release (CentOS/RHEL)
    if let Ok(content) = std::fs::read_to_string("/etc/redhat-release") {
        return parse_redhat_release(&content);
    }

    // Fallback: try `uname -r`
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        if let Ok(version_str) = String::from_utf8(output.stdout) {
            let parts: Vec<&str> = version_str.trim().split('.').collect();
            if parts.len() >= 2 {
                return Ok(OsVersion {
                    major: parts[0].parse().unwrap_or(0),
                    minor: parts[1].parse().unwrap_or(0),
                    patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                });
            }
        }
    }

    Ok(OsVersion::default())
}

#[cfg(target_os = "linux")]
fn parse_os_release(content: &str) -> Result<OsVersion> {
    for line in content.lines() {
        if let Some(version_str) = line.strip_prefix("VERSION_ID=") {
            let version_clean = version_str.trim_matches('"').trim_matches('\'');
            let parts: Vec<&str> = version_clean.split('.').collect();
            return Ok(OsVersion {
                major: parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0),
                minor: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
            });
        }
    }

    Ok(OsVersion::default())
}

#[cfg(target_os = "linux")]
fn parse_lsb_release(content: &str) -> Result<OsVersion> {
    for line in content.lines() {
        if let Some(version_str) = line.strip_prefix("DISTRIB_RELEASE=") {
            let parts: Vec<&str> = version_str.split('.').collect();
            return Ok(OsVersion {
                major: parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0),
                minor: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
            });
        }
    }

    Ok(OsVersion::default())
}

#[cfg(target_os = "linux")]
fn parse_redhat_release(content: &str) -> Result<OsVersion> {
    // Extract version from lines like "CentOS Linux release 7.9.2009"
    if let Some(line) = content.lines().next() {
        for word in line.split_whitespace() {
            if let Ok(major) = word.parse::<u32>() {
                let parts: Vec<&str> = word.split('.').collect();
                return Ok(OsVersion {
                    major,
                    minor: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                    patch: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                });
            }
        }
    }

    Ok(OsVersion::default())
}

/// Classify OS tier based on version
pub fn classify_tier(version: OsVersion) -> OsTier {
    let os_name = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    match os_name {
        "macos" => {
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
        "linux" => {
            // Simplified heuristic based on glibc version
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
        "windows" => {
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
        _ => OsTier::Compatible,
    }
}

/// Check if the OS supports a specific feature
pub fn supports_feature(feature: &str, tier: OsTier) -> bool {
    match (feature, tier) {
        ("parallel_jobs", tier) => tier >= OsTier::Compatible,
        ("symlinks", tier) => tier >= OsTier::Modern,
        ("hardlinks", _) => true,
        ("unicode_paths", tier) => tier >= OsTier::Compatible,
        ("long_paths", OsTier::Current | OsTier::Modern) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_os_version() {
        let _version = detect_os_version().unwrap();
        // Version detection should return a valid structure
    }

    #[test]
    fn test_classify_tier() {
        let legacy = OsVersion {
            major: 6,
            minor: 0,
            patch: 0,
        };
        assert_eq!(classify_tier(legacy), OsTier::Legacy);

        let modern = if cfg!(target_os = "macos") {
            OsVersion {
                major: 12,
                minor: 0,
                patch: 0,
            }
        } else if cfg!(target_os = "linux") {
            OsVersion {
                major: 2,
                minor: 33,
                patch: 0,
            }
        } else if cfg!(target_os = "windows") {
            OsVersion {
                major: 10,
                minor: 0,
                patch: 0,
            }
        } else {
            OsVersion {
                major: 10,
                minor: 0,
                patch: 0,
            }
        };
        assert_eq!(classify_tier(modern), OsTier::Modern);
    }

    #[test]
    fn test_supports_feature() {
        assert!(supports_feature("hardlinks", OsTier::Legacy));
        assert!(supports_feature(
            "parallel_jobs",
            OsTier::Compatible
        ));
        assert!(!supports_feature("symlinks", OsTier::Legacy));
    }
}
