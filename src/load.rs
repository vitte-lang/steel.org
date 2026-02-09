// src/load.rs
//
// Steel — load (workspace discovery + steelconf/build.muf ingestion)
//
// Purpose:
// - Locate a workspace root and steelconf/build.muf
// - Load + merge configuration layers into a single in-memory Workspace
// - Provide deterministic precedence + clear diagnostics
//
// This module focuses on:
// - discovery: walk up from cwd to find steelconf/build.muf (or use explicit path)
// - reading: uses a small read helper (inline) to read UTF-8 (BOM safe)
// - parsing: stub interface; plug your real parser/lowering pipeline
// - merging: overlays (profile/target/env) applied deterministically
//
// Notes:
// - No external deps.
// - Replace "stub parser" with your actual Steel AST/IR builder.
// - Designed to be used by CLI commands like `build steel`, `check`, etc.
//
// Typical usage:
//   let ctx = LoadCtx::from_cwd(".")?.with_profile("debug");
//   let ws = WorkspaceLoader::new().load(&ctx)?;
//
// Integration points:
// - crate::read (if you have it) can replace the inline read helpers.
// - crate::loadapi could wrap this module; here we implement an all-in-one loader.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::parser::ast::{File as MufFile, Line, LineTokenKind, Stmt, Value};
use crate::parser::{parse_muf, ParseError};

/* ============================== errors/diagnostics ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Io {
        path: PathBuf,
        op: &'static str,
        message: String,
    },
    NotFound {
        what: String,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Invalid {
        message: String,
    },
    Conflict {
        key: String,
        message: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, op, message } => write!(f, "{} {}: {}", op, path.display(), message),
            LoadError::NotFound { what } => write!(f, "not found: {what}"),
            LoadError::Parse { path, message } => write!(f, "parse {}: {}", path.display(), message),
            LoadError::Invalid { message } => write!(f, "invalid: {message}"),
            LoadError::Conflict { key, message } => write!(f, "conflict {key}: {message}"),
        }
    }
}

impl std::error::Error for LoadError {}

fn io_err(path: &Path, op: &'static str, e: std::io::Error) -> LoadError {
    LoadError::Io {
        path: path.to_path_buf(),
        op,
        message: e.to_string(),
    }
}

/* ============================== context ============================== */

#[derive(Debug, Clone)]
pub struct LoadCtx {
    pub cwd: PathBuf,

    /// Optional explicit workspace root.
    pub root_hint: Option<PathBuf>,

    /// Optional explicit steelfile path.
    pub steelfile_hint: Option<PathBuf>,

    /// Active profile & target (optional).
    pub profile: Option<String>,
    pub target: Option<String>,

    /// env var prefix for overrides (ex: MUFFIN_)
    pub env_prefix: String,

    /// if true, missing steelconf doesn't error (returns empty workspace)
    pub allow_missing: bool,
}

impl LoadCtx {
    pub fn from_cwd(cwd: impl Into<PathBuf>) -> Result<Self, LoadError> {
        Ok(Self {
            cwd: cwd.into(),
            root_hint: None,
            steelfile_hint: None,
            profile: None,
            target: None,
            env_prefix: "MUFFIN_".to_string(),
            allow_missing: false,
        })
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root_hint = Some(root.into());
        self
    }

    pub fn with_steelfile(mut self, path: impl Into<PathBuf>) -> Self {
        self.steelfile_hint = Some(path.into());
        self
    }

    pub fn with_profile(mut self, name: impl Into<String>) -> Self {
        self.profile = Some(name.into());
        self
    }

    pub fn with_target(mut self, name: impl Into<String>) -> Self {
        self.target = Some(name.into());
        self
    }

    pub fn with_env_prefix(mut self, p: impl Into<String>) -> Self {
        self.env_prefix = p.into();
        self
    }

    pub fn allow_missing(mut self, yes: bool) -> Self {
        self.allow_missing = yes;
        self
    }
}

