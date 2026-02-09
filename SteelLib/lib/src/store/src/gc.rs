//! Garbage Collection (GC) for Steel Store (gc.rs) — MAX (std-only).
//!
//! This module provides GC for a CAS-like store:
//! - mark phase: compute reachable digests from a set of roots (manifests, mff bundles, indexes)
//! - sweep phase: delete unreferenced blobs
//! - optional dry-run and reporting
//! - deterministic traversal and stable output
//!
//! Integration points (expected elsewhere in your repo):
//! - store index (e.g. `store/index` or `mff/index`) that can enumerate digests used by bundles
//! - a "roots provider" that lists the active bundles/manifests in the store
//!
//! Since this is std-only and your higher-level schemas are in other crates,
//! this file defines a generic `RootsProvider` and a default filesystem-based
//! provider that treats certain files as "root references" containing digests.
//!
//! Default root format supported here:
//! - text files containing digests, one per line
//! - optional prefix `algo:hex` supported
//!
//! You can adapt `scan_roots_*` to your real schema later.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::store::cas::{Cas, CasError, Digest, DigestAlgo};

#[derive(Debug)]
pub enum GcError {
    Io(io::Error),
    Cas(CasError),
    Invalid(&'static str),
    Msg(String),
}

impl fmt::Display for GcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GcError::Io(e) => write!(f, "io: {e}"),
            GcError::Cas(e) => write!(f, "cas: {e}"),
            GcError::Invalid(s) => write!(f, "invalid: {s}"),
            GcError::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for GcError {}

impl From<io::Error> for GcError {
    fn from(e: io::Error) -> Self {
        GcError::Io(e)
    }
}

impl From<CasError> for GcError {
    fn from(e: CasError) -> Self {
        GcError::Cas(e)
    }
}

fn gerr(msg: impl Into<String>) -> GcError {
    GcError::Msg(msg.into())
}

/// Options for GC.
#[derive(Debug, Clone)]
pub struct GcOptions {
    /// If true, do not delete; only report.
    pub dry_run: bool,
    /// If true, print verbose info via returned report.
    pub verbose: bool,
    /// If true, attempt to remove empty fanout directories after sweep.
    pub prune_empty_dirs: bool,
    /// Root files discovery path(s). (Default: `<store_root>/roots`)
    pub roots_dirs: Vec<PathBuf>,
    /// Accept unknown algo in root lines? (If false, reject)
    pub allow_unknown_algo: bool,
}

impl Default for GcOptions {
    fn default() -> Self {
        Self {
            dry_run: true,
            verbose: false,
            prune_empty_dirs: true,
            roots_dirs: Vec::new(),
            allow_unknown_algo: false,
        }
    }
}

/// Report of a GC run.
#[derive(Debug, Clone)]
pub struct GcReport {
    pub dry_run: bool,
    pub algo: DigestAlgo,

    pub roots_scanned: usize,
    pub root_files: Vec<PathBuf>,

    pub reachable: usize,
    pub total_blobs: usize,

    pub would_delete: usize,
    pub deleted: usize,

    pub bytes_would_free: u64,
    pub bytes_freed: u64,

