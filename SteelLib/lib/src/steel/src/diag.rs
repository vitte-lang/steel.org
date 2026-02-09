//! Diagnostics (errors / warnings / notes) for Steel (mcfg).
//!
//! Objectifs :
//! - Modèle stable (Severity + Diagnostic + Labels + Span).
//! - Indépendant (std uniquement).
//! - Rendu texte (humain) + JSON (tooling).
//! - Déterministe (tri stable, collections ordonnées).
//!
//! Hypothèses :
//! - `file_id` référence une entrée dans `SourceMap`.
//! - `Span` = offsets en octets (byte offsets) dans le fichier.
//! - Si `text` n'est pas disponible, on rend au minimum: chemin + offsets.
//
// NOTE: Si vous avez besoin de colonnes unicode (graphemes), calculez les colonnes
// dans le lexer, ou remplacez le calcul byte->col par une logique UTF-8/width.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// -------------------------------
/// Span / positions
/// -------------------------------

/// Byte span dans un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[inline]
    pub fn new(file_id: u32, start: u32, end: u32) -> Self {
        Self { file_id, start, end }
    }

    #[inline]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn merge(a: Span, b: Span) -> Span {
        if a.file_id != b.file_id {
            // Éviter les merges cross-file.
            return a;
        }
        Span {
            file_id: a.file_id,
            start: a.start.min(b.start),
            end: a.end.max(b.end),
        }
    }
}

/// Position 1-based (ligne/colonne).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Position résolue (start/end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    pub start: LineCol,
    pub end: LineCol,
}

/// -------------------------------
/// Diagnostic model
/// -------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Help => "help",
        }
    }

    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelStyle {
    Primary,
    Secondary,
}

/// Label sur un span (primary = location principale, secondary = contexte).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub style: LabelStyle,
    pub message: Option<String>,
}

impl Label {
    pub fn primary(span: Span) -> Self {
        Self { span, style: LabelStyle::Primary, message: None }
    }

    pub fn secondary(span: Span) -> Self {
        Self { span, style: LabelStyle::Secondary, message: None }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// Code diag (ex: MUF0001).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagCode(pub String);

impl DiagCode {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Diagnostic complet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<DiagCode>,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    pub fn note(message: impl Into<String>) -> Self {
        Self::new(Severity::Note, message)
    }

    pub fn help(message: impl Into<String>) -> Self {
        Self::new(Severity::Help, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(DiagCode::new(code));
        self
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn primary_span(&self) -> Option<Span> {
        self.labels
            .iter()
            .find(|l| matches!(l.style, LabelStyle::Primary))
            .map(|l| l.span)
    }
}

/// Helpers “fast constructors”.
pub fn err_at(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg).with_label(Label::primary(span))
}

pub fn warn_at(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::warning(msg).with_label(Label::primary(span))
}

/// -------------------------------
/// Source files / SourceMap
/// -------------------------------

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: u32,
    pub path: PathBuf,
    /// Texte complet UTF-8 (optionnel).
    pub text: Option<String>,
    /// Offsets (bytes) du début de chaque ligne (0-based), pré-calculés.
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(id: u32, path: impl Into<PathBuf>, text: Option<String>) -> Self {
        let mut sf = Self { id, path: path.into(), text, line_starts: Vec::new() };
        sf.recompute_lines();
        sf
    }

    pub fn set_text(&mut self, text: String) {
        self.text = Some(text);
        self.recompute_lines();
    }

    pub fn has_text(&self) -> bool {
        self.text.is_some()
    }

    fn recompute_lines(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);

        let Some(t) = &self.text else { return; };
        let bytes = t.as_bytes();

        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                let next = i + 1;
                if next <= u32::MAX as usize {
                    self.line_starts.push(next as u32);
                }
            }
            i += 1;
        }
    }

    /// Convertit un offset byte en (ligne, col) 1-based.
    pub fn byte_to_line_col(&self, offset: u32) -> LineCol {
        if self.text.is_none() {
            return LineCol { line: 1, col: 1 };
        }

        let idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };

        let line_start = self.line_starts.get(idx).copied().unwrap_or(0);
        let line = (idx as u32).saturating_add(1);
        let col = offset.saturating_sub(line_start).saturating_add(1);

        LineCol { line, col }
    }

    pub fn span_to_position(&self, span: Span) -> Position {
        let start = self.byte_to_line_col(span.start);
        let end = self.byte_to_line_col(span.end);
        Position { start, end }
    }

    /// Retourne le texte d'une ligne 1-based (sans le '\n').
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let text = self.text.as_ref()?;
        let li = (line as usize).checked_sub(1)?;

        let start = *self.line_starts.get(li)? as usize;
        let end = if li + 1 < self.line_starts.len() {
            (self.line_starts[li + 1] as usize).saturating_sub(1)
        } else {
            text.len()
        };

        Some(&text[start..end.min(text.len())])
    }
}

