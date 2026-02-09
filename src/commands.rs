// /Users/vincent/Documents/Github/steel/src/commands.rs
//! commands — CLI command implementations (std-only)
//!
//! This module provides a thin, deterministic CLI dispatcher for Steel.
//! It is designed to work without external crates and to keep the CLI contract
//! stable while internal modules evolve.
//!
//! Supported commands (current contract):
//! - `help` / `--help`
//! - `version`
//! - `build [flags...]`         (Configuration phase; emits steel.log)
//! - `build steel [flags...]`   (legacy alias)
//! - `resolve [flags...]`       (alias of `build`)
//! - `check [flags...]`         (best-effort validate; emits then deletes unless strict)
//! - `print [flags...]`         (emits + prints the resolved config)
//! - `graph`                    (stub; reserved for DOT/text graph export)
//! - `fmt`                      (stub; reserved for formatting steelconfs)

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;

use crate::build_muf;
use crate::build_muf::BuildMufError;
use crate::editor_setup;
use crate::ninja;
use crate::run_muf;
use crate::target_file;

pub const CLI_NAME: &str = "steel";

#[derive(Debug)]
pub enum CommandError {
    Usage(String),
    Failure { code: i32, msg: String },
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Usage(s) => write!(f, "{s}"),
            CommandError::Failure { msg, .. } => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CommandError {}

pub type Result<T> = std::result::Result<T, CommandError>;

#[derive(Debug, Clone)]
pub enum Cmd {
    Help,
    Welcome,
    Version,

    BuildFlan(build_muf::BuildMufOptions),
    Resolve(build_muf::BuildMufOptions),
    Check(build_muf::BuildMufOptions),
    Print(build_muf::BuildMufOptions),
    Run(run_muf::RunOptions),
    Ninja(NinjaOptions),

    Doctor(DoctorOptions),
    ToolchainDoctor(ToolchainDoctorOptions),
    Cache(CacheOptions),

    Graph(GraphOptions),
    Fmt(FmtOptions),
    EditorSetup,
    Editor(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct GraphOptions {
    pub root_dir: Option<PathBuf>,
    pub format: GraphFormat,
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum GraphFormat {
    #[default]
    Text,
    Dot,
}

#[derive(Debug, Clone, Default)]
pub struct FmtOptions {
    pub file: Option<PathBuf>,
    pub check_only: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, Default)]
pub struct NinjaOptions {
    pub root_dir: Option<PathBuf>,
    pub targets_path: Option<PathBuf>,
    pub emit_path: Option<PathBuf>,
    pub print: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorOptions {
    pub root_dir: Option<PathBuf>,
    pub verbose: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ToolchainDoctorOptions {
    pub verbose: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheAction {
    Status,
    Clear,
}

#[derive(Debug, Clone)]
pub struct CacheOptions {
    pub root_dir: Option<PathBuf>,
    pub action: CacheAction,
    pub verbose: bool,
    pub json: bool,
}

/// Entry point used by main binaries.
/// Returns a process exit code (0=OK, 2=usage, 1=runtime failure).
pub fn run_cli(args: &[String]) -> i32 {
    let _ = editor_setup::ensure_editor_setup();
    match dispatch(args) {
        Ok(()) => 0,
        Err(CommandError::Usage(msg)) => {
            eprintln!("{msg}");
            2
        }
        Err(CommandError::Failure { code, msg }) => {
            eprintln!("{msg}");
            if code <= 0 { 1 } else { code }
        }
    }
}

/// Parse + execute command.
pub fn dispatch(args: &[String]) -> Result<()> {
    let cmd = parse_command(args)?;
    execute(cmd)
}

/// Parse argv into a command.
/// `args` is expected to be the full argv (including program name at index 0).
pub fn parse_command(args: &[String]) -> Result<Cmd> {
    if args.len() <= 1 {
        return Ok(Cmd::Welcome);
    }

    let sub = args[1].as_str();

    // Global help/version shortcuts
    if sub == "--help" || sub == "-h" || sub == "help" {
        return Ok(Cmd::Help);
    }
    if sub == "--version" || sub == "-V" || sub == "version" {
        return Ok(Cmd::Version);
    }

    match sub {
        // `build steel ...`
        "build" => parse_build(&args[2..]),
        // `resolve ...` (alias)
        "resolve" => {
            let mut o = parse_build_args(&args[2..])?;
            // `resolve` is expected to emit; ensure print=false by default
            o.print = false;
            Ok(Cmd::Resolve(o))
        }
        // `check ...` (validate; best-effort)
        "check" => {
            let mut o = parse_build_args(&args[2..])?;
            o.print = false;
            Ok(Cmd::Check(o))
        }
        // `print ...` (emit + print)
        "print" => {
            let mut o = parse_build_args(&args[2..])?;
            o.print = true;
            Ok(Cmd::Print(o))
        }
        // `run ...` (execute tools)
        "run" => {
            let o = parse_run_args(&args[2..])?;
            Ok(Cmd::Run(o))
        }
        "ninja" => parse_ninja(&args[2..]),
        "doctor" => parse_doctor(&args[2..]),
        "toolchain" => parse_toolchain(&args[2..]),
        "cache" => parse_cache(&args[2..]),
        "editor-setup" => Ok(Cmd::EditorSetup),
        "editor" => {
            let file = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("steelconf"));
            Ok(Cmd::Editor(file))
        }
        "mitsou" => {
            let file = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("steelconf"));
            Ok(Cmd::Editor(file))
        }
        // stubs reserved for future
        "graph" => parse_graph(&args[2..]),
        "fmt" => parse_fmt(&args[2..]),
        other => Err(usage_unknown(other)),
    }
}

fn parse_build(rest: &[String]) -> Result<Cmd> {
    if matches!(rest.first().map(String::as_str), Some("-h" | "--help" | "help")) {
        return Err(usage_err("U005", "build help", usage_text()));
    }
    if rest.is_empty() {
        return Err(CommandError::Usage(build_missing_target_msg()));
    }

    let tool = rest[0].as_str();
    match tool {
        "steelconf" => {
            if rest.len() > 1 {
                return Err(CommandError::Usage(build_unknown_target_msg("flags not allowed")));
            }
            let mut args = Vec::with_capacity(2);
            args.push("--file".to_string());
            args.push(rest[0].clone());
            let o = parse_build_args(&args)?;
            Ok(Cmd::BuildFlan(o))
        }
        other => Err(CommandError::Usage(build_unknown_target_msg(other))),
    }
}

fn parse_toolchain(rest: &[String]) -> Result<Cmd> {
    if rest.is_empty() || matches!(rest.first().map(String::as_str), Some("-h" | "--help" | "help")) {
        return Err(usage_err("U005", "toolchain help", toolchain_help()));
    }

    match rest[0].as_str() {
        "doctor" => {
            let mut o = ToolchainDoctorOptions::default();
            let mut it = rest[1..].iter().peekable();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--json" => o.json = true,
                    "-v" | "--verbose" => o.verbose = true,
                    "-h" | "--help" => return Err(usage_err("U005", "toolchain help", toolchain_help())),
                    other => {
                        return Err(usage_err(
                            "U002",
                            format!("unknown flag: {other}"),
                            toolchain_help(),
                        ));
                    }
                }
            }
            Ok(Cmd::ToolchainDoctor(o))
        }
        other => Err(usage_err(
            "U002",
            format!("unknown toolchain command: {other}"),
            toolchain_help(),
        )),
    }
}

fn parse_graph(rest: &[String]) -> Result<Cmd> {
    let mut o = GraphOptions::default();

    let mut it = rest.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--root expects a path", usage_text()))?;
                o.root_dir = Some(PathBuf::from(v));
            }
            "--dot" => o.format = GraphFormat::Dot,
            "--text" => o.format = GraphFormat::Text,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(usage_err("U005", "graph help", graph_help())),
            x if x.starts_with('-') => {
                return Err(usage_err(
                    "U002",
                    format!("unknown flag: {x}"),
                    graph_help(),
                ))
            }
            other => {
                // positional root (single)
                if o.root_dir.is_some() {
                    return Err(usage_err(
                        "U002",
                        format!("unexpected argument: {other}"),
                        graph_help(),
                    ));
                }
                o.root_dir = Some(PathBuf::from(other));
            }
        }
    }

    Ok(Cmd::Graph(o))
}

