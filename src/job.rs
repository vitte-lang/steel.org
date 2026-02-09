// src/jobs.rs
//
// Steel — jobs (scheduler, execution plan, worker pool)
//
// Purpose:
// - Execute build "jobs" produced by the planner (remake/graph).
// - Provide:
//   - Job graph (rule -> job), dependencies, ready queue
//   - Parallel execution with bounded worker threads
//   - Structured events (start/end/stdout/stderr) via Output
//   - Cancellation / fail-fast / keep-going modes
//   - Deterministic logging ordering options
//   - Basic retries (optional) for flaky commands
//
// Notes:
// - No async runtime; uses std::thread + channels.
// - Command execution uses std::process::Command.
// - If you already have vmsjobs.rs / job.vit etc, adapt types accordingly.
// - This is "max": more knobs, still dependency-free.
//
// Integration points:
// - Replace the local Rule/Plan model with your real ones.
// - Hook `Output` from src/output.rs for consistent formatting.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/* ============================== external-ish hooks ============================== */

/// Minimal output interface (compatible with src/output.rs).
pub trait JobOutput: Send + Sync {
    fn info(&self, target: &str, msg: &str);
    fn warn(&self, target: &str, msg: &str);
    fn error(&self, target: &str, msg: &str);
    fn debug(&self, target: &str, msg: &str);
}

#[derive(Clone)]
pub struct NullOutput;

impl JobOutput for NullOutput {
    fn info(&self, _t: &str, _m: &str) {}
    fn warn(&self, _t: &str, _m: &str) {}
    fn error(&self, _t: &str, _m: &str) {}
    fn debug(&self, _t: &str, _m: &str) {}
}

/* ============================== ids/models ============================== */

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

impl JobId {
    pub fn new(v: u64) -> Self {
        Self(v)
    }
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JobId(0x{:016x})", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(u64);

impl RuleId {
    pub fn new(v: u64) -> Self {
        Self(v)
    }
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuleId(0x{:016x})", self.0)
    }
}

/// Minimal command spec.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        }
    }
}

/// A job corresponds to one rule execution.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub rule_id: RuleId,
    pub name: String,

    pub deps: Vec<JobId>,
    pub cmd: CommandSpec,

    pub retry: RetryPolicy,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub attempts: u32,      // total attempts (>=1)
    pub backoff_ms: u64,    // constant backoff between attempts
    pub retry_on_nonzero: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 1,
            backoff_ms: 0,
            retry_on_nonzero: false,
        }
    }
}

/// Execution plan: subset of jobs to run with dependency edges.
#[derive(Debug, Clone)]
pub struct JobPlan {
    pub jobs: Vec<Job>,
}

impl JobPlan {
    pub fn by_id(&self) -> HashMap<JobId, Job> {
        self.jobs.iter().cloned().map(|j| (j.id, j)).collect()
    }
}

/* ============================== config/results ============================== */

#[derive(Debug, Clone)]
pub struct JobsConfig {
    pub parallelism: usize,
    pub fail_fast: bool,
    pub keep_going: bool, // if true, continue running independent jobs after failures
    pub capture_output: bool,
    pub print_cmd: bool,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            parallelism: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
            fail_fast: true,
            keep_going: false,
            capture_output: true,
            print_cmd: true,
        }
    }
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

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobsError {
    InvalidPlan(String),
}

impl fmt::Display for JobsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsError::InvalidPlan(s) => write!(f, "invalid plan: {s}"),
        }
    }
}

impl std::error::Error for JobsError {}

/* ============================== executor ============================== */

pub struct JobExecutor {
    cfg: JobsConfig,
    out: Arc<dyn JobOutput>,
}

impl JobExecutor {
    pub fn new(cfg: JobsConfig, out: Arc<dyn JobOutput>) -> Self {
        Self { cfg, out }
    }

