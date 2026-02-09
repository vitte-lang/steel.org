//! build_muf — `build steel` (Configuration phase)
//!
//! Implements the **Configuration** step of the Steel pipeline:
//! parse/validate/resolve workspace configuration, then emit the canonical
//! resolved configuration artifact (optional emit).
//!
//! Design constraints
//! - std-only (no external crates)
//! - deterministic output formatting
//! - best-effort by default (strict mode available)
//!
//! Integration points
//! - A real steelconf parser/resolver can replace `resolve_workspace()`.
//! - A build runner can consume the emitted config when `--emit` is provided.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Canonical emitted file name (legacy).
pub const DEFAULT_EMIT_NAME: &str = "steelconfig.mff";

/// Current mff schema version (header: `mff <version>`).
pub const MFF_SCHEMA_VERSION: u32 = 1;

/// Default steelconf names for discovery.
pub const DEFAULT_MUFFINFILE_NAMES: &[&str] = &["steelconf", "steel"];

pub type Result<T> = std::result::Result<T, BuildMufError>;

/// Errors produced by the configuration phase.
#[derive(Debug)]
pub enum BuildMufError {
    Io {
        op: &'static str,
        path: PathBuf,
        err: io::Error,
    },
    Arg {
        msg: String,
    },
    Validate {
        msg: String,
    },
}

impl fmt::Display for BuildMufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildMufError::Io { op, path, err } => {
                write!(f, "I/O error during {op} on {}: {err}", path.display())
            }
            BuildMufError::Arg { msg } => write!(f, "argument error: {msg}"),
            BuildMufError::Validate { msg } => write!(f, "validation error: {msg}"),
        }
    }
}

impl std::error::Error for BuildMufError {}

fn io_err(op: &'static str, path: impl Into<PathBuf>, err: io::Error) -> BuildMufError {
    BuildMufError::Io {
        op,
        path: path.into(),
        err,
    }
}

/// Options for `build steel`.
#[derive(Debug, Clone)]
pub struct BuildMufOptions {
    /// Workspace root directory.
    pub root_dir: PathBuf,

    /// Explicit steelconf path (overrides discovery).
    pub steel_file: Option<PathBuf>,

    /// Selected build profile (e.g. debug/release/custom).
    pub profile: Option<String>,

    /// Selected target triple (e.g. x86_64-unknown-linux-gnu).
    pub target: Option<String>,

    /// Emit path for resolved config. If None, no file is emitted.
    pub emit_path: Option<PathBuf>,

    /// Offline mode (no network during resolution).
    pub offline: bool,

    /// Strict mode: fail on IO irregularities / unexpected situations.
    pub strict: bool,

    /// Disable tool fingerprint collection (tool --version).
    pub no_tool_fingerprint: bool,

    /// Include hidden dirs/files during discovery.
    pub include_hidden: bool,

    /// Follow symlinks during discovery.
    pub follow_symlinks: bool,

    /// Maximum recursion depth for discovery.
    pub max_depth: usize,

    /// If true, also print the emitted config to stdout.
    pub print: bool,

    /// Verbose diagnostics.
    pub verbose: bool,
}

impl Default for BuildMufOptions {
    fn default() -> Self {
        Self {
            root_dir: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            steel_file: None,
            profile: None,
            target: None,
            emit_path: None,
            offline: env_flag("MUFFIN_OFFLINE"),
            strict: false,
            no_tool_fingerprint: false,
            include_hidden: false,
            follow_symlinks: false,
            max_depth: 16,
            print: false,
            verbose: false,
        }
    }
}

/// Canonical resolved configuration (mff v1, text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub schema_version: u32,

    pub project_root: PathBuf,
    pub steelfile_path: PathBuf,

    pub profile: String,
    pub target: String,

    pub paths: ResolvedPaths,
    pub toolchain: ToolchainInfo,

    /// Deterministic map of resolved variables.
    pub vars: BTreeMap<String, String>,

    /// Deterministic fingerprint for cache invalidation.
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub build_dir: PathBuf,
    pub dist_dir: PathBuf,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainInfo {
    pub cc: Option<String>,
    pub cxx: Option<String>,
    pub ar: Option<String>,
    pub ld: Option<String>,
    pub rustc: Option<String>,
    pub python: Option<String>,
    pub ocaml: Option<String>,
    pub ghc: Option<String>,

    /// Tool versions (best effort): tool -> "version line"
    pub versions: BTreeMap<String, String>,
}

