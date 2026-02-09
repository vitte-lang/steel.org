

// /Users/vincent/Documents/Github/steel/src/builder.rs
//! builder — orchestration layer between Steel (config) and the execution layer (std-only)
//!
//! Role in the pipeline
//! - `build steel` resolves workspace configuration and emits `steelconfig.mff` + `steel.log`
//! - the execution layer consumes `.mff` + rule metadata to perform the actual build
//!
//! This module sits at the boundary:
//! - validates + normalizes resolved config
//! - packages resolved targets into a deterministic `.mff` text
//! - builds a deterministic dependency order for targets (toposort)
//! - provides a stable in-memory `BuildPlan` representation a runner (or tests) can consume
//!
//! Constraints
//! - std-only
//! - deterministic ordering
//! - no shell evaluation
//!
//! Notes
//! - This module does NOT implement a full execution engine.
//!   The execution step is delegated to a runner (or to a caller-provided backend).
//! - The types here are intentionally conservative, so they can be serialized easily.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{ConfigPolicy, ValidationReport};
use crate::debug::{self, DebugConfig, LogLevel};
use crate::def_target_file::{self, TargetDef};
use crate::dependancies::{self, DiGraph};
use crate::expand::{self, ExpandOptions, Vars};

/// High-level build stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    /// Only validate inputs/config.
    Validate,
    /// Validate + produce `.mff` text.
    EmitConfig,
    /// Validate + emit + compute dependency order and plan.
    Plan,
}

/// Builder policy knobs.
#[derive(Debug, Clone)]
pub struct BuilderPolicy {
    pub config_policy: ConfigPolicy,
    pub expand: ExpandOptions,

    /// If true, fail when a target dependency points to a missing target.
    pub strict_missing_deps: bool,

    /// If true, validate that all target paths are under project root (lexical).
    pub require_target_root_under_project: bool,

    /// When true, expand values in `TargetDef.options` using cfg.vars.
    pub expand_target_options: bool,

    /// When true, expand output paths and source paths too (rare, but sometimes useful).
    pub expand_target_paths: bool,
}

impl Default for BuilderPolicy {
    fn default() -> Self {
        Self {
            config_policy: ConfigPolicy::default(),
            expand: ExpandOptions::default(),
            strict_missing_deps: true,
            require_target_root_under_project: true,
            expand_target_options: true,
            expand_target_paths: false,
        }
    }
}

/// A resolved configuration for a build session.
///
/// This is intentionally narrow: it is what the builder needs to emit `.mff` and build a plan.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub resolved: crate::build_muf::ResolvedConfig,
    pub targets: Vec<TargetDef>,

    /// `.mff` output path (if emission is requested).
    pub mcfg_out: Option<PathBuf>,
}

/// Outcome of a builder run.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub report: ValidationReport,

    /// Deterministic `.mff` content (when stage >= EmitConfig).
    pub mcfg_text: Option<String>,

    /// Deterministic target build order (when stage == Plan).
    pub target_order: Vec<String>,

    /// Optional in-memory plan (when stage == Plan).
    pub plan: Option<BuildPlan>,
}

impl BuildOutput {
    pub fn ok(&self) -> bool {
        !self.report.has_errors()
    }
}

/// A high-level build plan.
///
/// A runner can consume this plan directly, or ignore it and just read `.mff`.
#[derive(Debug, Clone, Default)]
pub struct BuildPlan {
    /// Targets in topological order.
    pub targets: Vec<PlannedTarget>,

    /// A set of artifacts (outputs) declared by targets.
    pub artifacts: BTreeSet<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PlannedTarget {
    pub id: String,
    pub deps: Vec<String>,

    /// Declared sources.
    pub sources: Vec<PathBuf>,

    /// Declared outputs.
    pub outputs: Vec<PathBuf>,

    /// Option key-values (post-expansion if enabled).
    pub options: BTreeMap<String, String>,
}

/// Builder entry point.
#[derive(Debug, Clone)]
pub struct Builder {
    pub policy: BuilderPolicy,
    pub debug: DebugConfig,
}

impl Default for Builder {
    fn default() -> Self {
        let mut debug_cfg = DebugConfig::default();
        // default to info; caller can override
        debug_cfg.level = LogLevel::Info;
        Self {
            policy: BuilderPolicy::default(),
            debug: debug_cfg,
        }
    }
}

impl Builder {
    pub fn new(policy: BuilderPolicy, debug: DebugConfig) -> Self {
        Self { policy, debug }
    }

