//! Store index (index.rs) — MAX (std-only).
//!
//! This module provides a lightweight, deterministic index for Steel Store.
//! It is designed to:
//! - map logical keys (artifact path / label) -> CAS digest
//! - provide a stable on-disk serialization (line-based, merge-friendly)
//! - support fast "roots" extraction for GC / bundling
//!
//! Intended integration:
//! - Bake graph produces artifacts -> CAS digest -> index records
//! - MFF writer uses index to enumerate digests for packing
//! - GC uses index roots to mark reachable digests
//!
//! Format (v1, UTF-8 text):
//!   # steel-store-index v1
//!   algo=<fnv1a64|sha256>
//!   <key>\t<algo:hex>\t<size>\t<kind>\t<note>
//!
//! Where:
//! - key: logical identifier, usually unix-like relative path (e.g. "build/app.exe")
//! - digest: "algo:hex" (algo must match index algo unless `allow_mixed_algo`)
//! - size: u64 (bytes) best-effort
//! - kind: "blob" | "artifact" | "log" | "meta" (freeform string accepted)
//! - note: optional string (may contain spaces; tabs are not allowed)
//!
//! Parsing is tolerant:
//! - unknown lines starting with '#' are ignored
//! - missing columns are allowed (defaults)
//! - extra columns are merged into note
//!
//! Determinism:
//! - serialization sorts keys lexicographically
//! - stable line endings '\n'

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::store::cas::{Digest, DigestAlgo};

#[derive(Debug)]
pub enum IndexError {
    Io(io::Error),
    Invalid(&'static str),
    Parse(String),
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IndexError::Io(e) => write!(f, "io: {e}"),
            IndexError::Invalid(s) => write!(f, "invalid: {s}"),
            IndexError::Parse(s) => write!(f, "parse: {s}"),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<io::Error> for IndexError {
    fn from(e: io::Error) -> Self {
        IndexError::Io(e)
    }
}

fn perr(msg: impl Into<String>) -> IndexError {
    IndexError::Parse(msg.into())
}

/// Kind tag for entries (freeform allowed; these are just defaults).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    Blob,
    Artifact,
    Log,
    Meta,
    Other(String),
}

impl EntryKind {
    pub fn as_str(&self) -> &str {
        match self {
            EntryKind::Blob => "blob",
            EntryKind::Artifact => "artifact",
            EntryKind::Log => "log",
            EntryKind::Meta => "meta",
            EntryKind::Other(s) => s.as_str(),
        }
    }

    pub fn parse(s: &str) -> EntryKind {
        match s {
            "blob" => EntryKind::Blob,
            "artifact" => EntryKind::Artifact,
            "log" => EntryKind::Log,
            "meta" => EntryKind::Meta,
            other => EntryKind::Other(other.to_string()),
        }
    }
}

/// A single index record.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub key: String,
    pub digest: Digest,
    pub size: u64,
    pub kind: EntryKind,
    pub note: String,
}

impl IndexEntry {
    pub fn new(key: impl Into<String>, digest: Digest) -> Self {
        Self {
            key: key.into(),
            digest,
            size: 0,
            kind: EntryKind::Blob,
            note: String::new(),
        }
    }
}

/// Store index structure.
#[derive(Debug, Clone)]
pub struct StoreIndex {
    pub algo: DigestAlgo,
    pub entries: BTreeMap<String, IndexEntry>,
    /// Optional metadata lines (key->value) preserved and serialized.
    pub meta: BTreeMap<String, String>,
}

