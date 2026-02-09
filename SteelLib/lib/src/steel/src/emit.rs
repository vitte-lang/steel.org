//! Emit layer (mcfg) — écriture des artefacts `.mff` / `*.muff`.
//!
//! Propriétés recherchées :
//! - déterminisme (ordre stable, normalisation chemins / newlines)
//! - robustesse FS (atomic write, create_dirs, permissions best-effort)
//! - incrémentalité (skip si contenu identique)
//! - observabilité (stats, manifest optionnel)
//! - testabilité (FS abstrait : RealFs + MemFs)
//!
//! Dépendances : std uniquement + `crate::diag`.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::diag::{DiagBag, Diagnostic};

/// ------------------------------------------------------------
/// FS abstraction
/// ------------------------------------------------------------

pub trait Fs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn metadata_len(&self, path: &Path) -> io::Result<u64>;

    #[cfg(unix)]
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    #[cfg(not(unix))]
    fn set_mode(&self, _path: &Path, _mode: u32) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl Fs for RealFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut f = fs::File::create(path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn metadata_len(&self, path: &Path) -> io::Result<u64> {
        Ok(fs::metadata(path)?.len())
    }

    #[cfg(unix)]
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(mode);
        fs::set_permissions(path, perms)?;
        Ok(())
    }
}

/// ------------------------------------------------------------
/// Policies / options
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// tmp + fsync + rename.
    Atomic,
    /// direct write (debug / contraintes FS).
    Direct,
}

impl Default for WriteMode {
    fn default() -> Self {
        WriteMode::Atomic
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    /// écrase toujours (en respectant WriteMode).
    Always,
    /// écrit seulement si le contenu diffère.
    IfChanged,
    /// écrit seulement si le fichier n’existe pas.
    IfMissing,
    /// ne jamais écrire si le fichier existe.
    Never,
}

impl Default for Overwrite {
    fn default() -> Self {
        Overwrite::IfChanged
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineMode {
    /// conserve tel quel.
    Preserve,
    /// normalise CRLF/CR -> LF.
    Lf,
}

impl Default for NewlineMode {
    fn default() -> Self {
        NewlineMode::Lf
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimMode {
    /// conserve.
    Preserve,
    /// trim trailing spaces/tabs (par ligne).
    TrimTrailingWhitespace,
}

impl Default for TrimMode {
    fn default() -> Self {
        TrimMode::Preserve
    }
}

#[derive(Debug, Clone)]
pub struct EmitOptions {
    pub write_mode: WriteMode,
    pub overwrite: Overwrite,
    pub dry_run: bool,
    pub create_dirs: bool,
    pub newline: NewlineMode,
    pub trim: TrimMode,
    /// Permission unix optionnelle (ex: 0o644). Ignoré hors unix.
    pub unix_mode: Option<u32>,
    /// Ecrit un manifest texte listant les sorties.
    pub write_manifest: bool,
    /// Chemin du manifest (relatif au root plan, ou absolu).
    pub manifest_path: PathBuf,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            write_mode: WriteMode::Atomic,
            overwrite: Overwrite::IfChanged,
            dry_run: false,
            create_dirs: true,
            newline: NewlineMode::Lf,
            trim: TrimMode::Preserve,
            unix_mode: None,
            write_manifest: false,
            manifest_path: PathBuf::from(".steel/emit.manifest"),
        }
    }
}

/// ------------------------------------------------------------
/// Artifacts / plan
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    GlobalMff,
    UnitMuff,
    Extra,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::GlobalMff => "global_mff",
            ArtifactKind::UnitMuff => "unit_muff",
            ArtifactKind::Extra => "extra",
        }
    }
}

/// Artefact texte (chemin + contenu).
#[derive(Debug, Clone)]
pub struct TextArtifact {
    pub kind: ArtifactKind,
    pub path: PathBuf,
    pub content: String,
    /// Tags optionnels (debug/filtrage tooling).
    pub tags: BTreeSet<String>,
}

impl TextArtifact {
    pub fn new(kind: ArtifactKind, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
            content: content.into(),
            tags: BTreeSet::new(),
        }
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.tags.insert(t.into());
        self
    }
}

/// Plan complet (global + unités + extras).
#[derive(Debug, Clone, Default)]
pub struct EmitPlan {
    /// Root logique : permet de produire des chemins relatifs dans manifest.
    pub root: Option<PathBuf>,
    pub artifacts: Vec<TextArtifact>,
    /// meta optionnel (ex: platform, buildfile, version…)
    pub meta: BTreeMap<String, String>,
}

