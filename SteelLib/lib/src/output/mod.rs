// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\output\mod.rs

//! Output / emission layer.
//!
//! This module is responsible for producing deterministic, tooling-friendly artifacts
//! from resolved models (graph/config), typically under `target/`.
//!
//! Current exports:
//! - `graph_json` : JSON export of the resolved DAG for CI/IDE tooling.
//! - `mub`        : MUB (Steel Universal Binary) frozen configuration artifact.
//!
//! Notes:
//! - No `.mff` artifacts: the canonical frozen config is `config.mub`.
//! - Keep outputs stable (schema tags + deterministic ordering).

pub mod graph_json;
pub mod mub;

pub use graph_json::{
    graph_to_json_string, graph_to_json_value, write_graph_json_file, GraphJsonError, GraphJsonOptions,
    MUFFIN_GRAPH_JSON_SCHEMA,
};

pub use mub::{
    build_payload, decode_mub, encode_mub, read_mub_file, write_mub_file, MubError, MubOptions, MubPayload,
    DEFAULT_MUB_REL_PATH, MUB_MAGIC, MUB_SCHEMA_VERSION,
};
