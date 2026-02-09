//! mcfg driver (configuration generation) for Steel.
//!
//! Rôle:
//! - Localiser le buildfile (steel / steelconf / build.muf).
//! - Charger et valider la configuration (phase resolve).
//! - Scanner le workspace pour trouver les sources (.vitte / .vit).
//! - Optionnel: segmenter par répertoire et générer des fichiers `*.muff` par unité.
//! - Générer un artefact global `steel.log` (consommé par Vitte).
//!
//! Contraintes:
//! - std uniquement (pas de clap/serde/walkdir).
//! - Diagnostics structurées via `diag.rs`.
//!
//! Intégration attendue côté binaire:
//! - `mcfg` (ou `build steel`) appelle `Driver::run()`.
//! - Si `report.diags.has_error()` => exit != 0.
//!
//! Remarques format:
//! - Le format `.mff` / `.muff` ci-dessous est volontairement simple (kv + sections).
//!   Il sert de “freeze” déterministe: inputs/outputs/params. Le compilateur Vitte peut
//!   ensuite définir un parseur strict pour ces fichiers si nécessaire.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::diag::{err_at, warn_at, DiagBag, Diagnostic, Label, RenderOptions, Severity, SourceMap, Span};

/// Nom par défaut de l’artefact global gelé.
pub const DEFAULT_MFF_NAME: &str = "steel.log";

/// Candidats buildfile racine (ordre de priorité).
pub const DEFAULT_BUILDFILE_CANDIDATES: &[&str] = &["steel", "steelconf", "build.muf"];

/// Répertoires ignorés lors du scan.
pub const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "dist",
    "build",
    ".steel",
    ".muff",
    "node_modules",
];

/// Extensions sources reconnues.
pub const DEFAULT_SOURCE_EXTS: &[&str] = &["vitte", "vit"];

/// Plateforme de sortie (impacte l’extension exécutable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
    BSD,
    Solaris,
    Unknown,
}

impl Platform {
    pub fn detect() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "freebsd")
            || cfg!(target_os = "openbsd")
            || cfg!(target_os = "netbsd")
            || cfg!(target_os = "dragonfly")
        {
            Platform::BSD
        } else if cfg!(target_os = "solaris") || cfg!(target_os = "illumos") {
            Platform::Solaris
        } else {
            Platform::Unknown
        }
    }

    pub fn exe_ext(self) -> &'static str {
        match self {
            Platform::Windows => "exe",
            _ => "", // unix-like: pas d’extension
        }
    }
}

/// Mode de segmentation workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentationMode {
    /// Un seul artefact global (steel.log) + pas de fichiers par dossier.
    Off,
    /// Un fichier `*.muff` par répertoire contenant des sources.
    PerDir,
}

impl Default for SegmentationMode {
    fn default() -> Self {
        SegmentationMode::PerDir
    }
}

/// Options driver.
#[derive(Debug, Clone)]
pub struct DriverOptions {
    /// Répertoire de départ (souvent `.`).
    pub start_dir: PathBuf,
    /// Noms buildfile possibles.
    pub buildfile_candidates: Vec<String>,
    /// Nom fichier global `.mff`.
    pub mff_name: String,
    /// Répertoire interne (cache/artefacts intermédiaires).
    pub internal_dir: PathBuf,
    /// Répertoire final de sorties (artefacts “exportables”).
    pub out_dir: PathBuf,
    /// Dossiers ignorés.
    pub ignored_dirs: BTreeSet<String>,
    /// Extensions sources reconnues.
    pub source_exts: BTreeSet<String>,
    /// Segmentation.
    pub segmentation: SegmentationMode,
    /// Plateforme (impact naming sorties).
    pub platform: Platform,
    /// Émettre du debug dans le `.mff` (meta).
    pub embed_meta: bool,
}

impl Default for DriverOptions {
    fn default() -> Self {
        Self {
            start_dir: PathBuf::from("."),
            buildfile_candidates: DEFAULT_BUILDFILE_CANDIDATES.iter().map(|s| s.to_string()).collect(),
            mff_name: DEFAULT_MFF_NAME.to_string(),
            internal_dir: PathBuf::from(".steel"),
            out_dir: PathBuf::from("dist"),
            ignored_dirs: DEFAULT_IGNORED_DIRS.iter().map(|s| s.to_string()).collect(),
            source_exts: DEFAULT_SOURCE_EXTS.iter().map(|s| s.to_string()).collect(),
            segmentation: SegmentationMode::PerDir,
            platform: Platform::detect(),
            embed_meta: true,
        }
    }
}