    pub fn run(&self, plan: &JobPlan) -> Result<JobsReport, JobsError> {
        validate_plan(plan)?;
        if plan.jobs.is_empty() {
            return Ok(JobsReport {
                results: Vec::new(),
                ok: true,
                started: 0,
                finished: 0,
                failed: 0,
            });
        }

        let by_id = plan.by_id();

        // Build dependency tracking
        let mut indeg: HashMap<JobId, usize> = HashMap::new();
        let mut adj: HashMap<JobId, Vec<JobId>> = HashMap::new();

        for j in &plan.jobs {
            indeg.entry(j.id).or_insert(0);
            adj.entry(j.id).or_insert_with(Vec::new);
        }
        for j in &plan.jobs {
            for d in &j.deps {
                if !by_id.contains_key(d) {
                    return Err(JobsError::InvalidPlan(format!(
                        "job {:?} depends on missing {:?}",
                        j.id, d
                    )));
                }
                adj.entry(*d).or_insert_with(Vec::new).push(j.id);
                *indeg.entry(j.id).or_insert(0) += 1;
            }
        }

        let mut ready = VecDeque::<JobId>::new();
        for (&id, &d) in &indeg {
            if d == 0 {
                ready.push_back(id);
            }
        }

        let shared = Arc::new(Mutex::new(SharedState {
            indeg,
            ready,
            results: Vec::new(),
            running: 0,
            failed: 0,
            started: 0,
            finished: 0,
            canceled: false,
            done: false,
        }));

        let (tx, rx) = std::sync::mpsc::channel::<WorkerMsg>();

        // Spawn workers
        let workers = self.cfg.parallelism.max(1);
        let mut handles = Vec::new();
        for worker_idx in 0..workers {
            let tx = tx.clone();
            let shared = Arc::clone(&shared);
            let by_id = by_id.clone();
            let out = Arc::clone(&self.out);
            let cfg = self.cfg.clone();

            handles.push(thread::spawn(move || worker_loop(worker_idx, shared, by_id, cfg, out, tx)));
        }
        drop(tx);

        // Coordinator loop
        let mut adj_local = adj; // move
        while let Ok(msg) = rx.recv() {
            match msg {
                WorkerMsg::JobFinished(res) => {
                    let mut st = shared.lock().unwrap();
                    st.running = st.running.saturating_sub(1);
                    st.finished += 1;

                    if res.status == JobStatus::Failed {
                        st.failed += 1;
                        if self.cfg.fail_fast && !self.cfg.keep_going {
                            st.canceled = true;
                        }
                    }

                    // record result
                    st.results.push(res.clone());

                    // update downstream
                    if let Some(nexts) = adj_local.remove(&res.id) {
                        for n in nexts {
                            if st.canceled && !self.cfg.keep_going {
                                continue;
                            }
                            if let Some(d) = st.indeg.get_mut(&n) {
                                *d = d.saturating_sub(1);
                                if *d == 0 {
                                    st.ready.push_back(n);
                                }
                            }
                        }
                    }

                    // Termination: all finished or no work possible
                    if st.finished >= plan.jobs.len() {
                        st.done = true;
                    }
                }
                WorkerMsg::WorkerIdle => {
                    // no-op
                }
            }

            // Wake condition: if done, break
            if shared.lock().unwrap().done {
                break;
            }
        }

        // Ensure workers exit
        shared.lock().unwrap().canceled = true;
        for h in handles {
            let _ = h.join();
        }

        let st = shared.lock().unwrap();
        let ok = st.failed == 0;
        Ok(JobsReport {
            results: st.results.clone(),
            ok,
            started: st.started,
            finished: st.finished,
            failed: st.failed,
        })
    }
}

/* ============================== shared state ============================== */

#[derive(Debug)]
struct SharedState {
    indeg: HashMap<JobId, usize>,
    ready: VecDeque<JobId>,
    results: Vec<JobResult>,
    running: usize,
    failed: usize,
    started: usize,
    finished: usize,
    canceled: bool,
    done: bool,
}

#[derive(Debug)]
enum WorkerMsg {
    JobFinished(JobResult),
    WorkerIdle,
}

