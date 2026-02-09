// src/loadapi.rs
//
// Steel — load API (dynamic provider layer)
//
// Purpose:
// - Centralize "loading" of external configuration / data sources into Steel runtime:
//   - steelconf / build.muf parsing results (AST/IR) -> internal models
//   - plugin/registry metadata -> internal structures
//   - workspace config (global + local) -> merged view
//   - environment overlays
//
// This module defines:
// - LoadApi: a facade used by commands (build, check, graph, etc.)
// - Providers: trait-based sources that can be composed
// - LoadContext: shared inputs (cwd, root, profiles, target)
// - Merge rules with deterministic precedence
// - Diagnostics-friendly error model
//
// No external deps.
//
// Notes:
// - The actual parsing of steelconf is not implemented here; plug in your parser.
// - For "max", a small in-memory provider and a file provider stub are included.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::parser::ast::{File as MufFile, Line, LineTokenKind, Stmt, Value};
use crate::parser::parse_muf;

/* ============================== diagnostics/errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Io {
        path: PathBuf,
        op: &'static str,
        message: String,
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
    NotFound {
        what: String,
    },
    Other {
        message: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, op, message } => write!(f, "{} {}: {}", op, path.display(), message),
            LoadError::Parse { path, message } => write!(f, "parse {}: {}", path.display(), message),
            LoadError::Invalid { message } => write!(f, "invalid: {message}"),
            LoadError::Conflict { key, message } => write!(f, "conflict {key}: {message}"),
            LoadError::NotFound { what } => write!(f, "not found: {what}"),
            LoadError::Other { message } => write!(f, "{message}"),
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

/* ============================== core models loaded ============================== */

/// High-level workspace model used by commands.
#[derive(Debug, Clone)]
pub struct WorkspaceModel {
    pub root: PathBuf,
    pub steelfile: Option<PathBuf>,
    pub profiles: BTreeMap<String, ProfileModel>,
    pub targets: BTreeMap<String, TargetModel>,
    pub vars: BTreeMap<String, String>,
    pub tools: BTreeMap<String, ToolModel>,
    pub rules: BTreeMap<String, RuleModel>,
    pub created_at: SystemTime,
}

impl WorkspaceModel {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            steelfile: None,
            profiles: BTreeMap::new(),
            targets: BTreeMap::new(),
            vars: BTreeMap::new(),
            tools: BTreeMap::new(),
            rules: BTreeMap::new(),
            created_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileModel {
    pub name: String,
    pub vars: BTreeMap<String, String>,
    pub flags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct TargetModel {
    pub name: String,
    pub triple: Option<String>,
    pub vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ToolModel {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RuleModel {
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

/* ============================== load context ============================== */

#[derive(Debug, Clone)]
pub struct LoadContext {
    pub cwd: PathBuf,
    pub root_hint: Option<PathBuf>,
    pub steelfile_hint: Option<PathBuf>,

    pub profile: Option<String>,
    pub target: Option<String>,

    /// Strictness knobs
    pub allow_missing_steelfile: bool,
}

impl LoadContext {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            root_hint: None,
            steelfile_hint: None,
            profile: None,
            target: None,
            allow_missing_steelfile: false,
        }
    }
}

/* ============================== provider API ============================== */

pub trait LoadProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Provide partial workspace fragments. Merge order defines precedence.
    fn load(&self, ctx: &LoadContext) -> Result<WorkspaceFragment, LoadError>;
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceFragment {
    pub steelfile: Option<PathBuf>,
    pub profiles: BTreeMap<String, ProfileModel>,
    pub targets: BTreeMap<String, TargetModel>,
    pub vars: BTreeMap<String, String>,
    pub tools: BTreeMap<String, ToolModel>,
    pub rules: BTreeMap<String, RuleModel>,
}

/* ============================== merge semantics ============================== */

#[derive(Debug, Clone)]
pub struct MergePolicy {
    /// If true, later fragments override earlier ones on key collisions.
    pub last_wins: bool,
    /// If false, collisions produce LoadError::Conflict.
    pub allow_overrides: bool,
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self {
            last_wins: true,
            allow_overrides: true,
        }
    }
}

fn merge_maps<T: Clone>(
    into: &mut BTreeMap<String, T>,
    other: BTreeMap<String, T>,
    pol: &MergePolicy,
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

fn merge_kv(
    into: &mut BTreeMap<String, String>,
    other: BTreeMap<String, String>,
    pol: &MergePolicy,
    scope: &str,
) -> Result<(), LoadError> {
    merge_maps(into, other, pol, scope)
}

/* ============================== facade ============================== */

pub struct LoadApi {
    providers: Vec<Box<dyn LoadProvider>>,
    merge: MergePolicy,
}

impl LoadApi {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            merge: MergePolicy::default(),
        }
    }

    pub fn with_merge_policy(mut self, pol: MergePolicy) -> Self {
        self.merge = pol;
        self
    }

    pub fn register(mut self, p: Box<dyn LoadProvider>) -> Self {
        self.providers.push(p);
        self.providers.sort_by_key(|x| x.name());
        self
    }

    pub fn load_workspace(&self, ctx: &LoadContext) -> Result<WorkspaceModel, LoadError> {
        let root = resolve_root(ctx)?;
        let mut ws = WorkspaceModel::new(root);

        for p in &self.providers {
            let frag = p.load(ctx)?;
            if ws.steelfile.is_none() {
                ws.steelfile = frag.steelfile.clone();
            }

            merge_maps(&mut ws.profiles, frag.profiles, &self.merge, "profiles")?;
            merge_maps(&mut ws.targets, frag.targets, &self.merge, "targets")?;
            merge_kv(&mut ws.vars, frag.vars, &self.merge, "vars")?;
            merge_maps(&mut ws.tools, frag.tools, &self.merge, "tools")?;
            merge_maps(&mut ws.rules, frag.rules, &self.merge, "rules")?;
        }

        // Apply profile/target overlays if present
        apply_overlays(&mut ws, ctx)?;

        Ok(ws)
    }
}

fn resolve_root(ctx: &LoadContext) -> Result<PathBuf, LoadError> {
    if let Some(r) = &ctx.root_hint {
        return Ok(r.clone());
    }
    // fallback: cwd as root
    Ok(ctx.cwd.clone())
}

/* ============================== overlays ============================== */

fn apply_overlays(ws: &mut WorkspaceModel, ctx: &LoadContext) -> Result<(), LoadError> {
    if let Some(p) = &ctx.profile {
        let prof = ws.profiles.get(p).ok_or_else(|| LoadError::NotFound {
            what: format!("profile '{p}'"),
        })?;
        // merge vars last-wins
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
        // expose triple as var
        if let Some(triple) = &tgt.triple {
            ws.vars.insert("target.triple".to_string(), triple.clone());
        }
    }

    Ok(())
}

/* ============================== providers ============================== */

/// Provider: in-memory (tests, embedding).
pub struct MemoryProvider {
    frag: WorkspaceFragment,
}

impl MemoryProvider {
    pub fn new(frag: WorkspaceFragment) -> Self {
        Self { frag }
    }
}

impl LoadProvider for MemoryProvider {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn load(&self, _ctx: &LoadContext) -> Result<WorkspaceFragment, LoadError> {
        Ok(self.frag.clone())
    }
}

/// Provider: environment variables overlay.
pub struct EnvProvider {
    pub prefix: String,
}

impl EnvProvider {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }
}