impl ToolchainInfo {
    pub fn from_env(best_effort_versions: bool) -> Self {
        let explicit_python = env::var("MUFFIN_TOOLCHAIN_PYTHON").ok();
        let explicit_ocaml = env::var("MUFFIN_TOOLCHAIN_OCAML").ok();
        let explicit_ghc = env::var("MUFFIN_TOOLCHAIN_GHC").ok();
        let mut tc = ToolchainInfo {
            cc: env::var("CC").ok(),
            cxx: env::var("CXX").ok(),
            ar: env::var("AR").ok(),
            ld: env::var("LD").ok(),
            rustc: env::var("RUSTC").ok().or_else(|| Some("rustc".to_string())),
            python: explicit_python.clone().or_else(|| env::var("PYTHON").ok()),
            ocaml: explicit_ocaml.clone().or_else(|| env::var("OCAMLPATH").ok()),
            ghc: explicit_ghc.clone().or_else(|| env::var("GHC_PACKAGE_PATH").ok()),
            versions: BTreeMap::new(),
        };

        if best_effort_versions {
            if let Some(t) = tc.cc.clone() {
                if let Some(v) = tool_version_line(&t) {
                    tc.versions.insert("cc".to_string(), v);
                }
            }
            if let Some(t) = tc.cxx.clone() {
                if let Some(v) = tool_version_line(&t) {
                    tc.versions.insert("cxx".to_string(), v);
                }
            }
            if let Some(t) = tc.ar.clone() {
                if let Some(v) = tool_version_line(&t) {
                    tc.versions.insert("ar".to_string(), v);
                }
            }
            if let Some(t) = tc.ld.clone() {
                if let Some(v) = tool_version_line(&t) {
                    tc.versions.insert("ld".to_string(), v);
                }
            }
            if let Some(t) = tc.rustc.clone() {
                if let Some(v) = tool_version_line(&t) {
                    tc.versions.insert("rustc".to_string(), v);
                }
            }
            if explicit_python.is_some() {
                if let Some(t) = tc.python.clone() {
                    if let Some((impl_name, ver)) = python_impl_and_version(&t) {
                        tc.versions.insert("python".to_string(), ver);
                        tc.versions.insert("python_impl".to_string(), impl_name);
                    }
                }
            }
            if explicit_ghc.is_some() {
                if let Some(t) = tc.ghc.clone() {
                    if let Some(v) = ghc_numeric_version(&t) {
                        tc.versions.insert("ghc".to_string(), v);
                    }
                }
            }
        }

        tc
    }
}

