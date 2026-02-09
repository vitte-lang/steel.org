//! lower.rs 
//!
//! Lowering HIR -> IR.
//!
//! Rôle :
//! - valider la sémantique (ports, wire, export, run bindings)
//! - matérialiser un IR opérationnel (nodes/edges/artifacts/exports/plans)
//! - préparer l’exécution (DAG stable, artefacts attachés aux ports, hints de paths)
//!
//! Contrainte : std uniquement.
//!
//! Notes d’intégration :
//! - Ce module pousse des diagnostics (`DiagBag`) et tente de continuer (best-effort).
//! - Sur erreurs “bloquantes”, `lower()` renvoie `None`.
//!
//! Hypothèses raisonnables (cohérentes avec l’EBNF) :
//! - `wire` connecte OUT -> IN (sinon error).
//! - `export` référence un OUT port (sinon error).
//! - `make <name> ...` produit une “valeur”/artefact pour le port `<name>` (si port existe).
//! - `run tool <name>` : `takes` ne peut viser que des IN ports, `emits` que des OUT ports.
//!
//! Important : l’IR définit des Artifacts. Ici, on choisit :
//! - 1 artefact par port (in/out), nommé de façon stable: "<bake>.<port>"
//! - si `outputs_at` force un chemin, l’artefact OUT est annoté avec ce chemin (workspace).

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{DiagBag, Diagnostic};
use crate::hir::{
    Bake, BakeId, CacheMode, CapsuleId, Export, GlobalSet, Interner, MakeKind, NameId, Origin, Plan,
    PlanItem, PortDir, PortId, PrimType, ProfileId, Program as HirProgram, Ref, ResolvedRef, StoreId,
    ToolId, TypeRef, Value, VarDecl, VarId, Wire,
};
use crate::ir::{
    hash_bytes_fnv1a64, Artifact, ArtifactLocation, CacheKey, IrArtifactId, IrArtifactType, IrCapsule,
    IrCapsuleId, IrEdge, IrEdgeId, IrMake, IrNode, IrNodeId, IrPlan, IrPlanItem, IrPort, IrProfile,
    IrProfileId, IrProgram, IrRun, IrRunBinding, IrStore, IrStoreId, IrTool, IrToolId, IrType, IrValue,
};

/// ------------------------------------------------------------
/// Options
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LowerOptions {
    /// Autoriser `plan run` sur un port (sinon error).
    /// Si true, on exécute le node parent du port.
    pub allow_plan_run_port: bool,

    /// Si true, certaines erreurs deviennent “fatales”.
    pub strict: bool,
}

impl Default for LowerOptions {
    fn default() -> Self {
        Self { allow_plan_run_port: true, strict: false }
    }
}

/// ------------------------------------------------------------
/// Context / maps
/// ------------------------------------------------------------

#[derive(Debug, Default)]
pub struct LowerCtx {
    pub store_map: BTreeMap<StoreId, IrStoreId>,
    pub capsule_map: BTreeMap<CapsuleId, IrCapsuleId>,
    pub tool_map: BTreeMap<ToolId, IrToolId>,
    pub profile_map: BTreeMap<ProfileId, IrProfileId>,
    pub bake_map: BTreeMap<BakeId, IrNodeId>,
    pub port_map: BTreeMap<PortId, (IrNodeId, NameId, PortDir)>,
    pub var_map: BTreeMap<VarId, IrValue>,

    /// (node, port_name) -> artifact id
    pub port_artifacts: BTreeMap<(IrNodeId, NameId), IrArtifactId>,
}

/// ------------------------------------------------------------
/// Public API
/// ------------------------------------------------------------

