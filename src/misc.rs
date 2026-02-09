// src/misc.rs
//
// Steel — misc (shared small utilities)
//
// Purpose:
// - Centralize tiny, dependency-free helpers used across the codebase.
// - Keep "utility sprawl" contained.
// - Provide:
//   - string helpers (trim, split, join, normalize)
//   - path helpers (normalize, ensure parent dir, relative display)
//   - time helpers (now, monotonic ms)
//   - hashing helpers (fnv1a, stable ids)
//   - small collections helpers (dedup stable, set ops)
//   - error helpers (wrap io errors with context)
//   - env helpers (read + parse with defaults)
//
// Notes:
// - This file is intentionally broad but still conservative.
// - Prefer moving domain logic to dedicated modules; keep this as "glue".

#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hash::{Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscError {
    pub ctx: String,
    pub message: String,
}

impl MiscError {
    pub fn new(ctx: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ctx: ctx.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for MiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.ctx, self.message)
    }
}

impl std::error::Error for MiscError {}

pub fn io_err(ctx: impl Into<String>, e: io::Error) -> MiscError {
    MiscError::new(ctx, e.to_string())
}

/* ============================== string helpers ============================== */

pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

pub fn trim_one_newline(s: &str) -> &str {
    if let Some(x) = s.strip_suffix("\r\n") {
        x
    } else if let Some(x) = s.strip_suffix('\n') {
        x
    } else {
        s
    }
}

/// Split once on a separator, returning (left, right) trimmed.
pub fn split_once_trim<'a>(s: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let (a, b) = s.split_once(sep)?;
    Some((a.trim(), b.trim()))
}

/// Join non-empty parts with a delimiter.
pub fn join_non_empty<'a, I>(parts: I, delim: &str) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out = String::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str(delim);
        }
        out.push_str(p);
    }
    out
}

/// Normalize a "key" (config / var):
/// - trims
/// - collapses interior whitespace to single spaces
/// - lowercases if requested
pub fn normalize_key(s: &str, lower: bool) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            prev_space = false;
            out.push(if lower { ch.to_ascii_lowercase() } else { ch });
        }
    }
    out
}

/* ============================== path helpers ============================== */

/// Best-effort normalize:
/// - removes "." segments
/// - collapses "a/../" where possible (lexical)
/// - does NOT touch filesystem, does NOT resolve symlinks
pub fn normalize_path_lexical(path: &Path) -> PathBuf {
    let mut parts: Vec<Cow<'_, str>> = Vec::new();

    for comp in path.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // pop if safe (don't pop prefix/root)
                if let Some(last) = parts.last() {
                    if last != ".." && last != "/" {
                        parts.pop();
                    } else {
                        parts.push("..".into());
                    }
                } else {
                    parts.push("..".into());
                }
            }
            Component::RootDir => parts.push("/".into()),
            Component::Prefix(p) => parts.push(p.as_os_str().to_string_lossy()),
            Component::Normal(s) => parts.push(s.to_string_lossy()),
        }
    }

    // rebuild
    let mut out = PathBuf::new();
    for (i, p) in parts.iter().enumerate() {
        if i == 0 && p.as_ref() == "/" {
            out.push(Path::new("/"));
            continue;
        }
        out.push(p.as_ref());
    }
    out
}

/// Ensure parent directory exists for a file path.
pub fn ensure_parent_dir(path: &Path) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Make a displayable path relative to `base` if possible (lexical).
pub fn display_rel(path: &Path, base: &Path) -> String {
    if let Ok(p) = path.strip_prefix(base) {
        p.display().to_string()
    } else {
        path.display().to_string()
    }
}

/* ============================== time helpers ============================== */

pub fn now_system() -> SystemTime {
    SystemTime::now()
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct Stopwatch {
    start: Instant,
}

impl Stopwatch {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.elapsed().as_millis()
    }
}

/* ============================== collections helpers ============================== */

/// Dedup preserving order, using a BTreeSet (deterministic, but O(n log n)).
pub fn dedup_stable(items: &mut Vec<String>) {
    let mut seen = BTreeSet::<String>::new();
    items.retain(|x| seen.insert(x.clone()));
}

pub fn map_insert_if_absent(map: &mut BTreeMap<String, String>, k: &str, v: &str) -> bool {
    if map.contains_key(k) {
        false
    } else {
        map.insert(k.to_string(), v.to_string());
        true
    }
}

/* ============================== env helpers ============================== */

pub fn env_get(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

pub fn env_get_or(key: &str, default: &str) -> String {
    env_get(key).unwrap_or_else(|| default.to_string())
}

pub fn env_get_bool(key: &str, default: bool) -> bool {
    match env_get(key).as_deref().map(|s| s.trim().to_ascii_lowercase()) {
        Some(ref s) if s == "1" || s == "true" || s == "yes" || s == "on" => true,
        Some(ref s) if s == "0" || s == "false" || s == "no" || s == "off" => false,
        Some(_) => default,
        None => default,
    }
}

pub fn env_get_u64(key: &str, default: u64) -> u64 {
    env_get(key)
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

/* ============================== hashing helpers ============================== */

#[derive(Default)]
pub struct Fnv1aHasher {
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

/// Stable 64-bit hash of a string (fnv1a).
pub fn hash_str64(s: &str) -> u64 {
    let mut h = Fnv1aHasher::default();
    h.write(s.as_bytes());
    h.finish()
}

/// Stable ID from path-like input (normalized separator).
pub fn hash_path64(p: &Path) -> u64 {
    let s = p.to_string_lossy().replace('\\', "/");
    hash_str64(&s)
}

/* ============================== formatting helpers ============================== */

pub fn fmt_kv(map: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (k, v) in map {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_works() {
        assert_eq!(normalize_key("  A   B\tC ", true), "a b c");
        assert_eq!(normalize_key("X  Y", false), "X Y");
    }

    #[test]
    fn join_non_empty_works() {
        let s = join_non_empty(["a", "", "b", "  ", "c"], ",");
        assert_eq!(s, "a,b,c");
    }

    #[test]
    fn fnv_hash_stable() {
        assert_eq!(hash_str64("x"), hash_str64("x"));
        assert_ne!(hash_str64("x"), hash_str64("y"));
    }

    #[test]
    fn dedup_stable_works() {
        let mut v = vec!["a".to_string(), "b".to_string(), "a".to_string()];
        dedup_stable(&mut v);
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
    }
}
