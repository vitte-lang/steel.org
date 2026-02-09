//! mcfg — Steel Config Generator (Vitte toolchain)
//!
//! Rôle du crate :
//! - ingérer un buildfile Steel (Bakefile v2 : `steelconf` / `build.muf`)
//! - produire des artefacts de build (`.mff` global + unités `*.muff` par répertoire)
//! - fournir des primitives stables (diag, lexer, HIR/IR, emission) pour brancher le
//!   reste du pipeline (parser/resolver/lower/driver).
//!
//! Design goals :
//! - déterminisme (BTree*, ordering stable, newlines normalized)
//! - diagnostics exploitables (Span + messages)
//! - surface API claire (emit plan / emitter)
//!
//! Ce crate est volontairement “toolchain-friendly” : l’API expose des briques
//! plutôt qu’un monolithe opaque.

#![forbid(unsafe_code)]
#![warn(
    rust_2018_idioms,
    unused_must_use,
    dead_code,
    unused_imports,
    clippy::all,
    clippy::pedantic
)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

pub mod diag;
pub mod decompile;
pub mod emit;
pub mod hir;
pub mod ir;
pub mod lexer;
pub mod span;
pub mod token;

/// Version Bakefile attendue par défaut.
pub const MUFFIN_BAKEFILE_VERSION: u32 = 2;

/// Noms de buildfiles reconnus par défaut.
pub const DEFAULT_BUILD_FILES: [&str; 2] = ["steelconf", "build.muf"];

/// Extension “unit config” (ex: `src/in/folder/_.muff`).
pub const MUFF_UNIT_EXT: &str = "muff";

/// Extension “global config” (ex: `.mff`).
pub const MUFF_GLOBAL_EXT: &str = "mff";

/// ------------------------------------------------------------
/// Prelude
/// ------------------------------------------------------------

pub mod prelude {
    pub use crate::diag::{DiagBag, Diagnostic, Severity, Span};
    pub use crate::emit::{
        emit as emit_plan, EmitEvent, EmitOptions, EmitPlan, EmitResult, EmitStats, Emitter, RealFs,
        TextArtifact, WriteMode,
    };
    pub use crate::hir::{
        ArtifactType, Bake, CacheMode, Capsule, EnvPolicy, FsPolicy, GlobalSet, Interner, MakeKind,
        NameId, NetPolicy, Origin, Plan, PlanItem, Port, PortDir, PrimType, Profile, Program, Ref,
        ResolvedRef, Store, StoreMode, Switch, SwitchAction, Tool, TypeRef, Value, VarDecl,
    };
    pub use crate::ir::{lower_hir_to_ir, Dag as IrDag, IrProgram};
    pub use crate::token::{Token, TokenKind, TokenStream};
}

/// ------------------------------------------------------------
/// High-level public types (façade)
/// ------------------------------------------------------------

/// Identifiant logique d’un fichier source (pour Span/diag).
pub type FileId = u32;

/// Représentation d’un fichier chargé (chemin + contenu).
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: std::path::PathBuf,
    pub text: String,
}

impl SourceFile {
    pub fn new(id: FileId, path: impl Into<std::path::PathBuf>, text: impl Into<String>) -> Self {
        Self { id, path: path.into(), text: text.into() }
    }
}

/// Layout “workspace” attendu par défaut.
/// (C’est un contrat d’outil, pas une contrainte de langage.)
#[derive(Debug, Clone)]
pub struct WorkspaceLayout {
    /// Répertoire racine du projet.
    pub root: std::path::PathBuf,
    /// Répertoire de sorties (cache, manifests…).
    pub dot_dir: std::path::PathBuf,
    /// Sous-dossier pour manifests/out.
    pub out_dir: std::path::PathBuf,
}

impl WorkspaceLayout {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        let root = root.into();
        let dot_dir = root.join(".steel");
        let out_dir = dot_dir.join("out");
        Self { root, dot_dir, out_dir }
    }
}