/// Lowering principal.
/// Renvoie `None` si erreurs bloquantes.
pub fn lower(hir: &HirProgram, opts: &LowerOptions, diags: &mut DiagBag) -> Option<IrProgram> {
    let mut ir = IrProgram::new(hir.version, hir.interner.clone());
    let mut ctx = LowerCtx::default();

    // 0) Vars (déjà utiles pour flags/run_set, etc.)
    lower_vars(hir, &mut ctx);

    // 1) Stores / Capsules / Tools / Profiles
    lower_stores(hir, &mut ir, &mut ctx, diags);
    lower_capsules(hir, &mut ir, &mut ctx, diags);
    lower_tools(hir, &mut ir, &mut ctx, diags);
    lower_profiles(hir, &mut ir, &mut ctx, diags);

    // 2) Nodes (bakes) + ports + base artifacts
    lower_nodes_and_ports(hir, &mut ir, &mut ctx, diags);

    // 3) Make statements (attach aux ports si possible)
    attach_makes(hir, &mut ir, &mut ctx, diags);

    // 4) Wire edges (validate + build edges)
    lower_wires(hir, &mut ir, &mut ctx, diags);

    // 5) Exports (out ports -> artifact)
    lower_exports(hir, &mut ir, &mut ctx, diags);

    // 6) Plans (map to nodes/artifacts)
    lower_plans(hir, &mut ir, &mut ctx, opts, diags);

    // Final: si erreurs, décider si on retourne None
    if diags.has_error() && opts.strict {
        return None;
    }

    // Même sans strict, si IR impossible (pas de nodes ou map cassée) => None.
    // Ici: on considère qu’un IR vide est acceptable (outil “format/inspect”).
    Some(ir)
}

/// Compat: API courte (options par défaut).
pub fn lower_default(hir: &HirProgram, diags: &mut DiagBag) -> Option<IrProgram> {
    lower(hir, &LowerOptions::default(), diags)
}

/// ------------------------------------------------------------
/// Lowering steps
/// ------------------------------------------------------------

fn lower_vars(hir: &HirProgram, ctx: &mut LowerCtx) {
    for v in &hir.vars {
        ctx.var_map.insert(v.id, IrValue::from(&v.value));
    }
}

fn lower_stores(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
    for s in &hir.stores {
        let path_s = hir.name(s.path);
        let path = std::path::PathBuf::from(path_s);

        if path_s.is_empty() {
            diags.push(Diagnostic::error("store path is empty"));
        }

        let id = IrStoreId(ir.stores.len() as u32);
        ir.stores.push(IrStore { id, name: s.name, path, mode: s.mode.clone(), origin: s.origin.clone() });
        ctx.store_map.insert(s.id, id);
    }
}

fn lower_capsules(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, _diags: &mut DiagBag) {
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
}

fn lower_tools(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
    for t in &hir.tools {
        let exec_s = hir.name(t.exec);
        if exec_s.is_empty() {
            diags.push(Diagnostic::error("tool exec is empty"));
        }

        let capsule = t.capsule.and_then(|cid| ctx.capsule_map.get(&cid).copied());

        if t.capsule.is_some() && capsule.is_none() {
            diags.push(Diagnostic::error(format!(
                "tool `{}` references unknown capsule",
                hir.name(t.name)
            )));
        }

        let id = IrToolId(ir.tools.len() as u32);
        ir.tools.push(IrTool {
            id,
            name: t.name,
            exec: std::path::PathBuf::from(exec_s),
            expect_version: t.expect_version,
            sandbox: t.sandbox,
            capsule,
            origin: t.origin.clone(),
        });
        ctx.tool_map.insert(t.id, id);
    }
}

fn lower_profiles(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, _diags: &mut DiagBag) {
    for p in &hir.profiles {
        let id = IrProfileId(ir.profiles.len() as u32);
        let mut settings = BTreeMap::new();
        for (k, v) in &p.settings {
            settings.insert(*k, IrValue::from(v));
        }
        ir.profiles.push(IrProfile { id, name: p.name, settings, origin: p.origin.clone() });
        ctx.profile_map.insert(p.id, id);
    }
}

