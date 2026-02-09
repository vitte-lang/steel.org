// src/remote-cstms.rs
//
// Steel — Remote CSTMS (Custom Schemes / Transports)
//
// Purpose:
// - Provide a small routing layer for "remote" URIs with custom schemes.
// - Keep call sites stable: they talk to a single RemoteClient, which can resolve:
//     - http/https   (delegated to another RemoteClient, e.g. remote_stub or real http client)
//     - file://      (local filesystem reads)
//     - muf://       (steel pseudo scheme; maps to registry/ref paths)
//     - env://       (read from environment variables; useful for CI injection)
// - Dependency-free, async-friendly without async-trait.
//
// Notes:
// - Filename contains a dash; Rust module filenames should be identifiers.
//   If you keep this path, include it via:
//     #[path = "remote-cstms.rs"]
//     mod remote_cstms;
//
// Integration:
// - Pair with src/remote-stub.rs (or a real remote impl) and route http/https to it.
// - Use `CstmsRemote` as the app-wide client.
//
// Security:
// - file:// access is optionally restricted via RootPolicy.
// - env:// access is optionally restricted by allowlist.

#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

/* ============================== shared model ============================== */

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: Url,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub timeout: Option<Duration>,
}

impl Request {
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: BTreeMap::new(),
            body: Vec::new(),
            timeout: None,
        }
    }

    pub fn header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.insert(k.into(), v.into());
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

#[derive(Debug, Clone)]
pub struct RemoteOptions {
    pub offline: bool,
    pub user_agent: Option<String>,
    pub default_timeout: Duration,
}

impl Default for RemoteOptions {
    fn default() -> Self {
        Self {
            offline: true,
            user_agent: Some("steel-remote-cstms/0".to_string()),
            default_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    Offline,
    Unsupported,
    Timeout,
    BadRequest(String),
    Forbidden(String),
    NotFound(String),
    Io(String),
    Transport(String),
    HttpStatus(u16),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Offline => write!(f, "remote is offline"),
            RemoteError::Unsupported => write!(f, "remote scheme unsupported"),
            RemoteError::Timeout => write!(f, "remote request timeout"),
            RemoteError::BadRequest(s) => write!(f, "bad request: {s}"),
            RemoteError::Forbidden(s) => write!(f, "forbidden: {s}"),
            RemoteError::NotFound(s) => write!(f, "not found: {s}"),
            RemoteError::Io(s) => write!(f, "i/o error: {s}"),
            RemoteError::Transport(s) => write!(f, "transport error: {s}"),
            RemoteError::HttpStatus(code) => write!(f, "http status {code}"),
        }
    }
}

impl std::error::Error for RemoteError {}

pub trait RemoteClient: Send + Sync {
    fn options(&self) -> &RemoteOptions;
    fn request<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>>;

    fn get_text<'a>(&'a self, url: Url) -> BoxFuture<'a, Result<String, RemoteError>> {
        Box::pin(async move {
            let req = Request::new(Method::GET, url);
            let res = self.request(req).await?;
            if res.status >= 400 {
                return Err(RemoteError::HttpStatus(res.status));
            }
            Ok(res.text_lossy().to_string())
        })
    }
}

/* ============================== URL (small) ============================== */

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path_and_query: String,
}

impl Url {
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(UrlError::Empty);
        }

        let (scheme, rest) = s.split_once("://").ok_or_else(|| UrlError::Invalid {
            input: s.to_string(),
            reason: "missing '://': expected scheme://...".to_string(),
        })?;

        let scheme = scheme.to_ascii_lowercase();
        if !is_scheme(&scheme) {
            return Err(UrlError::Invalid {
                input: s.to_string(),
                reason: "invalid scheme".to_string(),
            });
        }

        // Custom schemes may not have a real host; allow empty host for file/muf/env via ":///" pattern.
        // Parse as:
        //   scheme://host[:port]/path...
        // or scheme:///path...
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let (host, port) = if hostport.is_empty() {
            (String::new(), None)
        } else if let Some((h, p)) = hostport.rsplit_once(':') {
            if h.contains(':') {
                return Err(UrlError::Invalid {
                    input: s.to_string(),
                    reason: "ipv6 hosts not supported in small parser".to_string(),
                });
            }
            let port: u16 = p.parse().map_err(|_| UrlError::Invalid {
                input: s.to_string(),
                reason: "invalid port".to_string(),
            })?;
            (h.to_string(), Some(port))
        } else {
            (hostport.to_string(), None)
        };

        let path_and_query = if path.is_empty() { "/".to_string() } else { path.to_string() };

        Ok(Self {
            scheme,
            host,
            port,
            path_and_query,
        })
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.scheme);
        s.push_str("://");
        s.push_str(&self.host);
        if let Some(p) = self.port {
            s.push(':');
            s.push_str(&p.to_string());
        }
        s.push_str(&self.path_and_query);
        s
    }
}