/// Parse CLI args (arguments after `build steel`) into options.
///
/// Flags:
/// - `--root <path>`
/// - `--file <path>`
/// - `--profile <name>`
/// - `--target <triple>`
/// - `--emit <path>`
/// - `--offline`
/// - `--strict`
/// - `--no-tool-fingerprint`
/// - `--include-hidden`
/// - `--follow-symlinks`
/// - `--max-depth <n>`
/// - `--print`
/// - `-v` / `--verbose`
pub fn parse_args(args: &[String]) -> Result<BuildMufOptions> {
    let mut o = BuildMufOptions::default();

    // Track whether we already consumed a positional root.
    let mut positional_root_set = false;

    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--root expects a path".into(),
                    })?;
                o.root_dir = PathBuf::from(v);
                positional_root_set = true;
            }
            "--file" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--file expects a path".into(),
                    })?;
                o.steel_file = Some(PathBuf::from(v));
            }
            "--profile" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--profile expects a name".into(),
                    })?;
                o.profile = Some(v.to_string());
            }
            "--target" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--target expects a triple".into(),
                    })?;
                o.target = Some(v.to_string());
            }
            "--emit" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--emit expects a path".into(),
                    })?;
                o.emit_path = Some(PathBuf::from(v));
            }
            "--offline" => o.offline = true,
            "--strict" => o.strict = true,
            "--no-tool-fingerprint" => o.no_tool_fingerprint = true,
            "--include-hidden" => o.include_hidden = true,
            "--follow-symlinks" => o.follow_symlinks = true,
            "--max-depth" => {
                let v = it
                    .next()
                    .ok_or_else(|| BuildMufError::Arg {
                        msg: "--max-depth expects a number".into(),
                    })?;
                let n: usize = v.parse().map_err(|_| BuildMufError::Arg {
                    msg: format!("invalid --max-depth: {v}"),
                })?;
                o.max_depth = n;
            }
            "--print" => o.print = true,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => {
                return Err(BuildMufError::Arg {
                    msg: help_text().to_string(),
                })
            }
            x if x.starts_with('-') => {
                return Err(BuildMufError::Arg {
                    msg: format!("unknown flag: {x}"),
                })
            }
            // Support: `build steel <path>` as shorthand root (single positional).
            other => {
                if positional_root_set {
                    return Err(BuildMufError::Arg {
                        msg: format!("unexpected argument: {other}"),
                    });
                }
                o.root_dir = PathBuf::from(other);
                positional_root_set = true;
            }
        }
    }

    Ok(o)
}

pub fn help_text() -> &'static str {
    "steel — Configuration phase (no flags; parameters are read from steelconf)

Usage:
  steel

Flags:
  --root <path>             Workspace root (default: cwd)
  --file <path>             Explicit steelconf path (skip discovery)
  --profile <name>          Profile (default: MUFFIN_PROFILE or debug)
  --target <triple>         Target triple (default: host triple best-effort)
  --emit <path>             Emit path (optional; no default)
  --print                   Also print emitted config to stdout
  --offline                 Offline mode
  --strict                  Fail on any IO irregularity
  --no-tool-fingerprint     Disable tool version fingerprint
  --include-hidden          Include hidden dirs/files during discovery
  --follow-symlinks         Follow symlinks during discovery
  --max-depth <n>           Discovery recursion depth (default: 16)
  -v, --verbose             Verbose diagnostics
  -h, --help                Print this help"
}

/// Execute the configuration phase:
/// 1) discover steelconf if needed
/// 2) validate minimal invariants
/// 3) resolve into a canonical `ResolvedConfig`
/// 4) optionally emit config when `--emit` is provided
pub fn run(opts: &BuildMufOptions) -> Result<ResolvedConfig> {
    let root = normalize_path(&opts.root_dir);

    let steelfile = if let Some(p) = &opts.steel_file {
        root_join_if_relative(&root, p)
    } else {
        discover_steelfile(&root, opts)?
    };

    validate_inputs(&root, &steelfile, opts)?;
    let resolved = resolve_workspace(&root, &steelfile, opts)?;

    if let Some(emit_path) = choose_emit_path(&root, opts) {
        emit_mcfg(&emit_path, &resolved)?;
    }

    if opts.print {
        println!("{}", format_mcfg(&resolved));
    }

    Ok(resolved)
}

/// Discover steelconf/steel by scanning the root directory (bounded recursion).
fn discover_steelfile(root: &Path, opts: &BuildMufOptions) -> Result<PathBuf> {
    // Fast path: root candidates.
    for n in DEFAULT_MUFFINFILE_NAMES {
        let p = root.join(n);
        if p.is_file() {
            return Ok(p);
        }
    }

    // Best-effort bounded scan.
    match discover_with_local_scan(root, opts) {
        Some(p) => Ok(p),
        None => Err(BuildMufError::Validate {
            msg: format!(
                "no steelconf found in {} (expected one of: {:?})",
                root.display(),
                DEFAULT_MUFFINFILE_NAMES
            ),
        }),
    }
}