fn lower_nodes_and_ports(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
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

        // 1) Ports
        for pid in &b.inputs {
            let p = match hir.ports.get(pid.0 as usize) {
                Some(p) => p,
                None => {
                    diags.push(Diagnostic::error("invalid port id in bake.inputs"));
                    continue;
                }
            };

            if p.dir != PortDir::In {
                diags.push(Diagnostic::error(format!(
                    "port `{}` in bake `{}` listed as input but is not `in`",
                    hir.name(p.name),
                    hir.name(b.name)
                )));
            }

            node.inputs.insert(
                p.name,
                IrPort { name: p.name, dir: p.dir, ty: IrType::from(&p.ty), origin: p.origin.clone(), artifact: None },
            );
            ctx.port_map.insert(*pid, (nid, p.name, p.dir));
        }

        for pid in &b.outputs {
            let p = match hir.ports.get(pid.0 as usize) {
                Some(p) => p,
                None => {
                    diags.push(Diagnostic::error("invalid port id in bake.outputs"));
                    continue;
                }
            };

            if p.dir != PortDir::Out {
                diags.push(Diagnostic::error(format!(
                    "port `{}` in bake `{}` listed as output but is not `out`",
                    hir.name(p.name),
                    hir.name(b.name)
                )));
            }

            node.outputs.insert(
                p.name,
                IrPort { name: p.name, dir: p.dir, ty: IrType::from(&p.ty), origin: p.origin.clone(), artifact: None },
            );
            ctx.port_map.insert(*pid, (nid, p.name, p.dir));
        }

        // 2) Runs (validation basique)
        for r in &b.runs {
            let tool = match ctx.tool_map.get(&r.tool).copied() {
                Some(t) => t,
                None => {
                    diags.push(Diagnostic::error(format!(
                        "bake `{}` references unknown tool",
                        hir.name(b.name)
                    )));
                    continue;
                }
            };

            // takes/emits validation
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

        // 3) outputs_at
        for o in &b.outputs_at {
            let at = std::path::PathBuf::from(hir.name(o.at));
            node.outputs_at.insert(o.port, at);
        }

        // 4) Insert node
        ir.node_index.insert(b.name, nid);
        ctx.bake_map.insert(b.id, nid);

        ir.nodes.push(node);
    }

    // 5) Create base artifacts for each port (stable)
    for node in &mut ir.nodes {
        // Inputs
        let in_keys: Vec<NameId> = node.inputs.keys().copied().collect();
        for pname in in_keys {
            let art = make_port_artifact(hir, ir, node.id, node.bake_name, pname, node.inputs[&pname].ty.clone(), node.inputs[&pname].origin.clone());
            node.inputs.get_mut(&pname).unwrap().artifact = Some(art);
            ctx.port_artifacts.insert((node.id, pname), art);
        }

        // Outputs
        let out_keys: Vec<NameId> = node.outputs.keys().copied().collect();
        for pname in out_keys {
            let mut art = make_port_artifact(hir, ir, node.id, node.bake_name, pname, node.outputs[&pname].ty.clone(), node.outputs[&pname].origin.clone());

            // Si outputs_at force un path, annoter location
            if let Some(p) = node.outputs_at.get(&pname).cloned() {
                if let Some(a) = ir.artifacts.get_mut(art.0 as usize) {
                    a.location = Some(ArtifactLocation::Workspace(p));
                }
            }

            node.outputs.get_mut(&pname).unwrap().artifact = Some(art);
            ctx.port_artifacts.insert((node.id, pname), art);
        }
    }

    // 6) Validate run bindings against ports
    for node in &ir.nodes {
        validate_run_bindings(hir, node, diags);
    }
}

