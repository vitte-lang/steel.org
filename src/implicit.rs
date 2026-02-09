// src/implicit.rs
//
// Steel — implicit (implicit rules, implicit variables, implicit defaults)
//
// Purpose:
// - Provide the implicit behavior layer that makes Steel ergonomic without
//   turning configuration into "magic":
//   - default variables (workspace root, build dir, profile/target defaults)
//   - implicit tools (cc, cxx, ar) when not defined explicitly
//   - implicit rules (all, clean) synthesized if missing
//   - implicit dependency inference (outputs -> inputs mapping by conventions)
//   - implicit file classification (sources, headers, objects, libs)
//
// Design goals:
// - Deterministic and explainable: always generate a trace of what was injected.
// - Never override explicit user definitions unless configured.
// - Keep logic self-contained so other subsystems (planner/executor) can rely on it.
//
// Notes:
// - Replace the local Workspace/Tool/Rule models with your actual ones.
// - This module uses stable BTreeMap/BTreeSet ordering for reproducible behavior.
// - "max": includes a lot of conventions; you can toggle via ImplicitConfig.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/* ============================== models (adapt to your crate) ============================== */

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub steelfile: Option<PathBuf>,
    pub vars: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, Profile>,
    pub targets: BTreeMap<String, Target>,
    pub tools: BTreeMap<String, Tool>,
    pub rules: BTreeMap<String, Rule>,
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

/* ============================== config + trace ============================== */

#[derive(Debug, Clone)]
pub struct ImplicitConfig {
    /// If true, allow implicit entries to override explicit (not recommended).
    pub allow_override: bool,

    /// Create "all" rule if missing.
    pub synth_all: bool,

    /// Create "clean" rule if missing.
    pub synth_clean: bool,

    /// Inject default tools if missing.
    pub synth_tools: bool,

    /// Inject default vars if missing.
    pub synth_vars: bool,

    /// Build directory var name/value.
    pub build_dir_key: String,
    pub build_dir_default: String,

    /// Default profile / target if not specified.
    pub default_profile: Option<String>,
    pub default_target: Option<String>,
}