fn parse_fmt(rest: &[String]) -> Result<Cmd> {
    let mut o = FmtOptions::default();

    let mut it = rest.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--file" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--file expects a path", fmt_help()))?;
                o.file = Some(PathBuf::from(v));
            }
            "--check" => o.check_only = true,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(usage_err("U005", "fmt help", fmt_help())),
            x if x.starts_with('-') => {
                return Err(usage_err("U002", format!("unknown flag: {x}"), fmt_help()))
            }
            other => {
                if o.file.is_some() {
                    return Err(usage_err(
                        "U002",
                        format!("unexpected argument: {other}"),
                        fmt_help(),
                    ));
                }
                o.file = Some(PathBuf::from(other));
            }
        }
    }

    Ok(Cmd::Fmt(o))
}

fn parse_doctor(rest: &[String]) -> Result<Cmd> {
    let mut o = DoctorOptions::default();

    let mut it = rest.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--root expects a path", doctor_help()))?;
                o.root_dir = Some(PathBuf::from(v));
            }
            "--json" => o.json = true,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(usage_err("U005", "doctor help", doctor_help())),
            x if x.starts_with('-') => {
                return Err(usage_err(
                    "U002",
                    format!("unknown flag: {x}"),
                    doctor_help(),
                ))
            }
            other => {
                return Err(usage_err(
                    "U002",
                    format!("unexpected argument: {other}"),
                    doctor_help(),
                ))
            }
        }
    }

    Ok(Cmd::Doctor(o))
}

