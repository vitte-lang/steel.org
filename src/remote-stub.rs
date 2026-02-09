// src/remote-stub.rs
//
// Steel — Remote Stub (network abstraction placeholder)
//
// Purpose:
// - Provide a minimal, dependency-free "remote" abstraction for Steel that can be
//   compiled even when real networking/registry support is not enabled.
// - Define the stable API surface that the rest of the codebase can depend on:
//   - Remote client trait
//   - Request/Response models
//   - Error types
//   - Basic URL parsing/validation (very small)
// - Offer a feature-flag friendly implementation:
//   - StubRemote: always returns "unsupported" / offline behavior
//
// Why "remote-stub":
// - Keeps the project building on all targets (incl. constrained environments).
// - Allows later replacement by real HTTP/registry implementation without touching call sites.
//
// Typical usage:
//   let remote = StubRemote::new();
//   let res = remote.get_text(&Url::parse("https://example.com")?).await;
//
// Notes:
// - This file intentionally does NOT depend on reqwest/hyper/tokio.
// - The async API is expressed via `std::future::Future` and boxed futures to avoid async-trait deps.
// - If you have tokio in your project, you can provide a real implementation behind cfg(feature="remote").
//
// IMPORTANT (Rust module note):
// - The filename contains a dash; Rust `mod` files must be valid identifiers.
// - Usually you'd keep the actual module as `remote_stub.rs` and optionally keep this file as a
//   copy/alias. If you really want this name on disk, you must include it via `#[path="remote-stub.rs"] mod remote_stub;`
//
//   Example in lib.rs/main.rs:
//     #[path = "remote-stub.rs"]
//     mod remote_stub;

#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/* ============================== url ============================== */

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

        // scheme://rest
        let (scheme, rest) = s.split_once("://").ok_or_else(|| UrlError::Invalid {
            input: s.to_string(),
            reason: "missing '://': expected scheme://host/path".to_string(),
        })?;

        let scheme = scheme.to_ascii_lowercase();
        if !is_scheme(&scheme) {
            return Err(UrlError::Invalid {
                input: s.to_string(),
                reason: "invalid scheme".to_string(),
            });
        }

        // host[:port][/...]
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        if hostport.trim().is_empty() {
            return Err(UrlError::Invalid {
                input: s.to_string(),
                reason: "missing host".to_string(),
            });
        }

        let (host, port) = if let Some((h, p)) = hostport.rsplit_once(':') {
            // If host contains ':' as IPv6, we'd need bracket support. Keep it simple for now.
            if h.contains(':') {
                return Err(UrlError::Invalid {
                    input: s.to_string(),
                    reason: "ipv6 hosts not supported in stub parser".to_string(),
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

        if !is_host(&host) {
            return Err(UrlError::Invalid {
                input: s.to_string(),
                reason: "invalid host".to_string(),
            });
        }

        let path_and_query = if path.trim().is_empty() { "/".to_string() } else { path.to_string() };

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

    pub fn authority(&self) -> String {
        if let Some(p) = self.port {
            format!("{}:{}", self.host, p)
        } else {
            self.host.clone()
        }
    }
}

fn is_scheme(s: &str) -> bool {
    // conservative: [a-z][a-z0-9+.-]*
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    it.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '-'))
}

fn is_host(s: &str) -> bool {
    // conservative: domain-like or ipv4.
    // domain: labels of [a-zA-Z0-9-], separated by '.', no empty label.
    // ipv4: digits and dots, naive check.
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if t.chars().all(|c| c.is_ascii_digit() || c == '.') && t.contains('.') {
        return true;
    }
    let parts: Vec<&str> = t.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    for p in parts {
        if !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }
    true
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

/* ============================== http-ish model ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    PATCH,
    DELETE,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Method::GET => "GET",
            Method::HEAD => "HEAD",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
        };
        f.write_str(s)
    }
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

    pub fn body_bytes(mut self, b: Vec<u8>) -> Self {
        self.body = b;
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

/* ============================== remote API ============================== */

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
            user_agent: Some("steel-remote-stub/0".to_string()),
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
    Transport(String),
    HttpStatus(u16),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Offline => write!(f, "remote is offline"),
            RemoteError::Unsupported => write!(f, "remote operations unsupported (stub)"),
            RemoteError::Timeout => write!(f, "remote request timeout"),
            RemoteError::BadRequest(s) => write!(f, "bad request: {s}"),
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

/* ============================== stub implementation ============================== */

#[derive(Debug, Clone)]
pub struct StubRemote {
    opts: RemoteOptions,
}

impl StubRemote {
    pub fn new() -> Self {
        Self {
            opts: RemoteOptions::default(),
        }
    }

    pub fn with_options(opts: RemoteOptions) -> Self {
        Self { opts }
    }
}

impl Default for StubRemote {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteClient for StubRemote {
    fn options(&self) -> &RemoteOptions {
        &self.opts
    }

    fn request<'a>(&'a self, req: Request) -> BoxFuture<'a, Result<Response, RemoteError>> {
        Box::pin(async move {
            if self.opts.offline {
                return Err(RemoteError::Offline);
            }

            // Even if "online", stub does not actually do network.
            // We validate request superficially so callers can test plumbing.
            if req.url.scheme != "http" && req.url.scheme != "https" {
                return Err(RemoteError::BadRequest(format!(
                    "unsupported scheme: {}",
                    req.url.scheme
                )));
            }

            Err(RemoteError::Unsupported)
        })
    }
}

/* ============================== registry-style helpers (stub) ============================== */

#[derive(Debug, Clone)]
pub struct RegistryRef {
    pub base: Url,
}

impl RegistryRef {
    pub fn new(base: Url) -> Self {
        Self { base }
    }

    pub fn join_path(&self, suffix: &str) -> Result<Url, UrlError> {
        let mut base = self.base.clone();
        let mut p = base.path_and_query.clone();
        if !p.ends_with('/') {
            p.push('/');
        }
        let suf = suffix.trim_start_matches('/');
        p.push_str(suf);
        base.path_and_query = p;
        Ok(base)
    }
}

pub async fn fetch_registry_index<C: RemoteClient>(
    client: &C,
    registry: &RegistryRef,
) -> Result<String, RemoteError> {
    let url = registry
        .join_path("index")
        .map_err(|e| RemoteError::BadRequest(e.to_string()))?;
    client.get_text(url).await
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parse_basic() {
        let u = Url::parse("https://example.com/path").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, None);
        assert_eq!(u.path_and_query, "/path");
        assert_eq!(u.to_string(), "https://example.com/path");
    }

    #[test]
    fn url_parse_with_port() {
        let u = Url::parse("http://localhost:8080/").unwrap();
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.to_string(), "http://localhost:8080/");
    }

    #[test]
    fn url_parse_rejects_no_scheme() {
        assert!(Url::parse("example.com").is_err());
    }

    #[test]
    fn stub_offline() {
        let c = StubRemote::new();
        let u = Url::parse("https://example.com/").unwrap();
        let _fut = c.get_text(u);
        // can't .await in sync test without executor; test request() directly
        let req = Request::new(Method::GET, Url::parse("https://example.com/").unwrap());
        let out = futures_like_block_on(c.request(req));
        assert!(matches!(out, Err(RemoteError::Offline)));
    }

    // Minimal "block_on" for this file only (no tokio).
    fn futures_like_block_on<T>(mut fut: BoxFuture<'_, T>) -> T {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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
}
