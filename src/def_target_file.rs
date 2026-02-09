// /Users/vincent/Documents/Github/steel/src/def_target_file.rs
//! def_target_file — target file definition + serialization (std-only)
//!
//! This module defines a compact, explicit representation for a resolved Target,
//! suitable for inclusion in `steelconfig.mff` and/or for consumption by a build runner.
//!
//! Key design points:
//! - deterministic ordering (BTreeMap/BTreeSet)
//! - no external crates
//! - explicit host/target selection + options
//! - optional lists of rules/steps (can be expanded later)
//!
//! Not a full build graph. The execution layer owns DAG orchestration; Steel emits
//! resolved target *configuration* and optional rule metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// Target kind (high-level intent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    Program,
    Service,
    Library,
    Kernel,
    Driver,
    Tool,
    Pipeline,
    Scenario,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Program => "program",
            TargetKind::Service => "service",
            TargetKind::Library => "library",
            TargetKind::Kernel => "kernel",
            TargetKind::Driver => "driver",
            TargetKind::Tool => "tool",
            TargetKind::Pipeline => "pipeline",
            TargetKind::Scenario => "scenario",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "program" => Some(TargetKind::Program),
            "service" => Some(TargetKind::Service),
            "library" => Some(TargetKind::Library),
            "kernel" => Some(TargetKind::Kernel),
            "driver" => Some(TargetKind::Driver),
            "tool" => Some(TargetKind::Tool),
            "pipeline" => Some(TargetKind::Pipeline),
            "scenario" => Some(TargetKind::Scenario),
            _ => None,
        }
    }
}

/// Output artifact type (what the build runner is expected to produce).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutputKind {
    Exe,
    StaticLib,
    SharedLib,
    Object,
    Archive,
    Package,
    Image,
    Custom,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OutputKind::Exe => "exe",
            OutputKind::StaticLib => "staticlib",
            OutputKind::SharedLib => "sharedlib",
            OutputKind::Object => "object",
            OutputKind::Archive => "archive",
            OutputKind::Package => "package",
            OutputKind::Image => "image",
            OutputKind::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "exe" => Some(OutputKind::Exe),
            "staticlib" | "static" => Some(OutputKind::StaticLib),
            "sharedlib" | "shared" | "dylib" | "so" => Some(OutputKind::SharedLib),
            "object" | "obj" => Some(OutputKind::Object),
            "archive" => Some(OutputKind::Archive),
            "package" => Some(OutputKind::Package),
            "image" => Some(OutputKind::Image),
            "custom" => Some(OutputKind::Custom),
            _ => None,
        }
    }
}

/// A fully resolved build target definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDef {
    /// Stable identifier (must be unique inside a workspace).
    pub id: String,

    /// Human-readable name (optional).
    pub name: Option<String>,

    pub kind: TargetKind,
    pub output: OutputKind,

    /// Root path for the target (usually a package directory).
    pub root: PathBuf,

    /// Sources (explicit list, post-resolution).
    pub sources: BTreeSet<PathBuf>,

    /// Include directories.
    pub include_dirs: BTreeSet<PathBuf>,

    /// Dependencies by id (targets or packages, depending on model).
    pub deps: BTreeSet<String>,

    /// Key-value options (compiler flags, feature toggles, etc.).
    pub options: BTreeMap<String, String>,

    /// Optional: produced artifacts (relative to dist/build).
    pub outputs: BTreeSet<PathBuf>,
}

impl TargetDef {
    pub fn new(id: impl Into<String>, kind: TargetKind, output: OutputKind, root: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            name: None,
            kind,
            output,
            root: root.into(),
            sources: BTreeSet::new(),
            include_dirs: BTreeSet::new(),
            deps: BTreeSet::new(),
            options: BTreeMap::new(),
            outputs: BTreeSet::new(),
        }
    }
}

/// Validation errors for target definitions.
#[derive(Debug, Clone)]
pub struct TargetError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TargetError {}

pub type Result<T> = std::result::Result<T, TargetError>;

/// Validate a target definition (best-effort; returns first error).
pub fn validate_target_def(t: &TargetDef) -> Result<()> {
    let id = t.id.trim();
    if id.is_empty() {
        return Err(TargetError {
            code: "TGT_ID_EMPTY",
            message: "target id is empty".to_string(),
        });
    }
    if !is_ident_like(id) {
        return Err(TargetError {
            code: "TGT_ID_INVALID",
            message: format!("target id contains invalid characters: {id}"),
        });
    }

    if t.root.as_os_str().is_empty() {
        return Err(TargetError {
            code: "TGT_ROOT_EMPTY",
            message: "target root is empty".to_string(),
        });
    }

    Ok(())
}

