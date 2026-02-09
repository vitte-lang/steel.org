//! Capsule sandbox runtime façade.
//!
//! This module provides a runtime-friendly interface to enforce `CapsulePolicy`
//! against I/O, env, net, time, and process operations.
//!
//! Design goals:
//! - deny-by-default
//! - small surface area
//! - explicit checks for every side-effect
//! - ergonomic error reporting for diagnostics
//!
//! This file does NOT implement OS-level isolation by itself. It is the policy
//! gate that you wire into:
//! - filesystem abstraction (VFS layer / host FS adapters)
//! - env getter/setter
//! - DNS resolver / socket API
//! - process spawning API
//! - clock/sleep abstractions
//! - resource limit plumbing (rlimit/cgroups/job objects)
//!
//! If you have an OS sandbox (seccomp, Landlock, AppContainer, etc.), this
//! module should still be used to validate user-level intent and to produce
//! consistent policy-denial errors.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::policy::{
    CapsulePolicy, CompiledPolicy, Decision, EnvOp, FsOp, NetOp, ProcOp, TimeOp,
};

#[derive(Debug)]
pub enum SandboxError {
    Denied(SandboxDenied),
    Policy(super::policy::PolicyError),
    Io(std::io::Error),
    Other(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::Denied(d) => write!(f, "denied: {d}"),
            SandboxError::Policy(e) => write!(f, "policy error: {e}"),
            SandboxError::Io(e) => write!(f, "io error: {e}"),
            SandboxError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for SandboxError {}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        SandboxError::Io(e)
    }
}

impl From<super::policy::PolicyError> for SandboxError {
    fn from(e: super::policy::PolicyError) -> Self {
        SandboxError::Policy(e)
    }
}

/// Structured denial info for diagnostics/logs.
#[derive(Debug, Clone)]
pub struct SandboxDenied {
    pub domain: SandboxDomain,
    pub action: String,
    pub target: String,
    pub reason: String,
}

impl fmt::Display for SandboxDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} on {} ({})",
            self.domain, self.action, self.target, self.reason
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxDomain {
    Fs,
    Env,
    Net,
    Time,
    Proc,
    Limits,
}

impl fmt::Display for SandboxDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SandboxDomain::Fs => "fs",
            SandboxDomain::Env => "env",
            SandboxDomain::Net => "net",
            SandboxDomain::Time => "time",
            SandboxDomain::Proc => "proc",
            SandboxDomain::Limits => "limits",
        };
        write!(f, "{s}")
    }
}

/// Minimal time provider abstraction.
pub trait Clock: Send + Sync {
    fn now(&self) -> SystemTime;
    fn sleep(&self, dur: Duration) -> Result<(), SandboxError>;
}

/// Default host clock.
#[derive(Debug, Default)]
pub struct HostClock;

impl Clock for HostClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn sleep(&self, dur: Duration) -> Result<(), SandboxError> {
        std::thread::sleep(dur);
        Ok(())
    }
}

/// Filesystem backend abstraction.
/// The sandbox mediates calls; backend does actual IO.
pub trait FsBackend: Send + Sync {
    fn read(&self, path: &Path) -> Result<Vec<u8>, SandboxError>;
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), SandboxError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), SandboxError>;
    fn remove_file(&self, path: &Path) -> Result<(), SandboxError>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, SandboxError>;
    fn metadata_len(&self, path: &Path) -> Result<u64, SandboxError>;
    fn exists(&self, path: &Path) -> Result<bool, SandboxError>;
}

/// Host FS backend (plain std::fs).
#[derive(Debug, Default)]
pub struct HostFs;

impl FsBackend for HostFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, SandboxError> {
        Ok(std::fs::read(path)?)
    }

    fn write(&self, path: &Path, data: &[u8]) -> Result<(), SandboxError> {
        Ok(std::fs::write(path, data)?)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), SandboxError> {
        Ok(std::fs::create_dir_all(path)?)
    }

    fn remove_file(&self, path: &Path) -> Result<(), SandboxError> {
        Ok(std::fs::remove_file(path)?)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, SandboxError> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(path)? {
            out.push(e?.path());
        }
        Ok(out)
    }

    fn metadata_len(&self, path: &Path) -> Result<u64, SandboxError> {
        Ok(std::fs::metadata(path)?.len())
    }

    fn exists(&self, path: &Path) -> Result<bool, SandboxError> {
        Ok(path.exists())
    }
}