#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: BTreeMap<u32, SourceFile>,
    next_id: u32,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: Option<String>) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.files.insert(id, SourceFile::new(id, path, text));
        id
    }

    pub fn insert_file(&mut self, id: u32, path: impl Into<PathBuf>, text: Option<String>) {
        self.files.insert(id, SourceFile::new(id, path, text));
        self.next_id = self.next_id.max(id.saturating_add(1));
    }

    pub fn get(&self, file_id: u32) -> Option<&SourceFile> {
        self.files.get(&file_id)
    }

    pub fn get_mut(&mut self, file_id: u32) -> Option<&mut SourceFile> {
        self.files.get_mut(&file_id)
    }

    pub fn path(&self, file_id: u32) -> Option<&Path> {
        self.files.get(&file_id).map(|f| f.path.as_path())
    }
}

/// -------------------------------
/// Bag (accumulateur)
/// -------------------------------

#[derive(Debug, Default, Clone)]
pub struct DiagBag {
    diags: Vec<Diagnostic>,
    pub warnings_as_errors: bool,
}

impl DiagBag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, mut d: Diagnostic) {
        if self.warnings_as_errors && matches!(d.severity, Severity::Warning) {
            d.severity = Severity::Error;
        }
        self.diags.push(d);
    }

    pub fn extend(&mut self, ds: impl IntoIterator<Item = Diagnostic>) {
        for d in ds {
            self.push(d);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diags.len()
    }

    pub fn has_error(&self) -> bool {
        self.diags.iter().any(|d| d.severity.is_error())
    }

    pub fn all(&self) -> &[Diagnostic] {
        &self.diags
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diags
    }

    /// Tri déterministe (file/start, severity, code, message).
    pub fn sort_deterministic(&mut self) {
        self.diags.sort_by(|a, b| {
            let (af, ap) = a.primary_span().map(|s| (s.file_id, s.start)).unwrap_or((u32::MAX, u32::MAX));
            let (bf, bp) = b.primary_span().map(|s| (s.file_id, s.start)).unwrap_or((u32::MAX, u32::MAX));
            let ac = a.code.as_ref().map(|c| c.0.as_str()).unwrap_or("");
            let bc = b.code.as_ref().map(|c| c.0.as_str()).unwrap_or("");
            (af, ap, a.severity.as_str(), ac, a.message.as_str())
                .cmp(&(bf, bp, b.severity.as_str(), bc, b.message.as_str()))
        });
    }
}

/// -------------------------------
/// Rendu texte
/// -------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub color: Color,
    /// lignes de contexte autour de la ligne primaire
    pub context: u32,
    pub show_secondary: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { color: Color::Auto, context: 1, show_secondary: true }
    }
}

pub fn render_to_stderr(bag: &DiagBag, sm: &SourceMap, opts: &RenderOptions) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    render(bag, sm, opts, &mut stderr)
}

pub fn render<W: Write>(bag: &DiagBag, sm: &SourceMap, opts: &RenderOptions, out: &mut W) -> io::Result<()> {
    let use_color = match opts.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => supports_color(),
    };

    for (i, d) in bag.all().iter().enumerate() {
        if i != 0 {
            writeln!(out)?;
        }
        render_one(d, sm, opts, use_color, out)?;
    }
    Ok(())
}

