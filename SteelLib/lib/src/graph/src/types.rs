//! Common graph types (IDs, artifacts, actions, nodes) for Steel build graphs.
//!
//! This module is a stable “types” layer that other graph modules can depend on
//! without pulling planning/serialization code.
//!
//! It contains:
//! - stable IDs: `NodeId`, `ArtifactId`
//! - artifact model: `Artifact`, `ArtifactKind`
//! - action model: `Action`
//! - caching: `CacheKey`
//! - node model: `Node`
//!
//! Notes:
//! - ID stability: `stable_hash64()` uses `DefaultHasher` (SipHash) which is not
//!   guaranteed stable across Rust versions. If cross-version stability is
//!   required, gate a fixed hash (xxhash/fnv) behind a feature.
//! - Ordering: uses `BTreeMap/BTreeSet` elsewhere for deterministic iteration.

use std::collections::BTreeMap;
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

/// A stable-ish 64-bit hash for IDs.
/// `DefaultHasher` (SipHash) stability is not guaranteed across rust versions.
pub fn stable_hash64<T: Hash>(v: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_ids_deterministic_for_same_input() {
        let a1 = Artifact::source("src/a.c");
        let a2 = Artifact::source("src/a.c");
        assert_eq!(a1.id, a2.id);
    }

    #[test]
    fn node_id_deterministic_for_same_cfg() {
        let n1 = Node::new("compile", Action::new("clang").arg("-c"));
        let n2 = Node::new("compile", Action::new("clang").arg("-c"));
        assert_eq!(n1.id, n2.id);
    }
}
