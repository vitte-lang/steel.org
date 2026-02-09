//! Steel OCaml – Detect (MAX)
//!
//! Détection de l’environnement OCaml :
//! - présence de ocamlc / ocamlopt
//! - versions installées
//! - capacités (bytecode / natif)
//! - outils auxiliaires (ocamlfind)
//!
//! Utilisé par :
//! - driver.rs
//! - validation toolchain
//! - planification Steel

use std::path::PathBuf;
use std::process::Command;

use super::error::SteelError;

/// Informations sur l’environnement OCaml
#[derive(Debug, Clone)]
pub struct OcamlInfo {
    /// Chemin vers ocamlc (bytecode)
    pub ocamlc: Option<PathBuf>,

    /// Chemin vers ocamlopt (natif)
    pub ocamlopt: Option<PathBuf>,

    /// Chemin vers ocamlfind (optionnel)
    pub ocamlfind: Option<PathBuf>,

    /// Version OCaml (ex: "5.1.1")
    pub version: String,

    /// Support bytecode
    pub has_bytecode: bool,

    /// Support compilation native
    pub has_native: bool,
}

/// Détection principale OCaml
pub fn detect_ocaml() -> Result<OcamlInfo, SteelError> {
    let ocamlc = which("ocamlc");
    let ocamlopt = which("ocamlopt");
    let ocamlfind = which("ocamlfind");

    if ocamlc.is_none() && ocamlopt.is_none() {
        return Err(SteelError::ValidationFailed(
            "no OCaml compiler found (ocamlc / ocamlopt missing)".into(),
        ));
    }

    // Version : priorité à ocamlc, sinon ocamlopt
    let version = if let Some(ref p) = ocamlc {
        ocaml_version(p)?
    } else if let Some(ref p) = ocamlopt {
        ocaml_version(p)?
    } else {
        unreachable!()
    };

    Ok(OcamlInfo {
        ocamlc: ocamlc.clone(),
        ocamlopt: ocamlopt.clone(),
        ocamlfind,
        version,
        has_bytecode: ocamlc.is_some(),
        has_native: ocamlopt.is_some(),
    })
}

/* ------------------------------------------------------------------------- */
/* Helpers                                                                   */
/* ------------------------------------------------------------------------- */

/// Résout un exécutable via le PATH (best-effort)
fn which(name: &str) -> Option<PathBuf> {
    let out = Command::new(name)
        .arg("-version")
        .output();

    match out {
        Ok(o) if o.status.success() => Some(PathBuf::from(name)),
        _ => None,
    }
}

/// Récupère la version OCaml via `-version`
fn ocaml_version(exe: &PathBuf) -> Result<String, SteelError> {
    let out = Command::new(exe)
        .arg("-version")
        .output()
        .map_err(|e| {
            SteelError::ValidationFailed(format!(
                "failed to execute {:?}: {e}",
                exe
            ))
        })?;

    if !out.status.success() {
        return Err(SteelError::ValidationFailed(
            "unable to query OCaml version".into(),
        ));
    }

    let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if ver.is_empty() {
        return Err(SteelError::ValidationFailed(
            "OCaml returned empty version".into(),
        ));
    }

    Ok(ver)
}
