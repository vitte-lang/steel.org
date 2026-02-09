//! HIR (High-level IR) for  
//!
//! Objectif : représentation “résolue” du steelconf (Bakefile v2) + primitives
//! nécessaires pour : validation, DAG, scheduling, cache, génération `.mff` / `*.muff`.
//!
//! Contrainte : std uniquement.
//!
//! Architecture typique :
//! - parser -> AST
//! - resolver -> HIR (ce fichier)  + diagnostics (diag.rs)
//! - planner -> DAG + exec plan
//! - emitter -> `.mff` + unités `.muff`
//!
//! Notes :
//! - Toutes les collections structurantes sont BTree* pour stabilité déterministe.
//! - Les identifiants internes (Id wrappers) évitent copies.
//! - Les “strings” sont internées via Interner (NameId).
//! - Les Spans/SourceMap viennent de crate::diag (si dispo).
//! - La couche resolve peut enrichir HIR avec `Origin` (span + fichier).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::diag::Span;

/// ------------------------------------------------------------
/// IDs
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NameId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StoreId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CapsuleId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ToolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ProfileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BakeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PlanId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PortId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct WireId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ExportId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SwitchId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId(pub u32);

/// ------------------------------------------------------------
/// Origin / metadata
/// ------------------------------------------------------------

/// Origine sémantique d’un item HIR (span + notes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub span: Option<Span>,
    pub note: Option<NameId>, // string internée
}

impl Origin {
    pub fn none() -> Self {
        Self { span: None, note: None }
    }
}

/// Attributs génériques.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attrs {
    pub tags: BTreeSet<NameId>,
    pub kv: BTreeMap<NameId, NameId>, // string->string
}

/// ------------------------------------------------------------
/// Interner
/// ------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Interner {
    to_id: BTreeMap<String, NameId>,
    from_id: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, s: impl Into<String>) -> NameId {
        let s = s.into();
        if let Some(id) = self.to_id.get(&s) {
            return *id;
        }
        let id = NameId(self.from_id.len() as u32);
        self.from_id.push(s.clone());
        self.to_id.insert(s, id);
        id
    }

    pub fn get(&self, id: NameId) -> Option<&str> {
        self.from_id.get(id.0 as usize).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.from_id.len()
    }
}

/// ------------------------------------------------------------
/// Types / values
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimType {
    Text,
    Int,
    Bool,
    Bytes,
}

impl PrimType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrimType::Text => "text",
            PrimType::Int => "int",
            PrimType::Bool => "bool",
            PrimType::Bytes => "bytes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactType {
    /// segments (ex: src.glob -> ["src","glob"])
    pub path: Vec<NameId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeRef {
    Prim(PrimType),
    Artifact(ArtifactType),
}

impl TypeRef {
    pub fn prim(p: PrimType) -> Self {
        TypeRef::Prim(p)
    }
    pub fn text() -> Self {
        TypeRef::Prim(PrimType::Text)
    }
    pub fn int() -> Self {
        TypeRef::Prim(PrimType::Int)
    }
    pub fn bool() -> Self {
        TypeRef::Prim(PrimType::Bool)
    }
    pub fn bytes() -> Self {
        TypeRef::Prim(PrimType::Bytes)
    }
}

/// Value typée (runtime) — liste = hétérogène.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Str(NameId),       // string literal
    Int(i64),          // int literal
    Bool(bool),        // bool literal
    List(Vec<Value>),  // list literal
    Ident(NameId),     // ident comme valeur (selon EBNF)
    Path(NameId),      // path literal (string, mais semantique path)
}

impl Value {
    pub fn str(id: NameId) -> Self {
        Value::Str(id)
    }
    pub fn ident(id: NameId) -> Self {
        Value::Ident(id)
    }
}

/// ------------------------------------------------------------
/// References (surface / resolved)
/// ------------------------------------------------------------

/// Référence textuelle : var ou bake.port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ref {
    Var(NameId),
    BakePort { bake: NameId, port: NameId },
}

/// Référence résolue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolvedRef {
    Var(VarId),
    Port(PortId),
    Bake(BakeId),
    Tool(ToolId),
    Profile(ProfileId),
    Store(StoreId),
    Capsule(CapsuleId),
}

/// ------------------------------------------------------------
/// Store
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreMode {
    Content,
    Mtime,
    Off,
}

