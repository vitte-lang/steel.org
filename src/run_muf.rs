// src/run_muf.rs
//! run_muf — minimal MUF runner for executing tool steps (gcc).
//!
//! This is intentionally narrow: it supports the MUF constructs used in the
//! examples (workspace/profile/tool/bake/run/make/output).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hash::Fingerprinter;
use crate::version::VersionInfo;
use vittelib::muf_parser::{Atom, Block, BlockItem, MufFile, Number};
use vittelib::path::{GlobSet, WalkOptions};

#[derive(Debug)]
pub enum RunError {
    Io {
        op: &'static str,
        path: PathBuf,
        err: String,
    },
    Parse {
        path: PathBuf,
        msg: String,
    },
    Config {
        msg: String,
        help: Option<String>,
    },
    Exec {
        cmd: String,
        status: Option<i32>,
        stderr: Option<String>,
    },
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Io { op, path, err } => write!(f, "{op} {}: {err}", path.display()),
            RunError::Parse { path, msg } => write!(f, "parse {}: {msg}", path.display()),
            RunError::Config { msg, help } => {
                if let Some(h) = help {
                    write!(f, "{msg}\nhelp: {h}")
                } else {
                    write!(f, "{msg}")
                }
            }
            RunError::Exec { cmd, status, stderr } => {
                if let Some(code) = status {
                    if let Some(err) = stderr {
                        write!(f, "command failed ({code}): {cmd}\n{err}")
                    } else {
                        write!(f, "command failed ({code}): {cmd}")
                    }
                } else if let Some(err) = stderr {
                    write!(f, "command failed: {cmd}\n{err}")
                } else {
                    write!(f, "command failed: {cmd}")
                }
            }
        }
    }
}

impl std::error::Error for RunError {}