/// Résultat du driver.
#[derive(Debug, Default, Clone)]
pub struct DriverOutput {
    /// Répertoire racine du projet (où se trouve le buildfile).
    pub root: PathBuf,
    /// Buildfile résolu.
    pub buildfile: PathBuf,
    /// Artefact global `.mff`.
    pub mff_path: PathBuf,
    /// Fichiers segmentés `*.muff` (optionnel).
    pub per_dir_muff: Vec<PathBuf>,
}

/// Rapport d’exécution.
#[derive(Debug, Default)]
pub struct RunReport {
    pub diags: DiagBag,
    pub sources: SourceMap,
    pub output: Option<DriverOutput>,
}

impl RunReport {
    pub fn ok(&self) -> bool {
        !self.diags.has_error()
    }
}

/// Driver principal.
#[derive(Debug)]
pub struct Driver {
    pub opts: DriverOptions,
}

impl Driver {
    pub fn new(opts: DriverOptions) -> Self {
        Self { opts }
    }

    pub fn run(&self) -> RunReport {
        let mut report = RunReport {
            diags: DiagBag::new(),
            sources: SourceMap::new(),
            output: None,
        };

        // 1) Root resolution + buildfile.
        let (root, buildfile) = match self.find_project_root(&mut report) {
            Some(x) => x,
            None => return report,
        };

        // 2) Read buildfile (for diagnostics / freeze metadata).
        let (buildfile_id, build_txt) = match read_text_file_with_diag(&buildfile, &mut report.sources, &mut report.diags) {
            Some(x) => x,
            None => return report,
        };

        // 3) Validate buildfile (placeholder: syntax/semantic checks).
        // Ici vous branchez votre parseur/validator Steel (EBNF bake v2).
        self.validate_buildfile_stub(buildfile_id, &build_txt, &mut report.diags);

        // 4) Scan sources (.vitte / .vit).
        let all_sources = scan_sources(&root, &self.opts, &mut report.diags);

        // 5) Segmentation (optionnelle) + génération.
        let mut out = DriverOutput {
            root: root.clone(),
            buildfile: buildfile.clone(),
            mff_path: root.join(&self.opts.mff_name),
            per_dir_muff: Vec::new(),
        };

        // Ensure internal dir exists.
        let _ = fs::create_dir_all(root.join(&self.opts.internal_dir));

        let mut per_dir_units: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
        if matches!(self.opts.segmentation, SegmentationMode::PerDir) {
            per_dir_units = group_sources_by_dir(&root, &all_sources);
        }

        // Write per-dir *.muff (optional).
        if matches!(self.opts.segmentation, SegmentationMode::PerDir) {
            for (dir, files) in &per_dir_units {
                if files.is_empty() {
                    continue;
                }
                let unit_name = unit_slug(dir.strip_prefix(&root).unwrap_or(dir));
                let muff_path = dir.join(format!("{unit_name}.muff"));
                let muff_txt = render_unit_muff(&root, dir, files, &self.opts);

                if let Err(e) = atomic_write_text(&muff_path, &muff_txt) {
                    report.diags.push(Diagnostic::error(format!(
                        "unable to write unit config: {} ({})",
                        muff_path.display(),
                        e
                    )));
                } else {
                    out.per_dir_muff.push(muff_path);
                }
            }
        }

        // Write global steel.log.
        let mff_txt = render_global_mff(&root, &buildfile, &all_sources, &per_dir_units, &self.opts);
        if let Err(e) = atomic_write_text(&out.mff_path, &mff_txt) {
            report.diags.push(Diagnostic::error(format!(
                "unable to write {} ({})",
                out.mff_path.display(),
                e
            )));
        }

        report.output = Some(out);
        report
    }

    fn find_project_root(&self, report: &mut RunReport) -> Option<(PathBuf, PathBuf)> {
        let start = canonicalize_or(self.opts.start_dir.clone());
        let mut cur = start.as_path();

        loop {
            for cand in &self.opts.buildfile_candidates {
                let p = cur.join(cand);
                if p.is_file() {
                    let root = cur.to_path_buf();
                    return Some((root, p));
                }
            }

            let parent = cur.parent();
            match parent {
                Some(p) => cur = p,
                None => {
                    report.diags.push(Diagnostic::error(format!(
                        "no buildfile found (candidates: {}) starting from {}",
                        self.opts.buildfile_candidates.join(", "),
                        start.display()
                    )));
                    return None;
                }
            }
        }
    }