fn render_one<W: Write>(
    d: &Diagnostic,
    sm: &SourceMap,
    opts: &RenderOptions,
    color: bool,
    out: &mut W,
) -> io::Result<()> {
    let sev = d.severity.as_str();
    let sev_col = match d.severity {
        Severity::Error => Ansi::Red,
        Severity::Warning => Ansi::Yellow,
        Severity::Note => Ansi::Cyan,
        Severity::Help => Ansi::Green,
    };

    if color {
        write!(out, "{}{}{}", sev_col.fg(), sev, Ansi::Reset.fg())?;
    } else {
        write!(out, "{sev}")?;
    }

    if let Some(code) = &d.code {
        write!(out, "[{}]", code.0)?;
    }

    writeln!(out, ": {}", d.message)?;

    // Primary
    let primary = d.labels.iter().find(|l| matches!(l.style, LabelStyle::Primary));
    if let Some(lbl) = primary {
        render_location(lbl, sm, opts, color, out)?;
    } else if let Some(lbl) = d.labels.first() {
        let path = sm.path(lbl.span.file_id).map(|p| p.display().to_string());
        if let Some(p) = path {
            writeln!(out, "  --> {p}")?;
        }
    }

    // Secondary
    if opts.show_secondary {
        for lbl in d.labels.iter().filter(|l| matches!(l.style, LabelStyle::Secondary)) {
            render_secondary(lbl, sm, color, out)?;
        }
    }

    for n in &d.notes {
        writeln!(out, "  note: {n}")?;
    }
    for h in &d.help {
        writeln!(out, "  help: {h}")?;
    }

    Ok(())
}

fn render_location<W: Write>(
    lbl: &Label,
    sm: &SourceMap,
    opts: &RenderOptions,
    color: bool,
    out: &mut W,
) -> io::Result<()> {
    let Some(sf) = sm.get(lbl.span.file_id) else {
        writeln!(out, "  --> <file:{}>:{}..{}", lbl.span.file_id, lbl.span.start, lbl.span.end)?;
        return Ok(());
    };

    if sf.has_text() {
        let pos = sf.span_to_position(lbl.span);
        writeln!(out, "  --> {}:{}:{}", sf.path.display(), pos.start.line, pos.start.col)?;

        let line = pos.start.line;
        let ctx = opts.context;
        let from = line.saturating_sub(ctx);
        let to = line.saturating_add(ctx);

        for l in from..=to {
            if let Some(txt) = sf.line_text(l) {
                writeln!(out, "{:>4} | {}", l, txt)?;

                if l == line {
                    let underline = build_underline(txt, pos.start.col, pos.end.col.max(pos.start.col));
                    if color {
                        let c = Ansi::Red;
                        writeln!(out, "     | {}{}{}", c.fg(), underline, Ansi::Reset.fg())?;
                    } else {
                        writeln!(out, "     | {underline}")?;
                    }

                    if let Some(m) = &lbl.message {
                        writeln!(out, "     = {m}")?;
                    }
                }
            }
        }
    } else {
        writeln!(out, "  --> {}:{}..{}", sf.path.display(), lbl.span.start, lbl.span.end)?;
        if let Some(m) = &lbl.message {
            writeln!(out, "     = {m}")?;
        }
    }

    Ok(())
}

fn render_secondary<W: Write>(lbl: &Label, sm: &SourceMap, color: bool, out: &mut W) -> io::Result<()> {
    let path = sm
        .path(lbl.span.file_id)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("<file:{}>", lbl.span.file_id));

    let msg = lbl.message.as_deref().unwrap_or("related");

    if color {
        writeln!(
            out,
            "  {}-->{} {}:{}..{} ({})",
            Ansi::Blue.fg(),
            Ansi::Reset.fg(),
            path,
            lbl.span.start,
            lbl.span.end,
            msg
        )?;
    } else {
        writeln!(out, "  --> {}:{}..{} ({})", path, lbl.span.start, lbl.span.end, msg)?;
    }

    Ok(())
}