fn attach_makes(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
    // Pour chaque node, convertir HIR makes en IR makes et attacher si possible:
    // - si un port (in/out) porte le même nom que `make.name`, on set son artifact
    //   (ingredient) ou on annote l’existant.
    // - sinon, créer un artifact “extra” (non attaché à un port).
    for node in &mut ir.nodes {
        let bake_name = node.bake_name;

        // récupérer Bake HIR (par nom)
        // (comme ir.nodes est dérivé de hir.bakes dans le même ordre, on peut indexer)
        let hir_bake = match find_hir_bake_by_name(hir, bake_name) {
            Some(b) => b,
            None => continue,
        };

        for m in &hir_bake.makes {
            node.makes.push(IrMake { name: m.name, kind: m.kind.clone(), arg: m.arg, origin: m.origin.clone() });

            // Port match
            if let Some(p) = node.outputs.get(&m.name) {
                // OUTPUT ingredient: acceptable (glob -> out typed)
                let art_id = ensure_make_attached(hir, ir, node, m.name, p.ty.clone(), m.origin.clone(), ctx);
                let _ = art_id;
                continue;
            }
            if let Some(p) = node.inputs.get(&m.name) {
                // INPUT ingredient: acceptable
                let art_id = ensure_make_attached(hir, ir, node, m.name, p.ty.clone(), m.origin.clone(), ctx);
                let _ = art_id;
                continue;
            }

            // Sinon: extra artifact
            let extra_name = intern_join(ir, hir.name(bake_name), hir.name(m.name), ".make.");
            let id = IrArtifactId(ir.artifacts.len() as u32);
            let mut a = Artifact::new(id, extra_name, IrType::Prim(PrimType::Bytes), m.origin.clone());
            a.attrs.insert(ir.interner.intern("make_kind".to_string()), IrValue::Ident(ir.interner.intern(format!("{:?}", m.kind))));
            a.attrs.insert(ir.interner.intern("make_arg".to_string()), IrValue::Str(m.arg));
            ir.artifacts.push(a);
        }
    }

    // Validation simple: make port name should exist as output in many cases
    // (warning only)
    for node in &ir.nodes {
        let hir_bake = match find_hir_bake_by_name(hir, node.bake_name) {
            Some(b) => b,
            None => continue,
        };
        for m in &hir_bake.makes {
            if !node.inputs.contains_key(&m.name) && !node.outputs.contains_key(&m.name) {
                diags.push(Diagnostic::warning(format!(
                    "make `{}` in bake `{}` does not match any port name",
                    hir.name(m.name),
                    hir.name(node.bake_name)
                )));
            }
        }
    }
}

fn lower_wires(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
    for w in &hir.wires {
        let (from_node, from_port, from_dir) = match w.from {
            ResolvedRef::Port(pid) => match ctx.port_map.get(&pid).copied() {
                Some(x) => x,
                None => {
                    diags.push(Diagnostic::error("wire.from references unknown port"));
                    continue;
                }
            },
            _ => {
                diags.push(Diagnostic::error("wire.from must reference a port"));
                continue;
            }
        };

        let (to_node, to_port, to_dir) = match w.to {
            ResolvedRef::Port(pid) => match ctx.port_map.get(&pid).copied() {
                Some(x) => x,
                None => {
                    diags.push(Diagnostic::error("wire.to references unknown port"));
                    continue;
                }
            },
            _ => {
                diags.push(Diagnostic::error("wire.to must reference a port"));
                continue;
            }
        };

        if from_dir != PortDir::Out {
            diags.push(Diagnostic::error(format!(
                "wire source must be an out port: {}.{}",
                hir.name(ir.nodes[from_node.0 as usize].bake_name),
                hir.name(from_port)
            )));
            continue;
        }
        if to_dir != PortDir::In {
            diags.push(Diagnostic::error(format!(
                "wire destination must be an in port: {}.{}",
                hir.name(ir.nodes[to_node.0 as usize].bake_name),
                hir.name(to_port)
            )));
            continue;
        }

        // Type compatibility (strict equality)
        let from_ty = ir.nodes[from_node.0 as usize].outputs.get(&from_port).map(|p| p.ty.clone());
        let to_ty = ir.nodes[to_node.0 as usize].inputs.get(&to_port).map(|p| p.ty.clone());
        if let (Some(a), Some(b)) = (from_ty, to_ty) {
            if a != b {
                diags.push(Diagnostic::error(format!(
                    "wire type mismatch: {}.{} ({}) -> {}.{} ({})",
                    hir.name(ir.nodes[from_node.0 as usize].bake_name),
                    hir.name(from_port),
                    display_ty(hir, ir, &a),
                    hir.name(ir.nodes[to_node.0 as usize].bake_name),
                    hir.name(to_port),
                    display_ty(hir, ir, &b),
                )));
            }
        }

        // Build IR edge
        let eid = IrEdgeId(ir.edges.len() as u32);
        ir.edges.push(IrEdge { id: eid, from: from_node, to: to_node, via: from_port, origin: w.origin.clone() });

        // Attach artifact flow (from OUT artifact -> to IN artifact)
        // On ne remplace pas l’artifact IN (il reste unique), mais on peut annoter
        // l’artifact IN avec un attribut `wired_from` pour debug/why.
        let from_a = ctx.port_artifacts.get(&(from_node, from_port)).copied();
        let to_a = ctx.port_artifacts.get(&(to_node, to_port)).copied();

        if let (Some(src), Some(dst)) = (from_a, to_a) {
            // annotate dst
            if let Some(art) = ir.artifacts.get_mut(dst.0 as usize) {
                let k = ir.interner.intern("wired_from".to_string());
                let v = ir.interner.intern(format!("{}#{}", src.0, dst.0));
                art.attrs.insert(k, IrValue::Str(v));
            }
        }
    }
}