    /// Run a builder stage.
    pub fn run(&self, stage: BuildStage, mut sess: SessionConfig) -> BuildOutput {
        let _sec = debug::section(&self.debug, LogLevel::Debug, format!("builder::{stage:?}"));

        // 1) validate config
        let mut report = crate::config::validate_resolved_config(&sess.resolved, &self.policy.config_policy);

        // 2) validate targets
        report.extend(self.validate_targets(&sess));

        // 3) optional expand
        if !report.has_errors() {
            self.apply_expansion(&mut sess, &mut report);
        }

        // stage: Validate
        if stage == BuildStage::Validate {
            return BuildOutput {
                report,
                mcfg_text: None,
                target_order: Vec::new(),
                plan: None,
            };
        }

        // 4) emit mcfg text
        let mcfg_text = if report.has_errors() {
            None
        } else {
            Some(build_mcfg_text(&sess.resolved, &sess.targets))
        };

        // write file if requested
        if let (Some(path), Some(text)) = (&sess.mcfg_out, &mcfg_text) {
            if let Err(e) = write_text_atomic(path, text) {
                report.push(crate::config::Diagnostic::err(
                    "MCFG_WRITE_FAIL",
                    format!("failed to write .mff: {e}"),
                ).with_path(path.clone()));
            }
        }

        if stage == BuildStage::EmitConfig {
            return BuildOutput {
                report,
                mcfg_text,
                target_order: Vec::new(),
                plan: None,
            };
        }

        // 5) build dependency order + plan
        let (order, plan, dep_report) = if report.has_errors() {
            (Vec::new(), None, ValidationReport::default())
        } else {
            self.plan(&sess)
        };
        report.extend(dep_report);

        BuildOutput {
            report,
            mcfg_text,
            target_order: order,
            plan,
        }
    }

    fn validate_targets(&self, sess: &SessionConfig) -> ValidationReport {
        let mut r = ValidationReport::default();

        // unique ids
        let mut ids = BTreeSet::new();
        for t in &sess.targets {
            if let Err(e) = def_target_file::validate_target_def(t) {
                r.push(crate::config::Diagnostic::err("TGT_INVALID", e.to_string()));
            }
            if !ids.insert(t.id.clone()) {
                r.push(crate::config::Diagnostic::err(
                    "TGT_DUP_ID",
                    format!("duplicate target id: {}", t.id),
                ));
            }

            if self.policy.require_target_root_under_project {
                if !crate::directory::is_under(&sess.resolved.project_root, &t.root) {
                    r.push(crate::config::Diagnostic::err(
                        "TGT_ROOT_OUTSIDE",
                        format!("target root outside project root: {}", t.id),
                    ));
                }
            }
        }

        // deps reference check
        if self.policy.strict_missing_deps {
            let set: BTreeSet<String> = sess.targets.iter().map(|t| t.id.clone()).collect();
            for t in &sess.targets {
                for d in &t.deps {
                    if !set.contains(d) {
                        r.push(crate::config::Diagnostic::err(
                            "TGT_DEP_MISSING",
                            format!("target {} depends on missing target {}", t.id, d),
                        ));
                    }
                }
            }
        }

        r
    }

    fn apply_expansion(&self, sess: &mut SessionConfig, report: &mut ValidationReport) {
        let _sec = debug::section(&self.debug, LogLevel::Trace, "expand".to_string());

        let vars: Vars = sess.resolved.vars.clone();

        // configure base_dir for path functions
        let mut ex = self.policy.expand.clone();
        ex.base_dir = Some(sess.resolved.project_root.clone());

        for t in &mut sess.targets {
            if self.policy.expand_target_options {
                let mut new = BTreeMap::new();
                for (k, v) in &t.options {
                    match expand::expand(v, &vars, &ex) {
                        Ok(s) => {
                            new.insert(k.clone(), s);
                        }
                        Err(e) => {
                            report.push(crate::config::Diagnostic::err(
                                "EXPAND_OPT",
                                format!("{}: {}", t.id, e),
                            ));
                            new.insert(k.clone(), v.clone());
                        }
                    }
                }
                t.options = new;
            }

            if self.policy.expand_target_paths {
                // sources
                let mut src = BTreeSet::new();
                for p in &t.sources {
                    let s = p.to_string_lossy().to_string();
                    match expand::expand(&s, &vars, &ex) {
                        Ok(ss) => src.insert(PathBuf::from(ss)),
                        Err(e) => {
                            report.push(crate::config::Diagnostic::err(
                                "EXPAND_PATH",
                                format!("{}: {}", t.id, e),
                            ));
                            src.insert(p.clone());
                            false
                        }
                    };
                }
                t.sources = src;

                // outputs
                let mut outs = BTreeSet::new();
                for p in &t.outputs {
                    let s = p.to_string_lossy().to_string();
                    match expand::expand(&s, &vars, &ex) {
                        Ok(ss) => outs.insert(PathBuf::from(ss)),
                        Err(e) => {
                            report.push(crate::config::Diagnostic::err(
                                "EXPAND_PATH",
                                format!("{}: {}", t.id, e),
                            ));
                            outs.insert(p.clone());
                            false
                        }
                    };
                }
                t.outputs = outs;
            }
        }
    }

