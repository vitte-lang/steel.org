//! OCaml backend integration for Steel (in-tree module).

pub mod args;
pub mod detect;
pub mod driver;
pub mod spec;

#[cfg(test)]
mod test;

pub mod error {
    pub use crate::error::{SteelError, Result};
}

pub mod runner {
    pub mod process {
        use std::ffi::OsStr;
        use std::path::Path;
        use std::process::Command;

        use crate::error::SteelError;

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
        }
    }
}

pub use args::{OcamlArgs, OcamlBackend, OcamlOptLevel, OcamlOutputKind};
pub use detect::{detect_ocaml, OcamlInfo};
pub use driver::OcamlDriver;
pub use spec::OcamlSpec;