/// Environment backend abstraction.
pub trait EnvBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SandboxError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SandboxError>;
    fn unset(&self, key: &str) -> Result<(), SandboxError>;
}

/// Host env backend.
#[derive(Debug, Default)]
pub struct HostEnv;

impl EnvBackend for HostEnv {
    fn get(&self, key: &str) -> Result<Option<String>, SandboxError> {
        Ok(std::env::var(key).ok())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SandboxError> {
        std::env::set_var(key, value);
        Ok(())
    }

    fn unset(&self, key: &str) -> Result<(), SandboxError> {
        std::env::remove_var(key);
        Ok(())
    }
}

/// Network backend abstraction.
/// This is deliberately small; adapt to your actual net stack.
pub trait NetBackend: Send + Sync {
    fn resolve_dns(&self, name: &str) -> Result<Vec<String>, SandboxError>;
    fn connect(&self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError>;
    fn bind(&self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError>;
    fn listen(&self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError>;
}

/// A no-op net backend that always errors. Useful for default builds.
#[derive(Debug, Default)]
pub struct NullNet;

impl NetBackend for NullNet {
    fn resolve_dns(&self, _name: &str) -> Result<Vec<String>, SandboxError> {
        Err(SandboxError::Other("network backend not available".into()))
    }

    fn connect(&self, _host: &str, _port: u16, _proto: Option<&str>) -> Result<(), SandboxError> {
        Err(SandboxError::Other("network backend not available".into()))
    }

    fn bind(&self, _host: &str, _port: u16, _proto: Option<&str>) -> Result<(), SandboxError> {
        Err(SandboxError::Other("network backend not available".into()))
    }

    fn listen(&self, _host: &str, _port: u16, _proto: Option<&str>) -> Result<(), SandboxError> {
        Err(SandboxError::Other("network backend not available".into()))
    }
}

/// Process backend abstraction.
/// The sandbox gate can restrict by policy and then delegate.
pub trait ProcBackend: Send + Sync {
    fn spawn(&self, bin: &Path, args: &[String]) -> Result<i32, SandboxError>;
}

/// Host process backend (std::process::Command).
#[derive(Debug, Default)]
pub struct HostProc;

impl ProcBackend for HostProc {
    fn spawn(&self, bin: &Path, args: &[String]) -> Result<i32, SandboxError> {
        let status = std::process::Command::new(bin).args(args).status()?;
        Ok(status.code().unwrap_or(1))
    }
}

/// Capsule sandbox context.
/// Holds compiled policy + backends + runtime accounting.
#[derive(Debug)]
pub struct Sandbox {
    policy: Arc<CompiledPolicy>,
    fs: Arc<dyn FsBackend>,
    env: Arc<dyn EnvBackend>,
    net: Arc<dyn NetBackend>,
    proc: Arc<dyn ProcBackend>,
    clock: Arc<dyn Clock>,

    // Runtime accounting (soft limits, enforced here; hard limits should be enforced by OS too).
    usage: Usage,
}

/// Simple runtime usage tracking for soft limits.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub started_at: Option<SystemTime>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub io_bytes: u64,
}

impl Sandbox {
    /// Create a sandbox from an already compiled policy.
    pub fn new_compiled(policy: CompiledPolicy) -> Self {
        Self {
            policy: Arc::new(policy),
            fs: Arc::new(HostFs::default()),
            env: Arc::new(HostEnv::default()),
            net: Arc::new(NullNet::default()),
            proc: Arc::new(HostProc::default()),
            clock: Arc::new(HostClock::default()),
            usage: Usage::default(),
        }
    }

    /// Create a sandbox from a policy (compiles/validates it).
    pub fn new(policy: &CapsulePolicy) -> Result<Self, SandboxError> {
        policy.validate()?;
        let compiled = policy.compile()?;
        Ok(Self::new_compiled(compiled))
    }

    /// Swap filesystem backend.
    pub fn with_fs_backend(mut self, fs: Arc<dyn FsBackend>) -> Self {
        self.fs = fs;
        self
    }

    /// Swap env backend.
    pub fn with_env_backend(mut self, env: Arc<dyn EnvBackend>) -> Self {
        self.env = env;
        self
    }

    /// Swap network backend.
    pub fn with_net_backend(mut self, net: Arc<dyn NetBackend>) -> Self {
        self.net = net;
        self
    }

    /// Swap process backend.
    pub fn with_proc_backend(mut self, proc: Arc<dyn ProcBackend>) -> Self {
        self.proc = proc;
        self
    }

    /// Swap clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Access compiled policy.
    pub fn policy(&self) -> &CompiledPolicy {
        &self.policy
    }

    /// Start accounting (wall clock).
    pub fn start(&mut self) {
        if self.usage.started_at.is_none() {
            self.usage.started_at = Some(self.clock.now());
        }
    }

    pub fn usage(&self) -> &Usage {
        &self.usage
    }

    /* ---------------------------- FS gated ops ---------------------------- */

    pub fn fs_read(&mut self, path: impl AsRef<Path>) -> Result<Vec<u8>, SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::Read, p)?;
        let data = self.fs.read(p)?;
        self.usage.io_bytes = self.usage.io_bytes.saturating_add(data.len() as u64);
        self.check_limits_io()?;
        Ok(data)
    }

