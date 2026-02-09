// C:\Users\gogin\Documents\GitHub\steel\src\vms_exit.rs
//
// Steel — VMS (Virtual Steel System) utilities
// Process exit codes, termination policy, and "best-effort" exit helpers.
//
// Goals:
// - Single source of truth for exit codes across Steel.
// - Provide ergonomic conversion from errors/diagnostics to exit status.
// - Support "soft exit" (return code) vs "hard exit" (std::process::exit).
// - Keep policy consistent: 0 success, non-zero failures.
//
// Notes:
// - This file intentionally avoids depending on heavy subsystems.
// - Integrate with your diagnostics layer by implementing ExitCodeProvider for your error types.

#![allow(dead_code)]

use std::fmt;
use std::process;

/// Canonical process exit code.
pub type ExitCode = i32;

/// Success.
pub const EXIT_OK: ExitCode = 0;

/// Generic failure.
pub const EXIT_ERROR: ExitCode = 1;

/// CLI usage / arguments error (often aligned with sysexits(3) style).
pub const EXIT_USAGE: ExitCode = 2;

/// Configuration error (invalid config, missing config, parse error, etc).
pub const EXIT_CONFIG: ExitCode = 3;

/// I/O error (filesystem, permissions, read/write failure).
pub const EXIT_IO: ExitCode = 4;

/// Network / remote fetch error (registry, HTTP, etc).
pub const EXIT_NET: ExitCode = 5;

/// Toolchain / external tool invocation error (clang, linkers, etc).
pub const EXIT_TOOL: ExitCode = 6;

/// Build graph / dependency resolution error.
pub const EXIT_DEPS: ExitCode = 7;

/// Compilation error (front-end/middle-end/back-end; diagnostics emitted).
pub const EXIT_COMPILE: ExitCode = 8;

/// Runtime / VM execution error.
pub const EXIT_RUNTIME: ExitCode = 9;

/// Internal bug (panic-like; invariant broken).
pub const EXIT_INTERNAL: ExitCode = 10;

/// Interrupted / cancelled (Ctrl+C, SIGINT) — keep it distinct for scripting.
pub const EXIT_INTERRUPTED: ExitCode = 130;

/// Terminated by signal (common Unix convention: 128 + signal).
pub const EXIT_SIGNAL_BASE: ExitCode = 128;

/// Policy for converting various conditions into an exit code.
#[derive(Debug, Clone)]
pub struct ExitPolicy {
    /// If true, "warnings only" still yields success.
    pub warnings_are_ok: bool,
    /// If true, treat warnings as failure (useful for CI).
    pub warnings_as_errors: bool,
    /// If true, treat "no work to do" as success.
    pub no_work_is_ok: bool,
    /// Exit code used when warnings are treated as errors.
    pub warnings_exit_code: ExitCode,
    /// Exit code used for generic errors when no better mapping exists.
    pub default_error_code: ExitCode,
}

impl Default for ExitPolicy {
    fn default() -> Self {
        Self {
            warnings_are_ok: true,
            warnings_as_errors: false,
            no_work_is_ok: true,
            warnings_exit_code: EXIT_ERROR,
            default_error_code: EXIT_ERROR,
        }
    }
}

/// A normalized "exit decision": code + optional message (for higher layers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitDecision {
    pub code: ExitCode,
    pub reason: ExitReason,
}

impl ExitDecision {
    #[inline]
    pub fn ok() -> Self {
        Self {
            code: EXIT_OK,
            reason: ExitReason::Ok,
        }
    }

    #[inline]
    pub fn with(code: ExitCode, reason: ExitReason) -> Self {
        Self { code, reason }
    }

    #[inline]
    pub fn is_ok(&self) -> bool {
        self.code == EXIT_OK
    }
}

/// Human-readable / structured reason for exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    Ok,
    Usage,
    Config,
    Io,
    Net,
    Tool,
    Deps,
    Compile,
    Runtime,
    Internal,
    Interrupted,
    Signal { signal: i32 },
    Custom { code: ExitCode, message: String },
}

