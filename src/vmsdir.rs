//! vmsdir.rs
//!
//! “VMS Dir” — utilitaires FS orientés build (workspace/build/store) pour Steel.
//!
//! Rôles typiques:
//! - Assurer l’existence de répertoires (build/, cache/, jobs/…)
//! - Gestion de fichiers atomiques (write + rename)
//! - Nettoyage (rm -rf) sous racines contrôlées
//! - Copy/mirror (best-effort) + filtrage
//! - Listing récursif (walk) std-only
//! - Helpers de lock fichier (best-effort) pour éviter build concurrent
//! - Helpers de timestamps/metadata
//!
//! Dépendances: std uniquement.
//!
//! Sécurité:
//! - Toute opération destructive peut être restreinte à une “allowed root”.
//! - Ce module fournit des garde-fous, mais la policy capsule/store doit être appliquée au-dessus.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Erreur FS “build-oriented”.
#[derive(Debug)]
pub enum VmsDirError {
    Io(io::Error),
    NotAllowed { path: PathBuf },
    NotFound { path: PathBuf },
    InvalidPath { path: PathBuf, reason: String },
    LockBusy { lock_path: PathBuf },
}

impl fmt::Display for VmsDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmsDirError::Io(e) => write!(f, "io: {}", e),
            VmsDirError::NotAllowed { path } => write!(f, "path not allowed: {}", path.display()),
            VmsDirError::NotFound { path } => write!(f, "not found: {}", path.display()),
            VmsDirError::InvalidPath { path, reason } => {
                write!(f, "invalid path: {} ({})", path.display(), reason)
            }
            VmsDirError::LockBusy { lock_path } => {
                write!(f, "lock busy: {}", lock_path.display())
            }
        }
    }
}

impl std::error::Error for VmsDirError {}

impl From<io::Error> for VmsDirError {
    fn from(e: io::Error) -> Self {
        VmsDirError::Io(e)
    }
}

/// Policy FS: limite les opérations à des racines autorisées.
#[derive(Debug, Clone)]
pub struct FsPolicy {
    /// Si vide: tout est autorisé.
    pub allowed_roots: Vec<PathBuf>,
    /// Si true: canonicalize best-effort avant check.
    pub canonicalize: bool,
}

impl Default for FsPolicy {
    fn default() -> Self {
        Self {
            allowed_roots: Vec::new(),
            canonicalize: true,
        }
    }
}

impl FsPolicy {
    pub fn allow_root(mut self, p: impl Into<PathBuf>) -> Self {
        self.allowed_roots.push(p.into());
        self
    }

    pub fn check(&self, path: &Path) -> Result<(), VmsDirError> {
        if self.allowed_roots.is_empty() {
            return Ok(());
        }

        let test = if self.canonicalize {
            canonicalize_best_effort(path)
        } else {
            path.to_path_buf()
        };

        for r in &self.allowed_roots {
            let rr = if self.canonicalize {
                canonicalize_best_effort(r)
            } else {
                r.to_path_buf()
            };
            if test.starts_with(&rr) {
                return Ok(());
            }
        }

        Err(VmsDirError::NotAllowed { path: test })
    }
}

/* ======================
 * Core helpers
 * ====================== */

/// create_dir_all avec policy.
pub fn ensure_dir(policy: &FsPolicy, dir: &Path) -> Result<(), VmsDirError> {
    policy.check(dir)?;
    fs::create_dir_all(dir)?;
    Ok(())
}

/// Supprime un fichier si présent.
pub fn remove_file_if_exists(policy: &FsPolicy, path: &Path) -> Result<bool, VmsDirError> {
    policy.check(path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Supprime un répertoire (récursif si demandé).
pub fn remove_dir(policy: &FsPolicy, dir: &Path, recursive: bool) -> Result<(), VmsDirError> {
    policy.check(dir)?;
    if recursive {
        match fs::remove_dir_all(dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(VmsDirError::NotFound { path: dir.to_path_buf() })
            }
            Err(e) => Err(e.into()),
        }
    } else {
        fs::remove_dir(dir)?;
        Ok(())
    }
}

/// Nettoyage: supprime tous les enfants d’un dossier (mais pas le dossier).
pub fn clear_dir(policy: &FsPolicy, dir: &Path) -> Result<(), VmsDirError> {
    policy.check(dir)?;
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            remove_dir(policy, &p, true)?;
        } else {
            remove_file_if_exists(policy, &p)?;
        }
    }
    Ok(())
}

