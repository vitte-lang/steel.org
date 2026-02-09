//! DOT export + utilities for BakeGraph.
//!
//! This module provides a richer DOT renderer than the minimal `to_dot()`
//! helper inside `bake.rs`:
//! - stable node naming (n<id>)
//! - configurable labels (name, tool, outputs)
//! - clusters (group nodes by tool/category)
//! - optional artifact nodes and edges (node->artifact, artifact->node)
//! - styling knobs (kept plaintext; no ANSI).
//!
//! The DOT output is intended for Graphviz:
//!   dot -Tsvg bake.dot > bake.svg
//!   dot -Tpng bake.dot > bake.png

use std::collections::BTreeMap;

use super::bake::{Artifact, ArtifactId, ArtifactKind, BakeGraph, Node, NodeId};

/// DOT rendering options.
#[derive(Debug, Clone)]
pub struct DotOptions {
    /// Include an overall graph label.
    pub graph_label: Option<String>,

    /// If true, include tool name in node labels.
    pub show_tool: bool,

    /// If true, include argv in node labels (may be large).
    pub show_argv: bool,

    /// If true, include outputs list in node labels.
    pub show_outputs: bool,

    /// If true, include inputs list in node labels.
    pub show_inputs: bool,

    /// If true, emit artifacts as DOT nodes and connect them.
    pub include_artifacts: bool,

    /// If true and artifacts included, connect as:
    ///   input artifact -> node -> output artifact
    /// else connect only node dependencies.
    pub artifact_flow_edges: bool,

    /// Group nodes by tool into DOT clusters.
    pub cluster_by_tool: bool,

    /// Max items to show in inputs/outputs lists (truncate with "…").
    pub max_list_items: usize,

    /// Add per-node metadata in label (k=v lines), limited by this count.
    pub max_meta_items: usize,
}

impl Default for DotOptions {
    fn default() -> Self {
        Self {
            graph_label: None,
            show_tool: true,
            show_argv: false,
            show_outputs: true,
            show_inputs: false,
            include_artifacts: false,
            artifact_flow_edges: false,
            cluster_by_tool: true,
            max_list_items: 6,
            max_meta_items: 4,
        }
    }
}

/// DOT exporter.
#[derive(Debug, Clone)]
pub struct DotExporter {
    pub opts: DotOptions,
}

impl Default for DotExporter {
    fn default() -> Self {
        Self {
            opts: DotOptions::default(),
        }
    }
}

impl DotExporter {
    pub fn new(opts: DotOptions) -> Self {
        Self { opts }
    }

    /// Export the graph to DOT.
    pub fn export(&self, g: &BakeGraph) -> String {
        let mut s = String::new();

        s.push_str("digraph bake {\n");
        s.push_str("  rankdir=LR;\n");
        s.push_str("  node [shape=box];\n");
        s.push_str("  edge [arrowsize=0.7];\n");

        if let Some(lbl) = &self.opts.graph_label {
            s.push_str(&format!("  label=\"{}\";\n", esc(lbl)));
            s.push_str("  labelloc=t;\n");
            s.push_str("  fontsize=18;\n");
        }

        if self.opts.cluster_by_tool {
            self.emit_clusters_by_tool(g, &mut s);
        } else {
            self.emit_nodes_flat(g, &mut s);
        }

        if self.opts.include_artifacts {
            self.emit_artifacts(g, &mut s);
        }

        if self.opts.include_artifacts && self.opts.artifact_flow_edges {
            self.emit_artifact_flow_edges(g, &mut s);
        } else {
            self.emit_dep_edges(g, &mut s);
        }

        s.push_str("}\n");
        s
    }

    fn emit_clusters_by_tool(&self, g: &BakeGraph, s: &mut String) {
        // tool -> nodes
        let mut by_tool: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
        for (&nid, n) in &g.nodes {
            by_tool.entry(n.action.tool.clone()).or_default().push(nid);
        }

        let mut cluster_idx = 0usize;
        for (tool, nodes) in by_tool {
            cluster_idx += 1;
            s.push_str(&format!("  subgraph cluster_{cluster_idx} {{\n"));
            s.push_str(&format!("    label=\"{}\";\n", esc(&tool)));
            s.push_str("    style=rounded;\n");
            s.push_str("    color=gray70;\n");

            for nid in nodes {
                if let Some(n) = g.nodes.get(&nid) {
                    self.emit_node(nid, n, g, s, 4);
                }
            }

            s.push_str("  }\n");
        }
    }

    fn emit_nodes_flat(&self, g: &BakeGraph, s: &mut String) {
        for (&nid, n) in &g.nodes {
            self.emit_node(nid, n, g, s, 2);
        }
    }

    fn emit_node(&self, nid: NodeId, n: &Node, g: &BakeGraph, s: &mut String, indent: usize) {
        let pad = " ".repeat(indent);
        let label = self.node_label(n, g);
        s.push_str(&format!(
            "{pad}n{} [label=\"{}\"];\n",
            nid.0,
            esc(&label)
        ));
    }

