//! Graph module root.
//!
//! This crate/module hosts build-graph primitives used by Steel.
//!
//! Submodules:
//! - `bake`: build graph (nodes/artifacts/deps) + planning helpers
//! - `dot`: DOT export for Graphviz
//! - `json`: JSON model + std-only (de)serialization
//!
//! Re-exports provide a convenient public surface.

pub mod bake;
pub mod dot;
pub mod json;

pub use bake::{
    Action, Artifact, ArtifactId, ArtifactKind, BakeGraph, CacheKey, GraphError, Node, NodeId,
};

pub use dot::{DotExporter, DotOptions};

pub use json::{GraphJson, JsonError, GRAPH_JSON_SCHEMA_VERSION};
