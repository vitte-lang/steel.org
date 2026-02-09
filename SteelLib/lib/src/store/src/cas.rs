//! Content-Addressable Storage (CAS) for Steel Store (cas.rs) — MAX (std-only).
//!
//! Goals:
//! - Store blobs by content hash (digest) under a directory root.
//! - Deterministic layout across platforms.
//! - Atomic writes, safe concurrent readers.
//! - Optional compression (stubbed; std-only).
//! - Streaming I/O for large blobs.
//!
//! Typical usage:
//! - Bake steps produce artifacts -> stored in CAS
//! - MFF writer references CAS digests
//! - Decompile can rehydrate artifacts from CAS
//!
//! Layout (default):
//!   <root>/cas/v1/<algo>/<aa>/<bb>/<digest>.blob
//! where <aa> is first 2 hex chars, <bb> next 2.
//!
//! Hash algorithm:
//! - std-only does not provide SHA-256.
//! - We implement a stable, non-cryptographic 64-bit FNV-1a digest for now.
//! - API is written to support real crypto digests later (sha256, blake3, ...).
//!
//! IMPORTANT:
//! - If you need integrity/security guarantees, integrate a cryptographic hash
//!   behind a feature flag, and set `DigestAlgo::Sha256` as default in release.
//!
//! Concurrency:
//! - `put_*` writes to temp file then `rename` to final path.
//! - If file already exists, it is not overwritten.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum CasError {
    Io(io::Error),
    Invalid(&'static str),
    Msg(String),
}

impl fmt::Display for CasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CasError::Io(e) => write!(f, "io: {e}"),
            CasError::Invalid(s) => write!(f, "invalid: {s}"),
            CasError::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for CasError {}

impl From<io::Error> for CasError {
    fn from(e: io::Error) -> Self {
        CasError::Io(e)
    }
}

fn cerr(msg: impl Into<String>) -> CasError {
    CasError::Msg(msg.into())
}

/// Digest algorithm supported by this CAS instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DigestAlgo {
    /// FNV-1a 64-bit (std-only fallback; not cryptographic).
    Fnv1a64,
    /// Placeholder for future integration.
    Sha256,
}

impl DigestAlgo {
    pub fn as_str(self) -> &'static str {
        match self {
            DigestAlgo::Fnv1a64 => "fnv1a64",
            DigestAlgo::Sha256 => "sha256",
        }
    }

    pub fn is_crypto(self) -> bool {
        matches!(self, DigestAlgo::Sha256)
    }
}

/// Digest value as lowercase hex string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest {
    pub algo: DigestAlgo,
    /// Lowercase hex string (no 0x), fixed width for algo if known.
    pub hex: String,
}

impl Digest {
    pub fn new(algo: DigestAlgo, hex: String) -> Result<Self, CasError> {
        if hex.is_empty() {
            return Err(CasError::Invalid("empty digest"));
        }
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CasError::Invalid("digest not hex"));
        }
        Ok(Self {
            algo,
            hex: hex.to_ascii_lowercase(),
        })
    }

    pub fn short(&self) -> String {
        let n = self.hex.len().min(12);
        self.hex[..n].to_string()
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algo.as_str(), self.hex)
    }
}

/// CAS configuration.
#[derive(Debug, Clone)]
pub struct CasConfig {
    /// Root directory of the store.
    pub root: PathBuf,
    /// CAS subdir within root.
    pub cas_dir_name: String,
    /// CAS version dir.
    pub version: String,
    /// Digest algorithm.
    pub algo: DigestAlgo,
    /// Subdir fanout: number of hex chars in first level (default 2).
    pub fanout_a: usize,
    /// Subdir fanout: number of hex chars in second level (default 2).
    pub fanout_b: usize,
}

impl Default for CasConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from(".steel-store"),
            cas_dir_name: "cas".into(),
            version: "v1".into(),
            algo: DigestAlgo::Fnv1a64,
            fanout_a: 2,
            fanout_b: 2,
        }
    }
}

/// CAS instance.
#[derive(Debug, Clone)]
pub struct Cas {
    cfg: CasConfig,
}

impl Cas {
    pub fn new(cfg: CasConfig) -> Result<Self, CasError> {
        if cfg.fanout_a + cfg.fanout_b > 32 {
            return Err(CasError::Invalid("fanout too large"));
        }
        Ok(Self { cfg })
    }

    pub fn config(&self) -> &CasConfig {
        &self.cfg
    }

    /// Ensure CAS directories exist.
    pub fn ensure_dirs(&self) -> Result<(), CasError> {
        fs::create_dir_all(self.base_dir())?;
        Ok(())
    }

