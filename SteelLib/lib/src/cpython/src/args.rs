//! Steel CPython – Args (MAX)
//!
//! Construction des lignes de commande Python utilisées par Steel.
//!
//! Couvre :
//! - compilation bytecode CPython (.pyc)
//! - build extensions natives (setuptools)
//! - bundle applicatif (zipapp / nuitka)
//!
//! Aucune exécution ici : arguments purs et déterministes.

use std::path::{Path, PathBuf};

/// Backend Python ciblé
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyBackend {
    /// CPython standard
    CPython,

    /// PyPy (si supporté)
    PyPy,

    /// Nuitka (compilation native)
    Nuitka,
}

/// Type d’action Python
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyAction {
    /// Compilation bytecode (.py → .pyc)
    CompileBytecode,

    /// Build extension native (C/C++)
    BuildExtension,

    /// Bundle applicatif (zipapp)
    BundleZipapp,

    /// Bundle natif (Nuitka)
    BundleNuitka,
}

/// Niveau d’optimisation Python
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyOptLevel {
    O0,
    O1,
    O2,
}

impl PyOptLevel {
    fn as_flag(self) -> &'static str {
        match self {
            PyOptLevel::O0 => "0",
            PyOptLevel::O1 => "1",
            PyOptLevel::O2 => "2",
        }
    }
}

/// Arguments Python génériques Steel
#[derive(Debug, Clone)]
pub struct PyArgs {
    /// Interpréteur (python, python3, pypy, etc.)
    pub python: PathBuf,

    /// Backend sélectionné
    pub backend: PyBackend,

    /// Action demandée
    pub action: PyAction,

    /// Fichiers sources
    pub sources: Vec<PathBuf>,

    /// Répertoire racine
    pub root: Option<PathBuf>,

    /// Point d’entrée (bundle)
    pub entry: Option<PathBuf>,

    /// Répertoire de sortie
    pub output: Option<PathBuf>,

    /// Niveau d’optimisation
    pub opt_level: PyOptLevel,

    /// Variables d’environnement injectées
    pub env: Vec<(String, String)>,

    /// Flags bruts additionnels
    pub extra: Vec<String>,
}

impl PyArgs {
    /* --------------------------------------------------------------------- */
    /* Constructeurs                                                         */
    /* --------------------------------------------------------------------- */

    /// Constructeur par défaut (CPython)
    pub fn new() -> Self {
        Self {
            python: PathBuf::from("python"),
            backend: PyBackend::CPython,
            action: PyAction::CompileBytecode,
            sources: Vec::new(),
            root: None,
            entry: None,
            output: None,
            opt_level: PyOptLevel::O0,
            env: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// Définit l’interpréteur explicitement
    pub fn with_python<P: Into<PathBuf>>(mut self, p: P) -> Self {
        self.python = p.into();
        self
    }

    /// Ajoute un fichier source
    pub fn add_source<P: Into<PathBuf>>(&mut self, p: P) {
        self.sources.push(p.into());
    }

    /// Définit la racine
    pub fn set_root<P: Into<PathBuf>>(&mut self, p: P) {
        self.root = Some(p.into());
    }

    /// Définit le point d’entrée
    pub fn set_entry<P: Into<PathBuf>>(&mut self, p: P) {
        self.entry = Some(p.into());
    }

    /// Définit le répertoire de sortie
    pub fn set_output<P: Into<PathBuf>>(&mut self, p: P) {
        self.output = Some(p.into());
    }

    /// Ajoute une variable d’environnement
    pub fn add_env<K, V>(&mut self, k: K, v: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.env.push((k.into(), v.into()));
    }

    /// Ajoute un flag brut
    pub fn add_flag<S: Into<String>>(&mut self, f: S) {
        self.extra.push(f.into());
    }

    /* --------------------------------------------------------------------- */
    /* Génération des arguments                                               */
    /* --------------------------------------------------------------------- */

    /// Génère la commande finale (argv)
    pub fn to_argv(&self) -> Vec<String> {
        match self.action {
            PyAction::CompileBytecode => self.argv_compile_bytecode(),
            PyAction::BuildExtension => self.argv_build_extension(),
            PyAction::BundleZipapp => self.argv_bundle_zipapp(),
            PyAction::BundleNuitka => self.argv_bundle_nuitka(),
        }
    }

    /// Arguments pour compilation bytecode
    fn argv_compile_bytecode(&self) -> Vec<String> {
        let mut args = vec![
            "-m".into(),
            "compileall".into(),
            "-q".into(),
            "-o".into(),
            self.opt_level.as_flag().into(),
        ];

        if let Some(root) = &self.root {
            args.push(root.display().to_string());
        } else {
            for src in &self.sources {
                args.push(src.display().to_string());
            }
        }

        args.extend(self.extra.iter().cloned());
        args
    }

    /// Arguments pour build extension native
    fn argv_build_extension(&self) -> Vec<String> {
        let mut args = Vec::new();

        args.push("setup.py".into());
        args.push("build_ext".into());
        args.push("--inplace".into());

        if let Some(out) = &self.output {
            args.push(format!("--build-lib={}", out.display()));
        }

        args.extend(self.extra.iter().cloned());
        args
    }

    /// Arguments pour bundle zipapp
    fn argv_bundle_zipapp(&self) -> Vec<String> {
        let mut args = vec![
            "-m".into(),
            "zipapp".into(),
        ];

        let root = self
            .root
            .as_ref()
            .expect("zipapp requires root");

        args.push(root.display().to_string());

        if let Some(entry) = &self.entry {
            args.push("-m".into());
            args.push(entry.display().to_string());
        }

        if let Some(out) = &self.output {
            args.push("-o".into());
            args.push(out.display().to_string());
        }

        args.extend(self.extra.iter().cloned());
        args
    }

    /// Arguments pour bundle Nuitka
    fn argv_bundle_nuitka(&self) -> Vec<String> {
        let mut args = vec![
            "-m".into(),
            "nuitka".into(),
            "--standalone".into(),
        ];

        if let Some(entry) = &self.entry {
            args.push(entry.display().to_string());
        }

        if let Some(out) = &self.output {
            args.push("--output-dir".into());
            args.push(out.display().to_string());
        }

        args.extend(self.extra.iter().cloned());
        args
    }

    /* --------------------------------------------------------------------- */
    /* Validation                                                            */
    /* --------------------------------------------------------------------- */

    /// Validation sémantique minimale
    pub fn validate(&self) -> Result<(), String> {
        match self.action {
            PyAction::CompileBytecode => {
                if self.sources.is_empty() && self.root.is_none() {
                    return Err("no sources or root provided for bytecode".into());
                }
            }
            PyAction::BuildExtension => {
                if self.root.is_none() {
                    return Err("build extension requires project root".into());
                }
            }
            PyAction::BundleZipapp | PyAction::BundleNuitka => {
                if self.entry.is_none() {
                    return Err("bundle requires entry point".into());
                }
            }
        }
        Ok(())
    }
}

/* ------------------------------------------------------------------------- */
/* Helpers                                                                   */
/* ------------------------------------------------------------------------- */

/// Détecte un fichier Python
pub fn is_python_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "py")
        .unwrap_or(false)
}