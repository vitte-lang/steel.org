//! warning.rs
//!
//! Système de diagnostics (warnings/notes/aide) pour Steel.
//!
//! Objectifs:
//! - Représentation stable: code + sévérité + message + spans + hints
//! - Rendu lisible style “compiler” (fichier:ligne:col + excerpt + caret)
//! - Mode “plain” (CI/log) et mode “ansi” (terminal)
//! - Sink extensible (stderr, buffer, JSON plus tard si besoin)
//!
//! Dépendances: std uniquement.

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Sévérité d’un diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Note,
    Help,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Note => "note",
            Severity::Help => "help",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }
}

/// Identifiant de diagnostic (style rustc: E0xxx / W0xxx / M0xxx).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagCode(Cow<'static, str>);

impl DiagCode {
    pub fn new<S: Into<Cow<'static, str>>>(s: S) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiagCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Position dans un fichier (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Intervalle dans un fichier (positions 1-based, inclusif → exclusif logique).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: PathBuf,
    pub start: Position,
    pub end: Position,
    /// Offsets bytes optionnels si vous avez une source bufferisée.
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    /// Label optionnel affiché sur l’annotation.
    pub label: Option<String>,
}

impl Span {
    pub fn new<P: Into<PathBuf>>(file: P, start: Position, end: Position) -> Self {
        Self {
            file: file.into(),
            start,
            end,
            start_byte: None,
            end_byte: None,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_bytes(mut self, start: usize, end: usize) -> Self {
        self.start_byte = Some(start);
        self.end_byte = Some(end);
        self
    }
}

/// Un “fix” potentiel (message + remplacement optionnel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixIt {
    pub message: String,
    pub replacement: Option<String>,
}

impl FixIt {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            replacement: None,
        }
    }

    pub fn with_replacement(mut self, replacement: impl Into<String>) -> Self {
        self.replacement = Some(replacement.into());
        self
    }
}

/// Diagnostic complet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<DiagCode>,
    pub title: String,
    pub message: Option<String>,
    pub spans: Vec<Span>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
    pub fixes: Vec<FixIt>,
    /// Catégorie interne (ex: "parser", "config", "runner") utile pour filtrer.
    pub category: Option<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, title: impl Into<String>) -> Self {
        Self {
            severity,
            code: None,
            title: title.into(),
            message: None,
            spans: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
            fixes: Vec::new(),
            category: None,
        }
    }

    pub fn warning(title: impl Into<String>) -> Self {
        Self::new(Severity::Warning, title)
    }

    pub fn error(title: impl Into<String>) -> Self {
        Self::new(Severity::Error, title)
    }

    pub fn note(title: impl Into<String>) -> Self {
        Self::new(Severity::Note, title)
    }

    pub fn help(title: impl Into<String>) -> Self {
        Self::new(Severity::Help, title)
    }

    pub fn with_code(mut self, code: impl Into<DiagCode>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    pub fn push_span(&mut self, span: Span) {
        self.spans.push(span);
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.spans.push(span);
        self
    }

    pub fn push_note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn push_help(&mut self, help: impl Into<String>) {
        self.help.push(help.into());
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn push_fix(&mut self, fix: FixIt) {
        self.fixes.push(fix);
    }

    pub fn with_fix(mut self, fix: FixIt) -> Self {
        self.fixes.push(fix);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }
}

/// Politique d’affichage (couleur + contexte + format).
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub color: ColorMode,
    pub context_lines: usize,
    pub show_category: bool,
    pub show_fixes: bool,
    pub compact: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            color: ColorMode::Auto,
            context_lines: 2,
            show_category: false,
            show_fixes: true,
            compact: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    fn enabled(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

/// Source text provider: permet de récupérer le contenu d’un fichier (ou d’un buffer).
pub trait SourceProvider {
    fn get(&self, path: &Path) -> io::Result<Cow<'_, str>>;
}

/// Provider par défaut: lit depuis le disque.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsSourceProvider;

impl SourceProvider for FsSourceProvider {
    fn get(&self, path: &Path) -> io::Result<Cow<'_, str>> {
        let bytes = std::fs::read(path)?;
        let s = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Cow::Owned(s))
    }
}

/// Collecteur/émetteur de diagnostics.
pub trait DiagnosticSink {
    fn emit(&mut self, diag: Diagnostic);
}

/// Sink en mémoire (tests / intégration).
#[derive(Debug, Default)]
pub struct VecSink {
    pub diags: Vec<Diagnostic>,
}