fn discover_with_local_scan(root: &Path, opts: &BuildMufOptions) -> Option<PathBuf> {
    // Deterministic DFS stack.
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    let ignore: Vec<OsString> = vec![
        OsString::from(".git"),
        OsString::from(".hg"),
        OsString::from(".svn"),
        OsString::from("target"),
        OsString::from("node_modules"),
        OsString::from("dist"),
        OsString::from("build"),
        OsString::from(".steel"),
        OsString::from(".steel-cache"),
    ];

    while let Some((dir, depth)) = stack.pop() {
        if depth > opts.max_depth {
            continue;
        }

        let rd = match fs::read_dir(&dir) {
            Ok(v) => v,
            Err(_) => {
                if opts.strict {
                    return None;
                }
                continue;
            }
        };

        let mut entries: Vec<fs::DirEntry> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for ent in entries {
            let name = ent.file_name();

            if !opts.include_hidden {
                if let Some(s) = name.to_str() {
                    if s.starts_with('.') {
                        continue;
                    }
                }
            }

            let path = ent.path();

            let md = if opts.follow_symlinks {
                fs::metadata(&path).ok()
            } else {
                fs::symlink_metadata(&path).ok()
            };

            let md = match md {
                Some(v) => v,
                None => {
                    if opts.strict {
                        return None;
                    }
                    continue;
                }
            };

            if md.is_dir() {
                if ignore.iter().any(|x| x.as_os_str() == name.as_os_str()) {
                    continue;
                }
                stack.push((path, depth + 1));
                continue;
            }

            if md.is_file() {
                if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                    if fname == "steelconf" || fname == "steel" {
                        return Some(path);
                    }
                }
            }
        }
    }

    None
}

fn validate_inputs(root: &Path, steelfile: &Path, opts: &BuildMufOptions) -> Result<()> {
    if !root.is_dir() {
        return Err(BuildMufError::Validate {
            msg: format!("root is not a directory: {}", root.display()),
        });
    }

    if !steelfile.is_file() {
        return Err(BuildMufError::Validate {
            msg: format!("steelconf not found: {}", steelfile.display()),
        });
    }

    // In strict mode, ensure steelconf is under root.
    if opts.strict && steelfile.strip_prefix(root).is_err() {
        return Err(BuildMufError::Validate {
            msg: format!(
                "--strict: steelconf must be under root. root={}, file={}",
                root.display(),
                steelfile.display()
            ),
        });
    }

    Ok(())
}

/// Resolve workspace configuration into a canonical config snapshot.
///
/// Replace this function with a real steelconf parser/resolver.
fn resolve_workspace(root: &Path, steelfile: &Path, opts: &BuildMufOptions) -> Result<ResolvedConfig> {
    let profile = opts
        .profile
        .clone()
        .or_else(|| env::var("MUFFIN_PROFILE").ok())
        .unwrap_or_else(|| "debug".to_string());

    let target = opts
        .target
        .clone()
        .or_else(|| env::var("MUFFIN_TARGET").ok())
        .unwrap_or_else(host_triple_best_effort);

    let paths = ResolvedPaths {
        build_dir: root.join("build"),
        dist_dir: root.join("dist"),
        cache_dir: root.join(".steel-cache"),
    };

    let collect_versions = !opts.no_tool_fingerprint;
    let toolchain = ToolchainInfo::from_env(collect_versions);

    // Deterministic vars: explicit and ordered (BTreeMap).
    let mut vars = BTreeMap::new();
    vars.insert("steel.profile".to_string(), profile.clone());
    vars.insert("steel.target".to_string(), target.clone());
    vars.insert("steel.offline".to_string(), opts.offline.to_string());
    vars.insert("steel.root".to_string(), root.to_string_lossy().to_string());
    vars.insert("steel.file".to_string(), steelfile.to_string_lossy().to_string());
    insert_toolchain_env_vars(&mut vars, &toolchain);

    // Best-effort: read steelconf bytes and hash for fingerprint.
    let file_bytes =
        fs::read(steelfile).map_err(|e| io_err("read", steelfile.to_path_buf(), e))?;

    let fingerprint = compute_fingerprint(&file_bytes, &profile, &target, &toolchain);

    if opts.verbose {
        eprintln!("[steel] root       : {}", root.display());
        eprintln!("[steel] steelfile : {}", steelfile.display());
        eprintln!("[steel] profile    : {profile}");
        eprintln!("[steel] target     : {target}");
        eprintln!("[steel] fingerprint: {fingerprint}");
    }

    Ok(ResolvedConfig {
        schema_version: MFF_SCHEMA_VERSION,
        project_root: root.to_path_buf(),
        steelfile_path: steelfile.to_path_buf(),
        profile,
        target,
        paths,
        toolchain,
        vars,
        fingerprint,
    })
}

