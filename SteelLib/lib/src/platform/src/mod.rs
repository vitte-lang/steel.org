//! Platform module root (mod.rs) — MAX.
//!
//! Provides a minimal cross-platform facade and OS-specific helpers.

pub mod bsd;
pub mod linux;
pub mod macos;
pub mod solaris;
pub mod windows;

/// Simple platform descriptor (compile-time best-effort).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: &'static str,
    pub arch: &'static str,
}

impl Platform {
    /// Return compile-time target OS/arch when available.
    pub fn current() -> Self {
        let os = option_env!("CARGO_CFG_TARGET_OS").unwrap_or("unknown");
        let arch = option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("unknown");
        Self { os, arch }
    }
}