/* ============================== workspace model ============================== */

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub steelfile: Option<PathBuf>,

    pub vars: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, Profile>,
    pub targets: BTreeMap<String, Target>,
    pub tools: BTreeMap<String, Tool>,
    pub rules: BTreeMap<String, Rule>,

    pub created_at: SystemTime,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            steelfile: None,
            vars: BTreeMap::new(),
            profiles: BTreeMap::new(),
            targets: BTreeMap::new(),
            tools: BTreeMap::new(),
            rules: BTreeMap::new(),
            created_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub vars: BTreeMap<String, String>,
    pub flags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub triple: Option<String>,
    pub vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub phony: bool,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
    pub deps: Vec<String>,
    pub tool: Option<String>,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tags: BTreeSet<String>,
    pub meta: BTreeMap<String, String>,
}

/* ============================== loader ============================== */

#[derive(Debug, Clone)]
pub struct WorkspaceLoader {
    pub search_filenames: Vec<&'static str>,
    pub allow_overrides: bool,
    pub last_wins: bool,
}

impl Default for WorkspaceLoader {
    fn default() -> Self {
        Self {
            search_filenames: vec!["steelconf", "build.muf"],
            allow_overrides: true,
            last_wins: true,
        }
    }
}

impl WorkspaceLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&self, ctx: &LoadCtx) -> Result<Workspace, LoadError> {
        let root = self.resolve_root(ctx)?;
        let mut ws = Workspace::new(root.clone());

        let steelfile = self.resolve_steelfile(ctx, &root)?;
        ws.steelfile = steelfile.clone();

        // Base layer: from steelconf/build.muf (if any)
        if let Some(path) = &steelfile {
            let text = read_text_utf8(path)?;
            let frag = parse_steelfile(path, &text)?;
            self.merge_fragment(&mut ws, frag, "file")?;
        } else if !ctx.allow_missing {
            return Err(LoadError::NotFound {
                what: format!(
                    "workspace steelfile (searched: {})",
                    self.search_filenames.join(", ")
                ),
            });
        }

        // Env overlay: MUFFIN_VAR_X=... , MUFFIN_PROFILE=... etc.
        let env_frag = load_env_overlay(&ctx.env_prefix);
        self.merge_fragment(&mut ws, env_frag, "env")?;

        // Apply profile/target overlays
        apply_overlays(&mut ws, ctx)?;