/// Écriture atomique: write dans tmp puis rename.
/// Sur Windows, rename peut échouer si destination existe; on remove puis rename best-effort.
pub fn write_atomic(policy: &FsPolicy, path: &Path, bytes: &[u8]) -> Result<(), VmsDirError> {
    policy.check(path)?;
    if let Some(parent) = path.parent() {
        ensure_dir(policy, parent)?;
    }

    let tmp = tmp_sibling_path(path, "tmp");
    policy.check(&tmp)?;
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok(); // best-effort
    }

    // replace
    if path.exists() {
        let _ = remove_file_if_exists(policy, path);
    }

    fs::rename(&tmp, path).or_else(|e| {
        // fallback: copy + remove tmp
        if e.kind() == io::ErrorKind::CrossesDevices {
            fs::copy(&tmp, path)?;
            let _ = fs::remove_file(&tmp);
            Ok(())
        } else {
            Err(e)
        }
    })?;

    Ok(())
}

/// Lecture complète.
pub fn read_all(policy: &FsPolicy, path: &Path) -> Result<Vec<u8>, VmsDirError> {
    policy.check(path)?;
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Copy file (créé les parents). Retourne bytes copiés.
pub fn copy_file(policy: &FsPolicy, src: &Path, dst: &Path, overwrite: bool) -> Result<u64, VmsDirError> {
    policy.check(src)?;
    policy.check(dst)?;
    if !src.exists() {
        return Err(VmsDirError::NotFound { path: src.to_path_buf() });
    }
    if dst.exists() && !overwrite {
        return Err(VmsDirError::InvalidPath {
            path: dst.to_path_buf(),
            reason: "destination exists".to_string(),
        });
    }
    if let Some(parent) = dst.parent() {
        ensure_dir(policy, parent)?;
    }
    let n = fs::copy(src, dst)?;
    Ok(n)
}

/// Copy dir récursif (best-effort). Ne copie pas les symlinks (les ignore).
pub fn copy_dir_recursive(
    policy: &FsPolicy,
    src_dir: &Path,
    dst_dir: &Path,
    overwrite: bool,
) -> Result<(), VmsDirError> {
    policy.check(src_dir)?;
    policy.check(dst_dir)?;

    if !src_dir.is_dir() {
        return Err(VmsDirError::InvalidPath {
            path: src_dir.to_path_buf(),
            reason: "source is not a directory".to_string(),
        });
    }

    ensure_dir(policy, dst_dir)?;

    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let sp = entry.path();
        let name = entry.file_name();
        let dp = dst_dir.join(name);

        let meta = entry.metadata()?;
        if meta.is_dir() {
            copy_dir_recursive(policy, &sp, &dp, overwrite)?;
        } else if meta.is_file() {
            let _ = copy_file(policy, &sp, &dp, overwrite)?;
        } else {
            // ignore symlink/special
        }
    }

    Ok(())
}

/* ======================
 * Walk / listing
 * ====================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkOrder {
    PreOrder,
    PostOrder,
}

/// Entrée walk.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
}

/// Walk récursif std-only. Ignore symlinks.
pub fn walk_dir(
    policy: &FsPolicy,
    root: &Path,
    order: WalkOrder,
    max_depth: Option<usize>,
) -> Result<Vec<WalkEntry>, VmsDirError> {
    policy.check(root)?;
    let mut out = Vec::new();
    walk_inner(policy, root, order, 0, max_depth, &mut out)?;
    Ok(out)
}

fn walk_inner(
    policy: &FsPolicy,
    dir: &Path,
    order: WalkOrder,
    depth: usize,
    max_depth: Option<usize>,
    out: &mut Vec<WalkEntry>,
) -> Result<(), VmsDirError> {
    policy.check(dir)?;

    if let Some(md) = max_depth {
        if depth > md {
            return Ok(());
        }
    }

    if order == WalkOrder::PreOrder {
        out.push(WalkEntry {
            path: dir.to_path_buf(),
            is_dir: true,
            depth,
        });
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk_inner(policy, &p, order, depth + 1, max_depth, out)?;
        } else if meta.is_file() {
            out.push(WalkEntry {
                path: p,
                is_dir: false,
                depth: depth + 1,
            });
        } else {
            // ignore specials
        }
    }

    if order == WalkOrder::PostOrder {
        out.push(WalkEntry {
            path: dir.to_path_buf(),
            is_dir: true,
            depth,
        });
    }

    Ok(())
}

/// Liste récursive (fichiers uniquement), triée lexicographiquement.
pub fn list_files_recursive(policy: &FsPolicy, root: &Path) -> Result<Vec<PathBuf>, VmsDirError> {
    let entries = walk_dir(policy, root, WalkOrder::PreOrder, None)?;
    let mut files: Vec<PathBuf> = entries
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path)
        .collect();
    files.sort();
    Ok(files)
}

/* ======================
 * Locks
 * ====================== */

