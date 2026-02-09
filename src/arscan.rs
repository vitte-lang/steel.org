//! arscan — Artifact/manifest scanner
//!
//! Small, dependency-free directory scanner used to discover Steel files
//! (e.g. `build.muf`, `mod.muf`, `steelconf`, `steelconfig.mff`, `steel.log`).
//!
//! Goals
//! - No external crates
//! - Deterministic traversal order
//! - Best-effort by default (collect errors, don't abort)
//! - Cheap metadata snapshot

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Known Steel artifact kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// A bake/build file (canonical: `build.muf`).
    BuildMuf,
    /// A module/package manifest (canonical: `mod.muf`).
    ModMuf,
    /// Main Steel configuration file (canonical: `steelconf` or `steel`).
    Steelconf,
    /// Resolved configuration (canonical: `steelconfig.mff`).
    SteelConfig,
    /// Build log (canonical: `steel.log`).
    SteelLog,
    /// Legacy resolved configuration (historical: `.mcf`).
    LegacyMcf,
    /// Legacy/alt config files mentioned in ecosystem (e.g. `.mfg`).
    LegacyMfg,
    /// Any `*.muf` file that is not recognized as a known canonical name.
    GenericMuf,
    /// Any `*.mff` file that is not recognized as `steelconfig.mff`.
    GenericMff,
    /// Unknown / not classified.
    Unknown,
}

impl ArtifactKind {
    #[inline]
    pub fn is_steel(&self) -> bool {
        matches!(
            self,
            ArtifactKind::BuildMuf
                | ArtifactKind::ModMuf
                | ArtifactKind::Steelconf
                | ArtifactKind::SteelConfig
                | ArtifactKind::SteelLog
                | ArtifactKind::LegacyMcf
                | ArtifactKind::LegacyMfg
                | ArtifactKind::GenericMuf
                | ArtifactKind::GenericMff
        )
    }

}

/// File metadata snapshot captured during scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMeta {
    pub size_bytes: u64,
    pub modified: Option<SystemTime>,
}

/// A discovered artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// Absolute or root-joined path to the artifact.
    pub path: PathBuf,
    /// Path relative to the scan root (best effort).
    pub rel_path: PathBuf,
    pub kind: ArtifactKind,
    pub meta: Option<ArtifactMeta>,
}

/// Error captured during scan.
#[derive(Debug)]
pub struct ScanError {
    pub path: PathBuf,
    pub op: &'static str,
    pub err: io::Error,
}

/// Options controlling directory scan.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Maximum recursion depth. `0` means only scan the root directory.
    pub max_depth: usize,
    /// Follow symlinks when traversing directories.
    pub follow_symlinks: bool,
    /// Include hidden files/dirs (starting with '.')
    pub include_hidden: bool,
    /// If true, abort the scan on first IO error.
    pub strict_errors: bool,
    /// Collect basic file metadata (size, mtime). If false, `meta` will be None.
    pub collect_meta: bool,
    /// Directory names to ignore (non-recursive name match).
    pub ignore_dir_names: Vec<OsString>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 32,
            follow_symlinks: false,
            include_hidden: false,
            strict_errors: false,
            collect_meta: true,
            ignore_dir_names: vec![
                OsString::from(".git"),
                OsString::from(".hg"),
                OsString::from(".svn"),
                OsString::from("target"),
                OsString::from("node_modules"),
                OsString::from("dist"),
                OsString::from("build"),
                OsString::from(".steel"),
                OsString::from(".steel-cache"),
            ],
        }
    }
}

/// Scan result.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub artifacts: Vec<Artifact>,
    pub errors: Vec<ScanError>,
    pub visited_dirs: u64,
    pub visited_files: u64,
    pub skipped_entries: u64,
}

/// Scan a directory tree for Steel artifacts.
pub fn scan(root: impl AsRef<Path>, opts: &ScanOptions) -> ScanReport {
    let root = root.as_ref();

    let mut report = ScanReport::default();

    // Normalize root for rel_path computation; keep best-effort (no canonicalize hard fail).
    let root_norm = normalize_path(root);

    scan_dir(root, &root_norm, 0, opts, &mut report);

    // Stable ordering across platforms.
    report.artifacts.sort_by(|a, b| {
        let ka = a.kind as u32;
        let kb = b.kind as u32;
        match ka.cmp(&kb) {
            Ordering::Equal => a.rel_path.cmp(&b.rel_path),
            other => other,
        }
    });

    report
}

