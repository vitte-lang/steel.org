// src/steelcustoms.rs
//
// Steel — customs (custom behaviors / extension points)
//
// Purpose:
// - Provide a stable extension surface for Steel without committing to a plugin ABI yet.
// - Centralize "custom" hooks that can be injected by:
//   - build.muf / steelconf configuration
//   - workspace policy (company CI rules, sandbox rules, registry rules)
//   - future dynamic plugins (not implemented here)
//
// This module focuses on:
// - Custom scheme handlers for remote (muf://, file://, env://, etc.) as a registry
// - Custom variable providers (env, git, user config)
// - Custom rule transforms (rewrites / normalization)
// - Custom validators (policy checks)
//
// Design goals:
// - dependency-free
// - deterministic ordering
// - explicit error reporting with reasons
// - easy to unit test
//
// Notes:
// - In your repo, replace local type copies with `use crate::...` to avoid duplication.
// - This is "max": lots of hooks, minimal runtime cost if unused.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

/* ============================== core errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomsError {
    Unsupported(String),
    Invalid(String),
    Forbidden(String),
    NotFound(String),
    Io(String),
    Other(String),
}

impl fmt::Display for CustomsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomsError::Unsupported(s) => write!(f, "unsupported: {s}"),
            CustomsError::Invalid(s) => write!(f, "invalid: {s}"),
            CustomsError::Forbidden(s) => write!(f, "forbidden: {s}"),
            CustomsError::NotFound(s) => write!(f, "not found: {s}"),
            CustomsError::Io(s) => write!(f, "i/o error: {s}"),
            CustomsError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for CustomsError {}

/* ============================== custom URL + schemes ============================== */

/// Minimal URL representation for custom scheme routing.
/// Prefer using your actual `remote_*` modules; this is a stable "customs" layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CUrl {
    pub scheme: String,
    pub host: String,
    pub path: String,
    pub query: Option<String>,
}

impl CUrl {
    pub fn parse(input: &str) -> Result<Self, CustomsError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(CustomsError::Invalid("url is empty".to_string()));
        }

        let (scheme, rest) = s.split_once("://").ok_or_else(|| {
            CustomsError::Invalid("missing '://': expected scheme://...".to_string())
        })?;

        let scheme = scheme.to_ascii_lowercase();

        let (host, pathq) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let (path, query) = if let Some((p, q)) = pathq.split_once('?') {
            (p.to_string(), Some(q.to_string()))
        } else {
            (pathq.to_string(), None)
        };

        Ok(Self {
            scheme,
            host: host.to_string(),
            path,
            query,
        })
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.scheme);
        s.push_str("://");
        s.push_str(&self.host);
        s.push_str(&self.path);
        if let Some(q) = &self.query {
            s.push('?');
            s.push_str(q);
        }
        s
    }
}

/* ============================== scheme handlers registry ============================== */

#[derive(Debug, Clone)]
pub struct SchemeContext {
    pub cwd: Option<PathBuf>,
    pub vars: BTreeMap<String, String>,
    pub allow_net: bool,
    pub allow_fs: bool,
}