/// Options de compilation “pipeline” (si vous branchez parser+resolver+lowering).
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Version attendue dans le header `steel bake <N>`.
    pub expected_version: u32,
    /// Profil par défaut (si applicable côté resolver).
    pub default_profile: Option<String>,
    /// Plan par défaut (si applicable côté driver).
    pub default_plan: Option<String>,
    /// Emission options (FS policy).
    pub emit: emit::EmitOptions,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            expected_version: MUFFIN_BAKEFILE_VERSION,
            default_profile: None,
            default_plan: None,
            emit: emit::EmitOptions::default(),
        }
    }
}

/// Résultat “compile” : diagnostics + éventuels outputs.
#[derive(Debug, Default)]
pub struct CompileResult {
    pub diags: diag::DiagBag,
    pub emit: Option<emit::EmitResult>,
}

/// ------------------------------------------------------------
/// Minimal helpers (sans parser/resolver complet)
/// ------------------------------------------------------------

/// Lex uniquement (utile pour debug/outils).
pub fn lex_source(file: &SourceFile, _diags: &mut diag::DiagBag) -> token::TokenStream {
    token::lex(span::FileId(file.id), &file.text)
}

/// Émission directe d’un plan d’artefacts (couche “output” pure).
pub fn emit(plan: emit::EmitPlan, opts: emit::EmitOptions, diags: &mut diag::DiagBag) -> emit::EmitResult {
    emit::Emitter::new(emit::RealFs, opts).emit(plan, diags)
}

/// ------------------------------------------------------------
/// Pipeline façade (stub volontaire)
/// ------------------------------------------------------------
///
/// Ce crate expose les briques. Le pipeline complet dépend de modules qui ne sont
/// pas forcément “dans le même commit” (parser, resolve, lowering d’artefacts).
///
/// En pratique, vous brancherez un `driver` qui fait :
/// - load steelconf
/// - lex/parse AST
/// - resolve -> HIR
/// - lower -> IR (dag/cache plan)
/// - build EmitPlan
/// - emit
///
/// Ici, on fournit un point d’entrée “placeholder” pour figer l’API.

/// Point d’entrée pipeline (stub).
///
/// Actuellement :
/// - vérifie le header via lexer (best-effort)
/// - renvoie un diagnostic “pipeline non branché”
///
/// À remplacer par l’intégration parser+hir+ir+emit quand les modules sont présents.
pub fn compile_stub(_layout: &WorkspaceLayout, file: &SourceFile, opts: &CompileOptions) -> CompileResult {
    let mut diags = diag::DiagBag::new();

    // Best-effort: repérer `steel bake <int>` via lexer (sans parser complet).
    let toks = lex_source(file, &mut diags);
    let mut saw_header = false;

    // Pattern: KwFlan KwBake Int
    for w in toks.tokens.windows(3) {
        if w[0].kind == token::TokenKind::KwFlan
            && w[1].kind == token::TokenKind::KwBake
            && w[2].kind == token::TokenKind::IntLit
        {
            saw_header = true;
            if let Some(txt) = &w[2].text {
                if let Ok(v) = txt.parse::<u32>() {
                    if v != opts.expected_version {
                        let span = diag::Span::new(
                            w[2].span.file.0,
                            w[2].span.lo.0,
                            w[2].span.hi.0,
                        );
                        diags.push(
                            diag::err_at(
                                span,
                                format!(
                                    "unsupported Steel Bakefile version: got {}, expected {}",
                                    v, opts.expected_version
                                ),
                            ),
                        );
                    }
                }
            }
            break;
        }
    }

    if !saw_header {
        diags.push(diag::Diagnostic::error("missing header: `steel bake <int>`"));
    }

    diags.push(diag::Diagnostic::error(
        "pipeline not connected: parser/resolver/lowering not wired in this build",
    ));

    CompileResult { diags, emit: None }
}