fn is_scheme(s: &str) -> bool {
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    it.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '-'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlError {
    Empty,
    Invalid { input: String, reason: String },
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlError::Empty => write!(f, "url is empty"),
            UrlError::Invalid { input, reason } => write!(f, "invalid url '{input}': {reason}"),
        }
    }
}

impl std::error::Error for UrlError {}

/* ============================== policies ============================== */

#[derive(Debug, Clone)]
pub struct RootPolicy {
    /// If set, file:// reads must be under one of these roots.
    pub allow_roots: Vec<PathBuf>,
    /// If true, allow file access without root restriction.
    pub allow_any_file: bool,
}

impl Default for RootPolicy {
    fn default() -> Self {
        Self {
            allow_roots: Vec::new(),
            allow_any_file: false,
        }
    }
}

impl RootPolicy {
    pub fn allow_any() -> Self {
        Self {
            allow_roots: Vec::new(),
            allow_any_file: true,
        }
    }

    pub fn is_allowed_path(&self, path: &Path) -> bool {
        if self.allow_any_file {
            return true;
        }
        if self.allow_roots.is_empty() {
            return false;
        }

        // Best-effort lexical check (no canonicalize to avoid IO surprises here).
        for root in &self.allow_roots {
            if path.starts_with(root) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub struct EnvPolicy {
    pub allow_any: bool,
    pub allowlist: BTreeSet<String>,
}

impl Default for EnvPolicy {
    fn default() -> Self {
        Self {
            allow_any: false,
            allowlist: BTreeSet::new(),
        }
    }
}

impl EnvPolicy {
    pub fn allow_any() -> Self {
        Self {
            allow_any: true,
            allowlist: BTreeSet::new(),
        }
    }

    pub fn is_allowed(&self, key: &str) -> bool {
        self.allow_any || self.allowlist.contains(key)
    }
}

/* ============================== scheme handlers ============================== */

pub trait SchemeHandler: Send + Sync {
    fn scheme(&self) -> &'static str;

    fn handle<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>>;
}

/* ============================== file:// handler ============================== */

#[derive(Debug, Clone)]
pub struct FileHandler {
    pub roots: RootPolicy,
}

impl FileHandler {
    pub fn new(roots: RootPolicy) -> Self {
        Self { roots }
    }

    fn url_to_path(&self, url: &Url) -> Result<PathBuf, RemoteError> {
        // file:///abs/path
        // file://./rel/path  (discouraged; but allow via host empty and path)
        if url.scheme != "file" {
            return Err(RemoteError::BadRequest("not a file url".to_string()));
        }

        if !url.host.is_empty() && url.host != "localhost" {
            // keep strict: remote file hosts are not supported.
            return Err(RemoteError::BadRequest(format!(
                "file host unsupported: {}",
                url.host
            )));
        }

        let mut p = url.path_and_query.clone();
        // strip query if present
        if let Some((path, _q)) = p.split_once('?') {
            p = path.to_string();
        }

        // file:///C:/... (Windows) or file:///home/...
        // Our parser keeps leading '/'.
        let path = if cfg!(windows) {
            // Convert "/C:/x" -> "C:/x"
            if let Some(stripped) = p.strip_prefix('/') {
                PathBuf::from(stripped)
            } else {
                PathBuf::from(p)
            }
        } else {
            PathBuf::from(p)
        };

        Ok(path)
    }
}

impl SchemeHandler for FileHandler {
    fn scheme(&self) -> &'static str {
        "file"
    }

    fn handle<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            if req.method != Method::GET && req.method != Method::HEAD {
                return Err(RemoteError::Unsupported);
            }

            let path = self.url_to_path(&req.url)?;

            if !self.roots.is_allowed_path(&path) {
                return Err(RemoteError::Forbidden(format!(
                    "file access denied: {}",
                    path.display()
                )));
            }

            let data = std::fs::read(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RemoteError::NotFound(path.display().to_string())
                } else {
                    RemoteError::Io(e.to_string())
                }
            })?;