#[derive(Debug, Default, Clone)]
pub struct RunOptions {
    pub root_dir: PathBuf,
    pub steel_file: Option<PathBuf>,
    pub profile: Option<String>,
    pub toolchain_dir: Option<PathBuf>,
    pub dry_run: bool,
    pub bakes: Vec<String>,
    pub run_all: bool,
    pub no_cache: bool,
    pub verbose: bool,
    pub log_path: Option<PathBuf>,
    pub log_mode: LogMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMode {
    Append,
    Truncate,
}

impl Default for LogMode {
    fn default() -> Self {
        LogMode::Append
    }
}

pub fn run(opts: &RunOptions) -> Result<(), RunError> {
    let cwd = std::env::current_dir().map_err(|e| RunError::Io {
        op: "cwd",
        path: PathBuf::from("."),
        err: e.to_string(),
    })?;
    let root = if opts.root_dir.as_os_str().is_empty() {
        cwd
    } else if opts.root_dir.is_absolute() {
        opts.root_dir.clone()
    } else {
        cwd.join(&opts.root_dir)
    };

    let steelfile = resolve_steelfile(&root, opts.steel_file.as_deref())?;
    let src = fs::read_to_string(&steelfile).map_err(|e| RunError::Io {
        op: "read",
        path: steelfile.clone(),
        err: e.to_string(),
    })?;

    let muf = vittelib::muf_parser::parse_muf(&src).map_err(|e| RunError::Parse {
        path: steelfile.clone(),
        msg: e.to_string(),
    })?;
    let cfg = interpret_muf(&muf)?;
    let tool_paths = resolve_tool_paths(&cfg.tools, opts.toolchain_dir.as_deref())?;

    let profile_name = opts
        .profile
        .clone()
        .or_else(|| cfg.workspace.get("profile").cloned())
        .unwrap_or_else(|| "debug".to_string());

    let profile = cfg
        .profiles
        .get(&profile_name)
        .ok_or_else(|| RunError::Config {
            msg: format!("unknown profile `{profile_name}`"),
            help: Some("use --profile <name> or set workspace.profile".into()),
        })?;

    let selected = select_bakes(&cfg, opts)?;
    let order = topo_order(&cfg, &selected)?;

    let mut vars = cfg.workspace.clone();
    for (k, v) in &profile.settings {
        vars.insert(k.clone(), v.clone());
    }

    let log_path = run_log_path(&root, &cfg.workspace, opts.log_path.as_deref());
    if opts.log_mode == LogMode::Truncate {
        prepare_log(&log_path)?;
    }
    ensure_log_header(&log_path)?;

    if opts.verbose {
        println!("run log: {}", log_path.display());
    }

    for name in order {
        let bake = cfg.bakes.get(&name).expect("bake exists");
        let sources = expand_sources(&root, &bake.makes)?;
        if sources.is_empty() {
            return Err(RunError::Config {
                msg: format!("bake `{}`: no sources matched", name),
                help: Some("check your .make patterns or file list and --root".into()),
            });
        }

        let output_rel = expand_vars(&bake.output_path, &vars);
        let output_abs = resolve_path_under_root(&root, &output_rel);

        let cache_fp = if !opts.dry_run && !opts.no_cache {
            let policy_fp = policy_fingerprint(&profile_name, &vars);
            let toolchain_fp = toolchain_fingerprint(&tool_paths);
            bake_fingerprint(
                &root,
                &name,
                bake,
                &sources,
                &output_rel,
                &vars,
                &tool_paths,
                policy_fp,
                toolchain_fp,
            )
        } else {
            None
        };
        let cache_path = cache_fp.as_ref().map(|_| bake_cache_path(&root, &name));

        if !opts.dry_run {
            if let (Some(fp), Some(path)) = (cache_fp, cache_path.as_deref()) {
                if should_skip_with_fingerprint(&output_abs, path, fp) {
                    if opts.verbose {
                        println!("skip {name} (up to date)");
                    }
                    continue;
                }
            } else if should_skip_mtime(&root, &output_abs, &sources, opts.no_cache) {
                if opts.verbose {
                    println!("skip {name} (up to date)");
                }
                continue;
            }
        }

        if let Some(parent) = output_abs.parent() {
            fs::create_dir_all(parent).map_err(|e| RunError::Io {
                op: "mkdir",
                path: parent.to_path_buf(),
                err: e.to_string(),
            })?;
        }

        let mut bake_run_count = 0u32;
        let mut bake_duration_ms = 0u64;
        if !opts.dry_run {
            append_bake_log_start(&log_path, &name)?;
            append_bake_log_sources(&log_path, &output_rel, &sources)?;
        }

        if opts.verbose {
            println!(
                "bake {name}: {} sources -> {}",
                sources.len(),
                output_rel
            );
        }

        for run in &bake.runs {
            let tool = cfg.tools.get(&run.tool).ok_or_else(|| RunError::Config {
                msg: format!("missing tool `{}`", run.tool),
                help: Some("add [tool <name>] with .exec".into()),
            })?;
            let tool_exec = tool_paths
                .get(&run.tool)
                .map(String::as_str)
                .unwrap_or(&tool.exec);

            let args = build_args(run, &sources, &bake.output_port, &output_rel, &vars)?;
            if opts.dry_run {
                println!("{} {}", tool_exec, args.join(" "));
                continue;
            }

            let mut cmd = Command::new(tool_exec);
            cmd.args(&args).current_dir(&root);
            let cmd_str = format!("{} {}", tool_exec, args.join(" "));
            if opts.verbose {
                println!("run {name}: {cmd_str}");
            }
            let start = std::time::Instant::now();
            let output = cmd.output().map_err(|e| RunError::Exec {
                cmd: cmd_str.clone(),
                status: None,
                stderr: Some(e.to_string()),
            })?;
            let duration_ms = start.elapsed().as_millis() as u64;

            bake_run_count = bake_run_count.saturating_add(1);
            bake_duration_ms = bake_duration_ms.saturating_add(duration_ms);
            append_run_log(&log_path, &cmd_str, &root, &output, duration_ms)?;
            if env_flag("MUFFIN_RUN_STDOUT") {
                if !output.stdout.is_empty() {
                    print!("{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    eprint!("{}", String::from_utf8_lossy(&output.stderr));
                }
            }

            if !output.status.success() {
                if !opts.dry_run {
                    append_bake_log_end(&log_path)?;
                }
                return Err(RunError::Exec {
                    cmd: cmd_str,
                    status: output.status.code(),
                    stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
                });
            }
        }

        if !opts.dry_run {
            append_bake_log_summary(&log_path, bake_run_count, bake_duration_ms)?;
            append_bake_log_end(&log_path)?;
        }

        if let (Some(fp), Some(path)) = (cache_fp, cache_path.as_deref()) {
            if let Err(e) = write_cache_fingerprint(path, fp) {
                if opts.verbose {
                    println!("cache write failed for {name}: {e}");
                }
            }
        }
    }

    if !opts.dry_run {
        append_global_log_summary(&log_path)?;
    }

    Ok(())
}

fn run_log_path(root: &Path, workspace: &BTreeMap<String, String>, override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return resolve_path_under_root(root, p.to_string_lossy().as_ref());
    }
    let target_dir = workspace
        .get("target_dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    let ts = run_log_stamp();
    let name = format!("steel_run_{ts}.mff");
    resolve_path_under_root(root, &target_dir.join(name).to_string_lossy())
}

fn append_run_log(
    path: &Path,
    cmd: &str,
    cwd: &Path,
    output: &std::process::Output,
    duration_ms: u64,
) -> Result<(), RunError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RunError::Io {
            op: "mkdir",
            path: parent.to_path_buf(),
            err: e.to_string(),
        })?;
    }

    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    let status = output.status.code().unwrap_or(-1);
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout_bytes = output.stdout.len();
    let stderr_bytes = output.stderr.len();

    use std::io::Write;
    let ts = run_log_stamp();
    let ts_iso = run_log_stamp_iso();
    writeln!(f, "[run log]") .map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "ts {ts}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "ts_iso \"{ts_iso}\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "duration_ms {duration_ms}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "cmd \"{}\"", cmd.replace('"', "\\\"")).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "cwd \"{}\"", cwd.to_string_lossy().replace('"', "\\\"")).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "status {status}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "ok {}", if ok { "true" } else { "false" }).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "stdout_bytes {stdout_bytes}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "stderr_bytes {stderr_bytes}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    if !stdout.is_empty() {
        writeln!(f, "stdout \"{}\"", stdout.replace('"', "\\\"")).map_err(|e| RunError::Io {
            op: "write",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;
    }
    if !stderr.is_empty() {
        writeln!(f, "stderr \"{}\"", stderr.replace('"', "\\\"")).map_err(|e| RunError::Io {
            op: "write",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;
    }
    writeln!(f, "..").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;

    Ok(())
}

fn append_bake_log_start(path: &Path, bake: &str) -> Result<(), RunError> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    use std::io::Write;
    writeln!(f, "[bake log \"{}\"]", bake.replace('"', "\\\"")).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    Ok(())
}

fn append_bake_log_sources(
    path: &Path,
    output_rel: &str,
    sources: &[String],
) -> Result<(), RunError> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    use std::io::Write;
    writeln!(f, "output \"{}\"", output_rel.replace('"', "\\\"")).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "sources_count {}", sources.len()).map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    for src in sources {
        writeln!(f, "source \"{}\"", src.replace('"', "\\\"")).map_err(|e| RunError::Io {
            op: "write",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;
    }
    Ok(())
}

fn append_bake_log_end(path: &Path) -> Result<(), RunError> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    use std::io::Write;
    writeln!(f, "..").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    Ok(())
}

