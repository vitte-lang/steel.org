// src/vmsjobs.rs
//
// Steel — VMS job model (minimal, std-only)
//
// Purpose:
// - Provide a small, self-contained job graph model used by vmsify/vmsfunctions.
// - Keep it dependency-free and deterministic.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Job identifier (stable string).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub String);

impl JobId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Logging levels for job steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// Command execution specification.
#[derive(Debug, Clone)]
pub struct ExecSpec {
    pub label: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub timeout: Option<Duration>,
    pub capture_stdout: bool,
    pub capture_stderr: bool,
}

impl ExecSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            label: None,
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            timeout: None,
            capture_stdout: false,
            capture_stderr: false,
        }
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn capture_stdout(mut self) -> Self {
        self.capture_stdout = true;
        self
    }

    pub fn capture_stderr(mut self) -> Self {
        self.capture_stderr = true;
        self
    }
}

/// Inline execution specification.
#[derive(Clone)]
pub struct InlineSpec {
    pub label: Option<String>,
    pub f: Arc<dyn Fn() -> Result<(), String> + Send + Sync + 'static>,
}

impl InlineSpec {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            label: None,
            f: Arc::new(f),
        }
    }

    pub fn label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }
}

impl fmt::Debug for InlineSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InlineSpec")
            .field("label", &self.label)
            .finish()
    }
}

/// A single step in a job.
#[derive(Debug, Clone)]
pub enum JobStep {
    Log { level: LogLevel, message: String },
    Command(ExecSpec),
    Inline(InlineSpec),
}

/// Job definition.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub name: String,
    pub deps: BTreeSet<JobId>,
    pub steps: Vec<JobStep>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub tags: BTreeSet<String>,
    pub allow_failure: bool,
}

impl Job {
    pub fn new(id: JobId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            deps: BTreeSet::new(),
            steps: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            tags: BTreeSet::new(),
            allow_failure: false,
        }
    }
}

/// Job graph.
#[derive(Debug, Clone, Default)]
pub struct JobGraph {
    pub jobs: BTreeMap<JobId, Job>,
}

impl JobGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, job: Job) -> Result<(), JobGraphError> {
        if self.jobs.contains_key(&job.id) {
            return Err(JobGraphError::Duplicate(job.id.0));
        }
        self.jobs.insert(job.id.clone(), job);
        Ok(())
    }
}

#[derive(Debug)]
pub enum JobGraphError {
    Duplicate(String),
}

impl fmt::Display for JobGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobGraphError::Duplicate(s) => write!(f, "duplicate job id: {s}"),
        }
    }
}

impl std::error::Error for JobGraphError {}

/// Runtime execution context (minimal).
#[derive(Debug, Clone)]
pub struct RunContext {
    pub root: PathBuf,
    pub jobs_dir: PathBuf,
    pub max_parallel: usize,
    pub dry_run: bool,
}

impl RunContext {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            jobs_dir: root.join("jobs"),
            root,
            max_parallel: 1,
            dry_run: false,
        }
    }

    pub fn max_parallel(mut self, n: usize) -> Self {
        self.max_parallel = n.max(1);
        self
    }

    pub fn dry_run(mut self, yes: bool) -> Self {
        self.dry_run = yes;
        self
    }
}
