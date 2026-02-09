// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\tool\src\mod_.rs
//! Core tool types (renamed file) — MAX.
//!
//! If you want `src/mod.rs` instead, rename this file to `mod.rs`
//! and update `lib.rs` accordingly.

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
    Timeout { elapsed: Duration },
    NonZeroExit { code: Option<i32> },
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Io(e) => write!(f, "io: {e}"),
            ToolError::Invalid(s) => write!(f, "invalid: {s}"),
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

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Success { status: ExitStatus },
    Failed { status: ExitStatus },
    Timeout { elapsed: Duration },
}

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

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<OsString, OsString>,
    pub timeout: Option<Duration>,
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
}

#[derive(Debug, Default)]
pub struct ToolRunner;

impl ToolRunner {
    pub fn new() -> Self {
        Self
    }

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

        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        if spec.inherit_stdin {
            cmd.stdin(Stdio::inherit());
        } else {
            cmd.stdin(Stdio::null());
        }

        let mut child = cmd.spawn()?;

        if spec.timeout.is_none() {
            let out = child.wait_with_output()?;
            let dur = start.elapsed();
            return Ok(ToolOutput {
                status: if out.status.success() {
                    ToolStatus::Success { status: out.status }
                } else {
                    ToolStatus::Failed { status: out.status }
                },
                stdout: out.stdout,
                stderr: out.stderr,
                duration: dur,
            });
        }

        let timeout = spec.timeout.unwrap();
        let poll = Duration::from_millis(10);

        loop {
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let out = child.wait_with_output().unwrap_or_else(|_| std::process::Output {
                    status: fake_status_failure(),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
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
                Some(_st) => {
                    let out = child.wait_with_output()?;
                    let dur = start.elapsed();
                    return Ok(ToolOutput {
                        status: if out.status.success() {
                            ToolStatus::Success { status: out.status }
                        } else {
                            ToolStatus::Failed { status: out.status }
                        },
                        stdout: out.stdout,
                        stderr: out.stderr,
                        duration: dur,
                    });
                }
                None => std::thread::sleep(poll),
            }
        }
    }

    pub fn run_checked(&self, spec: &ToolSpec) -> Result<ToolOutput, ToolError> {
        let out = self.run(spec)?;
        match &out.status {
            ToolStatus::Success { .. } => Ok(out),
            ToolStatus::Failed { status } => Err(ToolError::NonZeroExit { code: status.code() }),
            ToolStatus::Timeout { elapsed } => Err(ToolError::Timeout { elapsed: *elapsed }),
        }
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