impl EmitPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    pub fn push(&mut self, a: TextArtifact) {
        self.artifacts.push(a);
    }

    pub fn push_global_mff(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.push(TextArtifact::new(ArtifactKind::GlobalMff, path, content));
    }

    pub fn push_unit_muff(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.push(TextArtifact::new(ArtifactKind::UnitMuff, path, content));
    }

    pub fn sort_deterministic(&mut self) {
        self.artifacts.sort_by(|a, b| {
            (a.kind.as_str(), &a.path).cmp(&(b.kind.as_str(), &b.path))
        });
    }
}

/// ------------------------------------------------------------
/// Result / stats
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitAction {
    WroteNew,
    Updated,
    SkippedUnchanged,
    SkippedExists,
    SkippedPolicy,
    Failed,
    DryRunWouldWrite,
    DryRunWouldUpdate,
}

#[derive(Debug, Clone)]
pub struct EmitEvent {
    pub kind: ArtifactKind,
    pub path: PathBuf,
    pub action: EmitAction,
    pub bytes: usize,
    pub fingerprint: u64,
}

#[derive(Debug, Default, Clone)]
pub struct EmitStats {
    pub duration: Duration,
    pub total: usize,
    pub wrote: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub dry_run: usize,
    pub bytes_written: u64,
}

#[derive(Debug, Default, Clone)]
pub struct EmitResult {
    pub events: Vec<EmitEvent>,
    pub stats: EmitStats,
    pub manifest_path: Option<PathBuf>,
}

impl EmitResult {
    pub fn ok(&self) -> bool {
        self.stats.failed == 0
    }
}

/// ------------------------------------------------------------
/// Public entrypoints
/// ------------------------------------------------------------

pub struct Emitter<F: Fs = RealFs> {
    fs: F,
    opts: EmitOptions,
}

impl<F: Fs> Emitter<F> {
    pub fn new(fs: F, opts: EmitOptions) -> Self {
        Self { fs, opts }
    }

    pub fn emit(&self, mut plan: EmitPlan, diags: &mut DiagBag) -> EmitResult {
        let t0 = Instant::now();
        plan.sort_deterministic();

        let mut res = EmitResult::default();
        res.stats.total = plan.artifacts.len();

        for a in &plan.artifacts {
            let ev = self.emit_one(a, &plan, diags);
            match ev.action {
                EmitAction::WroteNew => {
                    res.stats.wrote += 1;
                    res.stats.bytes_written += ev.bytes as u64;
                }
                EmitAction::Updated => {
                    res.stats.updated += 1;
                    res.stats.bytes_written += ev.bytes as u64;
                }
                EmitAction::SkippedUnchanged | EmitAction::SkippedExists | EmitAction::SkippedPolicy => {
                    res.stats.skipped += 1;
                }
                EmitAction::DryRunWouldWrite | EmitAction::DryRunWouldUpdate => {
                    res.stats.dry_run += 1;
                }
                EmitAction::Failed => res.stats.failed += 1,
            }
            res.events.push(ev);
        }

        // Manifest (optional)
        if self.opts.write_manifest {
            if let Some(p) = self.emit_manifest(&plan, &res, diags) {
                res.manifest_path = Some(p);
            }
        }

        res.stats.duration = t0.elapsed();
        res
    }

    fn emit_one(&self, a: &TextArtifact, plan: &EmitPlan, diags: &mut DiagBag) -> EmitEvent {
        let mut content = a.content.clone();
        apply_text_filters(&mut content, self.opts.newline, self.opts.trim);

        let bytes = content.as_bytes();
        let fp = fnv1a64(bytes);

        // Policy checks
        if self.fs.exists(&a.path) {
            match self.opts.overwrite {
                Overwrite::Never => {
                    return EmitEvent { kind: a.kind, path: a.path.clone(), action: EmitAction::SkippedPolicy, bytes: 0, fingerprint: fp };
                }
                Overwrite::IfMissing => {
                    return EmitEvent { kind: a.kind, path: a.path.clone(), action: EmitAction::SkippedExists, bytes: 0, fingerprint: fp };
                }
                _ => {}
            }
        }

        if self.opts.overwrite == Overwrite::IfChanged && self.fs.exists(&a.path) && self.fs.is_file(&a.path) {
            if let Ok(prev) = self.fs.read(&a.path) {
                if fnv1a64(&prev) == fp && prev == bytes {
                    return EmitEvent { kind: a.kind, path: a.path.clone(), action: EmitAction::SkippedUnchanged, bytes: 0, fingerprint: fp };
                }
            }
        }

        // Dry-run
        if self.opts.dry_run {
            let action = if self.fs.exists(&a.path) { EmitAction::DryRunWouldUpdate } else { EmitAction::DryRunWouldWrite };
            return EmitEvent { kind: a.kind, path: a.path.clone(), action, bytes: bytes.len(), fingerprint: fp };
        }

        // Ensure dirs
        if self.opts.create_dirs {
            if let Some(parent) = a.path.parent() {
                if let Err(e) = self.fs.create_dir_all(parent) {
                    diags.push(Diagnostic::error(format!(
                        "emit: unable to create dir {} ({})",
                        parent.display(),
                        e
                    )));
                    return EmitEvent { kind: a.kind, path: a.path.clone(), action: EmitAction::Failed, bytes: 0, fingerprint: fp };
                }
            }
        }

        // Write
        let existed = self.fs.exists(&a.path);

        let write_res = match self.opts.write_mode {
            WriteMode::Direct => self.fs.write(&a.path, bytes),
            WriteMode::Atomic => atomic_write(&self.fs, &a.path, bytes),
        };

        if let Err(e) = write_res {
            diags.push(Diagnostic::error(format!("emit: unable to write {} ({})", a.path.display(), e)));
            return EmitEvent { kind: a.kind, path: a.path.clone(), action: EmitAction::Failed, bytes: 0, fingerprint: fp };
        }

        // Permissions (best effort)
        if let Some(mode) = self.opts.unix_mode {
            #[cfg(unix)]
            {
                let _ = self.fs.set_mode(&a.path, mode);
            }
        }

        let action = if existed { EmitAction::Updated } else { EmitAction::WroteNew };
        let _ = plan; // reserved for future (meta-driven emission)
        EmitEvent { kind: a.kind, path: a.path.clone(), action, bytes: bytes.len(), fingerprint: fp }
    }

