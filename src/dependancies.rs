// /Users/vincent/Documents/Github/steel/src/dependancies.rs
//! dependancies — dependency graph validation + utilities (std-only)
//!
//! This module focuses on *graph-level* invariants used by Steel:
//! - cycle detection
//! - topological ordering (deterministic)
//! - missing nodes detection
//! - duplicate edges normalization
//!
//! It is intentionally independent from any particular Workspace model.
//! You provide a node set and an adjacency list (edges).
//!
//! Determinism: node ordering is lexicographic by node id (String).
//!
//! Note: filename kept as `dependancies.rs` to match existing repo naming.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub type NodeId = String;

/// Directed graph: node -> sorted set of outgoing neighbors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiGraph {
    pub edges: BTreeMap<NodeId, BTreeSet<NodeId>>,
}

impl DiGraph {
    pub fn new() -> Self {
        Self { edges: BTreeMap::new() }
    }

    /// Ensure node exists (even if it has no edges).
    pub fn add_node(&mut self, id: impl Into<NodeId>) {
        self.edges.entry(id.into()).or_default();
    }

    /// Add directed edge `from -> to`.
    pub fn add_edge(&mut self, from: impl Into<NodeId>, to: impl Into<NodeId>) {
        let from = from.into();
        let to = to.into();
        self.edges.entry(from.clone()).or_default().insert(to.clone());
        self.edges.entry(to).or_default(); // ensure `to` exists in node set
    }

    /// Return a deterministic set of nodes.
    pub fn nodes(&self) -> BTreeSet<NodeId> {
        self.edges.keys().cloned().collect()
    }

    /// Return a deterministic list of edges (from, to).
    pub fn edge_list(&self) -> Vec<(NodeId, NodeId)> {
        let mut v = Vec::new();
        for (from, tos) in &self.edges {
            for to in tos {
                v.push((from.clone(), to.clone()));
            }
        }
        v
    }

    /// Build reverse adjacency map.
    pub fn reverse(&self) -> DiGraph {
        let mut r = DiGraph::new();
        for (from, tos) in &self.edges {
            r.add_node(from.clone());
            for to in tos {
                r.add_edge(to.clone(), from.clone());
            }
        }
        r
    }

    /// Compute indegrees for all nodes.
    pub fn indegrees(&self) -> BTreeMap<NodeId, usize> {
        let mut indeg: BTreeMap<NodeId, usize> = BTreeMap::new();
        for n in self.edges.keys() {
            indeg.insert(n.clone(), 0);
        }
        for (_from, tos) in &self.edges {
            for to in tos {
                *indeg.entry(to.clone()).or_insert(0) += 1;
            }
        }
        indeg
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub node: Option<NodeId>,
}

impl Diagnostic {
    pub fn err(code: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Error, code, message: msg.into(), node: None }
    }
    pub fn warn(code: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, code, message: msg.into(), node: None }
    }
    pub fn info(code: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Info, code, message: msg.into(), node: None }
    }
    pub fn with_node(mut self, n: impl Into<NodeId>) -> Self {
        self.node = Some(n.into());
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.diagnostics {
            let sev = match d.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            };
            if let Some(n) = &d.node {
                writeln!(f, "[{sev}] {}: {} (node={})", d.code, d.message, n)?;
            } else {
                writeln!(f, "[{sev}] {}: {}", d.code, d.message)?;
            }
        }
        Ok(())
    }
}

/// Validate a dependency graph:
/// - cycles (error)
/// - self-deps (error)
/// - missing nodes (error) if `declared_nodes` provided
pub fn validate_graph(graph: &DiGraph, declared_nodes: Option<&BTreeSet<NodeId>>) -> Report {
    let mut r = Report::default();

    // Missing nodes (if declared list supplied)
    if let Some(declared) = declared_nodes {
        for n in declared {
            if !graph.edges.contains_key(n) {
                r.push(Diagnostic::err("DEP_NODE_MISSING", "declared node missing from graph").with_node(n.clone()));
            }
        }
        for n in graph.edges.keys() {
            if !declared.contains(n) {
                r.push(Diagnostic::warn("DEP_NODE_UNDECLARED", "node exists in graph but not declared").with_node(n.clone()));
            }
        }
    }

    // Self dependencies
    for (from, tos) in &graph.edges {
        if tos.contains(from) {
            r.push(Diagnostic::err("DEP_SELF", "node depends on itself").with_node(from.clone()));
        }
    }

    // Cycle detection
    if let Some(cycle) = find_cycle(graph) {
        let mut msg = String::new();
        for (i, n) in cycle.iter().enumerate() {
            if i > 0 { msg.push_str(" -> "); }
            msg.push_str(n);
        }
        r.push(Diagnostic::err("DEP_CYCLE", format!("cycle detected: {msg}")));
    }

    r
}

