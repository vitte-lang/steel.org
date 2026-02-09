// src/target_file.rs
//
// Steel — target file specification & parsing
//
// Purpose:
// - Represent and parse a "target file" that describes build targets for Steel.
// - Provide a stable in-memory model used by dependency resolution, job planning, and execution.
// - Offer deterministic parsing with good diagnostics.
//
// This is a "max" reference implementation. You can wire it to your actual Steel syntax.
// The parser here supports a pragmatic line-oriented format that maps well to steelconf.
//
// Supported format (example):
//
//   # target file for Steel
//   target app
//     kind exe
//     crate steel
//     out  build/steel
//     src  src/main.vit src/lib.vit
//     deps runner vms
//     defines DEBUG=1 FEATURE_X=on
//     env PATH=/usr/bin
//     args --flag value
//   .end
//
//   target runtime
//     kind staticlib
//     out build/libruntime.a
//     src runtime/*.c
//   .end
//
// Notes:
// - `.end` terminates a target block.
// - Unknown keys are collected as warnings (optional).
// - Paths are kept as strings, resolution happens elsewhere.
// - No regex, no external deps.
//
// Integration:
// - Convert `TargetFileError` to your diagnostics system.
// - If you already have a tokenizer, replace the parsing layer, keep the model.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/* ============================== model ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetFile {
    pub path: Option<PathBuf>,
    pub targets: Vec<TargetSpec>,
    pub warnings: Vec<TargetFileWarning>,
}