fn scan_dir(dir: &Path, root_norm: &Path, depth: usize, opts: &ScanOptions, report: &mut ScanReport) {
    report.visited_dirs = report.visited_dirs.saturating_add(1);

    let rd = match fs::read_dir(dir) {
        Ok(v) => v,
        Err(err) => {
            push_err(report, dir.to_path_buf(), "read_dir", err, opts.strict_errors);
            return;
        }
    };

    // Deterministic order: collect then sort.
    let mut entries: Vec<fs::DirEntry> = Vec::new();
    for e in rd {
        match e {
            Ok(ent) => entries.push(ent),
            Err(err) => {
                push_err(report, dir.to_path_buf(), "read_dir_entry", err, opts.strict_errors);
                if opts.strict_errors {
                    return;
                }
            }
        }
    }

    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for ent in entries {
        let name = ent.file_name();
        if !opts.include_hidden {
            if let Some(s) = name.to_str() {
                if s.starts_with('.') {
                    report.skipped_entries = report.skipped_entries.saturating_add(1);
                    continue;
                }
            }
        }

        let path = ent.path();

        // Metadata for traversal decision.
        let md = if opts.follow_symlinks {
            fs::metadata(&path)
        } else {
            fs::symlink_metadata(&path)
        };

        let md = match md {
            Ok(v) => v,
            Err(err) => {
                push_err(report, path.clone(), "metadata", err, opts.strict_errors);
                if opts.strict_errors {
                    return;
                }
                report.skipped_entries = report.skipped_entries.saturating_add(1);
                continue;
            }
        };

        if md.is_dir() {
            if depth >= opts.max_depth {
                report.skipped_entries = report.skipped_entries.saturating_add(1);
                continue;
            }

            if should_ignore_dir_name(&name, &opts.ignore_dir_names) {
                report.skipped_entries = report.skipped_entries.saturating_add(1);
                continue;
            }

            scan_dir(&path, root_norm, depth + 1, opts, report);
            if opts.strict_errors && !report.errors.is_empty() {
                return;
            }
            continue;
        }

        if !md.is_file() {
            report.skipped_entries = report.skipped_entries.saturating_add(1);
            continue;
        }

        report.visited_files = report.visited_files.saturating_add(1);

        let kind = classify_path(&path);
        if kind == ArtifactKind::Unknown {
            continue;
        }

        let rel_path = make_rel_path(&path, root_norm);
        let meta = if opts.collect_meta {
            Some(ArtifactMeta {
                size_bytes: md.len(),
                modified: md.modified().ok(),
            })
        } else {
            None
        };

        report.artifacts.push(Artifact {
            path,
            rel_path,
            kind,
            meta,
        });
    }
}

fn should_ignore_dir_name(name: &OsStr, ignores: &[OsString]) -> bool {
    ignores.iter().any(|n| n.as_os_str() == name)
}

fn classify_path(path: &Path) -> ArtifactKind {
    let file_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(v) => v,
        None => return ArtifactKind::Unknown,
    };

    // Canonical file names (case-sensitive; if your FS is case-insensitive it still works).
    match file_name {
        "build.muf" => return ArtifactKind::BuildMuf,
        "mod.muf" => return ArtifactKind::ModMuf,
        "steelconf" | "steel" => return ArtifactKind::Steelconf,
        "steelconfig.mff" => return ArtifactKind::SteelConfig,
        "steel.log" => return ArtifactKind::SteelLog,
        "steel.mcf" => return ArtifactKind::LegacyMcf,
        _ => {}
    }

    // Extensions.
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext {
        "muf" => ArtifactKind::GenericMuf,
        "mff" => ArtifactKind::GenericMff,
        "mcf" => ArtifactKind::LegacyMcf,
        "mfg" => ArtifactKind::LegacyMfg,
        _ => ArtifactKind::Unknown,
    }
}