impl StoreIndex {
    pub fn new(algo: DigestAlgo) -> Self {
        let mut meta = BTreeMap::new();
        meta.insert("format".into(), "steel-store-index v1".into());
        Self {
            algo,
            entries: BTreeMap::new(),
            meta,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, key: &str) -> Option<&IndexEntry> {
        self.entries.get(key)
    }

    pub fn upsert(&mut self, mut e: IndexEntry) -> Result<(), IndexError> {
        if e.key.is_empty() {
            return Err(IndexError::Invalid("empty key"));
        }
        if e.key.contains('\t') || e.note.contains('\t') {
            return Err(IndexError::Invalid("tabs not allowed in key/note"));
        }
        if e.digest.algo != self.algo {
            return Err(IndexError::Invalid("digest algo mismatch vs index algo"));
        }
        // Normalize kind note
        if e.note.contains('\n') || e.note.contains('\r') {
            e.note = e.note.replace('\r', " ").replace('\n', " ");
        }
        self.entries.insert(e.key.clone(), e);
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> Option<IndexEntry> {
        self.entries.remove(key)
    }

    /// Return the set of all digests referenced by entries.
    pub fn referenced_digests(&self) -> BTreeSet<Digest> {
        self.entries.values().map(|e| e.digest.clone()).collect()
    }

    /// Extract GC roots.
    /// In this simple model: all digests in the index are roots.
    /// If you later introduce an object graph (manifests -> children), change this.
    pub fn roots_for_gc(&self) -> Vec<Digest> {
        self.entries.values().map(|e| e.digest.clone()).collect()
    }

    /// Merge another index into this one.
    /// Policy:
    /// - same algo required
    /// - entries override by key
    pub fn merge_from(&mut self, other: &StoreIndex) -> Result<(), IndexError> {
        if other.algo != self.algo {
            return Err(IndexError::Invalid("cannot merge indexes with different algo"));
        }
        for (k, v) in &other.meta {
            self.meta.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, e) in &other.entries {
            self.entries.insert(k.clone(), e.clone());
        }
        Ok(())
    }

    /* ---------------------------- Disk I/O ---------------------------- */

    pub fn read(path: impl AsRef<Path>) -> Result<Self, IndexError> {
        let mut f = fs::File::open(path)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Self::parse(&s)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), IndexError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp = tmp_path_next_to(path);
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(self.serialize().as_bytes())?;
            f.flush()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn parse(s: &str) -> Result<Self, IndexError> {
        let mut algo: Option<DigestAlgo> = None;
        let mut meta: BTreeMap<String, String> = BTreeMap::new();
        let mut entries: BTreeMap<String, IndexEntry> = BTreeMap::new();

        for (lineno, line) in s.lines().enumerate() {
            let lno = lineno + 1;
            let line = line.trim_end();

            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                // allow "header" comment to be stored
                if line.contains("steel-store-index") {
                    meta.insert("format".into(), "steel-store-index v1".into());
                }
                continue;
            }

            // meta: key=value
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if k == "algo" {
                    algo = Some(parse_algo(v).ok_or_else(|| perr(format!("line {lno}: invalid algo")))?);
                } else {
                    meta.insert(k.to_string(), v.to_string());
                }
                continue;
            }

            // entry: tab-separated
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.is_empty() {
                continue;
            }

            let key = cols.get(0).map(|x| x.trim()).unwrap_or("");
            if key.is_empty() {
                return Err(perr(format!("line {lno}: empty key")));
            }

            let digest_s = cols.get(1).map(|x| x.trim()).unwrap_or("");
            if digest_s.is_empty() {
                return Err(perr(format!("line {lno}: missing digest")));
            }

            let (d_algo, d_hex) = parse_digest_token(digest_s)
                .ok_or_else(|| perr(format!("line {lno}: invalid digest token")))?;

            if algo.is_none() {
                algo = Some(d_algo);
            }
            let idx_algo = algo.unwrap();

            if d_algo != idx_algo {
                return Err(perr(format!(
                    "line {lno}: digest algo {d_algo:?} differs from index algo {idx_algo:?}"
                )));
            }

            let size = cols
                .get(2)
                .and_then(|x| x.trim().parse::<u64>().ok())
                .unwrap_or(0);

            let kind = cols.get(3).map(|x| EntryKind::parse(x.trim())).unwrap_or(EntryKind::Blob);

            let note = if cols.len() >= 5 {
                // join remaining columns with tabs removed (should not exist, but be safe)
                cols[4..].join(" ").replace('\t', " ")
            } else {
                String::new()
            };

            let digest = Digest {
                algo: idx_algo,
                hex: d_hex.to_string(),
            };

            entries.insert(
                key.to_string(),
                IndexEntry {
                    key: key.to_string(),
                    digest,
                    size,
                    kind,
                    note,
                },
            );
        }

        let algo = algo.unwrap_or(DigestAlgo::Fnv1a64);
        if meta.get("format").is_none() {
            meta.insert("format".into(), "steel-store-index v1".into());
        }

        Ok(Self { algo, entries, meta })
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("# steel-store-index v1\n");
        out.push_str(&format!("algo={}\n", self.algo.as_str()));

        // meta (excluding reserved)
        for (k, v) in &self.meta {
            if k == "format" || k == "algo" {
                continue;
            }
            if k.contains('\n') || k.contains('\r') || k.contains('=') {
                continue;
            }
            let mut v = v.clone();
            v = v.replace('\r', " ").replace('\n', " ");
            out.push_str(k);
            out.push('=');
            out.push_str(&v);
            out.push('\n');
        }

        // entries sorted by BTreeMap
        for (k, e) in &self.entries {
            // key \t algo:hex \t size \t kind \t note
            out.push_str(k);
            out.push('\t');
            out.push_str(e.digest.algo.as_str());
            out.push(':');
            out.push_str(&e.digest.hex);
            out.push('\t');
            out.push_str(&e.size.to_string());
            out.push('\t');
            out.push_str(e.kind.as_str());
            out.push('\t');
            out.push_str(&e.note.replace('\t', " ").replace('\r', " ").replace('\n', " "));
            out.push('\n');
        }

        out
    }
}