/* ============================== worker loop ============================== */

fn worker_loop(
    worker_idx: usize,
    shared: Arc<Mutex<SharedState>>,
    by_id: HashMap<JobId, Job>,
    cfg: JobsConfig,
    out: Arc<dyn JobOutput>,
    tx: std::sync::mpsc::Sender<WorkerMsg>,
) {
    loop {
        let job_id_opt = {
            let mut st = shared.lock().unwrap();
            if st.canceled && !cfg.keep_going {
                return;
            }
            if let Some(id) = st.ready.pop_front() {
                st.running += 1;
                st.started += 1;
                Some(id)
            } else {
                None
            }
        };

        let Some(job_id) = job_id_opt else {
            // If no ready jobs, yield
            let _ = tx.send(WorkerMsg::WorkerIdle);
            thread::sleep(Duration::from_millis(10));
            // Check for termination condition: canceled OR no work and coordinator done.
            if shared.lock().unwrap().canceled && !cfg.keep_going {
                return;
            }
            continue;
        };

        let job = match by_id.get(&job_id) {
            Some(j) => j.clone(),
            None => {
                // Should not happen; treat as failure
                let res = JobResult {
                    id: job_id,
                    status: JobStatus::Failed,
                    code: None,
                    duration: Duration::from_millis(0),
                    stdout: Vec::new(),
                    stderr: b"internal: missing job".to_vec(),
                };
                let _ = tx.send(WorkerMsg::JobFinished(res));
                continue;
            }
        };

        out.info("jobs", &format!("[w{worker_idx}] start {}", job.name));
        if cfg.print_cmd {
            out.debug("jobs", &format!("cmd: {} {:?}", job.cmd.program, job.cmd.args));
        }

        let res = run_job(&job, &cfg, out.as_ref());
        let _ = tx.send(WorkerMsg::JobFinished(res));
    }
}

/* ============================== job execution ============================== */

fn run_job(job: &Job, cfg: &JobsConfig, out: &dyn JobOutput) -> JobResult {
    let t0 = Instant::now();

    let attempts = job.retry.attempts.max(1);
    let mut last = None::<JobResult>;

    for attempt in 1..=attempts {
        if attempt > 1 && job.retry.backoff_ms > 0 {
            thread::sleep(Duration::from_millis(job.retry.backoff_ms));
        }

        out.debug("jobs", &format!("{} attempt {}/{}", job.name, attempt, attempts));

        let mut cmd = Command::new(&job.cmd.program);
        cmd.args(&job.cmd.args);

        for (k, v) in &job.cmd.env {
            cmd.env(k, v);
        }

        if let Some(cwd) = &job.cmd.cwd {
            cmd.current_dir(cwd);
        }

        if cfg.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let dur = t0.elapsed();
                let res = JobResult {
                    id: job.id,
                    status: JobStatus::Failed,
                    code: None,
                    duration: dur,
                    stdout: Vec::new(),
                    stderr: format!("spawn failed: {e}").into_bytes(),
                };
                last = Some(res.clone());
                break;
            }
        };

        let (stdout, stderr, status) = if let Some(to) = job.timeout {
            wait_with_timeout(&mut child, to, cfg.capture_output)
        } else {
            wait_no_timeout(&mut child, cfg.capture_output)
        };

        let dur = t0.elapsed();
        let code = status.code();

        let ok = status.success();
        let res = JobResult {
            id: job.id,
            status: if ok { JobStatus::Success } else { JobStatus::Failed },
            code,
            duration: dur,
            stdout,
            stderr,
        };

        if ok {
            out.info("jobs", &format!("done {} ({:?})", job.name, dur));
            return res;
        }

        out.warn("jobs", &format!("fail {} code={:?}", job.name, code));
        last = Some(res.clone());

        if !job.retry.retry_on_nonzero {
            break;
        }
    }

    last.unwrap_or(JobResult {
        id: job.id,
        status: JobStatus::Failed,
        code: None,
        duration: t0.elapsed(),
        stdout: Vec::new(),
        stderr: b"unknown failure".to_vec(),
    })
}

