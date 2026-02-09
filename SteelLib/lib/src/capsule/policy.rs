//! Capsule security policy model + evaluation helpers.
//!
//! This module defines a compact but expressive policy surface for Steel “capsules”
//! (sandbox policies): filesystem, environment, networking, time, process, and resource
//! limits. It is designed to be:
//! - deterministic (no hidden implicit allow)
//! - serializable/deserializable
//! - easy to evaluate (deny-by-default)
//!
//! Notes:
//! - Any “Unknown/Unspecified” dimension defaults to DENY (safe).
//! - Prefer explicit allowlists over broad wildcards.
//!
//! Typical flow:
//!   1) Load `CapsulePolicy` from manifest / config
//!   2) `compile()` to `CompiledPolicy` (normalize globs, lower-case, expand aliases)
//!   3) Use `check_*()` methods at runtime to authorize actions

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    InvalidValue(String),
    InvalidRule(String),
    InvalidLimit(String),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyError::InvalidValue(s) => write!(f, "invalid policy value: {s}"),
            PolicyError::InvalidRule(s) => write!(f, "invalid policy rule: {s}"),
            PolicyError::InvalidLimit(s) => write!(f, "invalid policy limit: {s}"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// Decision result for checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    #[inline]
    pub fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// Global “default” when a dimension is absent.
/// We use deny-by-default semantics for safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultMode {
    Deny,
    Allow,
}

impl Default for DefaultMode {
    fn default() -> Self {
        DefaultMode::Deny
    }
}

/// A generic allow/deny rule set with optional default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleSet<T> {
    pub default: DefaultMode,
    pub allow: Vec<T>,
    pub deny: Vec<T>,
}

impl<T> RuleSet<T> {
    pub fn deny_all() -> Self {
        Self {
            default: DefaultMode::Deny,
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }

    pub fn allow_all() -> Self {
        Self {
            default: DefaultMode::Allow,
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }
}

/// File-system action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FsOp {
    Read,
    Write,
    Exec,
    Create,
    Delete,
    List,
    Metadata,
}

/// Environment action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvOp {
    Read,
    Write,
    Unset,
}

/// Network action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetOp {
    Connect,
    Bind,
    Listen,
    ResolveDns,
}

/// Process action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcOp {
    Spawn,
    Signal,
    SetUid,
    SetGid,
    SetCap,
}

/// Time action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeOp {
    ReadClock,
    SetClock,
    Sleep,
}

/// A simple path pattern (glob-like).
/// Supported:
/// - exact path (no wildcard)
/// - `*` for a single path segment
/// - `**` for any suffix
///
/// Examples:
/// - `/usr/bin/*`
/// - `/home/**`
/// - `C:\Users\*\Documents\**`
///
/// On Windows, `\` is normalized to `/` in compiled form for matching.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathPattern(pub String);

/// A simple string pattern (glob-like) for env keys, hosts, etc.
/// Supported:
/// - exact
/// - `*` wildcard for any sequence (like shell glob)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StrPattern(pub String);

/// Network endpoint pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetEndpoint {
    /// Hostname, IP literal, or pattern (e.g. `*.example.com`, `10.*`)
    pub host: StrPattern,
    /// Port number or 0 for any.
    pub port: u16,
    /// Optional protocol (e.g. `tcp`, `udp`). Empty means any.
    pub proto: Option<String>,
}

/// Resource limits for a capsule.
/// All limits are optional; absent means “unlimited” (but still subject to system).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Limits {
    /// Max wall time in milliseconds for the capsule execution.
    pub wall_ms: Option<u64>,
    /// Max CPU time in milliseconds.
    pub cpu_ms: Option<u64>,
    /// Max resident memory in bytes.
    pub memory_bytes: Option<u64>,
    /// Max number of open files.
    pub open_files: Option<u64>,
    /// Max number of processes/threads (implementation-specific).
    pub processes: Option<u64>,
    /// Max stdout bytes.
    pub stdout_bytes: Option<u64>,
    /// Max stderr bytes.
    pub stderr_bytes: Option<u64>,
    /// Max total IO bytes (read+write), implementation-specific.
    pub io_bytes: Option<u64>,
}