    pub fn fs_write(&mut self, path: impl AsRef<Path>, data: &[u8]) -> Result<(), SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::Write, p)?;
        self.fs.write(p, data)?;
        self.usage.io_bytes = self.usage.io_bytes.saturating_add(data.len() as u64);
        self.check_limits_io()?;
        Ok(())
    }

    pub fn fs_create_dir_all(&mut self, path: impl AsRef<Path>) -> Result<(), SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::Create, p)?;
        self.fs.create_dir_all(p)?;
        Ok(())
    }

    pub fn fs_remove_file(&mut self, path: impl AsRef<Path>) -> Result<(), SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::Delete, p)?;
        self.fs.remove_file(p)?;
        Ok(())
    }

    pub fn fs_list_dir(&mut self, path: impl AsRef<Path>) -> Result<Vec<PathBuf>, SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::List, p)?;
        self.fs.list_dir(p)
    }

    pub fn fs_metadata_len(&mut self, path: impl AsRef<Path>) -> Result<u64, SandboxError> {
        self.start();
        let p = path.as_ref();
        self.check_fs(FsOp::Metadata, p)?;
        self.fs.metadata_len(p)
    }

    pub fn fs_exists(&mut self, path: impl AsRef<Path>) -> Result<bool, SandboxError> {
        self.start();
        let p = path.as_ref();
        // existence is a metadata-like op
        self.check_fs(FsOp::Metadata, p)?;
        self.fs.exists(p)
    }

    /* ---------------------------- ENV gated ops --------------------------- */

    pub fn env_get(&mut self, key: &str) -> Result<Option<String>, SandboxError> {
        self.start();
        self.check_env(EnvOp::Read, key)?;
        self.env.get(key)
    }

    pub fn env_set(&mut self, key: &str, value: &str) -> Result<(), SandboxError> {
        self.start();
        self.check_env(EnvOp::Write, key)?;
        self.env.set(key, value)
    }

    pub fn env_unset(&mut self, key: &str) -> Result<(), SandboxError> {
        self.start();
        self.check_env(EnvOp::Unset, key)?;
        self.env.unset(key)
    }

    /* ---------------------------- NET gated ops --------------------------- */

    pub fn net_resolve_dns(&mut self, name: &str) -> Result<Vec<String>, SandboxError> {
        self.start();
        self.check_dns(name)?;
        self.net.resolve_dns(name)
    }

    pub fn net_connect(&mut self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError> {
        self.start();
        self.check_net(NetOp::Connect, host, port, proto)?;
        self.net.connect(host, port, proto)
    }

    pub fn net_bind(&mut self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError> {
        self.start();
        self.check_net(NetOp::Bind, host, port, proto)?;
        self.net.bind(host, port, proto)
    }

    pub fn net_listen(&mut self, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError> {
        self.start();
        self.check_net(NetOp::Listen, host, port, proto)?;
        self.net.listen(host, port, proto)
    }

    /* ---------------------------- TIME gated ops -------------------------- */

    pub fn time_now(&mut self) -> Result<SystemTime, SandboxError> {
        self.start();
        self.check_time(TimeOp::ReadClock)?;
        Ok(self.clock.now())
    }

    pub fn time_sleep(&mut self, dur: Duration) -> Result<(), SandboxError> {
        self.start();
        self.check_time(TimeOp::Sleep)?;
        self.clock.sleep(dur)?;
        self.check_limits_wall()?;
        Ok(())
    }

    /* ---------------------------- PROC gated ops -------------------------- */

    pub fn proc_spawn(&mut self, bin: impl AsRef<Path>, args: &[String]) -> Result<i32, SandboxError> {
        self.start();
        let p = bin.as_ref();
        self.check_proc_spawn(p)?;
        self.proc.spawn(p, args)
    }

    /* ---------------------------- Output accounting ----------------------- */

    pub fn account_stdout(&mut self, bytes: usize) -> Result<(), SandboxError> {
        self.usage.stdout_bytes = self.usage.stdout_bytes.saturating_add(bytes as u64);
        self.check_limits_output()?;
        Ok(())
    }

    pub fn account_stderr(&mut self, bytes: usize) -> Result<(), SandboxError> {
        self.usage.stderr_bytes = self.usage.stderr_bytes.saturating_add(bytes as u64);
        self.check_limits_output()?;
        Ok(())
    }

    /* ---------------------------- Internal checks ------------------------- */

    fn check_fs(&self, op: FsOp, path: &Path) -> Result<(), SandboxError> {
        let s = path_to_policy_string(path);
        let d = self.policy.check_fs(op, &s);
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Fs,
            action: format!("{op:?}"),
            target: s,
            reason: "policy denied".into(),
        }))
    }

    fn check_env(&self, op: EnvOp, key: &str) -> Result<(), SandboxError> {
        let d = self.policy.check_env(op, key);
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Env,
            action: format!("{op:?}"),
            target: key.to_string(),
            reason: "policy denied".into(),
        }))
    }

    fn check_dns(&self, name: &str) -> Result<(), SandboxError> {
        let d = self.policy.check_dns(name);
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Net,
            action: "ResolveDns".into(),
            target: name.to_string(),
            reason: "policy denied".into(),
        }))
    }

    fn check_net(&self, op: NetOp, host: &str, port: u16, proto: Option<&str>) -> Result<(), SandboxError> {
        let d = self.policy.check_net(op, host, port, proto);
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Net,
            action: format!("{op:?}"),
            target: format!("{host}:{port}{}", proto.map(|p| format!("/{p}")).unwrap_or_default()),
            reason: "policy denied".into(),
        }))
    }

    fn check_time(&self, op: TimeOp) -> Result<(), SandboxError> {
        let d = self.policy.check_time(op);
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Time,
            action: format!("{op:?}"),
            target: "-".into(),
            reason: "policy denied".into(),
        }))
    }

    fn check_proc_spawn(&self, bin: &Path) -> Result<(), SandboxError> {
        let s = path_to_policy_string(bin);
        let d = self.policy.check_proc(ProcOp::Spawn, Some(&s));
        if d.is_allowed() {
            return Ok(());
        }
        Err(SandboxError::Denied(SandboxDenied {
            domain: SandboxDomain::Proc,
            action: "Spawn".into(),
            target: s,
            reason: "policy denied".into(),
        }))
    }

    fn check_limits_wall(&self) -> Result<(), SandboxError> {
        let Some(wall_ms) = self.policy.limits().wall_ms else {
            return Ok(());
        };
        let Some(start) = self.usage.started_at else {
            return Ok(());
        };
        let now = self.clock.now();
        let elapsed = now.duration_since(start).unwrap_or(Duration::from_millis(0));
        if elapsed.as_millis() as u64 > wall_ms {
            return Err(SandboxError::Denied(SandboxDenied {
                domain: SandboxDomain::Limits,
                action: "WallTime".into(),
                target: format!("{}ms", elapsed.as_millis()),
                reason: format!("exceeds wall_ms={wall_ms}"),
            }));
        }
        Ok(())
    }

    fn check_limits_output(&self) -> Result<(), SandboxError> {
        if let Some(max) = self.policy.limits().stdout_bytes {
            if self.usage.stdout_bytes > max {
                return Err(SandboxError::Denied(SandboxDenied {
                    domain: SandboxDomain::Limits,
                    action: "StdoutBytes".into(),
                    target: self.usage.stdout_bytes.to_string(),
                    reason: format!("exceeds stdout_bytes={max}"),
                }));
            }
        }
        if let Some(max) = self.policy.limits().stderr_bytes {
            if self.usage.stderr_bytes > max {
                return Err(SandboxError::Denied(SandboxDenied {
                    domain: SandboxDomain::Limits,
                    action: "StderrBytes".into(),
                    target: self.usage.stderr_bytes.to_string(),
                    reason: format!("exceeds stderr_bytes={max}"),
                }));
            }
        }
        Ok(())
    }

    fn check_limits_io(&self) -> Result<(), SandboxError> {
        if let Some(max) = self.policy.limits().io_bytes {
            if self.usage.io_bytes > max {
                return Err(SandboxError::Denied(SandboxDenied {
                    domain: SandboxDomain::Limits,
                    action: "IoBytes".into(),
                    target: self.usage.io_bytes.to_string(),
                    reason: format!("exceeds io_bytes={max}"),
                }));
            }
        }
        Ok(())
    }
}