/* ------------------------------ Helpers ------------------------------ */

fn parse_algo(s: &str) -> Option<DigestAlgo> {
    match s {
        "fnv1a64" => Some(DigestAlgo::Fnv1a64),
        "sha256" => Some(DigestAlgo::Sha256),
        _ => None,
    }
}

fn parse_digest_token(s: &str) -> Option<(DigestAlgo, String)> {
    let (a, hex) = s.split_once(':')?;
    let algo = parse_algo(a)?;
    let hex = hex.trim();
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((algo, hex.to_ascii_lowercase()))
}

fn tmp_path_next_to(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    let file = path.file_name().and_then(|s| s.to_str()).unwrap_or("index");
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_nanos();
    p.set_file_name(format!(".{file}.{pid}.{ts}.tmp"));
    p
}

/* --------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_serialize_parse() {
        let mut idx = StoreIndex::new(DigestAlgo::Fnv1a64);
        idx.meta.insert("store".into(), "local".into());

        let d = Digest { algo: DigestAlgo::Fnv1a64, hex: "0011223344556677".into() };
        let mut e = IndexEntry::new("build/app.bin", d);
        e.size = 123;
        e.kind = EntryKind::Artifact;
        e.note = "hello".into();
        idx.upsert(e).unwrap();

        let txt = idx.serialize();
        let idx2 = StoreIndex::parse(&txt).unwrap();

        assert_eq!(idx2.algo, DigestAlgo::Fnv1a64);
        assert_eq!(idx2.len(), 1);
        let got = idx2.get("build/app.bin").unwrap();
        assert_eq!(got.size, 123);
        assert_eq!(got.kind.as_str(), "artifact");
        assert_eq!(got.digest.hex, "0011223344556677");
    }

    #[test]
    fn parse_tolerant_columns() {
        let s = "\
# steel-store-index v1
algo=fnv1a64
a\tfnv1a64:0011223344556677
";
        let idx = StoreIndex::parse(s).unwrap();
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get("a").unwrap().size, 0);
    }
}