/// Full capsule policy (human-facing).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapsulePolicy {
    /// Filesystem rules per operation.
    pub fs: FsPolicy,
    /// Environment rules.
    pub env: EnvPolicy,
    /// Network rules.
    pub net: NetPolicy,
    /// Time rules.
    pub time: TimePolicy,
    /// Process rules.
    pub proc: ProcPolicy,
    /// Resource limits.
    pub limits: Limits,
    /// Optional user-defined metadata (ignored by enforcement).
    pub meta: BTreeMap<String, String>,
}

/// Filesystem policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPolicy {
    pub read: RuleSet<PathPattern>,
    pub write: RuleSet<PathPattern>,
    pub exec: RuleSet<PathPattern>,
    pub create: RuleSet<PathPattern>,
    pub delete: RuleSet<PathPattern>,
    pub list: RuleSet<PathPattern>,
    pub metadata: RuleSet<PathPattern>,
}

impl Default for FsPolicy {
    fn default() -> Self {
        Self {
            read: RuleSet::deny_all(),
            write: RuleSet::deny_all(),
            exec: RuleSet::deny_all(),
            create: RuleSet::deny_all(),
            delete: RuleSet::deny_all(),
            list: RuleSet::deny_all(),
            metadata: RuleSet::deny_all(),
        }
    }
}

/// Env policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPolicy {
    pub read: RuleSet<StrPattern>,
    pub write: RuleSet<StrPattern>,
    pub unset: RuleSet<StrPattern>,
}

impl Default for EnvPolicy {
    fn default() -> Self {
        Self {
            read: RuleSet::deny_all(),
            write: RuleSet::deny_all(),
            unset: RuleSet::deny_all(),
        }
    }
}

/// Network policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetPolicy {
    pub connect: RuleSet<NetEndpoint>,
    pub bind: RuleSet<NetEndpoint>,
    pub listen: RuleSet<NetEndpoint>,
    pub resolve_dns: RuleSet<StrPattern>,
}

impl Default for NetPolicy {
    fn default() -> Self {
        Self {
            connect: RuleSet::deny_all(),
            bind: RuleSet::deny_all(),
            listen: RuleSet::deny_all(),
            resolve_dns: RuleSet::deny_all(),
        }
    }
}

/// Time policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePolicy {
    pub read_clock: DefaultMode,
    pub set_clock: DefaultMode,
    pub sleep: DefaultMode,
}

impl Default for TimePolicy {
    fn default() -> Self {
        Self {
            read_clock: DefaultMode::Deny,
            set_clock: DefaultMode::Deny,
            sleep: DefaultMode::Deny,
        }
    }
}

/// Process policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcPolicy {
    pub spawn: DefaultMode,
    pub signal: DefaultMode,
    pub setuid: DefaultMode,
    pub setgid: DefaultMode,
    pub setcap: DefaultMode,
    /// Allowed executables (only used if spawn is allowed).
    pub allowed_bins: RuleSet<PathPattern>,
}

impl Default for ProcPolicy {
    fn default() -> Self {
        Self {
            spawn: DefaultMode::Deny,
            signal: DefaultMode::Deny,
            setuid: DefaultMode::Deny,
            setgid: DefaultMode::Deny,
            setcap: DefaultMode::Deny,
            allowed_bins: RuleSet::deny_all(),
        }
    }
}