fn append_bake_log_summary(path: &Path, runs: u32, duration_ms: u64) -> Result<(), RunError> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    use std::io::Write;
    writeln!(f, "runs {runs}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "duration_ms {duration_ms}").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    Ok(())
}

fn append_global_log_summary(path: &Path) -> Result<(), RunError> {
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    let ts_iso = run_log_stamp_iso();
    use std::io::Write;
    writeln!(f, "[run summary]").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "ts_iso \"{ts_iso}\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "..").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    Ok(())
}

fn prepare_log(path: &Path) -> Result<(), RunError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RunError::Io {
            op: "mkdir",
            path: parent.to_path_buf(),
            err: e.to_string(),
        })?;
    }
    fs::File::create(path).map_err(|e| RunError::Io {
        op: "truncate",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    write_log_header(path)?;
    Ok(())
}

fn run_log_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

fn run_log_stamp_iso() -> String {
    use chrono::{SecondsFormat, Utc};
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn ensure_log_header(path: &Path) -> Result<(), RunError> {
    let need = match fs::metadata(path) {
        Ok(m) => m.len() == 0,
        Err(_) => true,
    };
    if need {
        write_log_header(path)?;
    }
    Ok(())
}

fn write_log_header(path: &Path) -> Result<(), RunError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RunError::Io {
            op: "mkdir",
            path: parent.to_path_buf(),
            err: e.to_string(),
        })?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| RunError::Io {
            op: "open",
            path: path.to_path_buf(),
            err: e.to_string(),
        })?;

    use std::io::Write;
    let ts_iso = run_log_stamp_iso();
    let ver = VersionInfo::current().format_short();
    writeln!(f, "[log meta]").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "format \"steel-runlog-1\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "tool \"steel\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "version \"{ver}\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "ts_iso \"{ts_iso}\"").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    writeln!(f, "..").map_err(|e| RunError::Io {
        op: "write",
        path: path.to_path_buf(),
        err: e.to_string(),
    })?;
    Ok(())
}

fn build_args(
    run: &RunBlock,
    sources: &[String],
    output_port: &str,
    output_rel: &str,
    vars: &BTreeMap<String, String>,
) -> Result<Vec<String>, RunError> {
    let mut args = Vec::new();

    for set in &run.sets {
        let flag = expand_vars(&set.flag, vars);
        let value = expand_vars(&set.value, vars);
        match value.as_str() {
            "1" | "true" | "yes" => args.push(flag),
            "0" | "false" | "no" => {}
            _ => {
                args.push(flag);
                args.push(value);
            }
        }
    }

    for inc in &run.includes {
        let val = expand_vars(inc, vars);
        args.push("-I".to_string());
        args.push(val);
    }

    for (k, v) in &run.defines {
        let key = expand_vars(k, vars);
        let arg = if let Some(v) = v {
            format!("-D{}={}", key, expand_vars(v, vars))
        } else {
            format!("-D{}", key)
        };
        args.push(arg);
    }

    for d in &run.lib_dirs {
        let val = expand_vars(d, vars);
        args.push("-L".to_string());
        args.push(val);
    }

    for take in &run.takes {
        if take.flag == "@args" {
            for src in sources {
                args.push(src.clone());
            }
        } else {
            return Err(RunError::Config {
                msg: format!("unsupported takes flag `{}` (expected @args)", take.flag),
                help: Some("use: .takes c_src as \"@args\"".into()),
            });
        }
    }

    for lib in &run.libs {
        let val = expand_vars(lib, vars);
        args.push("-l".to_string());
        args.push(val);
    }

    for emit in &run.emits {
        if emit.port == output_port {
            args.push(emit.flag.clone());
            args.push(output_rel.to_string());
        }
    }

    Ok(args)
}

