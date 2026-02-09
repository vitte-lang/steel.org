// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\output\mub.rs

use crate::model::graph::Graph;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io,
    path::{Path, PathBuf},
};

/// MUB (Steel Universal Binary) schema version.
///
/// This file is the frozen, portable, deterministic configuration artifact
/// generated from `steelconf` and consumed by downstream build execution.
///
/// - little-endian header
/// - versioned schema
/// - payload is messagepack by default (compact + fast)
pub const MUB_SCHEMA_VERSION: u32 = 1;

/// Magic bytes: "MUB\0"
pub const MUB_MAGIC: [u8; 4] = *b"MUB\0";

/// Default relative output location (recommended).
/// Typically written under `target/steel/config.mub`.
pub const DEFAULT_MUB_REL_PATH: &str = "target/steel/config.mub";

/// Fixed header (packed manually).
///
/// Layout (all little-endian):
/// - magic[4]
/// - schema_version u32
/// - header_len u32
/// - payload_kind u32   (1 = msgpack)
/// - payload_len u64
/// - flags u32
/// - reserved u32
#[derive(Debug, Clone)]
pub struct MubHeader {
    pub schema_version: u32,
    pub payload_kind: u32,
    pub payload_len: u64,
    pub flags: u32,
}

impl MubHeader {
    pub fn new(payload_len: u64) -> Self {
        Self {
            schema_version: MUB_SCHEMA_VERSION,
            payload_kind: 1,
            payload_len,
            flags: 0,
        }
    }
}

/// Options for emitting MUB.
#[derive(Debug, Clone)]
pub struct MubOptions {
    /// Ensure parent directories exist.
    pub create_dirs: bool,
    /// Include a graph JSON sidecar (tooling convenience).
    pub emit_graph_json_sidecar: bool,
    /// Include a minimal meta sidecar (`fingerprints.json`, etc.) (stub hook).
    pub emit_fingerprints_sidecar: bool,
}

impl Default for MubOptions {
    fn default() -> Self {
        Self {
            create_dirs: true,
            emit_graph_json_sidecar: false,
            emit_fingerprints_sidecar: false,
        }
    }
}

/// Frozen payload: keep it simple and stable.
/// The downstream executor (Vitte or Steel runtime) consumes this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MubPayload {
    /// Payload schema tag for tooling.
    pub schema: String,

    /// Resolved, deterministic metadata (root/profile/target/host/toolchain, etc.).
    #[serde(default)]
    pub meta: BTreeMap<String, String>,

    /// The resolved DAG / wiring.
    pub graph: Graph,
}

/// Errors for MUB.
#[derive(Debug, thiserror::Error)]
pub enum MubError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),

    #[error("deserialization failed: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid MUB magic")]
    BadMagic,

    #[error("unsupported schema version: {0}")]
    UnsupportedVersion(u32),

    #[error("unsupported payload kind: {0}")]
    UnsupportedPayloadKind(u32),

    #[error("corrupt header")]
    CorruptHeader,
}

/// Build a payload structure from a resolved `Graph`.
pub fn build_payload(graph: Graph) -> MubPayload {
    MubPayload {
        schema: format!("steel.mub/{}", MUB_SCHEMA_VERSION),
        meta: graph.meta.clone(),
        graph,
    }
}

/// Encode a payload into a byte vector with MUB header + msgpack body.
pub fn encode_mub(payload: &MubPayload) -> Result<Vec<u8>, MubError> {
    let body = rmp_serde::to_vec_named(payload)?;
    let hdr = MubHeader::new(body.len() as u64);
    Ok(encode_header_and_body(&hdr, &body))
}