impl DiagnosticSink for VecSink {
    fn emit(&mut self, diag: Diagnostic) {
        self.diags.push(diag);
    }
}

impl VecSink {
    pub fn take(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diags)
    }
}

/// Sink stderr avec rendu.
pub struct StderrSink<P: SourceProvider = FsSourceProvider> {
    provider: P,
    opts: RenderOptions,
}

impl<P: SourceProvider> StderrSink<P> {
    pub fn new(provider: P, opts: RenderOptions) -> Self {
        Self { provider, opts }
    }
}

impl StderrSink<FsSourceProvider> {
    pub fn with_default_fs(opts: RenderOptions) -> Self {
        Self {
            provider: FsSourceProvider,
            opts,
        }
    }
}

impl<P: SourceProvider> DiagnosticSink for StderrSink<P> {
    fn emit(&mut self, diag: Diagnostic) {
        let mut w = io::stderr().lock();
        let _ = render_to(&mut w, &self.provider, &diag, &self.opts);
        let _ = writeln!(w);
    }
}

/// Batch + stats (utile pour “build steel”).
#[derive(Debug, Default)]
pub struct DiagReport {
    pub warnings: usize,
    pub errors: usize,
    pub notes: usize,
    pub help: usize,
}

impl DiagReport {
    pub fn add(&mut self, d: &Diagnostic) {
        match d.severity {
            Severity::Warning => self.warnings += 1,
            Severity::Error => self.errors += 1,
            Severity::Note => self.notes += 1,
            Severity::Help => self.help += 1,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }
}

/// Hub central: collecte et émet (fanout possible plus tard).
pub struct Diagnostics<S: DiagnosticSink> {
    sink: S,
    pub report: DiagReport,
    pub fail_fast: bool,
}

impl<S: DiagnosticSink> Diagnostics<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            report: DiagReport::default(),
            fail_fast: false,
        }
    }

    pub fn with_fail_fast(mut self, enabled: bool) -> Self {
        self.fail_fast = enabled;
        self
    }

    pub fn emit(&mut self, diag: Diagnostic) -> Result<(), BuildAbort> {
        self.report.add(&diag);
        let is_error = diag.is_error();
        self.sink.emit(diag);
        if self.fail_fast && is_error {
            return Err(BuildAbort);
        }
        Ok(())
    }

    pub fn warning(&mut self, diag: Diagnostic) -> Result<(), BuildAbort> {
        self.emit(diag)
    }

    pub fn error(&mut self, diag: Diagnostic) -> Result<(), BuildAbort> {
        self.emit(diag)
    }

    pub fn into_sink(self) -> S {
        self.sink
    }
}

/// Erreur sentinelle quand `fail_fast` est activé.
#[derive(Debug, Clone, Copy)]
pub struct BuildAbort;

impl fmt::Display for BuildAbort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("build aborted (fail-fast)")
    }
}

impl std::error::Error for BuildAbort {}

/// Helpers “ergonomiques”.
pub fn w(code: impl Into<DiagCode>, title: impl Into<String>) -> Diagnostic {
    Diagnostic::warning(title).with_code(code)
}
pub fn e(code: impl Into<DiagCode>, title: impl Into<String>) -> Diagnostic {
    Diagnostic::error(title).with_code(code)
}
pub fn n(title: impl Into<String>) -> Diagnostic {
    Diagnostic::note(title)
}
pub fn h(title: impl Into<String>) -> Diagnostic {
    Diagnostic::help(title)
}

/// Macro pratique.
#[macro_export]
macro_rules! diag_span {
    ($file:expr, $sl:expr, $sc:expr, $el:expr, $ec:expr) => {
        $crate::warning::Span::new(
            std::path::PathBuf::from($file),
            $crate::warning::Position::new($sl, $sc),
            $crate::warning::Position::new($el, $ec),
        )
    };
}

#[macro_export]
macro_rules! warn {
    ($code:expr, $title:expr) => {
        $crate::warning::Diagnostic::warning($title).with_code($crate::warning::DiagCode::new($code))
    };
}

#[macro_export]
macro_rules! err {
    ($code:expr, $title:expr) => {
        $crate::warning::Diagnostic::error($title).with_code($crate::warning::DiagCode::new($code))
    };
}

/* =========================
 * Rendering (human-readable)
 * ========================= */