fn make_rel_path(path: &Path, root_norm: &Path) -> PathBuf {
    // Best-effort relative path; if strip fails, return file_name.
    match path.strip_prefix(root_norm) {
        Ok(p) => p.to_path_buf(),
        Err(_) => path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path)),
    }
}

fn normalize_path(p: &Path) -> PathBuf {
    // Avoid canonicalize() to not require the path to exist in all callers.
    // Normalize `.` and `..` segments in a minimal way.
    let mut out = PathBuf::new();
    for c in p.components() {
        use std::path::Component;
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn push_err(report: &mut ScanReport, path: PathBuf, op: &'static str, err: io::Error, strict: bool) {
    report.errors.push(ScanError { path, op, err });
    if strict {
        // Nothing else to do here; callers will check strict + errors and return.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let t = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let name = format!("{}_{}_{}", prefix, pid, t.as_nanos());
        base.join(name)
    }

    fn touch(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn scan_finds_canonical_files() {
        let root = unique_temp_dir("steel_arscan");
        fs::create_dir_all(&root).unwrap();

        touch(&root.join("build.muf"), "bake ...");
        touch(&root.join("mod.muf"), "mod ...");
        touch(&root.join("steelconf"), "workspace ...");
        touch(&root.join("steelconfig.mff"), "mff 1");
        touch(&root.join("steel.log"), "mff 1");

        // Noise
        touch(&root.join("README.md"), "hello");
        touch(&root.join("nested/other.muf"), "x");
        touch(&root.join("nested/other.mff"), "y");

        let opts = ScanOptions {
            max_depth: 8,
            ..Default::default()
        };

        let rep = scan(&root, &opts);

        // Count by kind.
        let mut build_muf = 0;
        let mut mod_muf = 0;
        let mut steelfile = 0;
        let mut steelconfig = 0;
        let mut steellog = 0;
        let mut generic_muf = 0;
        let mut generic_mff = 0;

        for a in &rep.artifacts {
            match a.kind {
                ArtifactKind::BuildMuf => build_muf += 1,
                ArtifactKind::ModMuf => mod_muf += 1,
                ArtifactKind::Steelconf => steelfile += 1,
                ArtifactKind::SteelConfig => steelconfig += 1,
                ArtifactKind::SteelLog => steellog += 1,
                ArtifactKind::GenericMuf => generic_muf += 1,
                ArtifactKind::GenericMff => generic_mff += 1,
                _ => {}
            }
        }

        assert_eq!(build_muf, 1);
        assert_eq!(mod_muf, 1);
        assert_eq!(steelfile, 1);
        assert_eq!(steelconfig, 1);
        assert_eq!(steellog, 1);
        assert_eq!(generic_muf, 1);
        assert_eq!(generic_mff, 1);

        // Cleanup (best effort).
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_is_deterministic_by_rel_path_ordering() {
        let root = unique_temp_dir("steel_arscan_det");
        fs::create_dir_all(&root).unwrap();

        touch(&root.join("z/build.muf"), "b");
        touch(&root.join("a/mod.muf"), "m");
        touch(&root.join("b/steel.log"), "c");

        let opts = ScanOptions {
            max_depth: 8,
            ..Default::default()
        };

        let rep1 = scan(&root, &opts);
        let rep2 = scan(&root, &opts);

        assert_eq!(rep1.artifacts.len(), rep2.artifacts.len());
        for (a, b) in rep1.artifacts.iter().zip(rep2.artifacts.iter()) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.rel_path, b.rel_path);
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_respects_ignore_and_hidden_defaults() {
        let root = unique_temp_dir("steel_arscan_ign");
        fs::create_dir_all(&root).unwrap();

        touch(&root.join(".git/build.muf"), "b");
        touch(&root.join("target/mod.muf"), "m");
        touch(&root.join(".hidden/steel.log"), "c");
        touch(&root.join("ok/build.muf"), "b");

        let opts = ScanOptions::default();
        let rep = scan(&root, &opts);

        // Only ok/build.muf should be found.
        assert_eq!(rep.artifacts.len(), 1);
        assert_eq!(rep.artifacts[0].kind, ArtifactKind::BuildMuf);

        let _ = fs::remove_dir_all(&root);
    }
}
