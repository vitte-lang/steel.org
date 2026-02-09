// src/remake.rs
//
// Steel — remake / incremental rebuild engine (planner-side)
//
// Purpose:
// - Decide what needs to be rebuilt, given a set of rules, their dependencies, and a cache DB.
// - Provide deterministic "dirty/clean" classification.
// - Support:
//   - mtime-based checking (cheap, legacy-style)
//   - fingerprint-based checking (robust, reproducible)
//   - tracking outputs to rules (producer map)
//   - cycle detection in rule graph
//   - topological ordering for execution
//   - "why dirty" explanations for diagnostics
//
// This module is intentionally self-contained and dependency-free.
// It does not execute commands. It only plans what should run.
//
// Integration points:
// - `Rule` (from src/rule.rs) is re-defined here minimally to avoid cross-file coupling.
//   In your repo, replace with `use crate::rule::{Rule, RuleId, Artifact, CacheMode, ...};`
// - `CacheDb` can be backed by a file (json, sqlite, etc.). This is an in-memory stub.
//
// Notes:
// - Fingerprints here are u64 (FNV-1a). Replace with BLAKE3 if allowed.
// - Path canonicalization is intentionally not done to avoid IO surprises.
// - For Windows, mtime semantics can be coarse; fingerprint mode recommended.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/* ============================== minimal rule types ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheMode {
    Mtime,
    Fingerprint,
}

#[derive(Debug, Clone)]
pub struct CachePolicy {
    pub enabled: bool,
    pub mode: CacheMode,
    pub key_salt: Option<String>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: CacheMode::Fingerprint,
            key_salt: None,
        }
    }
}

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
        let mut h = Fnv1aHasher::default();
        h.write(name.as_bytes());
        RuleId(h.finish())
    }
}

impl fmt::Debug for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuleId(0x{:016x})", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    Path(PathBuf),
    Named(String),
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
}

impl CommandSpec {
    pub fn new<S: Into<String>>(program: S) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: RuleId,
    pub name: String,
    pub phony: bool,

    pub inputs: Vec<Artifact>,
    pub outputs: Vec<Artifact>,
    pub deps: Vec<RuleId>,

    pub command: CommandSpec,
    pub env: BTreeMap<String, String>,

    pub cache: CachePolicy,
}

impl Rule {
    pub fn new<S: Into<String>>(name: S) -> Self {
        let name = name.into();
        let id = RuleId::from_name(&name);
        Self {
            id,
            name,
            phony: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
            deps: Vec::new(),
            command: CommandSpec::new(""),
            env: BTreeMap::new(),
            cache: CachePolicy::default(),
        }
    }
}

/* ============================== cache DB ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub u64);

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub rule_fp: Fingerprint, // fingerprint of rule definition (command/env/declared inputs etc.)
    pub io_fp: Fingerprint,   // fingerprint of IO state at last successful run
    pub last_ok: SystemTime,
}

#[derive(Debug, Default, Clone)]
pub struct CacheDb {
    // keyed by rule id
    map: HashMap<RuleId, CacheEntry>,
}

impl CacheDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: RuleId) -> Option<&CacheEntry> {
        self.map.get(&id)
    }

    pub fn put(&mut self, id: RuleId, entry: CacheEntry) {
        self.map.insert(id, entry);
    }

    pub fn remove(&mut self, id: RuleId) {
        self.map.remove(&id);
    }
}

/* ============================== remake result ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleState {
    Clean,
    Dirty,
}

#[derive(Debug, Clone)]
pub struct RemakePlan {
    pub order: Vec<RuleId>,                 // topological order of rules to run (subset dirty)
    pub states: HashMap<RuleId, RuleState>, // all rules
    pub reasons: HashMap<RuleId, DirtyReason>,
    pub cycles: Vec<Vec<RuleId>>,
}

impl RemakePlan {
    pub fn is_dirty(&self, id: RuleId) -> bool {
        matches!(self.states.get(&id), Some(RuleState::Dirty))
    }
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
    ProducedByUnknownRule(PathBuf),
    FingerprintMismatch,
    CycleMember,
    Unknown,
}

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemakeError {
    DuplicateRuleName(String),
    DuplicateRuleId(u64),
    DuplicateOutput(PathBuf),
    UnknownDep { rule: String, dep: RuleId },
    GraphCycle(Vec<RuleId>),
}

impl fmt::Display for RemakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemakeError::DuplicateRuleName(s) => write!(f, "duplicate rule name: {s}"),
            RemakeError::DuplicateRuleId(x) => write!(f, "duplicate rule id: {x:#x}"),
            RemakeError::DuplicateOutput(p) => write!(f, "duplicate output producer: {}", p.display()),
            RemakeError::UnknownDep { rule, dep } => write!(f, "rule '{rule}' depends on unknown id {dep:?}"),
            RemakeError::GraphCycle(c) => write!(f, "cycle in rule graph: {:?}", c),
        }
    }
}

impl std::error::Error for RemakeError {}

/* ============================== engine ============================== */

