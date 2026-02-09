//! mod.rs 
//!
//! Module racine “mcfg” : exposition stable des sous-modules + façade d’API.
//!
//! Hypothèse layout:
//! - src/mcfg/src/mod.rs  (ce fichier)
//! - src/mcfg/src/lib.rs  (crate root) OU ce mod.rs est utilisé par un crate parent.
//!
//! Si ce dossier est un crate autonome, préférez `lib.rs` comme root et remplacez
//! ce `mod.rs` par des `pub mod ...` équivalents. Ici, on fournit un `mod.rs`
//! complet (export + préface + helpers), adapté à un workspace où `mcfg` est un module.

#![forbid(unsafe_code)]

pub mod diag;
pub mod emit;
pub mod hir;
pub mod ir;
pub mod lexer;
pub mod lower;
pub mod driver;

/// Version Bakefile attendue.
pub const MUFFIN_BAKEFILE_VERSION: u32 = 2;

/// Noms de buildfiles reconnus par défaut.
pub const DEFAULT_BUILD_FILES: [&str; 2] = ["steelconf", "build.muf"];

/// Extension “unit config”.
pub const MUFF_UNIT_EXT: &str = "muff";

/// Extension “global config”.
pub const MUFF_GLOBAL_EXT: &str = "mff";

/// ------------------------------------------------------------
/// Prelude (re-export ergonomique)
/// ------------------------------------------------------------

pub mod prelude {
    pub use crate::mcfg::diag::{DiagBag, Diagnostic, Severity, Span};
    pub use crate::mcfg::driver::{BuildCommand, Driver, DriverOptions, PlanSelector};
    pub use crate::mcfg::emit::{
        emit as emit_plan, EmitEvent, EmitOptions, EmitPlan, EmitResult, EmitStats, Emitter, RealFs,
        TextArtifact, WriteMode,
    };
    pub use crate::mcfg::hir::{
        ArtifactType, Bake, CacheMode, Capsule, EnvPolicy, FsPolicy, GlobalSet, Interner, MakeKind,
        NameId, NetPolicy, Origin, Plan, PlanItem, Port, PortDir, PrimType, Profile, Program, Ref,
        ResolvedRef, Store, StoreMode, Switch, SwitchAction, Tool, TypeRef, Value, VarDecl,
    };
    pub use crate::mcfg::ir::{
        Dag as IrDag, IrProgram, IrType, IrValue, CacheKey, hash_bytes_fnv1a64,
    };
    pub use crate::mcfg::lexer::{Lexer, Token, TokenKind, TokenStream};
    pub use crate::mcfg::lower::{lower, lower_default, LowerOptions};
}

/// ------------------------------------------------------------
/// Façade types (si mcfg est utilisé comme module, pas crate)
/// ------------------------------------------------------------

pub type FileId = u32;

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

#[derive(Debug, Clone)]
pub struct WorkspaceLayout {
    pub root: std::path::PathBuf,
    pub dot_dir: std::path::PathBuf,
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

/// ------------------------------------------------------------
/// Helpers
/// ------------------------------------------------------------

pub fn lex_source(file: &SourceFile, diags: &mut diag::DiagBag) -> Vec<lexer::Token> {
    lexer::Lexer::new(file.id, &file.text).lex_all(diags)
}