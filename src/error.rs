use std::fmt;

#[derive(Debug)]
pub enum SteelError {
    ConfigNotFound,
    ValidationFailed(String),
    CompilationFailed(String),
    IoError(std::io::Error),
}

impl fmt::Display for SteelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SteelError::ConfigNotFound => write!(f, "SteelConfig not found"),
            SteelError::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
            SteelError::CompilationFailed(msg) => write!(f, "Compilation failed: {}", msg),
            SteelError::IoError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<std::io::Error> for SteelError {
    fn from(err: std::io::Error) -> Self {
        SteelError::IoError(err)
    }
}

impl std::error::Error for SteelError {}