impl fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExitReason::Ok => write!(f, "ok"),
            ExitReason::Usage => write!(f, "usage error"),
            ExitReason::Config => write!(f, "configuration error"),
            ExitReason::Io => write!(f, "I/O error"),
            ExitReason::Net => write!(f, "network error"),
            ExitReason::Tool => write!(f, "tool invocation error"),
            ExitReason::Deps => write!(f, "dependency resolution error"),
            ExitReason::Compile => write!(f, "compilation error"),
            ExitReason::Runtime => write!(f, "runtime error"),
            ExitReason::Internal => write!(f, "internal error"),
            ExitReason::Interrupted => write!(f, "interrupted"),
            ExitReason::Signal { signal } => write!(f, "terminated by signal {signal}"),
            ExitReason::Custom { code, message } => write!(f, "exit {code}: {message}"),
        }
    }
}

/// Trait for mapping errors to an exit decision.
pub trait ExitCodeProvider {
    fn exit_decision(&self, policy: &ExitPolicy) -> ExitDecision;
}

/// Convenience for types that only provide a numeric exit code.
pub trait ExitCodeOnly {
    fn exit_code(&self) -> ExitCode;
}

impl<T: ExitCodeOnly> ExitCodeProvider for T {
    fn exit_decision(&self, policy: &ExitPolicy) -> ExitDecision {
        let code = self.exit_code();
        ExitDecision::with(code, ExitReason::Custom { code, message: String::new() })
            .with_default_reason(policy)
    }
}

impl ExitDecision {
    fn with_default_reason(mut self, policy: &ExitPolicy) -> Self {
        if let ExitReason::Custom { code, message } = &self.reason {
            if message.is_empty() {
                self.reason = reason_from_code(*code).unwrap_or_else(|| ExitReason::Custom {
                    code: *code,
                    message: String::new(),
                });
            }
        }
        if self.code == 0 {
            self.reason = ExitReason::Ok;
        } else if self.code == EXIT_ERROR && matches!(self.reason, ExitReason::Custom { .. }) {
            self.reason = ExitReason::Custom {
                code: policy.default_error_code,
                message: String::new(),
            };
        }
        self
    }
}

/// Map canonical codes to reasons (best-effort).
pub fn reason_from_code(code: ExitCode) -> Option<ExitReason> {
    match code {
        EXIT_OK => Some(ExitReason::Ok),
        EXIT_USAGE => Some(ExitReason::Usage),
        EXIT_CONFIG => Some(ExitReason::Config),
        EXIT_IO => Some(ExitReason::Io),
        EXIT_NET => Some(ExitReason::Net),
        EXIT_TOOL => Some(ExitReason::Tool),
        EXIT_DEPS => Some(ExitReason::Deps),
        EXIT_COMPILE => Some(ExitReason::Compile),
        EXIT_RUNTIME => Some(ExitReason::Runtime),
        EXIT_INTERNAL => Some(ExitReason::Internal),
        EXIT_INTERRUPTED => Some(ExitReason::Interrupted),
        c if c >= EXIT_SIGNAL_BASE => Some(ExitReason::Signal { signal: (c - EXIT_SIGNAL_BASE) as i32 }),
        _ => None,
    }
}

/// Convert std::io::Error to a canonical exit decision.
pub fn from_io_error(_err: &std::io::Error) -> ExitDecision {
    ExitDecision::with(EXIT_IO, ExitReason::Io)
}

/// Convert "interrupted" (Ctrl+C) to canonical exit decision.
pub fn interrupted() -> ExitDecision {
    ExitDecision::with(EXIT_INTERRUPTED, ExitReason::Interrupted)
}

/// Convert a Unix signal number into exit code (128 + signal).
pub fn signal_exit_code(signal: i32) -> ExitCode {
    EXIT_SIGNAL_BASE + signal
}

/// Simple execution result summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub had_errors: bool,
    pub had_warnings: bool,
    pub did_work: bool,
}

