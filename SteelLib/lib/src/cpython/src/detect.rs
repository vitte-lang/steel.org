//! Steel CPython – Detect
//!
//! Détection de l’environnement Python :
//! - interpréteur disponible
//! - version
//! - implémentation (CPython / PyPy)
//! - chemins utiles
//!
//! Utilisé par :
//! - driver.rs
//! - plan / validation toolchain

use std::path::PathBuf;
use std::process::Command;

use super::error::SteelError;

/// Implémentation Python détectée
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonImpl {
    CPython,
    PyPy,
    Unknown,
}

/// Informations sur l’interpréteur Python
#[derive(Debug, Clone)]
pub struct PythonInfo {
    /// Chemin vers l’exécutable (ex: python, python3, pypy3)
    pub path: PathBuf,

    /// Version complète (ex: "3.11.7")
    pub version: String,

    /// Implémentation détectée
    pub implementation: PythonImpl,

    /// Version majeure
    pub major: u8,

    /// Version mineure
    pub minor: u8,
}

/// Détection principale (python / python3 fallback)
pub fn detect_python() -> Result<PythonInfo, SteelError> {
    // Ordre volontaire : python3 → python
    let candidates = ["python3", "python"];

    for name in candidates {
        if let Ok(info) = detect_with(name) {
            return Ok(info);
        }
    }

    Err(SteelError::ValidationFailed(
        "no compatible python interpreter found in PATH".into(),
    ))
}

/// Détection avec un exécutable donné
fn detect_with<S: Into<String>>(exe: S) -> Result<PythonInfo, SteelError> {
    let exe = exe.into();

    // Script inline pour introspection fiable
    let script = r#"
import sys, platform
impl = platform.python_implementation()
ver = sys.version_info
print(f"{impl};{ver.major}.{ver.minor}.{ver.micro};{ver.major};{ver.minor}")
"#;

    let out = Command::new(&exe)
        .arg("-c")
        .arg(script)
        .output()
        .map_err(|_| {
            SteelError::ValidationFailed(format!(
                "failed to execute python interpreter: {exe}"
            ))
        })?;

    if !out.status.success() {
        return Err(SteelError::ValidationFailed(format!(
            "python interpreter {exe} returned error"
        )));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = stdout.trim().split(';').collect();

    if parts.len() != 4 {
        return Err(SteelError::ValidationFailed(
            "unexpected python detect output".into(),
        ));
    }

    let implementation = match parts[0] {
        "CPython" => PythonImpl::CPython,
        "PyPy" => PythonImpl::PyPy,
        _ => PythonImpl::Unknown,
    };

    let version = parts[1].to_string();
    let major: u8 = parts[2].parse().unwrap_or(0);
    let minor: u8 = parts[3].parse().unwrap_or(0);

    Ok(PythonInfo {
        path: PathBuf::from(exe),
        version,
        implementation,
        major,
        minor,
    })
}
