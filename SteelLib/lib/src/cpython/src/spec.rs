//! Steel CPython – Spec (MAX)
//!
//! Spécification déclarative d’une unité de build Python.
//!
//! - aucune exécution
//! - aucune détection d’environnement
//! - aucune logique de graphe
//!
//! Utilisée par :
//! - graph (construction DAG)
//! - plan (ordonnancement)
//! - bake (PySpec -> PyArgs -> PythonDriver)

use std::path::PathBuf;

use super::args::{PyAction, PyBackend, PyOptLevel};

/// Type d’artefact Python attendu
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyOutputKind {
    /// Bytecode CPython (.pyc)
    Bytecode,

    /// Extension native Python (.so / .pyd)
    Extension,

    /// Bundle zipapp (.pyz)
    Zipapp,

    /// Bundle natif (Nuitka)
    NativeBundle,
}

/// Spécification complète d’un build Python
#[derive(Debug, Clone)]
pub struct PySpec {
    /* --------------------------------------------------------------------- */
    /* Identité logique                                                       */
    /* --------------------------------------------------------------------- */

    /// Nom logique de la cible
    pub name: String,

    /// Backend Python ciblé
    pub backend: PyBackend,

    /// Action Python à effectuer
    pub action: PyAction,

    /* --------------------------------------------------------------------- */
    /* Entrées                                                                */
    /* --------------------------------------------------------------------- */

    /// Fichiers sources Python (.py)
    pub sources: Vec<PathBuf>,

    /// Racine du projet (package / module)
    pub root: Option<PathBuf>,

    /// Point d’entrée (bundle)
    pub entry: Option<PathBuf>,

    /* --------------------------------------------------------------------- */
    /* Sorties                                                                */
    /* --------------------------------------------------------------------- */

    /// Type d’artefact attendu
    pub output_kind: PyOutputKind,

    /// Chemin de sortie (fichier ou dossier)
    pub output: Option<PathBuf>,

    /* --------------------------------------------------------------------- */
    /* Options                                                                */
    /* --------------------------------------------------------------------- */

    /// Niveau d’optimisation Python
    pub opt_level: PyOptLevel,

    /// Variables d’environnement injectées
    pub env: Vec<(String, String)>,

    /// Flags Python bruts additionnels
    pub extra_flags: Vec<String>,
}

impl PySpec {
    /* --------------------------------------------------------------------- */
    /* Constructeurs                                                          */
    /* --------------------------------------------------------------------- */

    /// Spécification minimale : bytecode CPython
    pub fn bytecode<N: Into<String>>(name: N) -> Self {
        Self {
            name: name.into(),
            backend: PyBackend::CPython,
            action: PyAction::CompileBytecode,
            sources: Vec::new(),
            root: None,
            entry: None,
            output_kind: PyOutputKind::Bytecode,
            output: None,
            opt_level: PyOptLevel::O0,
            env: Vec::new(),
            extra_flags: Vec::new(),
        }
    }

    /// Spécification extension native
    pub fn extension<N: Into<String>, P: Into<PathBuf>>(name: N, root: P) -> Self {
        Self {
            name: name.into(),
            backend: PyBackend::CPython,
            action: PyAction::BuildExtension,
            sources: Vec::new(),
            root: Some(root.into()),
            entry: None,
            output_kind: PyOutputKind::Extension,
            output: None,
            opt_level: PyOptLevel::O0,
            env: Vec::new(),
            extra_flags: Vec::new(),
        }
    }

    /// Spécification zipapp
    pub fn zipapp<N, P, E>(name: N, root: P, entry: E) -> Self
    where
        N: Into<String>,
        P: Into<PathBuf>,
        E: Into<PathBuf>,
    {
        Self {
            name: name.into(),
            backend: PyBackend::CPython,
            action: PyAction::BundleZipapp,
            sources: Vec::new(),
            root: Some(root.into()),
            entry: Some(entry.into()),
            output_kind: PyOutputKind::Zipapp,
            output: None,
            opt_level: PyOptLevel::O0,
            env: Vec::new(),
            extra_flags: Vec::new(),
        }
    }

    /// Spécification bundle natif Nuitka
    pub fn nuitka<N, E>(name: N, entry: E) -> Self
    where
        N: Into<String>,
        E: Into<PathBuf>,
    {
        Self {
            name: name.into(),
            backend: PyBackend::Nuitka,
            action: PyAction::BundleNuitka,
            sources: Vec::new(),
            root: None,
            entry: Some(entry.into()),
            output_kind: PyOutputKind::NativeBundle,
            output: None,
            opt_level: PyOptLevel::O0,
            env: Vec::new(),
            extra_flags: Vec::new(),
        }
    }

    /* --------------------------------------------------------------------- */
    /* Mutateurs                                                              */
    /* --------------------------------------------------------------------- */

    pub fn add_source<P: Into<PathBuf>>(&mut self, p: P) {
        self.sources.push(p.into());
    }

    pub fn set_output<P: Into<PathBuf>>(&mut self, p: P) {
        self.output = Some(p.into());
    }

    pub fn set_opt_level(&mut self, opt: PyOptLevel) {
        self.opt_level = opt;
    }

    pub fn add_env<K, V>(&mut self, k: K, v: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.env.push((k.into(), v.into()));
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
            return Err("empty python spec name".into());
        }

        match self.action {
            PyAction::CompileBytecode => {
                if self.sources.is_empty() && self.root.is_none() {
                    return Err("bytecode build requires sources or root".into());
                }
            }
            PyAction::BuildExtension => {
                if self.root.is_none() {
                    return Err("extension build requires project root".into());
                }
            }
            PyAction::BundleZipapp => {
                if self.root.is_none() || self.entry.is_none() {
                    return Err("zipapp requires root and entry".into());
                }
            }
            PyAction::BundleNuitka => {
                if self.entry.is_none() {
                    return Err("nuitka bundle requires entry".into());
                }
            }
        }

        Ok(())
    }
}