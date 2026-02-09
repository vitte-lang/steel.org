// src/interface.rs
//
// Steel — interface (public-facing core interfaces + glue)
//
// Purpose:
// - Define stable traits + types used across commands and subsystems:
//   - Workspace loading interface
//   - Planner interface (dirty/plan)
//   - Executor interface (jobs)
//   - Remote interface (fetch/resolve)
//   - Output interface (logging/diagnostics)
//   - Host interface (fs/env/time/process) for testability
//
// This module is intended to be imported by many files, so it avoids heavy deps,
// keeps types compact, and pushes implementation to other modules.
//
// Design goals:
// - Replace ad-hoc coupling between modules with explicit contracts.
// - Make unit testing easy: substitute Host/Output/Remote/Loader.
//
// Notes:
// - "max": includes many extension points and default adapters.
// - Use this as a central place for your execution boundary too.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/* ============================== basic ids ============================== */

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u64);

impl fmt::Debug for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuleId(0x{:016x})", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub u64);

impl fmt::Debug for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JobId(0x{:016x})", self.0)
    }
}

/* ============================== Output interface ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

pub trait IOutput: Send + Sync {
    fn emit(&self, level: Level, target: &str, msg: &str, kv: &[(&str, &str)]);
    fn flush(&self) {}

    fn error(&self, target: &str, msg: &str) {
        self.emit(Level::Error, target, msg, &[]);
    }
    fn warn(&self, target: &str, msg: &str) {
        self.emit(Level::Warn, target, msg, &[]);
    }
    fn info(&self, target: &str, msg: &str) {
        self.emit(Level::Info, target, msg, &[]);
    }
    fn debug(&self, target: &str, msg: &str) {
        self.emit(Level::Debug, target, msg, &[]);
    }
    fn trace(&self, target: &str, msg: &str) {
        self.emit(Level::Trace, target, msg, &[]);
    }
}

/* ============================== Host interface ============================== */

/// Host provides IO + process primitives for testability and sandboxing.
/// You can implement:
/// - RealHost (std::fs, std::process)
/// - SandboxHost (capsule restrictions)
/// - MockHost (unit tests)
pub trait IHost: Send + Sync {
    // filesystem
    fn read(&self, path: &Path) -> Result<Vec<u8>, HostError>;
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), HostError>;
    fn exists(&self, path: &Path) -> bool;
    fn mtime(&self, path: &Path) -> Result<SystemTime, HostError>;
    fn mkdir_all(&self, path: &Path) -> Result<(), HostError>;

    // environment
    fn env_get(&self, key: &str) -> Option<String>;
    fn env_list(&self) -> Vec<(String, String)>;

    // time
    fn now(&self) -> SystemTime;

    // process
    fn run(&self, cmd: &ProcSpec) -> Result<ProcResult, HostError>;
}

#[derive(Debug, Clone)]
pub struct ProcSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
    pub capture: bool,
}