fn lower_exports(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, diags: &mut DiagBag) {
    for e in &hir.exports {
        let what = match e.what {
            ResolvedRef::Port(pid) => match ctx.port_map.get(&pid).copied() {
                Some(x) => x,
                None => {
                    diags.push(Diagnostic::error("export references unknown port"));
                    continue;
                }
            },
            _ => {
                diags.push(Diagnostic::error("export must reference an out port"));
                continue;
            }
        };

        let (nid, pname, dir) = what;
        if dir != PortDir::Out {
            diags.push(Diagnostic::error(format!(
                "export must reference an out port: {}.{}",
                hir.name(ir.nodes[nid.0 as usize].bake_name),
                hir.name(pname)
            )));
            continue;
        }

        let art = ctx.port_artifacts.get(&(nid, pname)).copied();
        if let Some(aid) = art {
            ir.exports.push(aid);
        } else {
            diags.push(Diagnostic::error("export: missing artifact for port"));
        }
    }

    // Deterministic unique
    let mut set = BTreeSet::new();
    ir.exports.retain(|a| set.insert(*a));
}

fn lower_plans(hir: &HirProgram, ir: &mut IrProgram, ctx: &mut LowerCtx, opts: &LowerOptions, diags: &mut DiagBag) {
    for p in &hir.plans {
        let mut items = Vec::new();

        for it in &p.items {
            match it {
                PlanItem::RunExports { .. } => items.push(IrPlanItem::RunExports),
                PlanItem::Run { what, .. } => match what {
                    ResolvedRef::Bake(bid) => {
                        match ctx.bake_map.get(bid).copied() {
                            Some(nid) => items.push(IrPlanItem::RunNode(nid)),
                            None => diags.push(Diagnostic::error("plan references unknown bake")),
                        }
                    }
                    ResolvedRef::Port(pid) => {
                        if !opts.allow_plan_run_port {
                            diags.push(Diagnostic::error("plan run port is not allowed"));
                            continue;
                        }
                        // Exécuter node parent
                        match ctx.port_map.get(pid).copied() {
                            Some((nid, _pname, _dir)) => items.push(IrPlanItem::RunNode(nid)),
                            None => diags.push(Diagnostic::error("plan references unknown port")),
                        }
                    }
                    ResolvedRef::Var(_)
                    | ResolvedRef::Tool(_)
                    | ResolvedRef::Profile(_)
                    | ResolvedRef::Store(_)
                    | ResolvedRef::Capsule(_) => {
                        diags.push(Diagnostic::error("plan item reference not supported"));
                    }
                },
            }
        }

        ir.plans.insert(p.name, IrPlan { name: p.name, items, origin: p.origin.clone(), attrs: BTreeMap::new() });
    }
}

