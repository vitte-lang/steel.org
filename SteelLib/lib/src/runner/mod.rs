// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\runner\mod.rs

//! Execution layer (runner).
//!
//! The runner consumes the resolved, frozen configuration (Graph + toolchains),
//! then executes the DAG deterministically with incremental/cache support.
//!
//! - Inputs are expected to be fully expanded + explicit (no globs left).
//! - Outputs are materialized under `target/` by convention.
//!
//! This module is intentionally "runtime-y" and must not depend on parser/CLI.

pub mod cache;

use crate::{
    error::SteelError,
    model::{
        artifact::{Artifact, ArtifactKind},
        graph::{Edge, Graph, Node, NodeKind, Port},
    },
    runner::cache::{Cache, CacheKey, DEFAULT_CACHE_DIR},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Runner configuration.
#[derive(Debug, Clone)]
pub struct RunnerOptions {
    /// Workspace root.
    pub root: PathBuf,

    /// Target directory (default `target`).
    pub target_dir: PathBuf,

    /// Enable cache.
    pub cache_enabled: bool,

    /// Cache directory (default `target/cache`).
    pub cache_dir: PathBuf,

    /// Max parallelism (stubbed, sequential runner by default).
    pub jobs: usize,

    /// Verbose logging.
    pub verbose: bool,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            target_dir: PathBuf::from("target"),
            cache_enabled: true,
            cache_dir: PathBuf::from(DEFAULT_CACHE_DIR),
            jobs: 1,
            verbose: false,
        }
    }
}

/// Runner handle.
pub struct Runner {
    opts: RunnerOptions,
    cache: Option<Cache>,
}

impl Runner {
    pub fn new(mut opts: RunnerOptions) -> Self {
        // normalize defaults
        if opts.cache_dir.as_os_str().is_empty() {
            opts.cache_dir = PathBuf::from(DEFAULT_CACHE_DIR);
        }

        let cache = if opts.cache_enabled {
            Some(Cache::new(opts.cache_dir.clone()))
        } else {
            None
        };

        Self { opts, cache }
    }

    /// Execute a resolved graph.
    ///
    /// Current behavior:
    /// - topological order
    /// - sequential execution (jobs is reserved)
    /// - cache hooks are present but tool execution is stubbed here
    pub fn run(&self, graph: &Graph) -> Result<RunReport, SteelError> {
        let order = topo_sort(graph)?;

        let mut report = RunReport::new();

        for node_id in order {
            let node = graph
                .nodes
                .get(&node_id)
                .ok_or_else(|| SteelError::ExecutionFailed(format!("missing node {}", node_id)))?;

            let status = self.exec_node(graph, node)?;
            report.nodes.insert(node_id, status);
        }

        Ok(report)
    }

    fn exec_node(&self, graph: &Graph, node: &Node) -> Result<NodeStatus, SteelError> {
        // Only Tool nodes represent executable steps.
        if node.kind != NodeKind::Tool {
            return Ok(NodeStatus::Skipped("non-tool node".into()));
        }

        // 1) compute a cache key (inputs + args + tool identity)
        let key = self.compute_cache_key(graph, node)?;

        // 2) cache hit?
        if let (Some(cache), Some(key)) = (&self.cache, key.as_ref()) {
            if cache.contains(key) {
                // restore outputs (stub: in real impl, restore each output artifact)
                return Ok(NodeStatus::Cached {
                    key: key.hash.clone(),
                });
            }
        }

        // 3) execute tool (stub)
        // In real runner: spawn process, pass args/env, track stdout/stderr, write outputs.
        self.exec_tool_stub(node)?;

        // 4) store outputs in cache (stub)
        if let (Some(_cache), Some(key)) = (&self.cache, key.as_ref()) {
            // real impl would store each output and map key -> output set
            Ok(NodeStatus::Executed {
                key: Some(key.hash.clone()),
            })
        } else {
            Ok(NodeStatus::Executed { key: None })
        }
    }