fn parse_cache(rest: &[String]) -> Result<Cmd> {
    if rest.is_empty() || matches!(rest.first().map(String::as_str), Some("-h" | "--help" | "help")) {
        return Err(usage_err("U005", "cache help", cache_help()));
    }

    let action = match rest[0].as_str() {
        "status" => CacheAction::Status,
        "clear" => CacheAction::Clear,
        other => {
            return Err(usage_err(
                "U002",
                format!("unknown cache action: {other}"),
                cache_help(),
            ))
        }
    };

    let mut o = CacheOptions {
        root_dir: None,
        action,
        verbose: false,
        json: false,
    };

    let mut it = rest[1..].iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--root expects a path", cache_help()))?;
                o.root_dir = Some(PathBuf::from(v));
            }
            "--json" => o.json = true,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(usage_err("U005", "cache help", cache_help())),
            x if x.starts_with('-') => {
                return Err(usage_err(
                    "U002",
                    format!("unknown flag: {x}"),
                    cache_help(),
                ))
            }
            other => {
                return Err(usage_err(
                    "U002",
                    format!("unexpected argument: {other}"),
                    cache_help(),
                ))
            }
        }
    }

    Ok(Cmd::Cache(o))
}

fn parse_build_args(rest: &[String]) -> Result<build_muf::BuildMufOptions> {
    build_muf::parse_args(rest).map_err(map_build_error)
}

fn parse_run_args(rest: &[String]) -> Result<run_muf::RunOptions> {
    let mut o = run_muf::RunOptions::default();

    let mut positional_root_set = false;
    let mut it = rest.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--root expects a path", run_help()))?;
                o.root_dir = PathBuf::from(v);
                positional_root_set = true;
            }
            "--file" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--file expects a path", run_help()))?;
                o.steel_file = Some(PathBuf::from(v));
            }
            "--profile" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--profile expects a name", run_help()))?;
                o.profile = Some(v.to_string());
            }
            "--toolchain" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--toolchain expects a path", run_help()))?;
                o.toolchain_dir = Some(PathBuf::from(v));
            }
            "--bake" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--bake expects a name", run_help()))?;
                o.bakes.push(v.to_string());
            }
            "--all" => o.run_all = true,
            "--no-cache" => o.no_cache = true,
            "--print" => o.dry_run = true,
            "-v" | "--verbose" => o.verbose = true,
            "--log" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--log expects a path", run_help()))?;
                o.log_path = Some(PathBuf::from(v));
            }
            "--log-mode" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--log-mode expects append|truncate", run_help()))?;
                o.log_mode = match v.as_str() {
                    "append" => run_muf::LogMode::Append,
                    "truncate" => run_muf::LogMode::Truncate,
                    _ => {
                        return Err(usage_err(
                            "U002",
                            "invalid --log-mode (use append|truncate)",
                            run_help(),
                        ))
                    }
                };
            }
            "-h" | "--help" => return Err(usage_err("U005", "run help", run_help())),
            x if x.starts_with('-') => {
                return Err(usage_err("U002", format!("unknown flag: {x}"), run_help()))
            }
            other => {
                if positional_root_set {
                    return Err(usage_err(
                        "U002",
                        format!("unexpected argument: {other}"),
                        run_help(),
                    ));
                }
                o.root_dir = PathBuf::from(other);
                positional_root_set = true;
            }
        }
    }

    Ok(o)
}

fn map_build_error(err: BuildMufError) -> CommandError {
    match err {
        BuildMufError::Arg { msg } => {
            if msg.contains("build —") {
                CommandError::Usage(format_usage("U002", "invalid arguments", msg.as_str()))
            } else {
                usage_err("U002", msg, build_muf::help_text())
            }
        }
        other => CommandError::Failure {
            code: 1,
            msg: err_msg("E001", other.to_string()),
        },
    }
}

fn map_build_error_with_context(
    err: BuildMufError,
    root: &Path,
    file: Option<&Path>,
) -> CommandError {
    let ctx = format_context(root, file);
    match err {
        BuildMufError::Validate { msg } => CommandError::Failure {
            code: 1,
            msg: format!("{}\n{}", err_msg("V001", format!("validate: {msg}")), ctx),
        },
        BuildMufError::Io { .. } => CommandError::Failure {
            code: 1,
            msg: format!("{}\n{}", err_msg("IO01", format!("validate: {}", err)), ctx),
        },
        other => map_build_error(other),
    }
}

pub fn execute(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Help => {
            println!("{}", help_text());
            Ok(())
        }
        Cmd::Welcome => {
            println!("{}", welcome_text());
            Ok(())
        }
        Cmd::Version => {
            println!("{}", version_string());
            Ok(())
        }

        Cmd::BuildFlan(o) => exec_build_steel(o),
        Cmd::Resolve(o) => exec_build_steel(o),

        Cmd::Print(mut o) => {
            o.print = true;
            exec_build_steel(o)
        }
        Cmd::Run(o) => exec_run_muf(o),
        Cmd::Ninja(o) => exec_ninja(o),
        Cmd::Doctor(o) => exec_doctor(o),
        Cmd::ToolchainDoctor(o) => exec_toolchain_doctor(o),
        Cmd::Cache(o) => exec_cache(o),

        Cmd::Check(mut o) => {
            // Best-effort semantics:
            // - run the resolver
            // - emit to a temp-ish path under `.steel-cache/check/`
            // - remove after success (unless strict=false removal failures are ignored)
            let root = if o.root_dir.as_os_str().is_empty() {
                env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                o.root_dir.clone()
            };

            let check_emit = root
                .join(".steel-cache")
                .join("check")
                .join(build_muf::DEFAULT_EMIT_NAME);

            o.emit_path = Some(check_emit.clone());
            o.print = false;

            let file = resolve_file_for_context(&root, o.steel_file.as_deref());
            let _cfg = build_muf::run(&o).map_err(|e| {
                map_build_error_with_context(e, &root, file.as_deref())
            })?;

            match fs::remove_file(&check_emit) {
                Ok(_) => Ok(()),
                Err(err) => {
                    if o.strict {
                        Err(CommandError::Failure {
                            code: 1,
                            msg: err_msg(
                                "IO01",
                                format!(
                                    "cleanup: could not remove {}: {err}",
                                    check_emit.display()
                                )
                            ),
                        })
                    } else {
                        // best-effort: ignore cleanup failure
                        Ok(())
                    }
                }
            }
        }

        Cmd::Graph(o) => exec_graph(o),
        Cmd::Fmt(o) => exec_fmt(o),
        Cmd::EditorSetup => exec_editor_setup(),
        Cmd::Editor(file) => exec_editor(file),
    }
}