/// ------------------------------------------------------------
/// Helpers
/// ------------------------------------------------------------

fn validate_run_bindings(hir: &HirProgram, node: &IrNode, diags: &mut DiagBag) {
    for run in &node.runs {
        // takes: must exist + must be IN
        for (port, _flag) in &run.binding.takes {
            match node.inputs.get(port) {
                Some(p) => {
                    if p.dir != PortDir::In {
                        diags.push(Diagnostic::error(format!(
                            "run takes references non-in port: {}.{}",
                            hir.name(node.bake_name),
                            hir.name(*port)
                        )));
                    }
                }
                None => {
                    diags.push(Diagnostic::error(format!(
                        "run takes references unknown port: {}.{}",
                        hir.name(node.bake_name),
                        hir.name(*port)
                    )));
                }
            }
        }

        // emits: must exist + must be OUT
        for (port, _flag) in &run.binding.emits {
            match node.outputs.get(port) {
                Some(p) => {
                    if p.dir != PortDir::Out {
                        diags.push(Diagnostic::error(format!(
                            "run emits references non-out port: {}.{}",
                            hir.name(node.bake_name),
                            hir.name(*port)
                        )));
                    }
                }
                None => {
                    diags.push(Diagnostic::error(format!(
                        "run emits references unknown port: {}.{}",
                        hir.name(node.bake_name),
                        hir.name(*port)
                    )));
                }
            }
        }
    }
}

fn make_port_artifact(
    hir: &HirProgram,
    ir: &mut IrProgram,
    _nid: IrNodeId,
    bake: NameId,
    port: NameId,
    ty: IrType,
    origin: Origin,
) -> IrArtifactId {
    // name: "<bake>.<port>"
    let s = format!("{}.{}", hir.name(bake), hir.name(port));
    let name = ir.interner.intern(s);
    let id = IrArtifactId(ir.artifacts.len() as u32);
    let a = Artifact::new(id, name, ty, origin);
    ir.artifacts.push(a);
    id
}

fn ensure_make_attached(
    hir: &HirProgram,
    ir: &mut IrProgram,
    node: &mut IrNode,
    pname: NameId,
    ty: IrType,
    origin: Origin,
    ctx: &mut LowerCtx,
) -> IrArtifactId {
    // Si port déjà a un artefact, on l’annote avec make_*.
    let existing = node
        .inputs
        .get(&pname)
        .and_then(|p| p.artifact)
        .or_else(|| node.outputs.get(&pname).and_then(|p| p.artifact));

    if let Some(aid) = existing {
        if let Some(a) = ir.artifacts.get_mut(aid.0 as usize) {
            a.attrs.insert(ir.interner.intern("from_make".to_string()), IrValue::Bool(true));
            a.origin = origin;
        }
        return aid;
    }

    // Sinon, créer et attacher
    let aid = make_port_artifact(hir, ir, node.id, node.bake_name, pname, ty, origin);
    if let Some(p) = node.inputs.get_mut(&pname) {
        p.artifact = Some(aid);
    }
    if let Some(p) = node.outputs.get_mut(&pname) {
        p.artifact = Some(aid);
    }
    ctx.port_artifacts.insert((node.id, pname), aid);
    aid
}

fn find_hir_bake_by_name<'a>(hir: &'a HirProgram, name: NameId) -> Option<&'a Bake> {
    hir.bakes.iter().find(|b| b.name == name)
}

fn intern_join(ir: &mut IrProgram, a: &str, b: &str, sep: &str) -> NameId {
    ir.interner.intern(format!("{a}{sep}{b}"))
}

