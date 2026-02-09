// src/rule.rs
//
// Steel — build rules (planning primitives)
//
// Purpose:
// - Model a "Rule" (like Makefile rule) in a modern, typed way.
// - Provide deterministic rule hashing, validation, and "up-to-date" checks.
// - Represent inputs/outputs, tools/commands, env, working dir, and declared deps.
// - Support:
//   - file inputs/outputs
//   - phony rules
//   - implicit inputs (env vars, config vars)
//   - command lines (argv) and tool invocations
//   - caching / fingerprinting (content hash or mtime-based)
//   - rule graph edges (depends_on)
//
// This is a "max" reference layer. Execution engine can consume Rule + fingerprints.
//
// Notes:
// - Avoids heavy deps. Hashing uses std + a small FNV-1a fallback.
// - Path handling is conservative: store as PathBuf but canonicalization is delegated to higher layer.
// - "Up-to-date" check here can be mtime-based, or fully fingerprint-based if you provide digests.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/* ============================== model ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: RuleId,
    pub name: String,

    pub kind: RuleKind,
    pub phony: bool,

    pub inputs: Vec<Artifact>,
    pub outputs: Vec<Artifact>,

    pub deps: Vec<RuleId>, // edges in rule graph

    pub command: CommandSpec,

    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,

    pub cache: CachePolicy,
    pub tags: BTreeSet<String>,

    pub meta: BTreeMap<String, String>,
}

impl Rule {
    pub fn new<S: Into<String>>(name: S) -> Self {
        let name = name.into();
        let id = RuleId::from_name(&name);
        Self {
            id,
            name,
            kind: RuleKind::Build,
            phony: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
            deps: Vec::new(),
            command: CommandSpec::empty(),
            env: BTreeMap::new(),
            cwd: None,
            cache: CachePolicy::default(),
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn add_input_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.inputs.push(Artifact::Path(path.into()));
    }

    pub fn add_output_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.outputs.push(Artifact::Path(path.into()));
    }

    pub fn add_dep(&mut self, id: RuleId) {
        if !self.deps.contains(&id) {
            self.deps.push(id);
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.phony || !self.outputs.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleKind {
    Build,
    Test,
    Tool,
    Clean,
    Install,
    Publish,
    Custom,
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RuleKind::Build => "build",
            RuleKind::Test => "test",
            RuleKind::Tool => "tool",
            RuleKind::Clean => "clean",
            RuleKind::Install => "install",
            RuleKind::Publish => "publish",
            RuleKind::Custom => "custom",
        };
        f.write_str(s)
    }
}

/// Stable rule identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(u64);

impl RuleId {
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn from_name(name: &str) -> Self {
        // stable hash of name (FNV-1a)
        let mut h = Fnv1aHasher::default();
        h.write(name.as_bytes());
        RuleId::new(h.finish())
    }
}

impl fmt::Debug for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuleId(0x{:016x})", self.0)
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    Path(PathBuf),
    /// Named artifact (virtual), e.g. "std:module_cache", "registry:index".
    Named(String),
    /// Content-only artifact (string payload), used for env/config fingerprints.
    Value(String),
}

impl Artifact {
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Artifact::Path(p) => Some(p.as_path()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub display: Option<String>,
}

impl CommandSpec {
    pub fn empty() -> Self {
        Self {
            program: String::new(),
            args: Vec::new(),
            display: None,
        }
    }

