//! schema.rs 
//!
//! Schéma “MCFG” (Steel Config) : représentation sérialisable des unités `.mff` / `.muff`.
//!
//! Objectif : fournir un format de configuration *stable* entre le générateur Steel,
//! le compilateur Vitte, et l’outillage (runner/Vitte driver).
//!
//! Design :
//! - structure explicite et versionnée
//! - champs ordonnés/déterministes (BTreeMap)
//! - compatibilité multi-OS (Windows/macOS/Linux/BSD/Solaris)
//! - chemins normalisés (posix) mais conservant aussi la forme native si besoin
//! - intégration avec le “build graph” : unités par répertoire, outputs, dépendances,
//!   artefacts (.vo, .va, .exe, etc.)
//!
//! Contraste :
//! - `hir.rs` représente le buildfile Steel (déclaratif build DAG).
//! - `schema.rs` représente les fichiers générés `.mff/.muff` (configs compilateur).
//!
//! NOTE : sérialisation std-only : on fournit une impl “writer” et “reader”
//! minimaliste basée sur un format texte type “INI/kv + sections” (sans dépendances).
//! Si serde est disponible dans le projet, on pourra brancher serde derrière un feature.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::diag::{DiagBag, Diagnostic};

/// Version du schéma MCFG (format .mff/.muff)
pub const MCFG_SCHEMA_VERSION: u32 = 1;

/// ------------------------------------------------------------
/// OS / Target model
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostOs {
    Windows,
    MacOs,
    Linux,
    FreeBsd,
    OpenBsd,
    NetBsd,
    DragonFlyBsd,
    Solaris,
    Illumos,
    Haiku,
    Unknown,
}

