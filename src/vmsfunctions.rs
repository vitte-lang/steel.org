//! vmsfunctions.rs
//!
//! “VMS Functions” — registre de fonctions/ops utilisables par le moteur VMS (jobs).
//!
//! Contexte Steel:
//! - vmsjobs.rs exécute des JobStep::Command et JobStep::Inline(closure).
//! - vmsify.rs convertit manifests/config en JobGraph, mais a besoin d’une couche
//!   de fonctions standard pour:
//!   - expansions (vars, paths)
//!   - helpers build (mkdir, writefile, hash, glob minimal, copy/remove, etc.)
//!   - hooks “inline” identifiés par nom (plutôt que closure hardcodée)
//!
//! Ce module propose:
//! - Un registre FnRegistry (name -> FnHandler)
//! - Un RuntimeContext (workspace/build/profile/env, etc.)
//! - Un mini DSL d’appel: FunctionCall { name, args, kv }
//! - Des fonctions built-in: mkdir, write_text, append_text, rm, rmdir, copy, touch,
//!   hash_file (fnv1a64), hash_bytes, env_get/env_set, path_join, path_norm (via VPath si dispo),
//!   list_dir (best-effort), which (best-effort)
//!
//! Dépendances: std uniquement.
//!
//! Intégration:
//! - vmsify.rs: pour TargetStepIR::Inline { message }, remplace par FunctionCall.
//! - commands.rs: peut exposer `steel fn <name> ...` ou usage interne.
//!
//! Sécurité:
//! - Ce module ne “sandbox” pas. Si vous avez des capsules/sandbox, appliquez les politiques
//!   au-dessus (FS roots, net/time, etc.).
//!
//! Note:
//! - Si crate::vpath::VPath existe, on l’utilise en option (feature interne) pour normaliser
//!   des chemins déclaratifs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Résultat générique d’une fonction VMS.
pub type FnResult<T = Value> = Result<T, FnError>;

/// Erreur d’exécution (fonction).
#[derive(Debug)]
pub enum FnError {
    NotFound(String),
    InvalidArgs(String),
    Io(io::Error),
    Failed(String),
}

impl fmt::Display for FnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FnError::NotFound(s) => write!(f, "function not found: {}", s),
            FnError::InvalidArgs(s) => write!(f, "invalid args: {}", s),
            FnError::Io(e) => write!(f, "io: {}", e),
            FnError::Failed(s) => write!(f, "failed: {}", s),
        }
    }
}

impl std::error::Error for FnError {}

impl From<io::Error> for FnError {
    fn from(e: io::Error) -> Self {
        FnError::Io(e)
    }
}

/// Valeur typée minimaliste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b.as_slice()),
            _ => None,
        }
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

/// Appel d’une fonction.
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<Value>,
    pub kv: BTreeMap<String, Value>,
}

impl FunctionCall {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: Vec::new(),
            kv: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, v: impl Into<Value>) -> Self {
        self.args.push(v.into());
        self
    }

    pub fn kv(mut self, k: impl Into<String>, v: impl Into<Value>) -> Self {
        self.kv.insert(k.into(), v.into());
        self
    }
}

/// Contexte d’exécution.
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub workspace_root: PathBuf,
    pub build_dir: PathBuf,
    pub cwd: PathBuf,
    pub profile: Option<String>,
    pub env: BTreeMap<String, String>,

    /// Racines autorisées (optionnel) pour limiter FS ops.
    /// Si non vide: toute opération FS doit rester sous l’une de ces racines.
    pub allowed_roots: Vec<PathBuf>,
}

impl RuntimeContext {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let root = workspace_root.into();
        let build = root.join("build");
        Self {
            workspace_root: root.clone(),
            build_dir: build.clone(),
            cwd: root.clone(),
            profile: None,
            env: BTreeMap::new(),
            allowed_roots: Vec::new(),
        }
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn with_profile(mut self, p: impl Into<String>) -> Self {
        self.profile = Some(p.into());
        self
    }

