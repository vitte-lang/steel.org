// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\resolve\implicit.rs

//! Implicit resolution layer.
//!
//! This module injects *implicit defaults* into the resolved model
//! when the user did not specify them explicitly in `steelconf`.
//!
//! Rules here must be:
//! - deterministic
//! - minimal (never override explicit user intent)
//! - transparent (easy to explain via diagnostics)

use crate::{
    error::SteelError,
    model::{
        artifact::ArtifactKind,
        graph::{Graph, Node, NodeKind, Port},
        target::{Target, TargetKind},
    },
};
use std::collections::BTreeMap;

/// Apply implicit defaults to a resolved target.
///
/// This is called *after* parsing + validation, but *before* graph finalization.
pub fn apply_target_implicits(target: &mut Target) -> Result<(), SteelError> {
    // Default kind: binary
    if target.kind.is_none() {
        target.kind = Some(TargetKind::Binary);
    }

    // Default name fallback
    if target.name.is_empty() {
        return Err(SteelError::ValidationFailed(
            "target name cannot be empty".into(),
        ));
    }

    Ok(())
}

/// Apply implicit rules to the resolved build graph.
///
/// This injects:
/// - implicit compile steps
/// - implicit link steps
/// - implicit artifact ports
///
/// The goal is to keep user Steel files minimal while producing
/// a fully explicit DAG for execution.
pub fn apply_graph_implicits(graph: &mut Graph) -> Result<(), SteelError> {
    let mut new_nodes = Vec::new();

    for node in graph.nodes.values() {
        match node.kind {
            NodeKind::Target => {
                apply_target_node_implicits(node, &mut new_nodes)?;
            }
            _ => {}
        }
    }

    for n in new_nodes {
        graph.add_node(n);
    }

    Ok(())
}

fn apply_target_node_implicits(
    target_node: &Node,
    new_nodes: &mut Vec<Node>,
) -> Result<(), SteelError> {
    let target_name = target_node.label.clone();

    match target_node.subkind.as_deref() {
        Some("binary") => {
            // Implicit link node for binary targets
            let link_node_id = format!("{}.link", target_name);

            let mut link = Node::new(&link_node_id, NodeKind::Tool, "ld");
            link.meta.insert("implicit".into(), "true".into());

            link = link
                .with_port(Port::input("obj", ArtifactKind::Object))
                .with_port(Port::output("exe", ArtifactKind::Executable));

            new_nodes.push(link);
        }

        Some("library") => {
            // Implicit archive node for library targets
            let ar_node_id = format!("{}.archive", target_name);

            let mut ar = Node::new(&ar_node_id, NodeKind::Tool, "ar");
            ar.meta.insert("implicit".into(), "true".into());

            ar = ar
                .with_port(Port::input("obj", ArtifactKind::Object))
                .with_port(Port::output("lib", ArtifactKind::StaticLibrary));

            new_nodes.push(ar);
        }

        Some("test") => {
            // Tests are binaries with implicit run step
            let run_node_id = format!("{}.run", target_name);

            let mut run = Node::new(&run_node_id, NodeKind::Tool, "run");
            run.meta.insert("implicit".into(), "true".into());

            run = run
                .with_port(Port::input("exe", ArtifactKind::Executable))
                .with_port(Port::output("report", ArtifactKind::TestReport));

            new_nodes.push(run);
        }

        _ => {}
    }

    Ok(())
}

/// Populate implicit metadata keys if missing.
///
/// Used to ensure stable graph introspection.
pub fn apply_implicit_meta(
    meta: &mut BTreeMap<String, String>,
    defaults: &[(&str, &str)],
) {
    for (k, v) in defaults {
        meta.entry((*k).into()).or_insert_with(|| (*v).into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::graph::{Graph, NodeKind};

    #[test]
    fn implicit_binary_target_injects_link() {
        let mut g = Graph::new();

        let mut t = Node::new("app", NodeKind::Target, "binary");
        g.add_node(t.clone());

        apply_graph_implicits(&mut g).unwrap();

        assert!(g.nodes.contains_key("app.link"));
    }
}