#[derive(Debug, Clone)]
pub struct RemakeOptions {
    pub default_cache_mode: CacheMode,
    pub treat_missing_input_as_dirty: bool,
    pub treat_unknown_output_producer_as_dirty: bool,
    pub explain: bool,
}

impl Default for RemakeOptions {
    fn default() -> Self {
        Self {
            default_cache_mode: CacheMode::Fingerprint,
            treat_missing_input_as_dirty: true,
            treat_unknown_output_producer_as_dirty: true,
            explain: true,
        }
    }
}

/// Main entry:
/// - Validates rules
/// - Builds graph
/// - Computes dirty states
/// - Produces execution order for dirty rules
pub fn plan_remake(rules: &[Rule], db: &CacheDb, opts: &RemakeOptions) -> Result<RemakePlan, RemakeError> {
    validate_rules(rules)?;

    let by_id: HashMap<RuleId, &Rule> = rules.iter().map(|r| (r.id, r)).collect();

    // Producer map: output path -> rule id
    let mut producer: HashMap<PathBuf, RuleId> = HashMap::new();
    for r in rules {
        for o in &r.outputs {
            if let Artifact::Path(p) = o {
                if producer.insert(p.clone(), r.id).is_some() {
                    return Err(RemakeError::DuplicateOutput(p.clone()));
                }
            }
        }
    }

    // adjacency: dep -> user (for topo)
    let mut indeg: HashMap<RuleId, usize> = HashMap::new();
    let mut adj: HashMap<RuleId, Vec<RuleId>> = HashMap::new();

    for r in rules {
        indeg.entry(r.id).or_insert(0);
        adj.entry(r.id).or_insert_with(Vec::new);

        for d in &r.deps {
            if !by_id.contains_key(d) {
                return Err(RemakeError::UnknownDep {
                    rule: r.name.clone(),
                    dep: *d,
                });
            }
            adj.entry(*d).or_insert_with(Vec::new).push(r.id);
            *indeg.entry(r.id).or_insert(0) += 1;
        }
    }

    let (topo_all, cycles) = topo_with_cycles(&indeg, &adj);
    // We'll still produce a plan, but mark cycles as dirty.
    // If you want hard failure, return Err(GraphCycle(...)) when cycles non-empty.

    // First pass: compute per-rule definition fingerprint
    let mut rule_def_fp: HashMap<RuleId, Fingerprint> = HashMap::new();
    for r in rules {
        rule_def_fp.insert(r.id, fingerprint_rule_definition(r));
    }

    // Second pass: compute dirty/clean using topo order, propagating dep dirtiness
    let mut states: HashMap<RuleId, RuleState> = HashMap::new();
    let mut reasons: HashMap<RuleId, DirtyReason> = HashMap::new();

    // Pre-mark cycle members
    let mut cycle_members = HashSet::<RuleId>::new();
    for c in &cycles {
        for id in c {
            cycle_members.insert(*id);
        }
    }

    for id in &topo_all {
        let r = by_id[id];

        if cycle_members.contains(id) {
            states.insert(*id, RuleState::Dirty);
            reasons.insert(*id, DirtyReason::CycleMember);
            continue;
        }

        // if any dependency dirty -> dirty
        let mut dep_dirty: Option<RuleId> = None;
        for d in &r.deps {
            if matches!(states.get(d), Some(RuleState::Dirty)) {
                dep_dirty = Some(*d);
                break;
            }
        }
        if let Some(d) = dep_dirty {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::DependencyDirty(d));
            }
            continue;
        }

        // phony always dirty
        if r.phony {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::Phony);
            }
            continue;
        }

        // choose mode
        let mode = if r.cache.enabled { r.cache.mode } else { CacheMode::Mtime };
        let mode = match mode {
            CacheMode::Mtime => CacheMode::Mtime,
            CacheMode::Fingerprint => CacheMode::Fingerprint,
        };

        // Rule must have outputs for non-phony; if none, consider dirty.
        if r.outputs.is_empty() {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::MissingOutput(PathBuf::from("<no outputs>")));
            }
            continue;
        }

        // Validate outputs exist
        if let Some(p) = first_missing_output(r) {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::MissingOutput(p));
            }
            continue;
        }

        // Validate inputs exist (for path inputs)
        if opts.treat_missing_input_as_dirty {
            if let Some(p) = first_missing_input(r) {
                states.insert(*id, RuleState::Dirty);
                if opts.explain {
                    reasons.insert(*id, DirtyReason::InputMissing(p));
                }
                continue;
            }
        }

        // Check mtime mode
        if mode == CacheMode::Mtime {
            if let Some((inp, outp)) = mtime_newer_input_than_output(r) {
                states.insert(*id, RuleState::Dirty);
                if opts.explain {
                    reasons.insert(*id, DirtyReason::InputNewerThanOutput { input: inp, output: outp });
                }
                continue;
            }
            states.insert(*id, RuleState::Clean);
            continue;
        }

        // Fingerprint mode:
        // - Need cache entry
        let Some(entry) = db.get(*id) else {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::NoCacheEntry);
            }
            continue;
        };

        // - Rule definition changed?
        let def_fp = rule_def_fp[id];
        if entry.rule_fp != def_fp {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::RuleDefinitionChanged);
            }
            continue;
        }

        // - Compute IO fingerprint from current state
        let io_fp_now = fingerprint_io_state(r, &producer, opts);
        if entry.io_fp != io_fp_now {
            states.insert(*id, RuleState::Dirty);
            if opts.explain {
                reasons.insert(*id, DirtyReason::FingerprintMismatch);
            }
            continue;
        }

        states.insert(*id, RuleState::Clean);
    }

    // Execution order: take topo_all and keep only dirty
    let mut order = Vec::new();
    for id in &topo_all {
        if matches!(states.get(id), Some(RuleState::Dirty)) {
            order.push(*id);
        }
    }

    Ok(RemakePlan {
        order,
        states,
        reasons,
        cycles,
    })
}