/// Write a payload to a MUB file.
pub fn write_mub_file(
    payload: &MubPayload,
    path: impl AsRef<Path>,
    opts: &MubOptions,
) -> Result<PathBuf, MubError> {
    let path = path.as_ref().to_path_buf();
    if opts.create_dirs {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }

    let bytes = encode_mub(payload)?;
    fs::write(&path, &bytes)?;

    // Optional sidecars for tooling (do not affect contract).
    if opts.emit_graph_json_sidecar {
        let json_path = path.with_extension("graph.json");
        // Reuse graph_json module if present in your crate.
        let json = serde_json::json!({
            "schema": "steel.graph.json/1",
            "meta": payload.meta,
            "nodes": payload.graph.nodes,
            "edges": payload.graph.edges,
        });
        fs::write(&json_path, serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".into()))?;
    }

    if opts.emit_fingerprints_sidecar {
        let fp_path = path.with_extension("fingerprints.json");
        let json = serde_json::json!({
            "schema": "steel.fingerprints.json/1",
            "fingerprints": {},
        });
        fs::write(&fp_path, serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".into()))?;
    }

    Ok(path)
}

/// Read and decode a MUB file.
pub fn read_mub_file(path: impl AsRef<Path>) -> Result<MubPayload, MubError> {
    let bytes = fs::read(path)?;
    decode_mub(&bytes)
}

/// Decode a MUB buffer into payload.
pub fn decode_mub(bytes: &[u8]) -> Result<MubPayload, MubError> {
    let (hdr, body) = decode_header_and_body(bytes)?;
    if hdr.schema_version != MUB_SCHEMA_VERSION {
        return Err(MubError::UnsupportedVersion(hdr.schema_version));
    }
    if hdr.payload_kind != 1 {
        return Err(MubError::UnsupportedPayloadKind(hdr.payload_kind));
    }
    let payload: MubPayload = rmp_serde::from_slice(body)?;
    Ok(payload)
}

// --- header encoding/decoding ----------------------------------------------

fn encode_header_and_body(hdr: &MubHeader, body: &[u8]) -> Vec<u8> {
    // header_len is fixed: 4 + 4 + 4 + 4 + 8 + 4 + 4 = 32 bytes
    let header_len: u32 = 32;

    let mut out = Vec::with_capacity(header_len as usize + body.len());
    out.extend_from_slice(&MUB_MAGIC);
    out.extend_from_slice(&hdr.schema_version.to_le_bytes());
    out.extend_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&hdr.payload_kind.to_le_bytes());
    out.extend_from_slice(&hdr.payload_len.to_le_bytes());
    out.extend_from_slice(&hdr.flags.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
    out.extend_from_slice(body);
    out
}

fn decode_header_and_body(bytes: &[u8]) -> Result<(MubHeader, &[u8]), MubError> {
    if bytes.len() < 32 {
        return Err(MubError::CorruptHeader);
    }
    if bytes[0..4] != MUB_MAGIC {
        return Err(MubError::BadMagic);
    }

    let schema_version = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| MubError::CorruptHeader)?);
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| MubError::CorruptHeader)?);
    let payload_kind = u32::from_le_bytes(bytes[12..16].try_into().map_err(|_| MubError::CorruptHeader)?);
    let payload_len = u64::from_le_bytes(bytes[16..24].try_into().map_err(|_| MubError::CorruptHeader)?);
    let flags = u32::from_le_bytes(bytes[24..28].try_into().map_err(|_| MubError::CorruptHeader)?);

    if header_len as usize > bytes.len() || header_len < 32 {
        return Err(MubError::CorruptHeader);
    }

    let body = &bytes[header_len as usize..];
    if body.len() != payload_len as usize {
        // allow trailing bytes? no: keep strict.
        return Err(MubError::CorruptHeader);
    }

    Ok((
        MubHeader {
            schema_version,
            payload_kind,
            payload_len,
            flags,
        },
        body,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::artifact::ArtifactKind;
    use crate::model::graph::{Endpoint, Node, NodeId, NodeKind, Port};

    #[test]
    fn roundtrip_mub() {
        let mut g = Graph::new();
        g.meta.insert("root".into(), ".".into());
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

        let payload = build_payload(g);
        let bytes = encode_mub(&payload).unwrap();
        let decoded = decode_mub(&bytes).unwrap();

        assert_eq!(decoded.schema, payload.schema);
        assert_eq!(decoded.meta.get("profile").unwrap(), "debug");
        assert_eq!(decoded.graph.nodes.len(), 2);
        assert_eq!(decoded.graph.edges.len(), 1);
    }
}