impl Default for SchemeContext {
    fn default() -> Self {
        Self {
            cwd: None,
            vars: BTreeMap::new(),
            allow_net: false,
            allow_fs: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchemeResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

pub trait SchemeHandler: Send + Sync {
    fn scheme(&self) -> &'static str;

    /// Handle a custom scheme request.
    /// For synchronous customs, return response immediately.
    fn handle(&self, ctx: &SchemeContext, url: &CUrl) -> Result<SchemeResponse, CustomsError>;
}

#[derive(Default)]
pub struct SchemeRegistry {
    handlers: BTreeMap<String, Box<dyn SchemeHandler>>,
}

impl SchemeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, h: Box<dyn SchemeHandler>) -> Result<(), CustomsError> {
        let key = h.scheme().to_ascii_lowercase();
        if self.handlers.contains_key(&key) {
            return Err(CustomsError::Invalid(format!("scheme already registered: {key}")));
        }
        self.handlers.insert(key, h);
        Ok(())
    }

    pub fn get(&self, scheme: &str) -> Option<&dyn SchemeHandler> {
        self.handlers
            .get(&scheme.to_ascii_lowercase())
            .map(|b| b.as_ref())
    }

    pub fn schemes(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    pub fn handle(&self, ctx: &SchemeContext, url: &CUrl) -> Result<SchemeResponse, CustomsError> {
        let h = self
            .get(&url.scheme)
            .ok_or_else(|| CustomsError::Unsupported(format!("unknown scheme: {}", url.scheme)))?;
        h.handle(ctx, url)
    }
}

/* ============================== variable providers ============================== */

pub trait VarProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Provide variables. Deterministic ordering via BTreeMap.
    fn provide(&self, ctx: &VarsContext) -> Result<BTreeMap<String, String>, CustomsError>;
}

#[derive(Debug, Clone)]
pub struct VarsContext {
    pub cwd: Option<PathBuf>,
    pub env_snapshot: BTreeMap<String, String>,
    pub user_config: BTreeMap<String, String>,
}

impl Default for VarsContext {
    fn default() -> Self {
        Self {
            cwd: None,
            env_snapshot: BTreeMap::new(),
            user_config: BTreeMap::new(),
        }
    }
}

#[derive(Default)]
pub struct VarRegistry {
    providers: Vec<Box<dyn VarProvider>>,
}

impl VarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, p: Box<dyn VarProvider>) {
        self.providers.push(p);
        self.providers.sort_by_key(|x| x.name()); // stable
    }

    pub fn collect(&self, ctx: &VarsContext) -> Result<BTreeMap<String, String>, CustomsError> {
        let mut out = BTreeMap::new();
        for p in &self.providers {
            let vars = p.provide(ctx)?;
            for (k, v) in vars {
                // last writer wins, but deterministic due to provider order.
                out.insert(k, v);
            }
        }
        Ok(out)
    }
}

/// Provider: exposes a filtered snapshot of environment variables.
pub struct EnvVarProvider {
    pub allow_any: bool,
    pub allowlist: BTreeSet<String>,
    pub prefix: Option<String>,
}

impl EnvVarProvider {
    pub fn allow_any() -> Self {
        Self {
            allow_any: true,
            allowlist: BTreeSet::new(),
            prefix: None,
        }
    }

    pub fn allowlist(keys: &[&str]) -> Self {
        let mut s = BTreeSet::new();
        for k in keys {
            s.insert((*k).to_string());
        }
        Self {
            allow_any: false,
            allowlist: s,
            prefix: None,
        }
    }

    pub fn with_prefix(mut self, p: &str) -> Self {
        self.prefix = Some(p.to_string());
        self
    }
}

impl VarProvider for EnvVarProvider {
    fn name(&self) -> &'static str {
        "env"
    }

    fn provide(&self, ctx: &VarsContext) -> Result<BTreeMap<String, String>, CustomsError> {
        let mut out = BTreeMap::new();
        for (k, v) in &ctx.env_snapshot {
            if self.allow_any || self.allowlist.contains(k) {
                let key = if let Some(p) = &self.prefix {
                    format!("{p}{k}")
                } else {
                    k.clone()
                };
                out.insert(key, v.clone());
            }
        }
        Ok(out)
    }
}

/* ============================== rule transforms ============================== */

/// Minimal rule model for transformation hooks.
/// Replace with your actual Rule from src/rule.rs.
#[derive(Debug, Clone)]
pub struct CRule {
    pub name: String,
    pub phony: bool,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub tags: BTreeSet<String>,
    pub meta: BTreeMap<String, String>,
}

impl CRule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            phony: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
            env: BTreeMap::new(),
            tags: BTreeSet::new(),
            meta: BTreeMap::new(),
        }
    }
}

pub trait RuleTransform: Send + Sync {
    fn name(&self) -> &'static str;
    fn apply(&self, rule: &mut CRule) -> Result<(), CustomsError>;
}

#[derive(Default)]
pub struct RuleTransforms {
    transforms: Vec<Box<dyn RuleTransform>>,
}

impl RuleTransforms {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, t: Box<dyn RuleTransform>) {
        self.transforms.push(t);
        self.transforms.sort_by_key(|x| x.name());
    }

    pub fn apply_all(&self, rule: &mut CRule) -> Result<(), CustomsError> {
        for t in &self.transforms {
            t.apply(rule)?;
        }
        Ok(())
    }
}

/// Example transform: auto-tag rules based on outputs/extensions.
pub struct AutoTagByExt;