fn exec_build_steel(o: build_muf::BuildMufOptions) -> Result<()> {
    let root = if o.root_dir.as_os_str().is_empty() {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        o.root_dir.clone()
    };
    let file = resolve_file_for_context(&root, o.steel_file.as_deref());
    let log_path = build_log_path(&root);
    let emit_path = resolve_emit_path(&root, &o);

    match build_muf::run(&o) {
        Ok(_) => {
            let _ = write_build_log(&log_path, true, "build ok", emit_path.as_deref());
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = write_build_log(&log_path, false, &msg, emit_path.as_deref());
            Err(map_build_error_with_context(e, &root, file.as_deref()))
        }
    }
}

fn build_log_path(root: &Path) -> PathBuf {
    root.join("steel.log")
}

fn resolve_emit_path(root: &Path, o: &build_muf::BuildMufOptions) -> Option<PathBuf> {
    if let Some(p) = &o.emit_path {
        return Some(root_join_if_relative(root, p));
    }
    None
}

fn write_build_log(path: &Path, ok: bool, message: &str, emit_path: Option<&Path>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let text = format_build_log(ok, message, emit_path);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    file.write_all(text.as_bytes())
}

fn format_build_log(ok: bool, message: &str, emit_path: Option<&Path>) -> String {
    let mut out = String::new();
    out.push_str("mff 1\n\n");
    out.push_str("build\n");
    out.push_str(&format!("  tool \"{}\"\n", escape_mff(CLI_NAME)));
    out.push_str("  command \"steel\"\n");
    out.push_str(&format!("  ts_iso \"{}\"\n", escape_mff(&Utc::now().to_rfc3339()))); 
    out.push_str(&format!("  ok {}\n", if ok { "true" } else { "false" }));
    out.push_str(&format!("  message \"{}\"\n", escape_mff(message)));
    if let Some(p) = emit_path {
        out.push_str(&format!("  emit \"{}\"\n", escape_mff(&p.display().to_string())));
    }
    out.push_str(".end\n\n");

    if !ok {
        out.push_str("\nerrors\n");
        out.push_str(&format!("  item \"{}\"\n", escape_mff(message)));
        out.push_str(".end\n\n");
    }

    out
}

fn escape_mff(s: &str) -> String {
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

fn exec_run_muf(o: run_muf::RunOptions) -> Result<()> {
    let root = if o.root_dir.as_os_str().is_empty() {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        o.root_dir.clone()
    };
    let file = resolve_file_for_context(&root, o.steel_file.as_deref());
    run_muf::run(&o).map_err(|e| map_run_error(e, &root, file.as_deref()))?;
    println!("Success Build");
    Ok(())
}

fn exec_graph(o: GraphOptions) -> Result<()> {
    // Reserved: graph export of workspace/rules
    // Current behavior: deterministic placeholder.
    let root = o
        .root_dir
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match o.format {
        GraphFormat::Text => {
            println!("graph (text): not implemented");
            println!("root: {}", root.display());
        }
        GraphFormat::Dot => {
            println!("digraph steel {{");
            println!("  // graph export not implemented");
            println!("  root [label=\"{}\"];", escape_dot(&root.display().to_string()));
            println!("}}");
        }
    }
    Ok(())
}

fn exec_fmt(o: FmtOptions) -> Result<()> {
    // Reserved: steelconf formatter.
    // Current behavior: deterministic placeholder.
    let file = o.file.unwrap_or_else(|| PathBuf::from("steelconf"));
    if o.check_only {
        println!("fmt --check: not implemented (file={})", file.display());
    } else {
        println!("fmt: not implemented (file={})", file.display());
    }
    Ok(())
}

fn exec_editor_setup() -> Result<()> {
    editor_setup::ensure_editor_setup()
        .map_err(|e| CommandError::Failure { code: 1, msg: e.to_string() })?;
    println!("editor-setup: ok (vim, neovim, nano, emacs, helix, micro, zed)");
    Ok(())
}

fn exec_editor(file: PathBuf) -> Result<()> {
    let status = Command::new("steecleditor")
        .arg(file)
        .status()
        .map_err(|e| CommandError::Failure { code: 1, msg: e.to_string() })?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::Failure {
            code: status.code().unwrap_or(1),
            msg: "steecleditor exited with error".to_string(),
        })
    }
}