    pub fn new<S: Into<String>>(program: S) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            display: None,
        }
    }

    pub fn arg<S: Into<String>>(mut self, s: S) -> Self {
        self.args.push(s.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(it.into_iter().map(|x| x.into()));
        self
    }

    pub fn display<S: Into<String>>(mut self, s: S) -> Self {
        self.display = Some(s.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.program.is_empty()
    }

    pub fn to_argv(&self) -> Vec<String> {
        let mut v = Vec::with_capacity(1 + self.args.len());
        v.push(self.program.clone());
        v.extend(self.args.clone());
        v
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePolicy {
    pub enabled: bool,
    pub mode: CacheMode,
    pub key_salt: Option<String>,
    pub max_age_secs: Option<u64>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: CacheMode::Fingerprint,
            key_salt: None,
            max_age_secs: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheMode {
    /// Cheap: mtime-based (outputs newer than inputs).
    Mtime,
    /// Stronger: fingerprint-based (hash inputs + command + env + metadata).
    Fingerprint,
}

/* ============================== validation ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    EmptyName,
    EmptyCommand,
    NoOutputsForNonPhony,
    DuplicateOutput(PathBuf),
    InvalidEnvKey(String),
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::EmptyName => write!(f, "rule name is empty"),
            RuleError::EmptyCommand => write!(f, "rule command is empty"),
            RuleError::NoOutputsForNonPhony => write!(f, "non-phony rule must declare outputs"),
            RuleError::DuplicateOutput(p) => write!(f, "duplicate output: {}", p.display()),
            RuleError::InvalidEnvKey(k) => write!(f, "invalid env key: {k}"),
        }
    }
}

impl std::error::Error for RuleError {}

pub fn validate_rule(rule: &Rule) -> Result<(), RuleError> {
    if rule.name.trim().is_empty() {
        return Err(RuleError::EmptyName);
    }
    if rule.command.is_empty() {
        return Err(RuleError::EmptyCommand);
    }
    if !rule.phony && rule.outputs.is_empty() {
        return Err(RuleError::NoOutputsForNonPhony);
    }

    let mut seen = BTreeSet::<PathBuf>::new();
    for o in &rule.outputs {
        if let Artifact::Path(p) = o {
            if !seen.insert(p.clone()) {
                return Err(RuleError::DuplicateOutput(p.clone()));
            }
        }
    }

    for k in rule.env.keys() {
        if !is_env_key(k) {
            return Err(RuleError::InvalidEnvKey(k.clone()));
        }
    }

    Ok(())
}

fn is_env_key(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/* ============================== up-to-date checks ============================== */

#[derive(Debug, Clone)]
pub struct MtimeSnapshot {
    pub inputs: Vec<(PathBuf, Option<SystemTime>)>,
    pub outputs: Vec<(PathBuf, Option<SystemTime>)>,
}

pub fn snapshot_mtimes(rule: &Rule) -> MtimeSnapshot {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();

    for a in &rule.inputs {
        if let Artifact::Path(p) = a {
            inputs.push((p.clone(), file_mtime(p)));
        }
    }
    for a in &rule.outputs {
        if let Artifact::Path(p) = a {
            outputs.push((p.clone(), file_mtime(p)));
        }
    }

    MtimeSnapshot { inputs, outputs }
}

/// Mtime-based up-to-date check:
/// - If any output missing => dirty
/// - If any input newer than oldest output => dirty
/// - Else clean
pub fn is_uptodate_mtime(rule: &Rule) -> bool {
    if rule.phony {
        return false;
    }
    let snap = snapshot_mtimes(rule);

    let mut oldest_output: Option<SystemTime> = None;
    for (_, mt) in &snap.outputs {
        let Some(t) = mt else { return false }; // output missing
        oldest_output = Some(match oldest_output {
            Some(old) => if *t < old { *t } else { old },
            None => *t,
        });
    }

    let Some(oldest) = oldest_output else { return false };

    for (_, mt) in &snap.inputs {
        if let Some(t) = mt {
            if *t > oldest {
                return false;
            }
        } else {
            // missing input -> treat as dirty (can't guarantee correctness)
            return false;
        }
    }

    true
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/* ============================== fingerprinting ============================== */

/// A computed fingerprint (u64) for a rule, for caching / incremental builds.
/// You can replace with a stronger hash (blake3) if you allow deps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub u64);

pub fn fingerprint_rule(rule: &Rule) -> Fingerprint {
    let mut h = Fnv1aHasher::default();

    // identity
    h.write(rule.name.as_bytes());
    h.write_u64(rule.id.raw());

    // kind/phony/cache
    h.write_u8(rule.kind as u8);
    h.write_u8(rule.phony as u8);
    h.write_u8(rule.cache.enabled as u8);
    h.write_u8(rule.cache.mode as u8);

    if let Some(s) = &rule.cache.key_salt {
        h.write(s.as_bytes());
    }
    if let Some(age) = rule.cache.max_age_secs {
        h.write_u64(age);
    }

    // command
    h.write(rule.command.program.as_bytes());
    for a in &rule.command.args {
        h.write(a.as_bytes());
    }

    // env (sorted by BTreeMap order)
    for (k, v) in &rule.env {
        h.write(k.as_bytes());
        h.write(v.as_bytes());
    }

    // cwd
    if let Some(cwd) = &rule.cwd {
        h.write(cwd.to_string_lossy().as_bytes());
    }

    // artifacts
    for a in &rule.inputs {
        hash_artifact(&mut h, a);
    }
    for a in &rule.outputs {
        hash_artifact(&mut h, a);
    }

    // deps
    for d in &rule.deps {
        h.write_u64(d.raw());
    }

    // tags/meta
    for t in &rule.tags {
        h.write(t.as_bytes());
    }
    for (k, v) in &rule.meta {
        h.write(k.as_bytes());
        h.write(v.as_bytes());
    }

    Fingerprint(h.finish())
}

fn hash_artifact(h: &mut Fnv1aHasher, a: &Artifact) {
    match a {
        Artifact::Path(p) => {
            h.write(b"P:");
            h.write(p.to_string_lossy().as_bytes());
        }
        Artifact::Named(s) => {
            h.write(b"N:");
            h.write(s.as_bytes());
        }
        Artifact::Value(s) => {
            h.write(b"V:");
            h.write(s.as_bytes());
        }
    }
}

/* ============================== hashing impl ============================== */

#[derive(Default)]
struct Fnv1aHasher {
    state: u64,
}

impl Hasher for Fnv1aHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.state == 0 { 0xcbf29ce484222325 } else { self.state };
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.state = hash;
    }

    fn finish(&self) -> u64 {
        if self.state == 0 {
            0xcbf29ce484222325
        } else {
            self.state
        }
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_id_stable() {
        let a = RuleId::from_name("hello");
        let b = RuleId::from_name("hello");
        let c = RuleId::from_name("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn validate_rule_basic() {
        let mut r = Rule::new("compile");
        r.command = CommandSpec::new("cc").arg("-c").arg("a.c");
        r.add_output_path("a.o");
        validate_rule(&r).unwrap();
    }

    #[test]
    fn validate_requires_output_when_non_phony() {
        let mut r = Rule::new("x");
        r.command = CommandSpec::new("tool");
        let err = validate_rule(&r).unwrap_err();
        assert!(matches!(err, RuleError::NoOutputsForNonPhony));
    }

    #[test]
    fn fingerprint_changes_with_command() {
        let mut r = Rule::new("compile");
        r.command = CommandSpec::new("cc").arg("-c").arg("a.c");
        r.add_output_path("a.o");

        let f1 = fingerprint_rule(&r);

        r.command = CommandSpec::new("cc").arg("-c").arg("b.c");
        let f2 = fingerprint_rule(&r);

        assert_ne!(f1, f2);
    }

    #[test]
    fn uptodate_phony_false() {
        let mut r = Rule::new("phony");
        r.phony = true;
        r.command = CommandSpec::new("echo").arg("hi");
        assert_eq!(is_uptodate_mtime(&r), false);
    }
}
