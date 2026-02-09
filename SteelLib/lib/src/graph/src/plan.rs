//! Build plan: selecting targets, extracting subgraphs, ordering, and batching.
//!
//! This module builds an execution-ready plan from a `BakeGraph`:
//! - select output artifacts as targets
//! - extract required subgraph
//! - topologically order nodes
//! - compute levels (parallel batches) based on dependency depth
//!
//! The executor can then run each level in parallel (within capsule limits),
//! honoring the topological constraints.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::bake::{ArtifactId, BakeGraph, GraphError, NodeId};

/// A planned graph subset ready for execution.
#[derive(Debug, Clone)]
pub struct Plan {
    pub targets: Vec<ArtifactId>,
    pub nodes: Vec<NodeId>,                // topo order
    pub levels: Vec<Vec<NodeId>>,          // parallel batches
    pub indegree: BTreeMap<NodeId, usize>, // cached indegrees for the planned subgraph
}

impl Plan {
    /// Build a plan for the given output artifacts.
    pub fn for_outputs(g: &BakeGraph, outputs: &[ArtifactId]) -> Result<Self, GraphError> {
        let sub = g.subgraph_for_outputs(outputs)?;
        let nodes = sub.topo_order()?;
        let (levels, indeg) = compute_levels(&sub, &nodes)?;

        Ok(Self {
            targets: outputs.to_vec(),
            nodes,
            levels,
            indegree: indeg,
        })
    }

    /// Build a plan for all nodes (whole graph).
    pub fn all(g: &BakeGraph) -> Result<Self, GraphError> {
        let nodes = g.topo_order()?;
        let (levels, indeg) = compute_levels(g, &nodes)?;
        Ok(Self {
            targets: Vec::new(),
            nodes,
            levels,
            indegree: indeg,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn level_count(&self) -> usize {
        self.levels.len()
    }
}

/// Compute "levels" (Kahn batching): each level are nodes whose deps are satisfied.
fn compute_levels(
    g: &BakeGraph,
    topo: &[NodeId],
) -> Result<(Vec<Vec<NodeId>>, BTreeMap<NodeId, usize>), GraphError> {
    // indegree (within g)
    let mut indeg: BTreeMap<NodeId, usize> = BTreeMap::new();
    for (&n, ds) in &g.deps {
        indeg.insert(n, ds.len());
    }
    for &n in g.nodes.keys() {
        indeg.entry(n).or_insert(0);
    }

    // ensure topo list matches graph
    let topo_set: BTreeSet<NodeId> = topo.iter().copied().collect();
    if topo_set.len() != g.nodes.len() {
        // not fatal, but indicates mismatch
        // Use the graph's own nodes.
    }

    let mut ready: VecDeque<NodeId> = indeg
        .iter()
        .filter_map(|(&n, &d)| if d == 0 { Some(n) } else { None })
        .collect();

    // deterministic: queue in sorted order for each level
    let mut levels: Vec<Vec<NodeId>> = Vec::new();
    let mut indeg_mut = indeg.clone();

    let mut remaining = g.nodes.len();
    while remaining > 0 {
        if ready.is_empty() {
            // cycle (should have been caught by topo_order, but keep safety)
            let cyc: Vec<NodeId> = indeg_mut
                .iter()
                .filter_map(|(&n, &d)| if d > 0 { Some(n) } else { None })
                .collect();
            return Err(GraphError::Cycle(cyc));
        }

        // Take current ready set as one level (sorted)
        let mut level: Vec<NodeId> = ready.drain(..).collect();
        level.sort_by_key(|x| x.0);

        // consume level
        for &n in &level {
            remaining = remaining.saturating_sub(1);
            if let Some(children) = g.rdeps.get(&n) {
                for &m in children {
                    let e = indeg_mut.get_mut(&m).unwrap();
                    *e = e.saturating_sub(1);
                    if *e == 0 {
                        ready.push_back(m);
                    }
                }
            }
        }

        levels.push(level);
    }

    Ok((levels, indeg))
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::bake::{Action, Artifact, ArtifactKind, Node};

    #[test]
    fn plan_levels_basic() {
        let mut g = BakeGraph::new();

        let a = Artifact::source("a.c");
        let b = Artifact::source("b.c");
        let ao = Artifact::logical("a.o", ArtifactKind::Intermediate);
        let bo = Artifact::logical("b.o", ArtifactKind::Intermediate);
        let exe = Artifact::output("app");

        g.add_artifact(a.clone());
        g.add_artifact(b.clone());
        g.add_artifact(ao.clone());
        g.add_artifact(bo.clone());
        g.add_artifact(exe.clone());

        let ca = Node::new("compile-a", Action::new("clang").arg("-c"))
            .input(&a)
            .output(&ao);
        let cb = Node::new("compile-b", Action::new("clang").arg("-c"))
            .input(&b)
            .output(&bo);
        let link = Node::new("link", Action::new("clang"))
            .input(&ao)
            .input(&bo)
            .output(&exe);

        g.add_node(ca);
        g.add_node(cb);
        g.add_node(link);

        g.infer_deps_from_artifacts().unwrap();

        let p = Plan::for_outputs(&g, &[exe.id]).unwrap();
        assert_eq!(p.level_count(), 2);
        assert_eq!(p.levels[0].len(), 2); // compile a,b in parallel
        assert_eq!(p.levels[1].len(), 1); // link
    }
}
