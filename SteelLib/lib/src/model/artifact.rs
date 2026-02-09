// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\model\artifact.rs

#![allow(clippy::derive_partial_eq_without_eq)]

use core::fmt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Canonical kind of an artifact produced or consumed by the pipeline.
///
/// This is intentionally language-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Source file (e.g. .c, .cpp, .vitte, .rs)
    Source,
    /// Generated source (codegen output)
    GeneratedSource,
    /// Object file (.o/.obj)
    Object,
    /// Static library (.a/.lib)
    StaticLib,
    /// Shared library (.so/.dylib/.dll)
    SharedLib,
    /// Executable (.exe, no extension on Unix)
    Executable,
    /// Debug symbols / PDB / dSYM (sidecar)
    DebugSymbols,
    /// Archive / package (zip, tar, pkg, etc.)
    Package,
    /// Any text report (logs, json, dot, diagnostics)
    Report,
    /// Any config artifact (resolved config, fingerprints, graphs)
    Config,
    /// Unknown or custom artifact kind
    Other(String),
}

impl ArtifactKind {
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            ArtifactKind::Object
                | ArtifactKind::StaticLib
                | ArtifactKind::SharedLib
                | ArtifactKind::Executable
                | ArtifactKind::DebugSymbols
                | ArtifactKind::Package
        )
    }

    pub fn as_str(&self) -> &str {
        match self {
            ArtifactKind::Source => "source",
            ArtifactKind::GeneratedSource => "generated_source",
            ArtifactKind::Object => "object",
            ArtifactKind::StaticLib => "static_lib",
            ArtifactKind::SharedLib => "shared_lib",
            ArtifactKind::Executable => "executable",
            ArtifactKind::DebugSymbols => "debug_symbols",
            ArtifactKind::Package => "package",
            ArtifactKind::Report => "report",
            ArtifactKind::Config => "config",
            ArtifactKind::Other(_) => "other",
        }
    }
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactKind::Other(s) => write!(f, "other({})", s),
            _ => write!(f, "{}", self.as_str()),
        }
    }
}

/// Stability class of an artifact path.
///
/// - `WorkspaceRelative`: stable under repo relocation, recommended for config.
/// - `Absolute`: machine-specific, avoid storing as the primary identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactPathKind {
    WorkspaceRelative,
    Absolute,
}

impl fmt::Display for ArtifactPathKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactPathKind::WorkspaceRelative => write!(f, "workspace_relative"),
            ArtifactPathKind::Absolute => write!(f, "absolute"),
        }
    }
}

/// An artifact reference (input/output) used by the Steel model layer.
///
/// This type is designed to be:
/// - serializable (serde)
/// - deterministic (BTreeMap metadata)
/// - portable (prefers workspace-relative paths)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Artifact identifier (stable logical name), optional.
    ///
    /// Examples: "app.exe", "core.lib", "obj/main.o", "report/graph.dot"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Kind of artifact.
    pub kind: ArtifactKind,

    /// Primary path (workspace-relative preferred).
    pub path: PathBuf,

    /// How to interpret `path`.
    #[serde(default)]
    pub path_kind: ArtifactPathKind,

    /// Optional MIME-like type (ex: "application/json", "text/plain").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Deterministic metadata (ordered map).
    ///
    /// Examples:
    /// - "triple" => "x86_64-pc-windows-msvc"
    /// - "profile" => "debug"
    /// - "tool" => "gcc"
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Artifact {
    /// Create a new artifact with a path interpreted as workspace-relative.
    pub fn new(kind: ArtifactKind, path: impl Into<PathBuf>) -> Self {
        Self {
            id: None,
            kind,
            path: normalize_rel(path.into()),
            path_kind: ArtifactPathKind::WorkspaceRelative,
            content_type: None,
            meta: BTreeMap::new(),
        }
    }

    /// Create a new artifact with an absolute path.
    pub fn new_abs(kind: ArtifactKind, abs_path: impl Into<PathBuf>) -> Self {
        Self {
            id: None,
            kind,
            path: abs_path.into(),
            path_kind: ArtifactPathKind::Absolute,
            content_type: None,
            meta: BTreeMap::new(),
        }
    }

    /// Set a stable logical identifier.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the content-type.
    pub fn with_content_type(mut self, ct: impl Into<String>) -> Self {
        self.content_type = Some(ct.into());
        self
    }

    /// Attach deterministic metadata.
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Resolve to an absolute path given a workspace root.
    ///
    /// - If `path_kind` is absolute, returns `path` unchanged.
    /// - If workspace-relative, joins `root` + `path`.
    pub fn to_abs_path(&self, root: &Path) -> PathBuf {
        match self.path_kind {
            ArtifactPathKind::Absolute => self.path.clone(),
            ArtifactPathKind::WorkspaceRelative => root.join(&self.path),
        }
    }

    /// Ensure path is stored as workspace-relative (best-effort).
    ///
    /// If `abs_path` is not under `root`, keeps absolute.
    pub fn relativize_under(mut self, root: &Path) -> Self {
        let abs = self.to_abs_path(root);
        if let Ok(stripped) = abs.strip_prefix(root) {
            self.path = normalize_rel(stripped.to_path_buf());
            self.path_kind = ArtifactPathKind::WorkspaceRelative;
        } else {
            self.path = abs;
            self.path_kind = ArtifactPathKind::Absolute;
        }
        self
    }

    /// Human-friendly label for logs/diagnostics.
    pub fn label(&self) -> String {
        if let Some(id) = &self.id {
            return id.clone();
        }
        self.path.to_string_lossy().to_string()
    }
}

impl fmt::Display for Artifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.id.as_deref().unwrap_or("-");
        write!(
            f,
            "Artifact{{id={}, kind={}, path_kind={}, path={}}}",
            id,
            self.kind,
            self.path_kind,
            self.path.to_string_lossy()
        )
    }
}

/// Normalizes a workspace-relative path:
/// - strips leading "./"
/// - collapses empty segments
/// - preserves ".." segments (does not canonicalize on FS)
fn normalize_rel(mut p: PathBuf) -> PathBuf {
    // If user passes "./foo/bar", keep it stable as "foo/bar"
    while let Ok(stripped) = p.strip_prefix(".") {
        p = stripped.to_path_buf();
    }

    // Rebuild with clean components (without touching filesystem).
    let mut out = PathBuf::new();
    for c in p.components() {
        use std::path::Component;
        match c {
            Component::CurDir => {}
            Component::Normal(seg) => out.push(seg),
            Component::ParentDir => out.push(".."),
            // Workspace-relative should not include prefixes or root; keep best-effort.
            Component::Prefix(pre) => out.push(pre.as_os_str()),
            Component::RootDir => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_display() {
        assert_eq!(ArtifactKind::Executable.to_string(), "executable");
        assert_eq!(ArtifactKind::Other("x".into()).to_string(), "other(x)");
    }

    #[test]
    fn normalize_rel_strips_dot() {
        let a = Artifact::new(ArtifactKind::Source, "./src/main.c");
        assert_eq!(a.path.to_string_lossy(), "src\\main.c".replace('\\', std::path::MAIN_SEPARATOR.to_string().as_str()));
    }

    #[test]
    fn label_prefers_id() {
        let a = Artifact::new(ArtifactKind::Object, "build/obj.o").with_id("obj");
        assert_eq!(a.label(), "obj");
    }
}