impl HostOs {
    pub fn detect() -> Self {
        // compile-time detection
        #[cfg(target_os = "windows")]
        return HostOs::Windows;
        #[cfg(target_os = "macos")]
        return HostOs::MacOs;
        #[cfg(target_os = "linux")]
        return HostOs::Linux;
        #[cfg(target_os = "freebsd")]
        return HostOs::FreeBsd;
        #[cfg(target_os = "openbsd")]
        return HostOs::OpenBsd;
        #[cfg(target_os = "netbsd")]
        return HostOs::NetBsd;
        #[cfg(target_os = "dragonfly")]
        return HostOs::DragonFlyBsd;
        #[cfg(target_os = "solaris")]
        return HostOs::Solaris;

        HostOs::Unknown
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HostOs::Windows => "windows",
            HostOs::MacOs => "macos",
            HostOs::Linux => "linux",
            HostOs::FreeBsd => "freebsd",
            HostOs::OpenBsd => "openbsd",
            HostOs::NetBsd => "netbsd",
            HostOs::DragonFlyBsd => "dragonflybsd",
            HostOs::Solaris => "solaris",
            HostOs::Illumos => "illumos",
            HostOs::Haiku => "haiku",
            HostOs::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "windows" | "win" => HostOs::Windows,
            "macos" | "osx" | "darwin" => HostOs::MacOs,
            "linux" => HostOs::Linux,
            "freebsd" => HostOs::FreeBsd,
            "openbsd" => HostOs::OpenBsd,
            "netbsd" => HostOs::NetBsd,
            "dragonfly" | "dragonflybsd" => HostOs::DragonFlyBsd,
            "solaris" => HostOs::Solaris,
            "illumos" => HostOs::Illumos,
            "haiku" => HostOs::Haiku,
            _ => HostOs::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTriple {
    /// ex: x86_64-apple-darwin
    pub triple: String,
}

impl TargetTriple {
    pub fn new(triple: impl Into<String>) -> Self {
        Self { triple: triple.into() }
    }

    pub fn is_windows(&self) -> bool {
        self.triple.contains("windows") || self.triple.contains("mingw") || self.triple.contains("msvc")
    }
}

/// ------------------------------------------------------------
/// Artifact model (.vo / .va / .exe / etc.)
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactKind {
    /// compilation unit output (object-like)
    Vo,
    /// static library archive
    Va,
    /// executable
    Exe,
    /// dynamic library (optional)
    Vd,
    /// generic file
    File,
}

impl ArtifactKind {
    pub fn as_ext(&self) -> &'static str {
        match self {
            ArtifactKind::Vo => "vo",
            ArtifactKind::Va => "va",
            ArtifactKind::Exe => "exe",
            ArtifactKind::Vd => "vd",
            ArtifactKind::File => "",
        }
    }

    pub fn parse_ext(ext: &str) -> Option<Self> {
        match ext {
            "vo" => Some(ArtifactKind::Vo),
            "va" => Some(ArtifactKind::Va),
            "exe" => Some(ArtifactKind::Exe),
            "vd" => Some(ArtifactKind::Vd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub path: NormalPath,
    pub tags: BTreeSet<String>,         // ex: ["debug","profile:release","lto"]
    pub meta: BTreeMap<String, String>, // ex: {"hash":"...","size":"..."}
}

impl Artifact {
    pub fn new(id: impl Into<String>, kind: ArtifactKind, path: NormalPath) -> Self {
        Self { id: ArtifactId(id.into()), kind, path, tags: BTreeSet::new(), meta: BTreeMap::new() }
    }
}

/// ------------------------------------------------------------
/// Path normalization
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalPath {
    /// Path normalisé (séparateur `/`), relatif si possible.
    pub posix: String,
    /// Path original (optionnel), utile sur Windows.
    pub native: Option<String>,
}

impl NormalPath {
    pub fn from_path(root: &Path, p: &Path) -> Self {
        let rel = p.strip_prefix(root).unwrap_or(p);
        let posix = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");

        let native = Some(p.to_string_lossy().to_string());
        Self { posix, native }
    }

    pub fn join_posix(&self, seg: &str) -> NormalPath {
        let mut out = self.posix.clone();
        if !out.is_empty() && !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(seg.trim_start_matches('/'));
        NormalPath { posix: out, native: None }
    }

    pub fn as_posix(&self) -> &str {
        &self.posix
    }
}

/// ------------------------------------------------------------
/// Unit (.muff) : config par répertoire
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitConfig {
    pub schema_version: u32,

    /// ident stable : ex "src/in/folder_vitte_folder"
    pub unit: UnitId,

    /// racine du workspace (normalisé)
    pub workspace_root: NormalPath,

    /// répertoire source de cette unité
    pub unit_dir: NormalPath,

    /// toolchain / target
    pub host: HostOs,
    pub target: TargetTriple,

    /// Entrées
    pub sources_vit: Vec<NormalPath>, // ex: Src/program/lib.vit, error.vit, read.vit
    pub extra_inputs: Vec<NormalPath>, // fichiers non-.vit requis (assets, headers, etc.)

    /// Dépendances entre unités (imports / liens)
    pub deps_units: Vec<UnitId>,

    /// Sorties (artefacts produits par cette unité)
    pub outputs: Vec<Artifact>,

    /// Paramètres compilateur (key/value)
    pub compiler: BTreeMap<String, String>,

    /// Flags / features (set)
    pub features: BTreeSet<String>,

    /// Optionnel : mapping “alias -> artifact id”
    pub exports: BTreeMap<String, ArtifactId>,

    /// Métadonnées
    pub meta: BTreeMap<String, String>,
}

impl UnitConfig {
    pub fn new(unit: impl Into<String>, workspace_root: NormalPath, unit_dir: NormalPath) -> Self {
        Self {
            schema_version: MCFG_SCHEMA_VERSION,
            unit: UnitId(unit.into()),
            workspace_root,
            unit_dir,
            host: HostOs::Unknown,
            target: TargetTriple::new("unknown"),
            sources_vit: Vec::new(),
            extra_inputs: Vec::new(),
            deps_units: Vec::new(),
            outputs: Vec::new(),
            compiler: BTreeMap::new(),
            features: BTreeSet::new(),
            exports: BTreeMap::new(),
            meta: BTreeMap::new(),
        }
    }
}

/// ------------------------------------------------------------
/// Root (.mff) : index global
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootConfig {
    pub schema_version: u32,

    /// workspace root
    pub workspace_root: NormalPath,

    /// default unit / entry (anchor)
    pub entry_unit: Option<UnitId>,

    /// units index: id -> path to unit .muff
    pub units: BTreeMap<UnitId, NormalPath>,

    /// global compiler defaults
    pub compiler_defaults: BTreeMap<String, String>,

    /// global build settings
    pub build: BTreeMap<String, String>,

    /// meta
    pub meta: BTreeMap<String, String>,
}

impl RootConfig {
    pub fn new(workspace_root: NormalPath) -> Self {
        Self {
            schema_version: MCFG_SCHEMA_VERSION,
            workspace_root,
            entry_unit: None,
            units: BTreeMap::new(),
            compiler_defaults: BTreeMap::new(),
            build: BTreeMap::new(),
            meta: BTreeMap::new(),
        }
    }
}

/// ------------------------------------------------------------
/// Validation
/// ------------------------------------------------------------

pub fn validate_root(root: &RootConfig, diags: &mut DiagBag) -> bool {
    let mut ok = true;

    if root.schema_version == 0 {
        diags.push(Diagnostic::error("root config: schema_version is 0"));
        ok = false;
    }

    if root.units.is_empty() {
        diags.push(Diagnostic::warning("root config: no units declared"));
    }

    if let Some(entry) = &root.entry_unit {
        if !root.units.contains_key(entry) {
            diags.push(Diagnostic::error(format!("root config: entry_unit `{}` not found in units index", entry.0)));
            ok = false;
        }
    }

    ok
}

pub fn validate_unit(unit: &UnitConfig, diags: &mut DiagBag) -> bool {
    let mut ok = true;

    if unit.schema_version == 0 {
        diags.push(Diagnostic::error("unit config: schema_version is 0"));
        ok = false;
    }

    if unit.sources_vit.is_empty() {
        diags.push(Diagnostic::warning(format!("unit `{}`: no vit sources declared", unit.unit.0)));
    }

    // outputs sanity: ids must be unique
    let mut ids = BTreeSet::new();
    for a in &unit.outputs {
        if !ids.insert(a.id.0.clone()) {
            diags.push(Diagnostic::error(format!("unit `{}`: duplicate artifact id `{}`", unit.unit.0, a.id.0)));
            ok = false;
        }
    }

    ok
}

/// ------------------------------------------------------------
/// Text format (std-only) — writer
/// ------------------------------------------------------------
///
/// Format :
///
////  mcfg.schema = 1
////  root.workspace = "..."
////  root.entry = "unit_id"
////
////  [units]
////  "unit_id" = "path/to/unit.muff"
////
////  [compiler_defaults]
////  "opt" = "3"
///
/// Unit:
////  mcfg.schema = 1
////  unit.id = "src/in/..."
////  unit.dir = "src/in/..."
////  host.os = "linux"
////  target.triple = "x86_64-unknown-linux-gnu"
////
////  [sources]
////  vit = ["src/program/lib.vit", ...]
////  extra = ["assets/x", ...]
////
////  [deps]
////  units = ["u1","u2"]
////
////  [outputs]
////  "lib" = { kind="va", path="src/out/lib/..va" }
///
/// Délibérément simple (pas de JSON/serde).
///

pub fn write_root_text(root: &RootConfig) -> String {
    let mut out = String::new();

    push_kv(&mut out, "mcfg.schema", &root.schema_version.to_string());
    push_kv_str(&mut out, "root.workspace", root.workspace_root.as_posix());
    if let Some(entry) = &root.entry_unit {
        push_kv_str(&mut out, "root.entry", &entry.0);
    }

    out.push('\n');
    out.push_str("[units]\n");
    for (k, v) in &root.units {
        out.push('"');
        out.push_str(&escape(&k.0));
        out.push_str("\" = \"");
        out.push_str(&escape(v.as_posix()));
        out.push_str("\"\n");
    }

    if !root.compiler_defaults.is_empty() {
        out.push('\n');
        out.push_str("[compiler_defaults]\n");
        for (k, v) in &root.compiler_defaults {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(v));
            out.push_str("\"\n");
        }
    }

    if !root.build.is_empty() {
        out.push('\n');
        out.push_str("[build]\n");
        for (k, v) in &root.build {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(v));
            out.push_str("\"\n");
        }
    }

    if !root.meta.is_empty() {
        out.push('\n');
        out.push_str("[meta]\n");
        for (k, v) in &root.meta {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(v));
            out.push_str("\"\n");
        }
    }

    out
}

pub fn write_unit_text(unit: &UnitConfig) -> String {
    let mut out = String::new();

    push_kv(&mut out, "mcfg.schema", &unit.schema_version.to_string());
    push_kv_str(&mut out, "unit.id", &unit.unit.0);
    push_kv_str(&mut out, "workspace.root", unit.workspace_root.as_posix());
    push_kv_str(&mut out, "unit.dir", unit.unit_dir.as_posix());
    push_kv_str(&mut out, "host.os", unit.host.as_str());
    push_kv_str(&mut out, "target.triple", &unit.target.triple);

    out.push('\n');
    out.push_str("[sources]\n");
    out.push_str("vit = ");
    out.push_str(&fmt_list_str(unit.sources_vit.iter().map(|p| p.as_posix())));
    out.push('\n');
    out.push_str("extra = ");
    out.push_str(&fmt_list_str(unit.extra_inputs.iter().map(|p| p.as_posix())));
    out.push('\n');

    out.push('\n');
    out.push_str("[deps]\n");
    out.push_str("units = ");
    out.push_str(&fmt_list_str(unit.deps_units.iter().map(|u| u.0.as_str())));
    out.push('\n');

    if !unit.outputs.is_empty() {
        out.push('\n');
        out.push_str("[outputs]\n");
        for a in &unit.outputs {
            out.push('"');
            out.push_str(&escape(&a.id.0));
            out.push_str("\" = { kind=\"");
            out.push_str(a.kind.as_ext());
            out.push_str("\", path=\"");
            out.push_str(&escape(a.path.as_posix()));
            out.push_str("\" }\n");
        }
    }

    if !unit.compiler.is_empty() {
        out.push('\n');
        out.push_str("[compiler]\n");
        for (k, v) in &unit.compiler {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(v));
            out.push_str("\"\n");
        }
    }

