// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\output\graph_json.rs

use crate::model::graph::{Graph, GraphError};
use serde_json::Value;
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

/// JSON schema tag for graph exports (tooling/CI/IDE consumption).
pub const MUFFIN_GRAPH_JSON_SCHEMA: &str = "steel.graph.json/1";

/// Options for exporting a Graph to JSON.
#[derive(Debug, Clone)]
pub struct GraphJsonOptions {
    /// Pretty-print JSON output.
    pub pretty: bool,
    /// Validate graph (endpoints, ports, types) before writing.
    pub validate: bool,
    /// Ensure parent directories exist.
    pub create_dirs: bool,
}

impl Default for GraphJsonOptions {
    fn default() -> Self {
        Self {
            pretty: true,
            validate: true,
            create_dirs: true,
        }
    }
}

/// Export the graph to a JSON string.
///
/// This is stable, deterministic, and suitable for `target/steel/graph.json`.
pub fn graph_to_json_string(graph: &Graph, opts: &GraphJsonOptions) -> Result<String, GraphJsonError> {
    if opts.validate {
        graph.validate().map_err(GraphJsonError::Validate)?;
    }

    // Compose a stable envelope so tooling can evolve independently from internal structs.
    let payload = serde_json::json!({
        "schema": MUFFIN_GRAPH_JSON_SCHEMA,
        "meta": graph.meta,          // stable map
        "nodes": graph.nodes,        // BTreeMap => stable order
        "edges": graph.edges,        // Vec; ensure deterministic construction upstream
    });

    if opts.pretty {
        serde_json::to_string_pretty(&payload).map_err(GraphJsonError::Serialize)
    } else {
        serde_json::to_string(&payload).map_err(GraphJsonError::Serialize)
    }
}

/// Export the graph to JSON and write it to a file.
pub fn write_graph_json_file(
    graph: &Graph,
    path: impl AsRef<Path>,
    opts: &GraphJsonOptions,
) -> Result<PathBuf, GraphJsonError> {
    let path = path.as_ref().to_path_buf();

    if opts.create_dirs {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(GraphJsonError::Io)?;
        }
    }

    let s = graph_to_json_string(graph, opts)?;
    fs::write(&path, s).map_err(GraphJsonError::Io)?;
    Ok(path)
}

/// Convenience: export to a `serde_json::Value` (for in-memory consumers).
pub fn graph_to_json_value(graph: &Graph, opts: &GraphJsonOptions) -> Result<Value, GraphJsonError> {
    if opts.validate {
        graph.validate().map_err(GraphJsonError::Validate)?;
    }

    Ok(serde_json::json!({
        "schema": MUFFIN_GRAPH_JSON_SCHEMA,
        "meta": graph.meta,
        "nodes": graph.nodes,
        "edges": graph.edges,
    }))
}

/// Errors for graph JSON export.
#[derive(Debug, thiserror::Error)]
pub enum GraphJsonError {
    #[error("graph validation failed: {0}")]
    Validate(GraphError),

    #[error("json serialization failed: {0}")]
    Serialize(serde_json::Error),

    #[error("io error: {0}")]
    Io(io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::artifact::ArtifactKind;
    use crate::model::graph::{Endpoint, Node, NodeId, NodeKind, Port};

    #[test]
    fn export_graph_json_envelope() {
        let mut g = Graph::new();
        g.meta.insert("profile".into(), "debug".into());

        let n1 = Node::new("tool.cc", NodeKind::Tool, "cc").with_port(Port::output("obj", ArtifactKind::Object));
        let n2 = Node::new("tool.ld", NodeKind::Tool, "ld")
            .with_port(Port::input("obj", ArtifactKind::Object))
            .with_port(Port::output("exe", ArtifactKind::Executable));

        g.add_node(n1);
        g.add_node(n2);

        g.add_edge(crate::model::graph::Edge::new(
            Endpoint::new(NodeId::new("tool.cc"), "obj"),
            Endpoint::new(NodeId::new("tool.ld"), "obj"),
        ));

        let s = graph_to_json_string(&g, &GraphJsonOptions::default()).unwrap();
        assert!(s.contains(MUFFIN_GRAPH_JSON_SCHEMA));
        assert!(s.contains("\"nodes\""));
        assert!(s.contains("\"edges\""));
    }
}
