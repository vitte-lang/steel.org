//! Topological utilities for `BakeGraph`.
//!
//! This module provides:
//! - classic Kahn topo sort
//! - cycle detection + optional cycle extraction (best-effort)
//! - deterministic ordering (BTreeMap/BTreeSet iteration + sort)
//! - extra helpers: indegree map, reverse topo, reachability
//!
//! Note: `BakeGraph` already has `topo_order()` in `bake.rs`.
//! This module offers a richer API surface for callers that need
//! additional metadata (levels, indegrees, reverse order, etc.).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::bake::{BakeGraph, GraphError, NodeId};

#[derive(Debug, Clone)]
pub struct Topo {
    pub order: Vec<NodeId>,
    pub indegree: BTreeMap<NodeId, usize>,
}

impl Topo {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// Compute indegree map (within the graph).
pub fn indegrees(g: &BakeGraph) -> BTreeMap<NodeId, usize> {
    let mut indeg: BTreeMap<NodeId, usize> = BTreeMap::new();
    for (&n, ds) in &g.deps {
        indeg.insert(n, ds.len());
    }
    for &n in g.nodes.keys() {
        indeg.entry(n).or_insert(0);
    }
    indeg
}

/// Deterministic topo sort (Kahn).
pub fn topo_sort(g: &BakeGraph) -> Result<Topo, GraphError> {
    let mut indeg = indegrees(g);

    // queue nodes with indegree 0 (deterministic order)
    let mut ready: VecDeque<NodeId> = indeg
        .iter()
        .filter_map(|(&n, &d)| if d == 0 { Some(n) } else { None })
        .collect();

    let mut out: Vec<NodeId> = Vec::with_capacity(g.nodes.len());

    while let Some(n) = ready.pop_front() {
        out.push(n);
        if let Some(children) = g.rdeps.get(&n) {
            for &m in children {
                let e = indeg.get_mut(&m).unwrap();
                *e = e.saturating_sub(1);
                if *e == 0 {
                    ready.push_back(m);
                }
            }
        }
    }

    if out.len() != g.nodes.len() {
        let cyc: Vec<NodeId> = indeg
            .into_iter()
            .filter_map(|(n, d)| if d > 0 { Some(n) } else { None })
            .collect();
        return Err(GraphError::Cycle(cyc));
    }

    Ok(Topo { order: out, indegree: indegrees(g) })
}

/// Reverse topological order (useful for cleanup, pruning, etc.).
pub fn topo_sort_reverse(g: &BakeGraph) -> Result<Vec<NodeId>, GraphError> {
    let mut t = topo_sort(g)?.order;
    t.reverse();
    Ok(t)
}

/// Compute execution levels (parallel batches) from topo ordering.
/// Equivalent to a Kahn batching.
pub fn topo_levels(g: &BakeGraph) -> Result<Vec<Vec<NodeId>>, GraphError> {
    let mut indeg = indegrees(g);

    let mut ready: VecDeque<NodeId> = indeg
        .iter()
        .filter_map(|(&n, &d)| if d == 0 { Some(n) } else { None })
        .collect();

    let mut remaining = g.nodes.len();
    let mut levels: Vec<Vec<NodeId>> = Vec::new();

    while remaining > 0 {
        if ready.is_empty() {
            let cyc: Vec<NodeId> = indeg
                .iter()
                .filter_map(|(&n, &d)| if d > 0 { Some(n) } else { None })
                .collect();
            return Err(GraphError::Cycle(cyc));
        }

        let mut level: Vec<NodeId> = ready.drain(..).collect();
        level.sort_by_key(|x| x.0);

        for &n in &level {
            remaining = remaining.saturating_sub(1);
            if let Some(children) = g.rdeps.get(&n) {
                for &m in children {
                    let e = indeg.get_mut(&m).unwrap();
                    *e = e.saturating_sub(1);
                    if *e == 0 {
                        ready.push_back(m);
                    }
                }
            }
        }

        levels.push(level);
    }

    Ok(levels)
}

/// Return the set of nodes reachable from `start` following rdeps (forward edges).
pub fn reachable_forward(g: &BakeGraph, start: NodeId) -> BTreeSet<NodeId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];

    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            continue;
        }
        if let Some(next) = g.rdeps.get(&n) {
            for &m in next {
                stack.push(m);
            }
        }
    }

    seen
}