    /// Base directory: <root>/<cas>/<version>/<algo>
    pub fn base_dir(&self) -> PathBuf {
        self.cfg
            .root
            .join(&self.cfg.cas_dir_name)
            .join(&self.cfg.version)
            .join(self.cfg.algo.as_str())
    }

    /// Path for a given digest.
    pub fn path_for(&self, d: &Digest) -> Result<PathBuf, CasError> {
        if d.algo != self.cfg.algo {
            return Err(cerr(format!("digest algo mismatch: have {}, want {}", d.algo.as_str(), self.cfg.algo.as_str())));
        }

        let hex = &d.hex;
        if hex.len() < self.cfg.fanout_a + self.cfg.fanout_b {
            return Err(CasError::Invalid("digest too short for fanout"));
        }

        let a = &hex[..self.cfg.fanout_a];
        let b = &hex[self.cfg.fanout_a..self.cfg.fanout_a + self.cfg.fanout_b];

        Ok(self
            .base_dir()
            .join(a)
            .join(b)
            .join(format!("{hex}.blob")))
    }

    /// Check existence.
    pub fn exists(&self, d: &Digest) -> Result<bool, CasError> {
        Ok(self.path_for(d)?.exists())
    }

    /// Open for reading.
    pub fn open(&self, d: &Digest) -> Result<fs::File, CasError> {
        Ok(fs::File::open(self.path_for(d)?)?)
    }

    /// Read whole blob into memory (use carefully).
    pub fn get_bytes(&self, d: &Digest) -> Result<Vec<u8>, CasError> {
        let mut f = self.open(d)?;
        let mut v = Vec::new();
        f.read_to_end(&mut v)?;
        Ok(v)
    }

    /// Store bytes. Returns digest.
    pub fn put_bytes(&self, data: &[u8]) -> Result<Digest, CasError> {
        self.ensure_dirs()?;
        let d = digest_bytes(self.cfg.algo, data)?;
        self.put_bytes_with_digest(&d, data)?;
        Ok(d)
    }

    /// Store from a reader (streaming). Returns digest.
    pub fn put_reader<R: Read>(&self, mut r: R) -> Result<Digest, CasError> {
        self.ensure_dirs()?;

        // Stream into temp file while hashing.
        let tmp = self.tmp_path()?;
        if let Some(parent) = tmp.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut f = fs::File::create(&tmp)?;
        let mut hasher = Hasher::new(self.cfg.algo)?;

        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            f.write_all(&buf[..n])?;
        }
        f.flush()?;

