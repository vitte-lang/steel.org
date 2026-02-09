//! `steel decompile` (MAX).
//!
//! Decompile is the inverse view of Steel's build/bundle pipeline.
//!
//! It supports two main workflows:
//! 1) Decompile a `.mff` bundle (Steel File Format) into a directory tree,
//!    reconstructing sources/manifests/artifacts/tools (depending on policy).
//! 2) Decompile a "build config" (e.g. `build.muf`, project folder) into a
//!    normalized, machine-readable architecture report (JSON/TOML/text).
//!
//! This module focuses on `.mff` decompile: extract TOC/index, dump files,
//! generate a human-readable report, and optionally emit a project skeleton.
//
//! Security/Policy:
//! - In a strict mode, you should avoid extracting tool binaries by default.
//! - Enforce capsule policy: no traversal outside output dir.
//! - Optionally verify signature/provenance before extraction.
//
//! Dependencies:
//! - Uses `mff::reader` and `mff::index`.
//! - Avoids external crates; std-only.
//!
//! CLI integration expected shape:
//!   steel decompile <input.mff> [-o out_dir] [--report json|md|text] [--allow-tools]
//!   steel decompile <path> --arch-report
//!
//! This file does not implement argument parsing; it provides the callable API.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use steel_mff::index::{EntryKind, MffEntry};
use steel_mff::reader::{MffReader, ReadError};

#[derive(Debug)]
pub enum DecompileError {
    Io(io::Error),
    Read(ReadError),
    InvalidOutputDir(PathBuf),
    UnsafePath(String),
    Unsupported(&'static str),
    Msg(String),
}

impl fmt::Display for DecompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecompileError::Io(e) => write!(f, "io: {e}"),
            DecompileError::Read(e) => write!(f, "mff read: {e}"),
            DecompileError::InvalidOutputDir(p) => write!(f, "invalid output dir: {}", p.display()),
            DecompileError::UnsafePath(p) => write!(f, "unsafe path: {p}"),
            DecompileError::Unsupported(s) => write!(f, "unsupported: {s}"),
            DecompileError::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for DecompileError {}

impl From<io::Error> for DecompileError {
    fn from(e: io::Error) -> Self {
        DecompileError::Io(e)
    }
}

impl From<ReadError> for DecompileError {
    fn from(e: ReadError) -> Self {
        DecompileError::Read(e)
    }
}

fn derr(msg: impl Into<String>) -> DecompileError {
    DecompileError::Msg(msg.into())
}

/// Decompile options.
#[derive(Debug, Clone)]
pub struct DecompileOptions {
    /// Output directory for extraction.
    pub out_dir: PathBuf,

    /// If true, create `out_dir` if it doesn't exist.
    pub create_out_dir: bool,

    /// If true, overwrite existing files. If false, skip existing files.
    pub overwrite: bool,

    /// Extract tool binaries (EntryKind::Tool). Default false for safety.
    pub allow_tools: bool,

    /// Extract artifacts (EntryKind::Artifact). Default true (can be large).
    pub allow_artifacts: bool,

    /// Extract plugins (EntryKind::Plugin). Default false for safety.
    pub allow_plugins: bool,

    /// Extract logs (EntryKind::Log). Default true.
    pub allow_logs: bool,

    /// If true, generate an inventory report file.
    pub write_report: bool,

    /// Report format: "text" | "md" | "json".
    pub report_format: ReportFormat,

    /// If true, also emit a reconstructed "project skeleton" (src/, build files),
    /// best-effort, using extracted sources/manifests.
    pub emit_skeleton: bool,

    /// If true, verify signature/provenance before extraction (stub).
    pub verify: bool,

