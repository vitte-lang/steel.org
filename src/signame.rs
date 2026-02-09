// C:\Users\gogin\Documents\GitHub\steel\src\signame.rs
//
// Steel — signal naming and normalization
//
// Purpose:
// - Provide a stable mapping between OS signals and names, for diagnostics/logging.
// - Support parsing user input like "SIGINT", "int", "2", "TERM", "sigterm".
// - Provide canonical formatting for exit reporting (e.g., "SIGINT(2)").
// - Provide POSIX-like mappings (portable subset), with best-effort Windows support.
//
// Notes:
// - On Unix, exit code for signal termination is commonly 128 + signal.
// - This file does not attempt to deliver signals; it only maps/prints/parses.
// - Windows does not have POSIX signals; Rust exposes limited "signal-like" concepts.
//   We still accept names for cross-platform config/UI consistency.
//
// Integration:
// - Used by vms_exit.rs and CLI parsing (e.g. --on-signal=TERM).
//
// Safety / determinism:
// - Table-driven, no platform syscalls.
// - "Unknown" signals are preserved numerically (when parsing number).

#![allow(dead_code)]

use std::fmt;

/* ============================== model ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Signal {
    pub num: i32,
}

impl Signal {
    pub const fn new(num: i32) -> Self {
        Self { num }
    }

    pub fn is_valid(self) -> bool {
        self.num > 0
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = signal_name(self.num) {
            write!(f, "{name}({})", self.num)
        } else {
            write!(f, "SIG{}({})", self.num, self.num)
        }
    }
}

/* ============================== canonical table ============================== */

// Portable subset + common Linux/BSD additions.
// Numbers follow common POSIX conventions.
// If your platform differs, treat names as best-effort display only.
#[derive(Debug, Clone, Copy)]
struct SigEntry {
    num: i32,
    name: &'static str, // canonical "SIGINT"
    short: &'static str, // "INT"
}

const SIG_TABLE: &[SigEntry] = &[
    SigEntry { num: 1, name: "SIGHUP", short: "HUP" },
    SigEntry { num: 2, name: "SIGINT", short: "INT" },
    SigEntry { num: 3, name: "SIGQUIT", short: "QUIT" },
    SigEntry { num: 4, name: "SIGILL", short: "ILL" },
    SigEntry { num: 5, name: "SIGTRAP", short: "TRAP" },
    SigEntry { num: 6, name: "SIGABRT", short: "ABRT" },
    SigEntry { num: 7, name: "SIGBUS", short: "BUS" },
    SigEntry { num: 8, name: "SIGFPE", short: "FPE" },
    SigEntry { num: 9, name: "SIGKILL", short: "KILL" },
    SigEntry { num: 10, name: "SIGUSR1", short: "USR1" },
    SigEntry { num: 11, name: "SIGSEGV", short: "SEGV" },
    SigEntry { num: 12, name: "SIGUSR2", short: "USR2" },
    SigEntry { num: 13, name: "SIGPIPE", short: "PIPE" },
    SigEntry { num: 14, name: "SIGALRM", short: "ALRM" },
    SigEntry { num: 15, name: "SIGTERM", short: "TERM" },
    SigEntry { num: 16, name: "SIGSTKFLT", short: "STKFLT" }, // Linux-only (best-effort)
    SigEntry { num: 17, name: "SIGCHLD", short: "CHLD" },
    SigEntry { num: 18, name: "SIGCONT", short: "CONT" },
    SigEntry { num: 19, name: "SIGSTOP", short: "STOP" },
    SigEntry { num: 20, name: "SIGTSTP", short: "TSTP" },
    SigEntry { num: 21, name: "SIGTTIN", short: "TTIN" },
    SigEntry { num: 22, name: "SIGTTOU", short: "TTOU" },
    SigEntry { num: 23, name: "SIGURG", short: "URG" },
    SigEntry { num: 24, name: "SIGXCPU", short: "XCPU" },
    SigEntry { num: 25, name: "SIGXFSZ", short: "XFSZ" },
    SigEntry { num: 26, name: "SIGVTALRM", short: "VTALRM" },
    SigEntry { num: 27, name: "SIGPROF", short: "PROF" },
    SigEntry { num: 28, name: "SIGWINCH", short: "WINCH" },
    SigEntry { num: 29, name: "SIGIO", short: "IO" },
    SigEntry { num: 30, name: "SIGPWR", short: "PWR" }, // Linux-only
    SigEntry { num: 31, name: "SIGSYS", short: "SYS" },
];