            Ok(Response {
                status: 200,
                headers: BTreeMap::new(),
                body: if req.method == Method::HEAD { Vec::new() } else { data },
            })
        })
    }
}

/* ============================== env:// handler ============================== */

#[derive(Debug, Clone)]
pub struct EnvHandler {
    pub policy: EnvPolicy,
}

impl EnvHandler {
    pub fn new(policy: EnvPolicy) -> Self {
        Self { policy }
    }

    fn url_to_key(&self, url: &Url) -> Result<String, RemoteError> {
        // env:///KEY or env://KEY (host as KEY, path ignored)
        if url.scheme != "env" {
            return Err(RemoteError::BadRequest("not an env url".to_string()));
        }

        let key = if !url.host.is_empty() {
            url.host.clone()
        } else {
            let mut p = url.path_and_query.clone();
            if let Some((path, _q)) = p.split_once('?') {
                p = path.to_string();
            }
            p.trim_start_matches('/').to_string()
        };

        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(RemoteError::BadRequest("env key missing".to_string()));
        }
        if !is_env_key(&key) {
            return Err(RemoteError::BadRequest(format!("invalid env key: {key}")));
        }
        Ok(key)
    }
}

impl SchemeHandler for EnvHandler {
    fn scheme(&self) -> &'static str {
        "env"
    }

    fn handle<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            if req.method != Method::GET && req.method != Method::HEAD {
                return Err(RemoteError::Unsupported);
            }

            let key = self.url_to_key(&req.url)?;
            if !self.policy.is_allowed(&key) {
                return Err(RemoteError::Forbidden(format!("env access denied: {key}")));
            }

            let val = std::env::var(&key).map_err(|_| RemoteError::NotFound(key.clone()))?;
            Ok(Response {
                status: 200,
                headers: BTreeMap::new(),
                body: if req.method == Method::HEAD { Vec::new() } else { val.into_bytes() },
            })
        })
    }
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

/* ============================== muf:// handler ============================== */

/// muf:// is a Steel pseudo-scheme for registry-like addressing.
/// This handler rewrites muf://... into a base URL + path.
///
/// Example:
///   base = https://registry.example.com/
///   muf://publisher/pkg@1.2.3 -> https://registry.example.com/muf/publisher/pkg@1.2.3
#[derive(Debug, Clone)]
pub struct MufHandler {
    pub base_http: Url,
    pub prefix: String, // e.g. "/muf/"
}

impl MufHandler {
    pub fn new(base_http: Url) -> Self {
        Self {
            base_http,
            prefix: "/muf/".to_string(),
        }
    }

    fn rewrite(&self, url: &Url) -> Result<Url, RemoteError> {
        if url.scheme != "muf" {
            return Err(RemoteError::BadRequest("not a muf url".to_string()));
        }

        // Build path from host + path
        let mut path = String::new();
        let host = url.host.trim();
        if !host.is_empty() {
            path.push_str(host);
        }

        let mut p = url.path_and_query.clone();
        if let Some((pp, q)) = p.split_once('?') {
            // preserve query
            p = format!("{}?{}", pp, q);
        }

        let p_no_lead = p.trim_start_matches('/');
        if !p_no_lead.is_empty() {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(p_no_lead);
        }

        if path.is_empty() {
            return Err(RemoteError::BadRequest("muf reference missing".to_string()));
        }

        let mut out = self.base_http.clone();
        let mut base_path = out.path_and_query.clone();
        if base_path.is_empty() {
            base_path = "/".to_string();
        }

        // Ensure base ends with '/'
        if !base_path.ends_with('/') {
            base_path.push('/');
        }

        // Prefix normalization
        let mut prefix = self.prefix.clone();
        if !prefix.starts_with('/') {
            prefix.insert(0, '/');
        }
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        let prefix = prefix.trim_start_matches('/'); // because base_path already has '/'

        out.path_and_query = format!("{}{}{}", base_path, prefix, path);
        Ok(out)
    }
}