impl LoadProvider for EnvProvider {
    fn name(&self) -> &'static str {
        "env"
    }

    fn load(&self, _ctx: &LoadContext) -> Result<WorkspaceFragment, LoadError> {
        let mut frag = WorkspaceFragment::default();
        for (k, v) in std::env::vars() {
            if let Some(key) = k.strip_prefix(&self.prefix) {
                // Example: MUFFIN_VAR_FOO=bar -> vars["FOO"]="bar"
                frag.vars.insert(key.to_string(), v);
            }
        }
        Ok(frag)
    }
}

/// Provider: file loader stub (steelconf/build.muf).
/// This expects a single file path and returns a Parse error until you hook a parser.
pub struct SteelconfProvider {
    pub path: PathBuf,
}

impl SteelconfProvider {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LoadProvider for SteelconfProvider {
    fn name(&self) -> &'static str {
        "steelfile"
    }

    fn load(&self, ctx: &LoadContext) -> Result<WorkspaceFragment, LoadError> {
        let path = if let Some(p) = &ctx.steelfile_hint {
            p.clone()
        } else {
            self.path.clone()
        };

        if !path.exists() {
            if ctx.allow_missing_steelfile {
                return Ok(WorkspaceFragment {
                    steelfile: None,
                    ..Default::default()
                });
            }
            return Err(LoadError::NotFound {
                what: format!("steelfile {}", path.display()),
            });
        }

        let bytes = std::fs::read(&path).map_err(|e| io_err(&path, "read", e))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| LoadError::Parse {
            path: path.clone(),
            message: format!("invalid utf-8: {e}"),
        })?;

        let ast = parse_muf(text).map_err(|e| LoadError::Parse {
            path: path.clone(),
            message: format!("{} at {}:{}", e.message, e.span.start.line, e.span.start.col),
        })?;

        lower_muf_ast(&path, &ast)
    }
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
                ProfileModel {
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
                TargetModel {
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
                ToolModel {
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
    fn merges_providers_last_wins() {
        let mut frag1 = WorkspaceFragment::default();
        frag1.vars.insert("A".to_string(), "1".to_string());

        let mut frag2 = WorkspaceFragment::default();
        frag2.vars.insert("A".to_string(), "2".to_string());

        let api = LoadApi::new()
            .with_merge_policy(MergePolicy {
                last_wins: true,
                allow_overrides: true,
            })
            .register(Box::new(MemoryProvider::new(frag1)))
            .register(Box::new(MemoryProvider::new(frag2)));

        let ctx = LoadContext::new(PathBuf::from("."));
        let ws = api.load_workspace(&ctx).unwrap();
        assert_eq!(ws.vars.get("A").map(|s| s.as_str()), Some("2"));
    }

    #[test]
    fn applies_profile_overlay() {
        let mut frag = WorkspaceFragment::default();
        frag.profiles.insert(
            "debug".to_string(),
            ProfileModel {
                name: "debug".to_string(),
                vars: {
                    let mut m = BTreeMap::new();
                    m.insert("OPT".to_string(), "0".to_string());
                    m
                },
                flags: BTreeSet::new(),
            },
        );

        let api = LoadApi::new().register(Box::new(MemoryProvider::new(frag)));
        let mut ctx = LoadContext::new(PathBuf::from("."));
        ctx.profile = Some("debug".to_string());

        let ws = api.load_workspace(&ctx).unwrap();
        assert_eq!(ws.vars.get("OPT").map(|s| s.as_str()), Some("0"));
    }
}