    if !unit.features.is_empty() {
        out.push('\n');
        out.push_str("[features]\n");
        out.push_str("set = ");
        out.push_str(&fmt_list_str(unit.features.iter().map(|s| s.as_str())));
        out.push('\n');
    }

    if !unit.exports.is_empty() {
        out.push('\n');
        out.push_str("[exports]\n");
        for (k, v) in &unit.exports {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(&v.0));
            out.push_str("\"\n");
        }
    }

    if !unit.meta.is_empty() {
        out.push('\n');
        out.push_str("[meta]\n");
        for (k, v) in &unit.meta {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\" = \"");
            out.push_str(&escape(v));
            out.push_str("\"\n");
        }
    }

    out
}

/// ------------------------------------------------------------
/// Text format (std-only) — reader (minimal)
/// ------------------------------------------------------------
///
/// Reader minimaliste :
//! - parse seulement les clés racine indispensables
//! - sections simples [name]
/// - string quoted `"..."`
/// - listes `["a","b"]`
///
/// Cette partie est volontairement conservative.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Units,
    CompilerDefaults,
    Build,
    Meta,
    Sources,
    Deps,
    Outputs,
    Compiler,
    Features,
    Exports,
}

pub fn read_root_text(input: &str, diags: &mut DiagBag) -> Option<RootConfig> {
    let mut schema = 0u32;
    let mut workspace: Option<String> = None;
    let mut entry: Option<String> = None;

    let mut units: BTreeMap<UnitId, NormalPath> = BTreeMap::new();
    let mut compiler_defaults: BTreeMap<String, String> = BTreeMap::new();
    let mut build: BTreeMap<String, String> = BTreeMap::new();
    let mut meta: BTreeMap<String, String> = BTreeMap::new();

    let mut section = Section::None;

    for (ln, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = match &line[1..line.len() - 1] {
                "units" => Section::Units,
                "compiler_defaults" => Section::CompilerDefaults,
                "build" => Section::Build,
                "meta" => Section::Meta,
                _ => Section::None,
            };
            continue;
        }

        match section {
            Section::None => {
                if let Some((k, v)) = split_kv(line) {
                    match k {
                        "mcfg.schema" => schema = v.parse::<u32>().unwrap_or(0),
                        "root.workspace" => workspace = Some(unquote(v)),
                        "root.entry" => entry = Some(unquote(v)),
                        _ => {}
                    }
                } else {
                    diags.push(Diagnostic::warning(format!("root parse: ignored line {}: {}", ln + 1, raw)));
                }
            }
            Section::Units => {
                if let Some((k, v)) = split_kv(line) {
                    let uid = UnitId(unquote(k));
                    let path = NormalPath { posix: unquote(v), native: None };
                    units.insert(uid, path);
                }
            }
            Section::CompilerDefaults => {
                if let Some((k, v)) = split_kv(line) {
                    compiler_defaults.insert(unquote(k), unquote(v));
                }
            }
            Section::Build => {
                if let Some((k, v)) = split_kv(line) {
                    build.insert(unquote(k), unquote(v));
                }
            }
            Section::Meta => {
                if let Some((k, v)) = split_kv(line) {
                    meta.insert(unquote(k), unquote(v));
                }
            }
            _ => {}
        }
    }

    let workspace_root = NormalPath { posix: workspace.unwrap_or_default(), native: None };
    let mut root = RootConfig::new(workspace_root);
    root.schema_version = schema;
    root.entry_unit = entry.map(UnitId);
    root.units = units;
    root.compiler_defaults = compiler_defaults;
    root.build = build;
    root.meta = meta;

    Some(root)
}