impl ProcSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            stdin: None,
            timeout: None,
            capture: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcResult {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    Io { op: &'static str, path: Option<PathBuf>, message: String },
    Timeout { message: String },
    Forbidden { message: String },
    Other { message: String },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Io { op, path, message } => {
                if let Some(p) = path {
                    write!(f, "{} {}: {}", op, p.display(), message)
                } else {
                    write!(f, "{}: {}", op, message)
                }
            }
            HostError::Timeout { message } => write!(f, "timeout: {message}"),
            HostError::Forbidden { message } => write!(f, "forbidden: {message}"),
            HostError::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for HostError {}

/* ============================== Workspace interface ============================== */

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
    pub id: RuleId,
    pub name: String,
    pub phony: bool,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
    pub deps: Vec<RuleId>,
    pub tool: Option<String>,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tags: BTreeSet<String>,
    pub meta: BTreeMap<String, String>,
}

pub trait IWorkspaceLoader: Send + Sync {
    fn load(&self, ctx: &LoadContext) -> Result<Workspace, LoadError>;
}

#[derive(Debug, Clone)]
pub struct LoadContext {
    pub cwd: PathBuf,
    pub root_hint: Option<PathBuf>,
    pub steelfile_hint: Option<PathBuf>,
    pub profile: Option<String>,
    pub target: Option<String>,
    pub env_prefix: String,
    pub allow_missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Io { path: PathBuf, op: &'static str, message: String },
    NotFound { what: String },
    Parse { path: PathBuf, message: String },
    Invalid { message: String },
    Conflict { key: String, message: String },
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

/* ============================== Planner interface ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleState {
    Clean,
    Dirty,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub order: Vec<RuleId>,
    pub states: BTreeMap<RuleId, RuleState>,
    pub reasons: BTreeMap<RuleId, DirtyReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyReason {
    Phony,
    NoCacheEntry,
    RuleDefinitionChanged,
    MissingOutput(PathBuf),
    InputMissing(PathBuf),
    InputNewerThanOutput { input: PathBuf, output: PathBuf },
    DependencyDirty(RuleId),
    FingerprintMismatch,
    CycleMember,
    Unknown,
}

pub trait IPlanner: Send + Sync {
    fn plan(&self, ws: &Workspace, ctx: &PlanContext) -> Result<Plan, PlanError>;
}

#[derive(Debug, Clone)]
pub struct PlanContext {
    pub cache_enabled: bool,
    pub cache_mode: CacheMode,
    pub explain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Mtime,
    Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    Invalid(String),
    Io(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Invalid(s) => write!(f, "invalid: {s}"),
            PlanError::Io(s) => write!(f, "io: {s}"),
        }
    }
}

impl std::error::Error for PlanError {}

/* ============================== Job executor interface ============================== */

#[derive(Debug, Clone)]
pub struct JobPlan {
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub rule_id: RuleId,
    pub name: String,
    pub deps: Vec<JobId>,
    pub proc: ProcSpec,
}

#[derive(Debug, Clone)]
pub struct JobsConfig {
    pub parallelism: usize,
    pub fail_fast: bool,
    pub keep_going: bool,
    pub capture_output: bool,
    pub print_cmd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct JobResult {
    pub id: JobId,
    pub status: JobStatus,
    pub code: Option<i32>,
    pub duration: Duration,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct JobsReport {
    pub results: Vec<JobResult>,
    pub ok: bool,
    pub started: usize,
    pub finished: usize,
    pub failed: usize,
}

pub trait IJobExecutor: Send + Sync {
    fn run(&self, plan: &JobPlan, cfg: &JobsConfig) -> Result<JobsReport, JobsError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobsError {
    InvalidPlan(String),
    Host(String),
}

impl fmt::Display for JobsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsError::InvalidPlan(s) => write!(f, "invalid plan: {s}"),
            JobsError::Host(s) => write!(f, "host: {s}"),
        }
    }
}

impl std::error::Error for JobsError {}

/* ============================== Remote interface ============================== */

#[derive(Debug, Clone)]
pub struct RemoteRequest {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub allow_net: bool,
    pub allow_fs: bool,
}

#[derive(Debug, Clone)]
pub struct RemoteResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

pub trait IRemote: Send + Sync {
    fn fetch(&self, req: &RemoteRequest) -> Result<RemoteResponse, RemoteError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    Unsupported(String),
    Forbidden(String),
    NotFound(String),
    Io(String),
    Other(String),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Unsupported(s) => write!(f, "unsupported: {s}"),
            RemoteError::Forbidden(s) => write!(f, "forbidden: {s}"),
            RemoteError::NotFound(s) => write!(f, "not found: {s}"),
            RemoteError::Io(s) => write!(f, "io: {s}"),
            RemoteError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for RemoteError {}

/* ============================== default adapters (optional) ============================== */

/// Simple Output adapter to stdout/stderr.
/// Prefer using src/output.rs implementation; this is a minimal fallback.
pub struct StdoutOutput {
    pub level: Level,
}

impl StdoutOutput {
    pub fn new(level: Level) -> Self {
        Self { level }
    }
}

impl IOutput for StdoutOutput {
    fn emit(&self, level: Level, target: &str, msg: &str, kv: &[(&str, &str)]) {
        if level > self.level {
            return;
        }
        let mut line = format!("{level:?} [{target}] {msg}");
        if !kv.is_empty() {
            line.push(' ');
            for (k, v) in kv {
                line.push_str(k);
                line.push('=');
                line.push_str(v);
                line.push(' ');
            }
        }
        if level <= Level::Warn {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_debug() {
        let r = RuleId(1);
        assert!(format!("{r:?}").contains("RuleId"));
    }
}