fn wait_no_timeout(child: &mut std::process::Child, capture: bool) -> (Vec<u8>, Vec<u8>, ExitStatus) {
    if !capture {
        let status = child.wait().unwrap_or_else(|_| fake_status_fail());
        return (Vec::new(), Vec::new(), status);
    }

    let mut out = child.stdout.take();
    let mut err = child.stderr.take();

    let status = child.wait().unwrap_or_else(|_| fake_status_fail());

    let mut out_buf = Vec::new();
    let mut err_buf = Vec::new();

    if let Some(mut o) = out.take() {
        let _ = o.read_to_end(&mut out_buf);
    }
    if let Some(mut e) = err.take() {
        let _ = e.read_to_end(&mut err_buf);
    }

    (out_buf, err_buf, status)
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration, capture: bool) -> (Vec<u8>, Vec<u8>, ExitStatus) {
    let start = Instant::now();

    if !capture {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return (Vec::new(), Vec::new(), status),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return (Vec::new(), b"timeout".to_vec(), fake_status_fail());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return (Vec::new(), Vec::new(), fake_status_fail()),
            }
        }
    }

    // capture mode: poll and then read pipes after exit/kill
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out_buf = Vec::new();
                let mut err_buf = Vec::new();

                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_end(&mut out_buf);
                }
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_end(&mut err_buf);
                }
                return (out_buf, err_buf, status);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (Vec::new(), b"timeout".to_vec(), fake_status_fail());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return (Vec::new(), Vec::new(), fake_status_fail()),
        }
    }
}

#[cfg(unix)]
fn fake_status_fail() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

#[cfg(windows)]
fn fake_status_fail() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

/* ============================== plan validation ============================== */

fn validate_plan(plan: &JobPlan) -> Result<(), JobsError> {
    if plan.jobs.is_empty() {
        return Ok(());
    }

    let mut ids = HashSet::<JobId>::new();
    for j in &plan.jobs {
        if !ids.insert(j.id) {
            return Err(JobsError::InvalidPlan(format!("duplicate job id {:?}", j.id)));
        }
        if j.cmd.program.trim().is_empty() {
            return Err(JobsError::InvalidPlan(format!("job {:?} has empty program", j.id)));
        }
        if j.retry.attempts == 0 {
            return Err(JobsError::InvalidPlan(format!("job {:?} retry.attempts=0", j.id)));
        }
    }

    Ok(())
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    struct TestOut(Arc<Mutex<Vec<String>>>);
    impl JobOutput for TestOut {
        fn info(&self, t: &str, m: &str) {
            self.0.lock().unwrap().push(format!("I {t} {m}"));
        }
        fn warn(&self, t: &str, m: &str) {
            self.0.lock().unwrap().push(format!("W {t} {m}"));
        }
        fn error(&self, t: &str, m: &str) {
            self.0.lock().unwrap().push(format!("E {t} {m}"));
        }
        fn debug(&self, t: &str, m: &str) {
            self.0.lock().unwrap().push(format!("D {t} {m}"));
        }
    }

    #[test]
    fn validates_empty_plan_ok() {
        let ex = JobExecutor::new(JobsConfig::default(), Arc::new(NullOutput));
        let rep = ex.run(&JobPlan { jobs: vec![] }).unwrap();
        assert!(rep.ok);
    }

    #[test]
    fn invalid_empty_program_fails() {
        let ex = JobExecutor::new(JobsConfig::default(), Arc::new(NullOutput));
        let plan = JobPlan {
            jobs: vec![Job {
                id: JobId::new(1),
                rule_id: RuleId::new(1),
                name: "x".to_string(),
                deps: vec![],
                cmd: CommandSpec::new(""),
                retry: RetryPolicy::default(),
                timeout: None,
            }],
        };
        assert!(ex.run(&plan).is_err());
    }

    // Note: We don't run real commands in tests to keep them hermetic.
}