pub fn read_unit_text(input: &str, diags: &mut DiagBag) -> Option<UnitConfig> {
    let mut schema = 0u32;
    let mut unit_id: Option<String> = None;
    let mut workspace_root: Option<String> = None;
    let mut unit_dir: Option<String> = None;
    let mut host_os: HostOs = HostOs::Unknown;
    let mut target = TargetTriple::new("unknown");

    let mut sources_vit: Vec<NormalPath> = Vec::new();
    let mut extra_inputs: Vec<NormalPath> = Vec::new();
    let mut deps_units: Vec<UnitId> = Vec::new();
    let mut outputs: Vec<Artifact> = Vec::new();
    let mut compiler: BTreeMap<String, String> = BTreeMap::new();
    let mut features: BTreeSet<String> = BTreeSet::new();
    let mut exports: BTreeMap<String, ArtifactId> = BTreeMap::new();
    let mut meta: BTreeMap<String, String> = BTreeMap::new();

    let mut section = Section::None;

    for (ln, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = match &line[1..line.len() - 1] {
                "sources" => Section::Sources,
                "deps" => Section::Deps,
                "outputs" => Section::Outputs,
                "compiler" => Section::Compiler,
                "features" => Section::Features,
                "exports" => Section::Exports,
                "meta" => Section::Meta,
                _ => Section::None,
            };
            continue;
        }

        match section {
            Section::None => {
                if let Some((k, v)) = split_kv(line) {
                    match k {
                        "mcfg.schema" => schema = v.parse::<u32>().unwrap_or(0),
                        "unit.id" => unit_id = Some(unquote(v)),
                        "workspace.root" => workspace_root = Some(unquote(v)),
                        "unit.dir" => unit_dir = Some(unquote(v)),
                        "host.os" => host_os = HostOs::parse(&unquote(v)),
                        "target.triple" => target = TargetTriple::new(unquote(v)),
                        _ => {}
                    }
                } else {
                    diags.push(Diagnostic::warning(format!("unit parse: ignored line {}: {}", ln + 1, raw)));
                }
            }
            Section::Sources => {
                if let Some((k, v)) = split_kv(line) {
                    if k == "vit" {
                        sources_vit = parse_list_paths(&unquote(v));
                    } else if k == "extra" {
                        extra_inputs = parse_list_paths(&unquote(v));
                    }
                }
            }
            Section::Deps => {
                if let Some((k, v)) = split_kv(line) {
                    if k == "units" {
                        deps_units = parse_list_str(&unquote(v)).into_iter().map(UnitId).collect();
                    }
                }
            }
            Section::Outputs => {
                // "id" = { kind="va", path="x" }
                if let Some((k, v)) = split_kv(line) {
                    let id = unquote(k);
                    if let Some((kind, path)) = parse_output_obj(&unquote(v)) {
                        outputs.push(Artifact::new(id, kind, NormalPath { posix: path, native: None }));
                    } else {
                        diags.push(Diagnostic::warning(format!("unit outputs: invalid object at line {}", ln + 1)));
                    }
                }
            }
            Section::Compiler => {
                if let Some((k, v)) = split_kv(line) {
                    compiler.insert(unquote(k), unquote(v));
                }
            }
            Section::Features => {
                if let Some((k, v)) = split_kv(line) {
                    if k == "set" {
                        for s in parse_list_str(&unquote(v)) {
                            features.insert(s);
                        }
                    }
                }
            }
            Section::Exports => {
                if let Some((k, v)) = split_kv(line) {
                    exports.insert(unquote(k), ArtifactId(unquote(v)));
                }
            }
            Section::Meta => {
                if let Some((k, v)) = split_kv(line) {
                    meta.insert(unquote(k), unquote(v));
                }
            }
            _ => {}
        }
    }

    let ws = NormalPath { posix: workspace_root.unwrap_or_default(), native: None };
    let ud = NormalPath { posix: unit_dir.unwrap_or_default(), native: None };

    let mut unit = UnitConfig::new(unit_id.unwrap_or_default(), ws, ud);
    unit.schema_version = schema;
    unit.host = host_os;
    unit.target = target;
    unit.sources_vit = sources_vit;
    unit.extra_inputs = extra_inputs;
    unit.deps_units = deps_units;
    unit.outputs = outputs;
    unit.compiler = compiler;
    unit.features = features;
    unit.exports = exports;
    unit.meta = meta;

    Some(unit)
}