fn version_string() -> String {
    "Steel Version 2.2026".to_string()
}

fn usage_unknown(cmd: &str) -> CommandError {
    usage_err(
        "U001",
        format!("unknown command: {cmd}."),
        "Run `steel --help` for the list of commands.",
    )
}

pub fn usage_text() -> &'static str {
    "steel <command> [options]\nRun `steel --help` for the list of commands."
}

fn build_missing_target_msg() -> String {
    format_usage(
        "U003",
        "missing build target: expected `steelconf`.",
        "steel build steelconf",
    )
}

fn build_unknown_target_msg(target: &str) -> String {
    format_usage(
        "U004",
        format!("unknown build target: {target}. Expected `steelconf` only."),
        "steel build steelconf",
    )
}

fn format_usage(code: &str, msg: impl Into<String>, help: &str) -> String {
    let mut out = err_msg(code, msg);
    let h = help.trim();
    if !h.is_empty() {
        out.push('\n');
        out.push_str("help: ");
        out.push_str(h);
    }
    out
}

fn usage_err(code: &str, msg: impl Into<String>, help: &str) -> CommandError {
    CommandError::Usage(format_usage(code, msg, help))
}


fn help_text() -> String {
    "USAGE\n  steel <command> [options]\n\nCOMMANDS\n  run            Run a build (steelconf)\n  build          Build once (alias of run)\n  fmt            Format a steelconf\n  doctor         Diagnose environment\n  graph          Inspect graph\n  ninja          Emit Ninja (stub)\n  cache          Cache utilities\n  toolchain      Toolchain utilities\n  editor         Open steelconf editor\n  mitsou         Open Mitsou editor\n  editor-setup   Install editor settings for steelconf (vim/nano/emacs/etc)\n  help           Show help\n  version        Show version\n\nGLOBAL FLAGS\n  -h, --help     Show help\n  -v, --version  Show version\n\nKEYWORDS (steelconf core)\n  steel          File header / format marker\n  bake           Recipe block\n  store          Store block\n  capsule        Sandbox / policy block\n  var            Variable block\n  profile        Profile block\n  tool           Tool declaration\n  plan           Plan block\n  switch         Conditional block\n  run            Execution step\n  export         Export recipe\n  exports        Export list\n  wire           Wire outputs/inputs\n\nKEYWORDS (io + build)\n  in             Input binding\n  out            Output binding\n  make           Source collection (glob)\n  takes          Inputs -> flags\n  emits          Outputs -> flags\n  output         Final output\n  set            Add flag/value\n  at             Path anchor\n\nKEYWORDS (cache + sandbox)\n  cache          Cache block\n  mode           Cache or policy mode\n  path           Path policy\n  env            Env policy\n  fs             Filesystem policy\n  net            Network policy\n  time           Time policy\n  allow          Allow rule\n  deny           Deny rule\n  allow_read     Allow read access\n  allow_write    Allow write access\n  allow_write_exact Allow exact write path\n  stable         Mark stable inputs/outputs\n".to_string()
}

#[allow(dead_code)]
fn welcome_text() -> &'static str {
    "Welcome Vitte\n    _________ __                .__    _________                                           .___\n   /   _____//  |_  ____   ____ |  |   \\_   ___ \\  ____   _____   _____ _____    ____    __| _/\n   \\_____  \\\\   __\\/ __ \\_/ __ \\|  |   /    \\  \\/ /  _ \\ /     \\ /     \\\\__  \\  /    \\  / __ | \n   /        \\|  | \\  ___/\\  ___/|  |__ \\     \\___(  <_> )  Y Y  \\  Y Y  \\/ __ \\|   |  \\/ /_/ | \n  /_______  /|__|  \\___  >\\___  >____/  \\______  /\\____/|__|_|  /__|_|  (____  /___|  /\\____ | \n          \\/           \\/     \\/               \\/             \\/      \\/     \\/     \\/      \\/ \n<Vitte_ORG> 2026"
}

fn graph_help() -> &'static str {
    "steel graph (stub)

USAGE:
  steel graph [--root <path>] [--text|--dot] [-v]

FLAGS:
  --root <path>   Workspace root
  --text          Text format (default)
  --dot           DOT format
  -v, --verbose   Verbose output"
}

fn fmt_help() -> &'static str {
    "steel fmt (stub)

USAGE:
  steel fmt [--file <path>] [--check] [-v]

FLAGS:
  --file <path>   steelconf path (default: steelconf)
  --check         Check-only mode
  -v, --verbose   Verbose output"
}

fn ninja_help() -> &'static str {
    "steel ninja — Generate build.ninja from a target file

USAGE:
  steel ninja [--root <path>] [--targets <path>] [--emit <path>] [--print] [-v]

FLAGS:
  --root <path>      Workspace root (default: cwd)
  --targets <path>   Target file (default: <root>/targets.steel)
  --emit <path>      Output build.ninja path (default: <root>/build.ninja)
  --print            Also print to stdout
  -v, --verbose      Verbose output"
}

fn run_help() -> &'static str {
    "steel run — Execute tool steps from steelconf