impl StoreMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StoreMode::Content => "content",
            StoreMode::Mtime => "mtime",
            StoreMode::Off => "off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    pub id: StoreId,
    pub name: NameId,
    pub path: NameId, // string path
    pub mode: StoreMode,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Capsule (sandbox)
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvPolicy {
    Allow(Vec<NameId>),
    Deny(Vec<NameId>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsPolicy {
    AllowRead(Vec<NameId>),
    AllowWrite(Vec<NameId>),
    AllowWriteExact(Vec<NameId>),
    Deny(Vec<NameId>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePolicy {
    pub stable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capsule {
    pub id: CapsuleId,
    pub name: NameId,
    pub env: Option<EnvPolicy>,
    pub fs: Vec<FsPolicy>,
    pub net: Option<NetPolicy>,
    pub time: Option<TimePolicy>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Vars / global setters / profiles
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalSet {
    pub key: NameId,
    pub value: Value,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDecl {
    pub id: VarId,
    pub name: NameId,
    pub ty: TypeRef,
    pub value: Value,
    pub origin: Origin,
    pub attrs: Attrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: ProfileId,
    pub name: NameId,
    pub settings: BTreeMap<NameId, Value>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Tool
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub id: ToolId,
    pub name: NameId,
    pub exec: NameId, // string
    pub expect_version: Option<NameId>,
    pub sandbox: bool,
    pub capsule: Option<CapsuleId>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Bake (ports / makes / runs / cache / outputs)
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortDir {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub id: PortId,
    pub bake: BakeId,
    pub name: NameId,
    pub dir: PortDir,
    pub ty: TypeRef,
    pub origin: Origin,
    pub attrs: Attrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MakeKind {
    Glob,
    File,
    Text,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeStmt {
    pub name: NameId,
    pub kind: MakeKind,
    pub arg: NameId, // string
    pub origin: Origin,
    pub attrs: Attrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Takes {
    pub port: NameId,
    pub as_flag: NameId, // string
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emits {
    pub port: NameId,
    pub as_flag: NameId, // string
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSet {
    pub flag: NameId, // string
    pub value: Value,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunBlock {
    pub tool: ToolId,
    pub takes: Vec<Takes>,
    pub emits: Vec<Emits>,
    pub sets: Vec<RunSet>,
    pub origin: Origin,
    pub attrs: Attrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMode {
    Content,
    Mtime,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputStmt {
    pub port: NameId,
    pub at: NameId, // string path
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bake {
    pub id: BakeId,
    pub name: NameId,
    pub inputs: Vec<PortId>,
    pub outputs: Vec<PortId>,
    pub makes: Vec<MakeStmt>,
    pub runs: Vec<RunBlock>,
    pub cache: CacheMode,
    pub outputs_at: Vec<OutputStmt>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Wiring / export
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wire {
    pub id: WireId,
    pub from: ResolvedRef,
    pub to: ResolvedRef,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub id: ExportId,
    pub what: ResolvedRef, // doit être out-port
    pub origin: Origin,
}

/// ------------------------------------------------------------
/// Plan
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanItem {
    RunExports { origin: Origin },
    Run { what: ResolvedRef, origin: Origin },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub id: PlanId,
    pub name: NameId,
    pub items: Vec<PlanItem>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Switch (CLI mapping)
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchAction {
    Set { key: NameId, value: Value, origin: Origin },
    SetPlan { plan: NameId, origin: Origin },
    RunExports { origin: Origin },
    Run { what: Ref, origin: Origin }, // surface ref (résolution tardive)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchFlag {
    pub flag: NameId, // string "-debug"
    pub action: SwitchAction,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    pub id: SwitchId,
    pub flags: Vec<SwitchFlag>,
    pub origin: Origin,
    pub attrs: Attrs,
}

/// ------------------------------------------------------------
/// Graph / scheduling (post-lowering helpers)
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    Bake(BakeId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub via: PortId,
}

/// DAG (représentation minimale).
#[derive(Debug, Clone, Default)]
pub struct Dag {
    pub nodes: BTreeMap<NodeId, NodeKind>,
    pub edges: Vec<Edge>,
    pub topo: Vec<NodeId>,
}

/// ------------------------------------------------------------
/// Index (name -> id)
/// ------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Index {
    pub vars: BTreeMap<NameId, VarId>,
    pub stores: BTreeMap<NameId, StoreId>,
    pub capsules: BTreeMap<NameId, CapsuleId>,
    pub tools: BTreeMap<NameId, ToolId>,
    pub profiles: BTreeMap<NameId, ProfileId>,
    pub bakes: BTreeMap<NameId, BakeId>,
    pub plans: BTreeMap<NameId, PlanId>,
    pub ports: BTreeMap<(NameId, NameId), PortId>, // (bake,port)->id
}

impl Index {
    pub fn clear(&mut self) {
        *self = Index::default();
    }
}

/// ------------------------------------------------------------
/// Program root
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Program {
    pub version: u32,
    pub interner: Interner,

    pub globals: Vec<GlobalSet>,
    pub vars: Vec<VarDecl>,

    pub stores: Vec<Store>,
    pub capsules: Vec<Capsule>,
    pub tools: Vec<Tool>,
    pub profiles: Vec<Profile>,

    pub bakes: Vec<Bake>,
    pub ports: Vec<Port>,

    pub wires: Vec<Wire>,
    pub exports: Vec<Export>,

    pub plans: Vec<Plan>,
    pub switch: Option<Switch>,

    pub index: Index,

    /// Hints / meta.
    pub attrs: Attrs,
}

impl Program {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            interner: Interner::new(),
            globals: Vec::new(),
            vars: Vec::new(),
            stores: Vec::new(),
            capsules: Vec::new(),
            tools: Vec::new(),
            profiles: Vec::new(),
            bakes: Vec::new(),
            ports: Vec::new(),
            wires: Vec::new(),
            exports: Vec::new(),
            plans: Vec::new(),
            switch: None,
            index: Index::default(),
            attrs: Attrs::default(),
        }
    }

    pub fn name(&self, id: NameId) -> &str {
        self.interner.get(id).unwrap_or("<intern-miss>")
    }
}

/// ------------------------------------------------------------
/// Builders (helpers)
/// ------------------------------------------------------------

impl Program {
    pub fn add_store(&mut self, name: NameId, path: NameId, mode: StoreMode, origin: Origin) -> StoreId {
        let id = StoreId(self.stores.len() as u32);
        self.stores.push(Store { id, name, path, mode, origin, attrs: Attrs::default() });
        self.index.stores.insert(name, id);
        id
    }

    pub fn add_capsule(&mut self, name: NameId, origin: Origin) -> CapsuleId {
        let id = CapsuleId(self.capsules.len() as u32);
        self.capsules.push(Capsule {
            id,
            name,
            env: None,
            fs: Vec::new(),
            net: None,
            time: None,
            origin,
            attrs: Attrs::default(),
        });
        self.index.capsules.insert(name, id);
        id
    }

    pub fn add_tool(&mut self, name: NameId, exec: NameId, sandbox: bool, capsule: Option<CapsuleId>, origin: Origin) -> ToolId {
        let id = ToolId(self.tools.len() as u32);
        self.tools.push(Tool {
            id,
            name,
            exec,
            expect_version: None,
            sandbox,
            capsule,
            origin,
            attrs: Attrs::default(),
        });
        self.index.tools.insert(name, id);
        id
    }

    pub fn add_profile(&mut self, name: NameId, origin: Origin) -> ProfileId {
        let id = ProfileId(self.profiles.len() as u32);
        self.profiles.push(Profile { id, name, settings: BTreeMap::new(), origin, attrs: Attrs::default() });
        self.index.profiles.insert(name, id);
        id
    }

    pub fn add_var(&mut self, name: NameId, ty: TypeRef, value: Value, origin: Origin) -> VarId {
        let id = VarId(self.vars.len() as u32);
        self.vars.push(VarDecl { id, name, ty, value, origin, attrs: Attrs::default() });
        self.index.vars.insert(name, id);
        id
    }

    pub fn add_bake(&mut self, name: NameId, origin: Origin) -> BakeId {
        let id = BakeId(self.bakes.len() as u32);
        self.bakes.push(Bake {
            id,
            name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            makes: Vec::new(),
            runs: Vec::new(),
            cache: CacheMode::Content,
            outputs_at: Vec::new(),
            origin,
            attrs: Attrs::default(),
        });
        self.index.bakes.insert(name, id);
        id
    }

    pub fn add_port(&mut self, bake: BakeId, bake_name: NameId, name: NameId, dir: PortDir, ty: TypeRef, origin: Origin) -> PortId {
        let id = PortId(self.ports.len() as u32);
        self.ports.push(Port { id, bake, name, dir, ty, origin, attrs: Attrs::default() });
        self.index.ports.insert((bake_name, name), id);

        // attach to bake
        if let Some(b) = self.bakes.get_mut(bake.0 as usize) {
            match dir {
                PortDir::In => b.inputs.push(id),
                PortDir::Out => b.outputs.push(id),
            }
        }

        id
    }
}

/// ------------------------------------------------------------
/// Display (debug)
/// ------------------------------------------------------------

impl fmt::Display for PrimType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interner_basic() {
        let mut i = Interner::new();
        let a = i.intern("x");
        let b = i.intern("x");
        assert_eq!(a, b);
        assert_eq!(i.get(a), Some("x"));
    }

    #[test]
    fn program_builder_smoke() {
        let mut p = Program::new(2);
        let n_store = p.interner.intern("store0");
        let n_path = p.interner.intern("./.steel/store");
        let _sid = p.add_store(n_store, n_path, StoreMode::Content, Origin::none());
        assert_eq!(p.stores.len(), 1);
    }
}