    pub fn env_set(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.env.insert(k.into(), v.into());
    }

    pub fn env_get(&self, k: &str) -> Option<&str> {
        self.env.get(k).map(|s| s.as_str())
    }

    pub fn add_allowed_root(&mut self, p: impl Into<PathBuf>) {
        self.allowed_roots.push(p.into());
    }

    fn ensure_allowed(&self, path: &Path) -> FnResult<()> {
        if self.allowed_roots.is_empty() {
            return Ok(());
        }
        // Canonicalize best-effort (si le path n’existe pas, on teste parent).
        let test = path
            .canonicalize()
            .or_else(|_| {
                path.parent()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no parent"))?
                    .canonicalize()
                    .map(|p| p.join(path.file_name().unwrap_or_default()))
            })
            .unwrap_or_else(|_| path.to_path_buf());

        for r in &self.allowed_roots {
            if let Ok(rc) = r.canonicalize() {
                if test.starts_with(&rc) {
                    return Ok(());
                }
            } else if test.starts_with(r) {
                return Ok(());
            }
        }
        Err(FnError::Failed(format!(
            "path not allowed: {}",
            path.display()
        )))
    }
}

/// Signature d’une fonction.
pub type FnHandler = fn(&mut RuntimeContext, &FunctionCall) -> FnResult<Value>;

/// Registre.
#[derive(Debug, Default, Clone)]
pub struct FnRegistry {
    handlers: BTreeMap<String, FnHandler>,
    aliases: BTreeMap<String, String>,
    tags: BTreeMap<String, BTreeSet<String>>,
}

impl FnRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, f: FnHandler) -> &mut Self {
        let n = name.into();
        self.handlers.insert(n.clone(), f);
        self
    }

    pub fn alias(&mut self, from: impl Into<String>, to: impl Into<String>) -> &mut Self {
        self.aliases.insert(from.into(), to.into());
        self
    }

    pub fn tag(&mut self, name: impl Into<String>, tag: impl Into<String>) -> &mut Self {
        self.tags
            .entry(name.into())
            .or_default()
            .insert(tag.into());
        self
    }

    pub fn resolve_name<'a>(&'a self, name: &'a str) -> CowStr<'a> {
        if let Some(t) = self.aliases.get(name) {
            CowStr::Owned(t.clone())
        } else {
            CowStr::Borrowed(name)
        }
    }

    pub fn has(&self, name: &str) -> bool {
        let rn = self.resolve_name(name);
        self.handlers.contains_key(rn.as_ref())
    }

    pub fn call(&self, ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let rn = self.resolve_name(&call.name);
        let f = self
            .handlers
            .get(rn.as_ref())
            .ok_or_else(|| FnError::NotFound(call.name.clone()))?;
        f(ctx, call)
    }

    pub fn list(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    pub fn default_with_builtins() -> Self {
        let mut r = Self::new();
        builtins::register_all(&mut r);
        r
    }
}

pub enum CowStr<'a> {
    Borrowed(&'a str),
    Owned(String),
}
impl<'a> CowStr<'a> {
    pub fn as_ref(&self) -> &str {
        match self {
            CowStr::Borrowed(s) => s,
            CowStr::Owned(s) => s.as_str(),
        }
    }
}

/* ======================
 * Arg decoding helpers
 * ====================== */

fn arg_required_str<'a>(call: &'a FunctionCall, idx: usize, name: &str) -> FnResult<&'a str> {
    call.args
        .get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| FnError::InvalidArgs(format!("missing/invalid arg {} ({}: str)", idx, name)))
}

fn kv_opt_bool(call: &FunctionCall, k: &str) -> Option<bool> {
    call.kv.get(k).and_then(|v| v.as_bool())
}

/* ======================
 * Variable expansion
 * ====================== */