impl TargetFile {
    pub fn new() -> Self {
        Self {
            path: None,
            targets: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&TargetSpec> {
        self.targets.iter().find(|t| t.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.targets.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn validate_basic(&self) -> Result<(), TargetFileError> {
        let mut seen = BTreeSet::<&str>::new();
        for t in &self.targets {
            if t.name.trim().is_empty() {
                return Err(TargetFileError::InvalidTargetName { name: t.name.clone() });
            }
            if !seen.insert(t.name.as_str()) {
                return Err(TargetFileError::DuplicateTarget { name: t.name.clone() });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub name: String,
    pub kind: TargetKind,

    pub out: Option<String>,
    pub crate_name: Option<String>,

    pub sources: Vec<String>,
    pub deps: Vec<String>,

    pub defines: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,

    pub args: Vec<String>,

    pub meta: BTreeMap<String, String>, // future keys
    pub span: Span,
}

impl TargetSpec {
    pub fn new(name: String, span: Span) -> Self {
        Self {
            name,
            kind: TargetKind::Unknown,
            out: None,
            crate_name: None,
            sources: Vec::new(),
            deps: Vec::new(),
            defines: BTreeMap::new(),
            env: BTreeMap::new(),
            args: Vec::new(),
            meta: BTreeMap::new(),
            span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    Exe,
    StaticLib,
    SharedLib,
    Obj,
    Custom,
    Unknown,
}

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TargetKind::Exe => "exe",
            TargetKind::StaticLib => "staticlib",
            TargetKind::SharedLib => "sharedlib",
            TargetKind::Obj => "obj",
            TargetKind::Custom => "custom",
            TargetKind::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetFileWarning {
    pub message: String,
    pub span: Span,
}

/* ============================== spans/errors ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub line_end: u32,
    pub col_end: u32,
}

impl Span {
    pub fn line(line: u32) -> Self {
        Self {
            line,
            col: 0,
            line_end: line,
            col_end: 0,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            return Ok(());
        }
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetFileError {
    Io { path: PathBuf, message: String },
    Parse { message: String, span: Span },

    UnexpectedEof { message: String, span: Span },
    UnexpectedToken { message: String, span: Span },

    DuplicateTarget { name: String },
    InvalidTargetName { name: String },
}

impl fmt::Display for TargetFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetFileError::Io { path, message } => write!(f, "I/O error reading {}: {}", path.display(), message),
            TargetFileError::Parse { message, span } => write!(f, "parse error @{}: {}", span, message),
            TargetFileError::UnexpectedEof { message, span } => write!(f, "unexpected EOF @{}: {}", span, message),
            TargetFileError::UnexpectedToken { message, span } => write!(f, "unexpected token @{}: {}", span, message),
            TargetFileError::DuplicateTarget { name } => write!(f, "duplicate target '{name}'"),
            TargetFileError::InvalidTargetName { name } => write!(f, "invalid target name '{name}'"),
        }
    }
}

impl std::error::Error for TargetFileError {}

/* ============================== parsing ============================== */

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub allow_unknown_keys: bool,
    pub allow_empty_targets: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            allow_unknown_keys: true,
            allow_empty_targets: false,
        }
    }
}

pub fn parse_target_file_str(input: &str, opts: &ParseOptions) -> Result<TargetFile, TargetFileError> {
    let mut tf = TargetFile::new();

    let mut lines = input.lines().enumerate().peekable();
    let mut current: Option<TargetSpec> = None;

    while let Some((idx0, raw)) = lines.next() {
        let line_no = (idx0 + 1) as u32;
        let span = Span::line(line_no);
        let line = strip_comment(raw).trim();

        if line.is_empty() {
            continue;
        }

        // Block terminator
        if line == ".end" {
            if let Some(t) = current.take() {
                tf.targets.push(t);
            } else {
                return Err(TargetFileError::UnexpectedToken {
                    message: "unexpected .end (not inside target block)".to_string(),
                    span,
                });
            }
            continue;
        }

        // Start of block: target NAME
        if let Some(rest) = line.strip_prefix("target ") {
            if current.is_some() {
                return Err(TargetFileError::UnexpectedToken {
                    message: "nested target blocks are not allowed".to_string(),
                    span,
                });
            }
            let name = rest.trim().to_string();
            if name.is_empty() {
                return Err(TargetFileError::Parse {
                    message: "target name missing".to_string(),
                    span,
                });
            }
            current = Some(TargetSpec::new(name, span));
            continue;
        }

        // Must be inside a target block for key lines
        let Some(t) = current.as_mut() else {
            return Err(TargetFileError::UnexpectedToken {
                message: "statement outside of target block".to_string(),
                span,
            });
        };

        // key value...
        let (key, value) = split_key_value(line).ok_or_else(|| TargetFileError::Parse {
            message: "expected 'key value...'".to_string(),
            span,
        })?;

        match key {
            "kind" => {
                t.kind = parse_kind(value).ok_or_else(|| TargetFileError::Parse {
                    message: format!("unknown kind '{value}'"),
                    span,
                })?;
            }
            "out" => {
                t.out = Some(value.to_string());
            }
            "crate" => {
                t.crate_name = Some(value.to_string());
            }
            "src" => {
                t.sources.extend(split_list(value));
            }
            "deps" => {
                t.deps.extend(split_list(value));
            }
            "args" => {
                t.args.extend(split_list(value));
            }
            "tool" => {
                t.meta.insert("tool".to_string(), value.to_string());
            }
            "cmd" => {
                t.meta.insert("cmd".to_string(), value.to_string());
            }
            "defines" => {
                for kv in split_list(value) {
                    let (k, v) = split_kv(&kv).ok_or_else(|| TargetFileError::Parse {
                        message: format!("invalid define '{kv}', expected KEY=VALUE"),
                        span,
                    })?;
                    t.defines.insert(k.to_string(), v.to_string());
                }
            }
            "env" => {
                for kv in split_list(value) {
                    let (k, v) = split_kv(&kv).ok_or_else(|| TargetFileError::Parse {
                        message: format!("invalid env '{kv}', expected KEY=VALUE"),
                        span,
                    })?;
                    t.env.insert(k.to_string(), v.to_string());
                }
            }
            _ => {
                if opts.allow_unknown_keys {
                    t.meta.insert(key.to_string(), value.to_string());
                    tf.warnings.push(TargetFileWarning {
                        message: format!("unknown key '{key}' (stored in meta)"),
                        span,
                    });
                } else {
                    return Err(TargetFileError::Parse {
                        message: format!("unknown key '{key}'"),
                        span,
                    });
                }
            }
        }
    }

    // EOF
    if let Some(t) = current.take() {
        return Err(TargetFileError::UnexpectedEof {
            message: format!("target '{}' missing .end", t.name),
            span: t.span,
        });
    }

    if !opts.allow_empty_targets && tf.targets.is_empty() {
        tf.warnings.push(TargetFileWarning {
            message: "no targets declared".to_string(),
            span: Span::default(),
        });
    }

    // basic validation (duplicate names etc.)
    tf.validate_basic()?;

    Ok(tf)
}

pub fn parse_target_file_path(path: &Path, opts: &ParseOptions) -> Result<TargetFile, TargetFileError> {
    let content = std::fs::read_to_string(path).map_err(|e| TargetFileError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let mut tf = parse_target_file_str(&content, opts)?;
    tf.path = Some(path.to_path_buf());
    Ok(tf)
}

/* ============================== helpers ============================== */

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let mut it = line.splitn(2, char::is_whitespace);
    let key = it.next()?.trim();
    let rest = it.next()?.trim();
    if key.is_empty() || rest.is_empty() {
        None
    } else {
        Some((key, rest))
    }
}

fn split_list(s: &str) -> Vec<String> {
    // Minimal splitting with basic quote support.
    let mut out = Vec::<String>::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;

    for c in s.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
            continue;
        }

        if c == '"' || c == '\'' {
            quote = Some(c);
            continue;
        }

        if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        } else {
            cur.push(c);
        }
    }

    if !cur.is_empty() {
        out.push(cur);
    }

    out
}

fn split_kv(s: &str) -> Option<(&str, &str)> {
    let (k, v) = s.split_once('=')?;
    let k = k.trim();
    let v = v.trim();
    if k.is_empty() {
        None
    } else {
        Some((k, v))
    }
}

fn parse_kind(s: &str) -> Option<TargetKind> {
    match s.trim().to_ascii_lowercase().as_str() {
        "exe" | "bin" | "binary" => Some(TargetKind::Exe),
        "staticlib" | "static" | "a" | "lib" => Some(TargetKind::StaticLib),
        "sharedlib" | "shared" | "so" | "dll" | "dylib" => Some(TargetKind::SharedLib),
        "obj" | "object" | "o" => Some(TargetKind::Obj),
        "custom" => Some(TargetKind::Custom),
        "unknown" => Some(TargetKind::Unknown),
        _ => None,
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_target() {
        let src = r#"
# target file
target app
  kind exe
  out build/steel
  src src/main.vit src/lib.vit
  deps runner vms
  defines DEBUG=1 FEATURE=on
  env PATH=/usr/bin
  args --flag value
.end
"#;

        let tf = parse_target_file_str(src, &ParseOptions::default()).unwrap();
        assert_eq!(tf.targets.len(), 1);
        let t = &tf.targets[0];
        assert_eq!(t.name, "app");
        assert_eq!(t.kind, TargetKind::Exe);
        assert_eq!(t.out.as_deref(), Some("build/steel"));
        assert!(t.sources.contains(&"src/main.vit".to_string()));
        assert!(t.deps.contains(&"runner".to_string()));
        assert_eq!(t.defines.get("DEBUG").map(|s| s.as_str()), Some("1"));
        assert_eq!(t.env.get("PATH").map(|s| s.as_str()), Some("/usr/bin"));
        assert_eq!(t.args[0], "--flag");
    }

    #[test]
    fn parse_requires_end() {
        let src = r#"
target x
  kind exe
"#;
        let err = parse_target_file_str(src, &ParseOptions::default()).unwrap_err();
        assert!(matches!(err, TargetFileError::UnexpectedEof { .. }));
    }

    #[test]
    fn duplicate_target_rejected() {
        let src = r#"
target a
  kind exe
.end
target a
  kind exe
.end
"#;
        let err = parse_target_file_str(src, &ParseOptions::default()).unwrap_err();
        assert!(matches!(err, TargetFileError::DuplicateTarget { .. }));
    }

    #[test]
    fn unknown_keys_warn() {
        let src = r#"
target a
  kind exe
  foo bar
.end
"#;
        let tf = parse_target_file_str(src, &ParseOptions::default()).unwrap();
        assert_eq!(tf.targets.len(), 1);
        assert!(!tf.warnings.is_empty());
        assert_eq!(tf.targets[0].meta.get("foo").map(|s| s.as_str()), Some("bar"));
    }
}