/* ============================== helpers: validation ============================== */

fn validate_rules(rules: &[Rule]) -> Result<(), RemakeError> {
    let mut names = BTreeSet::<String>::new();
    let mut ids = BTreeSet::<u64>::new();

    for r in rules {
        if !names.insert(r.name.clone()) {
            return Err(RemakeError::DuplicateRuleName(r.name.clone()));
        }
        if !ids.insert(r.id.raw()) {
            return Err(RemakeError::DuplicateRuleId(r.id.raw()));
        }
    }
    Ok(())
}

/* ============================== helpers: topo + cycles ============================== */

fn topo_with_cycles(
    indeg: &HashMap<RuleId, usize>,
    adj: &HashMap<RuleId, Vec<RuleId>>,
) -> (Vec<RuleId>, Vec<Vec<RuleId>>) {
    let mut indeg = indeg.clone();
    let mut q = VecDeque::<RuleId>::new();
    for (&id, &d) in &indeg {
        if d == 0 {
            q.push_back(id);
        }
    }

    let mut out = Vec::<RuleId>::new();

    while let Some(id) = q.pop_front() {
        out.push(id);
        if let Some(nexts) = adj.get(&id) {
            for &n in nexts {
                let e = indeg.get_mut(&n).unwrap();
                *e -= 1;
                if *e == 0 {
                    q.push_back(n);
                }
            }
        }
    }

    if out.len() == indeg.len() {
        return (out, vec![]);
    }

    // Remaining nodes are in cycles or depend on cycles.
    // Extract SCC-ish cycles via DFS on remaining set (simple, best-effort).
    let remaining: HashSet<RuleId> = indeg.iter().filter(|(_, d)| **d > 0).map(|(k, _)| *k).collect();
    let cycles = find_cycles(&remaining, adj);

    // Append remaining nodes in stable order to keep plan deterministic.
    let mut rem_sorted: Vec<RuleId> = remaining.into_iter().collect();
    rem_sorted.sort_by_key(|id| id.raw());
    out.extend(rem_sorted);

    (out, cycles)
}

