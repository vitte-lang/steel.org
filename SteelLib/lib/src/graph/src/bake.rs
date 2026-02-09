//! Graph bake: build graph planning + execution model.
//!
//! This module models Steel's build graph as nodes (artifacts / steps) and edges (deps).
//!
//! The focus here is on:
//! - deterministic node IDs (stable hashing)
//! - explicit inputs/outputs
//! - caching keys (content hash + config hash)
//! - topological scheduling metadata
//!
//! This is NOT the full executor; it's the graph representation and planning helpers.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// A stable identifier for a node in the build graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// A stable identifier for an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(pub u64);

/// The "kind" of artifact produced/consumed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactKind {
    /// Source file (input).
    Source,
    /// Intermediate (object files, generated code, etc.).
    Intermediate,
    /// Final binary or bundle.
    Output,
    /// Metadata (lockfiles, dep graphs, manifests).
    Meta,
    /// External or virtual artifact (e.g. "toolchain:clang").
    External,
}

/// An artifact in the build graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub path: Option<PathBuf>,
    pub logical: Option<String>, // virtual name
    pub meta: BTreeMap<String, String>,
}

impl Artifact {
    pub fn source(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            id: ArtifactId(stable_hash64(&("source", path.to_string_lossy().as_ref()))),
            kind: ArtifactKind::Source,
            path: Some(path),
            logical: None,
            meta: BTreeMap::new(),
        }
    }

    pub fn output(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            id: ArtifactId(stable_hash64(&("output", path.to_string_lossy().as_ref()))),
            kind: ArtifactKind::Output,
            path: Some(path),
            logical: None,
            meta: BTreeMap::new(),
        }
    }

    pub fn logical(name: impl Into<String>, kind: ArtifactKind) -> Self {
        let name = name.into();
        Self {
            id: ArtifactId(stable_hash64(&("logical", name.as_str()))),
            kind,
            path: None,
            logical: Some(name),
            meta: BTreeMap::new(),
        }
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

/// A build action (command/tool invocation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub tool: String,              // e.g. "clang", "vittec", "steel"
    pub argv: Vec<String>,         // argv[0] is tool or subcommand
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub description: Option<String>,
}

impl Action {
    pub fn new(tool: impl Into<String>) -> Self {
        let tool = tool.into();
        Self {
            argv: vec![tool.clone()],
            tool,
            env: BTreeMap::new(),
            cwd: None,
            description: None,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.argv.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.argv.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.insert(k.into(), v.into());
        self
    }

    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }

    pub fn desc(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
}

/// Cache key for a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    /// Hash of inputs (content, timestamps, etc.) - computed by executor.
    pub inputs_hash: u64,
    /// Hash of action/config (argv/env/cwd/policy).
    pub config_hash: u64,
    /// Optional extra salt (toolchain version, target triple, profile).
    pub salt: Option<String>,
}

impl CacheKey {
    pub fn new(inputs_hash: u64, config_hash: u64) -> Self {
        Self {
            inputs_hash,
            config_hash,
            salt: None,
        }
    }

    pub fn with_salt(mut self, s: impl Into<String>) -> Self {
        self.salt = Some(s.into());
        self
    }

    pub fn as_u128(&self) -> u128 {
        // Combine into a wide key for storage
        let a = self.inputs_hash as u128;
        let b = self.config_hash as u128;
        (a << 64) | b
    }
}

/// A node in the bake graph: consumes artifacts, runs an action, produces artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub inputs: Vec<ArtifactId>,
    pub outputs: Vec<ArtifactId>,
    pub action: Action,
    pub cache: Option<CacheKey>,
    pub meta: BTreeMap<String, String>,
}

impl Node {
    pub fn new(name: impl Into<String>, action: Action) -> Self {
        let name = name.into();
        // node id should be stable across runs if name + action is stable
        let cfg_hash = stable_hash64(&(
            name.as_str(),
            action.tool.as_str(),
            &action.argv,
            &action.env,
            action.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
        ));
        Self {
            id: NodeId(cfg_hash),
            name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            action,
            cache: None,
            meta: BTreeMap::new(),
        }
    }

    pub fn input(mut self, a: &Artifact) -> Self {
        self.inputs.push(a.id);
        self
    }

    pub fn output(mut self, a: &Artifact) -> Self {
        self.outputs.push(a.id);
        self
    }

