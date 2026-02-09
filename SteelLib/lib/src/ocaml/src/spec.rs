//! Steel OCaml – Spec (MAX)
//!
//! Spécification déclarative d’une unité de build OCaml.
//!
//! - aucune exécution
//! - aucune détection toolchain
//! - aucune logique de graphe
//!
//! Utilisée par :
//! - graph (construction du DAG)
//! - plan (ordonnancement)
//! - bake (Spec → Args → Driver)

use std::path::PathBuf;

use super::args::{OcamlBackend, OcamlOptLevel, OcamlOutputKind};

/// Spécification complète d’un build OCaml
#[derive(Debug, Clone)]
pub struct OcamlSpec {
    /* --------------------------------------------------------------------- */
    /* Identité logique                                                       */
    /* --------------------------------------------------------------------- */

    /// Nom logique de la cible
    pub name: String,

    /* --------------------------------------------------------------------- */
    /* Backend / sortie                                                       */
    /* --------------------------------------------------------------------- */

    /// Backend OCaml utilisé (ocamlc / ocamlopt)
    pub backend: OcamlBackend,

    /// Type de sortie
    pub output_kind: OcamlOutputKind,

    /// Chemin de sortie final
    pub output: PathBuf,

    /* --------------------------------------------------------------------- */
    /* Entrées                                                                */
    /* --------------------------------------------------------------------- */

    /// Fichiers sources (.ml / .mli)
    pub sources: Vec<PathBuf>,

    /// Répertoires d’include (-I)
    pub include_dirs: Vec<PathBuf>,

    /// Librairies à lier
    pub libraries: Vec<PathBuf>,

    /* --------------------------------------------------------------------- */
    /* Options                                                                */
    /* --------------------------------------------------------------------- */

    /// Niveau d’optimisation (natif)
    pub opt_level: OcamlOptLevel,

    /// Flags OCaml bruts additionnels
    pub extra_flags: Vec<String>,
}

impl OcamlSpec {
    /* --------------------------------------------------------------------- */
    /* Constructeurs                                                          */
    /* --------------------------------------------------------------------- */

    /// Spécification minimale : exécutable natif
    pub fn executable<N, P>(name: N, output: P) -> Self
    where
        N: Into<String>,
        P: Into<PathBuf>,
    {
        Self {
            name: name.into(),
            backend: OcamlBackend::Ocamlopt,
            output_kind: OcamlOutputKind::Executable,
            output: output.into(),
            sources: Vec::new(),
            include_dirs: Vec::new(),
            libraries: Vec::new(),
            opt_level: OcamlOptLevel::O0,
            extra_flags: Vec::new(),
        }
    }

    /// Spécification librairie OCaml
    pub fn library<N, P>(name: N, output: P) -> Self
    where
        N: Into<String>,
        P: Into<PathBuf>,
    {
        Self {
            name: name.into(),
            backend: OcamlBackend::Ocamlopt,
            output_kind: OcamlOutputKind::Library,
            output: output.into(),
            sources: Vec::new(),
            include_dirs: Vec::new(),
            libraries: Vec::new(),
            opt_level: OcamlOptLevel::O0,
            extra_flags: Vec::new(),
        }
    }

    /// Spécification bytecode (ocamlc)
    pub fn bytecode<N, P>(name: N, output: P) -> Self
    where
        N: Into<String>,
        P: Into<PathBuf>,
    {
        Self {
            name: name.into(),
            backend: OcamlBackend::Ocamlc,
            output_kind: OcamlOutputKind::Executable,
            output: output.into(),
            sources: Vec::new(),
            include_dirs: Vec::new(),
            libraries: Vec::new(),
            opt_level: OcamlOptLevel::O0,
            extra_flags: Vec::new(),
        }
    }

    /* --------------------------------------------------------------------- */
    /* Mutateurs                                                              */
    /* --------------------------------------------------------------------- */

    pub fn add_source<P: Into<PathBuf>>(&mut self, p: P) {
        self.sources.push(p.into());
    }

    pub fn add_include_dir<P: Into<PathBuf>>(&mut self, p: P) {
        self.include_dirs.push(p.into());
    }

    pub fn add_library<P: Into<PathBuf>>(&mut self, p: P) {
        self.libraries.push(p.into());
    }

    pub fn set_opt_level(&mut self, opt: OcamlOptLevel) {
        self.opt_level = opt;
    }

    pub fn add_flag<S: Into<String>>(&mut self, f: S) {
        self.extra_flags.push(f.into());
    }

    /* --------------------------------------------------------------------- */
    /* Validation                                                             */
    /* --------------------------------------------------------------------- */

    /// Validation sémantique stricte
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("empty ocaml spec name".into());
        }

        if self.sources.is_empty() {
            return Err("no OCaml sources specified".into());
        }

        Ok(())
    }
}