impl RunSummary {
    pub fn ok() -> Self {
        Self {
            had_errors: false,
            had_warnings: false,
            did_work: true,
        }
    }
}

/// Decide exit code from a run summary and policy.
pub fn decide_from_summary(summary: RunSummary, policy: &ExitPolicy) -> ExitDecision {
    if summary.had_errors {
        return ExitDecision::with(EXIT_ERROR, ExitReason::Custom { code: EXIT_ERROR, message: String::new() })
            .with_default_reason(policy);
    }

    if summary.had_warnings && policy.warnings_as_errors {
        return ExitDecision::with(policy.warnings_exit_code, ExitReason::Custom {
            code: policy.warnings_exit_code,
            message: "warnings treated as errors".to_string(),
        });
    }

    if !summary.did_work && !policy.no_work_is_ok {
        return ExitDecision::with(EXIT_ERROR, ExitReason::Custom { code: EXIT_ERROR, message: "no work".to_string() });
    }

    ExitDecision::ok()
}

/// Exit immediately with a code.
pub fn hard_exit(code: ExitCode) -> ! {
    process::exit(code)
}

/// Exit immediately with an ExitDecision.
pub fn hard_exit_decision(decision: ExitDecision) -> ! {
    process::exit(decision.code)
}

/// Return code (soft exit) from decision.
pub fn soft_exit_code(decision: ExitDecision) -> ExitCode {
    decision.code
}

/// Map a `Result<T, E>` into an exit code.
/// - Ok => 0
/// - Err => resolved via ExitCodeProvider if implemented, else default error.
pub fn exit_code_from_result<T, E>(res: Result<T, E>, policy: &ExitPolicy) -> ExitCode
where
    E: ExitCodeProvider,
{
    match res {
        Ok(_) => EXIT_OK,
        Err(e) => e.exit_decision(policy).code,
    }
}

/// Map a `Result<T, anyhow-like>` into an exit code by downcasting (optional integration).
/// This is a small hook you can adapt if you use `anyhow`.
pub fn exit_code_from_display_error<T, E>(res: Result<T, E>, policy: &ExitPolicy) -> ExitCode
where
    E: fmt::Display,
{
    match res {
        Ok(_) => EXIT_OK,
        Err(_e) => policy.default_error_code,
    }
}

/// Normalize arbitrary user-provided numeric exit values into i32 range.
/// - Negative => default_error_code
/// - >255 => clamped to 255 (portable-ish for shells)
pub fn normalize_numeric_exit(code: i64, policy: &ExitPolicy) -> ExitCode {
    if code < 0 {
        return policy.default_error_code;
    }
    let c = if code > 255 { 255 } else { code as i32 };
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_code() {
        assert_eq!(signal_exit_code(2), 130);
        assert_eq!(reason_from_code(130), Some(ExitReason::Interrupted)); // matches EXIT_INTERRUPTED
    }

    #[test]
    fn decide_summary_ok() {
        let p = ExitPolicy::default();
        let d = decide_from_summary(RunSummary::ok(), &p);
        assert_eq!(d.code, EXIT_OK);
        assert_eq!(d.reason, ExitReason::Ok);
    }

    #[test]
    fn decide_summary_warnings_as_errors() {
        let mut p = ExitPolicy::default();
        p.warnings_as_errors = true;
        p.warnings_exit_code = EXIT_ERROR;

        let d = decide_from_summary(
            RunSummary {
                had_errors: false,
                had_warnings: true,
                did_work: true,
            },
            &p,
        );
        assert_eq!(d.code, EXIT_ERROR);
    }

    #[test]
    fn normalize_numeric() {
        let p = ExitPolicy::default();
        assert_eq!(normalize_numeric_exit(-1, &p), EXIT_ERROR);
        assert_eq!(normalize_numeric_exit(0, &p), 0);
        assert_eq!(normalize_numeric_exit(12, &p), 12);
        assert_eq!(normalize_numeric_exit(9999, &p), 255);
    }
}