        Ok(ws)
    }

    fn resolve_root(&self, ctx: &LoadCtx) -> Result<PathBuf, LoadError> {
        if let Some(r) = &ctx.root_hint {
            return Ok(r.clone());
        }

        // If steelfile hint provided, root is its parent (best-effort).
        if let Some(p) = &ctx.steelfile_hint {
            return Ok(p.parent().unwrap_or_else(|| Path::new(".")).to_path_buf());
        }

        // Otherwise: walk up from cwd to find steelconf/build.muf
        let mut cur = ctx.cwd.clone();
        loop {
            for name in &self.search_filenames {
                let candidate = cur.join(name);
                if candidate.exists() {
                    return Ok(cur);
                }
            }

            if !cur.pop() {
                break;
            }
        }

        // fallback: cwd as root
        Ok(ctx.cwd.clone())
    }

    fn resolve_steelfile(&self, ctx: &LoadCtx, root: &Path) -> Result<Option<PathBuf>, LoadError> {
        if let Some(p) = &ctx.steelfile_hint {
            return Ok(Some(p.clone()));
        }

        for name in &self.search_filenames {
            let candidate = root.join(name);
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn merge_fragment(&self, ws: &mut Workspace, frag: WorkspaceFragment, source: &str) -> Result<(), LoadError> {
        if ws.steelfile.is_none() {
            ws.steelfile = frag.steelfile.clone();
        }

        merge_kv(&mut ws.vars, frag.vars, self, &format!("{source}.vars"))?;
        merge_map(&mut ws.profiles, frag.profiles, self, &format!("{source}.profiles"))?;
        merge_map(&mut ws.targets, frag.targets, self, &format!("{source}.targets"))?;
        merge_map(&mut ws.tools, frag.tools, self, &format!("{source}.tools"))?;
        merge_map(&mut ws.rules, frag.rules, self, &format!("{source}.rules"))?;
        Ok(())
    }
}

/* ============================== fragments + merge ============================== */

#[derive(Debug, Clone, Default)]
pub struct WorkspaceFragment {
    pub steelfile: Option<PathBuf>,
    pub vars: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, Profile>,
    pub targets: BTreeMap<String, Target>,
    pub tools: BTreeMap<String, Tool>,
    pub rules: BTreeMap<String, Rule>,
}

fn merge_kv(
    into: &mut BTreeMap<String, String>,
    other: BTreeMap<String, String>,
    pol: &WorkspaceLoader,
    scope: &str,
) -> Result<(), LoadError> {
    for (k, v) in other {
        if into.contains_key(&k) {
            if !pol.allow_overrides {
                return Err(LoadError::Conflict {
                    key: format!("{scope}.{k}"),
                    message: "duplicate key".to_string(),
                });
            }
            if pol.last_wins {
                into.insert(k, v);
            }
        } else {
            into.insert(k, v);
        }
    }
    Ok(())
}

fn merge_map<T: Clone>(
    into: &mut BTreeMap<String, T>,
    other: BTreeMap<String, T>,
    pol: &WorkspaceLoader,
    scope: &str,
) -> Result<(), LoadError> {
    for (k, v) in other {
        if into.contains_key(&k) {
            if !pol.allow_overrides {
                return Err(LoadError::Conflict {
                    key: format!("{scope}.{k}"),
                    message: "duplicate key".to_string(),
                });
            }
            if pol.last_wins {
                into.insert(k, v);
            }
        } else {
            into.insert(k, v);
        }
    }
    Ok(())
}

/* ============================== overlays ============================== */

fn apply_overlays(ws: &mut Workspace, ctx: &LoadCtx) -> Result<(), LoadError> {
    if let Some(p) = &ctx.profile {
        let prof = ws.profiles.get(p).ok_or_else(|| LoadError::NotFound {
            what: format!("profile '{p}'"),
        })?;
        for (k, v) in &prof.vars {
            ws.vars.insert(k.clone(), v.clone());
        }
    }

    if let Some(t) = &ctx.target {
        let tgt = ws.targets.get(t).ok_or_else(|| LoadError::NotFound {
            what: format!("target '{t}'"),
        })?;
        for (k, v) in &tgt.vars {
            ws.vars.insert(k.clone(), v.clone());
        }
        if let Some(triple) = &tgt.triple {
            ws.vars.insert("target.triple".to_string(), triple.clone());
        }
    }

    Ok(())
}

/* ============================== env overlay ============================== */

fn load_env_overlay(prefix: &str) -> WorkspaceFragment {
    let mut frag = WorkspaceFragment::default();

    // Convention:
    //   {PREFIX}VAR_FOO=bar -> vars["FOO"]="bar"
    //   {PREFIX}PROFILE=debug -> vars["profile"]="debug" (optional)
    //   {PREFIX}TARGET=x86_64 -> vars["target"]="x86_64" (optional)
    let var_prefix = format!("{prefix}VAR_");

    for (k, v) in std::env::vars() {
        if let Some(key) = k.strip_prefix(&var_prefix) {
            frag.vars.insert(key.to_string(), v);
        } else if k == format!("{prefix}PROFILE") {
            frag.vars.insert("profile".to_string(), v);
        } else if k == format!("{prefix}TARGET") {
            frag.vars.insert("target".to_string(), v);
        }
    }

    frag
}

/* ============================== read helpers (utf8) ============================== */

fn read_text_utf8(path: &Path) -> Result<String, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, "read", e))?;
    let bytes = strip_utf8_bom(&bytes);
    let s = std::str::from_utf8(bytes).map_err(|e| LoadError::Parse {
        path: path.to_path_buf(),
        message: format!("invalid utf-8: {e}"),
    })?;
    Ok(s.to_string())
}

fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    if bytes.starts_with(BOM) {
        &bytes[BOM.len()..]
    } else {
        bytes
    }
}

/* ============================== parser + lowering ============================== */

fn parse_steelfile(path: &Path, text: &str) -> Result<WorkspaceFragment, LoadError> {
    let ast = parse_muf(text).map_err(|e| LoadError::Parse {
        path: path.to_path_buf(),
        message: format_parse_error(text, &e),
    })?;
    lower_muf_ast(path, &ast)
}

fn format_parse_error(text: &str, err: &ParseError) -> String {
    let line_no = err.span.start.line;
    let col_no = err.span.start.col;
    let line = text.lines().nth(line_no.saturating_sub(1)).unwrap_or("");
    let display_line = line.replace('\t', "    ");
    let caret_pos = visual_col(line, col_no).saturating_sub(1);
    let mut caret = String::new();
    caret.push_str(&" ".repeat(caret_pos));
    caret.push('^');
    format!(
        "{}\n  --> {}:{}\n   |\n{:4} | {}\n   | {}\nhelp: see syntax.md for MUF syntax",
        err.message, line_no, col_no, line_no, display_line, caret
    )
}

fn visual_col(line: &str, col_no: usize) -> usize {
    if col_no <= 1 {
        return 1;
    }
    let mut col = 1;
    for ch in line.chars() {
        if col >= col_no {
            break;
        }
        if ch == '\t' {
            col += 4;
        } else {
            col += 1;
        }
    }
    col
}

fn lower_muf_ast(path: &Path, ast: &MufFile) -> Result<WorkspaceFragment, LoadError> {
    let mut frag = WorkspaceFragment::default();
    frag.steelfile = Some(path.to_path_buf());
    for stmt in &ast.stmts {
        lower_stmt(stmt, &mut frag)?;
    }
    Ok(frag)
}

fn lower_stmt(stmt: &Stmt, frag: &mut WorkspaceFragment) -> Result<(), LoadError> {
    match stmt {
        Stmt::Set { key, value, .. } => {
            frag.vars.insert(key.clone(), value_to_string(value));
        }
        Stmt::Var { name, value, .. } => {
            frag.vars.insert(name.clone(), value_to_string(value));
        }
        Stmt::Profile { block } => {
            let name = block
                .name
                .clone()
                .ok_or_else(|| LoadError::Invalid {
                    message: "profile block missing name".to_string(),
                })?;
            let mut vars = BTreeMap::new();
            for item in &block.body {
                if let Some((k, v)) = stmt_kv(item) {
                    vars.insert(k, v);
                }
            }
            frag.profiles.insert(
                name.clone(),
                Profile {
                    name,
                    vars,
                    flags: BTreeSet::new(),
                },
            );
        }
        Stmt::Target { block } => {
            let name = block
                .name
                .clone()
                .ok_or_else(|| LoadError::Invalid {
                    message: "target block missing name".to_string(),
                })?;
            let mut vars = BTreeMap::new();
            let mut triple = None;
            for item in &block.body {
                if let Some((k, v)) = stmt_kv(item) {
                    if k == "triple" {
                        triple = Some(v.clone());
                    } else {
                        vars.insert(k, v);
                    }
                }
            }
            frag.targets.insert(
                name.clone(),
                Target {
                    name,
                    triple,
                    vars,
                },
            );
        }
        Stmt::Tool { block } => {
            let name = block
                .name
                .clone()
                .ok_or_else(|| LoadError::Invalid {
                    message: "tool block missing name".to_string(),
                })?;
            let mut program = String::new();
            let mut args = Vec::new();
            let mut env = BTreeMap::new();
            for item in &block.body {
                match item {
                    Stmt::Set { key, value, .. } if key == "exec" => {
                        program = value_to_string(value);
                    }
                    Stmt::Line { line } => {
                        if let Some(v) = line_kv(line, "exec") {
                            program = v;
                        } else if let Some(v) = line_kv(line, "arg") {
                            args.push(v);
                        } else if let Some(vs) = line_list(line, "args") {
                            args.extend(vs);
                        } else if let Some((k, v)) = line_env(line) {
                            env.insert(k, v);
                        }
                    }
                    _ => {}
                }
            }
            frag.tools.insert(
                name.clone(),
                Tool {
                    name,
                    program,
                    args,
                    env,
                },
            );
        }
        Stmt::Block { block } => {
            for item in &block.body {
                lower_stmt(item, frag)?;
            }
        }
        Stmt::Bake { .. }
        | Stmt::Capsule { .. }
        | Stmt::Plan { .. }
        | Stmt::Store { .. }
        | Stmt::Switch { .. } => {}
        Stmt::Line { .. } => {}
    }
    Ok(())
}