    fn compute_cache_key(&self, graph: &Graph, node: &Node) -> Result<Option<CacheKey>, SteelError> {
        if self.cache.is_none() {
            return Ok(None);
        }

        // Deterministic key material:
        // - tool name (node.tool)
        // - stable node id
        // - input artifact paths + mtimes (MVP)
        //
        // NOTE: long-term should be content hashes, plus tool fingerprint and args.
        let mut material = String::new();
        material.push_str("node:");
        material.push_str(&node.id);
        material.push('\n');

        material.push_str("tool:");
        material.push_str(&node.tool);
        material.push('\n');

        // inputs from incoming edges
        let inputs = incoming_input_paths(graph, &node.id)?;
        for p in inputs {
            material.push_str("in:");
            material.push_str(&p.to_string_lossy());
            material.push('|');
            material.push_str(&mtime_tag(&p)?);
            material.push('\n');
        }

        // node metadata (args/env) if any
        for (k, v) in node.meta.iter() {
            material.push_str("meta:");
            material.push_str(k);
            material.push('=');
            material.push_str(v);
            material.push('\n');
        }

        // hash the material
        let hash = sha256_hex(material.as_bytes());
        Ok(Some(CacheKey::new(hash)))
    }

    fn exec_tool_stub(&self, node: &Node) -> Result<(), SteelError> {
        // This is intentionally a stub to keep runner layer buildable
        // before the actual process driver is implemented.
        if self.opts.verbose {
            eprintln!("[steel] exec tool={} node={}", node.tool, node.id);
        }
        Ok(())
    }
}

/// Execution report.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub started_at: SystemTime,
    pub nodes: BTreeMap<String, NodeStatus>,
}

impl RunReport {
    pub fn new() -> Self {
        Self {
            started_at: SystemTime::now(),
            nodes: BTreeMap::new(),
        }
    }
}

/// Per-node status.
#[derive(Debug, Clone)]
pub enum NodeStatus {
    Skipped(String),
    Cached { key: String },
    Executed { key: Option<String> },
}

// --- graph helpers ----------------------------------------------------------

fn topo_sort(graph: &Graph) -> Result<Vec<String>, SteelError> {
    // Kahn, deterministic with BTreeSet queueing
    let mut indeg: BTreeMap<String, usize> = BTreeMap::new();
    for id in graph.nodes.keys() {
        indeg.insert(id.clone(), 0);
    }
    for e in graph.edges.iter() {
        *indeg
            .get_mut(&e.to)
            .ok_or_else(|| SteelError::ExecutionFailed(format!("edge to unknown node {}", e.to)))? +=
            1;
    }

    let mut q = BTreeSet::new();
    for (id, d) in indeg.iter() {
        if *d == 0 {
            q.insert(id.clone());
        }
    }

    let mut out = Vec::new();
    let mut indeg = indeg;

    while let Some(id) = q.iter().next().cloned() {
        q.remove(&id);
        out.push(id.clone());

        for e in graph.edges.iter().filter(|e| e.from == id) {
            let d = indeg.get_mut(&e.to).unwrap();
            *d -= 1;
            if *d == 0 {
                q.insert(e.to.clone());
            }
        }
    }

    if out.len() != graph.nodes.len() {
        return Err(SteelError::ExecutionFailed(
            "cycle detected in graph".into(),
        ));
    }

    Ok(out)
}

fn incoming_input_paths(graph: &Graph, node_id: &str) -> Result<Vec<PathBuf>, SteelError> {
    // MVP heuristic:
    // - if edges carry artifact paths in meta["path"], use it
    // - else ignore
    let mut out = BTreeSet::new();
    for e in graph.edges.iter().filter(|e| e.to == node_id) {
        if let Some(p) = e.meta.get("path") {
            out.insert(PathBuf::from(p));
        }
    }
    Ok(out.into_iter().collect())
}

fn mtime_tag(path: &Path) -> Result<String, SteelError> {
    let md = std::fs::metadata(path)
        .map_err(|_| SteelError::ExecutionFailed(format!("missing input {}", path.display())))?;
    let m = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(format!("{:?}", m))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::graph::{Graph, Node, NodeKind};

    #[test]
    fn topo_sort_works() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", NodeKind::Tool, "x"));
        g.add_node(Node::new("b", NodeKind::Tool, "y"));
        g.add_edge(Edge::new("a", "b"));

        let order = topo_sort(&g).unwrap();
        assert_eq!(order, vec!["a".to_string(), "b".to_string()]);
    }
}
