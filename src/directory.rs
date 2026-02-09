// /Users/vincent/Documents/Github/steel/src/directory.rs
//! directory — filesystem and directory utilities (std-only)
//!
//! Goals:
//! - deterministic directory traversal (stable ordering)
//! - safe-ish recursion controls (max depth, symlink policy, hidden policy)
//! - common ignore vocabulary for build/workspace tools
//! - helpers for "find steelconf", "collect sources", etc.
//!
//! Notes:
//! - std-only: no glob crate, no walkdir crate.
//! - best-effort by default; strict variants available.

use std::collections::{BTreeSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymlinkPolicy {
    /// Do not follow symlinks (use symlink_metadata).
    NoFollow,
    /// Follow symlinks (use metadata).
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenPolicy {
    /// Skip entries whose name starts with '.' (except explicit include list).
    Skip,
    /// Include hidden entries.
    Include,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Return error on first IO issue.
    Strict,
    /// Ignore IO errors and continue traversal.
    BestEffort,
}

/// Options for directory traversal.
#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub max_depth: usize,
    pub symlinks: SymlinkPolicy,
    pub hidden: HiddenPolicy,
    pub errors: ErrorPolicy,

    /// Directory names to ignore entirely (exact match).
    pub ignore_dirs: BTreeSet<OsString>,

    /// File names to ignore (exact match).
    pub ignore_files: BTreeSet<OsString>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        let mut ignore_dirs = BTreeSet::new();
        for n in [
            ".git",
            ".hg",
            ".svn",
            "target",
            "node_modules",
            "dist",
            "build",
            ".steel",
            ".steel-cache",
        ] {
            ignore_dirs.insert(OsString::from(n));
        }

        Self {
            max_depth: 16,
            symlinks: SymlinkPolicy::NoFollow,
            hidden: HiddenPolicy::Skip,
            errors: ErrorPolicy::BestEffort,
            ignore_dirs,
            ignore_files: BTreeSet::new(),
        }
    }
}

/// Collected walk entry info.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: PathBuf,
    pub file_name: OsString,
    pub depth: usize,
    pub file_type: FileType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Other,
}

/// Walk a directory tree deterministically (lexicographic within each directory).
///
/// Returns all entries (files + dirs), excluding the root itself.
pub fn walk(root: impl AsRef<Path>, opts: &WalkOptions) -> io::Result<Vec<WalkEntry>> {
    let root = root.as_ref();
    let mut out = Vec::new();

    let mut stack: VecDeque<(PathBuf, usize)> = VecDeque::new();
    stack.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = stack.pop_back() {
        if depth > opts.max_depth {
            continue;
        }

        let rd = match fs::read_dir(&dir) {
            Ok(v) => v,
            Err(e) => match opts.errors {
                ErrorPolicy::Strict => return Err(e),
                ErrorPolicy::BestEffort => continue,
            },
        };

        let mut entries: Vec<fs::DirEntry> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for ent in entries {
            let name = ent.file_name();
            if should_skip_name(&name, opts) {
                continue;
            }

            let path = ent.path();

            let md = match opts.symlinks {
                SymlinkPolicy::Follow => fs::metadata(&path),
                SymlinkPolicy::NoFollow => fs::symlink_metadata(&path),
            };

            let md = match md {
                Ok(v) => v,
                Err(e) => match opts.errors {
                    ErrorPolicy::Strict => return Err(e),
                    ErrorPolicy::BestEffort => continue,
                },
            };

            let ft = md.file_type();
            let file_type = if ft.is_dir() {
                FileType::Dir
            } else if ft.is_file() {
                FileType::File
            } else if ft.is_symlink() {
                FileType::Symlink
            } else {
                FileType::Other
            };

            if file_type == FileType::Dir && opts.ignore_dirs.contains(&name) {
                continue;
            }
            if file_type == FileType::File && opts.ignore_files.contains(&name) {
                continue;
            }

            out.push(WalkEntry {
                path: path.clone(),
                file_name: name.clone(),
                depth,
                file_type,
            });

            if file_type == FileType::Dir {
                stack.push_back((path, depth + 1));
            }
        }
    }

    Ok(out)
}

fn should_skip_name(name: &OsStr, opts: &WalkOptions) -> bool {
    if opts.hidden == HiddenPolicy::Include {
        return false;
    }
    if let Some(s) = name.to_str() {
        s.starts_with('.')
    } else {
        false
    }
}

