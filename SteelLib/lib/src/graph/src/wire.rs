//! Wiring helpers: connect types/graph/planning/exports into a coherent API.
//!
//! This module is a thin façade that composes:
//! - `BakeGraph` construction helpers (add node/artifact, infer deps)
//! - `Plan` creation helpers (targets -> plan)
//! - exporters (DOT/JSON)
//!
//! It is meant to be used by higher-level layers (manifest/buildfile loaders,
//! executor) without re-implementing boilerplate glue.

use std::path::Path;
use std::sync::Arc;

use super::bake::{Action, Artifact, ArtifactId, ArtifactKind, BakeGraph, GraphError, Node, NodeId};
use super::dot::{DotExporter, DotOptions};
use super::json::{GraphJson, JsonError};
use super::plan::Plan;

/// A convenient builder for `BakeGraph`.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    g: BakeGraph,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self { g: BakeGraph::new() }
    }

    pub fn graph(&self) -> &BakeGraph {
        &self.g
    }

    pub fn graph_mut(&mut self) -> &mut BakeGraph {
        &mut self.g
    }

    pub fn into_graph(self) -> BakeGraph {
        self.g
    }

    pub fn add_artifact(&mut self, a: Artifact) -> ArtifactId {
        self.g.add_artifact(a)
    }

    pub fn add_source(&mut self, path: impl Into<std::path::PathBuf>) -> Artifact {
        let a = Artifact::source(path);
        self.g.add_artifact(a.clone());
        a
    }

    pub fn add_output(&mut self, path: impl Into<std::path::PathBuf>) -> Artifact {
        let a = Artifact::output(path);
        self.g.add_artifact(a.clone());
        a
    }

    pub fn add_logical(&mut self, name: impl Into<String>, kind: ArtifactKind) -> Artifact {
        let a = Artifact::logical(name, kind);
        self.g.add_artifact(a.clone());
        a
    }

    pub fn add_node(&mut self, n: Node) -> NodeId {
        self.g.add_node(n)
    }

    pub fn dep(&mut self, before: NodeId, after: NodeId) -> Result<(), GraphError> {
        self.g.add_dep(before, after)
    }

    pub fn infer_deps(&mut self) -> Result<(), GraphError> {
        self.g.infer_deps_from_artifacts()
    }

    pub fn topo(&self) -> Result<Vec<NodeId>, GraphError> {
        self.g.topo_order()
    }
}

/// Common high-level operations.
pub struct GraphOps;

impl GraphOps {
    /// Build a plan for specific output artifacts.
    pub fn plan_for_outputs(g: &BakeGraph, outs: &[ArtifactId]) -> Result<Plan, GraphError> {
        Plan::for_outputs(g, outs)
    }

    /// Build a plan for the whole graph.
    pub fn plan_all(g: &BakeGraph) -> Result<Plan, GraphError> {
        Plan::all(g)
    }

    /// Export DOT.
    pub fn to_dot(g: &BakeGraph, opts: Option<DotOptions>) -> String {
        let ex = DotExporter::new(opts.unwrap_or_default());
        ex.export(g)
    }

    /// Export JSON (pretty).
    pub fn to_json_pretty(g: &BakeGraph) -> String {
        GraphJson::from_graph(g).to_string_pretty()
    }

    /// Export JSON (compact).
    pub fn to_json_compact(g: &BakeGraph) -> String {
        GraphJson::from_graph(g).to_string_compact()
    }

    /// Parse JSON -> graph.
    pub fn from_json(s: &str) -> Result<BakeGraph, JsonError> {
        GraphJson::parse(s)?.into_graph()
    }
}

/* ------------------------- Higher-level patterns ------------------------- */

/// A typical pattern: build a graph for a C project with compile+link nodes.
///
/// This is intentionally simplistic and does not compute cache keys.
pub fn wire_c_like_project(
    sources: &[impl AsRef<Path>],
    out_exe: impl AsRef<Path>,
) -> Result<(BakeGraph, ArtifactId), GraphError> {
    let mut b = GraphBuilder::new();

    let mut objs = Vec::new();
    for src in sources {
        let src_art = b.add_source(src.as_ref().to_path_buf());
        let obj_art = b.add_logical(
            format!("obj/{}.o", file_stem(src.as_ref())),
            ArtifactKind::Intermediate,
        );

        let compile = Node::new(
            format!("compile:{}", src.as_ref().display()),
            Action::new("clang").arg("-c").arg(src.as_ref().display().to_string()),
        )
        .input(&src_art)
        .output(&obj_art);

        b.add_node(compile);
        objs.push(obj_art);
    }

    let exe_art = b.add_output(out_exe.as_ref().to_path_buf());
    let mut link = Node::new("link", Action::new("clang"));
    for o in &objs {
        link.inputs.push(o.id);
    }
    link.outputs.push(exe_art.id);

    b.add_node(link);

    b.infer_deps()?;

    Ok((b.into_graph(), exe_art.id))
}

fn file_stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out")
        .to_string()
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_basic() {
        let mut b = GraphBuilder::new();
        let src = b.add_source("src/a.c");
        let obj = b.add_logical("obj/a.o", ArtifactKind::Intermediate);

        let n = Node::new("compile", Action::new("clang").arg("-c"))
            .input(&src)
            .output(&obj);
        b.add_node(n);

        b.infer_deps().unwrap();
        assert_eq!(b.graph().nodes.len(), 1);
    }

    #[test]
    fn wire_c_like_ok() {
        let (g, exe) = wire_c_like_project(&["a.c", "b.c"], "app").unwrap();
        assert!(g.artifacts.contains_key(&exe));
        assert!(g.nodes.len() >= 2);
    }
}