fn resolve_steelfile(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, RunError> {
    if let Some(p) = explicit {
        let out = if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        };
        return Ok(out);
    }

    let candidate = root.join("steelconf");
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(RunError::Config {
            msg: format!("missing steelconf in {}", root.display()),
            help: Some("pass --file <path> or create steelconf".into()),
        })
    }
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

fn select_bakes(cfg: &Config, opts: &RunOptions) -> Result<Vec<String>, RunError> {
    if opts.run_all {
        return Ok(cfg.bake_order.clone());
    }
    if !opts.bakes.is_empty() {
        return Ok(opts.bakes.clone());
    }
    if cfg.bakes.contains_key("app") {
        return Ok(vec!["app".to_string()]);
    }
    if let Some(first) = cfg.bake_order.first() {
        return Ok(vec![first.clone()]);
    }
    Err(RunError::Config {
        msg: "no bakes defined".into(),
        help: Some("add [bake <name>] blocks".into()),
    })
}

fn topo_order(cfg: &Config, targets: &[String]) -> Result<Vec<String>, RunError> {
    let mut out = Vec::new();
    let mut visiting = BTreeMap::new();
    let mut visited = BTreeMap::new();

    for t in targets {
        visit_bake(cfg, t, &mut visiting, &mut visited, &mut out)?;
    }

    Ok(out)
}

fn visit_bake(
    cfg: &Config,
    name: &str,
    visiting: &mut BTreeMap<String, bool>,
    visited: &mut BTreeMap<String, bool>,
    out: &mut Vec<String>,
) -> Result<(), RunError> {
    if visited.get(name).copied().unwrap_or(false) {
        return Ok(());
    }
    if visiting.get(name).copied().unwrap_or(false) {
        return Err(RunError::Config {
            msg: format!("dependency cycle detected at bake `{name}`"),
            help: Some("check your .needs declarations".into()),
        });
    }
    let bake = cfg.bakes.get(name).ok_or_else(|| RunError::Config {
        msg: format!("unknown bake `{name}`"),
        help: Some("use --bake <name> from the file".into()),
    })?;
    visiting.insert(name.to_string(), true);
    for dep in &bake.deps {
        visit_bake(cfg, dep, visiting, visited, out)?;
    }
    visiting.insert(name.to_string(), false);
    visited.insert(name.to_string(), true);
    out.push(name.to_string());
    Ok(())
}

fn should_skip_mtime(root: &Path, output: &Path, inputs: &[String], no_cache: bool) -> bool {
    if no_cache {
        return false;
    }
    let out_md = match fs::metadata(output) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let out_time = match out_md.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };
    for src in inputs {
        let p = root.join(src);
        let md = match fs::metadata(&p) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let t = match md.modified() {
            Ok(t) => t,
            Err(_) => return false,
        };
        if t > out_time {
            return false;
        }
    }
    true
}

fn should_skip_with_fingerprint(output: &Path, cache_path: &Path, expected: u64) -> bool {
    if !output.is_file() {
        return false;
    }
    match read_cache_fingerprint(cache_path) {
        Some(found) => found == expected,
        None => false,
    }
}

fn bake_cache_path(root: &Path, bake: &str) -> PathBuf {
    let mut name = String::with_capacity(bake.len());
    for ch in bake.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            name.push(ch);
        } else {
            name.push('_');
        }
    }
    if name.is_empty() {
        name.push_str("bake");
    }
    root.join(".steel-cache").join("run").join(format!("{name}.fp"))
}

fn read_cache_fingerprint(path: &Path) -> Option<u64> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("fingerprint ") {
            let hex = rest.trim();
            if let Ok(v) = u64::from_str_radix(hex, 16) {
                return Some(v);
            }
        }
    }
    None
}

fn write_cache_fingerprint(path: &Path, fp: u64) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = format!("fingerprint {:016x}\n", fp);
    fs::write(path, text).map_err(|e| e.to_string())
}

fn policy_fingerprint(profile: &str, vars: &BTreeMap<String, String>) -> u64 {
    let mut fp = Fingerprinter::new();
    fp.put_str(profile);
    fp.put_kv_sorted(vars);
    fp.finish().raw
}

fn toolchain_fingerprint(tool_paths: &BTreeMap<String, String>) -> u64 {
    let mut fp = Fingerprinter::new();
    for (name, path) in tool_paths {
        fp.put_str(name);
        fp.put_str(path);
        put_file_stamp(&mut fp, Path::new(path));
    }
    fp.finish().raw
}