/// Serialize a target into a deterministic text block for inclusion in `.mff`.
///
/// Example:
/// ```text
/// target "app"
///   kind "program"
///   output "exe"
///   root "packages/app"
///   sources
///     "src/main.vit"
///   .end
///   options
///     set "opt.level" "2"
///   .end
/// .end
/// ```
pub fn format_target_block(t: &TargetDef) -> String {
    let mut out = String::new();

    out.push_str(&format!("target \"{}\"\n", escape(&t.id)));
    out.push_str(&format!("  kind \"{}\"\n", t.kind.as_str()));
    out.push_str(&format!("  output \"{}\"\n", t.output.as_str()));

    if let Some(name) = &t.name {
        out.push_str(&format!("  name \"{}\"\n", escape(name)));
    }

    out.push_str(&format!("  root \"{}\"\n", escape(&t.root.to_string_lossy())));

    if !t.sources.is_empty() {
        out.push_str("  sources\n");
        for p in &t.sources {
            out.push_str(&format!("    \"{}\"\n", escape(&p.to_string_lossy())));
        }
        out.push_str("  .end\n");
    }

    if !t.include_dirs.is_empty() {
        out.push_str("  include_dirs\n");
        for p in &t.include_dirs {
            out.push_str(&format!("    \"{}\"\n", escape(&p.to_string_lossy())));
        }
        out.push_str("  .end\n");
    }

    if !t.deps.is_empty() {
        out.push_str("  deps\n");
        for d in &t.deps {
            out.push_str(&format!("    \"{}\"\n", escape(d)));
        }
        out.push_str("  .end\n");
    }

    if !t.options.is_empty() {
        out.push_str("  options\n");
        for (k, v) in &t.options {
            out.push_str(&format!("    set \"{}\" \"{}\"\n", escape(k), escape(v)));
        }
        out.push_str("  .end\n");
    }

    if !t.outputs.is_empty() {
        out.push_str("  outputs\n");
        for p in &t.outputs {
            out.push_str(&format!("    \"{}\"\n", escape(&p.to_string_lossy())));
        }
        out.push_str("  .end\n");
    }

    out.push_str(".end\n");
    out
}

/// Parse a target block from an extremely small line-based format.
///
/// This is *not* the steelconf grammar. This is a utility parser for the emitted `.mff` blocks
/// in tests/tools, using a minimal subset:
/// - `target "<id>"` begins a block
/// - `kind "<k>"`, `output "<o>"`, `root "<path>"`, `name "<name>"`
/// - section blocks `sources|include_dirs|deps|options|outputs` with `.end` terminators
/// - final `.end` ends the target
pub fn parse_target_block(lines: &[&str]) -> Result<TargetDef> {
    let mut i = 0;

    let (id, mut t) = {
        let l = lines.get(i).ok_or(TargetError {
            code: "TGT_PARSE_EOF",
            message: "empty input".to_string(),
        })?;
        let (kw, rest) = split_kw(l)?;
        if kw != "target" {
            return Err(TargetError {
                code: "TGT_PARSE_EXPECT",
                message: "expected `target \"id\"`".to_string(),
            });
        }
        let id = parse_quoted(rest)?;
        let t = TargetDef::new(id.clone(), TargetKind::Program, OutputKind::Exe, PathBuf::from("."));
        (id, t)
    };

    i += 1;

    while i < lines.len() {
        let l = lines[i].trim();
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l == ".end" {
            return Ok(t);
        }

        let (kw, rest) = split_kw(l)?;
        match kw {
            "kind" => {
                let v = parse_quoted(rest)?;
                t.kind = TargetKind::parse(&v).ok_or(TargetError {
                    code: "TGT_PARSE_KIND",
                    message: format!("unknown kind: {v}"),
                })?;
                i += 1;
            }
            "output" => {
                let v = parse_quoted(rest)?;
                t.output = OutputKind::parse(&v).ok_or(TargetError {
                    code: "TGT_PARSE_OUTPUT",
                    message: format!("unknown output: {v}"),
                })?;
                i += 1;
            }
            "root" => {
                let v = parse_quoted(rest)?;
                t.root = PathBuf::from(v);
                i += 1;
            }
            "name" => {
                let v = parse_quoted(rest)?;
                t.name = Some(v);
                i += 1;
            }
            "sources" => {
                i = parse_list_block(lines, i + 1, &mut t.sources)?;
            }
            "include_dirs" => {
                i = parse_list_block(lines, i + 1, &mut t.include_dirs)?;
            }
            "deps" => {
                let mut set = BTreeSet::new();
                i = parse_string_list_block(lines, i + 1, &mut set)?;
                t.deps = set;
            }
            "options" => {
                let mut map = BTreeMap::new();
                i = parse_kv_block(lines, i + 1, &mut map)?;
                t.options = map;
            }
            "outputs" => {
                i = parse_list_block(lines, i + 1, &mut t.outputs)?;
            }
            _ => {
                return Err(TargetError {
                    code: "TGT_PARSE_FIELD",
                    message: format!("unknown field `{kw}` in target {id}"),
                });
            }
        }
    }

    Err(TargetError {
        code: "TGT_PARSE_NO_END",
        message: "target block missing final .end".to_string(),
    })
}