USAGE:
  steel run [--root <path>] [--file <path>] [--profile <name>] [--toolchain <path>] [--bake <name>] [--all] [--print] [--no-cache] [--log <path>] [--log-mode <m>] [-v]

FLAGS:
  --root <path>     Workspace root (default: cwd)
  --file <path>     Explicit steelconf path (default: steelconf under root)
  --profile <name>  Select profile (default: workspace.profile or debug)
  --toolchain <p>   Toolchain directory (overrides PATH lookup for tools)
  --bake <name>     Run a specific bake (repeatable)
  --all             Run all bakes in file order (with deps)
  --print           Dry-run: print commands only
  --no-cache        Disable incremental skip
  --log <path>      Write run log to a specific .mff path
  --log-mode <m>    Log write mode: append (default) or truncate
  -v, --verbose     Verbose output"
}

fn doctor_help() -> &'static str {
    "steel doctor

USAGE:
  steel doctor [--root <path>] [--json] [-v]

FLAGS:
  --root <path>   Workspace root (default: cwd)
  --json          JSON output (machine-friendly)
  -v, --verbose   Verbose output"
}

fn toolchain_help() -> &'static str {
    "steel toolchain doctor

USAGE:
  steel toolchain doctor [--json] [-v]

FLAGS:
  --json          JSON output (machine-friendly)
  -v, --verbose   Verbose output"
}

fn cache_help() -> &'static str {
    "steel cache

USAGE:
  steel cache <status|clear> [--root <path>] [--json] [-v]

FLAGS:
  --root <path>   Workspace root (default: cwd)
  --json          JSON output (machine-friendly)
  -v, --verbose   Verbose output"
}

fn err_msg(code: &str, msg: impl Into<String>) -> String {
    format!("error[{code}]: {}", msg.into())
}

fn parse_ninja(rest: &[String]) -> Result<Cmd> {
    if matches!(rest.first().map(String::as_str), Some("-h" | "--help" | "help")) {
        return Err(usage_err("U005", "ninja help", ninja_help()));
    }

    let mut o = NinjaOptions::default();
    let mut it = rest.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--root expects a path", ninja_help()))?;
                o.root_dir = Some(PathBuf::from(v));
            }
            "--targets" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--targets expects a path", ninja_help()))?;
                o.targets_path = Some(PathBuf::from(v));
            }
            "--emit" => {
                let v = it
                    .next()
                    .ok_or_else(|| usage_err("U002", "--emit expects a path", ninja_help()))?;
                o.emit_path = Some(PathBuf::from(v));
            }
            "--print" => o.print = true,
            "-v" | "--verbose" => o.verbose = true,
            x if x.starts_with('-') => {
                return Err(usage_err(
                    "U005",
                    format!("unknown flag: {x}"),
                    ninja_help(),
                ))
            }
            other => {
                return Err(usage_err(
                    "U005",
                    format!("unexpected argument: {other}"),
                    ninja_help(),
                ))
            }
        }
    }

    Ok(Cmd::Ninja(o))
}

fn exec_ninja(o: NinjaOptions) -> Result<()> {
    let root = o
        .root_dir
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let targets_path = o
        .targets_path
        .clone()
        .unwrap_or_else(|| root.join("targets.steel"));
    let emit_path = o
        .emit_path
        .clone()
        .unwrap_or_else(|| root.join("build.ninja"));

    let tf = target_file::parse_target_file_path(&targets_path, &target_file::ParseOptions::default())
        .map_err(|e| CommandError::Failure {
            code: 1,
            msg: err_msg("P001", format!("targets: {e}")),
        })?;
    let ninja_text = ninja::render_ninja(&tf).map_err(|e| CommandError::Failure {
        code: 1,
        msg: err_msg("N001", format!("ninja: {e}")),
    })?;

    if let Some(parent) = emit_path.parent() {
        fs::create_dir_all(parent).map_err(|e| CommandError::Failure {
            code: 1,
            msg: err_msg("IO01", format!("mkdir {}: {e}", parent.display())),
        })?;
    }
    fs::write(&emit_path, ninja_text.as_bytes()).map_err(|e| CommandError::Failure {
        code: 1,
        msg: err_msg("IO01", format!("write {}: {e}", emit_path.display())),
    })?;

    if o.print {
        println!("{}", ninja_text);
    }

    if o.verbose {
        println!("ninja: {}", emit_path.display());
    }

    Ok(())
}

fn resolve_file_for_context(root: &Path, file: Option<&Path>) -> Option<PathBuf> {
    file.map(|p| if p.is_absolute() { p.to_path_buf() } else { root.join(p) })
}

fn format_context(root: &Path, file: Option<&Path>) -> String {
    let file_display = match file {
        Some(p) => p.display().to_string(),
        None => "auto".to_string(),
    };
    format!("context: root={} file={}", root.display(), file_display)
}

