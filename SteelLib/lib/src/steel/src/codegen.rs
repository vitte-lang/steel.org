//! Steel code generation.
//!
//! Responsibilities (convention):
//! - Emit the resolved configuration artifact: `steel.log` (stable, deterministic).
//! - Optionally export a build graph (DOT/text) for introspection.
//! - Provide stable escaping/formatting utilities shared by emitters.
//!
//! This module is designed to be usable even if your resolver/IR evolves:
//! - The `ResolvedConfig` model below is intentionally explicit and serialization-friendly.
//! - If you already have an IR, adapt by implementing `From<&YourIR> for ResolvedConfig`
//!   (or by building `ResolvedConfig` directly).
//!
//! Determinism requirements:
//! - Sort keys and collections before emission.
//! - Normalize paths (optional; do it in resolver ideally).
//! - Keep stable formatting/quoting across platforms.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// ------------------------------------------------------------
/// Public API
/// ------------------------------------------------------------

/// What to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    /// The canonical resolved artifact.
    Mff,
    /// A graph representation (typically of bakes/ports/wires).
    GraphDot,
    /// A human-readable graph dump.
    GraphText,
}

/// Configuration for emission.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    pub kind: EmitKind,
    /// If `None`, caller decides default path (`./steel.log` for Mff).
    pub out_path: Option<PathBuf>,
    /// Pretty formatting where applicable (text output already pretty).
    pub pretty: bool,
    /// Include fingerprints/hashes in the output.
    pub include_fingerprints: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            kind: EmitKind::Mff,
            out_path: None,
            pretty: true,
            include_fingerprints: true,
        }
    }
}

/// Emission result metadata.
#[derive(Debug, Clone)]
pub struct EmitResult {
    pub kind: EmitKind,
    pub path: PathBuf,
    pub bytes_written: u64,
}

