// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\model\graph.rs

#![allow(clippy::type_complexity)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

use super::artifact::{Artifact, ArtifactKind};

/// Stable identifier for nodes in the build/config graph.
///
/// Use small, opaque IDs for determinism and tooling friendliness.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Node category (typed node).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A tool invocation node (compile/link/archive/test/package).
    Tool,
    /// A build step node (language-agnostic).
    Step,
    /// A configuration / planning node (profile/target selection).
    Config,
    /// A package/workspace node.
    Package,
    /// Any custom node.
    Other(String),
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Tool => f.write_str("tool"),
            NodeKind::Step => f.write_str("step"),
            NodeKind::Config => f.write_str("config"),
            NodeKind::Package => f.write_str("package"),
            NodeKind::Other(s) => write!(f, "other({})", s),
        }
    }
}

/// Port direction. Used to describe how artifacts flow through nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDir {
    In,
    Out,
}

impl fmt::Display for PortDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortDir::In => f.write_str("in"),
            PortDir::Out => f.write_str("out"),
        }
    }
}

/// A named port on a node (e.g., "src", "obj", "exe").
///
/// Ports are stable user-level names, and they carry type information (`ArtifactKind`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub dir: PortDir,
    /// Expected kind on this port.
    pub kind: ArtifactKind,
    /// If true, port is variadic / can take multiple artifacts.
    #[serde(default)]
    pub many: bool,
    /// Deterministic metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Port {
    pub fn input(name: impl Into<String>, kind: ArtifactKind) -> Self {
        Self {
            name: name.into(),
            dir: PortDir::In,
            kind,
            many: false,
            meta: BTreeMap::new(),
        }
    }

    pub fn output(name: impl Into<String>, kind: ArtifactKind) -> Self {
        Self {
            name: name.into(),
            dir: PortDir::Out,
            kind,
            many: false,
            meta: BTreeMap::new(),
        }
    }

    pub fn many(mut self, v: bool) -> Self {
        self.many = v;
        self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

/// A graph node: holds ports + configuration metadata.
/// This node is language-agnostic and suitable for exporting to dot/json.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,

    /// Stable ports in deterministic order.
    #[serde(default)]
    pub ports: Vec<Port>,

    /// Deterministic metadata for tooling.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Node {
    pub fn new(id: impl Into<String>, kind: NodeKind, title: impl Into<String>) -> Self {
        Self {
            id: NodeId::new(id),
            kind,
            title: title.into(),
            ports: Vec::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn with_port(mut self, port: Port) -> Self {
        self.ports.push(port);
        self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }

    pub fn port(&self, name: &str, dir: PortDir) -> Option<&Port> {
        self.ports.iter().find(|p| p.name == name && p.dir == dir)
    }
}

/// Edge endpoint: node + port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: NodeId,
    pub port: String,
}

impl Endpoint {
    pub fn new(node: NodeId, port: impl Into<String>) -> Self {
        Self {
            node,
            port: port.into(),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.node, self.port)
    }
}

/// An edge in the graph connecting output port -> input port, optionally carrying artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: Endpoint,
    pub to: Endpoint,

    /// Optional artifacts flowing over the edge (expanded globs, resolved outputs, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,

    /// Deterministic metadata (ex: "wire" rules, selection reasons).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Edge {
    pub fn new(from: Endpoint, to: Endpoint) -> Self {
        Self {
            from,
            to,
            artifacts: Vec::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn with_artifact(mut self, a: Artifact) -> Self {
        self.artifacts.push(a);
        self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

/// A deterministic directed graph for Steel / build planning.
///
/// - Nodes and edges are stored in stable orders (BTreeMap/BTreeSet patterns)
/// - Suitable for dot/json export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    /// Nodes keyed by id for stable lookup.
    pub nodes: BTreeMap<NodeId, Node>,

    /// Edges in insertion order (kept as Vec but must be built deterministically by caller).
    pub edges: Vec<Edge>,

    /// Graph-level metadata (schema, root, profile, target, host info).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            meta: BTreeMap::new(),
        }
    }
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Basic validation:
    /// - endpoints exist
    /// - ports exist
    /// - port directions: from=Out, to=In
    pub fn validate(&self) -> Result<(), GraphError> {
        for (idx, e) in self.edges.iter().enumerate() {
            let from_node = self
                .nodes
                .get(&e.from.node)
                .ok_or_else(|| GraphError::UnknownNode {
                    edge_index: idx,
                    node: e.from.node.to_string(),
                })?;
            let to_node = self
                .nodes
                .get(&e.to.node)
                .ok_or_else(|| GraphError::UnknownNode {
                    edge_index: idx,
                    node: e.to.node.to_string(),
                })?;

            let from_port = from_node
                .port(&e.from.port, PortDir::Out)
                .ok_or_else(|| GraphError::UnknownPort {
                    edge_index: idx,
                    endpoint: e.from.to_string(),
                    expected_dir: PortDir::Out,
                })?;

            let to_port = to_node
                .port(&e.to.port, PortDir::In)
                .ok_or_else(|| GraphError::UnknownPort {
                    edge_index: idx,
                    endpoint: e.to.to_string(),
                    expected_dir: PortDir::In,
                })?;

            // Type compatibility: allow exact match, or allow "Other" to pass through.
            if !port_kind_compatible(&from_port.kind, &to_port.kind) {
                return Err(GraphError::IncompatiblePorts {
                    edge_index: idx,
                    from: e.from.to_string(),
                    to: e.to.to_string(),
                    from_kind: from_port.kind.to_string(),
                    to_kind: to_port.kind.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Topological sort over nodes using edges (from -> to).
    ///
    /// Note: this collapses ports and sorts at the node level.
    pub fn topo_sort(&self) -> Result<Vec<NodeId>, GraphError> {
        // Build adjacency and indegree.
        let mut indeg: BTreeMap<NodeId, usize> = self
            .nodes
            .keys()
            .cloned()
            .map(|k| (k, 0usize))
            .collect();

        let mut adj: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for e in &self.edges {
            adj.entry(e.from.node.clone())
                .or_default()
                .insert(e.to.node.clone());
        }

        for (_, tos) in &adj {
            for to in tos {
                if let Some(v) = indeg.get_mut(to) {
                    *v += 1;
                }
            }
        }

        let mut q = VecDeque::new();
        for (n, d) in &indeg {
            if *d == 0 {
                q.push_back(n.clone());
            }
        }

        let mut out = Vec::with_capacity(self.nodes.len());
        while let Some(n) = q.pop_front() {
            out.push(n.clone());
            if let Some(tos) = adj.get(&n) {
                for to in tos.iter() {
                    let d = indeg.get_mut(to).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        q.push_back(to.clone());
                    }
                }
            }
        }

        if out.len() != self.nodes.len() {
            return Err(GraphError::CycleDetected);
        }
        Ok(out)
    }
}

/// Graph validation errors.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("unknown node referenced by edge[{edge_index}]: {node}")]
    UnknownNode { edge_index: usize, node: String },

    #[error("unknown port referenced by edge[{edge_index}]: {endpoint} (expected dir={expected_dir})")]
    UnknownPort {
        edge_index: usize,
        endpoint: String,
        expected_dir: PortDir,
    },

    #[error("incompatible ports on edge[{edge_index}]: {from} ({from_kind}) -> {to} ({to_kind})")]
    IncompatiblePorts {
        edge_index: usize,
        from: String,
        to: String,
        from_kind: String,
        to_kind: String,
    },

    #[error("cycle detected in graph")]
    CycleDetected,
}

fn port_kind_compatible(from: &ArtifactKind, to: &ArtifactKind) -> bool {
    // exact matches
    if from == to {
        return true;
    }
    // allow "other" to pass as wildcard-ish, but keep explicit for deterministic tooling.
    matches!(from, ArtifactKind::Other(_)) || matches!(to, ArtifactKind::Other(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::artifact::ArtifactKind;

    #[test]
    fn validate_ok() {
        let mut g = Graph::new();

        let n1 = Node::new("tool.cc", NodeKind::Tool, "cc")
            .with_port(Port::output("obj", ArtifactKind::Object).many(true));
        let n2 = Node::new("tool.ld", NodeKind::Tool, "ld")
            .with_port(Port::input("obj", ArtifactKind::Object).many(true))
            .with_port(Port::output("exe", ArtifactKind::Executable));

        g.add_node(n1);
        g.add_node(n2);

        g.add_edge(Edge::new(
            Endpoint::new(NodeId::new("tool.cc"), "obj"),
            Endpoint::new(NodeId::new("tool.ld"), "obj"),
        ));

        g.validate().unwrap();
        let order = g.topo_sort().unwrap();
        assert_eq!(order[0].to_string(), "tool.cc");
        assert_eq!(order[1].to_string(), "tool.ld");
    }

    #[test]
    fn cycle_detected() {
        let mut g = Graph::new();
        let a = Node::new("a", NodeKind::Step, "a")
            .with_port(Port::output("out", ArtifactKind::Other("x".into())));
        let b = Node::new("b", NodeKind::Step, "b")
            .with_port(Port::input("in", ArtifactKind::Other("x".into())))
            .with_port(Port::output("out", ArtifactKind::Other("x".into())));
        g.add_node(a);
        g.add_node(b);
        g.add_edge(Edge::new(
            Endpoint::new(NodeId::new("a"), "out"),
            Endpoint::new(NodeId::new("b"), "in"),
        ));
        g.add_edge(Edge::new(
            Endpoint::new(NodeId::new("b"), "out"),
            Endpoint::new(NodeId::new("a"), "out"),
        ));
        assert!(matches!(g.topo_sort(), Err(GraphError::CycleDetected)));
    }
}