    pub fn with_cache(mut self, cache: CacheKey) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

/// A bake graph: artifacts + nodes + edges.
#[derive(Debug, Default, Clone)]
pub struct BakeGraph {
    pub artifacts: BTreeMap<ArtifactId, Artifact>,
    pub nodes: BTreeMap<NodeId, Node>,

    /// deps: node -> nodes it depends on (must run before).
    pub deps: BTreeMap<NodeId, BTreeSet<NodeId>>,

    /// reverse deps: node -> nodes depending on it.
    pub rdeps: BTreeMap<NodeId, BTreeSet<NodeId>>,
}

#[derive(Debug)]
pub enum GraphError {
    UnknownNode(NodeId),
    UnknownArtifact(ArtifactId),
    Cycle(Vec<NodeId>),
    DuplicateOutput(ArtifactId),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::UnknownNode(id) => write!(f, "unknown node: {:?}", id),
            GraphError::UnknownArtifact(id) => write!(f, "unknown artifact: {:?}", id),
            GraphError::Cycle(nodes) => write!(f, "cycle detected: {} nodes", nodes.len()),
            GraphError::DuplicateOutput(a) => write!(f, "duplicate output artifact: {:?}", a),
        }
    }
}

impl std::error::Error for GraphError {}

impl BakeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_artifact(&mut self, a: Artifact) -> ArtifactId {
        let id = a.id;
        self.artifacts.insert(id, a);
        id
    }

    pub fn add_node(&mut self, n: Node) -> NodeId {
        let id = n.id;
        self.nodes.insert(id, n);
        self.deps.entry(id).or_default();
        self.rdeps.entry(id).or_default();
        id
    }

    /// Add a dependency edge `a -> b` meaning: `b` depends on `a` (a runs before b).
    pub fn add_dep(&mut self, a: NodeId, b: NodeId) -> Result<(), GraphError> {
        if !self.nodes.contains_key(&a) {
            return Err(GraphError::UnknownNode(a));
        }
        if !self.nodes.contains_key(&b) {
            return Err(GraphError::UnknownNode(b));
        }
        self.deps.entry(b).or_default().insert(a);
        self.rdeps.entry(a).or_default().insert(b);
        Ok(())
    }

    /// Infer node dependencies from artifact flows:
    /// If node X outputs artifact A and node Y inputs artifact A => X -> Y.
    pub fn infer_deps_from_artifacts(&mut self) -> Result<(), GraphError> {
        // Map output artifact -> producer node
        let mut producer: BTreeMap<ArtifactId, NodeId> = BTreeMap::new();
        for (nid, n) in &self.nodes {
            for &out in &n.outputs {
                if producer.insert(out, *nid).is_some() {
                    return Err(GraphError::DuplicateOutput(out));
                }
            }
        }

        let mut edges = Vec::new();
        for (nid, n) in &self.nodes {
            for &inp in &n.inputs {
                if let Some(&p) = producer.get(&inp) {
                    if p != *nid {
                        edges.push((p, *nid));
                    }
                }
            }
        }

        for (a, b) in edges {
            self.add_dep(a, b)?;
        }
        Ok(())
    }

    /// Return nodes in topological order (Kahn).
    pub fn topo_order(&self) -> Result<Vec<NodeId>, GraphError> {
        let mut indeg: BTreeMap<NodeId, usize> = BTreeMap::new();
        for (&n, ds) in &self.deps {
            indeg.insert(n, ds.len());
        }
        for &n in self.nodes.keys() {
            indeg.entry(n).or_insert(0);
        }

        let mut q: VecDeque<NodeId> = indeg
            .iter()
            .filter_map(|(&n, &d)| if d == 0 { Some(n) } else { None })
            .collect();

        let mut out = Vec::with_capacity(self.nodes.len());
        let mut indeg_mut = indeg;

        while let Some(n) = q.pop_front() {
            out.push(n);
            if let Some(children) = self.rdeps.get(&n) {
                for &m in children {
                    let e = indeg_mut.get_mut(&m).unwrap();
                    *e = e.saturating_sub(1);
                    if *e == 0 {
                        q.push_back(m);
                    }
                }
            }
        }

        if out.len() != self.nodes.len() {
            // find nodes still with indeg > 0
            let cyc: Vec<NodeId> = indeg_mut
                .into_iter()
                .filter_map(|(n, d)| if d > 0 { Some(n) } else { None })
                .collect();
            return Err(GraphError::Cycle(cyc));
        }

        Ok(out)
    }

    /// Extract a subgraph containing nodes required to build the given outputs.
    pub fn subgraph_for_outputs(&self, outputs: &[ArtifactId]) -> Result<BakeGraph, GraphError> {
        // find producer nodes for each output
        let mut producer: BTreeMap<ArtifactId, NodeId> = BTreeMap::new();
        for (&nid, n) in &self.nodes {
            for &out in &n.outputs {
                producer.insert(out, nid);
            }
        }

        let mut needed: BTreeSet<NodeId> = BTreeSet::new();
        let mut stack: Vec<NodeId> = Vec::new();

        for o in outputs {
            let Some(&p) = producer.get(o) else {
                return Err(GraphError::UnknownArtifact(*o));
            };
            stack.push(p);
        }

        while let Some(n) = stack.pop() {
            if !needed.insert(n) {
                continue;
            }
            if let Some(ds) = self.deps.get(&n) {
                for &d in ds {
                    stack.push(d);
                }
            }
        }

        // Build new graph
        let mut g = BakeGraph::new();

        // Copy nodes + deps
        for &nid in &needed {
            let n = self.nodes.get(&nid).ok_or(GraphError::UnknownNode(nid))?.clone();
            g.add_node(n);
        }

        // Artifacts: include all inputs/outputs referenced by included nodes
        let mut arts_needed: BTreeSet<ArtifactId> = BTreeSet::new();
        for nid in g.nodes.keys() {
            let n = g.nodes.get(nid).unwrap();
            arts_needed.extend(n.inputs.iter().copied());
            arts_needed.extend(n.outputs.iter().copied());
        }
        for aid in arts_needed {
            if let Some(a) = self.artifacts.get(&aid) {
                g.add_artifact(a.clone());
            } else {
                return Err(GraphError::UnknownArtifact(aid));
            }
        }

        // Copy deps edges among needed nodes
        for &nid in needed.iter() {
            if let Some(ds) = self.deps.get(&nid) {
                for &d in ds {
                    if needed.contains(&d) {
                        g.add_dep(d, nid)?;
                    }
                }
            }
        }

        Ok(g)
    }

    /// Debug helper: DOT graph output.
    pub fn to_dot(&self) -> String {
        let mut s = String::new();
        s.push_str("digraph bake {\n");

        for (nid, n) in &self.nodes {
            s.push_str(&format!(
                "  n{} [label=\"{}\"];\n",
                nid.0,
                escape_dot(&n.name)
            ));
        }

        for (b, ds) in &self.deps {
            for a in ds {
                s.push_str(&format!("  n{} -> n{};\n", a.0, b.0));
            }
        }

        s.push_str("}\n");
        s
    }
}