/// Return the set of nodes that `start` depends on (following deps backwards).
pub fn reachable_backward(g: &BakeGraph, start: NodeId) -> BTreeSet<NodeId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];

    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            continue;
        }
        if let Some(prev) = g.deps.get(&n) {
            for &m in prev {
                stack.push(m);
            }
        }
    }

    seen
}

/// Best-effort cycle extraction: try to return a simple cycle path.
/// If no cycle, returns None.
pub fn find_cycle(g: &BakeGraph) -> Option<Vec<NodeId>> {
    // DFS color marking
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: BTreeMap<NodeId, Color> = g.nodes.keys().map(|&n| (n, Color::White)).collect();
    let mut parent: BTreeMap<NodeId, NodeId> = BTreeMap::new();

    fn dfs(
        g: &BakeGraph,
        u: NodeId,
        color: &mut BTreeMap<NodeId, Color>,
        parent: &mut BTreeMap<NodeId, NodeId>,
    ) -> Option<Vec<NodeId>> {
        color.insert(u, Color::Gray);

        // edges u -> v (forward), using rdeps
        if let Some(next) = g.rdeps.get(&u) {
            for &v in next {
                match color.get(&v).copied().unwrap_or(Color::White) {
                    Color::White => {
                        parent.insert(v, u);
                        if let Some(cyc) = dfs(g, v, color, parent) {
                            return Some(cyc);
                        }
                    }
                    Color::Gray => {
                        // Found back-edge u -> v, reconstruct cycle
                        let mut cycle = vec![v, u];
                        let mut cur = u;
                        while let Some(&p) = parent.get(&cur) {
                            cur = p;
                            cycle.push(cur);
                            if cur == v {
                                break;
                            }
                        }
                        cycle.reverse();
                        return Some(cycle);
                    }
                    Color::Black => {}
                }
            }
        }

        color.insert(u, Color::Black);
        None
    }

    for &n in g.nodes.keys() {
        if color.get(&n) == Some(&Color::White) {
            if let Some(cyc) = dfs(g, n, &mut color, &mut parent) {
                return Some(cyc);
            }
        }
    }
    None
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::bake::{Action, BakeGraph, Node};

    #[test]
    fn topo_sort_ok() {
        let mut g = BakeGraph::new();
        let a = g.add_node(Node::new("a", Action::new("t")));
        let b = g.add_node(Node::new("b", Action::new("t")));
        g.add_dep(a, b).unwrap();

        let t = topo_sort(&g).unwrap();
        assert_eq!(t.order, vec![a, b]);
    }

    #[test]
    fn topo_levels_ok() {
        let mut g = BakeGraph::new();
        let a = g.add_node(Node::new("a", Action::new("t")));
        let b = g.add_node(Node::new("b", Action::new("t")));
        let c = g.add_node(Node::new("c", Action::new("t")));
        g.add_dep(a, c).unwrap();
        g.add_dep(b, c).unwrap();

        let lv = topo_levels(&g).unwrap();
        assert_eq!(lv.len(), 2);
        assert_eq!(lv[1], vec![c]);
    }

    #[test]
    fn cycle_detect() {
        let mut g = BakeGraph::new();
        let a = g.add_node(Node::new("a", Action::new("t")));
        let b = g.add_node(Node::new("b", Action::new("t")));
        g.add_dep(a, b).unwrap();
        g.add_dep(b, a).unwrap();

        assert!(topo_sort(&g).is_err());
        let cyc = find_cycle(&g).unwrap();
        assert!(!cyc.is_empty());
    }
}
