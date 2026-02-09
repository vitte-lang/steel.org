//! Steel CLI entrypoint (main.rs) — MAX.
//!
//! This is a std-first CLI bootstrap that wires the "steel" crate commands.
//! It avoids external argument parsers; if you already use `clap`, replace the
//! `Argv` implementation with your clap derive layer.
//!
//! Commands (expected):
//! - `steel decompile <input.mff> [-o out] [--report text|md|json] [--overwrite] [--allow-tools] [--no-artifacts] [--no-logs]`
//! - `steel version`
//! - `steel help`
//!
//! The command implementations are delegated to `crate::steel::*` modules.
//!
//! Adapt module paths to your repo structure. In SteelLib, typical layout is:
//!   lib/src/steel/src/{decompile.rs,...}
//! and `lib/src/lib.rs` re-exports a `steel` module.
//!
//! If your binary crate is separate, change `use steellib::steel::...`.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use mcfg::decompile::{decompile_mff, DecompileOptions, ReportFormat};
use SteelLib::error::SteelError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e.render_cli("steel"));
            ExitCode::from(e.exit_code())
        }
    }
}

fn run() -> Result<(), SteelError> {
    let mut argv = Argv::from_env();

    // prog name
    let _prog = argv.next().unwrap_or_else(|| "steel".into());

    let cmd = match argv.peek() {
        None => return Err(cli_usage_error(
            "missing command",
            "Run `steel help` for the list of commands.",
        )),
        Some(c) => c.clone(),
    };

    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            print!("{}", help_text());
            Ok(())
        }
        "version" | "-V" | "--version" => {
            println!("{}", version_text());
            Ok(())
        }
        "decompile" => cmd_decompile(argv.consume()),
        _ => Err(cli_usage_error(
            format!("unknown command: {cmd}"),
            "Run `steel help` for the list of commands.",
        )),
    }
}

fn cmd_decompile(args: Vec<String>) -> Result<(), SteelError> {
    // Minimal flag parsing:
    // steel decompile <input> [-o out] [--report text|md|json] [--overwrite]
    //                    [--allow-tools] [--no-artifacts] [--no-logs] [--no-skeleton]
    let mut a = Argv::new(args);

    // drop "decompile"
    let _ = a.next();

    let input = a
        .next()
        .ok_or_else(|| cli_usage_error("decompile: missing <input.mff>", "Run `steel decompile --help` for options."))?;

    let mut opts = DecompileOptions::default();

    while let Some(t) = a.next() {
        match t.as_str() {
            "-o" | "--out" => {
                let v = a.next().ok_or_else(|| "decompile: -o/--out expects a path".to_string())?;
                opts.out_dir = PathBuf::from(v);
            }
            "--overwrite" => opts.overwrite = true,
            "--allow-tools" => opts.allow_tools = true,
            "--allow-plugins" => opts.allow_plugins = true,
            "--no-artifacts" => opts.allow_artifacts = false,
            "--no-logs" => opts.allow_logs = false,
            "--no-report" => opts.write_report = false,
            "--no-skeleton" => opts.emit_skeleton = false,
            "--report" => {
                let v = a.next().ok_or_else(|| {
                    cli_usage_error("decompile: --report expects text|md|json", "Run `steel decompile --help` for options.")
                })?;
                opts.report_format = match v.as_str() {
                    "text" | "txt" => ReportFormat::Text,
                    "md" | "markdown" => ReportFormat::Markdown,
                    "json" => ReportFormat::Json,
                    _ => {
                        return Err(cli_usage_error(
                            "decompile: invalid --report (use text|md|json)",
                            "Run `steel decompile --help` for options.",
                        ))
                    }
                };
            }
            "--verify" => opts.verify = true,
            "--no-strict-paths" => opts.strict_paths = false,
            "-h" | "--help" => {
                print!("{}", decompile_help_text());
                return Ok(());
            }
            _ => {
                return Err(cli_usage_error(
                    format!("decompile: unknown flag: {t}"),
                    "Run `steel decompile --help` for options.",
                ))
            }
        }
    }

    let res = decompile_mff(&input, opts)
        .map_err(|e| SteelError::ExecutionFailed(format!("decompile: {e}")))?;

    // Minimal human output
    println!("Input: {}", res.input.display());
    println!("Out:   {}", res.out_dir.display());
    println!("Entries: {}", res.entries_total);
    println!("Extracted: {}", res.extracted);
    println!("Skipped:   {}", res.skipped);

    if let Some(p) = res.report_path {
        println!("Report: {}", p.display());
    }

    Ok(())
}

fn cli_usage_error(message: impl Into<String>, help: impl Into<String>) -> SteelError {
    SteelError::InvalidCommand {
        message: message.into(),
        help: vec![help.into()],
    }
}

/* ----------------------------- Argv helper ----------------------------- */

#[derive(Debug, Clone)]
struct Argv {
    v: Vec<String>,
    i: usize,
}

impl Argv {
    fn from_env() -> Self {
        Self::new(env::args().collect())
    }

    fn new(v: Vec<String>) -> Self {
        Self { v, i: 0 }
    }

    fn next(&mut self) -> Option<String> {
        if self.i >= self.v.len() {
            None
        } else {
            let s = self.v[self.i].clone();
            self.i += 1;
            Some(s)
        }
    }

    fn peek(&self) -> Option<&String> {
        self.v.get(self.i)
    }

    fn consume(mut self) -> Vec<String> {
        // return remaining args starting at current cursor
        self.v.split_off(self.i)
    }
}

/* ----------------------------- Help/version ---------------------------- */

fn version_text() -> String {
    // If you have `CARGO_PKG_VERSION`, prefer it.
    let v = option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0");
    format!("steel {v}")
}

fn help_text() -> String {
    let mut s = String::new();
    s.push_str("steel — build system / toolchain\n\n");
    s.push_str("USAGE:\n");
    s.push_str("  steel <command> [options]\n\n");
    s.push_str("COMMANDS:\n");
    s.push_str("  decompile   Extract and report an .mff bundle\n");
    s.push_str("  version     Print version\n");
    s.push_str("  help        Show this help\n\n");
    s.push_str("Run `steel decompile --help` for decompile options.\n");
    s
}

fn decompile_help_text() -> String {
    let mut s = String::new();
    s.push_str("steel decompile\n\n");
    s.push_str("USAGE:\n");
    s.push_str("  steel decompile <input.mff> [options]\n\n");
    s.push_str("OPTIONS:\n");
    s.push_str("  -o, --out <dir>           Output directory (default: decompile_out)\n");
    s.push_str("      --overwrite           Overwrite existing files\n");
    s.push_str("      --report <fmt>        text|md|json (default: text)\n");
    s.push_str("      --no-report           Do not write report file\n");
    s.push_str("      --allow-tools         Extract tool entries (unsafe by default)\n");
    s.push_str("      --allow-plugins       Extract plugin entries (unsafe by default)\n");
    s.push_str("      --no-artifacts        Do not extract artifacts\n");
    s.push_str("      --no-logs             Do not extract logs\n");
    s.push_str("      --no-skeleton         Do not write skeleton README/src hints\n");
    s.push_str("      --verify              Verify signatures/provenance (not implemented std-only)\n");
    s.push_str("      --no-strict-paths     Relax bundle path sanitization\n");
    s
}
