// C:\Users\gogin\Documents\GitHub\steel\src\version.rs
//
// Steel — versioning primitives
//
// Goals:
// - Single source of truth for: crate version, build metadata, git info, target triple.
// - Provide a stable `VersionInfo` payload for `steel --version`, diagnostics banners, logs.
// - Support reproducible builds: metadata can be injected via env vars by CI.
// - Zero/low dependencies: no semver crate required (we keep it lightweight).
//
// Integration points:
// - In build.rs you can export env vars like:
//   - MUFFIN_VERSION
//   - MUFFIN_GIT_SHA
//   - MUFFIN_GIT_DIRTY
//   - MUFFIN_BUILD_TIME_UTC
//   - MUFFIN_TARGET
//   - MUFFIN_PROFILE
//   - MUFFIN_RUSTC
//   - MUFFIN_HOSTNAME (optional)
// - Also supports Cargo-provided vars:
//   - CARGO_PKG_VERSION, CARGO_PKG_NAME
//   - PROFILE, TARGET, HOST
//   - RUSTC (custom), RUSTC_VERSION (custom)
//
// Notes:
// - This module is designed to be called early during startup.
// - Keep strings small and printable for logs.
//
// Example usage:
//   let v = VersionInfo::current();
//   println!("{}", v.format_long());
//   println!("{}", v.format_short());

#![allow(dead_code)]

use std::borrow::Cow;
use std::fmt;

/// Environment variable overrides (CI/build.rs can set these).
pub const ENV_VERSION: &str = "MUFFIN_VERSION";
pub const ENV_GIT_SHA: &str = "MUFFIN_GIT_SHA";
pub const ENV_GIT_DIRTY: &str = "MUFFIN_GIT_DIRTY";
pub const ENV_BUILD_TIME_UTC: &str = "MUFFIN_BUILD_TIME_UTC";
pub const ENV_TARGET: &str = "MUFFIN_TARGET";
pub const ENV_PROFILE: &str = "MUFFIN_PROFILE";
pub const ENV_RUSTC: &str = "MUFFIN_RUSTC";
pub const ENV_HOSTNAME: &str = "MUFFIN_HOSTNAME";
pub const ENV_BUILD_ID: &str = "MUFFIN_BUILD_ID";

/// Cargo vars (available at compile-time via env! / option_env!).
pub const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional compile-time vars (provided by Cargo or build tooling).
pub const OPT_PROFILE: Option<&'static str> = option_env!("PROFILE");
pub const OPT_TARGET: Option<&'static str> = option_env!("TARGET");
pub const OPT_HOST: Option<&'static str> = option_env!("HOST");

/// A compact version structure (not strict semver; good enough for display/comparison).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
    pub build: Option<String>,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: None,
            build: None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        // Accept formats like:
        //  - 1.2.3
        //  - 1.2.3-alpha
        //  - 1.2.3-alpha+build.7
        //  - 1.2.3+build.7
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let (main_and_pre, build) = match s.split_once('+') {
            Some((a, b)) => (a, Some(b.trim().to_string()).filter(|x| !x.is_empty())),
            None => (s, None),
        };

        let (main, pre) = match main_and_pre.split_once('-') {
            Some((a, b)) => (a.trim(), Some(b.trim().to_string()).filter(|x| !x.is_empty())),
            None => (main_and_pre.trim(), None),
        };

        let mut parts = main.split('.').map(|p| p.trim());
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
            pre,
            build,
        })
    }

    pub fn to_string_compact(&self) -> String {
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
        f.write_str(&self.to_string_compact())
    }
}

/// One canonical payload to print everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub name: String,
    pub version: String,

    pub git_sha: Option<String>,
    pub git_dirty: Option<bool>,

    pub build_time_utc: Option<String>,
    pub build_id: Option<String>,

    pub target: Option<String>,
    pub host: Option<String>,
    pub profile: Option<String>,
    pub rustc: Option<String>,
    pub hostname: Option<String>,
}

impl VersionInfo {
    /// Build `VersionInfo` from compile-time + runtime env overrides.
    pub fn current() -> Self {
        let mut v = Self::from_compile_time();
        v.apply_runtime_env_overrides();
        v.sanitize();
        v
    }