fn find_cycles(remaining: &HashSet<RuleId>, adj: &HashMap<RuleId, Vec<RuleId>>) -> Vec<Vec<RuleId>> {
    let mut cycles = Vec::new();
    let mut visited = HashSet::<RuleId>::new();
    let mut stack = Vec::<RuleId>::new();
    let mut on_stack = HashSet::<RuleId>::new();

    for &start in remaining {
        if visited.contains(&start) {
            continue;
        }
        dfs_cycle(start, remaining, adj, &mut visited, &mut stack, &mut on_stack, &mut cycles);
    }

    // Deduplicate cycles by normalized key
    let mut uniq = BTreeMap::<String, Vec<RuleId>>::new();
    for c in cycles {
        let key = normalize_cycle_key(&c);
        uniq.entry(key).or_insert(c);
    }
    uniq.into_values().collect()
}

fn dfs_cycle(
    v: RuleId,
    remaining: &HashSet<RuleId>,
    adj: &HashMap<RuleId, Vec<RuleId>>,
    visited: &mut HashSet<RuleId>,
    stack: &mut Vec<RuleId>,
    on_stack: &mut HashSet<RuleId>,
    cycles: &mut Vec<Vec<RuleId>>,
) {
    visited.insert(v);
    stack.push(v);
    on_stack.insert(v);

    if let Some(nexts) = adj.get(&v) {
        for &n in nexts {
            if !remaining.contains(&n) {
                continue;
            }
            if !visited.contains(&n) {
                dfs_cycle(n, remaining, adj, visited, stack, on_stack, cycles);
            } else if on_stack.contains(&n) {
                // found cycle: extract stack suffix starting at n
                if let Some(pos) = stack.iter().position(|x| *x == n) {
                    let cyc = stack[pos..].to_vec();
                    cycles.push(cyc);
                }
            }
        }
    }

    on_stack.remove(&v);
    stack.pop();
}

fn normalize_cycle_key(c: &[RuleId]) -> String {
    // rotate cycle so minimal raw id first, then join
    if c.is_empty() {
        return String::new();
    }
    let mut min_i = 0usize;
    for i in 1..c.len() {
        if c[i].raw() < c[min_i].raw() {
            min_i = i;
        }
    }
    let mut s = String::new();
    for k in 0..c.len() {
        let id = c[(min_i + k) % c.len()];
        s.push_str(&format!("{:016x}-", id.raw()));
    }
    s
}

/* ============================== helpers: IO checks ============================== */

fn first_missing_output(r: &Rule) -> Option<PathBuf> {
    for a in &r.outputs {
        if let Artifact::Path(p) = a {
            if !p.exists() {
                return Some(p.clone());
            }
        }
    }
    None
}

fn first_missing_input(r: &Rule) -> Option<PathBuf> {
    for a in &r.inputs {
        if let Artifact::Path(p) = a {
            if !p.exists() {
                return Some(p.clone());
            }
        }
    }
    None
}

