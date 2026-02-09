//! Tool runner backend (runner.rs) — MAX (std-only).
//!
//! This module focuses on executing a prepared `ToolSpec` and returning a rich `RunResult`.
//! It complements `tool/mod.rs`:
//! - `tool/mod.rs` defines the public types (`ToolSpec`, `ToolRunner`, etc.)
//! - `runner.rs` can hold the detailed execution implementation, helpers,
//!   and platform-sensitive fallbacks.
//!
//! If you already implemented `ToolRunner::run` in `mod.rs`, you can:
//! - move that implementation here, and keep `mod.rs` as a thin facade, or
//! - keep `mod.rs` as-is and use this runner for advanced flows (streaming, logging).
//!
//! This file is std-only and sync. For async / PTY / process-tree killing, add feature backends.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::{ToolSpec, ToolStatus};

#[derive(Debug)]
pub enum RunnerError {
    Io(io::Error),
    Invalid(&'static str),
    Timeout { elapsed: Duration },
}

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunnerError::Io(e) => write!(f, "io: {e}"),
            RunnerError::Invalid(s) => write!(f, "invalid: {s}"),
            RunnerError::Timeout { elapsed } => write!(f, "timeout after {:?}", elapsed),
        }
    }
}

impl std::error::Error for RunnerError {}

impl From<io::Error> for RunnerError {
    fn from(e: io::Error) -> Self {
        RunnerError::Io(e)
    }
}

/// Output capturing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Capture stdout/stderr into memory.
    Capture,
    /// Inherit stdout/stderr from parent.
    Inherit,
    /// Discard output (null).
    Null,
}

/// Rich run options (runner-level).
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub timeout: Option<Duration>,
    pub stdout: Capture,
    pub stderr: Capture,
    pub stdin_inherit: bool,
    /// If true, include a minimal execution trace (argv/env/cwd).
    pub trace: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: None,
            stdout: Capture::Capture,
            stderr: Capture::Capture,
            stdin_inherit: false,
            trace: false,
        }
    }
}

/// Rich run result.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub status: ToolStatus,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
    pub trace: Option<RunTrace>,
}

impl RunResult {
    pub fn success(&self) -> bool {
        matches!(self.status, ToolStatus::Success { .. })
    }

    pub fn stdout_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    pub fn stderr_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Minimal trace for diagnostics (avoid secrets; env is filtered).
#[derive(Debug, Clone)]
pub struct RunTrace {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
}

/// Runner backend.
#[derive(Debug, Default)]
pub struct Runner;

impl Runner {
    pub fn new() -> Self {
        Self
    }

    /// Execute a ToolSpec with runner options.
    pub fn run(&self, spec: &ToolSpec, opt: RunOptions) -> Result<RunResult, RunnerError> {
        if spec.program.as_os_str().is_empty() {
            return Err(RunnerError::Invalid("empty program"));
        }

        let start = Instant::now();

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);

        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }

        // env overrides
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        // stdio
        match opt.stdout {
            Capture::Capture => cmd.stdout(Stdio::piped()),
            Capture::Inherit => cmd.stdout(Stdio::inherit()),
            Capture::Null => cmd.stdout(Stdio::null()),
        };
        match opt.stderr {
            Capture::Capture => cmd.stderr(Stdio::piped()),
            Capture::Inherit => cmd.stderr(Stdio::inherit()),
            Capture::Null => cmd.stderr(Stdio::null()),
        };
        if opt.stdin_inherit || spec.inherit_stdin {
            cmd.stdin(Stdio::inherit());
        } else {
            cmd.stdin(Stdio::null());
        }

        let trace = if opt.trace {
            Some(build_trace(spec))
        } else {
            None
        };

        let timeout = opt.timeout.or(spec.timeout);

        // If we capture outputs, prefer wait_with_output for simplicity.
        // If we inherit/null outputs, we can use wait/try_wait loop.
        let capture_mode = opt.stdout == Capture::Capture || opt.stderr == Capture::Capture;

        let mut child = cmd.spawn()?;