fn bake_fingerprint(
    root: &Path,
    bake_name: &str,
    bake: &Bake,
    sources: &[String],
    output_rel: &str,
    vars: &BTreeMap<String, String>,
    tool_paths: &BTreeMap<String, String>,
    policy_fp: u64,
    toolchain_fp: u64,
) -> Option<u64> {
    let mut fp = Fingerprinter::new();
    fp.put_str("bake");
    fp.put_str(bake_name);
    fp.put_str(output_rel);
    fp.put_str(&bake.output_port);
    fp.put_u64(policy_fp);
    fp.put_u64(toolchain_fp);
    fp.put_kv_sorted(vars);

    for make in &bake.makes {
        fp.put_str(&make.kind);
        fp.put_str(&make.pattern);
    }

    for run in &bake.runs {
        fp.put_str(&run.tool);
        if let Some(path) = tool_paths.get(&run.tool) {
            fp.put_str(path);
        }
        for take in &run.takes {
            fp.put_str(&take.flag);
        }
        for emit in &run.emits {
            fp.put_str(&emit.port);
            fp.put_str(&emit.flag);
        }
        for set in &run.sets {
            fp.put_str(&set.flag);
            fp.put_str(&set.value);
        }
        for inc in &run.includes {
            fp.put_str(inc);
        }
        for (k, v) in &run.defines {
            fp.put_str(k);
            if let Some(v) = v {
                fp.put_str(v);
            }
        }
        for dir in &run.lib_dirs {
            fp.put_str(dir);
        }
        for lib in &run.libs {
            fp.put_str(lib);
        }
    }

    let mut sorted_sources = sources.to_vec();
    sorted_sources.sort();
    for src in &sorted_sources {
        let path = root.join(src);
        if !path.exists() {
            return None;
        }
        fp.put_str(src);
        put_file_stamp(&mut fp, &path);
    }

    Some(fp.finish().raw)
}

fn put_file_stamp(fp: &mut Fingerprinter, path: &Path) {
    if let Ok(md) = fs::metadata(path) {
        fp.put_u64(md.len());
        if let Ok(mt) = md.modified() {
            let (secs, nanos) = system_time_parts(mt);
            fp.put_u64(secs);
            fp.put_u64(nanos as u64);
        } else {
            fp.put_u64(0);
            fp.put_u64(0);
        }
    } else {
        fp.put_u64(0);
        fp.put_u64(0);
        fp.put_u64(0);
    }
}

fn system_time_parts(t: SystemTime) -> (u64, u32) {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs(), d.subsec_nanos())
}

#[derive(Debug)]
struct Config {
    workspace: BTreeMap<String, String>,
    profiles: BTreeMap<String, Profile>,
    tools: BTreeMap<String, Tool>,
    bakes: BTreeMap<String, Bake>,
    bake_order: Vec<String>,
}

#[derive(Debug)]
struct Profile {
    settings: BTreeMap<String, String>,
}

#[derive(Debug)]
struct Tool {
    exec: String,
}

#[derive(Debug)]
struct Bake {
    makes: Vec<Make>,
    runs: Vec<RunBlock>,
    deps: Vec<String>,
    output_port: String,
    output_path: String,
}

#[derive(Debug)]
struct Make {
    kind: String,
    pattern: String,
}

#[derive(Debug)]
struct RunBlock {
    tool: String,
    takes: Vec<TakeBinding>,
    emits: Vec<EmitBinding>,
    sets: Vec<RunSet>,
    includes: Vec<String>,
    defines: Vec<(String, Option<String>)>,
    lib_dirs: Vec<String>,
    libs: Vec<String>,
}

#[derive(Debug)]
struct TakeBinding {
    flag: String,
}

#[derive(Debug)]
struct EmitBinding {
    port: String,
    flag: String,
}

#[derive(Debug)]
struct RunSet {
    flag: String,
    value: String,
}

fn interpret_muf(muf: &MufFile) -> Result<Config, RunError> {
    let mut cfg = Config {
        workspace: BTreeMap::new(),
        profiles: BTreeMap::new(),
        tools: BTreeMap::new(),
        bakes: BTreeMap::new(),
        bake_order: Vec::new(),
    };

    for blk in &muf.blocks {
        match blk.tag.as_str() {
            "workspace" => parse_workspace(blk, &mut cfg)?,
            "profile" => parse_profile(blk, &mut cfg)?,
            "tool" => parse_tool(blk, &mut cfg)?,
            "bake" => parse_bake(blk, &mut cfg)?,
            _ => {}
        }
    }

    Ok(cfg)
}

fn parse_workspace(blk: &Block, cfg: &mut Config) -> Result<(), RunError> {
    for item in &blk.items {
        if let BlockItem::Directive(d) = item {
            if d.op == "set" && d.args.len() >= 2 {
                let key = atom_to_string(&d.args[0])?;
                let val = atom_to_string(&d.args[1])?;
                cfg.workspace.insert(key, val);
            }
        }
    }
    Ok(())
}