/* ------------------------------ Helpers ---------------------------------- */

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A stable 64-bit hash for IDs.
/// Uses `DefaultHasher` (SipHash) but fed deterministically; stable per rust version is not guaranteed.
/// If you need cross-version stability, swap to a fixed hash (xxhash/fnv) under a feature.
fn stable_hash64<T: Hash>(v: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/* ------------------------------ Tests ------------------------------------ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_infer_and_topo() {
        let mut g = BakeGraph::new();

        let src = Artifact::source("src/main.c");
        let obj = Artifact::logical("obj/main.o", ArtifactKind::Intermediate);
        let exe = Artifact::output("bin/app");

        g.add_artifact(src.clone());
        g.add_artifact(obj.clone());
        g.add_artifact(exe.clone());

        let n1 = Node::new("compile", Action::new("clang").arg("-c"))
            .input(&src)
            .output(&obj);
        let n2 = Node::new("link", Action::new("clang"))
            .input(&obj)
            .output(&exe);

        let id1 = g.add_node(n1);
        let id2 = g.add_node(n2);

        g.infer_deps_from_artifacts().unwrap();

        let order = g.topo_order().unwrap();
        assert_eq!(order, vec![id1, id2]);
    }

    #[test]
    fn dot_output_contains_edges() {
        let mut g = BakeGraph::new();
        let a = g.add_node(Node::new("a", Action::new("tool")));
        let b = g.add_node(Node::new("b", Action::new("tool")));
        g.add_dep(a, b).unwrap();
        let dot = g.to_dot();
        assert!(dot.contains("->"));
    }
}