fn stmt_kv(stmt: &Stmt) -> Option<(String, String)> {
    match stmt {
        Stmt::Set { key, value, .. } => Some((key.clone(), value_to_string(value))),
        Stmt::Var { name, value, .. } => Some((name.clone(), value_to_string(value))),
        _ => None,
    }
}

fn line_kv(line: &Line, keyword: &str) -> Option<String> {
    let mut iter = line.tokens.iter();
    let first = iter.next()?;
    if !matches!(&first.kind, LineTokenKind::Ident(s) if s == keyword) {
        return None;
    }
    let value = iter.next()?;
    match &value.kind {
        LineTokenKind::Ident(s) => Some(s.clone()),
        LineTokenKind::Str(s) => Some(s.clone()),
        LineTokenKind::Int(v) => Some(v.to_string()),
        _ => None,
    }
}

fn line_list(line: &Line, keyword: &str) -> Option<Vec<String>> {
    let mut iter = line.tokens.iter();
    let first = iter.next()?;
    if !matches!(&first.kind, LineTokenKind::Ident(s) if s == keyword) {
        return None;
    }
    let mut out = Vec::new();
    for tok in iter {
        match &tok.kind {
            LineTokenKind::Ident(s) => out.push(s.clone()),
            LineTokenKind::Str(s) => out.push(s.clone()),
            LineTokenKind::Int(v) => out.push(v.to_string()),
            _ => {}
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn line_env(line: &Line) -> Option<(String, String)> {
    let mut iter = line.tokens.iter();
    let first = iter.next()?;
    if !matches!(&first.kind, LineTokenKind::Ident(s) if s == "env") {
        return None;
    }
    let key = iter.next()?;
    let val = iter.next()?;
    let key_s = match &key.kind {
        LineTokenKind::Ident(s) => s.clone(),
        LineTokenKind::Str(s) => s.clone(),
        _ => return None,
    };
    let val_s = match &val.kind {
        LineTokenKind::Ident(s) => s.clone(),
        LineTokenKind::Str(s) => s.clone(),
        LineTokenKind::Int(v) => v.to_string(),
        _ => return None,
    };
    Some((key_s, val_s))
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Int(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Ident(s) => s.clone(),
        Value::List(items) => items
            .iter()
            .map(value_to_string)
            .collect::<Vec<_>>()
            .join(","),
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overlay_loads_vars() {
        std::env::set_var("MUFFIN_VAR_FOO", "bar");
        let frag = load_env_overlay("MUFFIN_");
        assert_eq!(frag.vars.get("FOO").map(|s| s.as_str()), Some("bar"));
    }

    #[test]
    fn merge_conflict_errors_if_disallowed() {
        let loader = WorkspaceLoader {
            allow_overrides: false,
            last_wins: true,
            ..Default::default()
        };

        let mut ws = Workspace::new(PathBuf::from("."));
        ws.vars.insert("A".to_string(), "1".to_string());

        let mut frag = WorkspaceFragment::default();
        frag.vars.insert("A".to_string(), "2".to_string());

        let err = loader.merge_fragment(&mut ws, frag, "x").unwrap_err();
        assert!(matches!(err, LoadError::Conflict { .. }));
    }
}