fn build_underline(line: &str, col_start_1: u32, col_end_1: u32) -> String {
    // Colonnes bytes (simple). Limite la taille pour éviter des sorties énormes.
    let max = line.as_bytes().len().min(512);
    let s = col_start_1.saturating_sub(1) as usize;
    let e = col_end_1.saturating_sub(1) as usize;

    let mut out = String::new();
    for i in 0..max {
        if i < s {
            out.push(' ');
        } else if i <= e {
            out.push('^');
        }
    }
    if out.is_empty() {
        out.push('^');
    }
    out
}

/// -------------------------------
/// JSON output (tooling)
/// -------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonMode {
    Compact,
    Pretty,
}

pub fn to_json(bag: &DiagBag, sm: &SourceMap, mode: JsonMode) -> String {
    let mut s = String::new();

    match mode {
        JsonMode::Compact => s.push('{'),
        JsonMode::Pretty => s.push_str("{\n  "),
    }

    push_json_key(&mut s, "diagnostics", mode);
    match mode {
        JsonMode::Compact => s.push('['),
        JsonMode::Pretty => s.push_str("[\n"),
    }

    for (i, d) in bag.all().iter().enumerate() {
        if i != 0 {
            match mode {
                JsonMode::Compact => s.push(','),
                JsonMode::Pretty => s.push_str(",\n"),
            }
        }
        push_diag_json(&mut s, d, sm, mode, if matches!(mode, JsonMode::Pretty) { 4 } else { 0 });
    }

    match mode {
        JsonMode::Compact => s.push(']'),
        JsonMode::Pretty => s.push_str("\n  ]"),
    }

    match mode {
        JsonMode::Compact => s.push('}'),
        JsonMode::Pretty => s.push_str("\n}\n"),
    }

    s
}

fn push_diag_json(s: &mut String, d: &Diagnostic, sm: &SourceMap, mode: JsonMode, indent: usize) {
    let ind = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent) } else { String::new() };
    let ind2 = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent + 2) } else { String::new() };

    match mode {
        JsonMode::Compact => s.push('{'),
        JsonMode::Pretty => s.push_str(&format!("{ind}{{\n")),
    }

    json_kv_str(s, "severity", d.severity.as_str(), mode, &ind2);
    json_comma(s, mode);

    if let Some(code) = &d.code {
        json_kv_str(s, "code", &code.0, mode, &ind2);
    } else {
        json_kv_null(s, "code", mode, &ind2);
    }
    json_comma(s, mode);

    json_kv_str(s, "message", &d.message, mode, &ind2);
    json_comma(s, mode);

    // labels
    json_key(s, "labels", mode, &ind2);
    match mode {
        JsonMode::Compact => s.push('['),
        JsonMode::Pretty => s.push_str("[\n"),
    }

    for (i, l) in d.labels.iter().enumerate() {
        if i != 0 {
            match mode {
                JsonMode::Compact => s.push(','),
                JsonMode::Pretty => s.push_str(",\n"),
            }
        }
        push_label_json(s, l, sm, mode, indent + 4);
    }

    match mode {
        JsonMode::Compact => s.push(']'),
        JsonMode::Pretty => s.push_str(&format!("\n{}]", " ".repeat(indent + 2))),
    }
    json_comma(s, mode);

    // notes
    json_key(s, "notes", mode, &ind2);
    push_string_array(s, &d.notes, mode, indent + 2);
    json_comma(s, mode);

    // help
    json_key(s, "help", mode, &ind2);
    push_string_array(s, &d.help, mode, indent + 2);

    match mode {
        JsonMode::Compact => s.push('}'),
        JsonMode::Pretty => s.push_str(&format!("\n{ind}}}")),
    }
}

