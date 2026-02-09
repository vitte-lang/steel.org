//! CPython backend integration for Steel.

pub mod args;
pub mod detect;
pub mod driver;
pub mod spec;

#[cfg(test)]
mod test;

pub mod error {
    pub use steellib::error::{SteelError, Result};
}

pub mod runner {
    pub mod process {
        use std::ffi::OsStr;
        use std::path::Path;
        use std::process::Command;

        use steellib::error::SteelError;

        #[derive(Debug, Default)]
        pub struct CommandRunner;

        impl CommandRunner {
            pub fn new() -> Self {
                Self
            }

            pub fn run(
                &self,
                program: &OsStr,
                argv: &[String],
                cwd: Option<&Path>,
            ) -> Result<(), SteelError> {
                let mut cmd = Command::new(program);
                cmd.args(argv);
                if let Some(dir) = cwd {
                    cmd.current_dir(dir);
                }

                let status = cmd.status().map_err(SteelError::Io)?;
                if status.success() {
                    Ok(())
                } else {
                    Err(SteelError::ExecutionFailed(format!(
                        "command failed: {} ({})",
                        program.to_string_lossy(),
                        status
                    )))
                }
            }

            pub fn run_with_env(
                &self,
                program: &OsStr,
                argv: &[String],
                env: &[(String, String)],
                cwd: Option<&Path>,
            ) -> Result<(), SteelError> {
                let mut cmd = Command::new(program);
                cmd.args(argv);
                for (k, v) in env {
                    cmd.env(k, v);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(dir);
                }

                let status = cmd.status().map_err(SteelError::Io)?;
                if status.success() {
                    Ok(())
                } else {
                    Err(SteelError::ExecutionFailed(format!(
                        "command failed: {} ({})",
                        program.to_string_lossy(),
                        status
                    )))
                }
            }
        }
    }
}

pub use args::{PyAction, PyArgs, PyBackend, PyOptLevel};
pub use detect::{detect_python, PythonImpl, PythonInfo};
pub use driver::PythonDriver;
pub use spec::{PyOutputKind, PySpec};