pub fn render_to<W: Write, P: SourceProvider>(
    mut out: W,
    provider: &P,
    diag: &Diagnostic,
    opts: &RenderOptions,
) -> io::Result<()> {
    let color = opts.color.enabled();

    // Header line: "warning[W0123]: title"
    if color {
        write!(
            out,
            "{}{}{}",
            style_sev_prefix(diag.severity),
            style_reset(),
            ""
        )?;
    }

    write!(out, "{}", diag.severity.as_str())?;

    if let Some(code) = &diag.code {
        write!(out, "[{}]", code)?;
    }
    if opts.show_category {
        if let Some(cat) = &diag.category {
            write!(out, "({})", cat)?;
        }
    }
    write!(out, ": {}", diag.title)?;

    if let Some(msg) = &diag.message {
        if !opts.compact {
            writeln!(out)?;
            writeln!(out, "  {}", msg)?;
        } else {
            write!(out, " — {}", msg)?;
            writeln!(out)?;
        }
    } else {
        writeln!(out)?;
    }

    // Spans
    for span in &diag.spans {
        render_span(&mut out, provider, span, diag.severity, opts)?;
    }

    // Notes
    for note in &diag.notes {
        render_kv_line(&mut out, "note", note, color)?;
    }

    // Help
    for help in &diag.help {
        render_kv_line(&mut out, "help", help, color)?;
    }

    // Fixes
    if opts.show_fixes {
        for fix in &diag.fixes {
            if let Some(rep) = &fix.replacement {
                render_kv_line(
                    &mut out,
                    "fix",
                    &format!("{} (replace with: {})", fix.message, rep),
                    color,
                )?;
            } else {
                render_kv_line(&mut out, "fix", &fix.message, color)?;
            }
        }
    }

    Ok(())
}

fn render_kv_line<W: Write>(out: &mut W, key: &str, val: &str, color: bool) -> io::Result<()> {
    if color {
        writeln!(out, "  {}{}:{} {}", style_dim(), key, style_reset(), val)
    } else {
        writeln!(out, "  {}: {}", key, val)
    }
}

fn render_span<W: Write, P: SourceProvider>(
    out: &mut W,
    provider: &P,
    span: &Span,
    sev: Severity,
    opts: &RenderOptions,
) -> io::Result<()> {
    // Location line: " --> file:line:col"
    let color = opts.color.enabled();
    if color {
        writeln!(
            out,
            "  {}-->{} {}:{}:{}",
            style_dim(),
            style_reset(),
            span.file.display(),
            span.start.line,
            span.start.col
        )?;
    } else {
        writeln!(
            out,
            "  --> {}:{}:{}",
            span.file.display(),
            span.start.line,
            span.start.col
        )?;
    }

    // Try to fetch source content
    let src = match provider.get(&span.file) {
        Ok(s) => s,
        Err(_) => {
            // Pas bloquant: on affiche juste la localisation
            if let Some(label) = &span.label {
                render_kv_line(out, "at", label, color)?;
            }
            return Ok(());
        }
    };

    let lines: Vec<&str> = src.lines().collect();
    if lines.is_empty() {
        return Ok(());
    }

    let line_idx = span.start.line.saturating_sub(1);
    if line_idx >= lines.len() {
        return Ok(());
    }

    // Context window
    let ctx = opts.context_lines;
    let start_line = line_idx.saturating_sub(ctx);
    let end_line = (line_idx + ctx).min(lines.len().saturating_sub(1));

    // Gutter width
    let gutter_w = (end_line + 1).to_string().len().max(2);

    for i in start_line..=end_line {
        let ln = i + 1;
        let text = lines[i];

        // line print
        if color {
            writeln!(
                out,
                "  {}{:>width$}{} | {}",
                style_dim(),
                ln,
                style_reset(),
                text,
                width = gutter_w
            )?;
        } else {
            writeln!(out, "  {:>width$} | {}", ln, text, width = gutter_w)?;
        }

        // caret line for the primary line only
        if i == line_idx {
            let caret = build_caret_line(text, span.start.col, span.end.col);
            let sev_style = if color { style_for_sev(sev) } else { "" };
            let reset = if color { style_reset() } else { "" };

            if let Some(label) = &span.label {
                writeln!(
                    out,
                    "  {:>width$} | {}{}{} {}",
                    "",
                    sev_style,
                    caret,
                    reset,
                    label,
                    width = gutter_w
                )?;
            } else {
                writeln!(
                    out,
                    "  {:>width$} | {}{}{}",
                    "",
                    sev_style,
                    caret,
                    reset,
                    width = gutter_w
                )?;
            }
        }
    }

    Ok(())
}