impl RuleTransform for AutoTagByExt {
    fn name(&self) -> &'static str {
        "auto_tag_by_ext"
    }

    fn apply(&self, rule: &mut CRule) -> Result<(), CustomsError> {
        for o in &rule.outputs {
            if let Some(ext) = o.extension().and_then(|e| e.to_str()) {
                match ext.to_ascii_lowercase().as_str() {
                    "o" | "obj" => {
                        rule.tags.insert("object".to_string());
                    }
                    "a" | "lib" => {
                        rule.tags.insert("archive".to_string());
                    }
                    "so" | "dll" | "dylib" => {
                        rule.tags.insert("shared".to_string());
                    }
                    "exe" => {
                        rule.tags.insert("exe".to_string());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

/* ============================== validators ============================== */

pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate_rule(&self, rule: &CRule) -> Result<(), CustomsError>;
}

#[derive(Default)]
pub struct ValidatorSet {
    validators: Vec<Box<dyn Validator>>,
}

impl ValidatorSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, v: Box<dyn Validator>) {
        self.validators.push(v);
        self.validators.sort_by_key(|x| x.name());
    }

    pub fn validate(&self, rule: &CRule) -> Result<(), CustomsError> {
        for v in &self.validators {
            v.validate_rule(rule)?;
        }
        Ok(())
    }
}

/// Example validator: restrict output paths to be inside a workspace root.
pub struct OutputUnderRoot {
    pub root: PathBuf,
}

impl OutputUnderRoot {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Validator for OutputUnderRoot {
    fn name(&self) -> &'static str {
        "output_under_root"
    }

    fn validate_rule(&self, rule: &CRule) -> Result<(), CustomsError> {
        for o in &rule.outputs {
            if !o.starts_with(&self.root) {
                return Err(CustomsError::Forbidden(format!(
                    "rule '{}' output outside root: {} (root={})",
                    rule.name,
                    o.display(),
                    self.root.display()
                )));
            }
        }
        Ok(())
    }
}

/* ============================== customs bundle ============================== */

/// Convenience: a single struct holding all extension registries.
#[derive(Default)]
pub struct FlanCustoms {
    pub schemes: SchemeRegistry,
    pub vars: VarRegistry,
    pub transforms: RuleTransforms,
    pub validators: ValidatorSet,
}

impl FlanCustoms {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults(mut self) -> Self {
        // Default transforms
        self.transforms.register(Box::new(AutoTagByExt));

        // Default var provider: none by default (safer); user can opt-in.
        // self.vars.register(Box::new(EnvVarProvider::allow_any()));

        self
    }

    pub fn apply_to_rule(&self, rule: &mut CRule) -> Result<(), CustomsError> {
        self.transforms.apply_all(rule)?;
        self.validators.validate(rule)?;
        Ok(())
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_registry_registers() {
        struct X;
        impl SchemeHandler for X {
            fn scheme(&self) -> &'static str {
                "x"
            }
            fn handle(&self, _ctx: &SchemeContext, _url: &CUrl) -> Result<SchemeResponse, CustomsError> {
                Ok(SchemeResponse {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: b"ok".to_vec(),
                })
            }
        }

        let mut reg = SchemeRegistry::new();
        reg.register(Box::new(X)).unwrap();

        let ctx = SchemeContext::default();
        let url = CUrl::parse("x://host/path").unwrap();
        let res = reg.handle(&ctx, &url).unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, b"ok");
    }

    #[test]
    fn transforms_auto_tag() {
        let mut r = CRule::new("t");
        r.outputs.push(PathBuf::from("a.o"));

        let mut set = RuleTransforms::new();
        set.register(Box::new(AutoTagByExt));
        set.apply_all(&mut r).unwrap();

        assert!(r.tags.contains("object"));
    }

    #[test]
    fn validator_output_under_root() {
        let root = PathBuf::from("build");
        let v = OutputUnderRoot::new(root.clone());

        let mut r = CRule::new("x");
        r.outputs.push(PathBuf::from("build/out.o"));
        v.validate_rule(&r).unwrap();

        r.outputs.push(PathBuf::from("other/out.o"));
        assert!(v.validate_rule(&r).is_err());
    }

    #[test]
    fn env_provider_allowlist() {
        let ctx = VarsContext {
            env_snapshot: {
                let mut m = BTreeMap::new();
                m.insert("A".to_string(), "1".to_string());
                m.insert("B".to_string(), "2".to_string());
                m
            },
            ..Default::default()
        };

        let p = EnvVarProvider::allowlist(&["B"]).with_prefix("ENV_");
        let vars = p.provide(&ctx).unwrap();
        assert_eq!(vars.get("ENV_B").map(|s| s.as_str()), Some("2"));
        assert!(vars.get("ENV_A").is_none());
    }
}