impl SchemeHandler for MufHandler {
    fn scheme(&self) -> &'static str {
        "muf"
    }

    fn handle<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            // Rewrite to http(s) and let the routed client handle it.
            // This handler is intended to be used only inside CstmsRemote, so calling it directly is unsupported.
            let _ = req;
            Err(RemoteError::Unsupported)
        })
    }
}

/* ============================== router ============================== */

#[derive(Debug, Clone)]
pub struct CstmsConfig {
    pub roots: RootPolicy,
    pub env: EnvPolicy,

    /// Optional base for muf:// rewriting. If absent, muf:// is unsupported.
    pub muf_base: Option<Url>,

    /// If true, unknown schemes error; if false, they can be forwarded to fallback.
    pub strict_schemes: bool,
}

impl Default for CstmsConfig {
    fn default() -> Self {
        Self {
            roots: RootPolicy::default(),
            env: EnvPolicy::default(),
            muf_base: None,
            strict_schemes: true,
        }
    }
}

/// Router remote:
/// - Handles file/env internally
/// - Rewrites muf:// to http(s) and delegates
/// - Delegates http/https to `http_client`
/// - All else -> fallback or error
pub struct CstmsRemote {
    opts: RemoteOptions,

    file: FileHandler,
    env: EnvHandler,
    muf: Option<MufHandler>,

    http_client: Box<dyn RemoteClient>,
    fallback: Option<Box<dyn RemoteClient>>,
    strict_schemes: bool,
}

impl CstmsRemote {
    pub fn new(http_client: Box<dyn RemoteClient>, cfg: CstmsConfig) -> Self {
        let muf = cfg.muf_base.clone().map(MufHandler::new);

        Self {
            opts: RemoteOptions::default(),
            file: FileHandler::new(cfg.roots),
            env: EnvHandler::new(cfg.env),
            muf,
            http_client,
            fallback: None,
            strict_schemes: cfg.strict_schemes,
        }
    }

    pub fn with_options(mut self, opts: RemoteOptions) -> Self {
        self.opts = opts;
        self
    }

    pub fn with_fallback(mut self, fb: Box<dyn RemoteClient>) -> Self {
        self.fallback = Some(fb);
        self
    }

    fn delegate<'a>(&'a self, client: &'a dyn RemoteClient, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        client.request(req)
    }

    fn rewrite_muf_to_http(&self, url: &Url) -> Result<Url, RemoteError> {
        let h = self.muf.as_ref().ok_or(RemoteError::Unsupported)?;
        h.rewrite(url)
    }
}

impl RemoteClient for CstmsRemote {
    fn options(&self) -> &RemoteOptions {
        &self.opts
    }

    fn request<'a>(&'a self, mut req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            if self.opts.offline {
                // Still allow local schemes while "offline".
                match req.url.scheme.as_str() {
                    "file" | "env" => {}
                    _ => return Err(RemoteError::Offline),
                }
            }

            // Apply default timeout if caller did not specify.
            if req.timeout.is_none() {
                req.timeout = Some(self.opts.default_timeout);
            }

            match req.url.scheme.as_str() {
                "file" => self.file.handle(req).await,
                "env" => self.env.handle(req).await,
                "muf" => {
                    let rewritten = self.rewrite_muf_to_http(&req.url)?;
                    req.url = rewritten;
                    // rewritten scheme is http/https (expected)
                    self.delegate(self.http_client.as_ref(), req).await
                }
                "http" | "https" => self.delegate(self.http_client.as_ref(), req).await,
                _ => {
                    if let Some(fb) = self.fallback.as_ref() {
                        self.delegate(fb.as_ref(), req).await
                    } else if self.strict_schemes {
                        Err(RemoteError::Unsupported)
                    } else {
                        Err(RemoteError::Unsupported)
                    }
                }
            }
        })
    }
}

/* ============================== stub http client (optional) ============================== */

/// Minimal stub to use as http_client when you don't link real networking.
/// - If offline => Offline
/// - Else => Unsupported (no actual networking)
#[derive(Debug, Clone)]
pub struct StubHttpRemote {
    opts: RemoteOptions,
}

impl StubHttpRemote {
    pub fn new() -> Self {
        Self {
            opts: RemoteOptions {
                offline: true,
                user_agent: Some("steel-http-stub/0".to_string()),
                default_timeout: Duration::from_secs(30),
            },
        }
    }