fn mtime_newer_input_than_output(r: &Rule) -> Option<(PathBuf, PathBuf)> {
    let mut oldest_output: Option<(PathBuf, SystemTime)> = None;
    for a in &r.outputs {
        if let Artifact::Path(p) = a {
            let mt = file_mtime(p)?;
            oldest_output = Some(match oldest_output {
                Some((op, ot)) => {
                    if mt < ot {
                        (p.clone(), mt)
                    } else {
                        (op, ot)
                    }
                }
                None => (p.clone(), mt),
            });
        }
    }
    let (out_path, out_time) = oldest_output?;

    for a in &r.inputs {
        if let Artifact::Path(p) = a {
            let it = file_mtime(p)?;
            if it > out_time {
                return Some((p.clone(), out_path));
            }
        }
    }

    None
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/* ============================== fingerprinting ============================== */

fn fingerprint_rule_definition(r: &Rule) -> Fingerprint {
    let mut h = Fnv1aHasher::default();
    h.write(r.name.as_bytes());
    h.write_u64(r.id.raw());
    h.write_u8(r.phony as u8);

    h.write_u8(r.cache.enabled as u8);
    h.write_u8(match r.cache.mode {
        CacheMode::Mtime => 1,
        CacheMode::Fingerprint => 2,
    });

    if let Some(s) = &r.cache.key_salt {
        h.write(s.as_bytes());
    }

    h.write(r.command.program.as_bytes());
    for a in &r.command.args {
        h.write(a.as_bytes());
    }

    for (k, v) in &r.env {
        h.write(k.as_bytes());
        h.write(v.as_bytes());
    }

    // declared inputs/outputs participate in definition
    for a in &r.inputs {
        hash_artifact(&mut h, a);
    }
    for a in &r.outputs {
        hash_artifact(&mut h, a);
    }

    for d in &r.deps {
        h.write_u64(d.raw());
    }

    Fingerprint(h.finish())
}

fn fingerprint_io_state(r: &Rule, producer: &HashMap<PathBuf, RuleId>, opts: &RemakeOptions) -> Fingerprint {
    let mut h = Fnv1aHasher::default();

    // inputs: hash path + mtime + size (cheap but works)
    for a in &r.inputs {
        match a {
            Artifact::Path(p) => {
                h.write(b"P:");
                h.write(p.to_string_lossy().as_bytes());
                if let Ok(md) = std::fs::metadata(p) {
                    h.write_u64(md.len());
                    if let Ok(mt) = md.modified() {
                        h.write_u64(system_time_to_u64(mt));
                    }
                } else if opts.treat_missing_input_as_dirty {
                    // encode "missing" marker
                    h.write_u64(0);
                    h.write_u64(0);
                }
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

    // outputs: also hash mtimes and sizes
    for a in &r.outputs {
        if let Artifact::Path(p) = a {
            h.write(b"O:");
            h.write(p.to_string_lossy().as_bytes());
            if let Ok(md) = std::fs::metadata(p) {
                h.write_u64(md.len());
                if let Ok(mt) = md.modified() {
                    h.write_u64(system_time_to_u64(mt));
                }
            } else {
                h.write_u64(0);
                h.write_u64(0);
            }

            // producer consistency check (optional)
            if opts.treat_unknown_output_producer_as_dirty {
                if let Some(prod) = producer.get(p) {
                    h.write_u64(prod.raw());
                } else {
                    h.write_u64(0);
                }
            }
        }
    }

    Fingerprint(h.finish())
}

fn system_time_to_u64(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/* ============================== hashing ============================== */

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

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_cycle_detects() {
        let a = RuleId::new(1);
        let b = RuleId::new(2);
        let c = RuleId::new(3);

        let mut indeg = HashMap::new();
        indeg.insert(a, 1);
        indeg.insert(b, 1);
        indeg.insert(c, 0);

        let mut adj = HashMap::new();
        adj.insert(a, vec![b]);
        adj.insert(b, vec![a]);
        adj.insert(c, vec![]);

        let (topo, cycles) = topo_with_cycles(&indeg, &adj);
        assert_eq!(topo.len(), 3);
        assert!(!cycles.is_empty());
    }

    #[test]
    fn fingerprint_rule_changes() {
        let mut r = Rule::new("x");
        r.command = CommandSpec::new("cc");
        let f1 = fingerprint_rule_definition(&r);
        r.command.args.push("-c".to_string());
        let f2 = fingerprint_rule_definition(&r);
        assert_ne!(f1, f2);
    }
}
