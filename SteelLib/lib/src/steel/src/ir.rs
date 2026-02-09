//! IR (execution / build graph IR)  
//!
//! Objectif : IR opérationnel dérivé du HIR, optimisé pour :
//! - construction du DAG, topological scheduling, exécution
//! - calcul de clés de cache (inputs + tool + args + env/capsule)
//! - matérialisation des artefacts (paths, types, stores)
//!
//! Contrainte : std uniquement.
//!
//! Flux : AST -> HIR (resolve) -> IR (lowering) -> ExecPlan -> Emitter
//!
//! Ce fichier ne parse rien : il définit les structures et utilitaires.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::PathBuf;

use crate::hir::{
    ArtifactType, CacheMode, CapsuleId, EnvPolicy, FsPolicy, Interner, MakeKind, NameId, NetPolicy,
    Origin, PortDir, PortId, PrimType, ProfileId, ResolvedRef, StoreId, StoreMode, ToolId, TypeRef,
    Value, VarId, BakeId,
};

/// ------------------------------------------------------------
/// IR IDs
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrEdgeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrArtifactId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrStoreId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrCapsuleId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrToolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrProfileId(pub u32);

/// ------------------------------------------------------------
/// Artifact typing
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrType {
    Prim(PrimType),
    Artifact(IrArtifactType),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IrArtifactType {
    pub path: Vec<NameId>,
}

impl From<&TypeRef> for IrType {
    fn from(t: &TypeRef) -> Self {
        match t {
            TypeRef::Prim(p) => IrType::Prim(p.clone()),
            TypeRef::Artifact(ArtifactType { path }) => IrType::Artifact(IrArtifactType { path: path.clone() }),
        }
    }
}

/// ------------------------------------------------------------
/// Values
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IrValue {
    Str(NameId),
    Int(i64),
    Bool(bool),
    List(Vec<IrValue>),
    Ident(NameId),
    Path(NameId),
}

impl From<&Value> for IrValue {
    fn from(v: &Value) -> Self {
        match v {
            Value::Str(x) => IrValue::Str(*x),
            Value::Int(i) => IrValue::Int(*i),
            Value::Bool(b) => IrValue::Bool(*b),
            Value::List(xs) => IrValue::List(xs.iter().map(IrValue::from).collect()),
            Value::Ident(x) => IrValue::Ident(*x),
            Value::Path(x) => IrValue::Path(*x),
        }
    }
}

/// ------------------------------------------------------------
/// Resolved paths / artifacts
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactLocation {
    /// Un chemin dans le workspace.
    Workspace(PathBuf),
    /// Un chemin dans le store (cache).
    Store { store: IrStoreId, key: CacheKey, rel: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: IrArtifactId,
    pub name: NameId,
    pub ty: IrType,
    pub origin: Origin,
    pub location: Option<ArtifactLocation>,
    pub attrs: BTreeMap<NameId, IrValue>,
}

impl Artifact {
    pub fn new(id: IrArtifactId, name: NameId, ty: IrType, origin: Origin) -> Self {
        Self { id, name, ty, origin, location: None, attrs: BTreeMap::new() }
    }
}

/// ------------------------------------------------------------
/// Store / capsule / tool (IR view)
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrStore {
    pub id: IrStoreId,
    pub name: NameId,
    pub path: PathBuf,
    pub mode: StoreMode,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrCapsule {
    pub id: IrCapsuleId,
    pub name: NameId,
    pub env: Option<EnvPolicy>,
    pub fs: Vec<FsPolicy>,
    pub net: Option<NetPolicy>,
    pub time_stable: Option<bool>,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrTool {
    pub id: IrToolId,
    pub name: NameId,
    pub exec: PathBuf,
    pub expect_version: Option<NameId>,
    pub sandbox: bool,
    pub capsule: Option<IrCapsuleId>,
    pub origin: Origin,
}

/// ------------------------------------------------------------
/// Profile (IR view)
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrProfile {
    pub id: IrProfileId,
    pub name: NameId,
    pub settings: BTreeMap<NameId, IrValue>,
    pub origin: Origin,
}

/// ------------------------------------------------------------
/// Node (Bake execution) / Ports / Edges
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrPort {
    pub name: NameId,
    pub dir: PortDir,
    pub ty: IrType,
    pub origin: Origin,
    /// Port mappé vers un artefact IR.
    pub artifact: Option<IrArtifactId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRunBinding {
    pub takes: Vec<(NameId, NameId)>, // port -> flag
    pub emits: Vec<(NameId, NameId)>, // port -> flag
    pub sets: Vec<(NameId, IrValue)>,  // flag -> value
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMake {
    pub name: NameId,
    pub kind: MakeKind,
    pub arg: NameId, // string
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRun {
    pub tool: IrToolId,
    pub binding: IrRunBinding,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrNode {
    pub id: IrNodeId,
    pub bake_name: NameId,
    pub origin: Origin,

    pub cache: CacheMode,
    pub inputs: BTreeMap<NameId, IrPort>,
    pub outputs: BTreeMap<NameId, IrPort>,

    pub makes: Vec<IrMake>,
    pub runs: Vec<IrRun>,

    /// output port -> fixed workspace path (optional)
    pub outputs_at: BTreeMap<NameId, PathBuf>,

    /// Extra attributes / tuning.
    pub attrs: BTreeMap<NameId, IrValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEdge {
    pub id: IrEdgeId,
    pub from: IrNodeId,
    pub to: IrNodeId,
    pub via: NameId, // port name on edge (semantic)
    pub origin: Origin,
}

/// ------------------------------------------------------------
/// Cache key / fingerprints
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CacheKey(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDigest {
    pub key: CacheKey,
    pub details: BTreeMap<NameId, IrValue>,
}

pub fn hash_bytes_fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut h = OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Canonical string builder pour hashing (déterministe).
pub fn hash_kv_fnv1a64(pairs: impl IntoIterator<Item = (String, String)>) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    const PRIME: u64 = 0x100000001b3;
    for (k, v) in pairs {
        for b in k.as_bytes().iter().chain([b'='].iter()).chain(v.as_bytes()).chain([b'\n'].iter()) {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
    }
    h
}

/// ------------------------------------------------------------
/// Program IR root
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrProgram {
    pub interner: Interner,

    pub stores: Vec<IrStore>,
    pub capsules: Vec<IrCapsule>,
    pub tools: Vec<IrTool>,
    pub profiles: Vec<IrProfile>,

    pub artifacts: Vec<Artifact>,
    pub nodes: Vec<IrNode>,
    pub edges: Vec<IrEdge>,

    /// Map bake_name -> node id
    pub node_index: BTreeMap<NameId, IrNodeId>,

    /// Plans (exec scenarios)
    pub plans: BTreeMap<NameId, IrPlan>,

    /// Exports: list of out-artifacts
    pub exports: Vec<IrArtifactId>,

    /// Meta
    pub version: u32,
    pub origin: Origin,
}

impl IrProgram {
    pub fn new(version: u32, interner: Interner) -> Self {
        Self {
            interner,
            stores: Vec::new(),
            capsules: Vec::new(),
            tools: Vec::new(),
            profiles: Vec::new(),
            artifacts: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            node_index: BTreeMap::new(),
            plans: BTreeMap::new(),
            exports: Vec::new(),
            version,
            origin: Origin { span: None, note: None },
        }
    }

    pub fn name(&self, id: NameId) -> &str {
        self.interner.get(id).unwrap_or("<intern-miss>")
    }
}

/// ------------------------------------------------------------
/// Plans
/// ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrPlanItem {
    RunExports,
    RunNode(IrNodeId),
    RunArtifact(IrArtifactId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrPlan {
    pub name: NameId,
    pub items: Vec<IrPlanItem>,
    pub origin: Origin,
    pub attrs: BTreeMap<NameId, IrValue>,
}

/// ------------------------------------------------------------
/// Lowering skeleton (HIR -> IR) — “driver” côté compilation
/// ------------------------------------------------------------

#[derive(Debug, Default)]
pub struct LoweringCtx {
    /// mapping HIR ids -> IR ids
    pub store_map: BTreeMap<StoreId, IrStoreId>,
    pub capsule_map: BTreeMap<CapsuleId, IrCapsuleId>,
    pub tool_map: BTreeMap<ToolId, IrToolId>,
    pub profile_map: BTreeMap<ProfileId, IrProfileId>,
    pub bake_map: BTreeMap<BakeId, IrNodeId>,
    pub port_map: BTreeMap<PortId, (IrNodeId, NameId, PortDir)>,
    pub var_map: BTreeMap<VarId, IrValue>,
}

/// Minimal errors (la vraie impl fait diag.rs).
#[derive(Debug, Clone)]
pub enum LowerError {
    Missing(String),
    Invalid(String),
}

/// Entrée lowering : un HIR Program.
pub fn lower_hir_to_ir(hir: &crate::hir::Program) -> Result<IrProgram, LowerError> {
    // IMPORTANT : ce module définit l’IR. La logique complète de lowering est
    // généralement dans un module `lower.rs`. Ici on met un squelette cohérent.
    let mut ir = IrProgram::new(hir.version, hir.interner.clone());
    let mut ctx = LoweringCtx::default();

    // Stores
    for s in &hir.stores {
        let path = PathBuf::from(hir.name(s.path));
        let id = IrStoreId(ir.stores.len() as u32);
        ir.stores.push(IrStore { id, name: s.name, path, mode: s.mode.clone(), origin: s.origin.clone() });
        ctx.store_map.insert(s.id, id);
    }

    // Capsules
    for c in &hir.capsules {
        let id = IrCapsuleId(ir.capsules.len() as u32);
        ir.capsules.push(IrCapsule {
            id,
            name: c.name,
            env: c.env.clone(),
            fs: c.fs.clone(),
            net: c.net.clone(),
            time_stable: c.time.as_ref().map(|t| t.stable),
            origin: c.origin.clone(),
        });
        ctx.capsule_map.insert(c.id, id);
    }

    // Tools
    for t in &hir.tools {
        let id = IrToolId(ir.tools.len() as u32);
        let exec = PathBuf::from(hir.name(t.exec));
        let capsule = t.capsule.and_then(|cid| ctx.capsule_map.get(&cid).copied());
        ir.tools.push(IrTool {
            id,
            name: t.name,
            exec,
            expect_version: t.expect_version,
            sandbox: t.sandbox,
            capsule,
            origin: t.origin.clone(),
        });
        ctx.tool_map.insert(t.id, id);
    }

    // Profiles
    for p in &hir.profiles {
        let id = IrProfileId(ir.profiles.len() as u32);
        let mut settings = BTreeMap::new();
        for (k, v) in &p.settings {
            settings.insert(*k, IrValue::from(v));
        }
        ir.profiles.push(IrProfile { id, name: p.name, settings, origin: p.origin.clone() });
        ctx.profile_map.insert(p.id, id);
    }

    // Vars
    for v in &hir.vars {
        ctx.var_map.insert(v.id, IrValue::from(&v.value));
    }

    // Nodes (bakes)
    for b in &hir.bakes {
        let nid = IrNodeId(ir.nodes.len() as u32);
        let mut node = IrNode {
            id: nid,
            bake_name: b.name,
            origin: b.origin.clone(),
            cache: b.cache.clone(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            makes: Vec::new(),
            runs: Vec::new(),
            outputs_at: BTreeMap::new(),
            attrs: BTreeMap::new(),
        };

        // ports
        for pid in &b.inputs {
            let p = hir.ports.get(pid.0 as usize).ok_or_else(|| LowerError::Missing("port".into()))?;
            node.inputs.insert(
                p.name,
                IrPort { name: p.name, dir: p.dir, ty: IrType::from(&p.ty), origin: p.origin.clone(), artifact: None },
            );
            ctx.port_map.insert(*pid, (nid, p.name, p.dir));
        }
        for pid in &b.outputs {
            let p = hir.ports.get(pid.0 as usize).ok_or_else(|| LowerError::Missing("port".into()))?;
            node.outputs.insert(
                p.name,
                IrPort { name: p.name, dir: p.dir, ty: IrType::from(&p.ty), origin: p.origin.clone(), artifact: None },
            );
            ctx.port_map.insert(*pid, (nid, p.name, p.dir));
        }

        // makes
        for m in &b.makes {
            node.makes.push(IrMake { name: m.name, kind: m.kind.clone(), arg: m.arg, origin: m.origin.clone() });
        }

        // runs
        for r in &b.runs {
            let tool = ctx.tool_map.get(&r.tool).copied().ok_or_else(|| LowerError::Missing("tool".into()))?;
            let mut takes = Vec::new();
            let mut emits = Vec::new();
            let mut sets = Vec::new();

            for t in &r.takes {
                takes.push((t.port, t.as_flag));
            }
            for e in &r.emits {
                emits.push((e.port, e.as_flag));
            }
            for s in &r.sets {
                sets.push((s.flag, IrValue::from(&s.value)));
            }

            node.runs.push(IrRun { tool, binding: IrRunBinding { takes, emits, sets }, origin: r.origin.clone() });
        }

        // outputs_at
        for o in &b.outputs_at {
            node.outputs_at.insert(o.port, PathBuf::from(hir.name(o.at)));
        }

        ir.node_index.insert(b.name, nid);
        ctx.bake_map.insert(b.id, nid);
        ir.nodes.push(node);
    }

    // Edges (wires)
    for w in &hir.wires {
        let (from_node, via_name) = match w.from {
            ResolvedRef::Port(pid) => {
                let (nid, pname, _dir) = ctx.port_map.get(&pid).copied().ok_or_else(|| LowerError::Missing("wire.from".into()))?;
                (nid, pname)
            }
            _ => return Err(LowerError::Invalid("wire.from must be port".into())),
        };
        let to_node = match w.to {
            ResolvedRef::Port(pid) => {
                let (nid, _pname, _dir) = ctx.port_map.get(&pid).copied().ok_or_else(|| LowerError::Missing("wire.to".into()))?;
                nid
            }
            _ => return Err(LowerError::Invalid("wire.to must be port".into())),
        };

        let eid = IrEdgeId(ir.edges.len() as u32);
        ir.edges.push(IrEdge { id: eid, from: from_node, to: to_node, via: via_name, origin: w.origin.clone() });
    }

    // Plans (mapping -> nodes/artifacts)
    for p in &hir.plans {
        let mut items = Vec::new();
        for it in &p.items {
            match it {
                crate::hir::PlanItem::RunExports { .. } => items.push(IrPlanItem::RunExports),
                crate::hir::PlanItem::Run { what, .. } => match what {
                    ResolvedRef::Bake(bid) => {
                        let nid = ctx.bake_map.get(bid).copied().ok_or_else(|| LowerError::Missing("plan bake".into()))?;
                        items.push(IrPlanItem::RunNode(nid));
                    }
                    ResolvedRef::Port(_pid) => {
                        // possibilité : exécuter node du port
                        return Err(LowerError::Invalid("plan run port not supported in this skeleton".into()));
                    }
                    ResolvedRef::Var(_)
                    | ResolvedRef::Tool(_)
                    | ResolvedRef::Profile(_)
                    | ResolvedRef::Store(_)
                    | ResolvedRef::Capsule(_) => {
                        return Err(LowerError::Invalid("plan run ref not supported".into()));
                    }
                },
            }
        }
        ir.plans.insert(p.name, IrPlan { name: p.name, items, origin: p.origin.clone(), attrs: BTreeMap::new() });
    }

    // Exports: (placeholder) — la vraie impl mappe out-ports vers artifacts.
    // Ici on garde une liste vide; typiquement un module `artifact_lower.rs` les construit.
    Ok(ir)
}

/// ------------------------------------------------------------
/// DAG / scheduling utilities
/// ------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Dag {
    pub nodes: BTreeSet<IrNodeId>,
    pub edges_out: BTreeMap<IrNodeId, BTreeSet<IrNodeId>>,
    pub indeg: BTreeMap<IrNodeId, usize>,
}

impl Dag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_ir(ir: &IrProgram) -> Self {
        let mut dag = Dag::new();
        for n in &ir.nodes {
            dag.nodes.insert(n.id);
            dag.indeg.insert(n.id, 0);
        }
        for e in &ir.edges {
            dag.edges_out.entry(e.from).or_default().insert(e.to);
            *dag.indeg.entry(e.to).or_insert(0) += 1;
        }
        dag
    }

    /// Kahn topo sort.
    pub fn topo_sort(&self) -> Result<Vec<IrNodeId>, String> {
        let mut indeg = self.indeg.clone();
        let mut q = VecDeque::new();
        for &n in &self.nodes {
            if *indeg.get(&n).unwrap_or(&0) == 0 {
                q.push_back(n);
            }
        }

        let mut out = Vec::new();
        while let Some(n) = q.pop_front() {
            out.push(n);
            if let Some(succ) = self.edges_out.get(&n) {
                for &m in succ {
                    let e = indeg.get_mut(&m).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        q.push_back(m);
                    }
                }
            }
        }

        if out.len() != self.nodes.len() {
            return Err("cycle detected in DAG".to_string());
        }
        Ok(out)
    }
}

/// ------------------------------------------------------------
/// Display (debug)
/// ------------------------------------------------------------

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrType::Prim(p) => write!(f, "{}", p),
            IrType::Artifact(a) => {
                write!(f, "artifact(")?;
                for (i, seg) in a.path.iter().enumerate() {
                    if i != 0 {
                        write!(f, ".")?;
                    }
                    write!(f, "{}", seg.0)?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_hash_smoke() {
        assert_eq!(hash_bytes_fnv1a64(b"abc"), hash_bytes_fnv1a64(b"abc"));
        assert_ne!(hash_bytes_fnv1a64(b"abc"), hash_bytes_fnv1a64(b"abd"));
    }

    #[test]
    fn dag_topo_smoke() {
        let mut d = Dag::new();
        let a = IrNodeId(0);
        let b = IrNodeId(1);
        d.nodes.insert(a);
        d.nodes.insert(b);
        d.indeg.insert(a, 0);
        d.indeg.insert(b, 1);
        d.edges_out.entry(a).or_default().insert(b);

        let topo = d.topo_sort().unwrap();
        assert_eq!(topo, vec![a, b]);
    }
}