    pub deleted_paths: Vec<PathBuf>,
    pub errors: Vec<String>,
}

impl GcReport {
    pub fn summary_text(&self) -> String {
        let mut s = String::new();
        s.push_str("GC report\n");
        s.push_str(&format!("dry_run: {}\n", self.dry_run));
        s.push_str(&format!("algo: {}\n", self.algo.as_str()));
        s.push_str(&format!("roots_scanned: {}\n", self.roots_scanned));
        s.push_str(&format!("reachable: {}\n", self.reachable));
        s.push_str(&format!("total_blobs: {}\n", self.total_blobs));
        s.push_str(&format!("would_delete: {}\n", self.would_delete));
        s.push_str(&format!("deleted: {}\n", self.deleted));
        s.push_str(&format!("bytes_would_free: {}\n", self.bytes_would_free));
        s.push_str(&format!("bytes_freed: {}\n", self.bytes_freed));
        if !self.errors.is_empty() {
            s.push_str("errors:\n");
            for e in &self.errors {
                s.push_str("  - ");
                s.push_str(e);
                s.push('\n');
            }
        }
        s
    }
}

/// Roots provider interface.
/// In real Steel, this should come from `mff/index` or `store/index`.
pub trait RootsProvider {
    /// Return an iterator of digests considered "roots".
    fn roots(&self, cas: &Cas) -> Result<Vec<Digest>, GcError>;
}

/// Default provider that scans text files under `roots_dirs`.
/// Each line: `hex` or `algo:hex`, ignoring comments `#`.
#[derive(Debug, Clone)]
pub struct FsRootsProvider {
    pub roots_dirs: Vec<PathBuf>,
    pub allow_unknown_algo: bool,
}

impl FsRootsProvider {
    pub fn new(roots_dirs: Vec<PathBuf>) -> Self {
        Self {
            roots_dirs,
            allow_unknown_algo: false,
        }
    }
}

impl RootsProvider for FsRootsProvider {
    fn roots(&self, cas: &Cas) -> Result<Vec<Digest>, GcError> {
        let mut out = Vec::new();
        for dir in &self.roots_dirs {
            if !dir.exists() {
                continue;
            }
            let mut files = Vec::new();
            collect_files_recursive(dir, &mut files)?;
            files.sort();

            for f in files {
                if let Ok(digs) = read_root_file(cas, &f, self.allow_unknown_algo) {
                    out.extend(digs);
                }
            }
        }
        Ok(out)
    }
}

/* ------------------------------ Public API ------------------------------ */

/// Run GC using a provider (mark+sweep).
pub fn run_gc_with_provider<P: RootsProvider>(cas: &Cas, prov: &P, opt: GcOptions) -> Result<GcReport, GcError> {
    // 1) roots
    let roots = prov.roots(cas)?;
    let reachable = mark_reachable(cas, roots)?;

    // 2) sweep
    let all = cas.list_all()?;
    let total_blobs = all.len();

    let mut report = GcReport {
        dry_run: opt.dry_run,
        algo: cas.config().algo,
        roots_scanned: 0, // provider-specific; best-effort below for FsRootsProvider
        root_files: Vec::new(),
        reachable: reachable.len(),
        total_blobs,
        would_delete: 0,
        deleted: 0,
        bytes_would_free: 0,
        bytes_freed: 0,
        deleted_paths: Vec::new(),
        errors: Vec::new(),
    };

    // In std-only generic mode, we can't map path -> digest reliably without parsing filename.
    // We will treat filename stem as hex digest (as per cas.rs layout).
    for p in all {
        match digest_from_blob_path(cas.config().algo, &p) {
            Ok(d) => {
                if !reachable.contains(&d) {
                    let sz = file_len_best_effort(&p);
                    report.would_delete += 1;
                    report.bytes_would_free += sz;

                    if !opt.dry_run {
                        match fs::remove_file(&p) {
                            Ok(()) => {
                                report.deleted += 1;
                                report.bytes_freed += sz;
                                if opt.verbose {
                                    report.deleted_paths.push(p.clone());
                                }
                            }
                            Err(e) => report.errors.push(format!("delete {}: {}", p.display(), e)),
                        }
                    } else if opt.verbose {
                        report.deleted_paths.push(p.clone());
                    }
                }
            }
            Err(e) => {
                report.errors.push(format!("parse digest from {}: {e}", p.display()));
            }
        }
    }

    if !opt.dry_run && opt.prune_empty_dirs {
        if let Err(e) = prune_empty_fanout_dirs(&cas.base_dir()) {
            report.errors.push(format!("prune: {e}"));
        }
    }

    Ok(report)
}

/// Convenience: run GC with default filesystem roots provider.
/// If `opt.roots_dirs` is empty, defaults to `<store_root>/roots`.
pub fn run_gc(cas: &Cas, mut opt: GcOptions) -> Result<GcReport, GcError> {
    if opt.roots_dirs.is_empty() {
        // store_root = cas.config().root
        opt.roots_dirs = vec![cas.config().root.join("roots")];
    }
    let prov = FsRootsProvider {
        roots_dirs: opt.roots_dirs.clone(),
        allow_unknown_algo: opt.allow_unknown_algo,
    };

    // Enhance report with root file listing counts (best-effort).
    let mut rep = run_gc_with_provider(cas, &prov, opt.clone())?;
    let (n, files) = list_root_files(&prov.roots_dirs)?;
    rep.roots_scanned = n;
    rep.root_files = files;
    Ok(rep)
}

/* ------------------------------ Mark phase ------------------------------ */

/// Mark reachable digests. In a richer graph, you'd traverse from bundle roots
/// into referenced digests. In this std-only version, roots are treated as the
/// complete reachable set (no edges).
///
/// Extension point: add an "edge provider" that expands a digest into child digests.
pub fn mark_reachable(_cas: &Cas, roots: Vec<Digest>) -> Result<BTreeSet<Digest>, GcError> {
    // Placeholder for future traversal:
    // let mut q = VecDeque::from(roots);
    // while let Some(d) = q.pop_front() { for child in edges(d) { q.push_back(child) } }
    let mut set = BTreeSet::new();
    for d in roots {
        set.insert(d);
    }
    Ok(set)
}

/* ------------------------------ Root scanning ------------------------------ */

fn list_root_files(dirs: &[PathBuf]) -> Result<(usize, Vec<PathBuf>), GcError> {
    let mut files = Vec::new();
    for d in dirs {
        if d.exists() {
            collect_files_recursive(d, &mut files)?;
        }
    }
    files.sort();
    Ok((files.len(), files))
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), GcError> {
    let mut kids: Vec<PathBuf> = Vec::new();
    for e in fs::read_dir(dir)? {
        kids.push(e?.path());
    }
    kids.sort();

    for p in kids {
        let md = fs::symlink_metadata(&p)?;
        if md.file_type().is_dir() {
            collect_files_recursive(&p, out)?;
        } else if md.file_type().is_file() {
            out.push(p);
        }
    }
    Ok(())
}

fn read_root_file(cas: &Cas, path: &Path, allow_unknown_algo: bool) -> Result<Vec<Digest>, GcError> {
    let mut f = fs::File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;

    let mut out = Vec::new();
    for (i, line) in s.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // allow inline comments after whitespace '#'
        let line = match line.split_once('#') {
            Some((a, _)) => a.trim(),
            None => line,
        };
        if line.is_empty() {
            continue;
        }

        // parse digest
        let d = parse_digest_line(cas.config().algo, line, allow_unknown_algo)
            .map_err(|e| gerr(format!("{}:{}: {e}", path.display(), i + 1)))?;
        out.push(d);
    }

