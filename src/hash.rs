// src/hash.rs
//
// Steel — hash (stable hashing + fingerprints)
//
// Purpose:
// - Provide fast, dependency-free hashing primitives used for:
//   - rule ids, job ids
//   - cache keys / fingerprints
//   - content hashing (optional)
//   - deterministic build graph keys
//
// Features:
// - FNV-1a 64-bit (fast, stable across platforms)
// - SplitMix-like mixer (for key diffusion)
// - Fingerprint builder with typed updates
// - Optional file hashing helpers (read + hash) with size limit
//
// Notes:
// - Not cryptographically secure.
// - If you need crypto-grade hashing (SHA-256), wire an external crate; keep this as baseline.

#![allow(dead_code)]

use std::fmt;
use std::hash::Hasher;
use std::io;
use std::path::{Path, PathBuf};

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashError {
    Io { path: PathBuf, op: &'static str, message: String },
    TooLarge { path: PathBuf, limit: usize, actual: usize },
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashError::Io { path, op, message } => write!(f, "{} {}: {}", op, path.display(), message),
            HashError::TooLarge { path, limit, actual } => write!(
                f,
                "file too large {}: {} > {} bytes",
                path.display(),
                actual,
                limit
            ),
        }
    }
}

impl std::error::Error for HashError {}

fn io_err(path: &Path, op: &'static str, e: io::Error) -> HashError {
    HashError::Io {
        path: path.to_path_buf(),
        op,
        message: e.to_string(),
    }
}

/* ============================== fnv1a 64 ============================== */

pub const FNV_OFFSET_BASIS_64: u64 = 0xcbf29ce484222325;
pub const FNV_PRIME_64: u64 = 0x100000001b3;

#[derive(Clone)]
pub struct Fnv1a64 {
    state: u64,
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self { state: FNV_OFFSET_BASIS_64 }
    }
}

impl Fnv1a64 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.state = FNV_OFFSET_BASIS_64;
    }

    pub fn value(&self) -> u64 {
        self.state
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        let mut h = self.state;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME_64);
        }
        self.state = h;
    }

    pub fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }

    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_bool(&mut self, v: bool) {
        self.write_u8(if v { 1 } else { 0 });
    }

    pub fn write_str(&mut self, s: &str) {
        // length-delimited to avoid ambiguity
        self.write_u32(s.len() as u32);
        self.write_bytes(s.as_bytes());
    }

    pub fn write_path_norm(&mut self, p: &Path) {
        // normalize separators for cross-platform determinism
        let s = p.to_string_lossy().replace('\\', "/");
        self.write_str(&s);
    }
}

impl Hasher for Fnv1a64 {
    fn write(&mut self, bytes: &[u8]) {
        self.write_bytes(bytes);
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

/* ============================== mixing ============================== */

/// Non-crypto mixing (SplitMix64 finalizer style).
pub fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    x
}

/* ============================== one-shot hashing ============================== */

pub fn hash64_bytes(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_bytes(bytes);
    h.value()
}

pub fn hash64_str(s: &str) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_str(s);
    h.value()
}

pub fn hash64_path_norm(p: &Path) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_path_norm(p);
    h.value()
}

/* ============================== fingerprint builder ============================== */

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub raw: u64,
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.raw)
    }
}

#[derive(Clone)]
pub struct Fingerprinter {
    h: Fnv1a64,
}

impl Default for Fingerprinter {
    fn default() -> Self {
        Self { h: Fnv1a64::new() }
    }
}

impl Fingerprinter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_u64(&mut self, v: u64) -> &mut Self {
        self.h.write_u64(v);
        self
    }

    pub fn put_i64(&mut self, v: i64) -> &mut Self {
        self.h.write_i64(v);
        self
    }

    pub fn put_bool(&mut self, v: bool) -> &mut Self {
        self.h.write_bool(v);
        self
    }

    pub fn put_str(&mut self, s: &str) -> &mut Self {
        self.h.write_str(s);
        self
    }

    pub fn put_path(&mut self, p: &Path) -> &mut Self {
        self.h.write_path_norm(p);
        self
    }

    pub fn put_kv_sorted(&mut self, map: &std::collections::BTreeMap<String, String>) -> &mut Self {
        self.h.write_u32(map.len() as u32);
        for (k, v) in map {
            self.h.write_str(k);
            self.h.write_str(v);
        }
        self
    }

    pub fn finish(&self) -> Fingerprint {
        Fingerprint { raw: mix64(self.h.value()) }
    }
}

/* ============================== file hashing ============================== */

pub const DEFAULT_MAX_FILE_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

pub fn hash_file_fnv1a64(path: &Path) -> Result<u64, HashError> {
    hash_file_fnv1a64_limited(path, DEFAULT_MAX_FILE_BYTES)
}

pub fn hash_file_fnv1a64_limited(path: &Path, max_bytes: usize) -> Result<u64, HashError> {
    let mut f = std::fs::File::open(path).map_err(|e| io_err(path, "open", e))?;

    if let Ok(md) = f.metadata() {
        let len = md.len() as usize;
        if max_bytes > 0 && len > max_bytes {
            return Err(HashError::TooLarge {
                path: path.to_path_buf(),
                limit: max_bytes,
                actual: len,
            });
        }
    }

    let mut h = Fnv1a64::new();
    let mut buf = [0u8; 8192];
    let mut total = 0usize;

    loop {
        let n = f.read(&mut buf).map_err(|e| io_err(path, "read", e))?;
        if n == 0 {
            break;
        }
        total += n;
        if max_bytes > 0 && total > max_bytes {
            return Err(HashError::TooLarge {
                path: path.to_path_buf(),
                limit: max_bytes,
                actual: total,
            });
        }
        h.write_bytes(&buf[..n]);
    }

    Ok(h.value())
}

use std::io::Read;

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(hash64_str("x"), hash64_str("x"));
        assert_ne!(hash64_str("x"), hash64_str("y"));
    }

    #[test]
    fn fingerprint_builder() {
        let mut fp = Fingerprinter::new();
        fp.put_str("rule").put_u64(42).put_bool(true);
        let a = fp.finish().raw;
        let b = fp.finish().raw;
        assert_eq!(a, b);
    }

    #[test]
    fn mix_changes_distribution() {
        let a = mix64(1);
        let b = mix64(2);
        assert_ne!(a, b);
    }
}