fn parse_profile(blk: &Block, cfg: &mut Config) -> Result<(), RunError> {
    let name = blk
        .name
        .clone()
        .ok_or_else(|| RunError::Config {
            msg: "profile name missing".into(),
            help: Some("use: [profile <name>]".into()),
        })?;
    let mut settings = BTreeMap::new();
    for item in &blk.items {
        if let BlockItem::Directive(d) = item {
            if d.op == "set" && d.args.len() >= 2 {
                let key = atom_to_string(&d.args[0])?;
                let val = atom_to_string(&d.args[1])?;
                settings.insert(key, val);
            }
        }
    }
    cfg.profiles.insert(name, Profile { settings });
    Ok(())
}

fn parse_tool(blk: &Block, cfg: &mut Config) -> Result<(), RunError> {
    let name = blk
        .name
        .clone()
        .ok_or_else(|| RunError::Config {
            msg: "tool name missing".into(),
            help: Some("use: [tool <name>]".into()),
        })?;
    let mut exec = None;
    for item in &blk.items {
        if let BlockItem::Directive(d) = item {
            if d.op == "exec" && !d.args.is_empty() {
                exec = Some(atom_to_string(&d.args[0])?);
            }
        }
    }
    let exec = exec.ok_or_else(|| RunError::Config {
        msg: format!("tool `{name}` missing exec"),
        help: Some("use: .exec \"gcc\"".into()),
    })?;
    cfg.tools.insert(name, Tool { exec });
    Ok(())
}

fn parse_bake(blk: &Block, cfg: &mut Config) -> Result<(), RunError> {
    let name = blk
        .name
        .clone()
        .ok_or_else(|| RunError::Config {
            msg: "bake name missing".into(),
            help: Some("use: [bake <name>]".into()),
        })?;

    let mut makes = Vec::new();
    let mut runs = Vec::new();
    let mut deps = Vec::new();
    let mut output_port = None;
    let mut output_path = None;

    for item in &blk.items {
        match item {
            BlockItem::Directive(d) => {
                match d.op.as_str() {
                    "make" if d.args.len() >= 2 => {
                        let _id = atom_to_string(&d.args[0])?;
                        let (kind, pattern) = if d.args.len() >= 3 {
                            let kind = atom_to_string(&d.args[1])?;
                            let pattern = atom_to_string(&d.args[2])?;
                            (kind, pattern)
                        } else {
                            let pattern = atom_to_string(&d.args[1])?;
                            ("file".to_string(), pattern)
                        };
                        makes.push(Make { kind, pattern });
                    }
                    "needs" if d.args.len() >= 1 => {
                        let dep = atom_to_string(&d.args[0])?;
                        deps.push(dep);
                    }
                    "output" if d.args.len() >= 2 => {
                        output_port = Some(atom_to_string(&d.args[0])?);
                        output_path = Some(atom_to_string(&d.args[1])?);
                    }
                    _ => {}
                }
            }
            BlockItem::Block(b) => {
                if b.tag == "run" {
                    runs.push(parse_run(b)?);
                }
            }
        }
    }

    if runs.is_empty() {
        return Err(RunError::Config {
            msg: format!("bake `{name}` missing run block"),
            help: Some("add: [run <tool>] ...".into()),
        });
    }
    let output_port = output_port.ok_or_else(|| RunError::Config {
        msg: format!("bake `{name}` missing output"),
        help: Some("use: .output exe \"target/out/app\"".into()),
    })?;
    let output_path = output_path.ok_or_else(|| RunError::Config {
        msg: format!("bake `{name}` missing output path"),
        help: Some("use: .output exe \"target/out/app\"".into()),
    })?;

    cfg.bake_order.push(name.clone());
    cfg.bakes.insert(
        name,
        Bake {
            makes,
            runs,
            deps,
            output_port,
            output_path,
        },
    );
    Ok(())
}

fn parse_run(blk: &Block) -> Result<RunBlock, RunError> {
    let tool = blk
        .name
        .clone()
        .ok_or_else(|| RunError::Config {
            msg: "run block missing tool name".into(),
            help: Some("use: [run gcc]".into()),
        })?;

    let mut takes = Vec::new();
    let mut emits = Vec::new();
    let mut sets = Vec::new();
    let mut includes = Vec::new();
    let mut defines = Vec::new();
    let mut lib_dirs = Vec::new();
    let mut libs = Vec::new();

    for item in &blk.items {
        if let BlockItem::Directive(d) = item {
            match d.op.as_str() {
                "takes" if d.args.len() >= 3 => {
                    let flag = atom_to_string(&d.args[2])?;
                    takes.push(TakeBinding { flag });
                }
                "emits" if d.args.len() >= 3 => {
                    let port = atom_to_string(&d.args[0])?;
                    let flag = atom_to_string(&d.args[2])?;
                    emits.push(EmitBinding { port, flag });
                }
                "set" if d.args.len() >= 2 => {
                    let flag = atom_to_string(&d.args[0])?;
                    let value = atom_to_string(&d.args[1])?;
                    sets.push(RunSet { flag, value });
                }
                "include" if d.args.len() >= 1 => {
                    includes.push(atom_to_string(&d.args[0])?);
                }
                "define" if d.args.len() >= 1 => {
                    let key = atom_to_string(&d.args[0])?;
                    let val = if d.args.len() >= 2 {
                        Some(atom_to_string(&d.args[1])?)
                    } else {
                        None
                    };
                    defines.push((key, val));
                }
                "libdir" if d.args.len() >= 1 => {
                    lib_dirs.push(atom_to_string(&d.args[0])?);
                }
                "lib" if d.args.len() >= 1 => {
                    libs.push(atom_to_string(&d.args[0])?);
                }
                _ => {}
            }
        }
    }

    Ok(RunBlock {
        tool,
        takes,
        emits,
        sets,
        includes,
        defines,
        lib_dirs,
        libs,
    })
}

