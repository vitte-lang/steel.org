//! SteelLib error types and Result alias.

use std::fmt;

/// Structured diagnostic data for CLI rendering.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub help: Vec<String>,
}

/// Error type for the SteelLib facade crate.
#[derive(Debug)]
pub enum SteelError {
    /// Generic error message.
    Message(String),
    /// User-facing validation failure.
    ValidationFailed(String),
    /// Execution failure (tools/processes/sandbox).
    ExecutionFailed(String),
    /// CLI usage/command error (option parsing, unknown command).
    InvalidCommand { message: String, help: Vec<String> },
    /// Not found or missing resource.
    NotFound(String),
    /// Internal logic error.
    Internal(String),
    /// IO errors.
    Io(std::io::Error),
}

impl fmt::Display for SteelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SteelError::Message(msg) => write!(f, "{msg}"),
            SteelError::ValidationFailed(msg) => write!(f, "{msg}"),
            SteelError::ExecutionFailed(msg) => write!(f, "{msg}"),
            SteelError::InvalidCommand { message, .. } => write!(f, "{message}"),
            SteelError::NotFound(msg) => write!(f, "{msg}"),
            SteelError::Internal(msg) => write!(f, "{msg}"),
            SteelError::Io(err) => write!(f, "io error: {err}"),
        }
    }
}

impl std::error::Error for SteelError {}

impl From<std::io::Error> for SteelError {
    fn from(err: std::io::Error) -> Self {
        SteelError::Io(err)
    }
}

impl SteelError {
    /// Build a structured diagnostic for rendering in the CLI.
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            SteelError::Message(msg) => Diagnostic {
                code: "E0001",
                message: msg.clone(),
                help: Vec::new(),
            },
            SteelError::ValidationFailed(msg) => Diagnostic {
                code: "E0100",
                message: msg.clone(),
                help: Vec::new(),
            },
            SteelError::ExecutionFailed(msg) => Diagnostic {
                code: "E0200",
                message: msg.clone(),
                help: Vec::new(),
            },
            SteelError::InvalidCommand { message, help } => Diagnostic {
                code: "E0300",
                message: message.clone(),
                help: help.clone(),
            },
            SteelError::NotFound(msg) => Diagnostic {
                code: "E0404",
                message: msg.clone(),
                help: Vec::new(),
            },
            SteelError::Internal(msg) => Diagnostic {
                code: "E0500",
                message: msg.clone(),
                help: Vec::new(),
            },
            SteelError::Io(err) => Diagnostic {
                code: "E0002",
                message: format!("io error: {err}"),
                help: Vec::new(),
            },
        }
    }

    /// Exit code mapping for CLI consumers.
    pub fn exit_code(&self) -> u8 {
        match self {
            SteelError::InvalidCommand { .. } | SteelError::ValidationFailed(_) => 2,
            SteelError::ExecutionFailed(_) => 3,
            SteelError::Io(_) | SteelError::NotFound(_) => 4,
            SteelError::Internal(_) => 5,
            SteelError::Message(_) => 1,
        }
    }

    /// Render a CLI-friendly error string with codes and optional help.
    pub fn render_cli(&self, prefix: &str) -> String {
        let diag = self.diagnostic();
        let mut s = String::new();
        s.push_str(prefix);
        s.push_str(": error[");
        s.push_str(diag.code);
        s.push_str("]: ");
        s.push_str(&diag.message);
        if !diag.help.is_empty() {
            for h in &diag.help {
                s.push_str("\nhelp: ");
                s.push_str(h);
            }
        }
        s
    }
}

/// Convenience Result alias for SteelLib consumers.
pub type Result<T> = std::result::Result<T, SteelError>;
