// src/strcache.rs
//
// Steel — string interning / string cache
//
// Purpose:
// - Provide a fast, deterministic string cache for repeated identifiers/paths/keys.
// - Reduce allocations and enable pointer-like equality via stable Symbol IDs.
// - Support:
//   - immutable interned strings (Symbol -> &'static-like via Arc<str>)
//   - fast hashing and comparisons
//   - optional reverse lookup (Symbol -> str)
//   - optional "freeze" to prevent new inserts after planning (debug/correctness)
//
// Design goals:
// - No external deps.
// - Thread-safe option (StrCacheSync) and single-thread (StrCache).
// - Stable symbol ids within a process.
// - Avoid leaking memory: uses Arc<str> (strings live while cache lives).
//
// Typical usage:
//   let mut sc = StrCache::new();
//   let sym = sc.intern("hello");
//   assert_eq!(sc.resolve(sym), Some("hello"));
//
// If you need global interning across threads, use StrCacheSync.
//
// Notes:
// - This is not a perfect "string interner" for long-lived global state. It's intended as
//   a component inside Steel planning/execution where cache lifetime is bounded.

#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/* ============================== symbol ============================== */

/// Interned string id.
/// - 0 is reserved as INVALID.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(u32);

impl Symbol {
    pub const INVALID: Symbol = Symbol(0);

    #[inline]
    pub fn new(raw: u32) -> Self {
        Symbol(raw)
    }

    #[inline]
    pub fn raw(self) -> u32 {
        self.0
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_valid() {
            write!(f, "Symbol({})", self.0)
        } else {
            write!(f, "Symbol(INVALID)")
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_valid() {
            write!(f, "{}", self.0)
        } else {
            f.write_str("INVALID")
        }
    }
}

/* ============================== cache core ============================== */

#[derive(Debug, Clone)]
pub struct StrCacheStats {
    pub strings: usize,
    pub bytes: usize,
    pub frozen: bool,
}

#[derive(Debug, Clone)]
pub struct StrCacheOptions {
    /// If true, `intern` returns INVALID when frozen instead of error.
    pub return_invalid_on_frozen: bool,
    /// If true, trims input before interning.
    pub trim: bool,
    /// Max string length accepted (0 = unlimited).
    pub max_len: usize,
}

impl Default for StrCacheOptions {
    fn default() -> Self {
        Self {
            return_invalid_on_frozen: false,
            trim: false,
            max_len: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrCacheError {
    Frozen,
    TooLong { max_len: usize, actual: usize },
}

impl fmt::Display for StrCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StrCacheError::Frozen => write!(f, "string cache is frozen"),
            StrCacheError::TooLong { max_len, actual } => write!(f, "string too long: {actual} > {max_len}"),
        }
    }
}

impl std::error::Error for StrCacheError {}

/// Single-thread string cache.
pub struct StrCache {
    opts: StrCacheOptions,
    frozen: bool,

    // forward: string -> symbol
    map: HashMap<Arc<str>, Symbol, ArcStrBuildHasher>,

    // reverse: symbol -> string (index = raw-1)
    vec: Vec<Arc<str>>,

    bytes: usize,
}

impl Default for StrCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StrCache {
    pub fn new() -> Self {
        Self::with_options(StrCacheOptions::default())
    }

    pub fn with_options(opts: StrCacheOptions) -> Self {
        Self {
            opts,
            frozen: false,
            map: HashMap::with_capacity_and_hasher(1024, ArcStrBuildHasher::default()),
            vec: Vec::with_capacity(1024),
            bytes: 0,
        }
    }

