// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\resolve\mod.rs

//! Resolution pipeline.
//!
//! This module orchestrates the deterministic "configure" phase:
//! - variables: collect + validate + precedence (defaults/env/config/cli)
//! - expansion: interpolate `${VAR}` in strings/paths
//! - implicits: inject minimal implicit defaults/rules
//!
//! Output of this layer is a fully explicit, stable model (Graph + metadata)
//! ready to be emitted as `target/steel/config.mub`.
//!
//! Notes:
//! - No `.mff` outputs: the frozen contract artifact is `config.mub`.

pub mod expand;
pub mod implicit;
pub mod variable;

use crate::{
    error::SteelError,
    model::{
        graph::Graph,
        target::{Target, TargetKind},
    },
};
use std::{collections::BTreeMap, path::PathBuf};

pub use expand::{expand_list_dedup_sorted, expand_map, expand_path, expand_str, ExpandCtx};
pub use implicit::{apply_graph_implicits, apply_implicit_meta, apply_target_implicits};
pub use variable::{resolve_vars, VarSet};

/// Inputs for the resolve phase.
/// Typically built from parsing `steelconf` + CLI flags.
#[derive(Debug, Clone, Default)]
pub struct ResolveInput {
    /// Workspace root (absolute or repo-relative; callers decide).
    pub root: PathBuf,

    /// Selected target triple (optional).
    pub triple: Option<String>,

    /// Selected profile name (optional).
    pub profile: Option<String>,

    /// Variables explicitly declared in steelconf (KEY -> VALUE).
    pub config_vars: BTreeMap<String, String>,

    /// CLI overrides `-D KEY=VALUE` (highest priority).
    pub cli_vars: BTreeMap<String, String>,
}

/// Output of resolution: explicit model ready for emission.
#[derive(Debug, Clone)]
pub struct ResolveOutput {
    pub vars: VarSet,
    pub expand: ExpandCtx,
    pub graph: Graph,
}

/// Main entry point: resolve a graph with deterministic configuration.
///
/// Expected call order:
/// 1) build/parse an initial Graph (nodes/edges/tags) from steelconf
/// 2) call `resolve_graph(...)`
/// 3) emit via `output::mub::write_mub_file(...)`
pub fn resolve_graph(mut graph: Graph, input: &ResolveInput) -> Result<ResolveOutput, SteelError> {
    // 1) Variables
    let vars = resolve_vars(&input.config_vars, &input.cli_vars)?;

    // 2) Build expansion context
    let mut ctx = ExpandCtx::new(input.root.clone());

    // push variables into ctx
    for (k, v) in vars.as_map().iter() {
        ctx.set_var(k.clone(), v.clone());
    }

    if let Some(p) = &input.profile {
        ctx.set_profile(p.clone());
    } else if let Some(p) = vars.get("PROFILE") {
        ctx.set_profile(p.clone());
    }

    if let Some(t) = &input.triple {
        ctx.set_triple(t.clone());
    } else if let Some(t) = vars.get("TRIPLE") {
        ctx.set_triple(t.clone());
    }

    // 3) Expand graph metadata + node fields that carry paths/strings.
    //    (Keep this minimal: only expand known keys to preserve semantics.)
    expand_graph_in_place(&mut graph, &ctx)?;

    // 4) Apply implicit defaults/rules (never override explicit user nodes/ports)
    apply_graph_implicits(&mut graph)?;

    // 5) Ensure stable meta keys for tooling
    apply_implicit_meta(
        &mut graph.meta,
        &[
            ("schema", "steel.graph/1"),
            ("target_dir", "target"),
        ],
    );

    // Propagate some canonical meta (if not present)
    if let Some(p) = ctx.profile.clone() {
        graph.meta.entry("profile".into()).or_insert(p);
    }
    if let Some(t) = ctx.triple.clone() {
        graph.meta.entry("triple".into()).or_insert(t);
    }
    graph.meta
        .entry("root".into())
        .or_insert_with(|| ctx.root.to_string_lossy().to_string());

    Ok(ResolveOutput {
        vars,
        expand: ctx,
        graph,
    })
}

/// Resolve a Target record (if you model targets separately).
pub fn resolve_target(mut target: Target, input: &ResolveInput) -> Result<Target, SteelError> {
    // Variables exist mostly for path interpolation; apply implicits here.
    apply_target_implicits(&mut target)?;

    // Ensure defaults if missing (example)
    if target.kind.is_none() {
        target.kind = Some(TargetKind::Binary);
    }

    // Expand any user strings/paths in target fields (if present in your model).
    // Keep stubby unless you have concrete fields to expand.

    Ok(target)
}

// --- internal helpers -------------------------------------------------------

fn expand_graph_in_place(graph: &mut Graph, ctx: &ExpandCtx) -> Result<(), SteelError> {
    // Expand graph-level metadata values
    let new_meta = expand_map(&graph.meta, ctx)?;
    graph.meta = new_meta;

    // Expand node metadata; also expand known string fields (label/subkind) if you allow it.
    // Conservative: do not expand IDs (must remain stable).
    for (_id, node) in graph.nodes.iter_mut() {
        node.meta = expand_map(&node.meta, ctx)?;

        // Optional: expand label/subkind if you treat them as user strings.
        node.label = expand_str(&node.label, ctx)?;
        node.subkind = match node.subkind.take() {
            Some(s) => Some(expand_str(&s, ctx)?),
            None => None,
        };

        // Expand ports metadata if you store paths/args there
        for p in node.ports.values_mut() {
            p.meta = expand_map(&p.meta, ctx)?;
        }
    }

    // Expand edge metadata if any
    for e in graph.edges.iter_mut() {
        e.meta = expand_map(&e.meta, ctx)?;
    }

    Ok(())
}