/// Deterministic topological sort using Kahn.
/// Returns error if cycle exists.
pub fn topo_sort(graph: &DiGraph) -> std::result::Result<Vec<NodeId>, TopoError> {
    let mut indeg = graph.indegrees();

    // Use a deterministic min-queue by lexicographic ordering.
    let mut ready: BTreeSet<NodeId> = BTreeSet::new();
    for (n, d) in &indeg {
        if *d == 0 {
            ready.insert(n.clone());
        }
    }
    let mut out = Vec::with_capacity(indeg.len());

    while let Some(n) = ready.iter().next().cloned() {
        ready.remove(&n);
        out.push(n.clone());
        if let Some(tos) = graph.edges.get(&n) {
            for to in tos {
                if let Some(d) = indeg.get_mut(to) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        ready.insert(to.clone());
                    }
                }
            }
        }
    }

    if out.len() != indeg.len() {
        return Err(TopoError::Cycle);
    }

    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopoError {
    Cycle,
}

/// Find any cycle (deterministic) using DFS coloring.
/// Returns the cycle path including repeated start at end (classic).
pub fn find_cycle(graph: &DiGraph) -> Option<Vec<NodeId>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color { White, Gray, Black }

    let mut color: BTreeMap<NodeId, Color> = BTreeMap::new();
    for n in graph.edges.keys() {
        color.insert(n.clone(), Color::White);
    }

    let mut parent: BTreeMap<NodeId, NodeId> = BTreeMap::new();

    fn dfs(
        graph: &DiGraph,
        u: &NodeId,
        color: &mut BTreeMap<NodeId, Color>,
        parent: &mut BTreeMap<NodeId, NodeId>,
    ) -> Option<Vec<NodeId>> {
        color.insert(u.clone(), Color::Gray);

        let tos = graph.edges.get(u);
        if let Some(tos) = tos {
            for v in tos {
                let vc = *color.get(v).unwrap_or(&Color::White);
                if vc == Color::White {
                    parent.insert(v.clone(), u.clone());
                    if let Some(c) = dfs(graph, v, color, parent) {
                        return Some(c);
                    }
                } else if vc == Color::Gray {
                    // Found back-edge u -> v, reconstruct cycle.
                    let mut cycle = Vec::new();
                    cycle.push(v.clone());
                    let mut x = u.clone();
                    while x != *v {
                        cycle.push(x.clone());
                        x = parent.get(&x).cloned().unwrap_or_else(|| v.clone());
                        if cycle.len() > graph.edges.len() + 2 {
                            break; // safety
                        }
                    }
                    cycle.push(v.clone());
                    cycle.reverse();
                    return Some(cycle);
                }
            }
        }

        color.insert(u.clone(), Color::Black);
        None
    }

    // Deterministic start order
    let mut nodes: Vec<NodeId> = graph.edges.keys().cloned().collect();
    nodes.sort();

    for n in nodes {
        if *color.get(&n).unwrap_or(&Color::White) == Color::White {
            if let Some(c) = dfs(graph, &n, &mut color, &mut parent) {
                return Some(c);
            }
        }
    }

    None
}

/// Convenience: build a graph from explicit lists.
pub fn graph_from_edges(nodes: impl IntoIterator<Item = NodeId>, edges: impl IntoIterator<Item = (NodeId, NodeId)>) -> DiGraph {
    let mut g = DiGraph::new();
    for n in nodes {
        g.add_node(n);
    }
    for (a, b) in edges {
        g.add_edge(a, b);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_sort_ok() {
        let g = graph_from_edges(
            vec!["a".into(), "b".into(), "c".into()],
            vec![("a".into(), "b".into()), ("a".into(), "c".into())],
        );
        let order = topo_sort(&g).unwrap();
        // a must come before b and c
        let ia = order.iter().position(|x| x == "a").unwrap();
        let ib = order.iter().position(|x| x == "b").unwrap();
        let ic = order.iter().position(|x| x == "c").unwrap();
        assert!(ia < ib);
        assert!(ia < ic);
    }

    #[test]
    fn detects_cycle() {
        let g = graph_from_edges(
            vec!["a".into(), "b".into()],
            vec![("a".into(), "b".into()), ("b".into(), "a".into())],
        );
        assert!(topo_sort(&g).is_err());
        let c = find_cycle(&g).unwrap();
        assert!(c.len() >= 3);
    }

    #[test]
    fn validate_self_dep() {
        let mut g = DiGraph::new();
        g.add_edge("a", "a");
        let r = validate_graph(&g, None);
        assert!(r.has_errors());
    }
}