/// Convert a `Path` to a string suitable for `PathMatcher`.
/// - normalizes `\` to `/`
/// - keeps relative paths relative (no canonicalize: avoid TOCTOU + implicit FS reads)
fn path_to_policy_string(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    s.replace('\\', "/")
}

/* ---------------------------- Convenience types --------------------------- */

/// A convenience builder for assembling a Sandbox with custom backends.
#[derive(Debug, Default)]
pub struct SandboxBuilder {
    policy: Option<CapsulePolicy>,
    compiled: Option<CompiledPolicy>,
    fs: Option<Arc<dyn FsBackend>>,
    env: Option<Arc<dyn EnvBackend>>,
    net: Option<Arc<dyn NetBackend>>,
    proc: Option<Arc<dyn ProcBackend>>,
    clock: Option<Arc<dyn Clock>>,
}

impl SandboxBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn policy(mut self, p: CapsulePolicy) -> Self {
        self.policy = Some(p);
        self
    }

    pub fn compiled_policy(mut self, p: CompiledPolicy) -> Self {
        self.compiled = Some(p);
        self
    }

    pub fn fs_backend(mut self, fs: Arc<dyn FsBackend>) -> Self {
        self.fs = Some(fs);
        self
    }

    pub fn env_backend(mut self, env: Arc<dyn EnvBackend>) -> Self {
        self.env = Some(env);
        self
    }

    pub fn net_backend(mut self, net: Arc<dyn NetBackend>) -> Self {
        self.net = Some(net);
        self
    }

    pub fn proc_backend(mut self, proc: Arc<dyn ProcBackend>) -> Self {
        self.proc = Some(proc);
        self
    }

    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    pub fn build(self) -> Result<Sandbox, SandboxError> {
        let compiled = if let Some(c) = self.compiled {
            c
        } else if let Some(p) = self.policy {
            p.validate()?;
            p.compile()?
        } else {
            return Err(SandboxError::Other("SandboxBuilder: missing policy".into()));
        };

        let mut sb = Sandbox::new_compiled(compiled);

        if let Some(fs) = self.fs {
            sb = sb.with_fs_backend(fs);
        }
        if let Some(env) = self.env {
            sb = sb.with_env_backend(env);
        }
        if let Some(net) = self.net {
            sb = sb.with_net_backend(net);
        }
        if let Some(proc) = self.proc {
            sb = sb.with_proc_backend(proc);
        }
        if let Some(clock) = self.clock {
            sb = sb.with_clock(clock);
        }

        Ok(sb)
    }
}

