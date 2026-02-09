// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\runner\process.rs

//! Process execution layer for the runner.
//!
//! Responsibilities:
//! - spawn tools deterministically
//! - capture stdout/stderr
//! - provide structured result (exit code, output, duration)
//! - support env/working dir
//!
//! This is used by `runner::Runner` to execute Tool nodes.

use crate::error::SteelError;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

/// Result of a tool execution.
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn code(&self) -> Option<i32> {
        self.status.code()
    }
}

/// Specification to run a process.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    /// Program path or name (resolved by PATH).
    pub program: String,

    /// Arguments.
    pub args: Vec<String>,

    /// Current working directory.
    pub cwd: Option<PathBuf>,

    /// Environment variables (merged with parent env).
    pub env: Vec<(String, String)>,

    /// If true, inherit stdout/stderr instead of capturing.
    pub inherit_stdio: bool,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            inherit_stdio: false,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for s in it {
            self.args.push(s.into());
        }
        self
    }

    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }

    pub fn inherit_stdio(mut self, yes: bool) -> Self {
        self.inherit_stdio = yes;
        self
    }
}

/// Run a process and capture output.
pub fn run_process(spec: &ProcessSpec) -> Result<ProcessOutput, SteelError> {
    // Resolve program is delegated to Command/OS PATH.
    let mut cmd = Command::new(&spec.program);

    cmd.args(&spec.args);

    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }

    for (k, v) in &spec.env {
        cmd.env(k, v);
    }

    if spec.inherit_stdio {
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    }

    let started = Instant::now();
    let out = cmd.output().map_err(|e| {
        SteelError::ExecutionFailed(format!(
            "failed to spawn tool '{}': {}",
            spec.program, e
        ))
    })?;
    let duration = started.elapsed();

    Ok(ProcessOutput {
        status: out.status,
        stdout: out.stdout,
        stderr: out.stderr,
        duration,
    })
}

/// Validate a process exit status and map to SteelError with context.
pub fn ensure_success(spec: &ProcessSpec, out: &ProcessOutput) -> Result<(), SteelError> {
    if out.success() {
        return Ok(());
    }

    let code = out.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());

    let stderr = if out.stderr.is_empty() {
        "<empty>".to_string()
    } else {
        // lossy but practical for diagnostics
        String::from_utf8_lossy(&out.stderr).to_string()
    };

    Err(SteelError::ExecutionFailed(format!(
        "tool failed: program='{}' code={} stderr={}",
        spec.program, code, sanitize_one_line(&stderr)
    )))
}

fn sanitize_one_line(s: &str) -> String {
    s.replace('\r', " ").replace('\n', " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_process_works_for_trivial_command() {
        // Cross-platform-ish: `rustc --version` should exist in this repo context.
        let spec = ProcessSpec::new("rustc").arg("--version");
        let out = run_process(&spec).unwrap();
        assert!(out.status.success());
        assert!(!out.stdout.is_empty());
    }
}