fn push_label_json(s: &mut String, l: &Label, sm: &SourceMap, mode: JsonMode, indent: usize) {
    let ind = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent) } else { String::new() };
    let ind2 = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent + 2) } else { String::new() };

    match mode {
        JsonMode::Compact => s.push('{'),
        JsonMode::Pretty => s.push_str(&format!("{ind}{{\n")),
    }

    let style = match l.style {
        LabelStyle::Primary => "primary",
        LabelStyle::Secondary => "secondary",
    };

    json_kv_str(s, "style", style, mode, &ind2);
    json_comma(s, mode);

    let file = sm
        .path(l.span.file_id)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("<file:{}>", l.span.file_id));
    json_kv_str(s, "file", &file, mode, &ind2);
    json_comma(s, mode);

    // span
    json_key(s, "span", mode, &ind2);
    match mode {
        JsonMode::Compact => s.push('{'),
        JsonMode::Pretty => s.push_str("{\n"),
    }
    let ind3 = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent + 4) } else { String::new() };
    json_kv_u32(s, "start", l.span.start, mode, &ind3);
    json_comma(s, mode);
    json_kv_u32(s, "end", l.span.end, mode, &ind3);

    match mode {
        JsonMode::Compact => s.push('}'),
        JsonMode::Pretty => s.push_str(&format!("\n{}{}", " ".repeat(indent + 2), "}")),
    }
    json_comma(s, mode);

    // pos (si texte présent)
    if let Some(sf) = sm.get(l.span.file_id) {
        if sf.has_text() {
            let pos = sf.span_to_position(l.span);
            json_key(s, "pos", mode, &ind2);
            match mode {
                JsonMode::Compact => s.push('{'),
                JsonMode::Pretty => s.push_str("{\n"),
            }
            let ind3 = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent + 4) } else { String::new() };
            json_key(s, "start", mode, &ind3);
            push_linecol_json(s, pos.start, mode, indent + 6);
            json_comma(s, mode);
            json_key(s, "end", mode, &ind3);
            push_linecol_json(s, pos.end, mode, indent + 6);

            match mode {
                JsonMode::Compact => s.push('}'),
                JsonMode::Pretty => s.push_str(&format!("\n{}{}", " ".repeat(indent + 2), "}")),
            }
            json_comma(s, mode);
        }
    }

    // message
    if let Some(m) = &l.message {
        json_kv_str(s, "message", m, mode, &ind2);
    } else {
        json_kv_null(s, "message", mode, &ind2);
    }

    match mode {
        JsonMode::Compact => s.push('}'),
        JsonMode::Pretty => s.push_str(&format!("\n{ind}}}")),
    }
}

fn push_linecol_json(s: &mut String, lc: LineCol, mode: JsonMode, indent: usize) {
    let ind = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent) } else { String::new() };
    match mode {
        JsonMode::Compact => s.push('{'),
        JsonMode::Pretty => s.push_str(&format!("{ind}{{\n")),
    }

    let ind2 = if matches!(mode, JsonMode::Pretty) { " ".repeat(indent + 2) } else { String::new() };
    json_kv_u32(s, "line", lc.line, mode, &ind2);
    json_comma(s, mode);
    json_kv_u32(s, "col", lc.col, mode, &ind2);

    match mode {
        JsonMode::Compact => s.push('}'),
        JsonMode::Pretty => s.push_str(&format!("\n{ind}}}")),
    }
}

fn push_string_array(s: &mut String, xs: &[String], mode: JsonMode, indent: usize) {
    match mode {
        JsonMode::Compact => {
            s.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i != 0 {
                    s.push(',');
                }
                s.push('"');
                s.push_str(&escape_json(x));
                s.push('"');
            }
            s.push(']');
        }
        JsonMode::Pretty => {
            let ind = " ".repeat(indent);
            let ind2 = " ".repeat(indent + 2);
            s.push_str("[\n");
            for (i, x) in xs.iter().enumerate() {
                if i != 0 {
                    s.push_str(",\n");
                }
                s.push_str(&ind2);
                s.push('"');
                s.push_str(&escape_json(x));
                s.push('"');
            }
            if !xs.is_empty() {
                s.push('\n');
            }
            s.push_str(&ind);
            s.push(']');
        }
    }
}

fn push_json_key(s: &mut String, key: &str, mode: JsonMode) {
    match mode {
        JsonMode::Compact => {
            s.push('"');
            s.push_str(key);
            s.push_str("\":[");
        }
        JsonMode::Pretty => {
            s.push('"');
            s.push_str(key);
            s.push_str("\": ");
        }
    }
}