/* ---------------------------- Tests -------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capsule::policy::{DefaultMode, PathPattern};

    #[test]
    fn deny_by_default_fs() {
        let pol = CapsulePolicy::default();
        let mut sb = Sandbox::new(&pol).unwrap();
        let e = sb.fs_read("Cargo.toml").unwrap_err();
        match e {
            SandboxError::Denied(d) => assert_eq!(d.domain, SandboxDomain::Fs),
            _ => panic!("expected denied"),
        }
    }

    #[test]
    fn allow_read_pattern_fs() {
        let mut pol = CapsulePolicy::default();
        pol.fs.read.allow.push(PathPattern("./**".into()));
        let mut sb = Sandbox::new(&pol).unwrap();

        // We cannot assume files exist in test env; just ensure policy passes check via exists.
        // exists() is metadata op, so allow metadata too.
        pol.fs.metadata.allow.push(PathPattern("./**".into()));
        let mut sb = Sandbox::new(&pol).unwrap();

        let _ = sb.fs_exists("./").unwrap();
    }

    #[test]
    fn limits_stdout() {
        let mut pol = CapsulePolicy::default();
        pol.fs.metadata.default = DefaultMode::Allow;
        pol.time.read_clock = DefaultMode::Allow;
        pol.limits.stdout_bytes = Some(4);

        let mut sb = Sandbox::new(&pol).unwrap();
        sb.account_stdout(2).unwrap();
        sb.account_stdout(2).unwrap();
        assert!(sb.account_stdout(1).is_err());
    }
}