/// Return canonical signal name like "SIGINT" for a known signal number.
pub fn signal_name(num: i32) -> Option<&'static str> {
    SIG_TABLE.iter().find(|e| e.num == num).map(|e| e.name)
}

/// Return canonical short name like "INT" for a known signal number.
pub fn signal_short_name(num: i32) -> Option<&'static str> {
    SIG_TABLE.iter().find(|e| e.num == num).map(|e| e.short)
}

/// Return number for a given name.
/// Accepts: "SIGINT", "INT", "sigint", "int", with whitespace.
pub fn signal_number(name: &str) -> Option<i32> {
    let n = normalize_name(name);
    if n.is_empty() {
        return None;
    }

    // Accept pure numeric.
    if let Some(num) = parse_i32(&n) {
        return if num > 0 { Some(num) } else { None };
    }

    // Accept "SIGxxx" and "xxx"
    for e in SIG_TABLE {
        if n == e.name || n == e.short {
            return Some(e.num);
        }
    }
    None
}

/// Parse user input into a Signal.
/// Accepts: "2", "SIGINT", "int", "sigterm", "TERM".
pub fn parse_signal(input: &str) -> Result<Signal, SignalParseError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(SignalParseError::Empty);
    }

    let n = normalize_name(raw);

    if let Some(num) = parse_i32(&n) {
        if num <= 0 {
            return Err(SignalParseError::InvalidNumber { value: num });
        }
        return Ok(Signal::new(num));
    }

    if let Some(num) = signal_number(&n) {
        return Ok(Signal::new(num));
    }

    Err(SignalParseError::UnknownName { name: raw.to_string() })
}

/// Canonical formatting helper.
pub fn format_signal(num: i32) -> String {
    if let Some(name) = signal_name(num) {
        format!("{name}({num})")
    } else {
        format!("SIG{num}({num})")
    }
}

/// Return exit code for a signal termination, using conventional 128+signal.
pub fn signal_exit_code(num: i32) -> i32 {
    128 + num
}

/// Inverse: if exit code looks like signal termination (>=128), return the signal number.
pub fn signal_from_exit_code(code: i32) -> Option<i32> {
    if code >= 128 {
        Some(code - 128)
    } else {
        None
    }
}

/* ============================== normalization ============================== */

fn normalize_name(s: &str) -> String {
    let mut out = String::new();
    let t = s.trim();

    // strip leading "SIG" (case-insensitive), but preserve if user typed numeric.
    let upper = t.to_ascii_uppercase();
    let stripped = upper.strip_prefix("SIG").unwrap_or(&upper);

    // keep only [A-Z0-9]
    for c in stripped.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        }
    }
    out
}

fn parse_i32(s: &str) -> Option<i32> {
    // strict numeric parse (no +/-)
    if s.is_empty() {
        return None;
    }
    if !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse::<i32>().ok()
}

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalParseError {
    Empty,
    InvalidNumber { value: i32 },
    UnknownName { name: String },
}

impl fmt::Display for SignalParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalParseError::Empty => write!(f, "signal is empty"),
            SignalParseError::InvalidNumber { value } => write!(f, "invalid signal number: {value}"),
            SignalParseError::UnknownName { name } => write!(f, "unknown signal name: {name}"),
        }
    }
}

impl std::error::Error for SignalParseError {}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_lookup() {
        assert_eq!(signal_name(2), Some("SIGINT"));
        assert_eq!(signal_short_name(15), Some("TERM"));
        assert_eq!(signal_name(999), None);
    }

    #[test]
    fn number_lookup_accepts_variants() {
        assert_eq!(signal_number("SIGINT"), Some(2));
        assert_eq!(signal_number("int"), Some(2));
        assert_eq!(signal_number("sigterm"), Some(15));
        assert_eq!(signal_number("TERM"), Some(15));
    }

    #[test]
    fn parse_signal_numeric() {
        let s = parse_signal("2").unwrap();
        assert_eq!(s.num, 2);
        assert!(parse_signal("0").is_err());
    }

    #[test]
    fn parse_signal_named() {
        let s = parse_signal("SIGINT").unwrap();
        assert_eq!(s.num, 2);
        let s = parse_signal("term").unwrap();
        assert_eq!(s.num, 15);
    }

    #[test]
    fn format_signal_unknown() {
        assert_eq!(format_signal(2), "SIGINT(2)");
        assert_eq!(format_signal(999), "SIG999(999)");
    }

    #[test]
    fn exit_code_mapping() {
        assert_eq!(signal_exit_code(2), 130);
        assert_eq!(signal_from_exit_code(130), Some(2));
        assert_eq!(signal_from_exit_code(42), None);
    }
}