    /// Best-effort from compile-time information only.
    pub fn from_compile_time() -> Self {
        Self {
            name: CARGO_PKG_NAME.to_string(),
            version: CARGO_PKG_VERSION.to_string(),
            git_sha: None,
            git_dirty: None,
            build_time_utc: None,
            build_id: None,
            target: OPT_TARGET.map(|s| s.to_string()),
            host: OPT_HOST.map(|s| s.to_string()),
            profile: OPT_PROFILE.map(|s| s.to_string()),
            rustc: option_env!("RUSTC_VERSION").map(|s| s.to_string()).or_else(|| option_env!("RUSTC").map(|s| s.to_string())),
            hostname: None,
        }
    }

    /// Apply runtime overrides (env vars set at runtime).
    pub fn apply_runtime_env_overrides(&mut self) {
        // Version override
        if let Ok(s) = std::env::var(ENV_VERSION) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                self.version = s;
            }
        }

        // Name override (rare, but for wrappers)
        if let Ok(s) = std::env::var("MUFFIN_NAME") {
            let s = s.trim().to_string();
            if !s.is_empty() {
                self.name = s;
            }
        }

        if let Ok(s) = std::env::var(ENV_GIT_SHA) {
            self.git_sha = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_GIT_DIRTY) {
            self.git_dirty = parse_bool(&s);
        }

        if let Ok(s) = std::env::var(ENV_BUILD_TIME_UTC) {
            self.build_time_utc = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_BUILD_ID) {
            self.build_id = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_TARGET) {
            self.target = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_PROFILE) {
            self.profile = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_RUSTC) {
            self.rustc = non_empty(s);
        }

        if let Ok(s) = std::env::var(ENV_HOSTNAME) {
            self.hostname = non_empty(s);
        }
    }

    /// Normalize fields for printing.
    pub fn sanitize(&mut self) {
        self.name = self.name.trim().to_string();
        self.version = self.version.trim().to_string();
        if self.name.is_empty() {
            self.name = "steel".to_string();
        }
        if self.version.is_empty() {
            self.version = "0.0.0".to_string();
        }

        self.git_sha = self.git_sha.as_ref().and_then(|s| short_git_sha(s));
        // keep dirty as-is

        // Trim all option strings
        self.build_time_utc = self.build_time_utc.take().and_then(non_empty);
        self.build_id = self.build_id.take().and_then(non_empty);
        self.target = self.target.take().and_then(non_empty);
        self.host = self.host.take().and_then(non_empty);
        self.profile = self.profile.take().and_then(non_empty);
        self.rustc = self.rustc.take().and_then(non_empty);
        self.hostname = self.hostname.take().and_then(non_empty);
    }

    /// Short string: "steel 1.2.3 (abc1234 dirty)" or "steel 1.2.3".
    pub fn format_short(&self) -> String {
        let mut s = format!("{} {}", self.name, self.version);

        if let Some(sha) = &self.git_sha {
            s.push_str(" (");
            s.push_str(sha);

            if self.git_dirty.unwrap_or(false) {
                s.push_str(" dirty");
            }

            s.push(')');
        }

        s
    }

    /// Long / verbose string, stable keys (nice for `--version --verbose`).
    pub fn format_long(&self) -> String {
        // key alignment for readability
        let mut out = String::new();

        push_kv(&mut out, "name", &self.name);
        push_kv(&mut out, "version", &self.version);

        if let Some(sha) = &self.git_sha {
            push_kv(&mut out, "git_sha", sha);
        }
        if let Some(dirty) = self.git_dirty {
            push_kv(&mut out, "git_dirty", if dirty { "true" } else { "false" });
        }
        if let Some(t) = &self.build_time_utc {
            push_kv(&mut out, "build_time_utc", t);
        }
        if let Some(id) = &self.build_id {
            push_kv(&mut out, "build_id", id);
        }
        if let Some(t) = &self.target {
            push_kv(&mut out, "target", t);
        }
        if let Some(h) = &self.host {
            push_kv(&mut out, "host", h);
        }
        if let Some(p) = &self.profile {
            push_kv(&mut out, "profile", p);
        }
        if let Some(r) = &self.rustc {
            push_kv(&mut out, "rustc", r);
        }
        if let Some(hn) = &self.hostname {
            push_kv(&mut out, "hostname", hn);
        }

        // Trim trailing newline
        while out.ends_with('\n') {
            out.pop();
        }
        out
    }

    /// A single-line "UA-like" string, useful for HTTP headers / registry calls.
    /// Example: "steel/1.2.3 (x86_64-unknown-linux-gnu; release; abc1234)"
    pub fn format_user_agent(&self) -> String {
        let mut s = format!("{}/{}", self.name, self.version);

        let mut bits: Vec<Cow<'_, str>> = Vec::new();

        if let Some(t) = &self.target {
            bits.push(Cow::Borrowed(t));
        }
        if let Some(p) = &self.profile {
            bits.push(Cow::Borrowed(p));
        }
        if let Some(sha) = &self.git_sha {
            bits.push(Cow::Borrowed(sha));
        }

        if !bits.is_empty() {
            s.push_str(" (");
            for (i, b) in bits.iter().enumerate() {
                if i != 0 {
                    s.push_str("; ");
                }
                s.push_str(b);
            }
            s.push(')');
        }

        s
    }
}