    pub fn options(&self) -> &StrCacheOptions {
        &self.opts
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn stats(&self) -> StrCacheStats {
        StrCacheStats {
            strings: self.vec.len(),
            bytes: self.bytes,
            frozen: self.frozen,
        }
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Resolve a symbol to string slice.
    pub fn resolve(&self, sym: Symbol) -> Option<&str> {
        if !sym.is_valid() {
            return None;
        }
        let idx = (sym.raw() as usize).checked_sub(1)?;
        self.vec.get(idx).map(|s| s.as_ref())
    }

    /// Returns the owned Arc for a symbol (cheap clone).
    pub fn resolve_arc(&self, sym: Symbol) -> Option<Arc<str>> {
        if !sym.is_valid() {
            return None;
        }
        let idx = (sym.raw() as usize).checked_sub(1)?;
        self.vec.get(idx).cloned()
    }

    /// Find symbol without inserting.
    pub fn lookup<S: AsRef<str>>(&self, s: S) -> Option<Symbol> {
        let mut cow = Cow::Borrowed(s.as_ref());
        if self.opts.trim {
            cow = Cow::Owned(cow.trim().to_string());
        }
        if cow.is_empty() {
            return None;
        }
        // Build an Arc<str> key for lookup without allocating? Not possible without hash of str.
        // We accept allocating here only if needed by caller; provide `lookup_str` which avoids Arc.
        self.lookup_str(cow.as_ref())
    }

    /// Find symbol for &str without allocating an Arc by using a temporary lookup wrapper.
    pub fn lookup_str(&self, s: &str) -> Option<Symbol> {
        if s.is_empty() {
            return None;
        }
        let s = if self.opts.trim { s.trim() } else { s };
        if s.is_empty() {
            return None;
        }
        // Use equivalent hashing (Arc<str> key uses str hashing).
        self.map.get(s as &str).copied()
    }

    /// Intern string. Returns existing symbol if already present.
    pub fn intern<S: AsRef<str>>(&mut self, s: S) -> Result<Symbol, StrCacheError> {
        if self.frozen {
            if self.opts.return_invalid_on_frozen {
                return Ok(Symbol::INVALID);
            }
            return Err(StrCacheError::Frozen);
        }

        let mut cow = Cow::Borrowed(s.as_ref());
        if self.opts.trim {
            cow = Cow::Owned(cow.trim().to_string());
        }

        let st = cow.as_ref();
        if st.is_empty() {
            return Ok(Symbol::INVALID);
        }

        if self.opts.max_len > 0 {
            let n = st.chars().count();
            if n > self.opts.max_len {
                return Err(StrCacheError::TooLong { max_len: self.opts.max_len, actual: n });
            }
        }

        // Fast path: lookup by &str
        if let Some(sym) = self.lookup_str(st) {
            return Ok(sym);
        }

        // Insert
        let arc: Arc<str> = Arc::from(st);
        match self.map.entry(arc.clone()) {
            Entry::Occupied(o) => Ok(*o.get()),
            Entry::Vacant(v) => {
                let raw = (self.vec.len() as u32) + 1; // 1-based
                let sym = Symbol::new(raw);
                self.bytes += arc.len();
                self.vec.push(arc.clone());
                v.insert(sym);
                Ok(sym)
            }
        }
    }

    /// Intern many strings; returns symbols in order.
    pub fn intern_all<I, S>(&mut self, it: I) -> Result<Vec<Symbol>, StrCacheError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::new();
        for s in it {
            out.push(self.intern(s)?);
        }
        Ok(out)
    }

    /// Remove all interned strings (drops backing allocations).
    pub fn clear(&mut self) {
        self.map.clear();
        self.vec.clear();
        self.bytes = 0;
        self.frozen = false;
    }
}

/* ============================== thread-safe wrapper ============================== */

/// Thread-safe interner wrapper.
/// - Uses a Mutex around StrCache.
/// - Stable IDs per process, but order depends on interning timing across threads.
#[derive(Default)]
pub struct StrCacheSync {
    inner: Mutex<StrCache>,
}

impl StrCacheSync {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StrCache::new()),
        }
    }

    pub fn with_options(opts: StrCacheOptions) -> Self {
        Self {
            inner: Mutex::new(StrCache::with_options(opts)),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, StrCache> {
        self.inner.lock().expect("StrCacheSync mutex poisoned")
    }

    pub fn intern<S: AsRef<str>>(&self, s: S) -> Result<Symbol, StrCacheError> {
        self.lock().intern(s)
    }

    pub fn lookup_str(&self, s: &str) -> Option<Symbol> {
        self.lock().lookup_str(s)
    }

    pub fn resolve(&self, sym: Symbol) -> Option<String> {
        self.lock().resolve(sym).map(|s| s.to_string())
    }

    pub fn freeze(&self) {
        self.lock().freeze();
    }

    pub fn stats(&self) -> StrCacheStats {
        self.lock().stats()
    }
}

/* ============================== hashing ============================== */

/// Custom hasher builder optimized for Arc<str> keys by hashing underlying &str bytes.
/// This avoids accidentally hashing Arc pointer address.
#[derive(Default, Clone)]
struct ArcStrBuildHasher;

impl std::hash::BuildHasher for ArcStrBuildHasher {
    type Hasher = Fnv1aHasher;

    fn build_hasher(&self) -> Self::Hasher {
        Fnv1aHasher::default()
    }
}

/// Small deterministic hasher (FNV-1a 64-bit) to keep runtime stable across platforms.
/// Rust default SipHash is secure but slower and randomized (affects determinism in perf profiles).
#[derive(Default, Clone)]
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
    fn intern_and_resolve() {
        let mut sc = StrCache::new();
        let a = sc.intern("hello").unwrap();
        let b = sc.intern("hello").unwrap();
        let c = sc.intern("world").unwrap();

        assert!(a.is_valid());
        assert_eq!(a, b);
        assert_ne!(a, c);

        assert_eq!(sc.resolve(a), Some("hello"));
        assert_eq!(sc.resolve(c), Some("world"));
        assert_eq!(sc.resolve(Symbol::INVALID), None);
    }

    #[test]
    fn lookup_does_not_insert() {
        let mut sc = StrCache::new();
        assert_eq!(sc.lookup_str("x"), None);
        let x = sc.intern("x").unwrap();
        assert_eq!(sc.lookup_str("x"), Some(x));
        assert_eq!(sc.len(), 1);
    }

    #[test]
    fn freeze_blocks_intern() {
        let mut sc = StrCache::new();
        sc.intern("a").unwrap();
        sc.freeze();
        let err = sc.intern("b").unwrap_err();
        assert_eq!(err, StrCacheError::Frozen);
    }

    #[test]
    fn frozen_returns_invalid_if_configured() {
        let mut sc = StrCache::with_options(StrCacheOptions {
            return_invalid_on_frozen: true,
            trim: false,
            max_len: 0,
        });
        sc.intern("a").unwrap();
        sc.freeze();
        let sym = sc.intern("b").unwrap();
        assert_eq!(sym, Symbol::INVALID);
    }

    #[test]
    fn options_trim() {
        let mut sc = StrCache::with_options(StrCacheOptions {
            return_invalid_on_frozen: false,
            trim: true,
            max_len: 0,
        });
        let a = sc.intern("  hello ").unwrap();
        let b = sc.intern("hello").unwrap();
        assert_eq!(a, b);
        assert_eq!(sc.resolve(a), Some("hello"));
    }

    #[test]
    fn options_max_len() {
        let mut sc = StrCache::with_options(StrCacheOptions {
            return_invalid_on_frozen: false,
            trim: false,
            max_len: 3,
        });
        assert!(sc.intern("abcd").is_err());
        assert!(sc.intern("abc").is_ok());
    }

    #[test]
    fn sync_wrapper_basic() {
        let sc = StrCacheSync::new();
        let a = sc.intern("a").unwrap();
        let b = sc.intern("a").unwrap();
        assert_eq!(a, b);
        assert_eq!(sc.resolve(a).as_deref(), Some("a"));
    }
}