    fn plan(&self, sess: &SessionConfig) -> (Vec<String>, Option<BuildPlan>, ValidationReport) {
        let _sec = debug::section(&self.debug, LogLevel::Debug, "plan".to_string());
        let mut r = ValidationReport::default();

        // build graph
        let mut g = DiGraph::new();
        for t in &sess.targets {
            g.add_node(t.id.clone());
        }
        for t in &sess.targets {
            for d in &t.deps {
                // edge d -> t (dep must be built before target)
                g.add_edge(d.clone(), t.id.clone());
            }
        }

        // validate graph
        let declared: BTreeSet<String> = sess.targets.iter().map(|t| t.id.clone()).collect();
        let rep = dependancies::validate_graph(&g, Some(&declared));
        if rep.has_errors() {
            // convert to config::ValidationReport
            for d in rep.diagnostics {
                let diag = match d.severity {
                    dependancies::Severity::Info => crate::config::Diagnostic::info(d.code, d.message),
                    dependancies::Severity::Warning => crate::config::Diagnostic::warn(d.code, d.message),
                    dependancies::Severity::Error => crate::config::Diagnostic::err(d.code, d.message),
                };
                r.push(diag);
            }
            return (Vec::new(), None, r);
        }

        // topo sort
        let order = match dependancies::topo_sort(&g) {
            Ok(v) => v,
            Err(_) => {
                let msg = match dependancies::find_cycle(&g) {
                    Some(cycle) => format!("dependency cycle detected: {}", cycle.join(" -> ")),
                    None => "dependency cycle detected".to_string(),
                };
                r.push(crate::config::Diagnostic::err("DEP_CYCLE", msg));
                return (Vec::new(), None, r);
            }
        };

        // build plan
        let mut by_id: BTreeMap<String, &TargetDef> = BTreeMap::new();
        for t in &sess.targets {
            by_id.insert(t.id.clone(), t);
        }

        let mut plan = BuildPlan::default();

        for id in &order {
            if let Some(t) = by_id.get(id) {
                let deps = t.deps.iter().cloned().collect::<Vec<_>>();
                let sources = t.sources.iter().cloned().collect::<Vec<_>>();
                let outputs = t.outputs.iter().cloned().collect::<Vec<_>>();

                for o in &outputs {
                    plan.artifacts.insert(o.clone());
                }

                plan.targets.push(PlannedTarget {
                    id: t.id.clone(),
                    deps,
                    sources,
                    outputs,
                    options: t.options.clone(),
                });
            } else {
                r.push(crate::config::Diagnostic::err(
                    "TGT_MISSING_INTERNAL",
                    format!("target not found during planning: {id}"),
                ));
            }
        }

        (order, Some(plan), r)
    }
}

/// Build `.mff` text deterministically.
///
/// This format is intentionally simple and line-oriented.
/// It is not the steelconf language.
pub fn build_mcfg_text(cfg: &crate::build_muf::ResolvedConfig, targets: &[TargetDef]) -> String {
    let mut out = String::new();

    out.push_str("# steelconfig.mff\n");
    out.push_str("# generated by build steel\n\n");

    out.push_str(&format!("schema {}\n", cfg.schema_version));
    out.push_str(&format!("profile \"{}\"\n", escape(&cfg.profile)));
    out.push_str(&format!("target \"{}\"\n", escape(&cfg.target)));
    out.push_str(&format!("root \"{}\"\n", escape(&cfg.project_root.to_string_lossy())));
    out.push_str(&format!("steelfile \"{}\"\n", escape(&cfg.steelfile_path.to_string_lossy())));

    out.push_str("\npaths\n");
    out.push_str(&format!("  build \"{}\"\n", escape(&cfg.paths.build_dir.to_string_lossy())));
    out.push_str(&format!("  dist \"{}\"\n", escape(&cfg.paths.dist_dir.to_string_lossy())));
    out.push_str(&format!("  cache \"{}\"\n", escape(&cfg.paths.cache_dir.to_string_lossy())));
    out.push_str(".end\n\n");

    out.push_str("toolchain\n");
    if let Some(cc) = &cfg.toolchain.cc {
        out.push_str(&format!("  cc \"{}\"\n", escape(cc)));
    }
    if let Some(cxx) = &cfg.toolchain.cxx {
        out.push_str(&format!("  cxx \"{}\"\n", escape(cxx)));
    }
    if let Some(ar) = &cfg.toolchain.ar {
        out.push_str(&format!("  ar \"{}\"\n", escape(ar)));
    }
    if let Some(ld) = &cfg.toolchain.ld {
        out.push_str(&format!("  ld \"{}\"\n", escape(ld)));
    }
    if let Some(rustc) = &cfg.toolchain.rustc {
        out.push_str(&format!("  rustc \"{}\"\n", escape(rustc)));
    }
    if let Some(python) = &cfg.toolchain.python {
        out.push_str(&format!("  python \"{}\"\n", escape(python)));
    }
    if let Some(ocaml) = &cfg.toolchain.ocaml {
        out.push_str(&format!("  ocaml \"{}\"\n", escape(ocaml)));
    }
    if let Some(ghc) = &cfg.toolchain.ghc {
        out.push_str(&format!("  ghc \"{}\"\n", escape(ghc)));
    }

    if !cfg.toolchain.versions.is_empty() {
        out.push_str("  versions\n");
        for (k, v) in &cfg.toolchain.versions {
            out.push_str(&format!("    set \"{}\" \"{}\"\n", escape(k), escape(v)));
        }
        out.push_str("  .end\n");
    }

    out.push_str(".end\n\n");

    if !cfg.vars.is_empty() {
        out.push_str("vars\n");
        for (k, v) in &cfg.vars {
            out.push_str(&format!("  set \"{}\" \"{}\"\n", escape(k), escape(v)));
        }
        out.push_str(".end\n\n");
    }

    if !cfg.fingerprint.trim().is_empty() {
        out.push_str(&format!("fingerprint \"{}\"\n\n", escape(&cfg.fingerprint)));
    }

    // targets
    out.push_str("targets\n");
    let mut list: Vec<&TargetDef> = targets.iter().collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));

    for t in list {
        // indent target block by 2 spaces for readability
        let block = def_target_file::format_target_block(t);
        for line in block.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(".end\n");

    out
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

/// Atomic-ish write: write to temp then rename.
fn write_text_atomic(path: &Path, text: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let mut tmp = path.to_path_buf();
    tmp.set_extension("tmp");

    fs::write(&tmp, text.as_bytes())?;

    // best-effort rename
    match fs::rename(&tmp, path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // fallback: copy + remove
            fs::write(path, text.as_bytes())?;
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ------------------------------ optional error types ------------------------------

#[derive(Debug, Clone)]
pub struct BuilderError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BuilderError {}

// ------------------------------ tests ------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_muf;
    use crate::def_target_file::{OutputKind, TargetKind};

    #[test]
    fn emits_mcfg_contains_targets() {
        let mut cfg = build_muf::generate_default_mcfg("/tmp/steel");
        cfg.profile = "debug".into();
        cfg.target = "x86_64-unknown-linux-gnu".into();

        let mut t = TargetDef::new("app", TargetKind::Program, OutputKind::Exe, "pkg/app");
        t.deps.insert("core".into());
        t.sources.insert(PathBuf::from("src/main.vit"));
        t.outputs.insert(PathBuf::from("dist/app"));
        t.options.insert("opt.level".into(), "2".into());

        let mut core = TargetDef::new("core", TargetKind::Library, OutputKind::StaticLib, "pkg/core");
        core.outputs.insert(PathBuf::from("dist/libcore.a"));

        let text = build_mcfg_text(&cfg, &[t, core]);
        assert!(text.contains("targets"));
        assert!(text.contains("target \"app\""));
        assert!(text.contains("target \"core\""));
    }

    #[test]
    fn planning_toposort_respects_deps() {
        let mut root = std::env::temp_dir();
        let pid = std::process::id();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        root.push(format!("steel_builder_plan_{pid}_{ts}"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("steelconf"), "workspace ...\n").unwrap();

        let mut cfg = build_muf::generate_default_mcfg(&root);
        cfg.profile = "debug".into();
        cfg.target = "x86_64-unknown-linux-gnu".into();

        let mut app = TargetDef::new(
            "app",
            TargetKind::Program,
            OutputKind::Exe,
            cfg.project_root.join("pkg/app"),
        );
        app.deps.insert("core".into());

        let core = TargetDef::new(
            "core",
            TargetKind::Library,
            OutputKind::StaticLib,
            cfg.project_root.join("pkg/core"),
        );

        let b = Builder::default();
        let sess = SessionConfig { resolved: cfg, targets: vec![app, core], mcfg_out: None };
        let out = b.run(BuildStage::Plan, sess);

        assert!(!out.report.has_errors(), "report: {0}", out.report);
        let ia = out.target_order.iter().position(|x| x == "app").unwrap();
        let ic = out.target_order.iter().position(|x| x == "core").unwrap();
        assert!(ic < ia);
    }
}