fn build_caret_line(line_text: &str, start_col_1: usize, end_col_1: usize) -> String {
    // Colonnes 1-based (UTF-8: approximation by char count, “compiler-grade” suffisant pour Steel).
    let start = start_col_1.saturating_sub(1);
    let end = end_col_1.saturating_sub(1).max(start + 1);

    let mut out = String::new();
    let mut col = 0usize;

    // Preserve tabs alignment: convert to single char offset (approx)
    for ch in line_text.chars() {
        if col >= start {
            break;
        }
        out.push(if ch == '\t' { '\t' } else { ' ' });
        col += 1;
    }

    // Mark range
    let len = end.saturating_sub(start).max(1);
    out.push('^');
    if len > 1 {
        for _ in 1..len {
            out.push('~');
        }
    }
    out
}

/* =========
 * ANSI
 * ========= */

fn style_reset() -> &'static str {
    "\x1b[0m"
}
fn style_dim() -> &'static str {
    "\x1b[2m"
}
fn style_yellow() -> &'static str {
    "\x1b[33m"
}
fn style_red() -> &'static str {
    "\x1b[31m"
}
fn style_blue() -> &'static str {
    "\x1b[34m"
}
fn style_magenta() -> &'static str {
    "\x1b[35m"
}

fn style_for_sev(sev: Severity) -> &'static str {
    match sev {
        Severity::Warning => style_yellow(),
        Severity::Error => style_red(),
        Severity::Note => style_blue(),
        Severity::Help => style_magenta(),
    }
}

fn style_sev_prefix(sev: Severity) -> &'static str {
    // Bold + color for the severity token
    match sev {
        Severity::Warning => "\x1b[1m\x1b[33m",
        Severity::Error => "\x1b[1m\x1b[31m",
        Severity::Note => "\x1b[1m\x1b[34m",
        Severity::Help => "\x1b[1m\x1b[35m",
    }
}

/* =====================
 * Tests (std only)
 * ===================== */

#[cfg(test)]
mod tests {
    use super::*;

    struct MemProvider {
        path: PathBuf,
        content: String,
    }

    impl SourceProvider for MemProvider {
        fn get(&self, path: &Path) -> io::Result<Cow<'_, str>> {
            if path == self.path.as_path() {
                Ok(Cow::Borrowed(self.content.as_str()))
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "not found"))
            }
        }
    }

    #[test]
    fn caret_line_basic() {
        let s = "hello world";
        let c = build_caret_line(s, 7, 12);
        // "world" ~ range, 6 spaces then ^~~~~
        assert!(c.contains("^"));
        assert!(c.contains("~"));
    }

    #[test]
    fn render_smoke() {
        let file = PathBuf::from("Memfile.muf");
        let provider = MemProvider {
            path: file.clone(),
            content: "a = 1\nb = 2\nc = 3\n".into(),
        };

        let diag = Diagnostic::warning("unused variable")
            .with_code(DiagCode::new("W0001"))
            .with_message("`b` is assigned but never read")
            .with_span(
                Span::new(
                    file.clone(),
                    Position::new(2, 1),
                    Position::new(2, 2),
                )
                .with_label("remove or use `b`"),
            )
            .with_note("this will be an error in strict mode");

        let mut buf = Vec::new();
        let mut opts = RenderOptions::default();
        opts.color = ColorMode::Never;
        render_to(&mut buf, &provider, &diag, &opts).unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("warning[W0001]"));
        assert!(out.contains("Memfile.muf:2:1"));
        assert!(out.contains("remove or use `b`"));
        assert!(out.contains("note:"));
    }

    #[test]
    fn report_counts() {
        let mut r = DiagReport::default();
        r.add(&Diagnostic::warning("w"));
        r.add(&Diagnostic::error("e"));
        r.add(&Diagnostic::note("n"));
        r.add(&Diagnostic::help("h"));
        assert_eq!(r.warnings, 1);
        assert_eq!(r.errors, 1);
        assert_eq!(r.notes, 1);
        assert_eq!(r.help, 1);
        assert!(r.has_errors());
    }

    #[test]
    fn fail_fast_aborts_on_error() {
        let mut d = Diagnostics::new(VecSink::default()).with_fail_fast(true);
        d.emit(Diagnostic::warning("w")).unwrap();
        let err = d.emit(Diagnostic::error("e")).unwrap_err();
        let _ = err; // just ensure it triggers
    }
}
