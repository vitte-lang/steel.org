// /Users/vincent/Documents/Github/steel/src/debug.rs
//! debug — logging, tracing, and diagnostics helpers (std-only)
//!
//! Goals:
//! - deterministic and lightweight logging (no external crates)
//! - consistent message format across Steel modules
//! - opt-in verbosity (quiet/info/debug/trace)
//! - small helpers for timing + scoped sections
//!
//! Typical usage:
//! - wire `DebugConfig` from CLI flags / env vars
//! - call `log_*` helpers or use `section()` for scoped messages
//!
//! Environment variables:
//! - `MUFFIN_LOG` : one of `quiet|error|warn|info|debug|trace`
//! - `MUFFIN_COLOR` : `0/1/auto` (default auto; best-effort ANSI)
//! - `MUFFIN_TIMING` : `0/1` (default 0) enable timing output

use std::env;
use std::fmt;
use std::io::{self, Write};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Quiet = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    pub fn parse(s: &str) -> Option<LogLevel> {
        match s.trim().to_ascii_lowercase().as_str() {
            "quiet" | "silent" | "off" => Some(LogLevel::Quiet),
            "error" | "err" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Quiet => "quiet",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn parse(s: &str) -> Option<ColorMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ColorMode::Auto),
            "1" | "on" | "yes" | "true" | "always" => Some(ColorMode::Always),
            "0" | "off" | "no" | "false" | "never" => Some(ColorMode::Never),
            _ => None,
        }
    }
}

/// Runtime logging configuration.
#[derive(Debug, Clone)]
pub struct DebugConfig {
    pub level: LogLevel,
    pub color: ColorMode,
    pub timing: bool,
    pub prefix: &'static str,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            level: env_level().unwrap_or(LogLevel::Info),
            color: env_color().unwrap_or(ColorMode::Auto),
            timing: env_flag("MUFFIN_TIMING"),
            prefix: "steel",
        }
    }
}

fn env_level() -> Option<LogLevel> {
    env::var("MUFFIN_LOG").ok().and_then(|s| LogLevel::parse(&s))
}

fn env_color() -> Option<ColorMode> {
    env::var("MUFFIN_COLOR").ok().and_then(|s| ColorMode::parse(&s))
}

fn env_flag(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn should_color(cfg: &DebugConfig) -> bool {
    match cfg.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            // Best-effort: color if stderr is a TTY. std-only: no isatty stable API.
            // Use env hint + common CI vars.
            if env_flag("NO_COLOR") {
                return false;
            }
            if env::var("CI").is_ok() {
                // In CI, default to no color unless explicitly forced.
                return false;
            }
            // Default to true in interactive contexts.
            true
        }
    }
}

fn ansi(code: &'static str, enabled: bool) -> &'static str {
    if enabled {
        code
    } else {
        ""
    }
}

fn lvl_tag(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Quiet => "",
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

fn lvl_color(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "\x1b[31m", // red
        LogLevel::Warn => "\x1b[33m",  // yellow
        LogLevel::Info => "\x1b[36m",  // cyan
        LogLevel::Debug => "\x1b[35m", // magenta
        LogLevel::Trace => "\x1b[90m", // gray
        LogLevel::Quiet => "",
    }
}

/// Central logging function.
pub fn log(cfg: &DebugConfig, level: LogLevel, msg: impl AsRef<str>) {
    if cfg.level == LogLevel::Quiet || level > cfg.level {
        return;
    }
    let colored = should_color(cfg);
    let reset = ansi("\x1b[0m", colored);

    let tag = lvl_tag(level);
    let col = lvl_color(level);
    let col = ansi(col, colored);

    let mut w = io::stderr().lock();
    let _ = writeln!(
        w,
        "[{}] {}{}{}: {}",
        cfg.prefix,
        col,
        tag,
        reset,
        msg.as_ref()
    );
}

pub fn error(cfg: &DebugConfig, msg: impl AsRef<str>) {
    log(cfg, LogLevel::Error, msg);
}
pub fn warn(cfg: &DebugConfig, msg: impl AsRef<str>) {
    log(cfg, LogLevel::Warn, msg);
}
pub fn info(cfg: &DebugConfig, msg: impl AsRef<str>) {
    log(cfg, LogLevel::Info, msg);
}
pub fn debug(cfg: &DebugConfig, msg: impl AsRef<str>) {
    log(cfg, LogLevel::Debug, msg);
}
pub fn trace(cfg: &DebugConfig, msg: impl AsRef<str>) {
    log(cfg, LogLevel::Trace, msg);
}

/// Log an error and return a std::io::Error compatible message (convenience).
pub fn io_fail(cfg: &DebugConfig, op: &str, err: &io::Error) -> String {
    let s = format!("{op}: {err}");
    error(cfg, &s);
    s
}

/// A scoped logging section with optional timing.
pub struct Section<'a> {
    cfg: &'a DebugConfig,
    name: String,
    level: LogLevel,
    start: Instant,
    enabled: bool,
}

impl<'a> Section<'a> {
    pub fn new(cfg: &'a DebugConfig, level: LogLevel, name: impl Into<String>) -> Self {
        let enabled = cfg.level != LogLevel::Quiet && level <= cfg.level;
        let name = name.into();
        if enabled {
            log(cfg, level, format!("begin: {name}"));
        }
        Self {
            cfg,
            name,
            level,
            start: Instant::now(),
            enabled,
        }
    }

    pub fn note(&self, msg: impl AsRef<str>) {
        if self.enabled {
            log(
                self.cfg,
                self.level,
                format!("{}: {}", self.name, msg.as_ref()),
            );
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl<'a> Drop for Section<'a> {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        if self.cfg.timing {
            let d = self.start.elapsed();
            log(
                self.cfg,
                self.level,
                format!("end: {} ({} ms)", self.name, d.as_millis()),
            );
        } else {
            log(self.cfg, self.level, format!("end: {}", self.name));
        }
    }
}

/// Convenience constructor.
pub fn section(cfg: &DebugConfig, level: LogLevel, name: impl Into<String>) -> Section<'_> {
    Section::new(cfg, level, name)
}

/// Small helper to pretty-print a key-value map with deterministic ordering.
pub fn fmt_kv(map: &std::collections::BTreeMap<String, String>) -> String {
    let mut s = String::new();
    for (k, v) in map {
        s.push_str(k);
        s.push('=');
        s.push_str(v);
        s.push('\n');
    }
    s
}

/// Minimal error type for modules that want std::error::Error without pulling dependencies.
#[derive(Debug, Clone)]
pub struct SimpleError(pub String);

impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SimpleError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn parse_levels() {
        assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::parse("off"), Some(LogLevel::Quiet));
        assert_eq!(LogLevel::parse("nope"), None);
    }

    #[test]
    fn section_timing_does_not_panic() {
        let mut cfg = DebugConfig::default();
        cfg.level = LogLevel::Trace;
        cfg.timing = true;

        {
            let _s = section(&cfg, LogLevel::Info, "phase");
        }
    }

    #[test]
    fn kv_is_deterministic() {
        let mut m = BTreeMap::new();
        m.insert("b".to_string(), "2".to_string());
        m.insert("a".to_string(), "1".to_string());
        let s = fmt_kv(&m);
        assert!(s.lines().next().unwrap().starts_with("a="));
    }
}
