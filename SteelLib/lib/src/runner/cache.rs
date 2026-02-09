// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\runner\cache.rs

//! Cache / store layer for Steel runner.
//!
//! This module provides a deterministic, content-addressed cache used during
//! the execution phase (build vitte / runner).
//!
//! Design goals:
//! - deterministic paths
//! - content-addressed (hash-based)
//! - portable across machines
//! - policy-driven (on/off/readonly/strict in future)

use crate::error::SteelError;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// Cache root (relative to workspace or target dir).
pub const DEFAULT_CACHE_DIR: &str = "target/cache";

/// A cache entry identified by a content hash.
#[derive(Debug, Clone)]
pub struct CacheKey {
    pub hash: String,
}

impl CacheKey {
    pub fn new(hash: String) -> Self {
        Self { hash }
    }
}

/// Cache handle.
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// Create a cache rooted at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { root: path.into() }
    }

    /// Return the on-disk path for a cache key.
    pub fn entry_path(&self, key: &CacheKey) -> PathBuf {
        // Split hash for fanout: ab/cd/abcdef...
        let h = &key.hash;
        let (a, b) = (&h[0..2], &h[2..4]);
        self.root.join(a).join(b).join(h)
    }

    /// Check whether a cache entry exists.
    pub fn contains(&self, key: &CacheKey) -> bool {
        self.entry_path(key).exists()
    }

    /// Store a file in the cache (content-addressed).
    pub fn store_file(&self, src: &Path) -> Result<CacheKey, SteelError> {
        let hash = hash_file(src)?;
        let key = CacheKey::new(hash.clone());
        let dst = self.entry_path(&key);

        if dst.exists() {
            return Ok(key);
        }

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(src, &dst)?;
        Ok(key)
    }

    /// Restore a cached file to a destination path.
    pub fn restore_file(&self, key: &CacheKey, dst: &Path) -> Result<(), SteelError> {
        let src = self.entry_path(key);
        if !src.exists() {
            return Err(SteelError::ExecutionFailed(format!(
                "cache miss for key {}",
                key.hash
            )));
        }

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(src, dst)?;
        Ok(())
    }
}

/// Compute SHA-256 hash of a file (hex-encoded).
pub fn hash_file(path: &Path) -> Result<String, SteelError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn cache_roundtrip_file() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = Cache::new(&cache_dir);

        let src = dir.path().join("input.txt");
        let mut f = fs::File::create(&src).unwrap();
        writeln!(f, "hello steel").unwrap();

        let key = cache.store_file(&src).unwrap();
        assert!(cache.contains(&key));

        let dst = dir.path().join("output.txt");
        cache.restore_file(&key, &dst).unwrap();

        let content = fs::read_to_string(dst).unwrap();
        assert!(content.contains("hello steel"));
    }
}