/// Lock fichier simple:
/// - crée un fichier .lock en exclusif
/// - écrit pid + timestamp
/// - supprime à drop
///
/// Best-effort: ne gère pas crash recovery automatique.
pub struct FileLock {
    lock_path: PathBuf,
}

impl FileLock {
    pub fn acquire(policy: &FsPolicy, lock_path: impl Into<PathBuf>) -> Result<Self, VmsDirError> {
        let lock_path = lock_path.into();
        policy.check(&lock_path)?;
        if let Some(parent) = lock_path.parent() {
            ensure_dir(policy, parent)?;
        }

        // create_new => exclusif
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    VmsDirError::LockBusy {
                        lock_path: lock_path.clone(),
                    }
                } else {
                    VmsDirError::Io(e)
                }
            })?;

        let pid = std::process::id();
        let ts = now_unix_ms();
        let _ = writeln!(f, "pid={}", pid);
        let _ = writeln!(f, "ts_ms={}", ts);

        Ok(Self { lock_path })
    }

    pub fn path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

/* ======================
 * Metadata / time
 * ====================== */

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn modified_unix_ms(path: &Path) -> Result<i64, VmsDirError> {
    let md = fs::metadata(path)?;
    let t = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(t.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64)
}

pub fn file_size(path: &Path) -> Result<u64, VmsDirError> {
    let md = fs::metadata(path)?;
    Ok(md.len())
}

/* ======================
 * Path utilities
 * ====================== */

/// Retourne un “tmp sibling” unique, ex: file -> file.tmp.<ts>.<pid>
pub fn tmp_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let ts = now_unix_ms();
    let pid = std::process::id();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.{}.{}", suffix, ts, pid));
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// Canonicalize best-effort (si absent, tente parent).
pub fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize()
        .or_else(|_| {
            path.parent()
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no parent"))?
                .canonicalize()
                .map(|p| p.join(path.file_name().unwrap_or_default()))
        })
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Normalise un path “lexical” (sans accès FS):
/// - supprime '.' et résout '..' best-effort (sans passer sous root lexical)
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir => out.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::Normal(x) => out.push(x),
        }
    }
    out
}

/// Assure qu’un path (après canonicalize) reste sous root.
pub fn ensure_under(root: &Path, path: &Path) -> Result<(), VmsDirError> {
    let rc = canonicalize_best_effort(root);
    let pc = canonicalize_best_effort(path);
    if pc.starts_with(&rc) {
        Ok(())
    } else {
        Err(VmsDirError::NotAllowed { path: pc })
    }
}

/* ======================
 * Filtering / mirror
 * ====================== */

/// Mirror d’un dossier src vers dst en copiant uniquement les fichiers dont l’extension est
/// dans `ext_allow` (si Some). Si None, copie tout.
/// - Ne supprime pas les fichiers en trop côté dst (safe mirror).
pub fn mirror_dir_filtered(
    policy: &FsPolicy,
    src: &Path,
    dst: &Path,
    ext_allow: Option<&BTreeSet<String>>,
    overwrite: bool,
) -> Result<(), VmsDirError> {
    policy.check(src)?;
    policy.check(dst)?;
    ensure_dir(policy, dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let sp = entry.path();
        let name = entry.file_name();
        let dp = dst.join(name);

        let meta = entry.metadata()?;
        if meta.is_dir() {
            mirror_dir_filtered(policy, &sp, &dp, ext_allow, overwrite)?;
        } else if meta.is_file() {
            if let Some(set) = ext_allow {
                if let Some(ext) = sp.extension().and_then(|e| e.to_str()) {
                    if !set.contains(ext) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let _ = copy_file(policy, &sp, &dp, overwrite)?;
        }
    }

    Ok(())
}

/* ======================
 * Tests
 * ====================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_sibling_has_suffix() {
        let p = PathBuf::from("a.txt");
        let t = tmp_sibling_path(&p, "tmp");
        assert!(t.to_string_lossy().contains(".tmp."));
    }

    #[test]
    fn normalize_lexical_basic() {
        let p = PathBuf::from("a/./b/../c");
        let n = normalize_lexical(&p);
        assert!(n.to_string_lossy().contains("a"));
        assert!(n.to_string_lossy().contains("c"));
    }

    #[test]
    fn policy_allows_root() {
        let pol = FsPolicy::default().allow_root(".");
        assert!(pol.check(Path::new(".")).is_ok());
    }
}