/// Expansion simple `${VAR}` dans une chaîne.
/// Sources: ctx.env puis std::env.
pub fn expand_vars(ctx: &RuntimeContext, s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // parse ${...}
            if let Some(end) = s[i + 2..].find('}') {
                let key = &s[i + 2..i + 2 + end];
                let val = ctx
                    .env_get(key)
                    .map(|s| s.to_string())
                    .or_else(|| std::env::var(key).ok())
                    .unwrap_or_default();
                out.push_str(&val);
                i = i + 2 + end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

/// Interprète un “path-like” et le résout sous ctx.cwd si relatif.
pub fn resolve_path(ctx: &RuntimeContext, s: &str) -> PathBuf {
    let expanded = expand_vars(ctx, s);
    let p = PathBuf::from(expanded);
    if p.is_absolute() {
        p
    } else {
        ctx.cwd.join(p)
    }
}

/* ======================
 * Builtins
 * ====================== */

pub mod builtins {
    use super::*;

    pub fn register_all(r: &mut FnRegistry) {
        r.register("mkdir", mkdir).tag("mkdir", "fs");
        r.register("rmdir", rmdir).tag("rmdir", "fs");
        r.register("rm", rm).tag("rm", "fs");
        r.register("copy", copy).tag("copy", "fs");
        r.register("touch", touch).tag("touch", "fs");
        r.register("write_text", write_text).tag("write_text", "fs");
        r.register("append_text", append_text).tag("append_text", "fs");
        r.register("read_text", read_text).tag("read_text", "fs");
        r.register("read_bytes", read_bytes).tag("read_bytes", "fs");

        r.register("hash_file", hash_file).tag("hash_file", "hash");
        r.register("hash_bytes", hash_bytes).tag("hash_bytes", "hash");

        r.register("env_get", env_get).tag("env_get", "env");
        r.register("env_set", env_set).tag("env_set", "env");

        r.register("path_join", path_join).tag("path_join", "path");
        r.register("path_norm", path_norm).tag("path_norm", "path");

        r.register("list_dir", list_dir).tag("list_dir", "fs");
        r.register("which", which).tag("which", "proc");

        // aliases ergonomiques
        r.alias("mkd", "mkdir");
        r.alias("cp", "copy");
        r.alias("del", "rm");
        r.alias("cat", "read_text");
    }

    fn mkdir(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let recursive = kv_opt_bool(call, "recursive").unwrap_or(true);
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        if recursive {
            fs::create_dir_all(&path)?;
        } else {
            fs::create_dir(&path)?;
        }
        Ok(Value::Bool(true))
    }

    fn rmdir(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let recursive = kv_opt_bool(call, "recursive").unwrap_or(false);
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        if recursive {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_dir(&path)?;
        }
        Ok(Value::Bool(true))
    }

    fn rm(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let missing_ok = kv_opt_bool(call, "missing_ok").unwrap_or(true);
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        match fs::remove_file(&path) {
            Ok(()) => Ok(Value::Bool(true)),
            Err(e) if missing_ok && e.kind() == io::ErrorKind::NotFound => Ok(Value::Bool(false)),
            Err(e) => Err(e.into()),
        }
    }

    fn copy(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let src = arg_required_str(call, 0, "src")?;
        let dst = arg_required_str(call, 1, "dst")?;
        let overwrite = kv_opt_bool(call, "overwrite").unwrap_or(true);

        let srcp = resolve_path(ctx, src);
        let dstp = resolve_path(ctx, dst);
        ctx.ensure_allowed(&srcp)?;
        ctx.ensure_allowed(&dstp)?;

        if !overwrite && dstp.exists() {
            return Err(FnError::Failed(format!(
                "destination exists: {}",
                dstp.display()
            )));
        }

        if let Some(parent) = dstp.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&srcp, &dstp)?;
        Ok(Value::Bool(true))
    }

    fn touch(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        f.write_all(b"")?;
        Ok(Value::Bool(true))
    }

    fn write_text(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let content = arg_required_str(call, 1, "content")?;
        let create_dirs = kv_opt_bool(call, "create_dirs").unwrap_or(true);

        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        if create_dirs {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(&path, content.as_bytes())?;
        Ok(Value::Bool(true))
    }

    fn append_text(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let content = arg_required_str(call, 1, "content")?;
        let create_dirs = kv_opt_bool(call, "create_dirs").unwrap_or(true);

        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        if create_dirs {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
        }

        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        f.write_all(content.as_bytes())?;
        Ok(Value::Bool(true))
    }

    fn read_text(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        let data = fs::read_to_string(&path)?;
        Ok(Value::Str(data))
    }

    fn read_bytes(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        let data = fs::read(&path)?;
        Ok(Value::Bytes(data))
    }

    fn env_get(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let k = arg_required_str(call, 0, "key")?;
        if let Some(v) = ctx.env.get(k) {
            return Ok(Value::Str(v.clone()));
        }
        match std::env::var(k) {
            Ok(v) => Ok(Value::Str(v)),
            Err(_) => Ok(Value::Null),
        }
    }

    fn env_set(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let k = arg_required_str(call, 0, "key")?;
        let v = arg_required_str(call, 1, "value")?;
        ctx.env_set(k.to_string(), v.to_string());
        Ok(Value::Bool(true))
    }

    fn path_join(_ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        if call.args.is_empty() {
            return Err(FnError::InvalidArgs("path_join requires >= 1 arg".into()));
        }
        let mut p = PathBuf::new();
        for (i, a) in call.args.iter().enumerate() {
            let s = a.as_str().ok_or_else(|| {
                FnError::InvalidArgs(format!("path_join arg {} must be str", i))
            })?;
            p.push(s);
        }
        Ok(Value::Str(p.to_string_lossy().to_string()))
    }

    fn path_norm(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let expanded = expand_vars(ctx, p);

        // Optionnel: si VPath existe et si l’input ressemble à un vpath (contient '/')
        // on peut normaliser via VPath. Sinon, normalisation basique PathBuf.
        #[allow(unused)]
        fn norm_pathbuf(s: &str) -> String {
            let pb = PathBuf::from(s);
            pb.components()
                .fold(PathBuf::new(), |mut acc, c| {
                    match c {
                        std::path::Component::CurDir => {}
                        std::path::Component::ParentDir => {
                            acc.pop();
                        }
                        _ => acc.push(c.as_os_str()),
                    }
                    acc
                })
                .to_string_lossy()
                .to_string()
        }

        // Try VPath normalization if module present.
        // (Si crate::vpath n’est pas disponible, commente l’usage côté intégration.)
        let out = match crate::vpath::VPath::parse(expanded.as_str()) {
            Ok(vp) => vp.as_str().to_string(),
            Err(_) => norm_pathbuf(expanded.as_str()),
        };

        Ok(Value::Str(out))
    }

    fn list_dir(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        let mut out = Vec::new();
        for e in fs::read_dir(&path)? {
            let e = e?;
            out.push(Value::Str(e.path().to_string_lossy().to_string()));
        }
        Ok(Value::List(out))
    }

    fn which(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let bin = arg_required_str(call, 0, "program")?;
        if bin.contains(std::path::MAIN_SEPARATOR) || bin.contains('/') || bin.contains('\\') {
            let p = resolve_path(ctx, bin);
            return Ok(Value::Bool(p.exists()));
        }

        let path_var = std::env::var_os("PATH").unwrap_or_default();
        for p in std::env::split_paths(&path_var) {
            let candidate = p.join(bin);
            if candidate.exists() {
                return Ok(Value::Str(candidate.to_string_lossy().to_string()));
            }
            // Windows: try .exe
            #[cfg(windows)]
            {
                let c2 = p.join(format!("{}.exe", bin));
                if c2.exists() {
                    return Ok(Value::Str(c2.to_string_lossy().to_string()));
                }
            }
        }
        Ok(Value::Null)
    }

    fn hash_file(ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let p = arg_required_str(call, 0, "path")?;
        let path = resolve_path(ctx, p);
        ctx.ensure_allowed(&path)?;

        let mut f = fs::File::open(&path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        let h = fnv1a64(&buf);
        Ok(Value::Str(format!("{:016x}", h)))
    }

    fn hash_bytes(_ctx: &mut RuntimeContext, call: &FunctionCall) -> FnResult<Value> {
        let b = call
            .args
            .get(0)
            .and_then(|v| v.as_bytes())
            .ok_or_else(|| FnError::InvalidArgs("hash_bytes requires bytes arg".into()))?;
        let h = fnv1a64(b);
        Ok(Value::Str(format!("{:016x}", h)))
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut h = FNV_OFFSET;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }
}

/* ======================
 * Inline adapter
 * ====================== */

/// Construit un InlineSpec à partir d’un FunctionCall + registry.
///
/// Usage:
/// - vmsify.rs peut transformer TargetStepIR::Inline en InlineSpec::new(|| registry.call(...))
pub fn inline_from_call(reg: FnRegistry, ctx: RuntimeContext, call: FunctionCall) -> crate::vmsjobs::InlineSpec {
    inline_from_call_shared(
        std::sync::Arc::new(reg),
        std::sync::Arc::new(std::sync::Mutex::new(ctx)),
        call,
    )
}

/// Variante: emprunte registry et contexte via Arc<Mutex<...>>.
pub fn inline_from_call_shared(
    reg: std::sync::Arc<FnRegistry>,
    ctx: std::sync::Arc<std::sync::Mutex<RuntimeContext>>,
    call: FunctionCall,
) -> crate::vmsjobs::InlineSpec {
    let name = call.name.clone();
    crate::vmsjobs::InlineSpec {
        label: Some(format!("fn:{}", name)),
        f: std::sync::Arc::new(move || {
            let mut guard = ctx.lock().map_err(|_| "ctx mutex poisoned".to_string())?;
            reg.call(&mut *guard, &call).map(|_| ()).map_err(|e| e.to_string())
        }),
    }
}

/* ======================
 * Utility: timestamp
 * ====================== */

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/* ======================
 * Tests
 * ====================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_vars_basic() {
        let mut ctx = RuntimeContext::new(".");
        ctx.env_set("X", "42");
        assert_eq!(expand_vars(&ctx, "a${X}b"), "a42b");
    }

    #[test]
    fn registry_builtins_smoke() {
        let reg = FnRegistry::default_with_builtins();
        let mut ctx = RuntimeContext::new(".");
        let call = FunctionCall::new("env_set").arg("A").arg("B");
        reg.call(&mut ctx, &call).unwrap();
        let call2 = FunctionCall::new("env_get").arg("A");
        let v = reg.call(&mut ctx, &call2).unwrap();
        assert_eq!(v, Value::Str("B".to_string()));
    }

    #[test]
    fn hash_bytes_stable() {
        let reg = FnRegistry::default_with_builtins();
        let mut ctx = RuntimeContext::new(".");
        let call = FunctionCall::new("hash_bytes").arg(Value::Bytes(b"abc".to_vec()));
        let v = reg.call(&mut ctx, &call).unwrap();
        assert!(matches!(v, Value::Str(_)));
    }

    #[test]
    fn path_join_basic() {
        let reg = FnRegistry::default_with_builtins();
        let mut ctx = RuntimeContext::new(".");
        let call = FunctionCall::new("path_join").arg("a").arg("b").arg("c");
        let v = reg.call(&mut ctx, &call).unwrap();
        assert!(v.as_str().unwrap().contains("a"));
    }
}