    Ok(out)
}

fn parse_digest_line(default_algo: DigestAlgo, s: &str, allow_unknown_algo: bool) -> Result<Digest, GcError> {
    if let Some((a, hex)) = s.split_once(':') {
        let algo = match a {
            "fnv1a64" => DigestAlgo::Fnv1a64,
            "sha256" => DigestAlgo::Sha256,
            _ => {
                if allow_unknown_algo {
                    return Ok(Digest {
                        algo: default_algo,
                        hex: hex.to_string(),
                    });
                } else {
                    return Err(gerr("unknown digest algo"));
                }
            }
        };
        Ok(Digest {
            algo,
            hex: hex.to_ascii_lowercase(),
        })
    } else {
        Ok(Digest {
            algo: default_algo,
            hex: s.to_ascii_lowercase(),
        })
    }
}

/* ------------------------------ Sweep helpers ------------------------------ */

fn digest_from_blob_path(algo: DigestAlgo, p: &Path) -> Result<Digest, GcError> {
    // Expect filename: "<hex>.blob"
    let name = p.file_name().and_then(|s| s.to_str()).ok_or_else(|| gerr("non-utf8 filename"))?;
    let hex = name.strip_suffix(".blob").ok_or_else(|| gerr("not a .blob"))?;
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(gerr("invalid hex in filename"));
    }
    Ok(Digest {
        algo,
        hex: hex.to_ascii_lowercase(),
    })
}

fn file_len_best_effort(p: &Path) -> u64 {
    fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Prune empty directories under a base (best-effort).
/// We walk bottom-up and remove empty dirs.
/// Errors are ignored for non-empty or permission issues.
fn prune_empty_fanout_dirs(base: &Path) -> Result<(), GcError> {
    if !base.exists() {
        return Ok(());
    }
    let mut dirs = Vec::new();
    collect_dirs_recursive(base, &mut dirs)?;
    // bottom-up
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
    for d in dirs {
        // Never remove base itself.
        if d == base {
            continue;
        }
        let _ = fs::remove_dir(&d);
    }
    Ok(())
}

fn collect_dirs_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), GcError> {
    out.push(dir.to_path_buf());
    let mut kids: Vec<PathBuf> = Vec::new();
    for e in fs::read_dir(dir)? {
        kids.push(e?.path());
    }
    kids.sort();
    for p in kids {
        let md = fs::symlink_metadata(&p)?;
        if md.file_type().is_dir() {
            collect_dirs_recursive(&p, out)?;
        }
    }
    Ok(())
}

/* --------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::cas::{CasConfig, Cas};

    fn tmp_root() -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        p.push(format!("steel_gc_test_{pid}_{ts}"));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn gc_dry_run_reports_deletions() {
        let root = tmp_root();
        let cas = Cas::new(CasConfig { root: root.clone(), ..CasConfig::default() }).unwrap();

        // Put 2 blobs
        let a = cas.put_bytes(b"a").unwrap();
        let _b = cas.put_bytes(b"b").unwrap();

        // Roots file only references a
        let roots_dir = root.join("roots");
        std::fs::create_dir_all(&roots_dir).unwrap();
        std::fs::write(roots_dir.join("roots.txt"), format!("{}\n", a.hex)).unwrap();

        let rep = run_gc(&cas, GcOptions { dry_run: true, roots_dirs: vec![roots_dir], ..GcOptions::default() }).unwrap();
        assert_eq!(rep.reachable, 1);
        assert_eq!(rep.total_blobs, 2);
        assert_eq!(rep.would_delete, 1);
        assert_eq!(rep.deleted, 0);
    }

    #[test]
    fn gc_sweep_deletes_unreachable() {
        let root = tmp_root();
        let cas = Cas::new(CasConfig { root: root.clone(), ..CasConfig::default() }).unwrap();

        let a = cas.put_bytes(b"a").unwrap();
        let b = cas.put_bytes(b"b").unwrap();

        let roots_dir = root.join("roots");
        std::fs::create_dir_all(&roots_dir).unwrap();
        std::fs::write(roots_dir.join("roots.txt"), format!("{}\n", a.hex)).unwrap();

        let rep = run_gc(&cas, GcOptions {
            dry_run: false,
            verbose: true,
            roots_dirs: vec![roots_dir],
            ..GcOptions::default()
        })
        .unwrap();

        assert_eq!(rep.deleted, 1);
        assert!(!cas.exists(&b).unwrap());
        assert!(cas.exists(&a).unwrap());
    }
}