fn parse_list_block(
    lines: &[&str],
    mut i: usize,
    out: &mut BTreeSet<PathBuf>,
) -> Result<usize> {
    while i < lines.len() {
        let l = lines[i].trim();
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l == ".end" {
            return Ok(i + 1);
        }
        let s = parse_quoted(l)?;
        out.insert(PathBuf::from(s));
        i += 1;
    }
    Err(TargetError {
        code: "TGT_PARSE_LIST_EOF",
        message: "list block missing .end".to_string(),
    })
}

fn parse_string_list_block(
    lines: &[&str],
    mut i: usize,
    out: &mut BTreeSet<String>,
) -> Result<usize> {
    while i < lines.len() {
        let l = lines[i].trim();
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l == ".end" {
            return Ok(i + 1);
        }
        let s = parse_quoted(l)?;
        out.insert(s);
        i += 1;
    }
    Err(TargetError {
        code: "TGT_PARSE_LIST_EOF",
        message: "string list block missing .end".to_string(),
    })
}

fn parse_kv_block(
    lines: &[&str],
    mut i: usize,
    out: &mut BTreeMap<String, String>,
) -> Result<usize> {
    while i < lines.len() {
        let l = lines[i].trim();
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l == ".end" {
            return Ok(i + 1);
        }
        // Expect: set "k" "v"
        let (kw, rest) = split_kw(l)?;
        if kw != "set" {
            return Err(TargetError {
                code: "TGT_PARSE_KV",
                message: "expected `set \"k\" \"v\"`".to_string(),
            });
        }
        let (k, v) = parse_two_quoted(rest)?;
        out.insert(k, v);
        i += 1;
    }
    Err(TargetError {
        code: "TGT_PARSE_KV_EOF",
        message: "kv block missing .end".to_string(),
    })
}

fn split_kw(line: &str) -> Result<(&str, &str)> {
    let l = line.trim();
    let mut it = l.splitn(2, char::is_whitespace);
    let kw = it.next().unwrap_or("");
    let rest = it.next().unwrap_or("").trim();
    if kw.is_empty() {
        return Err(TargetError {
            code: "TGT_PARSE_EMPTY",
            message: "empty line".to_string(),
        });
    }
    Ok((kw, rest))
}

fn parse_quoted(s: &str) -> Result<String> {
    let t = s.trim();
    if !t.starts_with('"') {
        return Err(TargetError {
            code: "TGT_PARSE_QUOTE",
            message: format!("expected quoted string, got: {t}"),
        });
    }
    let mut out = String::new();
    let mut chars = t.chars();
    let _ = chars.next(); // opening quote

    let mut escaped = false;
    for ch in chars {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Ok(out),
            c => out.push(c),
        }
    }

    Err(TargetError {
        code: "TGT_PARSE_UNTERM",
        message: "unterminated string".to_string(),
    })
}

fn parse_two_quoted(s: &str) -> Result<(String, String)> {
    // crude but deterministic parser for: "a" "b"
    let first = parse_quoted(s)?;
    let rest = s.trim();
    let idx = rest.find('"').ok_or(TargetError {
        code: "TGT_PARSE_TWO",
        message: "expected two quoted strings".to_string(),
    })?;
    // skip first quoted token by re-scanning
    let mut pos = idx + 1;
    let mut escaped = false;
    for (j, ch) in rest[pos..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => {
                pos = pos + j + 1;
                break;
            }
            _ => {}
        }
    }
    let rest2 = rest[pos..].trim();
    if rest2.is_empty() {
        return Err(TargetError {
            code: "TGT_PARSE_TWO",
            message: "expected second quoted string".to_string(),
        });
    }
    let second = parse_quoted(rest2)?;
    Ok((first, second))
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o
}

fn is_ident_like(s: &str) -> bool {
    // `[A-Za-z_][A-Za-z0-9._-]*`
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' ))
}

/// Helper: join a path under root if relative.
pub fn root_join_if_relative(root: &Path, p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_block_roundtrip() {
        let mut t = TargetDef::new("app", TargetKind::Program, OutputKind::Exe, "packages/app");
        t.sources.insert(PathBuf::from("src/main.vit"));
        t.include_dirs.insert(PathBuf::from("include"));
        t.deps.insert("core".to_string());
        t.options.insert("opt.level".to_string(), "2".to_string());
        t.outputs.insert(PathBuf::from("dist/app"));

        validate_target_def(&t).unwrap();

        let text = format_target_block(&t);
        let lines: Vec<&str> = text.lines().collect();
        let parsed = parse_target_block(&lines).unwrap();

        assert_eq!(parsed.id, "app");
        assert_eq!(parsed.kind, TargetKind::Program);
        assert_eq!(parsed.output, OutputKind::Exe);
        assert!(parsed.sources.contains(Path::new("src/main.vit")));
        assert!(parsed.include_dirs.contains(Path::new("include")));
        assert!(parsed.deps.contains("core"));
        assert_eq!(parsed.options.get("opt.level").map(|s| s.as_str()), Some("2"));
    }
}