fn map_run_error(err: run_muf::RunError, root: &Path, file: Option<&Path>) -> CommandError {
    let ctx = format_context(root, file);
    match err {
        run_muf::RunError::Config { msg, help } => {
            let mut msg = err_msg("V001", format!("validate: {msg}"));
            msg.push('\n');
            msg.push_str(&ctx);
            if let Some(h) = help {
                msg.push('\n');
                msg.push_str("help: ");
                msg.push_str(&h);
            }
            CommandError::Failure { code: 2, msg }
        }
        run_muf::RunError::Parse { path, msg } => CommandError::Failure {
            code: 2,
            msg: format!(
                "{}\n{}",
                err_msg("P001", format!("parse: {}: {msg}", path.display())),
                ctx
            ),
        },
        run_muf::RunError::Io { op, path, err } => CommandError::Failure {
            code: 4,
            msg: format!(
                "{}\n{}",
                err_msg("IO01", format!("run: {op} {}: {err}", path.display())),
                ctx
            ),
        },
        run_muf::RunError::Exec { cmd, status, stderr } => {
            let mut s = match status {
                Some(code) => format!("command failed ({code}): {cmd}"),
                None => format!("command failed: {cmd}"),
            };
            if let Some(err) = stderr {
                if !err.trim().is_empty() {
                    s.push('\n');
                    s.push_str(err.trim_end());
                }
            }
            CommandError::Failure {
                code: 3,
                msg: format!("{}\n{}", err_msg("X001", format!("run: {s}")), ctx),
            }
        }
    }
}