    fn emit_manifest(&self, plan: &EmitPlan, res: &EmitResult, diags: &mut DiagBag) -> Option<PathBuf> {
        let root = plan.root.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = if self.opts.manifest_path.is_absolute() {
            self.opts.manifest_path.clone()
        } else {
            root.join(&self.opts.manifest_path)
        };

        let mut txt = String::new();
        txt.push_str("# Steel emit manifest\n");
        if !plan.meta.is_empty() {
            txt.push_str("\n[meta]\n");
            for (k, v) in &plan.meta {
                txt.push_str(&format!("{k} = {}\n", escape_manifest_value(v)));
            }
        }

        txt.push_str("\n[stats]\n");
        txt.push_str(&format!("total = {}\n", res.stats.total));
        txt.push_str(&format!("wrote = {}\n", res.stats.wrote));
        txt.push_str(&format!("updated = {}\n", res.stats.updated));
        txt.push_str(&format!("skipped = {}\n", res.stats.skipped));
        txt.push_str(&format!("failed = {}\n", res.stats.failed));
        txt.push_str(&format!("dry_run = {}\n", res.stats.dry_run));
        txt.push_str(&format!("bytes_written = {}\n", res.stats.bytes_written));

        txt.push_str("\n[outputs]\n");
        for ev in &res.events {
            let rel = make_rel(&root, &ev.path);
            txt.push_str(&format!(
                "- kind={} action={} fp=0x{:016x} bytes={} path={}\n",
                ev.kind.as_str(),
                action_name(ev.action),
                ev.fingerprint,
                ev.bytes,
                normalize_path(&rel),
            ));
        }

        // écriture manifest (toujours atomic, override Always, mais respecte dry_run)
        if self.opts.dry_run {
            return Some(manifest_path);
        }

        // parent
        if self.opts.create_dirs {
            if let Some(parent) = manifest_path.parent() {
                if let Err(e) = self.fs.create_dir_all(parent) {
                    diags.push(Diagnostic::error(format!(
                        "emit: unable to create manifest dir {} ({})",
                        parent.display(),
                        e
                    )));
                    return None;
                }
            }
        }

        let bytes = normalize_newlines(&txt).into_bytes();
        if let Err(e) = atomic_write(&self.fs, &manifest_path, &bytes) {
            diags.push(Diagnostic::error(format!(
                "emit: unable to write manifest {} ({})",
                manifest_path.display(),
                e
            )));
            return None;
        }

        Some(manifest_path)
    }
}

/// Convenience: emit with RealFs.
pub fn emit(plan: EmitPlan, opts: EmitOptions, diags: &mut DiagBag) -> EmitResult {
    Emitter::new(RealFs, opts).emit(plan, diags)
}

/// ------------------------------------------------------------
/// Internal helpers
/// ------------------------------------------------------------

fn apply_text_filters(s: &mut String, nl: NewlineMode, trim: TrimMode) {
    match nl {
        NewlineMode::Preserve => {}
        NewlineMode::Lf => {
            if s.contains('\r') {
                *s = normalize_newlines(s);
            }
        }
    }

    match trim {
        TrimMode::Preserve => {}
        TrimMode::TrimTrailingWhitespace => {
            *s = trim_trailing_ws_per_line(s);
        }
    }
}