/* ============================= helpers ============================= */

fn push_kv(out: &mut String, k: &str, v: &str) {
    out.push_str(k);
    out.push_str(": ");
    out.push_str(v);
    out.push('\n');
}

fn non_empty<S: Into<String>>(s: S) -> Option<String> {
    let s = s.into();
    let t = s.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Shorten git sha to 7..12 chars (common), keep only hex-ish chars.
fn short_git_sha(s: &str) -> Option<String> {
    let raw = s.trim();
    if raw.is_empty() {
        return None;
    }

    // keep [0-9a-fA-F]
    let filtered: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if filtered.is_empty() {
        return None;
    }

    let n = filtered.len();
    let take = if n >= 12 { 12 } else if n >= 7 { 7 } else { n };
    Some(filtered[..take].to_string())
}

/* ============================= tests ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_basic() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre, None);
        assert_eq!(v.build, None);
        assert_eq!(v.to_string_compact(), "1.2.3");
    }

    #[test]
    fn version_parse_pre_build() {
        let v = Version::parse("1.2.3-alpha+build.7").unwrap();
        assert_eq!(v.pre.as_deref(), Some("alpha"));
        assert_eq!(v.build.as_deref(), Some("build.7"));
        assert_eq!(v.to_string_compact(), "1.2.3-alpha+build.7");
    }

    #[test]
    fn short_git_sha_filters_and_shortens() {
        assert_eq!(short_git_sha("abc1234def5678").as_deref(), Some("abc1234def56"));
        assert_eq!(short_git_sha("  ABCD  ").as_deref(), Some("ABCD"));
        assert_eq!(short_git_sha("----").as_deref(), None);
    }

    #[test]
    fn parse_bool_ok() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("???"), None);
    }

    #[test]
    fn format_short_includes_sha_dirty() {
        let mut vi = VersionInfo::from_compile_time();
        vi.name = "steel".to_string();
        vi.version = "1.2.3".to_string();
        vi.git_sha = Some("abc1234def".to_string());
        vi.git_dirty = Some(true);
        vi.sanitize();
        let s = vi.format_short();
        assert!(s.contains("steel 1.2.3"));
        assert!(s.contains("abc1234"));
        assert!(s.contains("dirty"));
    }

    #[test]
    fn format_user_agent_contains_bits() {
        let mut vi = VersionInfo::from_compile_time();
        vi.name = "steel".to_string();
        vi.version = "1.2.3".to_string();
        vi.target = Some("x86_64-unknown-linux-gnu".to_string());
        vi.profile = Some("release".to_string());
        vi.git_sha = Some("abc1234def".to_string());
        vi.sanitize();
        let ua = vi.format_user_agent();
        assert!(ua.starts_with("steel/1.2.3"));
        assert!(ua.contains("x86_64-unknown-linux-gnu"));
        assert!(ua.contains("release"));
        assert!(ua.contains("abc1234"));
    }
}