fn atom_to_string(a: &Atom) -> Result<String, RunError> {
    match a {
        Atom::Str(s) => Ok(s.clone()),
        Atom::Name(s) => Ok(s.clone()),
        Atom::Number(n) => Ok(match n {
            Number::Int { raw, .. } => raw.clone(),
            Number::Float { raw, .. } => raw.clone(),
        }),
        Atom::Ref(r) => Ok(r.segments.join("/")),
    }
}

fn expand_sources(root: &Path, makes: &[Make]) -> Result<Vec<String>, RunError> {
    let mut out = Vec::new();
    for m in makes {
        match m.kind.as_str() {
            "cglob" | "glob" => {
                let mut set = GlobSet::new();
                set.add(m.pattern.clone()).map_err(|e| RunError::Config {
                    msg: format!("invalid glob `{}`: {e}", m.pattern),
                    help: Some("example: **/*.c".into()),
                })?;
                let files = set
                    .walk(root, WalkOptions::default())
                    .map_err(|e| RunError::Config {
                        msg: format!("glob scan failed: {e}"),
                        help: Some("check --root and filesystem permissions".into()),
                    })?;
                for p in files {
                    let rel = p.strip_prefix(root).unwrap_or(&p);
                    out.push(rel.to_string_lossy().to_string());
                }
            }
            "file" | "list" => {
                out.push(m.pattern.clone());
            }
            _ => {}
        }
    }
    Ok(out)
}