/// Emission errors.
#[derive(Debug)]
pub enum CodegenError {
    Io(io::Error),
    Invalid(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::Io(e) => write!(f, "io error: {e}"),
            CodegenError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<io::Error> for CodegenError {
    fn from(e: io::Error) -> Self {
        CodegenError::Io(e)
    }
}

/// Emit to file.
pub fn emit_to_path(cfg: &ResolvedConfig, opts: &EmitOptions) -> Result<EmitResult, CodegenError> {
    let (path, content) = match opts.kind {
        EmitKind::Mff => {
            let p = opts
                .out_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("steel.log"));
            (p, emit_mff_to_string(cfg, opts)?)
        }
        EmitKind::GraphDot => {
            let p = opts.out_path.clone().unwrap_or_else(|| PathBuf::from("dag.dot"));
            (p, emit_graph_dot_to_string(cfg, opts)?)
        }
        EmitKind::GraphText => {
            let p = opts.out_path.clone().unwrap_or_else(|| PathBuf::from("dag.txt"));
            (p, emit_graph_text_to_string(cfg, opts)?)
        }
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(&path, content.as_bytes())?;
    let bytes_written = content.as_bytes().len() as u64;

    Ok(EmitResult {
        kind: opts.kind,
        path,
        bytes_written,
    })
}

/// Emit to string (main artifact).
pub fn emit_mff_to_string(cfg: &ResolvedConfig, opts: &EmitOptions) -> Result<String, CodegenError> {
    let mut w = MffWriter::new();
    w.line("mff 1");
    w.blank();

    // host
    w.block_start("host");
    w.kv_str("os", &cfg.host.os);
    w.kv_str("arch", &cfg.host.arch);
    if let Some(v) = &cfg.host.vendor {
        w.kv_str("vendor", v);
    }
    if let Some(v) = &cfg.host.abi {
        w.kv_str("abi", v);
    }
    w.block_end();

    w.blank();

    // selection
    w.kv_str_inline("profile", &cfg.selection.profile);
    if let Some(t) = &cfg.selection.target {
        w.kv_str_inline("target", t);
    }
    if let Some(p) = &cfg.selection.plan {
        w.kv_str_inline("plan", p);
    }
    w.blank();

    // paths
    w.block_start("paths");
    w.kv_str("root", &cfg.paths.root);
    w.kv_str("dist", &cfg.paths.dist);
    if let Some(v) = &cfg.paths.cache_dir {
        w.kv_str("cache", v);
    }
    if let Some(v) = &cfg.paths.store_dir {
        w.kv_str("store", v);
    }
    w.block_end();

    w.blank();

    // stores
    if !cfg.stores.is_empty() {
        w.block_start("stores");
        for (name, s) in cfg.stores.iter() {
            w.block_start_named("store", name);
            w.kv_str("path", &s.path);
            w.kv_str("mode", s.mode.as_str());
            if opts.include_fingerprints {
                if let Some(fp) = &s.fingerprint {
                    w.kv_str("fp", fp);
                }
            }
            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // capsules
    if !cfg.capsules.is_empty() {
        w.block_start("capsules");
        for (name, c) in cfg.capsules.iter() {
            w.block_start_named("capsule", name);

            if let Some(p) = &c.env {
                w.block_start("env");
                match p {
                    EnvPolicy::Allow(xs) => {
                        w.kv_list_str("allow", xs);
                    }
                    EnvPolicy::Deny(xs) => {
                        w.kv_list_str("deny", xs);
                    }
                }
                w.block_end();
            }

            if let Some(p) = &c.fs {
                w.block_start("fs");
                if !p.allow_read.is_empty() {
                    w.kv_list_str("allow_read", &p.allow_read);
                }
                if !p.allow_write.is_empty() {
                    w.kv_list_str("allow_write", &p.allow_write);
                }
                if !p.allow_write_exact.is_empty() {
                    w.kv_list_str("allow_write_exact", &p.allow_write_exact);
                }
                if !p.deny.is_empty() {
                    w.kv_list_str("deny", &p.deny);
                }
                w.block_end();
            }

            if let Some(p) = &c.net {
                w.kv_str("net", p.as_str());
            }

            if let Some(p) = &c.time {
                w.block_start("time");
                w.kv_bool("stable", p.stable);
                w.block_end();
            }

            if opts.include_fingerprints {
                if let Some(fp) = &c.fingerprint {
                    w.kv_str("fp", fp);
                }
            }

            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // variables (resolved)
    if !cfg.vars.is_empty() {
        w.block_start("vars");
        for (k, v) in cfg.vars.iter() {
            w.block_start_named("var", k);
            w.kv_str("type", v.ty.as_str());
            w.kv_value("value", &v.value);
            if opts.include_fingerprints {
                if let Some(fp) = &v.fingerprint {
                    w.kv_str("fp", fp);
                }
            }
            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // tools
    if !cfg.tools.is_empty() {
        w.block_start("tools");
        for (name, t) in cfg.tools.iter() {
            w.block_start_named("tool", name);
            w.kv_str("exec", &t.exec);
            if let Some(v) = &t.expect_version {
                w.kv_str("expect_version", v);
            }
            w.kv_bool("sandbox", t.sandbox);
            if let Some(c) = &t.capsule {
                w.kv_str("capsule", c);
            }
            if opts.include_fingerprints {
                if let Some(fp) = &t.fingerprint {
                    w.kv_str("fp", fp);
                }
            }
            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // bakes
    if !cfg.bakes.is_empty() {
        w.block_start("bakes");
        for (name, b) in cfg.bakes.iter() {
            w.block_start_named("bake", name);

            if !b.inputs.is_empty() {
                w.block_start("in");
                for (pname, p) in b.inputs.iter() {
                    w.block_start_named("port", pname);
                    w.kv_str("type", p.ty.as_str());
                    w.block_end();
                }
                w.block_end();
            }

            if !b.outputs.is_empty() {
                w.block_start("out");
                for (pname, p) in b.outputs.iter() {
                    w.block_start_named("port", pname);
                    w.kv_str("type", p.ty.as_str());
                    if let Some(path) = &p.output_path {
                        w.kv_str("at", path);
                    }
                    w.block_end();
                }
                w.block_end();
            }

            if !b.makes.is_empty() {
                w.block_start("make");
                for m in &b.makes {
                    w.block_start_named("node", &m.name);
                    w.kv_str("kind", m.kind.as_str());
                    w.kv_str("spec", &m.spec);
                    if opts.include_fingerprints {
                        if let Some(fp) = &m.fingerprint {
                            w.kv_str("fp", fp);
                        }
                    }
                    w.block_end();
                }
                w.block_end();
            }

            if let Some(r) = &b.run {
                w.block_start("run");
                w.kv_str("tool", &r.tool);
                if !r.takes.is_empty() {
                    w.block_start("takes");
                    for t in &r.takes {
                        w.block_start("bind");
                        w.kv_str("port", &t.port);
                        w.kv_str("as", &t.flag);
                        w.block_end();
                    }
                    w.block_end();
                }
                if !r.emits.is_empty() {
                    w.block_start("emits");
                    for e in &r.emits {
                        w.block_start("bind");
                        w.kv_str("port", &e.port);
                        w.kv_str("as", &e.flag);
                        w.block_end();
                    }
                    w.block_end();
                }
                if !r.sets.is_empty() {
                    w.block_start("set");
                    for s in &r.sets {
                        w.block_start("arg");
                        w.kv_str("flag", &s.flag);
                        w.kv_value("value", &s.value);
                        w.block_end();
                    }
                    w.block_end();
                }
                if opts.include_fingerprints {
                    if let Some(fp) = &r.fingerprint {
                        w.kv_str("fp", fp);
                    }
                }
                w.block_end();
            }

            if let Some(c) = &b.cache {
                w.kv_str("cache", c.as_str());
            }

            if opts.include_fingerprints {
                if let Some(fp) = &b.fingerprint {
                    w.kv_str("fp", fp);
                }
            }

            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // wiring
    if !cfg.wires.is_empty() {
        w.block_start("wires");
        for w0 in &cfg.wires {
            w.line(&format!(
                "wire {} -> {}",
                format_ref(&w0.from),
                format_ref(&w0.to)
            ));
        }
        w.block_end();
        w.blank();
    }

    // exports
    if !cfg.exports.is_empty() {
        w.block_start("exports");
        for e in &cfg.exports {
            w.line(&format!("export {}", format_ref(e)));
        }
        w.block_end();
        w.blank();
    }

    // plans
    if !cfg.plans.is_empty() {
        w.block_start("plans");
        for (name, p) in cfg.plans.iter() {
            w.block_start_named("plan", name);
            for step in &p.steps {
                match step {
                    PlanStep::RunExports => w.line("run exports"),
                    PlanStep::RunRef(r) => w.line(&format!("run {}", format_ref(r))),
                }
            }
            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    // switches (optional, for CLI mapping)
    if !cfg.switches.is_empty() {
        w.block_start("switch");
        for s in &cfg.switches {
            w.block_start("flag");
            w.kv_str("name", &s.flag);
            match &s.action {
                SwitchAction::SetVar { key, value } => {
                    w.kv_str("action", "set");
                    w.kv_str("key", key);
                    w.kv_value("value", value);
                }
                SwitchAction::SetPlan { plan } => {
                    w.kv_str("action", "set_plan");
                    w.kv_str("plan", plan);
                }
                SwitchAction::RunExports => {
                    w.kv_str("action", "run");
                    w.kv_str("target", "exports");
                }
                SwitchAction::RunRef { target } => {
                    w.kv_str("action", "run");
                    w.kv_str("target", &format_ref(target));
                }
            }
            w.block_end();
        }
        w.block_end();
        w.blank();
    }

    Ok(w.finish())
}

/// Emit DOT graph string.
pub fn emit_graph_dot_to_string(cfg: &ResolvedConfig, _opts: &EmitOptions) -> Result<String, CodegenError> {
    let mut g = String::new();
    g.push_str("digraph steel {\n");
    g.push_str("  rankdir=LR;\n");
    g.push_str("  node [shape=box];\n");

    // bake nodes
    for (bake, b) in cfg.bakes.iter() {
        let label = if b.outputs.is_empty() {
            bake.clone()
        } else {
            format!("{}\\n(out: {})", bake, b.outputs.keys().cloned().collect::<Vec<_>>().join(", "))
        };
        g.push_str(&format!("  \"bake:{bake}\" [label=\"{label}\"];\n"));
    }

    // var nodes (optional)
    for (k, v) in cfg.vars.iter() {
        let label = format!("var:{}\\n{}", k, v.ty.as_str());
        g.push_str(&format!("  \"var:{k}\" [shape=note,label=\"{label}\"];\n"));
    }

    // wires
    for w in &cfg.wires {
        let from = dot_ref_id(&w.from);
        let to = dot_ref_id(&w.to);
        g.push_str(&format!("  \"{from}\" -> \"{to}\";\n"));
    }

    // map refs to bake nodes where possible (port refs)
    for w in &cfg.wires {
        if let Ref::Port { bake, .. } = &w.from {
            g.push_str(&format!("  \"bake:{bake}\" -> \"{}\" [style=dotted];\n", dot_ref_id(&w.from)));
        }
        if let Ref::Port { bake, .. } = &w.to {
            g.push_str(&format!("  \"{}\" -> \"bake:{bake}\" [style=dotted];\n", dot_ref_id(&w.to)));
        }
    }

    g.push_str("}\n");
    Ok(g)
}

/// Emit a human-readable graph dump.
pub fn emit_graph_text_to_string(cfg: &ResolvedConfig, _opts: &EmitOptions) -> Result<String, CodegenError> {
    let mut out = String::new();
    out.push_str("bakes:\n");
    for (name, b) in cfg.bakes.iter() {
        out.push_str(&format!("  - {name}\n"));
        if !b.inputs.is_empty() {
            out.push_str("    in:\n");
            for (p, pd) in b.inputs.iter() {
                out.push_str(&format!("      - {p}: {}\n", pd.ty.as_str()));
            }
        }
        if !b.outputs.is_empty() {
            out.push_str("    out:\n");
            for (p, pd) in b.outputs.iter() {
                if let Some(at) = &pd.output_path {
                    out.push_str(&format!("      - {p}: {} at {at}\n", pd.ty.as_str()));
                } else {
                    out.push_str(&format!("      - {p}: {}\n", pd.ty.as_str()));
                }
            }
        }
        if let Some(run) = &b.run {
            out.push_str(&format!("    run: tool={}\n", run.tool));
        }
        if let Some(c) = &b.cache {
            out.push_str(&format!("    cache: {}\n", c.as_str()));
        }
    }

    out.push_str("\nwires:\n");
    for w in &cfg.wires {
        out.push_str(&format!("  - {} -> {}\n", format_ref(&w.from), format_ref(&w.to)));
    }

    out.push_str("\nexports:\n");
    for e in &cfg.exports {
        out.push_str(&format!("  - {}\n", format_ref(e)));
    }

    out.push_str("\nplans:\n");
    for (name, p) in cfg.plans.iter() {
        out.push_str(&format!("  - {name}:\n"));
        for s in &p.steps {
            match s {
                PlanStep::RunExports => out.push_str("      run exports\n"),
                PlanStep::RunRef(r) => out.push_str(&format!("      run {}\n", format_ref(r))),
            }
        }
    }

    Ok(out)
}

/// ------------------------------------------------------------
/// Resolved model (artifact-friendly)
/// ------------------------------------------------------------

/// Resolved configuration root.
/// All maps are BTree* for determinism.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedConfig {
    pub host: HostInfo,
    pub selection: Selection,
    pub paths: Paths,

    pub stores: BTreeMap<String, StoreResolved>,
    pub capsules: BTreeMap<String, CapsuleResolved>,
    pub vars: BTreeMap<String, VarResolved>,
    pub tools: BTreeMap<String, ToolResolved>,

    pub bakes: BTreeMap<String, BakeResolved>,
    pub wires: Vec<WireResolved>,
    pub exports: Vec<Ref>,
    pub plans: BTreeMap<String, PlanResolved>,
    pub switches: Vec<SwitchResolved>,

    /// Optional: a precomputed global fingerprint for the entire config.
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub vendor: Option<String>,
    pub abi: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub profile: String,
    pub target: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paths {
    pub root: String,
    pub dist: String,
    pub cache_dir: Option<String>,
    pub store_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreResolved {
    pub path: String,
    pub mode: StoreMode,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreMode {
    Content,
    Mtime,
    Off,
}

impl StoreMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StoreMode::Content => "content",
            StoreMode::Mtime => "mtime",
            StoreMode::Off => "off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleResolved {
    pub env: Option<EnvPolicy>,
    pub fs: Option<FsPolicyResolved>,
    pub net: Option<NetPolicy>,
    pub time: Option<TimePolicyResolved>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvPolicy {
    Allow(Vec<String>),
    Deny(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FsPolicyResolved {
    pub allow_read: Vec<String>,
    pub allow_write: Vec<String>,
    pub allow_write_exact: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetPolicy {
    Allow,
    Deny,
}

impl NetPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetPolicy::Allow => "allow",
            NetPolicy::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePolicyResolved {
    pub stable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarResolved {
    pub ty: VarType,
    pub value: ScalarValue,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarType {
    Text,
    Int,
    Bool,
    Bytes,
    Artifact(String),
}

impl VarType {
    pub fn as_str(&self) -> &str {
        match self {
            VarType::Text => "text",
            VarType::Int => "int",
            VarType::Bool => "bool",
            VarType::Bytes => "bytes",
            VarType::Artifact(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResolved {
    pub exec: String,
    pub expect_version: Option<String>,
    pub sandbox: bool,
    pub capsule: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeResolved {
    pub inputs: BTreeMap<String, PortResolved>,
    pub outputs: BTreeMap<String, PortResolved>,
    pub makes: Vec<MakeResolved>,
    pub run: Option<RunResolved>,
    pub cache: Option<CacheMode>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortResolved {
    pub ty: VarType,
    pub output_path: Option<String>, // when `output <port> at "<path>"`
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeResolved {
    pub name: String,
    pub kind: MakeKind,
    pub spec: String,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MakeKind {
    Glob,
    File,
    Text,
    Value,
}

impl MakeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MakeKind::Glob => "glob",
            MakeKind::File => "file",
            MakeKind::Text => "text",
            MakeKind::Value => "value",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResolved {
    pub tool: String,
    pub takes: Vec<PortBind>,
    pub emits: Vec<PortBind>,
    pub sets: Vec<ArgSet>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortBind {
    pub port: String,
    pub flag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgSet {
    pub flag: String,
    pub value: ScalarValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMode {
    Content,
    Mtime,
    Off,
}

impl CacheMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheMode::Content => "content",
            CacheMode::Mtime => "mtime",
            CacheMode::Off => "off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireResolved {
    pub from: Ref,
    pub to: Ref,
}

/// Reference: var or bake.port (same as language ref model).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ref {
    Var(String),
    Port { bake: String, port: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanResolved {
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStep {
    RunExports,
    RunRef(Ref),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchResolved {
    pub flag: String,
    pub action: SwitchAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchAction {
    SetVar { key: String, value: ScalarValue },
    SetPlan { plan: String },
    RunExports,
    RunRef { target: Ref },
}

/// Scalar values used for emission (a reduced subset of runtime values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarValue {
    String(String),
    Int(i64),
    Bool(bool),
    Ident(String),
    List(Vec<ScalarValue>),
}

impl ScalarValue {
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            ScalarValue::Ident(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// ------------------------------------------------------------
/// MFF writer (deterministic formatting)
/// ------------------------------------------------------------

struct MffWriter {
    buf: String,
    indent: usize,
}

impl MffWriter {
    fn new() -> Self {
        Self {
            buf: String::new(),
            indent: 0,
        }
    }

    fn finish(self) -> String {
        self.buf
    }

    fn blank(&mut self) {
        if !self.buf.ends_with('\n') {
            self.buf.push('\n');
        }
        self.buf.push('\n');
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.buf.push_str("  ");
        }
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    fn block_start(&mut self, name: &str) {
        self.line(name);
        self.indent += 1;
    }

    fn block_start_named(&mut self, name: &str, arg: &str) {
        let argq = quote(arg);
        self.line(&format!("{name} {argq}"));
        self.indent += 1;
    }

    fn block_end(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
        self.line(".end");
    }

    fn kv_str(&mut self, key: &str, value: &str) {
        self.line(&format!("{key} {}", quote(value)));
    }

    fn kv_str_inline(&mut self, key: &str, value: &str) {
        self.line(&format!("{key} {}", quote(value)));
    }

    fn kv_bool(&mut self, key: &str, v: bool) {
        self.line(&format!("{key} {}", if v { "true" } else { "false" }));
    }

    fn kv_list_str(&mut self, key: &str, xs: &[String]) {
        // stable order: caller should already provide stable; we still sort defensively.
        let mut v: Vec<&str> = xs.iter().map(|s| s.as_str()).collect();
        v.sort();
        let mut s = String::new();
        s.push('[');
        for (i, it) in v.iter().enumerate() {
            if i != 0 {
                s.push_str(", ");
            }
            s.push_str(&quote(it));
        }
        s.push(']');
        self.line(&format!("{key} {s}"));
    }

    fn kv_value(&mut self, key: &str, v: &ScalarValue) {
        self.line(&format!("{key} {}", format_scalar_value(v)));
    }
}

fn quote(s: &str) -> String {
    // Minimal escaping: \, ", \n, \r, \t.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn format_scalar_value(v: &ScalarValue) -> String {
    match v {
        ScalarValue::String(s) => quote(s),
        ScalarValue::Int(i) => i.to_string(),
        ScalarValue::Bool(b) => b.to_string(),
        ScalarValue::Ident(s) => s.clone(),
        ScalarValue::List(xs) => {
            let mut out = String::new();
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i != 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_scalar_value(x));
            }
            out.push(']');
            out
        }
    }
}

/// ------------------------------------------------------------
/// Helpers: ref formatting / dot ids
/// ------------------------------------------------------------

fn format_ref(r: &Ref) -> String {
    match r {
        Ref::Var(v) => v.clone(),
        Ref::Port { bake, port } => format!("{bake}.{port}"),
    }
}

fn dot_ref_id(r: &Ref) -> String {
    match r {
        Ref::Var(v) => format!("var:{v}"),
        Ref::Port { bake, port } => format!("port:{bake}.{port}"),
    }
}

/// ------------------------------------------------------------
/// Deterministic fingerprinting (optional utility)
/// ------------------------------------------------------------

/// Deterministic FNV-1a 64-bit fingerprint (portable, dependency-free).
/// Use it to fingerprint inputs/args/toolchains if you don't want extra crates.
///
/// For stronger hashes, integrate `sha2`/`blake3` behind features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv1a64(u64);

impl Fnv1a64 {
    pub fn new() -> Self {
        // offset basis
        Self(0xcbf29ce484222325)
    }

    pub fn update_bytes(&mut self, bytes: &[u8]) {
        const PRIME: u64 = 0x00000100000001B3;
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub fn update_str(&mut self, s: &str) {
        self.update_bytes(s.as_bytes());
    }

    pub fn update_u64(&mut self, x: u64) {
        self.update_bytes(&x.to_le_bytes());
    }

    pub fn finish_hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Build a canonical fingerprint of the resolved config.
/// If you already compute per-node fingerprints in the resolver, you can ignore this.
pub fn fingerprint_config(cfg: &ResolvedConfig) -> String {
    let mut h = Fnv1a64::new();

    // host
    h.update_str("host.os=");
    h.update_str(&cfg.host.os);
    h.update_str(";host.arch=");
    h.update_str(&cfg.host.arch);

    // selection
    h.update_str(";sel.profile=");
    h.update_str(&cfg.selection.profile);
    if let Some(t) = &cfg.selection.target {
        h.update_str(";sel.target=");
        h.update_str(t);
    }
    if let Some(p) = &cfg.selection.plan {
        h.update_str(";sel.plan=");
        h.update_str(p);
    }

    // paths
    h.update_str(";paths.root=");
    h.update_str(&cfg.paths.root);
    h.update_str(";paths.dist=");
    h.update_str(&cfg.paths.dist);

    // stores
    for (k, s) in &cfg.stores {
        h.update_str(";store.");
        h.update_str(k);
        h.update_str(".path=");
        h.update_str(&s.path);
        h.update_str(".mode=");
        h.update_str(s.mode.as_str());
    }

    // tools
    for (k, t) in &cfg.tools {
        h.update_str(";tool.");
        h.update_str(k);
        h.update_str(".exec=");
        h.update_str(&t.exec);
        if let Some(v) = &t.expect_version {
            h.update_str(".ver=");
            h.update_str(v);
        }
        h.update_str(".sandbox=");
        h.update_str(if t.sandbox { "1" } else { "0" });
        if let Some(c) = &t.capsule {
            h.update_str(".capsule=");
            h.update_str(c);
        }
    }

    // bakes (names + ports + run tool + bindings)
    for (bk, b) in &cfg.bakes {
        h.update_str(";bake.");
        h.update_str(bk);

        for (p, pd) in &b.inputs {
            h.update_str(".in.");
            h.update_str(p);
            h.update_str("=");
            h.update_str(pd.ty.as_str());
        }

        for (p, pd) in &b.outputs {
            h.update_str(".out.");
            h.update_str(p);
            h.update_str("=");
            h.update_str(pd.ty.as_str());
            if let Some(at) = &pd.output_path {
                h.update_str("@");
                h.update_str(at);
            }
        }

        if let Some(run) = &b.run {
            h.update_str(".run.tool=");
            h.update_str(&run.tool);
            for t in &run.takes {
                h.update_str(".takes.");
                h.update_str(&t.port);
                h.update_str("->");
                h.update_str(&t.flag);
            }
            for e in &run.emits {
                h.update_str(".emits.");
                h.update_str(&e.port);
                h.update_str("->");
                h.update_str(&e.flag);
            }
        }

        if let Some(c) = &b.cache {
            h.update_str(".cache=");
            h.update_str(c.as_str());
        }
    }

    // wires
    for w in &cfg.wires {
        h.update_str(";wire:");
        h.update_str(&format_ref(&w.from));
        h.update_str("->");
        h.update_str(&format_ref(&w.to));
    }

    // exports
    for e in &cfg.exports {
        h.update_str(";export:");
        h.update_str(&format_ref(e));
    }

    // plans
    for (name, p) in &cfg.plans {
        h.update_str(";plan.");
        h.update_str(name);
        for s in &p.steps {
            match s {
                PlanStep::RunExports => h.update_str(":run=exports"),
                PlanStep::RunRef(r) => {
                    h.update_str(":run=");
                    h.update_str(&format_ref(r));
                }
            }
        }
    }

    h.finish_hex()
}

/// ------------------------------------------------------------
/// Optional conveniences for path normalization / safety
/// ------------------------------------------------------------

/// Normalize a path-ish string to a forward-slash form (portable stable strings).
/// Prefer doing this earlier (resolver) but provided here for convenience.
pub fn normalize_path_string<S: AsRef<str>>(s: S) -> String {
    let raw = s.as_ref();
    raw.replace('\\', "/")
}

/// Ensure output path is inside workspace root (guard rail).
pub fn ensure_within_root(root: &Path, out: &Path) -> Result<(), CodegenError> {
    let root = root
        .canonicalize()
        .map_err(|e| CodegenError::Io(e))?;
    let out = if out.is_absolute() {
        out.to_path_buf()
    } else {
        root.join(out)
    };
    let out = out
        .canonicalize()
        .or_else(|_| Ok(out)) // allow non-existing targets
        .map_err(|e| CodegenError::Io(e))?;

    if !out.starts_with(&root) {
        return Err(CodegenError::Invalid(format!(
            "output path escapes root: out={:?} root={:?}",
            out, root
        )));
    }
    Ok(())
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_escapes() {
        assert_eq!(quote("a"), "\"a\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn format_ref_port() {
        let r = Ref::Port {
            bake: "app".into(),
            port: "exe".into(),
        };
        assert_eq!(format_ref(&r), "app.exe");
    }

    #[test]
    fn fingerprint_is_stableish() {
        let mut cfg = ResolvedConfig::default();
        cfg.host.os = "linux".into();
        cfg.host.arch = "x86_64".into();
        cfg.selection.profile = "debug".into();
        cfg.paths.root = "/repo".into();
        cfg.paths.dist = "dist".into();

        let fp1 = fingerprint_config(&cfg);
        let fp2 = fingerprint_config(&cfg);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 16);
    }

    #[test]
    fn emit_mff_smoke() {
        let mut cfg = ResolvedConfig::default();
        cfg.host.os = "linux".into();
        cfg.host.arch = "x86_64".into();
        cfg.selection.profile = "debug".into();
        cfg.paths.root = "/repo".into();
        cfg.paths.dist = "dist".into();

        cfg.tools.insert(
            "vittec".into(),
            ToolResolved {
                exec: "vittec".into(),
                expect_version: None,
                sandbox: true,
                capsule: None,
                fingerprint: Some("deadbeef".into()),
            },
        );

        cfg.bakes.insert(
            "app".into(),
            BakeResolved {
                inputs: BTreeMap::new(),
                outputs: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "exe".into(),
                        PortResolved {
                            ty: VarType::Artifact("bin.exe".into()),
                            output_path: Some("./app".into()),
                        },
                    );
                    m
                },
                makes: vec![],
                run: None,
                cache: Some(CacheMode::Content),
                fingerprint: None,
            },
        );

        cfg.exports.push(Ref::Port {
            bake: "app".into(),
            port: "exe".into(),
        });

        let s = emit_mff_to_string(&cfg, &EmitOptions::default()).unwrap();
        assert!(s.contains("mff 1"));
        assert!(s.contains("tools"));
        assert!(s.contains("bakes"));
        assert!(s.contains("export \"app.exe\"") || s.contains("export app.exe")); // depending on writer usage
    }
}