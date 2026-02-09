// src/generator.rs
//
// Steel — generator (emit files, response files, manifests, graphs)
//
// Purpose:
// - Convert internal models (Workspace, Rule, Plan, JobsReport) into artifacts:
//   - generated source files (headers, config, stamp files)
//   - response files (.rsp) for toolchains (clang/gcc/link)
//   - build metadata (json-ish, text) for IDE/CI
//   - dependency graph outputs (dot)
//   - "stamp" outputs for incremental checks
//
// This module is dependency-free and uses simple, explicit formatting.
// For JSON, it emits a minimal JSON serializer (no external crate).
//
// Notes:
// - Replace local model copies with `use crate::...` as needed.
// - Keep generator deterministic: BTreeMap ordering, stable formatting.
// - "max": includes multiple generators and a mini templating facility.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    Io { path: PathBuf, op: &'static str, message: String },
    Invalid(String),
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::Io { path, op, message } => write!(f, "{} {}: {}", op, path.display(), message),
            GenError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for GenError {}

fn io_err(path: &Path, op: &'static str, e: io::Error) -> GenError {
    GenError::Io {
        path: path.to_path_buf(),
        op,
        message: e.to_string(),
    }
}

/* ============================== minimal models (adapt to your crate) ============================== */

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub steelfile: Option<PathBuf>,
    pub vars: BTreeMap<String, String>,
    pub tools: BTreeMap<String, Tool>,
    pub rules: BTreeMap<String, Rule>,
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

/* ============================== generator facade ============================== */

#[derive(Debug, Clone)]
pub struct Generator {
    pub emit_dir: PathBuf,
    pub overwrite: bool,
}

impl Generator {
    pub fn new(emit_dir: PathBuf) -> Self {
        Self {
            emit_dir,
            overwrite: true,
        }
    }

    pub fn ensure_dir(&self) -> Result<(), GenError> {
        std::fs::create_dir_all(&self.emit_dir).map_err(|e| io_err(&self.emit_dir, "mkdir", e))?;
        Ok(())
    }

    pub fn write_text(&self, rel: &str, text: &str) -> Result<PathBuf, GenError> {
        self.ensure_dir()?;
        let path = self.emit_dir.join(rel);
        if path.exists() && !self.overwrite {
            return Err(GenError::Invalid(format!("refuse overwrite: {}", path.display())));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_err(parent, "mkdir", e))?;
        }
        std::fs::write(&path, text.as_bytes()).map_err(|e| io_err(&path, "write", e))?;
        Ok(path)
    }
}

/* ============================== response files (.rsp) ============================== */

/// Generate a response file content (one arg per line, quoted if needed).
pub fn gen_rsp(args: &[String]) -> String {
    let mut out = String::new();
    for a in args {
        out.push_str(&quote_rsp(a));
        out.push('\n');
    }
    out
}

fn quote_rsp(s: &str) -> String {
    // Conservative quoting:
    // - If contains whitespace or quotes, wrap in double quotes and escape internal quotes/backslashes.
    let needs = s.chars().any(|c| c.is_whitespace() || c == '"' || c == '\\');
    if !needs {
        return s.to_string();
    }
    let mut o = String::new();
    o.push('"');
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            _ => o.push(ch),
        }
    }
    o.push('"');
    o
}

/* ============================== dot graph ============================== */

pub fn gen_dot(ws: &Workspace) -> String {
    let mut out = String::new();
    out.push_str("digraph steel {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box];\n");

    // nodes
    for (name, r) in &ws.rules {
        let label = if r.phony { format!("{name}\\n(phony)") } else { name.clone() };
        out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", escape_dot(name), escape_dot(&label)));
    }

    // edges
    for (name, r) in &ws.rules {
        for d in &r.deps {
            if ws.rules.contains_key(d) {
                out.push_str(&format!(
                    "  \"{}\" -> \"{}\";\n",
                    escape_dot(d),
                    escape_dot(name)
                ));
            }
        }
    }

    out.push_str("}\n");
    out
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/* ============================== stamp files ============================== */

/// A stamp file is a deterministic summary of a rule definition.
/// Useful for detecting rule changes even when outputs exist.
///
/// This is NOT a cryptographic signature. Combine with src/hash.rs if needed.
pub fn gen_rule_stamp(r: &Rule) -> String {
    let mut out = String::new();
    out.push_str("rule:\n");
    out.push_str(&format!("  name: {}\n", r.name));
    out.push_str(&format!("  phony: {}\n", r.phony));
    out.push_str(&format!("  tool: {}\n", r.tool.as_deref().unwrap_or("")));
    out.push_str("  inputs:\n");
    for p in &r.inputs {
        out.push_str(&format!("    - {}\n", norm_path(p)));
    }
    out.push_str("  outputs:\n");
    for p in &r.outputs {
        out.push_str(&format!("    - {}\n", norm_path(p)));
    }
    out.push_str("  deps:\n");
    for d in &r.deps {
        out.push_str(&format!("    - {}\n", d));
    }
    out.push_str("  argv:\n");
    for a in &r.argv {
        out.push_str(&format!("    - {}\n", a));
    }
    out.push_str("  env:\n");
    for (k, v) in &r.env {
        out.push_str(&format!("    {}={}\n", k, v));
    }
    out.push_str("  tags:\n");
    for t in &r.tags {
        out.push_str(&format!("    - {}\n", t));
    }
    out.push_str("  meta:\n");
    for (k, v) in &r.meta {
        out.push_str(&format!("    {}={}\n", k, v));
    }
    out
}

fn norm_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/* ============================== mini JSON serializer ============================== */

#[derive(Debug, Clone)]
pub enum Json {
    Null,
    Bool(bool),
    Num(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

pub fn json_stringify(v: &Json) -> String {
    let mut out = String::new();
    json_write(v, &mut out);
    out
}

fn json_write(v: &Json, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(&n.to_string()),
        Json::Str(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push(ch),
                }
            }
            out.push('"');
        }
        Json::Arr(a) => {
            out.push('[');
            let mut first = true;
            for x in a {
                if !first {
                    out.push(',');
                }
                first = false;
                json_write(x, out);
            }
            out.push(']');
        }
        Json::Obj(m) => {
            out.push('{');
            let mut first = true;
            for (k, v) in m {
                if !first {
                    out.push(',');
                }
                first = false;
                json_write(&Json::Str(k.clone()), out);
                out.push(':');
                json_write(v, out);
            }
            out.push('}');
        }
    }
}

