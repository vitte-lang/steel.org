//! Version utilities (version.rs) — MAX (std-only).
//!
//! This module centralizes Steel/SteelLib version strings and compatibility helpers.
//!
//! Design goals:
//! - Provide a single place to query the library version.
//! - Provide lightweight parsing/comparison without pulling `semver` crate.
//! - Provide "wire format" versions for on-disk formats (mff/cas/index/...) and
//!   compatibility checks.
//!
//! Notes:
//! - This is NOT a full semver implementation.
//! - Parsing is permissive: "1.2.3", "1.2", "1", "1.2.3-alpha+meta" accepted.
//! - Comparison uses numeric major/minor/patch; pre-release metadata is ignored
//!   for ordering by default (but preserved as strings).

use std::fmt;

/// Library version string from Cargo.
pub const MUFFINLIB_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Library name from Cargo.
pub const MUFFINLIB_NAME: &str = env!("CARGO_PKG_NAME");

/// Common format version constants (bump when breaking on-disk formats).
pub mod formats {
    /// CAS layout version.
    pub const CAS_V: u32 = 1;
    /// Store index format version.
    pub const STORE_INDEX_V: u32 = 1;
    /// MFF schema version (placeholder; bump when you define schema.rs).
    pub const MFF_SCHEMA_V: u32 = 1;
}

/// A very small semver-like structure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// optional pre-release tag (e.g. "alpha.1")
    pub pre: Option<String>,
    /// optional build metadata (e.g. "20260108")
    pub build: Option<String>,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch, pre: None, build: None }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        // Split build metadata
        let (core, build) = match s.split_once('+') {
            Some((a, b)) => (a, Some(b.to_string())),
            None => (s, None),
        };

        // Split pre-release
        let (nums, pre) = match core.split_once('-') {
            Some((a, b)) => (a, Some(b.to_string())),
            None => (core, None),
        };

        let mut it = nums.split('.');
        let major = it.next()?.parse::<u32>().ok()?;
        let minor = it.next().map(|x| x.parse::<u32>().ok()).flatten().unwrap_or(0);
        let patch = it.next().map(|x| x.parse::<u32>().ok()).flatten().unwrap_or(0);

        Some(Self { major, minor, patch, pre, build })
    }

    /// Compare numeric components only (ignore pre/build).
    pub fn cmp_numeric(&self, other: &Version) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }

    /// Return true if this version is compatible with `other` under a simple rule:
    /// - same major => compatible
    /// - otherwise incompatible
    pub fn compatible_major(&self, other: &Version) -> bool {
        self.major == other.major
    }

    /// Render as string.
    pub fn to_string_full(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if let Some(pre) = &self.pre {
            s.push('-');
            s.push_str(pre);
        }
        if let Some(build) = &self.build {
            s.push('+');
            s.push_str(build);
        }
        s
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_full())
    }
}

/// Return parsed library version (best-effort).
pub fn steellib_version_parsed() -> Version {
    Version::parse(MUFFINLIB_VERSION).unwrap_or_else(|| Version::new(0, 0, 0))
}

/// Return a concise identification string.
pub fn id_string() -> String {
    format!(
        "{} {} ({}/{})",
        MUFFINLIB_NAME,
        MUFFINLIB_VERSION,
        env!("CARGO_CFG_TARGET_OS"),
        env!("CARGO_CFG_TARGET_ARCH")
    )
}

/// Check if a given on-disk CAS version is supported.
pub fn cas_version_supported(v: u32) -> bool {
    v == formats::CAS_V
}

/// Check if a given store index version is supported.
pub fn store_index_version_supported(v: u32) -> bool {
    v == formats::STORE_INDEX_V
}

/// Check if a given MFF schema version is supported.
pub fn mff_schema_version_supported(v: u32) -> bool {
    v == formats::MFF_SCHEMA_V
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn parse_short() {
        let v = Version::parse("2").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_pre_build() {
        let v = Version::parse("1.2.3-alpha+meta").unwrap();
        assert_eq!(v.pre.as_deref(), Some("alpha"));
        assert_eq!(v.build.as_deref(), Some("meta"));
    }

    #[test]
    fn compat_major() {
        let a = Version::new(1, 0, 0);
        let b = Version::new(1, 9, 9);
        let c = Version::new(2, 0, 0);
        assert!(a.compatible_major(&b));
        assert!(!a.compatible_major(&c));
    }
}