/// ------------------------------------------------------------
/// Helpers parsing/writing
/// ------------------------------------------------------------

fn push_kv(out: &mut String, k: &str, v: &str) {
    out.push_str(k);
    out.push_str(" = ");
    out.push_str(v);
    out.push('\n');
}

fn push_kv_str(out: &mut String, k: &str, v: &str) {
    out.push_str(k);
    out.push_str(" = \"");
    out.push_str(&escape(v));
    out.push_str("\"\n");
}

fn escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            _ => o.push(ch),
        }
    }
    o
}

fn unquote(s: &str) -> String {
    let mut t = s.trim().to_string();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        t = t[1..t.len() - 1].to_string();
    }
    // unescape minimal
    let mut out = String::new();
    let mut it = t.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(x) => out.push(x),
                None => break,
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut it = line.splitn(2, '=');
    let k = it.next()?.trim();
    let v = it.next()?.trim();
    Some((k, v))
}

fn fmt_list_str<'a, I>(iter: I) -> String
where
    I: Iterator<Item = &'a str>,
{
    let mut out = String::from("[");
    let mut first = true;
    for s in iter {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push('"');
        out.push_str(&escape(s));
        out.push('"');
    }
    out.push(']');
    out
}

fn parse_list_str(s: &str) -> Vec<String> {
    // expects ["a","b"] or []
    let t = s.trim();
    if !t.starts_with('[') || !t.ends_with(']') {
        return Vec::new();
    }
    let inner = &t[1..t.len() - 1].trim();
    if inner.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    // very small parser: split on quotes blocks
    let mut cur = String::new();
    let mut in_str = false;
    let mut esc = false;

    for ch in inner.chars() {
        if !in_str {
            if ch == '"' {
                in_str = true;
                cur.clear();
                esc = false;
            }
            continue;
        } else {
            if esc {
                cur.push(ch);
                esc = false;
                continue;
            }
            if ch == '\\' {
                esc = true;
                continue;
            }
            if ch == '"' {
                in_str = false;
                out.push(cur.clone());
                continue;
            }
            cur.push(ch);
        }
    }

    out
}

