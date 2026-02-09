//! Tool abstraction for Steel (mod.rs) — MAX (std-only).
//!
//! This module defines:
//! - `ToolSpec`: declarative description of a tool (exe + args + env + cwd)
//! - `ToolRunner`: execution with stdout/stderr capture, timeouts (best-effort std-only)
//! - `ToolError` + `ToolStatus`
//! - `ToolOutput`: captured outputs
//!
//! Design goals:
//! - deterministic command lines (stable args ordering when built from maps)
//! - portable across Windows/macOS/Linux/BSD
//! - allow Steel "capsule" policy layers to constrain tool execution (wired elsewhere)
//!
//! Notes:
//! - std-only: no async, no kill-timeout on all platforms (best-effort).
//! - For strict timeouts/process trees, add feature-gated platform backends.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ToolError {
    Io(std::io::Error),
    Invalid(&'static str),
    Spawn(String),
    Timeout { elapsed: Duration },
    NonZeroExit { code: Option<i32> },
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Io(e) => write!(f, "io: {e}"),
            ToolError::Invalid(s) => write!(f, "invalid: {s}"),
            ToolError::Spawn(s) => write!(f, "spawn: {s}"),
            ToolError::Timeout { elapsed } => write!(f, "timeout after {:?}", elapsed),
            ToolError::NonZeroExit { code } => write!(f, "non-zero exit: {:?}", code),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::Io(e)
    }
}

/// Tool execution status.
#[derive(Debug, Clone)]
pub enum ToolStatus {
    Success { status: ExitStatus },
    Failed { status: ExitStatus },
    Timeout { elapsed: Duration },
}

/// Captured output.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub status: ToolStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
}

impl ToolOutput {
    pub fn stdout_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
    pub fn stderr_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Declarative tool specification.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<OsString, OsString>,
    /// If set, max duration to wait. Best-effort in std-only mode.
    pub timeout: Option<Duration>,
    /// If true, inherit stdin (interactive tools).
    pub inherit_stdin: bool,
}

impl ToolSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            timeout: None,
            inherit_stdin: false,
        }
    }

    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }

    pub fn env(mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> Self {
        self.env.insert(k.into(), v.into());
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    pub fn inherit_stdin(mut self, yes: bool) -> Self {
        self.inherit_stdin = yes;
        self
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn argv_lossy(&self) -> Vec<String> {
        let mut v = Vec::new();
        v.push(self.program.to_string_lossy().to_string());
        for a in &self.args {
            v.push(a.to_string_lossy().to_string());
        }
        v
    }
}

/// Tool runner (sync, std-only).
#[derive(Debug, Default)]
pub struct ToolRunner;

impl ToolRunner {
    pub fn new() -> Self {
        Self
    }

    /// Execute tool and capture output.
    pub fn run(&self, spec: &ToolSpec) -> Result<ToolOutput, ToolError> {
        if spec.program.as_os_str().is_empty() {
            return Err(ToolError::Invalid("empty program"));
        }

        let start = Instant::now();

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);

        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }

        // env: start from inherited env; then override.
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        if spec.inherit_stdin {
            cmd.stdin(Stdio::inherit());
        } else {
            cmd.stdin(Stdio::null());
        }

        // std-only timeout: run via `wait_with_output` and poll by elapsed.
        // We implement a best-effort loop: spawn child, then periodically try_wait.
        let mut child = cmd.spawn().map_err(ToolError::Io)?;

        if spec.timeout.is_none() {
            let out = child.wait_with_output()?;
            let dur = start.elapsed();
            let status = if out.status.success() {
                ToolStatus::Success { status: out.status }
            } else {
                ToolStatus::Failed { status: out.status }
            };
            return Ok(ToolOutput {
                status,
                stdout: out.stdout,
                stderr: out.stderr,
                duration: dur,
            });
        }

        let timeout = spec.timeout.unwrap();
        let poll = Duration::from_millis(10);

        // We cannot capture stdout/stderr incrementally without extra threads.
        // So we do a fallback:
        // - If it exits before timeout, use wait_with_output.
        // - If not, try to kill, then collect whatever wait_with_output returns.
        loop {
            if start.elapsed() >= timeout {
                // timeout
                let _ = child.kill();
                // Try to wait and capture output after kill
                let out = child.wait_with_output().unwrap_or_else(|_| {
                    // Minimal fallback
                    std::process::Output {
                        status: fake_status_failure(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    }
                });

                let dur = start.elapsed();
                return Ok(ToolOutput {
                    status: ToolStatus::Timeout { elapsed: dur },
                    stdout: out.stdout,
                    stderr: out.stderr,
                    duration: dur,
                });
            }

            match child.try_wait()? {
                Some(_status) => {
                    // child exited; now capture outputs
                    let out = child.wait_with_output()?;
                    let dur = start.elapsed();
                    let status = if out.status.success() {
                        ToolStatus::Success { status: out.status }
                    } else {
                        ToolStatus::Failed { status: out.status }
                    };
                    return Ok(ToolOutput {
                        status,
                        stdout: out.stdout,
                        stderr: out.stderr,
                        duration: dur,
                    });
                }
                None => {
                    std::thread::sleep(poll);
                }
            }
        }
    }

    /// Execute tool and error on non-zero exit (convenience).
    pub fn run_checked(&self, spec: &ToolSpec) -> Result<ToolOutput, ToolError> {
        let out = self.run(spec)?;
        match &out.status {
            ToolStatus::Success { .. } => Ok(out),
            ToolStatus::Failed { status } => Err(ToolError::NonZeroExit { code: status.code() }),
            ToolStatus::Timeout { elapsed } => Err(ToolError::Timeout { elapsed: *elapsed }),
        }
    }
}

/* ------------------------------ Helpers ------------------------------ */

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
        // unreachable in practice, but keep compile.
        // There's no stable way; return a successful status? Prefer failure-ish.
        // We'll just spawn a "true" status using std::process::Command is not acceptable here.
        // So: panic in tests would reveal.
        panic!("unsupported platform for fake_status_failure");
    }
}

/* -------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn argv_lossy_includes_program() {
        let spec = ToolSpec::new("echo").arg("hello");
        let v = spec.argv_lossy();
        assert!(!v.is_empty());
        assert_eq!(v[0], "echo");
    }

    #[test]
    fn env_set_roundtrip() {
        let spec = ToolSpec::new("echo").env("A", "B");
        assert_eq!(spec.env.get(OsStr::new("A")), Some(&OsString::from("B")));
    }
}
