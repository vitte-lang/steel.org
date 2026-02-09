//! source.rs 
//!
//! Abstraction “source input” pour Steel/MCFG.
//!
//! Objectifs :
//! - Représenter de manière uniforme : fichiers, globs, texte inline, valeurs, artefacts.
//! - Permettre à Steel de “matérialiser” une liste exhaustive de fichiers à reconstruire.
//! - Supporter le mode multi-répertoire (un `.muff` par répertoire) + index `.mff` racine.
//! - Hashing / fingerprint stable pour cache invalidation.
//!
//! Dépend de :
//! - crate::diag::*
//! - crate::schema::*
//! - (optionnel) crate::directory / crate::expand si présents
//!
//! Note :
//! - Ce module ne fait pas de lecture disque directe “obligatoire” (peut être branché ailleurs),
//!   mais fournit les structures et helpers + interfaces “provider”.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::diag::{DiagBag, Diagnostic};
use crate::schema::{ArtifactId, NormalPath, TargetTriple};

/// Identifiant stable d’une source (pour debug/why/graph)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(pub String);

/// Type de source
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    /// Un fichier exact : `path`
    File,
    /// Un glob : `pattern` (ex: "src/**/*.vit")
    Glob,
    /// Un texte inline (config, script, etc.)
    Text,
    /// Une valeur scalaire (string/int/bool) — ex: variable CLI -D
    Value,
    /// Un artefact produit par une autre unité
    Artifact,
}

/// Sens de la source dans le pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRole {
    /// Input direct d’une compilation
    Input,
    /// Input de “linking” / packaging
    Dependency,
    /// Source d’export (entrypoint / deliverable)
    Export,
    /// Meta/diagnostic/trace
    Meta,
}

/// Valeur scalaire
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scalar {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl Scalar {
    pub fn as_str(&self) -> Option<&str> {
        if let Scalar::Str(s) = self { Some(s) } else { None }
    }
}

/// Spécification de glob (portable)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobSpec {
    /// pattern posix (slash `/`)
    pub pattern: String,
    /// inclure fichiers cachés
    pub include_hidden: bool,
    /// follow symlinks (policy)
    pub follow_symlinks: bool,
}

impl GlobSpec {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self { pattern: pattern.into(), include_hidden: false, follow_symlinks: false }
    }
}

/// SourceSpec : représentation uniforme d’une source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpec {
    pub id: SourceId,
    pub kind: SourceKind,
    pub role: SourceRole,

    /// Chemin principal (si File) — posix string
    pub path: Option<NormalPath>,

    /// Glob (si Glob)
    pub glob: Option<GlobSpec>,

    /// Texte inline (si Text)
    pub text: Option<String>,

    /// Valeur (si Value)
    pub value: Option<Scalar>,

    /// Artefact (si Artifact)
    pub artifact: Option<ArtifactRef>,

    /// Tags / métadonnées (debug, profile, etc.)
    pub tags: BTreeSet<String>,
    pub meta: BTreeMap<String, String>,
}