fn display_ty(hir: &HirProgram, ir: &IrProgram, t: &IrType) -> String {
    match t {
        IrType::Prim(p) => p.as_str().to_string(),
        IrType::Artifact(IrArtifactType { path }) => {
            let mut s = String::new();
            for (i, seg) in path.iter().enumerate() {
                if i != 0 {
                    s.push('.');
                }
                // seg est NameId du (clone) interner hir/ir — on préfère hir pour cohérence
                s.push_str(hir.name(*seg));
            }
            s
        }
    }
}

/// ------------------------------------------------------------
/// Optional: small utilities (why / fingerprint) for downstream
/// ------------------------------------------------------------

/// Build a deterministic “node fingerprint” (cache seed) from tool+bindings+makes.
/// Ce n’est pas la clé finale de cache (qui inclut inputs content), mais un seed stable.
pub fn node_seed(ir: &IrProgram, node: &IrNode) -> CacheKey {
    // Simple deterministic encoding
    let mut s = String::new();
    s.push_str("node\n");
    s.push_str(&format!("name={}\n", ir.name(node.bake_name)));
    s.push_str(&format!("cache={:?}\n", node.cache));

    for m in &node.makes {
        s.push_str(&format!("make={:?}|{}\n", m.kind, ir.name(m.arg)));
    }

    for r in &node.runs {
        s.push_str(&format!("tool={}\n", ir.name(ir.tools[r.tool.0 as usize].name)));
        for (p, f) in &r.binding.takes {
            s.push_str(&format!("takes={}->{}\n", ir.name(*p), ir.name(*f)));
        }
        for (p, f) in &r.binding.emits {
            s.push_str(&format!("emits={}->{}\n", ir.name(*p), ir.name(*f)));
        }
        for (f, v) in &r.binding.sets {
            s.push_str(&format!("set={}->{}\n", ir.name(*f), format_ir_value(ir, v)));
        }
    }

    CacheKey(hash_bytes_fnv1a64(s.as_bytes()))
}

fn format_ir_value(ir: &IrProgram, v: &IrValue) -> String {
    match v {
        IrValue::Str(x) => format!("\"{}\"", ir.name(*x)),
        IrValue::Int(i) => i.to_string(),
        IrValue::Bool(b) => b.to_string(),
        IrValue::Ident(x) => ir.name(*x).to_string(),
        IrValue::Path(x) => format!("path(\"{}\")", ir.name(*x)),
        IrValue::List(xs) => {
            let mut s = String::from("[");
            for (i, x) in xs.iter().enumerate() {
                if i != 0 {
                    s.push_str(", ");
                }
                s.push_str(&format_ir_value(ir, x));
            }
            s.push(']');
            s
        }
    }
}

/// ------------------------------------------------------------
/// Tests (smoke)
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{CacheMode, StoreMode};

    #[test]
    fn lower_smoke_empty_ok() {
        let hir = HirProgram::new(2);
        let mut diags = DiagBag::new();
        let ir = lower_default(&hir, &mut diags);
        assert!(ir.is_some());
    }

    #[test]
    fn lower_store_tool_basic() {
        let mut hir = HirProgram::new(2);
        let mut i = hir.interner.clone();

        let n_store = i.intern("cache");
        let n_path = i.intern("./.steel/store");
        let sid = hir.add_store(n_store, n_path, StoreMode::Content, Origin::none());

        let n_caps = i.intern("cap0");
        let cid = hir.add_capsule(n_caps, Origin::none());

        let n_tool = i.intern("vittec");
        let n_exec = i.intern("vittec");
        let _tid = hir.add_tool(n_tool, n_exec, true, Some(cid), Origin::none());

        // restore interner (builders used self.interner, test uses local i)
        hir.interner = i;

        let mut diags = DiagBag::new();
        let ir = lower_default(&hir, &mut diags).unwrap();
        assert_eq!(ir.stores.len(), 1);
        assert_eq!(ir.tools.len(), 1);
        assert!(!diags.has_error());
        let _ = sid;
    }
}