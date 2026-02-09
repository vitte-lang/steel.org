//! vmsify.rs
//!
//! “VMSify” — transformation d’artefacts Steel (manifest / config) en jobs VMS.
//!
//! Objectifs:
//! - API pure (pas de FS obligatoire), mais support FS si fourni
//! - Prend des structures “config-like” et produit un JobGraph (vmsjobs.rs)
//! - Résolution des chemins via VPath (vpath.rs) + racines (workspace/store/capsule)
//! - Détection d’erreurs structurées (pas de panics), avec diagnostics (warning.rs) optionnels
//! - Mode strict: erreurs sur incongruences, mode permissif: warnings
//!
//! Dépendances: std uniquement.
//!
//! Intégration attendue:
//! - config.rs / default.rs: lecture de .mff
//! - steel commands: “build steel” => vmsify(config) => runner.run2(graph, targets)
//!
//! NOTE: ce module définit des “IR” minimaux pour rester autonome.
//! Dans ton repo, tu peux remplacer ces IR par tes structs réelles de config.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::vmsjobs::{
    ExecSpec, InlineSpec, Job, JobGraph, JobId, JobStep, LogLevel, RunContext,
};
use crate::vpath::VPath;

/// Politique de conversion.
#[derive(Debug, Clone)]
pub struct VmsifyOptions {
    /// Ajoute automatiquement des jobs d’initialisation (mkdir build dir, etc.).
    pub add_bootstrap_jobs: bool,
    /// Ajoute des tags informatifs.
    pub add_tags: bool,
    /// Injecte des variables d’environnement globales.
    pub global_env: BTreeMap<String, String>,
    /// Timeout default pour les commandes.
    pub default_timeout: Option<Duration>,
    /// Mode strict: certaines incohérences deviennent des erreurs.
    pub strict: bool,
}

impl Default for VmsifyOptions {
    fn default() -> Self {
        Self {
            add_bootstrap_jobs: true,
            add_tags: true,
            global_env: BTreeMap::new(),
            default_timeout: None,
            strict: true,
        }
    }
}

/// Erreur vmsify (conversion).
#[derive(Debug)]
pub enum VmsifyError {
    Invalid(String),
    UnknownTarget(String),
    DuplicateJob(String),
    Path(String),
    Cycle(String),
}

impl fmt::Display for VmsifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmsifyError::Invalid(s) => write!(f, "invalid: {}", s),
            VmsifyError::UnknownTarget(s) => write!(f, "unknown target: {}", s),
            VmsifyError::DuplicateJob(s) => write!(f, "duplicate job: {}", s),
            VmsifyError::Path(s) => write!(f, "path error: {}", s),
            VmsifyError::Cycle(s) => write!(f, "cycle: {}", s),
        }
    }
}

impl std::error::Error for VmsifyError {}

/// “IR” minimal d’entrée: configuration globale.
#[derive(Debug, Clone)]
pub struct SteelconfIr {
    pub workspace_root: PathBuf,
    pub build_dir: PathBuf,
    pub profiles: BTreeMap<String, ProfileIR>,
    pub selected_profile: Option<String>,
    pub tools: BTreeMap<String, ToolIR>,
    pub targets: BTreeMap<String, TargetIR>,
}