impl SourceSpec {
    pub fn file(id: impl Into<String>, role: SourceRole, path: NormalPath) -> Self {
        Self {
            id: SourceId(id.into()),
            kind: SourceKind::File,
            role,
            path: Some(path),
            glob: None,
            text: None,
            value: None,
            artifact: None,
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn glob(id: impl Into<String>, role: SourceRole, spec: GlobSpec) -> Self {
        Self {
            id: SourceId(id.into()),
            kind: SourceKind::Glob,
            role,
            path: None,
            glob: Some(spec),
            text: None,
            value: None,
            artifact: None,
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn text(id: impl Into<String>, role: SourceRole, text: impl Into<String>) -> Self {
        Self {
            id: SourceId(id.into()),
            kind: SourceKind::Text,
            role,
            path: None,
            glob: None,
            text: Some(text.into()),
            value: None,
            artifact: None,
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn value(id: impl Into<String>, role: SourceRole, v: Scalar) -> Self {
        Self {
            id: SourceId(id.into()),
            kind: SourceKind::Value,
            role,
            path: None,
            glob: None,
            text: None,
            value: Some(v),
            artifact: None,
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn artifact(id: impl Into<String>, role: SourceRole, a: ArtifactRef) -> Self {
        Self {
            id: SourceId(id.into()),
            kind: SourceKind::Artifact,
            role,
            path: None,
            glob: None,
            text: None,
            value: None,
            artifact: Some(a),
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }
}

/// Référence artefact (cross-unit)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactRef {
    /// unit id (ex: "src/in/foo")
    pub unit: String,
    /// artifact id (ex: "lib", "obj", "exe")
    pub artifact: ArtifactId,
    /// target (optionnel) si multi-target
    pub target: Option<TargetTriple>,
}

impl ArtifactRef {
    pub fn new(unit: impl Into<String>, artifact: impl Into<String>) -> Self {
        Self { unit: unit.into(), artifact: ArtifactId(artifact.into()), target: None }
    }
}

/// Matérialisation : fichier concret sélectionné
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedFile {
    pub source: SourceId,
    pub path: NormalPath,
}

/// Résultat d’expansion d’une SourceSpec
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedSource {
    pub source: SourceId,
    pub files: Vec<MaterializedFile>,
    pub notes: Vec<String>,
}

/// Provider pour l’expansion (abstraction système de fichiers / workspace)
pub trait SourceProvider {
    /// Résout un glob -> liste de fichiers (NormalPath posix)
    fn expand_glob(&self, spec: &GlobSpec, diags: &mut DiagBag) -> Vec<NormalPath>;

    /// Normalise un chemin (si besoin)
    fn normalize_path(&self, p: &Path) -> NormalPath;

    /// Check existence (best-effort)
    fn exists(&self, p: &NormalPath) -> bool;

    /// Read file bytes (optionnel)
    fn read_bytes(&self, p: &NormalPath) -> std::io::Result<Vec<u8>>;
}

/// ------------------------------------------------------------
/// Expansion
/// ------------------------------------------------------------

pub fn materialize_sources(
    provider: &dyn SourceProvider,
    sources: &[SourceSpec],
    diags: &mut DiagBag,
) -> Vec<MaterializedSource> {
    let mut out = Vec::new();

    for s in sources {
        let mut ms = MaterializedSource { source: s.id.clone(), files: Vec::new(), notes: Vec::new() };

        match s.kind {
            SourceKind::File => {
                if let Some(p) = &s.path {
                    if !provider.exists(p) {
                        diags.push(Diagnostic::warning(format!("source `{}`: file missing: {}", s.id.0, p.as_posix())));
                    }
                    ms.files.push(MaterializedFile { source: s.id.clone(), path: p.clone() });
                } else {
                    diags.push(Diagnostic::error(format!("source `{}`: kind=file but path is none", s.id.0)));
                }
            }
            SourceKind::Glob => {
                if let Some(g) = &s.glob {
                    let files = provider.expand_glob(g, diags);
                    if files.is_empty() {
                        diags.push(Diagnostic::warning(format!("source `{}`: glob matched nothing: {}", s.id.0, g.pattern)));
                    }
                    for p in files {
                        ms.files.push(MaterializedFile { source: s.id.clone(), path: p });
                    }
                } else {
                    diags.push(Diagnostic::error(format!("source `{}`: kind=glob but glob is none", s.id.0)));
                }
            }
            SourceKind::Text => {
                // Pas de fichiers : le “texte” est une source logique
                if s.text.is_none() {
                    diags.push(Diagnostic::error(format!("source `{}`: kind=text but text is none", s.id.0)));
                }
                ms.notes.push("text source".into());
            }
            SourceKind::Value => {
                if s.value.is_none() {
                    diags.push(Diagnostic::error(format!("source `{}`: kind=value but value is none", s.id.0)));
                }
                ms.notes.push("scalar source".into());
            }
            SourceKind::Artifact => {
                if s.artifact.is_none() {
                    diags.push(Diagnostic::error(format!("source `{}`: kind=artifact but artifact is none", s.id.0)));
                }
                ms.notes.push("artifact source".into());
            }
        }

        out.push(ms);
    }

    out
}

/// ------------------------------------------------------------
/// Fingerprinting (cache invalidation)
/// ------------------------------------------------------------

/// Fingerprint minimal : stable hash input.
/// (Sans crypto : FNV-1a 64 bits) — suffisant pour cache local.
/// Si besoin, brancher un hash cryptographique ailleurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(pub u64);

pub fn fingerprint_sources(
    provider: &dyn SourceProvider,
    sources: &[SourceSpec],
    diags: &mut DiagBag,
) -> Fingerprint {
    let mats = materialize_sources(provider, sources, diags);

    let mut h: u64 = 0xcbf29ce484222325; // FNV offset
    for ms in mats {
        h = fnv64(h, ms.source.0.as_bytes());

        for f in ms.files {
            h = fnv64(h, f.path.as_posix().as_bytes());

            // include bytes if readable (optional) — expensive; keep conservative:
            // we include only path by default; callers can add more (mtime/content) elsewhere.
            let _ = f;
        }
    }

    Fingerprint(h)
}

fn fnv64(mut h: u64, bytes: &[u8]) -> u64 {
    const P: u64 = 0x00000100000001B3;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(P);
    }
    h
}

/// ------------------------------------------------------------
/// Builders helpers: convert schema UnitConfig -> SourceSpec list
/// ------------------------------------------------------------

pub fn unit_sources_from_config(unit: &UnitConfig) -> Vec<SourceSpec> {
    let mut out = Vec::new();

    // sources_vit => File sources
    for (i, p) in unit.sources_vit.iter().enumerate() {
        let mut s = SourceSpec::file(format!("vit:{}", i), SourceRole::Input, p.clone());
        s.tags.insert("vit".into());
        out.push(s);
    }

    for (i, p) in unit.extra_inputs.iter().enumerate() {
        let mut s = SourceSpec::file(format!("extra:{}", i), SourceRole::Dependency, p.clone());
        s.tags.insert("extra".into());
        out.push(s);
    }

    // deps units => logical artifact source markers (resolved elsewhere)
    for dep in &unit.deps_units {
        let mut s = SourceSpec::value(format!("dep:{}", dep.0), SourceRole::Dependency, Scalar::Str(dep.0.clone()));
        s.tags.insert("unit-dep".into());
        out.push(s);
    }

    // compiler key/values => Value sources (affect fingerprint)
    for (k, v) in &unit.compiler {
        let mut s = SourceSpec::value(format!("cfg:{}", k), SourceRole::Meta, Scalar::Str(v.clone()));
        s.tags.insert("compiler".into());
        s.meta.insert("key".into(), k.clone());
        out.push(s);
    }

    // features => Value sources
    for f in &unit.features {
        let mut s = SourceSpec::value(format!("feat:{}", f), SourceRole::Meta, Scalar::Bool(true));
        s.tags.insert("feature".into());
        out.push(s);
    }

    out
}

/// ------------------------------------------------------------
/// Minimal provider impl (filesystem) — optionnel
/// ------------------------------------------------------------
///
/// Si ton projet a déjà `directory.rs` / `expand.rs`, tu peux remplacer cette impl
/// par celles du projet. Ici : impl conservative (no glob engine interne).
///
/// Cette impl ne résout pas réellement les globs sans moteur. Elle renvoie vide.
/// Le moteur réel peut être branché via crate::expand.

pub struct FsProvider {
    pub workspace_root: std::path::PathBuf,
}

impl FsProvider {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self { workspace_root: workspace_root.into() }
    }
}

impl SourceProvider for FsProvider {
    fn expand_glob(&self, spec: &GlobSpec, diags: &mut DiagBag) -> Vec<NormalPath> {
        // Placeholder : brancher sur crate::expand si dispo.
        diags.push(Diagnostic::warning(format!(
            "glob expansion not implemented in FsProvider (pattern: {})",
            spec.pattern
        )));
        Vec::new()
    }

    fn normalize_path(&self, p: &Path) -> NormalPath {
        NormalPath::from_path(&self.workspace_root, p)
    }

    fn exists(&self, p: &NormalPath) -> bool {
        let native = if let Some(n) = &p.native {
            std::path::PathBuf::from(n)
        } else {
            // posix -> platform join
            let mut pb = self.workspace_root.clone();
            for seg in p.posix.split('/') {
                if seg.is_empty() {
                    continue;
                }
                pb.push(seg);
            }
            pb
        };
        native.exists()
    }

    fn read_bytes(&self, p: &NormalPath) -> std::io::Result<Vec<u8>> {
        let native = if let Some(n) = &p.native {
            std::path::PathBuf::from(n)
        } else {
            let mut pb = self.workspace_root.clone();
            for seg in p.posix.split('/') {
                if seg.is_empty() {
                    continue;
                }
                pb.push(seg);
            }
            pb
        };
        std::fs::read(native)
    }
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{UnitConfig, UnitId};

    #[test]
    fn fingerprint_is_stable_for_paths() {
        let mut diags = DiagBag::new();
        let provider = FsProvider::new(".");

        let mut u = UnitConfig::new(
            "src/in/a",
            NormalPath { posix: ".".into(), native: None },
            NormalPath { posix: "src/in/a".into(), native: None },
        );
        u.sources_vit.push(NormalPath { posix: "src/program/lib.vit".into(), native: None });
        u.compiler.insert("opt".into(), "3".into());
        u.deps_units.push(UnitId("src/in/b".into()));

        let s = unit_sources_from_config(&u);
        let fp1 = fingerprint_sources(&provider, &s, &mut diags);
        let fp2 = fingerprint_sources(&provider, &s, &mut diags);

        assert_eq!(fp1, fp2);
    }

    #[test]
    fn materialize_glob_warns_if_unimplemented() {
        let mut diags = DiagBag::new();
        let provider = FsProvider::new(".");
        let srcs = vec![SourceSpec::glob("g", SourceRole::Input, GlobSpec::new("src/**/*.vit"))];
        let _ = materialize_sources(&provider, &srcs, &mut diags);
        assert!(diags.has_warning());
    }
}