/* ============================== workspace export ============================== */

pub fn gen_workspace_json(ws: &Workspace) -> Json {
    let mut obj = BTreeMap::new();
    obj.insert("root".to_string(), Json::Str(norm_path(&ws.root)));
    obj.insert(
        "steelfile".to_string(),
        match &ws.steelfile {
            Some(p) => Json::Str(norm_path(p)),
            None => Json::Null,
        },
    );

    let mut vars = BTreeMap::new();
    for (k, v) in &ws.vars {
        vars.insert(k.clone(), Json::Str(v.clone()));
    }
    obj.insert("vars".to_string(), Json::Obj(vars));

    let mut tools = BTreeMap::new();
    for (name, t) in &ws.tools {
        let mut to = BTreeMap::new();
        to.insert("program".to_string(), Json::Str(t.program.clone()));
        to.insert(
            "args".to_string(),
            Json::Arr(t.args.iter().cloned().map(Json::Str).collect()),
        );
        let mut env = BTreeMap::new();
        for (k, v) in &t.env {
            env.insert(k.clone(), Json::Str(v.clone()));
        }
        to.insert("env".to_string(), Json::Obj(env));
        tools.insert(name.clone(), Json::Obj(to));
    }
    obj.insert("tools".to_string(), Json::Obj(tools));

    let mut rules = BTreeMap::new();
    for (name, r) in &ws.rules {
        let mut ro = BTreeMap::new();
        ro.insert("phony".to_string(), Json::Bool(r.phony));
        ro.insert(
            "inputs".to_string(),
            Json::Arr(r.inputs.iter().map(|p| Json::Str(norm_path(p))).collect()),
        );
        ro.insert(
            "outputs".to_string(),
            Json::Arr(r.outputs.iter().map(|p| Json::Str(norm_path(p))).collect()),
        );
        ro.insert(
            "deps".to_string(),
            Json::Arr(r.deps.iter().cloned().map(Json::Str).collect()),
        );
        ro.insert(
            "tool".to_string(),
            match &r.tool {
                Some(t) => Json::Str(t.clone()),
                None => Json::Null,
            },
        );
        ro.insert(
            "argv".to_string(),
            Json::Arr(r.argv.iter().cloned().map(Json::Str).collect()),
        );

        let mut env = BTreeMap::new();
        for (k, v) in &r.env {
            env.insert(k.clone(), Json::Str(v.clone()));
        }
        ro.insert("env".to_string(), Json::Obj(env));

        ro.insert(
            "tags".to_string(),
            Json::Arr(r.tags.iter().cloned().map(Json::Str).collect()),
        );

        let mut meta = BTreeMap::new();
        for (k, v) in &r.meta {
            meta.insert(k.clone(), Json::Str(v.clone()));
        }
        ro.insert("meta".to_string(), Json::Obj(meta));

        rules.insert(name.clone(), Json::Obj(ro));
    }
    obj.insert("rules".to_string(), Json::Obj(rules));

    Json::Obj(obj)
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsp_quotes() {
        let a = vec!["-Ifoo".to_string(), "path with space".to_string(), "x\"y".to_string()];
        let s = gen_rsp(&a);
        assert!(s.contains("-Ifoo"));
        assert!(s.contains("\"path with space\""));
        assert!(s.contains("\\\""));
    }

    #[test]
    fn dot_emits_graph() {
        let mut ws = Workspace {
            root: PathBuf::from("."),
            steelfile: None,
            vars: BTreeMap::new(),
            tools: BTreeMap::new(),
            rules: BTreeMap::new(),
        };
        ws.rules.insert(
            "a".to_string(),
            Rule {
                name: "a".to_string(),
                phony: false,
                inputs: vec![],
                outputs: vec![PathBuf::from("a.o")],
                deps: vec![],
                tool: None,
                argv: vec![],
                env: BTreeMap::new(),
                tags: BTreeSet::new(),
                meta: BTreeMap::new(),
            },
        );
        ws.rules.insert(
            "b".to_string(),
            Rule {
                name: "b".to_string(),
                phony: true,
                inputs: vec![],
                outputs: vec![],
                deps: vec!["a".to_string()],
                tool: None,
                argv: vec![],
                env: BTreeMap::new(),
                tags: BTreeSet::new(),
                meta: BTreeMap::new(),
            },
        );

        let dot = gen_dot(&ws);
        assert!(dot.contains("digraph steel"));
        assert!(dot.contains("\"a\" -> \"b\""));
    }

    #[test]
    fn json_stringify_works() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), Json::Num(1));
        let s = json_stringify(&Json::Obj(m));
        assert_eq!(s, "{\"a\":1}");
    }
}