fn parse_list_paths(s: &str) -> Vec<NormalPath> {
    parse_list_str(s)
        .into_iter()
        .map(|p| NormalPath { posix: p, native: None })
        .collect()
}

fn parse_output_obj(s: &str) -> Option<(ArtifactKind, String)> {
    // expects { kind="va", path="x" }
    let t = s.trim();
    if !t.starts_with('{') || !t.ends_with('}') {
        return None;
    }
    let inner = t[1..t.len() - 1].trim();
    let mut kind: Option<ArtifactKind> = None;
    let mut path: Option<String> = None;

    for part in inner.split(',') {
        let part = part.trim();
        let (k, v) = split_kv(part)?;
        match k.trim() {
            "kind" => {
                let ext = unquote(v);
                kind = ArtifactKind::parse_ext(ext.as_str()).or_else(|| {
                    if ext == "file" { Some(ArtifactKind::File) } else { None }
                });
            }
            "path" => {
                path = Some(unquote(v));
            }
            _ => {}
        }
    }

    Some((kind?, path?))
}

/// ------------------------------------------------------------
/// IO helpers (paths)
/// ------------------------------------------------------------

pub fn write_root_file(path: &Path, root: &RootConfig) -> std::io::Result<()> {
    std::fs::write(path, write_root_text(root))
}