        let d = hasher.finish()?;
        self.commit_tmp_as_digest(&tmp, &d)?;
        Ok(d)
    }

    /// Store file (streaming). Returns digest.
    pub fn put_file(&self, path: impl AsRef<Path>) -> Result<Digest, CasError> {
        let f = fs::File::open(path)?;
        self.put_reader(f)
    }

    /// Store bytes but using an externally provided digest (caller asserted).
    pub fn put_bytes_with_digest(&self, d: &Digest, data: &[u8]) -> Result<(), CasError> {
        self.ensure_dirs()?;
        let final_path = self.path_for(d)?;
        if final_path.exists() {
            return Ok(());
        }
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp = self.tmp_path()?;
        if let Some(parent) = tmp.parent() {
            fs::create_dir_all(parent)?;
        }

        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(data)?;
            f.flush()?;
        }

        self.atomic_rename(&tmp, &final_path)?;
        Ok(())
    }

    /// Remove a blob (dangerous). Returns whether it existed.
    pub fn remove(&self, d: &Digest) -> Result<bool, CasError> {
        let p = self.path_for(d)?;
        if p.exists() {
            fs::remove_file(p)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Garbage-collect helper: list all blobs under base dir.
    pub fn list_all(&self) -> Result<Vec<PathBuf>, CasError> {
        let base = self.base_dir();
        if !base.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        walk_files(&base, &mut out)?;
        out.sort();
        Ok(out)
    }

    /// Parse a digest from a string like `algo:hex` or just `hex` (uses config algo).
    pub fn parse_digest(&self, s: &str) -> Result<Digest, CasError> {
        if let Some((a, hex)) = s.split_once(':') {
            let algo = match a {
                "fnv1a64" => DigestAlgo::Fnv1a64,
                "sha256" => DigestAlgo::Sha256,
                _ => return Err(CasError::Invalid("unknown digest algo")),
            };
            Digest::new(algo, hex.to_string())
        } else {
            Digest::new(self.cfg.algo, s.to_string())
        }
    }

    /* -------------------------- internals -------------------------- */

    fn tmp_path(&self) -> Result<PathBuf, CasError> {
        // <root>/<cas>/<version>/.tmp/<pid>-<ts>-<counter>.tmp
        // std-only: no monotonic clock; use system time + pid best-effort.
        let pid = std::process::id();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();

        Ok(self
            .cfg
            .root
            .join(&self.cfg.cas_dir_name)
            .join(&self.cfg.version)
            .join(".tmp")
            .join(format!("{pid}-{ts}.tmp")))
    }

    fn commit_tmp_as_digest(&self, tmp: &Path, d: &Digest) -> Result<(), CasError> {
        let final_path = self.path_for(d)?;
        if final_path.exists() {
            // Another writer already stored it. Remove tmp.
            let _ = fs::remove_file(tmp);
            return Ok(());
        }
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        self.atomic_rename(tmp, &final_path)?;
        Ok(())
    }

    fn atomic_rename(&self, from: &Path, to: &Path) -> Result<(), CasError> {
        // On Windows, rename fails if target exists.
        // We do: if exists -> remove tmp; else rename.
        if to.exists() {
            let _ = fs::remove_file(from);
            return Ok(());
        }
        fs::rename(from, to)?;
        Ok(())
    }
}

/* ------------------------------ Hashing ------------------------------ */

/// Streaming hasher interface (std-only fallback).
struct Hasher {
    algo: DigestAlgo,
    state: HasherState,
}

enum HasherState {
    Fnv1a64 { h: u64 },
    Sha256Unsupported,
}

impl Hasher {
    fn new(algo: DigestAlgo) -> Result<Self, CasError> {
        let state = match algo {
            DigestAlgo::Fnv1a64 => HasherState::Fnv1a64 { h: FNV_OFFSET },
            DigestAlgo::Sha256 => HasherState::Sha256Unsupported,
        };
        Ok(Self { algo, state })
    }

    fn update(&mut self, bytes: &[u8]) {
        match &mut self.state {
            HasherState::Fnv1a64 { h } => {
                for b in bytes {
                    *h ^= *b as u64;
                    *h = h.wrapping_mul(FNV_PRIME);
                }
            }
            HasherState::Sha256Unsupported => {}
        }
    }

    fn finish(self) -> Result<Digest, CasError> {
        match self.state {
            HasherState::Fnv1a64 { h } => {
                let hex = format!("{:016x}", h);
                Digest::new(self.algo, hex)
            }
            HasherState::Sha256Unsupported => Err(CasError::Invalid("sha256 unsupported (std-only)")),
        }
    }
}

const FNV_OFFSET: u64 = 14695981039346656037;
const FNV_PRIME: u64 = 1099511628211;

pub fn digest_bytes(algo: DigestAlgo, bytes: &[u8]) -> Result<Digest, CasError> {
    let mut h = Hasher::new(algo)?;
    h.update(bytes);
    h.finish()
}

/* ------------------------------ Walk helpers ------------------------------ */

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CasError> {
    let mut kids: Vec<PathBuf> = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        kids.push(e.path());
    }
    kids.sort();

    for p in kids {
        let md = fs::symlink_metadata(&p)?;
        let ft = md.file_type();
        if ft.is_dir() {
            walk_files(&p, out)?;
        } else if ft.is_file() {
            out.push(p);
        }
    }
    Ok(())
}

/* --------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        p.push(format!("steel_cas_test_{pid}_{ts}"));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn put_get_roundtrip() {
        let root = tmp_root();
        let cas = Cas::new(CasConfig {
            root: root.clone(),
            ..CasConfig::default()
        })
        .unwrap();

        let d = cas.put_bytes(b"hello").unwrap();
        assert!(cas.exists(&d).unwrap());
        let v = cas.get_bytes(&d).unwrap();
        assert_eq!(v, b"hello");
    }

    #[test]
    fn put_is_dedup() {
        let root = tmp_root();
        let cas = Cas::new(CasConfig {
            root: root.clone(),
            ..CasConfig::default()
        })
        .unwrap();

        let a = cas.put_bytes(b"same").unwrap();
        let b = cas.put_bytes(b"same").unwrap();
        assert_eq!(a, b);

        let list = cas.list_all().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn parse_digest_formats() {
        let cas = Cas::new(CasConfig::default()).unwrap();
        let d1 = cas.parse_digest("fnv1a64:0011223344556677").unwrap();
        assert_eq!(d1.algo, DigestAlgo::Fnv1a64);
        let d2 = cas.parse_digest("0011223344556677").unwrap();
        assert_eq!(d2.algo, DigestAlgo::Fnv1a64);
    }
}