/// Find a file by exact name within `root` with deterministic traversal.
/// Returns the first match in DFS order (lexicographic per dir).
pub fn find_file_named(root: impl AsRef<Path>, filename: &str, opts: &WalkOptions) -> io::Result<Option<PathBuf>> {
    let root = root.as_ref();
    if root.join(filename).is_file() {
        return Ok(Some(root.join(filename)));
    }

    // For discovery, avoid ignoring build/dist by default if caller wants them.
    // (Callers can override via opts.ignore_dirs.)
    let entries = walk(root, opts)?;
    for e in entries {
        if e.file_type == FileType::File {
            if let Some(n) = e.file_name.to_str() {
                if n == filename {
                    return Ok(Some(e.path));
                }
            }
        }
    }
    Ok(None)
}

/// Discover any file among a list of candidate names (in order).
pub fn discover_first_named(root: impl AsRef<Path>, candidates: &[&str], opts: &WalkOptions) -> io::Result<Option<PathBuf>> {
    let root = root.as_ref();
    for &c in candidates {
        if let Some(p) = find_file_named(root, c, opts)? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// Collect files with extensions (case-sensitive) under `root`.
pub fn collect_files_with_exts(
    root: impl AsRef<Path>,
    exts: &[&str],
    opts: &WalkOptions,
) -> io::Result<Vec<PathBuf>> {
    let root = root.as_ref();
    let entries = walk(root, opts)?;
    let mut out = Vec::new();

    for e in entries {
        if e.file_type != FileType::File {
            continue;
        }
        if let Some(ext) = e.path.extension().and_then(|s| s.to_str()) {
            if exts.iter().any(|x| *x == ext) {
                out.push(e.path);
            }
        }
    }

    // already deterministic because walk is deterministic
    Ok(out)
}

/// Collect files matching a predicate on file name.
pub fn collect_files_by_name_pred(
    root: impl AsRef<Path>,
    pred: impl Fn(&OsStr) -> bool,
    opts: &WalkOptions,
) -> io::Result<Vec<PathBuf>> {
    let root = root.as_ref();
    let entries = walk(root, opts)?;
    let mut out = Vec::new();

    for e in entries {
        if e.file_type == FileType::File && pred(&e.file_name) {
            out.push(e.path);
        }
    }

    Ok(out)
}

/// Normalize a path without canonicalizing (purely lexical).
pub fn normalize_path(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    let mut out = PathBuf::new();
    for c in p.components() {
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

/// Return a path relative to root if possible, else return the original path.
pub fn relativize(root: impl AsRef<Path>, p: impl AsRef<Path>) -> PathBuf {
    let root = root.as_ref();
    let p = p.as_ref();
    match p.strip_prefix(root) {
        Ok(r) => r.to_path_buf(),
        Err(_) => p.to_path_buf(),
    }
}

/// Check if `p` is under `root` (lexical, not filesystem-realpath).
pub fn is_under(root: impl AsRef<Path>, p: impl AsRef<Path>) -> bool {
    let root = normalize_path(root);
    let p = normalize_path(p);
    p.strip_prefix(&root).is_ok()
}

/// Common ignore vocabulary for build systems.
pub fn default_ignore_dirs() -> BTreeSet<OsString> {
    let mut s = BTreeSet::new();
    for n in [
        ".git",
        ".hg",
        ".svn",
        "target",
        "node_modules",
        "dist",
        "build",
        ".steel",
        ".steel-cache",
    ] {
        s.insert(OsString::from(n));
    }
    s
}

/// Small helper: join relative path under root, keep absolute unchanged.
pub fn join_under(root: impl AsRef<Path>, p: impl AsRef<Path>) -> PathBuf {
    let root = root.as_ref();
    let p = p.as_ref();
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        base.join(format!("{prefix}_{pid}_{}", t.as_nanos()))
    }

    #[test]
    fn normalize_lexical() {
        let p = normalize_path("a/./b/../c");
        assert_eq!(p, PathBuf::from("a/c"));
    }

    #[test]
    fn walk_is_deterministic_basic() {
        let dir = tmp("steel_dir");
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::write(dir.join("b").join("z.txt"), "z").unwrap();
        fs::write(dir.join("a").join("a.txt"), "a").unwrap();

        let opts = WalkOptions { hidden: HiddenPolicy::Include, ..WalkOptions::default() };
        let entries = walk(&dir, &opts).unwrap();

        // Ensure we see both files.
        let mut files: Vec<String> = entries
            .into_iter()
            .filter(|e| e.file_type == FileType::File)
            .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        files.sort();
        assert_eq!(files, vec!["a.txt".to_string(), "z.txt".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }
}