fn escape_dot(s: &str) -> String {
    // Minimal DOT string escape.
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn exec_doctor(o: DoctorOptions) -> Result<()> {
    let root = o
        .root_dir
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let config_path = root.join("steelconf");

    let tools = ["steel", "gcc", "ar"];
    if o.json {
        let config_exists = config_path.exists();
        let mut out = String::new();
        out.push_str("{\"root\":\"");
        out.push_str(&json_escape(&root.display().to_string()));
        out.push_str("\",\"config\":{\"path\":\"");
        out.push_str(&json_escape(&config_path.display().to_string()));
        out.push_str("\",\"exists\":");
        out.push_str(if config_exists { "true" } else { "false" });
        out.push_str("},\"tools\":[");
        let mut first = true;
        for tool in tools {
            if !first {
                out.push(',');
            }
            first = false;
            match find_in_path(tool) {
                Some(path) => {
                    out.push_str("{\"name\":\"");
                    out.push_str(tool);
                    out.push_str("\",\"ok\":true,\"path\":\"");
                    out.push_str(&json_escape(&path.display().to_string()));
                    out.push_str("\"}");
                }
                None => {
                    out.push_str("{\"name\":\"");
                    out.push_str(tool);
                    out.push_str("\",\"ok\":false}");
                }
            }
        }
        out.push_str("]}");
        println!("{out}");
        return Ok(());
    }

    println!("steel doctor");
    println!("root: {}", root.display());
    if config_path.exists() {
        println!("config: ok ({})", config_path.display());
    } else {
        println!("config: missing ({})", config_path.display());
    }

    for tool in tools {
        match find_in_path(tool) {
            Some(path) => {
                if o.verbose {
                    println!("tool: {tool} => {}", path.display());
                } else {
                    println!("tool: {tool} ok");
                }
            }
            None => println!("tool: {tool} missing"),
        }
    }

    Ok(())
}

fn exec_toolchain_doctor(o: ToolchainDoctorOptions) -> Result<()> {
    let python_override = env::var("MUFFIN_TOOLCHAIN_PYTHON").ok();
    let python_env = env::var("PYTHON").ok();
    let python_source = if python_override.is_some() {
        "override"
    } else if python_env.is_some() {
        "env"
    } else {
        "path"
    };
    let python_exec = python_override
        .clone()
        .or(python_env.clone())
        .or_else(|| find_in_path("python3").map(|p| p.display().to_string()))
        .or_else(|| find_in_path("python").map(|p| p.display().to_string()));
    let python_info = python_exec
        .as_deref()
        .and_then(|p| python_impl_and_version(p));

    let ocamlc_exec = find_in_path("ocamlc").map(|p| p.display().to_string());
    let ocamlopt_exec = find_in_path("ocamlopt").map(|p| p.display().to_string());
    let ocamlc_version = ocamlc_exec
        .as_deref()
        .and_then(|p| ocaml_version(p));
    let ocamlopt_version = ocamlopt_exec
        .as_deref()
        .and_then(|p| ocaml_version(p));

    if o.json {
        let mut out = String::new();
        out.push_str("{\"python\":");
        match (&python_exec, &python_info) {
            (Some(path), Some((impl_name, ver))) => {
                out.push_str("{\"ok\":true,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\",\"implementation\":\"");
                out.push_str(&json_escape(impl_name));
                out.push_str("\",\"version\":\"");
                out.push_str(&json_escape(ver));
                out.push_str("\",\"source\":\"");
                out.push_str(python_source);
                out.push_str("\"}");
            }
            (Some(path), None) => {
                out.push_str("{\"ok\":false,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\",\"source\":\"");
                out.push_str(python_source);
                out.push_str("\"}");
            }
            (None, _) => out.push_str("{\"ok\":false}"),
        }

        out.push_str(",\"ocaml\":{");
        out.push_str("\"ocamlc\":");
        match (&ocamlc_exec, &ocamlc_version) {
            (Some(path), Some(ver)) => {
                out.push_str("{\"ok\":true,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\",\"version\":\"");
                out.push_str(&json_escape(ver));
                out.push_str("\"}");
            }
            (Some(path), None) => {
                out.push_str("{\"ok\":false,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\"}");
            }
            (None, _) => out.push_str("{\"ok\":false}"),
        }
        out.push_str(",\"ocamlopt\":");
        match (&ocamlopt_exec, &ocamlopt_version) {
            (Some(path), Some(ver)) => {
                out.push_str("{\"ok\":true,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\",\"version\":\"");
                out.push_str(&json_escape(ver));
                out.push_str("\"}");
            }
            (Some(path), None) => {
                out.push_str("{\"ok\":false,\"path\":\"");
                out.push_str(&json_escape(path));
                out.push_str("\"}");
            }
            (None, _) => out.push_str("{\"ok\":false}"),
        }
        out.push_str("}");

        out.push_str("}");
        println!("{out}");
        return Ok(());
    }

    println!("steel toolchain doctor");

    match (&python_exec, &python_info) {
        (Some(path), Some((impl_name, ver))) => {
            if o.verbose {
                println!("python: ok ({path}) {impl_name} {ver} [{python_source}]");
            } else {
                println!("python: ok ({impl_name} {ver})");
            }
        }
        (Some(path), None) => println!("python: failed ({path})"),
        (None, _) => println!("python: missing"),
    }

    match (&ocamlc_exec, &ocamlc_version) {
        (Some(path), Some(ver)) => {
            if o.verbose {
                println!("ocamlc: ok ({path}) {ver}");
            } else {
                println!("ocamlc: ok ({ver})");
            }
        }
        (Some(path), None) => println!("ocamlc: failed ({path})"),
        (None, _) => println!("ocamlc: missing"),
    }

    match (&ocamlopt_exec, &ocamlopt_version) {
        (Some(path), Some(ver)) => {
            if o.verbose {
                println!("ocamlopt: ok ({path}) {ver}");
            } else {
                println!("ocamlopt: ok ({ver})");
            }
        }
        (Some(path), None) => println!("ocamlopt: failed ({path})"),
        (None, _) => println!("ocamlopt: missing"),
    }

    Ok(())
}

fn exec_cache(o: CacheOptions) -> Result<()> {
    let root = o
        .root_dir
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cache_dir = root.join(".steel-cache");

    match o.action {
        CacheAction::Status => {
            if o.json {
                let exists = cache_dir.exists();
                let (files, bytes) = if exists {
                    dir_stats(&cache_dir)?
                } else {
                    (0, 0)
                };
                println!(
                    "{{\"cache_dir\":\"{}\",\"exists\":{},\"files\":{},\"bytes\":{}}}",
                    json_escape(&cache_dir.display().to_string()),
                    if exists { "true" } else { "false" },
                    files,
                    bytes
                );
                return Ok(());
            }
            if !cache_dir.exists() {
                println!("cache: empty ({})", cache_dir.display());
                return Ok(());
            }
            let (files, bytes) = dir_stats(&cache_dir)?;
            println!("cache: {}", cache_dir.display());
            println!("files: {files}");
            println!("bytes: {bytes}");
        }
        CacheAction::Clear => {
            let existed = cache_dir.exists();
            if !existed {
                if o.json {
                    println!(
                        "{{\"cache_dir\":\"{}\",\"cleared\":false,\"exists\":false}}",
                        json_escape(&cache_dir.display().to_string())
                    );
                } else {
                    println!("cache: empty ({})", cache_dir.display());
                }
                return Ok(());
            }
            fs::remove_dir_all(&cache_dir).map_err(|e| CommandError::Failure {
                code: 4,
                msg: err_msg("IO01", format!("remove {}: {e}", cache_dir.display())),
            })?;
            if o.json {
                println!(
                    "{{\"cache_dir\":\"{}\",\"cleared\":true,\"exists\":true}}",
                    json_escape(&cache_dir.display().to_string())
                );
            } else {
                println!("cache cleared: {}", cache_dir.display());
            }
        }
    }

    Ok(())
}

fn dir_stats(path: &Path) -> Result<(u64, u64)> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(p) = stack.pop() {
        let entries = fs::read_dir(&p).map_err(|e| CommandError::Failure {
            code: 4,
            msg: err_msg("IO01", format!("read {}: {e}", p.display())),
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| CommandError::Failure {
                code: 4,
                msg: err_msg("IO01", format!("read {}: {e}", p.display())),
            })?;
            let meta = entry.metadata().map_err(|e| CommandError::Failure {
                code: 4,
                msg: err_msg("IO01", format!("stat {}: {e}", entry.path().display())),
            })?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                files += 1;
                bytes += meta.len();
            }
        }
    }

    Ok((files, bytes))
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

fn ocaml_version(ocaml: &str) -> Option<String> {
    let out = Command::new(ocaml).arg("-version").output().ok()?;
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

fn find_in_path(tool: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(tool);
        if candidate.exists() {
            return Some(candidate);
        }
        if cfg!(windows) && !tool.to_ascii_lowercase().ends_with(".exe") {
            let candidate = dir.join(format!("{tool}.exe"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Helper: resolve a path relative to a root if `p` is not absolute.
pub fn root_join_if_relative(root: &Path, p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}