pub fn write_unit_file(path: &Path, unit: &UnitConfig) -> std::io::Result<()> {
    std::fs::write(path, write_unit_text(unit))
}

pub fn read_root_file(path: &Path, diags: &mut DiagBag) -> std::io::Result<Option<RootConfig>> {
    let s = std::fs::read_to_string(path)?;
    Ok(read_root_text(&s, diags))
}

pub fn read_unit_file(path: &Path, diags: &mut DiagBag) -> std::io::Result<Option<UnitConfig>> {
    let s = std::fs::read_to_string(path)?;
    Ok(read_unit_text(&s, diags))
}

/// ------------------------------------------------------------
/// Builders “Vitte-like” : helpers pour générer outputs standard
/// ------------------------------------------------------------

pub fn default_output_paths(
    workspace_root: &Path,
    unit_rel_dir: &str,
    unit_name_slug: &str,
    target: &TargetTriple,
) -> (NormalPath, NormalPath, Option<NormalPath>) {
    // paths are posix (relative to root)
    let lib_va = format!("src/out/lib/{}_{}.va", unit_name_slug, target.triple.replace('-', "_"));
    let obj_vo = format!("src/out/bin/{}_{}.vo", unit_name_slug, target.triple.replace('-', "_"));
    let exe = if target.is_windows() {
        Some(format!("src/out/bin/{}_{}.exe", unit_name_slug, target.triple.replace('-', "_")))
    } else {
        None
    };

    let _ = (workspace_root, unit_rel_dir); // placeholders for future richer layout rules

    (
        NormalPath { posix: lib_va, native: None },
        NormalPath { posix: obj_vo, native: None },
        exe.map(|p| NormalPath { posix: p, native: None }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_root_roundtrip_smoke() {
        let mut diags = DiagBag::new();
        let mut root = RootConfig::new(NormalPath { posix: ".", native: None });
        root.entry_unit = Some(UnitId("src/in/a".into()));
        root.units.insert(UnitId("src/in/a".into()), NormalPath { posix: "steel/a.muff".into(), native: None });
        root.compiler_defaults.insert("opt".into(), "3".into());

        let txt = write_root_text(&root);
        let parsed = read_root_text(&txt, &mut diags).unwrap();
        assert_eq!(parsed.schema_version, root.schema_version);
        assert_eq!(parsed.units.len(), 1);
    }

    #[test]
    fn write_read_unit_roundtrip_smoke() {
        let mut diags = DiagBag::new();
        let mut u = UnitConfig::new(
            "src/in/a",
            NormalPath { posix: ".", native: None },
            NormalPath { posix: "src/in/a".into(), native: None },
        );
        u.host = HostOs::Linux;
        u.target = TargetTriple::new("x86_64-unknown-linux-gnu");
        u.sources_vit.push(NormalPath { posix: "src/program/lib.vit".into(), native: None });
        u.outputs.push(Artifact::new("lib", ArtifactKind::Va, NormalPath { posix: "src/out/lib/a.va".into(), native: None }));

        let txt = write_unit_text(&u);
        let parsed = read_unit_text(&txt, &mut diags).unwrap();
        assert_eq!(parsed.unit.0, u.unit.0);
        assert_eq!(parsed.outputs.len(), 1);
    }
}