        if timeout.is_none() {
            if capture_mode {
                let out = child.wait_with_output()?;
                let dur = start.elapsed();
                let status = map_status(out.status);
                return Ok(RunResult {
                    status,
                    exit_code: out.status.code(),
                    stdout: out.stdout,
                    stderr: out.stderr,
                    duration: dur,
                    trace,
                });
            } else {
                let st = child.wait()?;
                let dur = start.elapsed();
                let status = map_status(st);
                return Ok(RunResult {
                    status,
                    exit_code: st.code(),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration: dur,
                    trace,
                });
            }
        }

        let timeout = timeout.unwrap();
        let poll = Duration::from_millis(10);

        loop {
            if start.elapsed() >= timeout {
                let _ = child.kill();

                if capture_mode {
                    // attempt capture after kill
                    let out = child.wait_with_output().unwrap_or_else(|_| std::process::Output {
                        status: fake_status_failure(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                    let dur = start.elapsed();
                    return Ok(RunResult {
                        status: ToolStatus::Timeout { elapsed: dur },
                        exit_code: out.status.code(),
                        stdout: out.stdout,
                        stderr: out.stderr,
                        duration: dur,
                        trace,
                    });
                } else {
                    let _ = child.wait();
                    let dur = start.elapsed();
                    return Ok(RunResult {
                        status: ToolStatus::Timeout { elapsed: dur },
                        exit_code: None,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        duration: dur,
                        trace,
                    });
                }
            }

            match child.try_wait()? {
                Some(st) => {
                    if capture_mode {
                        let out = child.wait_with_output()?;
                        let dur = start.elapsed();
                        return Ok(RunResult {
                            status: map_status(out.status),
                            exit_code: out.status.code(),
                            stdout: out.stdout,
                            stderr: out.stderr,
                            duration: dur,
                            trace,
                        });
                    } else {
                        let dur = start.elapsed();
                        return Ok(RunResult {
                            status: map_status(st),
                            exit_code: st.code(),
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                            duration: dur,
                            trace,
                        });
                    }
                }
                None => std::thread::sleep(poll),
            }
        }
    }

    /// Convenience: run with captured outputs and default options.
    pub fn run_capture(&self, spec: &ToolSpec) -> Result<RunResult, RunnerError> {
        self.run(spec, RunOptions::default())
    }

    /// Convenience: run with inherited output.
    pub fn run_inherit(&self, spec: &ToolSpec) -> Result<RunResult, RunnerError> {
        self.run(
            spec,
            RunOptions {
                stdout: Capture::Inherit,
                stderr: Capture::Inherit,
                ..RunOptions::default()
            },
        )
    }
}

/* ------------------------------ Utilities ------------------------------ */

fn map_status(st: ExitStatus) -> ToolStatus {
    if st.success() {
        ToolStatus::Success { status: st }
    } else {
        ToolStatus::Failed { status: st }
    }
}

fn build_trace(spec: &ToolSpec) -> RunTrace {
    let mut env_map = BTreeMap::new();
    // Filter: keep only a small allowlist by default.
    for k in ["PATH", "HOME", "USERPROFILE", "SHELL", "ComSpec", "TMP", "TEMP", "TMPDIR"] {
        if let Ok(v) = std::env::var(k) {
            env_map.insert(k.to_string(), v);
        }
    }

    // Include overrides (but as lossy strings)
    for (k, v) in &spec.env {
        env_map.insert(k.to_string_lossy().to_string(), v.to_string_lossy().to_string());
    }

    RunTrace {
        program: spec.program.clone(),
        args: spec.args.iter().map(|a| a.to_string_lossy().to_string()).collect(),
        cwd: spec.cwd.clone(),
        env: env_map,
    }
}

fn fake_status_failure() -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(1 << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(1)
    }
    #[cfg(not(any(unix, windows)))]
    {
        panic!("unsupported platform for fake_status_failure");
    }
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolSpec;

    #[test]
    fn trace_builds() {
        let spec = ToolSpec::new("echo").arg("hello");
        let t = build_trace(&spec);
        assert!(!t.args.is_empty());
    }

    #[test]
    fn runner_constructs() {
        let _r = Runner::new();
    }
}