fn json_key(s: &mut String, key: &str, mode: JsonMode, indent: &str) {
    match mode {
        JsonMode::Compact => {
            s.push('"');
            s.push_str(key);
            s.push_str("\":");
        }
        JsonMode::Pretty => {
            s.push_str(indent);
            s.push('"');
            s.push_str(key);
            s.push_str("\": ");
        }
    }
}

fn json_kv_str(s: &mut String, key: &str, val: &str, mode: JsonMode, indent: &str) {
    json_key(s, key, mode, indent);
    s.push('"');
    s.push_str(&escape_json(val));
    s.push('"');
}

fn json_kv_u32(s: &mut String, key: &str, val: u32, mode: JsonMode, indent: &str) {
    json_key(s, key, mode, indent);
    s.push_str(&val.to_string());
}

fn json_kv_null(s: &mut String, key: &str, mode: JsonMode, indent: &str) {
    json_key(s, key, mode, indent);
    s.push_str("null");
}

fn json_comma(s: &mut String, mode: JsonMode) {
    match mode {
        JsonMode::Compact => s.push(','),
        JsonMode::Pretty => s.push_str(",\n"),
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// -------------------------------
/// ANSI helpers
/// -------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ansi {
    Reset,
    Red,
    Yellow,
    Green,
    Blue,
    Cyan,
}

impl Ansi {
    fn fg(self) -> &'static str {
        match self {
            Ansi::Reset => "\x1b[0m",
            Ansi::Red => "\x1b[31m",
            Ansi::Yellow => "\x1b[33m",
            Ansi::Green => "\x1b[32m",
            Ansi::Blue => "\x1b[34m",
            Ansi::Cyan => "\x1b[36m",
        }
    }
}

fn supports_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(term) = std::env::var("TERM") {
        if term == "dumb" {
            return false;
        }
    }
    true
}

/// -------------------------------
/// Display helpers (optional)
/// -------------------------------

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge_same_file() {
        let a = Span::new(1, 10, 12);
        let b = Span::new(1, 5, 20);
        let m = Span::merge(a, b);
        assert_eq!(m.file_id, 1);
        assert_eq!(m.start, 5);
        assert_eq!(m.end, 20);
    }

    #[test]
    fn byte_to_line_col_smoke() {
        let mut sm = SourceMap::new();
        let fid = sm.add_file("t.muf", Some("ab\ncd\nef".to_string()));
        let sf = sm.get(fid).unwrap();

        assert_eq!(sf.byte_to_line_col(0), LineCol { line: 1, col: 1 });
        assert_eq!(sf.byte_to_line_col(2), LineCol { line: 1, col: 3 });
        assert_eq!(sf.byte_to_line_col(3), LineCol { line: 2, col: 1 }); // after '\n'
    }

    #[test]
    fn bag_sort_deterministic() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("a.muf", Some("x\n".to_string()));

        let d1 = Diagnostic::error("e1").with_label(Label::primary(Span::new(f, 0, 1)));
        let d2 = Diagnostic::error("e2").with_label(Label::primary(Span::new(f, 0, 1)));

        let mut bag = DiagBag::new();
        bag.push(d2);
        bag.push(d1);
        bag.sort_deterministic();

        assert_eq!(bag.all()[0].message, "e1");
        assert_eq!(bag.all()[1].message, "e2");
    }

    #[test]
    fn json_smoke() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("a.muf", Some("hello\n".to_string()));

        let mut bag = DiagBag::new();
        bag.push(
            Diagnostic::error("boom")
                .with_code("MUF0001")
                .with_label(Label::primary(Span::new(f, 0, 5)).with_message("here"))
                .with_note("n")
                .with_help("h"),
        );

        let j = to_json(&bag, &sm, JsonMode::Compact);
        assert!(j.contains("\"diagnostics\""));
        assert!(j.contains("MUF0001"));
        assert!(j.contains("boom"));
    }
}