    pub fn with_options(opts: RemoteOptions) -> Self {
        Self { opts }
    }
}

impl Default for StubHttpRemote {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteClient for StubHttpRemote {
    fn options(&self) -> &RemoteOptions {
        &self.opts
    }

    fn request<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            if self.opts.offline {
                return Err(RemoteError::Offline);
            }
            if req.url.scheme != "http" && req.url.scheme != "https" {
                return Err(RemoteError::BadRequest(format!(
                    "unsupported scheme for http stub: {}",
                    req.url.scheme
                )));
            }
            Err(RemoteError::Unsupported)
        })
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn block_on<T>(mut fut: BoxFuture<'_, T>) -> T {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);

        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);

        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => continue,
            }
        }
    }

    #[test]
    fn env_handler_reads_allowlisted() {
        std::env::set_var("MUFFIN_TEST_ENV_X", "ok");

        let policy = {
            let mut p = EnvPolicy::default();
            p.allowlist.insert("MUFFIN_TEST_ENV_X".to_string());
            p
        };

        let envh = EnvHandler::new(policy);
        let url = Url::parse("env:///MUFFIN_TEST_ENV_X").unwrap();
        let req = Request::new(Method::GET, url);
        let res = block_on(envh.handle(req)).unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.text_lossy(), "ok");
    }

    #[test]
    fn cstms_offline_allows_file_and_env() {
        let http = Box::new(StubHttpRemote::new());
        let cfg = CstmsConfig {
            roots: RootPolicy::allow_any(),
            env: EnvPolicy::allow_any(),
            muf_base: None,
            strict_schemes: true,
        };

        let remote = CstmsRemote::new(http, cfg).with_options(RemoteOptions {
            offline: true,
            user_agent: None,
            default_timeout: Duration::from_secs(1),
        });

        // env should work
        std::env::set_var("MUFFIN_TEST_ENV_Y", "y");
        let url = Url::parse("env:///MUFFIN_TEST_ENV_Y").unwrap();
        let req = Request::new(Method::GET, url);
        let res = block_on(remote.request(req)).unwrap();
        assert_eq!(res.text_lossy(), "y");

        // http should fail offline
        let url = Url::parse("https://example.com/").unwrap();
        let req = Request::new(Method::GET, url);
        let err = block_on(remote.request(req)).unwrap_err();
        assert!(matches!(err, RemoteError::Offline));
    }

    #[test]
    fn muf_rewrite_requires_base() {
        let http = Box::new(StubHttpRemote::with_options(RemoteOptions {
            offline: false,
            user_agent: None,
            default_timeout: Duration::from_secs(1),
        }));

        let cfg = CstmsConfig {
            roots: RootPolicy::allow_any(),
            env: EnvPolicy::allow_any(),
            muf_base: None,
            strict_schemes: true,
        };

        let remote = CstmsRemote::new(http, cfg).with_options(RemoteOptions {
            offline: false,
            user_agent: None,
            default_timeout: Duration::from_secs(1),
        });

        let url = Url::parse("muf://publisher/pkg@1.0.0").unwrap();
        let req = Request::new(Method::GET, url);
        let err = block_on(remote.request(req)).unwrap_err();
        assert!(matches!(err, RemoteError::Unsupported));
    }

    #[test]
    fn muf_rewrite_builds_http_url() {
        let http = Box::new(StubHttpRemote::with_options(RemoteOptions {
            offline: false,
            user_agent: None,
            default_timeout: Duration::from_secs(1),
        }));

        let cfg = CstmsConfig {
            roots: RootPolicy::allow_any(),
            env: EnvPolicy::allow_any(),
            muf_base: Some(Url::parse("https://registry.example.com/").unwrap()),
            strict_schemes: true,
        };

        let remote = CstmsRemote::new(http, cfg).with_options(RemoteOptions {
            offline: false,
            user_agent: None,
            default_timeout: Duration::from_secs(1),
        });

        // Request will be delegated to http stub -> Unsupported, but rewrite must succeed.
        let url = Url::parse("muf://publisher/pkg@1.0.0").unwrap();
        let req = Request::new(Method::GET, url);
        let err = block_on(remote.request(req)).unwrap_err();
        assert!(matches!(err, RemoteError::Unsupported | RemoteError::Transport(_) | RemoteError::HttpStatus(_)));
    }
}