    /// Placeholder: branchez ici le parseur/validator Steel (EBNF bake v2).
    fn validate_buildfile_stub(&self, buildfile_id: u32, txt: &str, diags: &mut DiagBag) {
        // Exemple: header attendu "steel bake <int>"
        // Ce stub fait une validation superficielle “signal”.
        let head = txt.lines().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
        let Some(h) = head else {
            diags.push(Diagnostic::error("buildfile is empty"));
            return;
        };

        let htrim = h.trim();
        if !htrim.starts_with("steel bake") {
            // Span approximatif: début du fichier.
            diags.push(
                err_at(Span::new(buildfile_id, 0, (htrim.len().min(64)) as u32), "invalid buildfile header")
                    .with_help("expected: `steel bake <version>`")
                    .with_note(format!("found: `{htrim}`")),
            );
        }
    }
}

/// -------------------------------
/// Scan / grouping
/// -------------------------------

fn scan_sources(root: &Path, opts: &DriverOptions, diags: &mut DiagBag) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let read = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                diags.push(Diagnostic::warning(format!("unable to read dir {} ({})", dir.display(), e)));
                continue;
            }
        };

        for ent in read.flatten() {
            let p = ent.path();

            let md = match ent.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if md.is_dir() {
                if is_ignored_dir(&p, root, opts) {
                    continue;
                }
                stack.push(p);
                continue;
            }

            if md.is_file() && is_source_file(&p, opts) {
                out.push(p);
            }
        }
    }

    out.sort();
    out
}

fn is_ignored_dir(path: &Path, root: &Path, opts: &DriverOptions) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let name = rel.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if opts.ignored_dirs.contains(name) {
        return true;
    }
    // ignore hidden internal dirs by default
    if name.starts_with('.') && (name == ".steel" || name == ".muff") {
        return true;
    }
    false
}

fn is_source_file(path: &Path, opts: &DriverOptions) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    opts.source_exts.contains(ext)
}

fn group_sources_by_dir(root: &Path, sources: &[PathBuf]) -> BTreeMap<PathBuf, Vec<PathBuf>> {
    let mut map: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for p in sources {
        let dir = p.parent().unwrap_or(root).to_path_buf();
        map.entry(dir).or_default().push(p.clone());
    }
    for v in map.values_mut() {
        v.sort();
    }
    map
}

/// -------------------------------
/// Rendering formats (.muff / .mff)
/// -------------------------------

fn render_unit_muff(root: &Path, unit_dir: &Path, files: &[PathBuf], opts: &DriverOptions) -> String {
    let rel_dir = unit_dir.strip_prefix(root).unwrap_or(unit_dir);
    let unit_name = unit_slug(rel_dir);

    // Naming outputs “type safe” (artefacts).
    let out_lib = PathBuf::from("src/out/lib").join(format!("{unit_name}.va"));
    let out_obj = PathBuf::from("src/out/bin").join(format!("{unit_name}.vo"));

    let exe = if opts.platform == Platform::Windows {
        PathBuf::from("src/out/bin").join(format!("{unit_name}.exe"))
    } else {
        PathBuf::new()
    };

    let mut s = String::new();
    s.push_str("# Steel unit config (generated)\n");
    s.push_str(&format!("unit {}\n", unit_name));
    s.push_str(&format!("dir {}\n", normalize_path(rel_dir)));


    s.push_str("\n[sources]\n");
    for p in files {
        let rel = p.strip_prefix(root).unwrap_or(p);
        s.push_str(&format!("- {}\n", normalize_path(rel)));
    }

    s.push_str("\n[outputs]\n");
    s.push_str(&format!("- lib {}\n", normalize_path(&out_lib)));
    s.push_str(&format!("- obj {}\n", normalize_path(&out_obj)));
    if opts.platform == Platform::Windows {
        s.push_str(&format!("- exe {}\n", normalize_path(&exe)));
    }

    s.push_str("\n[params]\n");
    s.push_str(&format!("platform {}\n", platform_name(opts.platform)));
    s.push_str("profile default\n");

    s
}