    fn node_label(&self, n: &Node, g: &BakeGraph) -> String {
        let mut lines: Vec<String> = Vec::new();

        lines.push(n.name.clone());

        if self.opts.show_tool {
            lines.push(format!("tool: {}", n.action.tool));
        }

        if self.opts.show_argv {
            lines.push(format!("argv: {}", n.action.argv.join(" ")));
        }

        if self.opts.show_inputs {
            let list = ids_to_names(&n.inputs, &g.artifacts, self.opts.max_list_items);
            if !list.is_empty() {
                lines.push(format!("in: {list}"));
            }
        }

        if self.opts.show_outputs {
            let list = ids_to_names(&n.outputs, &g.artifacts, self.opts.max_list_items);
            if !list.is_empty() {
                lines.push(format!("out: {list}"));
            }
        }

        if !n.meta.is_empty() && self.opts.max_meta_items > 0 {
            let mut shown = 0usize;
            for (k, v) in &n.meta {
                lines.push(format!("{k}={v}"));
                shown += 1;
                if shown >= self.opts.max_meta_items {
                    if n.meta.len() > shown {
                        lines.push("…".into());
                    }
                    break;
                }
            }
        }

        lines.join("\\n")
    }

    fn emit_artifacts(&self, g: &BakeGraph, s: &mut String) {
        // Artifacts as ellipse nodes.
        s.push_str("  // artifacts\n");
        s.push_str("  node [shape=ellipse];\n");

        for (aid, a) in &g.artifacts {
            let name = artifact_display(a);
            let style = match a.kind {
                ArtifactKind::Source => "filled",
                ArtifactKind::Intermediate => "solid",
                ArtifactKind::Output => "bold",
                ArtifactKind::Meta => "dashed",
                ArtifactKind::External => "dotted",
            };
            s.push_str(&format!(
                "  a{} [label=\"{}\", style={}];\n",
                aid.0,
                esc(&format!("{name}\\n({})", artifact_kind_str(&a.kind))),
                style
            ));
        }

        // restore defaults for nodes
        s.push_str("  node [shape=box];\n");
    }

    fn emit_dep_edges(&self, g: &BakeGraph, s: &mut String) {
        s.push_str("  // node dependency edges\n");
        for (b, ds) in &g.deps {
            for a in ds {
                s.push_str(&format!("  n{} -> n{};\n", a.0, b.0));
            }
        }
    }

    fn emit_artifact_flow_edges(&self, g: &BakeGraph, s: &mut String) {
        s.push_str("  // artifact flow edges\n");

        // For each node: input artifact -> node, node -> output artifact
        for (&nid, n) in &g.nodes {
            for &inp in &n.inputs {
                s.push_str(&format!("  a{} -> n{};\n", inp.0, nid.0));
            }
            for &out in &n.outputs {
                s.push_str(&format!("  n{} -> a{};\n", nid.0, out.0));
            }
        }

        // Keep explicit deps too, but styled as dashed (optional).
        s.push_str("  // explicit deps (dashed)\n");
        for (b, ds) in &g.deps {
            for a in ds {
                s.push_str(&format!("  n{} -> n{} [style=dashed];\n", a.0, b.0));
            }
        }
    }
}

/* -------------------------------- Helpers -------------------------------- */

fn artifact_kind_str(k: &ArtifactKind) -> &'static str {
    match k {
        ArtifactKind::Source => "source",
        ArtifactKind::Intermediate => "intermediate",
        ArtifactKind::Output => "output",
        ArtifactKind::Meta => "meta",
        ArtifactKind::External => "external",
    }
}

fn artifact_display(a: &Artifact) -> String {
    if let Some(p) = &a.path {
        p.to_string_lossy().to_string()
    } else if let Some(l) = &a.logical {
        l.clone()
    } else {
        format!("artifact:{:?}", a.id)
    }
}

fn ids_to_names(
    ids: &[ArtifactId],
    map: &BTreeMap<ArtifactId, Artifact>,
    max: usize,
) -> String {
    let mut names: Vec<String> = Vec::new();
    for &id in ids {
        if let Some(a) = map.get(&id) {
            names.push(artifact_display(a));
        } else {
            names.push(format!("<?>:{:?}", id));
        }
    }
    if names.len() > max && max > 0 {
        names.truncate(max);
        names.push("…".into());
    }
    names.join(", ")
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::bake::{Action, Artifact, ArtifactKind, BakeGraph, Node};

    #[test]
    fn dot_exports_graph() {
        let mut g = BakeGraph::new();
        let src = Artifact::source("src/a.c");
        let obj = Artifact::logical("obj/a.o", ArtifactKind::Intermediate);
        let exe = Artifact::output("bin/a");

        g.add_artifact(src.clone());
        g.add_artifact(obj.clone());
        g.add_artifact(exe.clone());

        let n1 = Node::new("compile", Action::new("clang")).input(&src).output(&obj);
        let n2 = Node::new("link", Action::new("clang")).input(&obj).output(&exe);

        let id1 = g.add_node(n1);
        let id2 = g.add_node(n2);

        g.infer_deps_from_artifacts().unwrap();
        assert_eq!(g.topo_order().unwrap(), vec![id1, id2]);

        let dot = DotExporter::default().export(&g);
        assert!(dot.contains("digraph bake"));
        assert!(dot.contains("cluster_"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn dot_with_artifacts() {
        let mut g = BakeGraph::new();
        let src = Artifact::source("src/a.c");
        g.add_artifact(src.clone());
        let n1 = Node::new("compile", Action::new("clang")).input(&src);
        g.add_node(n1);

        let dot = DotExporter::new(DotOptions {
            include_artifacts: true,
            artifact_flow_edges: true,
            ..DotOptions::default()
        })
        .export(&g);

        assert!(dot.contains("a")); // artifact node prefix
    }
}