    /// If true, sanitize bundle paths aggressively (no symlinks, no .., no absolute).
    pub strict_paths: bool,
}

impl Default for DecompileOptions {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from("decompile_out"),
            create_out_dir: true,
            overwrite: false,
            allow_tools: false,
            allow_artifacts: true,
            allow_plugins: false,
            allow_logs: true,
            write_report: true,
            report_format: ReportFormat::Text,
            emit_skeleton: true,
            verify: false,
            strict_paths: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Markdown,
    Json,
}

impl ReportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ReportFormat::Text => "txt",
            ReportFormat::Markdown => "md",
            ReportFormat::Json => "json",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecompileResult {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub extracted: usize,
    pub skipped: usize,
    pub entries_total: usize,
    pub by_kind: BTreeMap<EntryKind, usize>,
    pub report_path: Option<PathBuf>,
}

/// Decompile an `.mff` file into `opts.out_dir`.
pub fn decompile_mff(input_mff: impl AsRef<Path>, opts: DecompileOptions) -> Result<DecompileResult, DecompileError> {
    let input_mff = input_mff.as_ref().to_path_buf();

    prepare_out_dir(&opts.out_dir, opts.create_out_dir)?;

    // Optional verification (stub): signature/provenance validation before extraction.
    if opts.verify {
        // Hook point: verify signatures, check provenance hash, etc.
        // In max mode we keep std-only and do not implement crypto here.
        // Return Unsupported for now to force explicit implementation.
        return Err(DecompileError::Unsupported("signature verification not implemented (std-only)"));
    }

    let mut r = MffReader::open(&input_mff)?;

    let entries_total = r.index.entries.len();

    let mut extracted = 0usize;
    let mut skipped = 0usize;
    let mut by_kind: BTreeMap<EntryKind, usize> = BTreeMap::new();

    // Inventory for report
    let mut inventory: Vec<InventoryRow> = Vec::with_capacity(entries_total);

    for e in r.index.entries.clone() {
        *by_kind.entry(e.kind).or_insert(0) += 1;

        // Decide extraction policy
        if !kind_allowed(e.kind, &opts) {
            inventory.push(InventoryRow::from_entry(&e, InventoryStatus::Denied));
            skipped += 1;
            continue;
        }

        // Determine output path
        let out_path = match make_entry_out_path(&e, &opts) {
            Ok(p) => p,
            Err(err) => {
                inventory.push(InventoryRow::from_entry(&e, InventoryStatus::Error));
                if opts.strict_paths {
                    return Err(err);
                }
                skipped += 1;
                continue;
            }
        };

        // If it has no path/logical, skip (cannot name it)
        if out_path.is_none() {
            inventory.push(InventoryRow::from_entry(&e, InventoryStatus::Skipped));
            skipped += 1;
            continue;
        }

        let out_path = out_path.unwrap();

        // Create parent directories
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if out_path.exists() && !opts.overwrite {
            inventory.push(InventoryRow::from_entry(&e, InventoryStatus::Exists));
            skipped += 1;
            continue;
        }

        // Extract bytes (decompression is not applied by default; see MffReader option)
        let bytes = r.read_entry(e.id)?;
        std::fs::write(&out_path, bytes)?;
        extracted += 1;

        inventory.push(InventoryRow::from_entry(&e, InventoryStatus::Extracted));
    }

    // Emit skeleton (best effort)
    if opts.emit_skeleton {
        emit_project_skeleton(&opts.out_dir, &inventory)?;
    }

    // Report
    let mut report_path = None;
    if opts.write_report {
        let p = opts
            .out_dir
            .join(format!("mff_report.{}", opts.report_format.ext()));
        write_report(&p, &input_mff, &r, &inventory, opts.report_format)?;
        report_path = Some(p);
    }

    Ok(DecompileResult {
        input: input_mff,
        out_dir: opts.out_dir,
        extracted,
        skipped,
        entries_total,
        by_kind,
        report_path,
    })
}

/* ------------------------------ Policy ----------------------------------- */

fn kind_allowed(kind: EntryKind, opts: &DecompileOptions) -> bool {
    match kind {
        EntryKind::Tool => opts.allow_tools,
        EntryKind::Plugin => opts.allow_plugins,
        EntryKind::Artifact => opts.allow_artifacts,
        EntryKind::Log => opts.allow_logs,
        _ => true,
    }
}

/* ------------------------------ Paths ------------------------------------ */

fn prepare_out_dir(out_dir: &Path, create: bool) -> Result<(), DecompileError> {
    if out_dir.exists() {
        if !out_dir.is_dir() {
            return Err(DecompileError::InvalidOutputDir(out_dir.to_path_buf()));
        }
        return Ok(());
    }
    if create {
        std::fs::create_dir_all(out_dir)?;
        Ok(())
    } else {
        Err(DecompileError::InvalidOutputDir(out_dir.to_path_buf()))
    }
}

fn make_entry_out_path(e: &MffEntry, opts: &DecompileOptions) -> Result<Option<PathBuf>, DecompileError> {
    // Prefer path, else logical -> map into "logical/<kind>/<name>"
    if let Some(p) = &e.path {
        let safe = sanitize_bundle_rel_path(p, opts.strict_paths)?;
        return Ok(Some(opts.out_dir.join(safe)));
    }

    if let Some(l) = &eical {
        let safe_logical = sanitize_logical_name(l)?;
        let rel = PathBuf::from("logical")
            .join(e.kind.name())
            .join(format!("{safe_logical}.bin"));
        return Ok(Some(opts.out_dir.join(rel)));
    }

    Ok(None)
}

fn sanitize_bundle_rel_path(p: &str, strict: bool) -> Result<PathBuf, DecompileError> {
    // We expect bundle paths normalized with forward slashes.
    // Reject absolute, traversal, or Windows drive prefixes.
    if p.is_empty() {
        return Err(DecompileError::UnsafePath(p.into()));
    }
    if p.starts_with('/') || p.starts_with('\\') {
        return Err(DecompileError::UnsafePath(p.into()));
    }
    if p.contains(':') {
        // blocks "C:..."
        return Err(DecompileError::UnsafePath(p.into()));
    }
    if p.contains("..") {
        return Err(DecompileError::UnsafePath(p.into()));
    }
    if strict && (p.contains('\\') || p.contains('\0')) {
        return Err(DecompileError::UnsafePath(p.into()));
    }

    // Convert to PathBuf
    let mut out = PathBuf::new();
    for seg in p.split('/') {
        if seg.is_empty() {
            continue;
        }
        if seg == "." || seg == ".." {
            return Err(DecompileError::UnsafePath(p.into()));
        }
        out.push(seg);
    }

    // final guard: ensure it is relative and contains no prefixes
    if out.components().any(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
        return Err(DecompileError::UnsafePath(p.into()));
    }

    Ok(out)
}

fn sanitize_logical_name(s: &str) -> Result<String, DecompileError> {
    if s.is_empty() {
        return Err(derr("empty logical name"));
    }
    let mut out = String::new();
    for ch in s.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '@' | '+');
        if ok {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    Ok(out)
}

/* ------------------------------ Inventory -------------------------------- */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryStatus {
    Extracted,
    Exists,
    Skipped,
    Denied,
    Error,
}

#[derive(Debug, Clone)]
struct InventoryRow {
    id_hex: String,
    kind: EntryKind,
    path: Option<String>,
    logical: Option<String>,
    offset: u64,
    stored_size: u64,
    original_size: u64,
    compression: String,
    status: InventoryStatus,
}

impl InventoryRow {
    fn from_entry(e: &MffEntry, status: InventoryStatus) -> Self {
        Self {
            id_hex: format!("{:016x}", e.id.0),
            kind: e.kind,
            path: e.path.clone(),
            logical: eical.clone(),
            offset: e.offset,
            stored_size: e.stored_size,
            original_size: e.original_size,
            compression: format!("{:?}", e.compression),
            status,
        }
    }
}

/* ------------------------------ Skeleton --------------------------------- */

fn emit_project_skeleton(out_dir: &Path, inv: &[InventoryRow]) -> Result<(), DecompileError> {
    // Minimal, best-effort: if sources exist, ensure `src/` exists and write
    // a placeholder README describing how to rebuild.
    let mut has_sources = false;
    for r in inv {
        if r.kind == EntryKind::Source {
            has_sources = true;
            break;
        }
    }

    if has_sources {
        std::fs::create_dir_all(out_dir.join("src"))?;
    }

    let readme = out_dir.join("DECOMPILE_README.md");
    if !readme.exists() {
        let mut s = String::new();
        s.push_str("# Decompiled Steel Bundle\n\n");
        s.push_str("This directory was generated by `steel decompile` from an `.mff` bundle.\n\n");
        s.push_str("## Contents\n\n");
        s.push_str("- Extracted bundle entries are placed at their recorded bundle paths.\n");
        s.push_str("- Logical entries are placed under `logical/<kind>/`.\n");
        s.push_str("- A report is available as `mff_report.*`.\n\n");
        s.push_str("## Rebuild (best-effort)\n\n");
        s.push_str("If manifests/build files were extracted, run your standard Steel build workflow.\n");
        s.push_str("Example:\n\n");
        s.push_str("```sh\n");
        s.push_str("build steel -all\n");
        s.push_str("```\n");
        std::fs::write(readme, s)?;
    }

    Ok(())
}

/* ------------------------------ Report ----------------------------------- */

fn write_report(
    path: &Path,
    input: &Path,
    r: &MffReader<std::fs::File>,
    inv: &[InventoryRow],
    fmt: ReportFormat,
) -> Result<(), DecompileError> {
    match fmt {
        ReportFormat::Text => write_report_text(path, input, r, inv),
        ReportFormat::Markdown => write_report_md(path, input, r, inv),
        ReportFormat::Json => write_report_json(path, input, r, inv),
    }
}

fn write_report_text(path: &Path, input: &Path, r: &MffReader<std::fs::File>, inv: &[InventoryRow]) -> Result<(), DecompileError> {
    let mut s = String::new();
    s.push_str("Steel MFF Decompile Report\n");
    s.push_str("===========================\n\n");
    s.push_str(&format!("Input: {}\n", input.display()));
    s.push_str(&format!("Version: {:?}\n", r.header.version));
    s.push_str(&format!("Endian: {:?}\n", r.header.endian));
    s.push_str(&format!("TOC offset: {}\n", r.header.toc_offset));
    s.push_str(&format!("TOC size: {}\n", r.header.toc_size));
    s.push_str(&format!("Entries: {}\n\n", inv.len()));

    let mut by_kind: BTreeMap<EntryKind, usize> = BTreeMap::new();
    let mut by_status: BTreeMap<&'static str, usize> = BTreeMap::new();
    for x in inv {
        *by_kind.entry(x.kind).or_insert(0) += 1;
        *by_status.entry(status_name(x.status)).or_insert(0) += 1;
    }

    s.push_str("By kind:\n");
    for (k, n) in by_kind {
        s.push_str(&format!("  - {}: {}\n", k.name(), n));
    }
    s.push_str("\nBy status:\n");
    for (k, n) in by_status {
        s.push_str(&format!("  - {k}: {n}\n"));
    }

    s.push_str("\nEntries:\n");
    for x in inv {
        s.push_str(&format!(
            "- id={} kind={} status={} path={} logical={} off={} stored={} orig={} comp={}\n",
            x.id_hex,
            x.kind.name(),
            status_name(x.status),
            x.path.as_deref().unwrap_or("-"),
            xical.as_deref().unwrap_or("-"),
            x.offset,
            x.stored_size,
            x.original_size,
            x.compression
        ));
    }

    std::fs::write(path, s)?;
    Ok(())
}

fn write_report_md(path: &Path, input: &Path, r: &MffReader<std::fs::File>, inv: &[InventoryRow]) -> Result<(), DecompileError> {
    let mut s = String::new();
    s.push_str("# Steel MFF Decompile Report\n\n");
    s.push_str(&format!("- **Input:** `{}`\n", input.display()));
    s.push_str(&format!("- **Version:** `{:?}`\n", r.header.version));
    s.push_str(&format!("- **Endian:** `{:?}`\n", r.header.endian));
    s.push_str(&format!("- **TOC:** offset `{}` size `{}`\n", r.header.toc_offset, r.header.toc_size));
    s.push_str(&format!("- **Entries:** `{}`\n\n", inv.len()));

    let mut by_kind: BTreeMap<EntryKind, usize> = BTreeMap::new();
    let mut by_status: BTreeMap<&'static str, usize> = BTreeMap::new();
    for x in inv {
        *by_kind.entry(x.kind).or_insert(0) += 1;
        *by_status.entry(status_name(x.status)).or_insert(0) += 1;
    }

    s.push_str("## Summary\n\n");
    s.push_str("### By kind\n\n");
    for (k, n) in &by_kind {
        s.push_str(&format!("- `{}`: {}\n", k.name(), n));
    }
    s.push_str("\n### By status\n\n");
    for (k, n) in &by_status {
        s.push_str(&format!("- `{k}`: {n}\n"));
    }

    s.push_str("\n## Entries\n\n");
    s.push_str("| id | kind | status | path | logical | offset | stored | original | comp |\n");
    s.push_str("|---:|:-----|:-------|:-----|:--------|------:|------:|--------:|:-----|\n");
    for x in inv {
        s.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | {} | {} | {} | `{}` |\n",
            x.id_hex,
            x.kind.name(),
            status_name(x.status),
            x.path.as_deref().unwrap_or("-"),
            xical.as_deref().unwrap_or("-"),
            x.offset,
            x.stored_size,
            x.original_size,
            x.compression
        ));
    }

    std::fs::write(path, s)?;
    Ok(())
}