fn choose_emit_path(root: &Path, opts: &BuildMufOptions) -> Option<PathBuf> {
    opts.emit_path
        .as_ref()
        .map(|p| root_join_if_relative(root, p))
}

/// Emit `steelconfig.mff`.
pub fn emit_mcfg(path: &Path, cfg: &ResolvedConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| io_err("create_dir_all", parent.to_path_buf(), e))?;
    }

    let text = format_mcfg(cfg);
    fs::write(path, text).map_err(|e| io_err("write", path.to_path_buf(), e))?;
    Ok(())
}

/// Render config as deterministic text (mff v1).
pub fn format_mcfg(cfg: &ResolvedConfig) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!("mff {}\n\n", cfg.schema_version));

    // Project
    out.push_str("project\n");
    out.push_str(&format!(
        "  root \"{}\"\n",
        escape(cfg.project_root.to_string_lossy().as_ref())
    ));
    out.push_str(&format!(
        "  steelfile \"{}\"\n",
        escape(cfg.steelfile_path.to_string_lossy().as_ref())
    ));
    out.push_str(".end\n\n");

    // Selection
    out.push_str("select\n");
    out.push_str(&format!("  profile \"{}\"\n", escape(&cfg.profile)));
    out.push_str(&format!("  target \"{}\"\n", escape(&cfg.target)));
    out.push_str(".end\n\n");

    // Paths
    out.push_str("paths\n");
    out.push_str(&format!(
        "  build \"{}\"\n",
        escape(cfg.paths.build_dir.to_string_lossy().as_ref())
    ));
    out.push_str(&format!(
        "  dist \"{}\"\n",
        escape(cfg.paths.dist_dir.to_string_lossy().as_ref())
    ));
    out.push_str(&format!(
        "  cache \"{}\"\n",
        escape(cfg.paths.cache_dir.to_string_lossy().as_ref())
    ));
    out.push_str(".end\n\n");

    // Toolchain
    out.push_str("toolchain\n");
    if let Some(v) = &cfg.toolchain.cc {
        out.push_str(&format!("  cc \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.cxx {
        out.push_str(&format!("  cxx \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.ar {
        out.push_str(&format!("  ar \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.ld {
        out.push_str(&format!("  ld \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.rustc {
        out.push_str(&format!("  rustc \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.python {
        out.push_str(&format!("  python \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.ocaml {
        out.push_str(&format!("  ocaml \"{}\"\n", escape(v)));
    }
    if let Some(v) = &cfg.toolchain.ghc {
        out.push_str(&format!("  ghc \"{}\"\n", escape(v)));
    }

    if !cfg.toolchain.versions.is_empty() {
        out.push_str("\n  versions\n");
        for (k, v) in &cfg.toolchain.versions {
            out.push_str(&format!("    {} \"{}\"\n", k, escape(v)));
        }
        out.push_str("  .end\n");
    }
    out.push_str(".end\n\n");

    // Vars
    out.push_str("vars\n");
    for (k, v) in &cfg.vars {
        out.push_str(&format!(
            "  set \"{}\" \"{}\"\n",
            escape(k),
            escape(v)
        ));
    }
    out.push_str(".end\n\n");

    // Fingerprint
    out.push_str("fingerprint\n");
    out.push_str(&format!("  value \"{}\"\n", escape(&cfg.fingerprint)));
    out.push_str(".end\n");

    out
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o
}

fn env_flag(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn host_triple_best_effort() -> String {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    let triple = match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => "unknown-unknown-unknown",
    };

    triple.to_string()
}

fn root_join_if_relative(root: &Path, p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

fn normalize_path(p: &Path) -> PathBuf {
    // Avoid canonicalize(): preserve best-effort behavior.
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

fn tool_version_line(tool: &str) -> Option<String> {
    // Best-effort: `tool --version`, capture first stdout line.
    let out = Command::new(tool).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn python_impl_and_version(python: &str) -> Option<(String, String)> {
    let script = r#"
import platform,sys
print(f"{platform.python_implementation()};{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}")
"#;
    let out = Command::new(python).arg("-c").arg(script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut parts = stdout.trim().splitn(2, ';');
    let impl_name = parts.next()?.trim();
    let ver = parts.next()?.trim();
    if impl_name.is_empty() || ver.is_empty() {
        return None;
    }
    Some((impl_name.to_string(), ver.to_string()))
}

fn ghc_numeric_version(ghc: &str) -> Option<String> {
    let out = Command::new(ghc).arg("--numeric-version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ver = stdout.trim();
    if ver.is_empty() {
        None
    } else {
        Some(ver.to_string())
    }
}

fn insert_toolchain_env_vars(vars: &mut BTreeMap<String, String>, toolchain: &ToolchainInfo) {
    if let Some(v) = &toolchain.python {
        vars.entry("PYTHON".to_string()).or_insert_with(|| v.clone());
    }
    if let Some(v) = &toolchain.ocaml {
        vars.entry("OCAMLPATH".to_string()).or_insert_with(|| v.clone());
    }
    if let Some(v) = &toolchain.ghc {
        vars.entry("GHC_PACKAGE_PATH".to_string()).or_insert_with(|| v.clone());
    }
}

fn compute_fingerprint(file_bytes: &[u8], profile: &str, target: &str, toolchain: &ToolchainInfo) -> String {
    // Deterministic non-cryptographic hash (FNV-1a 64-bit).
    let mut h = 0xcbf29ce484222325u64;

    fn mix(mut h: u64, data: &[u8]) -> u64 {
        for &b in data {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h
    }

    h = mix(h, file_bytes);
    h = mix(h, profile.as_bytes());
    h = mix(h, target.as_bytes());

    if let Some(v) = &toolchain.cc {
        h = mix(h, b"cc=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.cxx {
        h = mix(h, b"cxx=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.ar {
        h = mix(h, b"ar=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.ld {
        h = mix(h, b"ld=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.rustc {
        h = mix(h, b"rustc=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.python {
        h = mix(h, b"python=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.ocaml {
        h = mix(h, b"ocaml=");
        h = mix(h, v.as_bytes());
    }
    if let Some(v) = &toolchain.ghc {
        h = mix(h, b"ghc=");
        h = mix(h, v.as_bytes());
    }

    for (k, v) in &toolchain.versions {
        h = mix(h, k.as_bytes());
        h = mix(h, v.as_bytes());
    }

    // Optional time salt for debugging (OFF by default; breaks determinism).
    if env_flag("MUFFIN_FINGERPRINT_TIME") {
        if let Ok(d) = SystemTime::now().duration_since(UNIX_EPOCH) {
            let ns = d.as_nanos().to_le_bytes();
            h = mix(h, &ns);
        }
    }

    format!("fnv1a64:{:016x}", h)
}

/// Optional utility: generate a default config skeleton (no steelconf read).
pub fn generate_default_mcfg(root: impl AsRef<Path>) -> ResolvedConfig {
    let root = normalize_path(root.as_ref());
    let steelfile = root.join("steelconf");

    let profile = env::var("MUFFIN_PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target = env::var("MUFFIN_TARGET").unwrap_or_else(|_| host_triple_best_effort());

    let toolchain = ToolchainInfo::from_env(false);

    let mut vars = BTreeMap::new();
    vars.insert("steel.profile".to_string(), profile.clone());
    vars.insert("steel.target".to_string(), target.clone());
    insert_toolchain_env_vars(&mut vars, &toolchain);

    let fingerprint = compute_fingerprint(b"", &profile, &target, &toolchain);

    ResolvedConfig {
        schema_version: MFF_SCHEMA_VERSION,
        project_root: root.clone(),
        steelfile_path: steelfile,
        profile,
        target,
        paths: ResolvedPaths {
            build_dir: root.join("build"),
            dist_dir: root.join("dist"),
            cache_dir: root.join(".steel-cache"),
        },
        toolchain,
        vars,
        fingerprint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        base.join(format!("{}_{}_{}", prefix, pid, t.as_nanos()))
    }

    fn touch(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn parse_args_accepts_basic_flags() {
        let args = vec![
            "--root".into(),
            "./proj".into(),
            "--profile".into(),
            "release".into(),
            "--target".into(),
            "x86_64-unknown-linux-gnu".into(),
            "--emit".into(),
            "out/steelconfig.mff".into(),
            "--offline".into(),
            "--strict".into(),
            "--print".into(),
            "--max-depth".into(),
            "9".into(),
            "-v".into(),
        ];

        let o = parse_args(&args).unwrap();
        assert_eq!(o.root_dir, PathBuf::from("./proj"));
        assert_eq!(o.profile.as_deref(), Some("release"));
        assert_eq!(o.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(o.emit_path.as_deref(), Some(Path::new("out/steelconfig.mff")));
        assert!(o.offline);
        assert!(o.strict);
        assert!(o.print);
        assert_eq!(o.max_depth, 9);
        assert!(o.verbose);
    }

    #[test]
    fn format_is_deterministic() {
        let cfg = generate_default_mcfg("/tmp/project");
        let a = format_mcfg(&cfg);
        let b = format_mcfg(&cfg);
        assert_eq!(a, b);
        assert!(a.starts_with(&format!("mff {MFF_SCHEMA_VERSION}\n")));
        assert!(a.contains("fingerprint\n"));
    }

    #[test]
    fn choose_emit_path_is_optional() {
        let root = PathBuf::from("/workspace");

        let mut o = BuildMufOptions::default();
        o.root_dir = root.clone();
        assert_eq!(choose_emit_path(&root, &o), None);

        o.emit_path = Some(PathBuf::from("out/steelconfig.mff"));
        assert_eq!(
            choose_emit_path(&root, &o),
            Some(PathBuf::from("/workspace/out/steelconfig.mff"))
        );
    }

    #[test]
    fn run_skips_emit_by_default() {
        let dir = unique_temp_dir("steel_build_muf");
        fs::create_dir_all(&dir).unwrap();

        // create steelconf
        touch(&dir.join("steelconf"), "workspace ...\n");

        let opts = BuildMufOptions {
            root_dir: dir.clone(),
            print: false,
            verbose: false,
            ..BuildMufOptions::default()
        };

        let cfg = run(&opts).unwrap();
        assert_eq!(cfg.schema_version, 1);
        let emitted = dir.join(DEFAULT_EMIT_NAME);
        assert!(!emitted.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_emits_when_requested() {
        let dir = unique_temp_dir("steel_build_muf_emit");
        fs::create_dir_all(&dir).unwrap();

        // create steelconf
        touch(&dir.join("steelconf"), "workspace ...\n");

        let opts = BuildMufOptions {
            root_dir: dir.clone(),
            emit_path: Some(PathBuf::from("steelconfig.mff")),
            print: false,
            verbose: false,
            ..BuildMufOptions::default()
        };

        let cfg = run(&opts).unwrap();
        assert_eq!(cfg.schema_version, 1);

        let emitted = dir.join("steelconfig.mff");
        assert!(emitted.is_file());

        let text = fs::read_to_string(&emitted).unwrap();
        assert!(text.starts_with(&format!("mff {MFF_SCHEMA_VERSION}\n")));

        let _ = fs::remove_dir_all(&dir);
    }
}