fn render_global_mff(
    root: &Path,
    buildfile: &Path,
    sources: &[PathBuf],
    per_dir: &BTreeMap<PathBuf, Vec<PathBuf>>,
    opts: &DriverOptions,
) -> String {
    let mut s = String::new();
    s.push_str("# steel.log (freeze)\n");

    if opts.embed_meta {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()).unwrap_or(0);
        s.push_str(&format!("# generated_at_unix {}\n", ts));
        s.push_str(&format!("# platform {}\n", platform_name(opts.platform)));
    }

    let rel_build = buildfile.strip_prefix(root).unwrap_or(buildfile);
    s.push_str(&format!("\n[buildfile]\npath {}\n", normalize_path(rel_build)));

    s.push_str("\n[workspace]\n");
    s.push_str(&format!("root {}\n", normalize_path(Path::new("."))));
    s.push_str(&format!("internal {}\n", normalize_path(&opts.internal_dir)));
    s.push_str(&format!("out {}\n", normalize_path(&opts.out_dir)));
    s.push_str(&format!("segmentation {}\n", match opts.segmentation { SegmentationMode::Off => "off", SegmentationMode::PerDir => "per_dir" }));

    s.push_str("\n[sources]\n");
    for p in sources {
        let rel = p.strip_prefix(root).unwrap_or(p);
        s.push_str(&format!("- {}\n", normalize_path(rel)));
    }

    if matches!(opts.segmentation, SegmentationMode::PerDir) {
        s.push_str("\n[units]\n");
        for (dir, files) in per_dir {
            if files.is_empty() {
                continue;
            }
            let rel_dir = dir.strip_prefix(root).unwrap_or(dir);
            let unit_name = unit_slug(rel_dir);
            s.push_str(&format!("- {} {}\n", unit_name, normalize_path(rel_dir)));
        }
    }

    s
}

/// -------------------------------
/// I/O helpers
/// -------------------------------

fn read_text_file_with_diag(path: &Path, sm: &mut SourceMap, diags: &mut DiagBag) -> Option<(u32, String)> {
    let txt = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            diags.push(Diagnostic::error(format!("unable to read {} ({})", path.display(), e)));
            return None;
        }
    };
    let fid = sm.add_file(path.to_path_buf(), Some(txt.clone()));
    Some((fid, txt))
}

fn atomic_write_text(path: &Path, text: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(OsStr::to_str).unwrap_or("txt")
    ));

    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }

    // Best effort atomic replace.
    // On Windows, rename over existing can fail; fallback remove.
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(tmp, path)?;
    Ok(())
}

/// -------------------------------
/// Naming / formatting
/// -------------------------------

fn unit_slug(rel_dir: &Path) -> String {
    // Exemple demandé: "compilation_vitte_folder_generate_..." -> on standardise.
    // Ici: `compilation_<path_segments_joined>_generate_`.
    let mut parts = Vec::new();
    for c in rel_dir.components() {
        if let Component::Normal(os) = c {
            if let Some(s) = os.to_str() {
                if !s.is_empty() {
                    parts.push(sanitize_ident(s));
                }
            }
        }
    }
    if parts.is_empty() {
        return "compilation_root_generate_".to_string();
    }
    format!("compilation_{}_generate_", parts.join("_"))
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    // éviter ident vide
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn normalize_path(p: &Path) -> String {
    // Rend un chemin stable (slash) pour fichiers config.
    let s = p.to_string_lossy().replace('\\', "/");
    if s.is_empty() { ".".to_string() } else { s }
}

fn platform_name(p: Platform) -> &'static str {
    match p {
        Platform::Windows => "windows",
        Platform::MacOS => "macos",
        Platform::Linux => "linux",
        Platform::BSD => "bsd",
        Platform::Solaris => "solaris",
        Platform::Unknown => "unknown",
    }
}

fn canonicalize_or(p: PathBuf) -> PathBuf {
    fs::canonicalize(&p).unwrap_or(p)
}

/// -------------------------------
/// Optional: CLI-style convenience
/// -------------------------------

/// Exécute et imprime les diagnostics en STDERR.
pub fn run_and_print(opts: DriverOptions) -> i32 {
    let driver = Driver::new(opts);
    let mut report = driver.run();

    // tri diag stable
    report.diags.sort_deterministic();

    // rendu texte
    let _ = crate::diag::render_to_stderr(&report.diags, &report.sources, &RenderOptions::default());

    if report.ok() { 0 } else { 1 }
}

/// -------------------------------
/// Tests
/// -------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_slug_is_stable() {
        assert_eq!(unit_slug(Path::new("src/in/folder_a")), "compilation_src_in_folder_a_generate_");
        assert_eq!(unit_slug(Path::new(".")), "compilation_root_generate_");
    }

    #[test]
    fn sanitize_ident_basic() {
        assert_eq!(sanitize_ident("AbC-12"), "abc_12");
        assert_eq!(sanitize_ident(""), "_");
    }

    #[test]
    fn normalize_path_basic() {
        assert_eq!(normalize_path(Path::new("a\\b\\c")), "a/b/c");
    }
}