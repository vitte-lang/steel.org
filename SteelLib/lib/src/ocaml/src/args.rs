//! Steel OCaml – Args (MAX)
//!
//! Construction des lignes de commande OCaml (ocamlc / ocamlopt)
//! utilisées par Steel.
//!
//! Couvre :
//! - bytecode (.byte)
//! - natif (.exe)
//! - librairies (.cma / .cmxa)
//!
//! Aucune exécution ici : arguments purs et déterministes.

use std::path::{Path, PathBuf};

/// Backend OCaml
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcamlBackend {
    /// Compilateur bytecode
    Ocamlc,

    /// Compilateur natif
    Ocamlopt,
}

/// Type de sortie OCaml
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcamlOutputKind {
    /// Exécutable
    Executable,

    /// Librairie
    Library,
}

/// Niveau d’optimisation (natif)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcamlOptLevel {
    O0,
    O2,
}

impl OcamlOptLevel {
    fn as_flags(self) -> &'static [&'static str] {
        match self {
            OcamlOptLevel::O0 => &[],
            OcamlOptLevel::O2 => &["-O2"],
        }
    }
}

/// Arguments OCaml génériques Steel
#[derive(Debug, Clone)]
pub struct OcamlArgs {
    /// Backend utilisé
    pub backend: OcamlBackend,

    /// Type de sortie
    pub output_kind: OcamlOutputKind,

    /// Fichiers sources (.ml / .mli)
    pub sources: Vec<PathBuf>,

    /// Répertoires d’include (-I)
    pub include_dirs: Vec<PathBuf>,

    /// Librairies à lier (-cma / -cmxa)
    pub libraries: Vec<PathBuf>,

    /// Répertoire de sortie
    pub output: PathBuf,

    /// Niveau d’optimisation (natif)
    pub opt_level: OcamlOptLevel,

    /// Flags OCaml bruts
    pub extra_flags: Vec<String>,
}

impl OcamlArgs {
    /* --------------------------------------------------------------------- */
    /* Constructeurs                                                         */
    /* --------------------------------------------------------------------- */

    /// Constructeur minimal (exécutable natif)
    pub fn executable<P: Into<PathBuf>>(output: P) -> Self {
        Self {
            backend: OcamlBackend::Ocamlopt,
            output_kind: OcamlOutputKind::Executable,
            sources: Vec::new(),
            include_dirs: Vec::new(),
            libraries: Vec::new(),
            output: output.into(),
            opt_level: OcamlOptLevel::O0,
            extra_flags: Vec::new(),
        }
    }

    /// Constructeur librairie
    pub fn library<P: Into<PathBuf>>(output: P) -> Self {
        Self {
            backend: OcamlBackend::Ocamlopt,
            output_kind: OcamlOutputKind::Library,
            sources: Vec::new(),
            include_dirs: Vec::new(),
            libraries: Vec::new(),
            output: output.into(),
            opt_level: OcamlOptLevel::O0,
            extra_flags: Vec::new(),
        }
    }

    /* --------------------------------------------------------------------- */
    /* Mutateurs                                                             */
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
    /* Génération des arguments                                               */
    /* --------------------------------------------------------------------- */

    /// Exécutable à invoquer (`ocamlc` ou `ocamlopt`)
    pub fn compiler(&self) -> &'static str {
        match self.backend {
            OcamlBackend::Ocamlc => "ocamlc",
            OcamlBackend::Ocamlopt => "ocamlopt",
        }
    }

    /// Génère argv final
    pub fn to_argv(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Includes
        for dir in &self.include_dirs {
            args.push("-I".into());
            args.push(dir.display().to_string());
        }

        // Optimisation (natif)
        if self.backend == OcamlBackend::Ocamlopt {
            for f in self.opt_level.as_flags() {
                args.push((*f).into());
            }
        }

        // Sources
        for src in &self.sources {
            args.push(src.display().to_string());
        }

        // Librairies
        for lib in &self.libraries {
            args.push(lib.display().to_string());
        }

        // Type de sortie
        match self.output_kind {
            OcamlOutputKind::Executable => {
                args.push("-o".into());
                args.push(self.output.display().to_string());
            }
            OcamlOutputKind::Library => {
                args.push("-a".into());
                args.push("-o".into());
                args.push(self.output.display().to_string());
            }
        }

        // Flags additionnels
        args.extend(self.extra_flags.iter().cloned());

        args
    }

    /* --------------------------------------------------------------------- */
    /* Validation                                                            */
    /* --------------------------------------------------------------------- */

    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no OCaml sources provided".into());
        }

        Ok(())
    }
}

/* ------------------------------------------------------------------------- */
/* Helpers                                                                   */
/* ------------------------------------------------------------------------- */

pub fn is_ocaml_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "ml" || e == "mli")
        .unwrap_or(false)
}