/// A normalized/compiled policy optimized for checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPolicy {
    pub fs: CompiledFsPolicy,
    pub env: CompiledEnvPolicy,
    pub net: CompiledNetPolicy,
    pub time: TimePolicy,
    pub proc: CompiledProcPolicy,
    pub limits: Limits,
    pub meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFsPolicy {
    pub read: CompiledRuleSet<PathMatcher>,
    pub write: CompiledRuleSet<PathMatcher>,
    pub exec: CompiledRuleSet<PathMatcher>,
    pub create: CompiledRuleSet<PathMatcher>,
    pub delete: CompiledRuleSet<PathMatcher>,
    pub list: CompiledRuleSet<PathMatcher>,
    pub metadata: CompiledRuleSet<PathMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledEnvPolicy {
    pub read: CompiledRuleSet<StrMatcher>,
    pub write: CompiledRuleSet<StrMatcher>,
    pub unset: CompiledRuleSet<StrMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNetPolicy {
    pub connect: CompiledRuleSet<NetMatcher>,
    pub bind: CompiledRuleSet<NetMatcher>,
    pub listen: CompiledRuleSet<NetMatcher>,
    pub resolve_dns: CompiledRuleSet<StrMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledProcPolicy {
    pub spawn: DefaultMode,
    pub signal: DefaultMode,
    pub setuid: DefaultMode,
    pub setgid: DefaultMode,
    pub setcap: DefaultMode,
    pub allowed_bins: CompiledRuleSet<PathMatcher>,
}

/// A compiled RuleSet: lists of matchers + default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRuleSet<M> {
    pub default: DefaultMode,
    pub allow: Vec<M>,
    pub deny: Vec<M>,
}

impl<M> CompiledRuleSet<M> {
    fn decide<F: Fn(&M) -> bool>(&self, matches: F) -> Decision {
        // Explicit deny wins over allow.
        for d in &self.deny {
            if matches(d) {
                return Decision::Deny;
            }
        }
        for a in &self.allow {
            if matches(a) {
                return Decision::Allow;
            }
        }
        match self.default {
            DefaultMode::Allow => Decision::Allow,
            DefaultMode::Deny => Decision::Deny,
        }
    }
}

/// Path matcher.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathMatcher {
    raw: String,
    parts: Vec<String>, // normalized with '/' separator
    has_globstar: bool,
    // per-part flags
    // '*' is represented as literal "*"
}

impl PathMatcher {
    pub fn new(pattern: &str) -> Result<Self, PolicyError> {
        let raw = pattern.trim().to_string();
        if raw.is_empty() {
            return Err(PolicyError::InvalidRule("empty path pattern".into()));
        }
        let norm = normalize_path_like(&raw);
        let mut parts: Vec<String> = norm.split('/').filter(|p| !p.is_empty()).map(|s| s.to_string()).collect();
        // Allow root "/" as special case -> no parts.
        if norm == "/" {
            parts.clear();
        }
        let has_globstar = parts.iter().any(|p| p == "**");
        // Validate: "**" only as whole segment; "*" only as whole segment.
        for p in &parts {
            if p.contains("**") && p != "**" {
                return Err(PolicyError::InvalidRule(format!("invalid globstar segment: {p}")));
            }
            if p.contains('*') && p != "*" && p != "**" {
                return Err(PolicyError::InvalidRule(format!("wildcards must be whole segment: {p}")));
            }
        }
        Ok(Self {
            raw,
            parts,
            has_globstar,
        })
    }

    pub fn is_match(&self, path: &str) -> bool {
        let path = normalize_path_like(path);
        let target: Vec<&str> = if path == "/" {
            Vec::new()
        } else {
            path.split('/').filter(|p| !p.is_empty()).collect()
        };

        // Fast path: no glob
        if !self.has_globstar && !self.parts.iter().any(|p| p == "*") {
            return self.parts.len() == target.len()
                && self
                    .parts
                    .iter()
                    .zip(target.iter())
                    .all(|(a, b)| a.eq_ignore_ascii_case(b));
        }

        match_parts(&self.parts, &target)
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// String matcher for `StrPattern` using `*` wildcard.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StrMatcher {
    raw: String,
    // store lowercase for case-insensitive matching where it makes sense (env keys: case-sensitive on unix),
    // but we’ll default to exact-case match, and allow caller to normalize before checking if needed.
    // For hostnames, caller should normalize to lowercase.
    parts: Vec<String>, // split by '*'
    has_star: bool,
    anchored_start: bool,
    anchored_end: bool,
}

impl StrMatcher {
    pub fn new(pattern: &str) -> Result<Self, PolicyError> {
        let raw = pattern.trim().to_string();
        if raw.is_empty() {
            return Err(PolicyError::InvalidRule("empty string pattern".into()));
        }
        let has_star = raw.contains('*');
        let anchored_start = !raw.starts_with('*');
        let anchored_end = !raw.ends_with('*');

        let parts: Vec<String> = raw
            .split('*')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        Ok(Self {
            raw,
            parts,
            has_star,
            anchored_start,
            anchored_end,
        })
    }

    pub fn is_match(&self, s: &str) -> bool {
        if !self.has_star {
            return self.raw == s;
        }

        // Greedy left-to-right contains matching for segments.
        let mut idx = 0usize;
        let bytes = s.as_bytes();

        for (i, part) in self.parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            // Find next occurrence from idx.
            let found = find_subslice(bytes, part.as_bytes(), idx);
            let Some(pos) = found else {
                return false;
            };

            // start anchoring
            if i == 0 && self.anchored_start && pos != 0 {
                return false;
            }
            idx = pos + part.len();
        }

        if self.anchored_end {
            if let Some(last) = self.parts.last() {
                if !s.ends_with(last) {
                    return false;
                }
            }
        }

        true
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// Network matcher compiles host pattern + port + proto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetMatcher {
    host: StrMatcher,
    port: u16, // 0 => any
    proto: Option<String>, // lowercase, None => any
    raw: String,
}

impl NetMatcher {
    pub fn new(ep: &NetEndpoint) -> Result<Self, PolicyError> {
        let host = StrMatcher::new(&ep.host.0)?;
        let proto = ep.proto.as_ref().map(|p| p.trim().to_ascii_lowercase()).filter(|s| !s.is_empty());
        let raw = format!(
            "{}:{}{}",
            ep.host.0,
            ep.port,
            proto.as_ref().map(|p| format!("/{p}")).unwrap_or_default()
        );
        Ok(Self {
            host,
            port: ep.port,
            proto,
            raw,
        })
    }

    pub fn is_match(&self, host: &str, port: u16, proto: Option<&str>) -> bool {
        let h = host.to_ascii_lowercase();
        if !self.host.is_match(&h) {
            return false;
        }
        if self.port != 0 && self.port != port {
            return false;
        }
        if let Some(p) = &self.proto {
            let Some(pp) = proto else { return false };
            if p != &pp.to_ascii_lowercase() {
                return false;
            }
        }
        true
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

impl CapsulePolicy {
    /// Compile/normalize the policy for runtime checks.
    pub fn compile(&self) -> Result<CompiledPolicy, PolicyError> {
        Ok(CompiledPolicy {
            fs: CompiledFsPolicy {
                read: compile_path_rules(&self.fs.read)?,
                write: compile_path_rules(&self.fs.write)?,
                exec: compile_path_rules(&self.fs.exec)?,
                create: compile_path_rules(&self.fs.create)?,
                delete: compile_path_rules(&self.fs.delete)?,
                list: compile_path_rules(&self.fs.list)?,
                metadata: compile_path_rules(&self.fs.metadata)?,
            },
            env: CompiledEnvPolicy {
                read: compile_str_rules(&self.env.read)?,
                write: compile_str_rules(&self.env.write)?,
                unset: compile_str_rules(&self.env.unset)?,
            },
            net: CompiledNetPolicy {
                connect: compile_net_rules(&self.net.connect)?,
                bind: compile_net_rules(&self.net.bind)?,
                listen: compile_net_rules(&self.net.listen)?,
                resolve_dns: compile_str_rules(&self.net.resolve_dns)?,
            },
            time: self.time.clone(),
            proc: CompiledProcPolicy {
                spawn: self.proc.spawn,
                signal: self.proc.signal,
                setuid: self.proc.setuid,
                setgid: self.proc.setgid,
                setcap: self.proc.setcap,
                allowed_bins: compile_path_rules(&self.proc.allowed_bins)?,
            },
            limits: self.limits.clone(),
            meta: self.meta.clone(),
        })
    }

    /// Validate the policy for obvious issues (limits, rule formats).
    pub fn validate(&self) -> Result<(), PolicyError> {
        // Compile is a validation step.
        let _ = self.compile()?;

        validate_limits(&self.limits)?;

        Ok(())
    }
}

impl CompiledPolicy {
    pub fn check_fs(&self, op: FsOp, path: &str) -> Decision {
        match op {
            FsOp::Read => self.fs.read.decide(|m| m.is_match(path)),
            FsOp::Write => self.fs.write.decide(|m| m.is_match(path)),
            FsOp::Exec => self.fs.exec.decide(|m| m.is_match(path)),
            FsOp::Create => self.fs.create.decide(|m| m.is_match(path)),
            FsOp::Delete => self.fs.delete.decide(|m| m.is_match(path)),
            FsOp::List => self.fs.list.decide(|m| m.is_match(path)),
            FsOp::Metadata => self.fs.metadata.decide(|m| m.is_match(path)),
        }
    }

    pub fn check_env(&self, op: EnvOp, key: &str) -> Decision {
        match op {
            EnvOp::Read => self.env.read.decide(|m| m.is_match(key)),
            EnvOp::Write => self.env.write.decide(|m| m.is_match(key)),
            EnvOp::Unset => self.env.unset.decide(|m| m.is_match(key)),
        }
    }

    pub fn check_dns(&self, name: &str) -> Decision {
        // DNS names are case-insensitive -> normalize lowercase before match.
        let n = name.to_ascii_lowercase();
        self.net.resolve_dns.decide(|m| m.is_match(&n))
    }

    pub fn check_net(&self, op: NetOp, host: &str, port: u16, proto: Option<&str>) -> Decision {
        match op {
            NetOp::Connect => self.net.connect.decide(|m| m.is_match(host, port, proto)),
            NetOp::Bind => self.net.bind.decide(|m| m.is_match(host, port, proto)),
            NetOp::Listen => self.net.listen.decide(|m| m.is_match(host, port, proto)),
            NetOp::ResolveDns => self.check_dns(host),
        }
    }

    pub fn check_time(&self, op: TimeOp) -> Decision {
        let mode = match op {
            TimeOp::ReadClock => self.time.read_clock,
            TimeOp::SetClock => self.time.set_clock,
            TimeOp::Sleep => self.time.sleep,
        };
        match mode {
            DefaultMode::Allow => Decision::Allow,
            DefaultMode::Deny => Decision::Deny,
        }
    }

    pub fn check_proc(&self, op: ProcOp, bin: Option<&str>) -> Decision {
        match op {
            ProcOp::Spawn => {
                if self.proc.spawn == DefaultMode::Deny {
                    return Decision::Deny;
                }
                if let Some(b) = bin {
                    self.proc.allowed_bins.decide(|m| m.is_match(b))
                } else {
                    // No binary provided -> deny (caller should specify).
                    Decision::Deny
                }
            }
            ProcOp::Signal => mode_to_decision(self.proc.signal),
            ProcOp::SetUid => mode_to_decision(self.proc.setuid),
            ProcOp::SetGid => mode_to_decision(self.proc.setgid),
            ProcOp::SetCap => mode_to_decision(self.proc.setcap),
        }
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn meta(&self) -> &BTreeMap<String, String> {
        &self.meta
    }
}

fn mode_to_decision(m: DefaultMode) -> Decision {
    match m {
        DefaultMode::Allow => Decision::Allow,
        DefaultMode::Deny => Decision::Deny,
    }
}

fn compile_path_rules(r: &RuleSet<PathPattern>) -> Result<CompiledRuleSet<PathMatcher>, PolicyError> {
    let mut allow = Vec::with_capacity(r.allow.len());
    let mut deny = Vec::with_capacity(r.deny.len());
    for a in &r.allow {
        allow.push(PathMatcher::new(&a.0)?);
    }
    for d in &r.deny {
        deny.push(PathMatcher::new(&d.0)?);
    }
    Ok(CompiledRuleSet {
        default: r.default,
        allow,
        deny,
    })
}

fn compile_str_rules(r: &RuleSet<StrPattern>) -> Result<CompiledRuleSet<StrMatcher>, PolicyError> {
    let mut allow = Vec::with_capacity(r.allow.len());
    let mut deny = Vec::with_capacity(r.deny.len());
    for a in &r.allow {
        allow.push(StrMatcher::new(&a.0)?);
    }
    for d in &r.deny {
        deny.push(StrMatcher::new(&d.0)?);
    }
    Ok(CompiledRuleSet {
        default: r.default,
        allow,
        deny,
    })
}

fn compile_net_rules(r: &RuleSet<NetEndpoint>) -> Result<CompiledRuleSet<NetMatcher>, PolicyError> {
    let mut allow = Vec::with_capacity(r.allow.len());
    let mut deny = Vec::with_capacity(r.deny.len());
    for a in &r.allow {
        allow.push(NetMatcher::new(a)?);
    }
    for d in &r.deny {
        deny.push(NetMatcher::new(d)?);
    }
    Ok(CompiledRuleSet {
        default: r.default,
        allow,
        deny,
    })
}

/// Validate limits for sanity: non-zero, and some ordering constraints where applicable.
fn validate_limits(l: &Limits) -> Result<(), PolicyError> {
    fn nonzero(name: &str, v: Option<u64>) -> Result<(), PolicyError> {
        if let Some(x) = v {
            if x == 0 {
                return Err(PolicyError::InvalidLimit(format!("{name} must be > 0")));
            }
        }
        Ok(())
    }

    nonzero("wall_ms", l.wall_ms)?;
    nonzero("cpu_ms", l.cpu_ms)?;
    nonzero("memory_bytes", l.memory_bytes)?;
    nonzero("open_files", l.open_files)?;
    nonzero("processes", l.processes)?;
    nonzero("stdout_bytes", l.stdout_bytes)?;
    nonzero("stderr_bytes", l.stderr_bytes)?;
    nonzero("io_bytes", l.io_bytes)?;

    // Optional: cpu_ms <= wall_ms if both specified
    if let (Some(cpu), Some(wall)) = (l.cpu_ms, l.wall_ms) {
        if cpu > wall {
            return Err(PolicyError::InvalidLimit(format!(
                "cpu_ms ({cpu}) cannot exceed wall_ms ({wall})"
            )));
        }
    }

    Ok(())
}

/// Normalize Windows/Unix path separators into a consistent matcher format.
/// - Converts backslashes to forward slashes
/// - Collapses repeated slashes
/// - Trims whitespace
fn normalize_path_like(p: &str) -> String {
    let mut s = p.trim().replace('\\', "/");
    // Collapse '//' -> '/'
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    // Keep leading "//" not needed here; treat as "/"
    if s.starts_with("//") {
        while s.starts_with("//") {
            s = s.replacen("//", "/", 1);
        }
    }
    // Root special-case
    if s.is_empty() {
        "/".to_string()
    } else {
        s
    }
}

/// Match path segments with support for `*` and `**`.
fn match_parts(pattern: &[String], target: &[&str]) -> bool {
    fn rec(pat: &[String], tgt: &[&str]) -> bool {
        if pat.is_empty() {
            return tgt.is_empty();
        }
        if pat[0] == "**" {
            // ** matches zero or more segments
            if rec(&pat[1..], tgt) {
                return true;
            }
            for i in 0..tgt.len() {
                if rec(&pat[1..], &tgt[i + 1..]) {
                    return true;
                }
            }
            return false;
        }
        if tgt.is_empty() {
            return false;
        }
        if pat[0] == "*" {
            return rec(&pat[1..], &tgt[1..]);
        }
        if !pat[0].eq_ignore_ascii_case(tgt[0]) {
            return false;
        }
        rec(&pat[1..], &tgt[1..])
    }
    rec(pattern, target)
}

fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from);
    }
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/* ---------- Optional presets / helpers ---------- */

/// A few named presets for convenience.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyPreset {
    /// Strict sandbox: deny everything except minimal read of cwd.
    Strict,
    /// Typical build sandbox: allow reading sources, writing build outputs, no network.
    Build,
    /// Network client: allow outbound connect to allowlisted hosts, no bind/listen.
    NetClient,
    /// Developer mode: broad allow, intended for local use only.
    Dev,
}

impl CapsulePolicy {
    pub fn preset(p: PolicyPreset) -> Self {
        match p {
            PolicyPreset::Strict => {
                let mut pol = CapsulePolicy::default();
                // Allow read/list/metadata only in current directory (represented by "." patterns).
                pol.fs.read.allow.push(PathPattern("./**".into()));
                pol.fs.list.allow.push(PathPattern("./**".into()));
                pol.fs.metadata.allow.push(PathPattern("./**".into()));
                pol
            }
            PolicyPreset::Build => {
                let mut pol = CapsulePolicy::default();

                // Read sources anywhere under workspace; conservative default: "./**"
                pol.fs.read.allow.push(PathPattern("./**".into()));
                pol.fs.list.allow.push(PathPattern("./**".into()));
                pol.fs.metadata.allow.push(PathPattern("./**".into()));

                // Write only to ./out and ./build
                pol.fs.write.allow.push(PathPattern("./out/**".into()));
                pol.fs.write.allow.push(PathPattern("./build/**".into()));
                pol.fs.create.allow.push(PathPattern("./out/**".into()));
                pol.fs.create.allow.push(PathPattern("./build/**".into()));
                pol.fs.delete.allow.push(PathPattern("./out/**".into()));
                pol.fs.delete.allow.push(PathPattern("./build/**".into()));

                // Exec: allow toolchain under ./tools and system bins optionally
                pol.fs.exec.allow.push(PathPattern("./tools/**".into()));
                // Time: allow read clock + sleep, deny set
                pol.time.read_clock = DefaultMode::Allow;
                pol.time.sleep = DefaultMode::Allow;

                // Proc: allow spawn but restrict bins
                pol.proc.spawn = DefaultMode::Allow;
                pol.proc.allowed_bins.allow.push(PathPattern("./tools/**".into()));

                // Limits: reasonable defaults
                pol.limits.wall_ms = Some(10 * 60 * 1000);
                pol.limits.cpu_ms = Some(10 * 60 * 1000);
                pol.limits.memory_bytes = Some(2 * 1024 * 1024 * 1024);
                pol
            }
            PolicyPreset::NetClient => {
                let mut pol = CapsulePolicy::default();

                // Minimal fs
                pol.fs.read.allow.push(PathPattern("./**".into()));
                pol.fs.list.allow.push(PathPattern("./**".into()));
                pol.fs.metadata.allow.push(PathPattern("./**".into()));

                // Network: allow DNS + connect to allowlist
                pol.net.resolve_dns.default = DefaultMode::Allow;
                pol.net.connect.default = DefaultMode::Deny;
                pol.net.connect.allow.push(NetEndpoint {
                    host: StrPattern("*.example.com".into()),
                    port: 443,
                    proto: Some("tcp".into()),
                });

                pol.time.read_clock = DefaultMode::Allow;
                pol.time.sleep = DefaultMode::Allow;
                pol
            }
            PolicyPreset::Dev => {
                let mut pol = CapsulePolicy::default();

                // Broad allow
                pol.fs.read.default = DefaultMode::Allow;
                pol.fs.write.default = DefaultMode::Allow;
                pol.fs.exec.default = DefaultMode::Allow;
                pol.fs.create.default = DefaultMode::Allow;
                pol.fs.delete.default = DefaultMode::Allow;
                pol.fs.list.default = DefaultMode::Allow;
                pol.fs.metadata.default = DefaultMode::Allow;

                pol.env.read.default = DefaultMode::Allow;
                pol.env.write.default = DefaultMode::Allow;
                pol.env.unset.default = DefaultMode::Allow;

                pol.net.resolve_dns.default = DefaultMode::Allow;
                pol.net.connect.default = DefaultMode::Allow;
                pol.net.bind.default = DefaultMode::Allow;
                pol.net.listen.default = DefaultMode::Allow;

                pol.time.read_clock = DefaultMode::Allow;
                pol.time.set_clock = DefaultMode::Allow;
                pol.time.sleep = DefaultMode::Allow;

                pol.proc.spawn = DefaultMode::Allow;
                pol.proc.signal = DefaultMode::Allow;

                pol
            }
        }
    }
}

/* ---------- Optional: audit / introspection ---------- */

/// A small “capabilities” summary to help diagnostics/logging.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PolicySummary {
    pub fs_allowed_ops: BTreeSet<FsOp>,
    pub env_allowed_ops: BTreeSet<EnvOp>,
    pub net_allowed_ops: BTreeSet<NetOp>,
    pub time_allowed_ops: BTreeSet<TimeOp>,
    pub proc_allowed_ops: BTreeSet<ProcOp>,
}

impl CompiledPolicy {
    pub fn summarize(&self) -> PolicySummary {
        let mut s = PolicySummary::default();

        // FS: consider op allowed if default allow OR any allow rule exists (coarse).
        macro_rules! fs_op {
            ($field:ident, $op:expr) => {{
                if self.fs.$field.default == DefaultMode::Allow || !self.fs.$field.allow.is_empty() {
                    s.fs_allowed_ops.insert($op);
                }
            }};
        }
        fs_op!(read, FsOp::Read);
        fs_op!(write, FsOp::Write);
        fs_op!(exec, FsOp::Exec);
        fs_op!(create, FsOp::Create);
        fs_op!(delete, FsOp::Delete);
        fs_op!(list, FsOp::List);
        fs_op!(metadata, FsOp::Metadata);

        macro_rules! env_op {
            ($field:ident, $op:expr) => {{
                if self.env.$field.default == DefaultMode::Allow || !self.env.$field.allow.is_empty() {
                    s.env_allowed_ops.insert($op);
                }
            }};
        }
        env_op!(read, EnvOp::Read);
        env_op!(write, EnvOp::Write);
        env_op!(unset, EnvOp::Unset);

        macro_rules! net_op {
            ($field:ident, $op:expr) => {{
                if self.net.$field.default == DefaultMode::Allow || !self.net.$field.allow.is_empty() {
                    s.net_allowed_ops.insert($op);
                }
            }};
        }
        net_op!(connect, NetOp::Connect);
        net_op!(bind, NetOp::Bind);
        net_op!(listen, NetOp::Listen);
        if self.net.resolve_dns.default == DefaultMode::Allow || !self.net.resolve_dns.allow.is_empty() {
            s.net_allowed_ops.insert(NetOp::ResolveDns);
        }

        if self.time.read_clock == DefaultMode::Allow {
            s.time_allowed_ops.insert(TimeOp::ReadClock);
        }
        if self.time.set_clock == DefaultMode::Allow {
            s.time_allowed_ops.insert(TimeOp::SetClock);
        }
        if self.time.sleep == DefaultMode::Allow {
            s.time_allowed_ops.insert(TimeOp::Sleep);
        }

        if self.proc.spawn == DefaultMode::Allow {
            s.proc_allowed_ops.insert(ProcOp::Spawn);
        }
        if self.proc.signal == DefaultMode::Allow {
            s.proc_allowed_ops.insert(ProcOp::Signal);
        }
        if self.proc.setuid == DefaultMode::Allow {
            s.proc_allowed_ops.insert(ProcOp::SetUid);
        }
        if self.proc.setgid == DefaultMode::Allow {
            s.proc_allowed_ops.insert(ProcOp::SetGid);
        }
        if self.proc.setcap == DefaultMode::Allow {
            s.proc_allowed_ops.insert(ProcOp::SetCap);
        }

        s
    }
}

/* ---------- Tests ---------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_match_exact() {
        let m = PathMatcher::new("/a/b/c").unwrap();
        assert!(m.is_match("/a/b/c"));
        assert!(!m.is_match("/a/b"));
        assert!(!m.is_match("/a/b/c/d"));
    }

    #[test]
    fn path_match_star_segment() {
        let m = PathMatcher::new("/a/*/c").unwrap();
        assert!(m.is_match("/a/b/c"));
        assert!(m.is_match("/a/xxx/c"));
        assert!(!m.is_match("/a/b/d"));
        assert!(!m.is_match("/a/b/c/d"));
    }

    #[test]
    fn path_match_globstar() {
        let m = PathMatcher::new("/a/**").unwrap();
        assert!(m.is_match("/a"));
        assert!(m.is_match("/a/"));
        assert!(m.is_match("/a/b"));
        assert!(m.is_match("/a/b/c/d"));
        assert!(!m.is_match("/x/a/b"));
    }

    #[test]
    fn str_match_simple() {
        let m = StrMatcher::new("ABC").unwrap();
        assert!(m.is_match("ABC"));
        assert!(!m.is_match("AB"));
        assert!(!m.is_match("ABCD"));
    }

    #[test]
    fn str_match_wildcard() {
        let m = StrMatcher::new("A*Z").unwrap();
        assert!(m.is_match("AZ"));
        assert!(m.is_match("A___Z"));
        assert!(!m.is_match("BAZ"));
        assert!(!m.is_match("AZZ"));
    }

    #[test]
    fn ruleset_deny_overrides_allow() {
        let r = RuleSet {
            default: DefaultMode::Deny,
            allow: vec![PathPattern("/a/**".into())],
            deny: vec![PathPattern("/a/b/**".into())],
        };
        let c = compile_path_rules(&r).unwrap();
        assert_eq!(c.decide(|m| m.is_match("/a/x")), Decision::Allow);
        assert_eq!(c.decide(|m| m.is_match("/a/b/x")), Decision::Deny);
    }

    #[test]
    fn compiled_policy_fs_check() {
        let mut pol = CapsulePolicy::default();
        pol.fs.read.allow.push(PathPattern("/a/**".into()));
        let c = pol.compile().unwrap();

        assert_eq!(c.check_fs(FsOp::Read, "/a/b/c"), Decision::Allow);
        assert_eq!(c.check_fs(FsOp::Write, "/a/b/c"), Decision::Deny);
    }

    #[test]
    fn net_match_host_port_proto() {
        let ep = NetEndpoint {
            host: StrPattern("*.example.com".into()),
            port: 443,
            proto: Some("tcp".into()),
        };
        let m = NetMatcher::new(&ep).unwrap();
        assert!(m.is_match("api.example.com", 443, Some("tcp")));
        assert!(!m.is_match("api.example.com", 80, Some("tcp")));
        assert!(!m.is_match("api.example.com", 443, Some("udp")));
        assert!(!m.is_match("example.org", 443, Some("tcp")));
    }

    #[test]
    fn limits_validate() {
        let mut l = Limits::default();
        l.wall_ms = Some(10);
        l.cpu_ms = Some(20);
        assert!(validate_limits(&l).is_err());

        l.cpu_ms = Some(10);
        assert!(validate_limits(&l).is_ok());
    }
}