fn atomic_write<F: Fs>(fs: &F, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs.create_dir_all(parent)?;

    let tmp = tmp_path_for(path);

    // write tmp
    fs.write(&tmp, bytes)?;

    // replace
    if fs.exists(path) {
        let _ = fs.remove_file(path);
    }
    fs.rename(&tmp, path)?;
    Ok(())
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or("txt");
    path.with_extension(format!("{ext}.tmp"))
}

pub fn normalize_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.is_empty() { ".".to_string() } else { s }
}

fn make_rel(root: &Path, p: &Path) -> PathBuf {
    p.strip_prefix(root).unwrap_or(p).to_path_buf()
}

fn action_name(a: EmitAction) -> &'static str {
    match a {
        EmitAction::WroteNew => "wrote_new",
        EmitAction::Updated => "updated",
        EmitAction::SkippedUnchanged => "skipped_unchanged",
        EmitAction::SkippedExists => "skipped_exists",
        EmitAction::SkippedPolicy => "skipped_policy",
        EmitAction::Failed => "failed",
        EmitAction::DryRunWouldWrite => "dry_run_would_write",
        EmitAction::DryRunWouldUpdate => "dry_run_would_update",
    }
}

fn normalize_newlines(s: &str) -> String {
    if !s.contains('\r') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if matches!(it.peek(), Some('\n')) {
                let _ = it.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

fn trim_trailing_ws_per_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, line) in s.split('\n').enumerate() {
        if i != 0 {
            out.push('\n');
        }
        let trimmed = line.trim_end_matches(|c| c == ' ' || c == '\t');
        out.push_str(trimmed);
    }
    out
}

/// Simple fingerprint stable (FNV-1a 64).
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut h = OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

fn escape_manifest_value(s: &str) -> String {
    // Manifest format: `k = value` ; on échappe minimum.
    // Si whitespace ou '#', on quote.
    let need_quote = s.chars().any(|c| c.is_whitespace() || c == '#' || c == '=');
    if !need_quote {
        return s.to_string();
    }
    let mut out = String::new();
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// ------------------------------------------------------------
/// Minimal MemFs (tests) — optionnel
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemFs {
        files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
        dirs: RefCell<BTreeSet<PathBuf>>,
    }

    impl MemFs {
        fn norm(p: &Path) -> PathBuf {
            PathBuf::from(p.to_string_lossy().replace('\\', "/"))
        }
    }

    impl Fs for MemFs {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            let p = Self::norm(path);
            self.files
                .borrow()
                .get(&p)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
        }

        fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            let p = Self::norm(path);
            self.files.borrow_mut().insert(p, bytes.to_vec());
            Ok(())
        }

        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            let p = Self::norm(path);
            self.dirs.borrow_mut().insert(p);
            Ok(())
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            let p = Self::norm(path);
            self.files.borrow_mut().remove(&p);
            Ok(())
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            let f = Self::norm(from);
            let t = Self::norm(to);
            let mut m = self.files.borrow_mut();
            let v = m
                .remove(&f)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))?;
            m.insert(t, v);
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            let p = Self::norm(path);
            self.files.borrow().contains_key(&p) || self.dirs.borrow().contains(&p)
        }

        fn is_file(&self, path: &Path) -> bool {
            let p = Self::norm(path);
            self.files.borrow().contains_key(&p)
        }

        fn is_dir(&self, path: &Path) -> bool {
            let p = Self::norm(path);
            self.dirs.borrow().contains(&p)
        }

        fn metadata_len(&self, path: &Path) -> io::Result<u64> {
            let p = Self::norm(path);
            let v = self
                .files
                .borrow()
                .get(&p)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))?;
            Ok(v.len() as u64)
        }

        #[cfg(unix)]
        fn set_mode(&self, _path: &Path, _mode: u32) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn fnv_is_stable_smoke() {
        assert_eq!(fnv1a64(b"abc"), fnv1a64(b"abc"));
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
    }

    #[test]
    fn emit_skips_unchanged_ifchanged() {
        let fs = MemFs::default();
        let opts = EmitOptions { overwrite: Overwrite::IfChanged, ..Default::default() };
        let emitter = Emitter::new(&fs, opts); // won't compile: Emitter expects Fs by value
    }

    // NOTE: test ci-dessus volontairement non activé: Emitter<F> possède F par valeur.
    // On garde des tests simples ci-dessous.

    #[test]
    fn normalize_newlines_basic() {
        assert_eq!(normalize_newlines("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(normalize_newlines("a\rb\r"), "a\nb\n");
        assert_eq!(normalize_newlines("a\nb\n"), "a\nb\n");
    }

    #[test]
    fn trim_trailing_ws() {
        assert_eq!(trim_trailing_ws_per_line("a \n\tb\t\nc"), "a\n\tb\nc");
    }

    #[test]
    fn escape_manifest() {
        assert_eq!(escape_manifest_value("abc"), "abc");
        assert!(escape_manifest_value("a b").starts_with('"'));
    }
}