impl SteelconfIr {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let root = workspace_root.into();
        Self {
            build_dir: root.join("build"),
            workspace_root: root,
            profiles: BTreeMap::new(),
            selected_profile: None,
            tools: BTreeMap::new(),
            targets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileIR {
    pub name: String,
    pub env: BTreeMap<String, String>,
    pub flags: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ToolIR {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<VPath>,
}

#[derive(Debug, Clone)]
pub struct TargetIR {
    pub name: String,
    pub deps: Vec<String>,
    pub steps: Vec<TargetStepIR>,
    pub cwd: Option<VPath>,
    pub env: BTreeMap<String, String>,
    pub allow_failure: bool,
    pub tags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub enum TargetStepIR {
    /// Exécute un tool déclaré dans config.tools
    ToolCall {
        tool: String,
        extra_args: Vec<String>,
        label: Option<String>,
        timeout: Option<Duration>,
        capture: bool,
    },
    /// Commande brute.
    Command {
        program: String,
        args: Vec<String>,
        cwd: Option<VPath>,
        env: BTreeMap<String, String>,
        label: Option<String>,
        timeout: Option<Duration>,
        capture: bool,
    },
    /// Inline hook.
    Inline {
        label: Option<String>,
        // Dans la vraie intégration, ce serait un ident de callback.
        // Ici on met juste un message.
        message: String,
    },
    /// Log.
    Log {
        level: LogLevel,
        message: String,
    },
}

/// Résultat vmsify: graph + mapping utile.
#[derive(Debug, Clone)]
pub struct Vmsified {
    pub graph: JobGraph,
    /// Mapping target->job ids
    pub targets: BTreeMap<String, JobId>,
    /// Jobs bootstrap (si ajoutés)
    pub bootstrap_jobs: Vec<JobId>,
}

impl Vmsified {
    pub fn empty() -> Self {
        Self {
            graph: JobGraph::new(),
            targets: BTreeMap::new(),
            bootstrap_jobs: Vec::new(),
        }
    }
}

/* =====================
 * Public entrypoints
 * ===================== */

/// Convertit toute la config en JobGraph.
/// - Retourne mapping target->job.
pub fn vmsify_config(cfg: &SteelconfIr, ctx: &RunContext, opts: &VmsifyOptions) -> Result<Vmsified, VmsifyError> {
    let mut out = Vmsified::empty();

    // bootstrap
    if opts.add_bootstrap_jobs {
        let bs = make_bootstrap_jobs(cfg, ctx, opts)?;
        for j in &bs {
            out.graph.insert(j.clone()).map_err(|e| VmsifyError::Invalid(e.to_string()))?;
            out.bootstrap_jobs.push(j.id.clone());
        }
    }

    // profile env
    let profile_env = selected_profile_env(cfg)?;

    // targets Steel
    for (name, t) in &cfg.targets {
        let jid = JobId::new(format!("steel:{}", name));
        if out.graph.jobs.contains_key(&jid) {
            return Err(VmsifyError::DuplicateJob(jid.0));
        }
        let job = target_to_job(cfg, opts, &profile_env, name, t, &out.bootstrap_jobs)?;
        out.graph.insert(job).map_err(|e| VmsifyError::Invalid(e.to_string()))?;
        out.targets.insert(name.clone(), jid);
    }

    Ok(out)
}

/* =====================
 * Target conversion
 * ===================== */

fn target_to_job(
    cfg: &SteelconfIr,
    opts: &VmsifyOptions,
    profile_env: &BTreeMap<String, String>,
    name: &str,
    t: &TargetIR,
    bootstrap_jobs: &[JobId],
) -> Result<Job, VmsifyError> {
    let jid = JobId::new(format!("steel:{}", name));
    let mut job = Job::new(jid.clone(), format!("Target: {}", t.name));
    job.allow_failure = t.allow_failure;

    // deps
    for b in bootstrap_jobs {
        job.deps.insert(b.clone());
    }
    for d in &t.deps {
        // On suppose que les deps sont des targets Steel par défaut.
        // Si tu veux d'autres namespaces, fais un resolver plus riche.
        job.deps.insert(JobId::new(format!("steel:{}", d)));
    }

    // cwd
    if let Some(cwd) = &t.cwd {
        let p = resolve_vpath_to_host(cfg, cwd)?;
        job.cwd = Some(p);
    }

    // env: global + profile + target
    for (k, v) in &opts.global_env {
        job.env.insert(k.clone(), v.clone());
    }
    for (k, v) in profile_env {
        job.env.insert(k.clone(), v.clone());
    }
    for (k, v) in &t.env {
        job.env.insert(k.clone(), v.clone());
    }

    // tags
    if opts.add_tags {
        job.tags.insert("steel".to_string());
        for tag in &t.tags {
            job.tags.insert(tag.clone());
        }
    }

    // steps
    if t.steps.is_empty() && opts.strict {
        return Err(VmsifyError::Invalid(format!(
            "target '{}' has no steps",
            name
        )));
    }

    job.steps.push(JobStep::Log {
        level: LogLevel::Info,
        message: format!("build target '{}'", name),
    });

    for s in &t.steps {
        job.steps.push(step_to_jobstep(cfg, opts, profile_env, t, s)?);
    }

    Ok(job)
}

fn step_to_jobstep(
    cfg: &SteelconfIr,
    opts: &VmsifyOptions,
    profile_env: &BTreeMap<String, String>,
    t: &TargetIR,
    s: &TargetStepIR,
) -> Result<JobStep, VmsifyError> {
    match s {
        TargetStepIR::Log { level, message } => Ok(JobStep::Log {
            level: *level,
            message: message.clone(),
        }),

        TargetStepIR::Inline { label, message } => {
            let msg = message.clone();
            let mut inl = InlineSpec::new(move || {
                // Place-holder: tu peux hooker ici des callbacks.
                // Exemple: pré-calcul de hash, scan deps, etc.
                if msg.is_empty() {
                    Ok(())
                } else {
                    Ok(())
                }
            });
            inl.label = label.clone();
            Ok(JobStep::Inline(inl))
        }

        TargetStepIR::ToolCall {
            tool,
            extra_args,
            label,
            timeout,
            capture,
        } => {
            let tool_def = cfg.tools.get(tool).ok_or_else(|| {
                VmsifyError::UnknownTarget(format!("unknown tool '{}'", tool))
            })?;

            let mut ex = ExecSpec::new(tool_def.program.clone())
                .args(tool_def.args.iter().cloned())
                .args(extra_args.iter().cloned());

            // cwd: step tool cwd > target cwd
            if let Some(cwd) = &tool_def.cwd {
                ex.cwd = Some(resolve_vpath_to_host(cfg, cwd)?);
            } else if let Some(cwd) = &t.cwd {
                ex.cwd = Some(resolve_vpath_to_host(cfg, cwd)?);
            }

            // env: global/profile/target déjà au niveau job; ici on ajoute env tool
            for (k, v) in &tool_def.env {
                ex.env.insert(k.clone(), v.clone());
            }
            for (k, v) in profile_env {
                let _ = k;
                let _ = v;
                // (déjà dans job.env, pas nécessaire)
            }

            // capture
            if *capture {
                ex = ex.capture_stdout().capture_stderr();
            }

            // timeout
            ex.timeout = timeout.or(opts.default_timeout);

            ex.label = label.clone().or_else(|| Some(format!("tool: {}", tool_def.name)));
            Ok(JobStep::Command(ex))
        }

        TargetStepIR::Command {
            program,
            args,
            cwd,
            env,
            label,
            timeout,
            capture,
        } => {
            let mut ex = ExecSpec::new(program.clone()).args(args.iter().cloned());

            // cwd: step > target
            if let Some(cwd) = cwd {
                ex.cwd = Some(resolve_vpath_to_host(cfg, cwd)?);
            } else if let Some(cwd) = &t.cwd {
                ex.cwd = Some(resolve_vpath_to_host(cfg, cwd)?);
            }

            // env step
            for (k, v) in env {
                ex.env.insert(k.clone(), v.clone());
            }

            // capture
            if *capture {
                ex = ex.capture_stdout().capture_stderr();
            }

            // timeout
            ex.timeout = timeout.or(opts.default_timeout);

            ex.label = label.clone().or_else(|| Some(format!("cmd: {}", program)));
            Ok(JobStep::Command(ex))
        }
    }
}

/* =====================
 * Bootstrap jobs
 * ===================== */

fn make_bootstrap_jobs(
    cfg: &SteelconfIr,
    ctx: &RunContext,
    opts: &VmsifyOptions,
) -> Result<Vec<Job>, VmsifyError> {
    let mut out = Vec::new();

    // Job: ensure build dirs exist.
    let jid = JobId::new("bootstrap:dirs".to_string());
    let mut j = Job::new(jid.clone(), "Bootstrap: ensure directories".to_string());

    if opts.add_tags {
        j.tags.insert("bootstrap".to_string());
    }

    // Inline: create dirs (in-process, cross-platform).
    let build = cfg.build_dir.clone();
    let jobs_dir = ctx.jobs_dir.clone();
    let inline = InlineSpec::new(move || {
        std::fs::create_dir_all(&build).map_err(|e| format!("mkdir {}: {}", build.display(), e))?;
        std::fs::create_dir_all(&jobs_dir)
            .map_err(|e| format!("mkdir {}: {}", jobs_dir.display(), e))?;
        Ok(())
    })
    .label("mkdir build/jobs".to_string());

    j.steps.push(JobStep::Inline(inline));

    out.push(j);

    Ok(out)
}

/* =====================
 * Path resolution
 * ===================== */

fn resolve_vpath_to_host(cfg: &SteelconfIr, p: &VPath) -> Result<PathBuf, VmsifyError> {
    // Règle: root = workspace_root ; cwd = workspace_root
    // Dans un modèle “capsule/store”, root pourrait être store_root.
    let root = cfg.workspace_root.as_path();
    let cwd = cfg.workspace_root.as_path();
    Ok(p.to_host_path(root, cwd))
}

/* =====================
 * Profile selection
 * ===================== */

fn selected_profile_env(cfg: &SteelconfIr) -> Result<BTreeMap<String, String>, VmsifyError> {
    let mut env = BTreeMap::new();
    let sel = match &cfg.selected_profile {
        Some(s) => s.clone(),
        None => return Ok(env),
    };
    let p = cfg.profiles.get(&sel).ok_or_else(|| {
        VmsifyError::Invalid(format!("selected_profile '{}' not found", sel))
    })?;
    for (k, v) in &p.env {
        env.insert(k.clone(), v.clone());
    }
    Ok(env)
}

/* =====================
 * Helpers “builders”
 * ===================== */

pub fn tool(name: impl Into<String>, program: impl Into<String>) -> ToolIR {
    ToolIR {
        name: name.into(),
        program: program.into(),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
    }
}

pub fn profile(name: impl Into<String>) -> ProfileIR {
    ProfileIR {
        name: name.into(),
        env: BTreeMap::new(),
        flags: BTreeMap::new(),
    }
}

pub fn target(name: impl Into<String>) -> TargetIR {
    TargetIR {
        name: name.into(),
        deps: Vec::new(),
        steps: Vec::new(),
        cwd: None,
        env: BTreeMap::new(),
        allow_failure: false,
        tags: BTreeSet::new(),
    }
}

/* =====================
 * Tests
 * ===================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmsify_basic_target() {
        let mut cfg = SteelconfIr::new(".");
        cfg.targets.insert(
            "steel".to_string(),
            TargetIR {
                name: "steel".to_string(),
                deps: vec![],
                steps: vec![TargetStepIR::Log {
                    level: LogLevel::Info,
                    message: "hello".to_string(),
                }],
                cwd: None,
                env: BTreeMap::new(),
                allow_failure: false,
                tags: BTreeSet::new(),
            },
        );

        let ctx = RunContext::new(".").max_parallel(1).dry_run(true);
        let opts = VmsifyOptions::default();
        let v = vmsify_config(&cfg, &ctx, &opts).unwrap();

        assert!(!v.graph.jobs.is_empty());
        assert!(v.targets.contains_key("steel"));
        let jid = v.targets.get("steel").unwrap();
        assert!(v.graph.jobs.contains_key(jid));
    }

    #[test]
    fn vmsify_and_run_dry() {
        let mut cfg = SteelconfIr::new(".");
        cfg.targets.insert(
            "a".to_string(),
            TargetIR {
                name: "a".to_string(),
                deps: vec![],
                steps: vec![TargetStepIR::Inline {
                    label: Some("noop".to_string()),
                    message: "".to_string(),
                }],
                cwd: None,
                env: BTreeMap::new(),
                allow_failure: false,
                tags: BTreeSet::new(),
            },
        );

        let ctx = RunContext::new(".").max_parallel(1).dry_run(true);
        let opts = VmsifyOptions::default();
        let v = vmsify_config(&cfg, &ctx, &opts).unwrap();

        let jid = JobId::new("steel:a".to_string());
        assert!(v.graph.jobs.contains_key(&jid));
    }

}