impl Default for ImplicitConfig {
    fn default() -> Self {
        Self {
            allow_override: false,
            synth_all: true,
            synth_clean: true,
            synth_tools: true,
            synth_vars: true,
            build_dir_key: "build.dir".to_string(),
            build_dir_default: "build".to_string(),
            default_profile: None,
            default_target: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImplicitTrace {
    pub injected_vars: Vec<(String, String, String)>,   // (k, v, reason)
    pub injected_tools: Vec<(String, String)>,          // (name, reason)
    pub injected_rules: Vec<(String, String)>,          // (name, reason)
    pub notes: Vec<String>,
}

impl ImplicitTrace {
    fn note(&mut self, s: impl Into<String>) {
        self.notes.push(s.into());
    }
}

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplicitError {
    Invalid(String),
}

impl fmt::Display for ImplicitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImplicitError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for ImplicitError {}

/* ============================== API ============================== */

pub fn apply_implicit(ws: &mut Workspace, cfg: &ImplicitConfig) -> Result<ImplicitTrace, ImplicitError> {
    let mut tr = ImplicitTrace::default();

    if cfg.synth_vars {
        inject_default_vars(ws, cfg, &mut tr)?;
    }
    if cfg.synth_tools {
        inject_default_tools(ws, cfg, &mut tr)?;
    }
    if cfg.synth_all {
        synthesize_all_rule(ws, cfg, &mut tr)?;
    }
    if cfg.synth_clean {
        synthesize_clean_rule(ws, cfg, &mut tr)?;
    }

    // Optional: derive tags/classification for all rules
    classify_rules(ws, &mut tr);

    Ok(tr)
}

/* ============================== vars ============================== */

fn inject_default_vars(ws: &mut Workspace, cfg: &ImplicitConfig, tr: &mut ImplicitTrace) -> Result<(), ImplicitError> {
    // workspace.root
    insert_var(ws, cfg, tr, "workspace.root", ws.root.display().to_string(), "workspace root")?;

    // steelfile path (if any)
    if let Some(mf) = &ws.steelfile {
        insert_var(ws, cfg, tr, "workspace.steelfile", mf.display().to_string(), "steelfile path")?;
    }

    // build dir
    if !ws.vars.contains_key(&cfg.build_dir_key) || cfg.allow_override {
        let v = cfg.build_dir_default.clone();
        insert_var(ws, cfg, tr, &cfg.build_dir_key, v, "default build directory")?;
    }

    // default profile/target hints
    if let Some(p) = &cfg.default_profile {
        insert_var(ws, cfg, tr, "workspace.default_profile", p.clone(), "default profile hint")?;
    }
    if let Some(t) = &cfg.default_target {
        insert_var(ws, cfg, tr, "workspace.default_target", t.clone(), "default target hint")?;
    }

    Ok(())
}

fn insert_var(
    ws: &mut Workspace,
    cfg: &ImplicitConfig,
    tr: &mut ImplicitTrace,
    k: &str,
    v: String,
    reason: &str,
) -> Result<(), ImplicitError> {
    if ws.vars.contains_key(k) && !cfg.allow_override {
        return Ok(());
    }
    ws.vars.insert(k.to_string(), v.clone());
    tr.injected_vars.push((k.to_string(), v, reason.to_string()));
    Ok(())
}

/* ============================== tools ============================== */

fn inject_default_tools(ws: &mut Workspace, cfg: &ImplicitConfig, tr: &mut ImplicitTrace) -> Result<(), ImplicitError> {
    // Conventional tool names: cc, cxx, ar, ld
    // Programs: environment override first, then defaults.
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "c++".to_string());
    let ar = std::env::var("AR").unwrap_or_else(|_| "ar".to_string());
    let ld = std::env::var("LD").ok();

    insert_tool(ws, cfg, tr, Tool {
        name: "cc".to_string(),
        program: cc,
        args: vec![],
        env: BTreeMap::new(),
    }, "implicit tool (C compiler)")?;

    insert_tool(ws, cfg, tr, Tool {
        name: "cxx".to_string(),
        program: cxx,
        args: vec![],
        env: BTreeMap::new(),
    }, "implicit tool (C++ compiler)")?;

    insert_tool(ws, cfg, tr, Tool {
        name: "ar".to_string(),
        program: ar,
        args: vec![],
        env: BTreeMap::new(),
    }, "implicit tool (archiver)")?;

    if let Some(ldp) = ld {
        insert_tool(ws, cfg, tr, Tool {
            name: "ld".to_string(),
            program: ldp,
            args: vec![],
            env: BTreeMap::new(),
        }, "implicit tool (linker via LD)")?;
    }

    Ok(())
}

fn insert_tool(ws: &mut Workspace, cfg: &ImplicitConfig, tr: &mut ImplicitTrace, tool: Tool, reason: &str) -> Result<(), ImplicitError> {
    if ws.tools.contains_key(&tool.name) && !cfg.allow_override {
        return Ok(());
    }
    let name = tool.name.clone();
    ws.tools.insert(name.clone(), tool);
    tr.injected_tools.push((name, reason.to_string()));
    Ok(())
}

/* ============================== rules: synth all/clean ============================== */

fn synthesize_all_rule(ws: &mut Workspace, cfg: &ImplicitConfig, tr: &mut ImplicitTrace) -> Result<(), ImplicitError> {
    if ws.rules.contains_key("all") && !cfg.allow_override {
        return Ok(());
    }

    // "all" depends on all non-phony rules that produce outputs
    let mut deps = Vec::<String>::new();
    for (name, r) in &ws.rules {
        if name == "all" || name == "clean" {
            continue;
        }
        if !r.outputs.is_empty() {
            deps.push(name.clone());
        }
    }
    deps.sort();

    let rule = Rule {
        name: "all".to_string(),
        phony: true,
        inputs: vec![],
        outputs: vec![],
        deps,
        tool: None,
        argv: vec![],
        env: BTreeMap::new(),
        tags: {
            let mut s = BTreeSet::new();
            s.insert("implicit".to_string());
            s.insert("phony".to_string());
            s
        },
        meta: {
            let mut m = BTreeMap::new();
            m.insert("reason".to_string(), "synthesized default 'all'".to_string());
            m
        },
    };

    ws.rules.insert("all".to_string(), rule);
    tr.injected_rules.push(("all".to_string(), "synthesized default target".to_string()));
    Ok(())
}

fn synthesize_clean_rule(ws: &mut Workspace, cfg: &ImplicitConfig, tr: &mut ImplicitTrace) -> Result<(), ImplicitError> {
    if ws.rules.contains_key("clean") && !cfg.allow_override {
        return Ok(());
    }

    let build_dir = ws
        .vars
        .get(&cfg.build_dir_key)
        .cloned()
        .unwrap_or_else(|| cfg.build_dir_default.clone());

    // clean uses a "shell" tool if available; otherwise a no-op placeholder.
    let tool = if ws.tools.contains_key("sh") {
        Some("sh".to_string())
    } else {
        None
    };

    let argv = if tool.is_some() {
        // POSIX-ish; on Windows you might switch to powershell/cmd
        vec!["-lc".to_string(), format!("rm -rf \"{}\"", build_dir)]
    } else {
        vec![]
    };

    let rule = Rule {
        name: "clean".to_string(),
        phony: true,
        inputs: vec![],
        outputs: vec![],
        deps: vec![],
        tool,
        argv,
        env: BTreeMap::new(),
        tags: {
            let mut s = BTreeSet::new();
            s.insert("implicit".to_string());
            s.insert("phony".to_string());
            s
        },
        meta: {
            let mut m = BTreeMap::new();
            m.insert("reason".to_string(), "synthesized default 'clean'".to_string());
            m.insert("clean.dir".to_string(), build_dir);
            m
        },
    };

    ws.rules.insert("clean".to_string(), rule);
    tr.injected_rules.push(("clean".to_string(), "synthesized clean rule".to_string()));
    Ok(())
}

/* ============================== classification / inference ============================== */

fn classify_rules(ws: &mut Workspace, tr: &mut ImplicitTrace) {
    for (_name, r) in ws.rules.iter_mut() {
        // tag implicit rules
        if r.meta.get("reason").is_some() && !r.tags.contains("implicit") {
            r.tags.insert("implicit".to_string());
        }

        // classify by outputs extension
        for o in &r.outputs {
            if let Some(ext) = o.extension().and_then(|e| e.to_str()) {
                match ext.to_ascii_lowercase().as_str() {
                    "o" | "obj" => {
                        r.tags.insert("object".to_string());
                    }
                    "a" | "lib" => {
                        r.tags.insert("archive".to_string());
                    }
                    "so" | "dll" | "dylib" => {
                        r.tags.insert("shared".to_string());
                    }
                    "exe" => {
                        r.tags.insert("exe".to_string());
                    }
                    _ => {}
                }
            }
        }

        // infer "phony" tag
        if r.phony {
            r.tags.insert("phony".to_string());
        }
    }

    tr.note("classified rules by output extension + phony");
}

/* ============================== helpers ============================== */

pub fn is_source_file(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(ext) if ext == "c" || ext == "cc" || ext == "cpp" || ext == "cxx" => true,
        Some(ext) if ext == "s" || ext == "asm" => true,
        _ => false,
    }
}

pub fn default_object_path(build_dir: &str, src: &Path) -> PathBuf {
    // build/obj/<src_path>.o (flattened path separators to '_')
    let mut name = src.to_string_lossy().replace('\\', "/");
    name = name.replace('/', "_");
    name.push_str(".o");
    PathBuf::from(build_dir).join("obj").join(name)
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_defaults() {
        let mut ws = Workspace {
            root: PathBuf::from("."),
            steelfile: None,
            vars: BTreeMap::new(),
            profiles: BTreeMap::new(),
            targets: BTreeMap::new(),
            tools: BTreeMap::new(),
            rules: BTreeMap::new(),
        };

        let cfg = ImplicitConfig::default();
        let tr = apply_implicit(&mut ws, &cfg).unwrap();

        assert!(ws.vars.contains_key("workspace.root"));
        assert!(ws.vars.contains_key(&cfg.build_dir_key));
        assert!(ws.rules.contains_key("all"));
        assert!(ws.rules.contains_key("clean"));
        assert!(!tr.injected_vars.is_empty());
    }

    #[test]
    fn object_path_is_deterministic() {
        let p = default_object_path("build", Path::new("src/main.c"));
        assert!(p.to_string_lossy().contains("build"));
        assert!(p.to_string_lossy().contains("src_main.c.o"));
    }
}