fn resolve_path_under_root(root: &Path, p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn expand_vars(input: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && matches!(chars.peek(), Some('{')) {
            let _ = chars.next();
            let mut key = String::new();
            while let Some(ch) = chars.next() {
                if ch == '}' {
                    break;
                }
                key.push(ch);
            }
            if let Some(v) = vars.get(&key) {
                out.push_str(v);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn resolve_tool_paths(
    tools: &BTreeMap<String, Tool>,
    toolchain_dir: Option<&Path>,
) -> Result<BTreeMap<String, String>, RunError> {
    let mut out = BTreeMap::new();
    for (name, tool) in tools {
        let resolved = find_executable(&tool.exec, toolchain_dir).ok_or_else(|| RunError::Config {
            msg: format!("tool `{name}` not found: {}", tool.exec),
            help: Some("install the toolchain, set PATH, or pass --toolchain <dir>".into()),
        })?;
        out.insert(name.clone(), resolved.to_string_lossy().to_string());
    }
    Ok(out)
}

fn find_executable(exec: &str, toolchain_dir: Option<&Path>) -> Option<PathBuf> {
    let exec_path = Path::new(exec);
    if exec_path.is_absolute() || exec_path.components().count() > 1 {
        if exec_path.exists() {
            return Some(exec_path.to_path_buf());
        }
        return None;
    }

    if let Some(dir) = toolchain_dir {
        if let Some(found) = find_in_dir(exec, dir) {
            return Some(found);
        }
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if let Some(found) = find_in_dir(exec, &dir) {
            return Some(found);
        }
    }
    None
}

fn find_in_dir(exec: &str, dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join(exec);
    if candidate.exists() {
        return Some(candidate);
    }
    if cfg!(windows) && !exec.to_ascii_lowercase().ends_with(".exe") {
        let candidate = dir.join(format!("{exec}.exe"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("steel_test_{prefix}_{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn interpret_muf_basic() {
        let src = r#"
!muf 4

[workspace]
  .set name "app"
  .set profile "debug"
..

[profile debug]
  .set opt 0
  .set debug 1
  .set ndebug 0
..

[tool gcc]
  .exec "gcc"
..

[bake app]
  .make c_src cglob "src/**/*.c"
  [run gcc]
    .takes c_src as "@args"
    .set "-O${opt}" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app"
..
"#;
        let muf = vittelib::muf_parser::parse_muf(src).unwrap();
        let cfg = interpret_muf(&muf).unwrap();

        assert_eq!(cfg.workspace.get("name").unwrap(), "app");
        assert_eq!(cfg.workspace.get("profile").unwrap(), "debug");

        let profile = cfg.profiles.get("debug").unwrap();
        assert_eq!(profile.settings.get("opt").unwrap(), "0");
        assert_eq!(profile.settings.get("debug").unwrap(), "1");

        let tool = cfg.tools.get("gcc").unwrap();
        assert_eq!(tool.exec, "gcc");

        let bake = cfg.bakes.get("app").unwrap();
        assert_eq!(bake.makes.len(), 1);
        assert_eq!(bake.output_port, "exe");
        assert_eq!(bake.output_path, "target/out/app");
        assert_eq!(bake.runs.len(), 1);
    }

    #[test]
    fn log_header_written_once() {
        let dir = temp_dir("log");
        let log = dir.join("run.mff");
        ensure_log_header(&log).unwrap();
        ensure_log_header(&log).unwrap();
        let s = fs::read_to_string(&log).unwrap();
        assert_eq!(s.matches("[log meta]").count(), 1);
        assert!(s.contains("format \"steel-runlog-1\""));
        assert!(s.contains("tool \"steel\""));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_tool_paths_prefers_toolchain_dir() {
        let dir = temp_dir("toolchain");
        let tool_path = dir.join("gcc");
        File::create(&tool_path).unwrap();

        let mut tools = BTreeMap::new();
        tools.insert(
            "gcc".to_string(),
            Tool {
                exec: "gcc".to_string(),
            },
        );
        let resolved = resolve_tool_paths(&tools, Some(&dir)).unwrap();
        assert_eq!(resolved.get("gcc").unwrap(), tool_path.to_string_lossy().as_ref());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bake_fingerprint_changes_with_policy_and_toolchain() {
        let root = temp_dir("cachefp");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("main.c"), "int main(){return 0;}").unwrap();

        let tool_dir = temp_dir("toolchain_a");
        let tool_path = tool_dir.join("gcc");
        File::create(&tool_path).unwrap();

        let mut tool_paths = BTreeMap::new();
        tool_paths.insert("gcc".to_string(), tool_path.to_string_lossy().to_string());

        let bake = Bake {
            makes: vec![Make {
                kind: "cglob".to_string(),
                pattern: "src/*.c".to_string(),
            }],
            runs: vec![RunBlock {
                tool: "gcc".to_string(),
                takes: vec![TakeBinding {
                    flag: "@args".to_string(),
                }],
                emits: vec![EmitBinding {
                    port: "exe".to_string(),
                    flag: "-o".to_string(),
                }],
                sets: Vec::new(),
                includes: Vec::new(),
                defines: Vec::new(),
                lib_dirs: Vec::new(),
                libs: Vec::new(),
            }],
            deps: Vec::new(),
            output_port: "exe".to_string(),
            output_path: "target/out/app".to_string(),
        };

        let sources = vec!["src/main.c".to_string()];

        let mut vars = BTreeMap::new();
        vars.insert("opt".to_string(), "0".to_string());
        let policy_fp = policy_fingerprint("debug", &vars);
        let toolchain_fp = toolchain_fingerprint(&tool_paths);
        let fp_a = bake_fingerprint(
            &root,
            "app",
            &bake,
            &sources,
            "target/out/app",
            &vars,
            &tool_paths,
            policy_fp,
            toolchain_fp,
        )
        .unwrap();

        vars.insert("opt".to_string(), "2".to_string());
        let policy_fp2 = policy_fingerprint("debug", &vars);
        let fp_b = bake_fingerprint(
            &root,
            "app",
            &bake,
            &sources,
            "target/out/app",
            &vars,
            &tool_paths,
            policy_fp2,
            toolchain_fp,
        )
        .unwrap();
        assert_ne!(fp_a, fp_b);

        let tool_dir_b = temp_dir("toolchain_b");
        let tool_path_b = tool_dir_b.join("gcc");
        File::create(&tool_path_b).unwrap();
        let mut tool_paths_b = BTreeMap::new();
        tool_paths_b.insert("gcc".to_string(), tool_path_b.to_string_lossy().to_string());
        let toolchain_fp_b = toolchain_fingerprint(&tool_paths_b);
        let fp_c = bake_fingerprint(
            &root,
            "app",
            &bake,
            &sources,
            "target/out/app",
            &vars,
            &tool_paths_b,
            policy_fp2,
            toolchain_fp_b,
        )
        .unwrap();
        assert_ne!(fp_b, fp_c);

        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&tool_dir).ok();
        fs::remove_dir_all(&tool_dir_b).ok();
    }

    #[cfg(windows)]
    #[test]
    fn find_executable_adds_exe_on_windows() {
        let dir = temp_dir("exe");
        let exe = dir.join("tool.exe");
        File::create(&exe).unwrap();
        let found = find_executable("tool", Some(&dir)).unwrap();
        assert_eq!(found, exe);
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn find_executable_unix_plain_name() {
        let dir = temp_dir("exe");
        let exe = dir.join("tool");
        File::create(&exe).unwrap();
        let found = find_executable("tool", Some(&dir)).unwrap();
        assert_eq!(found, exe);
        fs::remove_dir_all(&dir).ok();
    }
}