fn write_report_json(path: &Path, input: &Path, r: &MffReader<std::fs::File>, inv: &[InventoryRow]) -> Result<(), DecompileError> {
    // std-only JSON encoder (minimal)
    let mut out = String::new();
    out.push('{');

    // header
    push_kv_str(&mut out, "schema", "steel.decompile.report"); out.push(',');
    push_kv_str(&mut out, "input", &input.display().to_string()); out.push(',');
    push_kv_str(&mut out, "version", &format!("{:?}", r.header.version)); out.push(',');
    push_kv_str(&mut out, "endian", &format!("{:?}", r.header.endian)); out.push(',');
    push_kv_u64(&mut out, "toc_offset", r.header.toc_offset); out.push(',');
    push_kv_u64(&mut out, "toc_size", r.header.toc_size); out.push(',');
    push_kv_u64(&mut out, "entries", inv.len() as u64); out.push(',');

    out.push_str("\"items\":[");
    for (i, x) in inv.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push('{');
        push_kv_str(&mut out, "id", &x.id_hex); out.push(',');
        push_kv_str(&mut out, "kind", x.kind.name()); out.push(',');
        push_kv_str(&mut out, "status", status_name(x.status)); out.push(',');
        push_kv_opt_str(&mut out, "path", x.path.as_deref()); out.push(',');
        push_kv_opt_str(&mut out, "logical", xical.as_deref()); out.push(',');
        push_kv_u64(&mut out, "offset", x.offset); out.push(',');
        push_kv_u64(&mut out, "stored_size", x.stored_size); out.push(',');
        push_kv_u64(&mut out, "original_size", x.original_size); out.push(',');
        push_kv_str(&mut out, "compression", &x.compression);
        out.push('}');
    }
    out.push_str("]}");

    std::fs::write(path, out)?;
    Ok(())
}

fn status_name(s: InventoryStatus) -> &'static str {
    match s {
        InventoryStatus::Extracted => "extracted",
        InventoryStatus::Exists => "exists",
        InventoryStatus::Skipped => "skipped",
        InventoryStatus::Denied => "denied",
        InventoryStatus::Error => "error",
    }
}

/* ------------------------------ JSON helpers ----------------------------- */

fn push_kv_str(out: &mut String, k: &str, v: &str) {
    push_str(out, k);
    out.push(':');
    push_str(out, v);
}

fn push_kv_opt_str(out: &mut String, k: &str, v: Option<&str>) {
    push_str(out, k);
    out.push(':');
    match v {
        Some(s) => push_str(out, s),
        None => out.push_str("null"),
    }
}

fn push_kv_u64(out: &mut String, k: &str, v: u64) {
    push_str(out, k);
    out.push(':');
    out.push_str(&v.to_string());
}

fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_logical() {
        let s = sanitize_logical_name("toolchain:clang@16.0.1").unwrap();
        assert!(s.contains("toolchain:clang"));
    }

    #[test]
    fn bundle_path_sanitize_rejects_parent() {
        let r = sanitize_bundle_rel_path("../x", true);
        assert!(r.is_err());
    }
}
