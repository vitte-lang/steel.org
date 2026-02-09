//! Diagnostics core types (Span, Label, Diag) + render helpers.
//!
//! This module provides:
//! - `Span` and `SourceId` for stable source references
//! - `Label` to attach highlights to a span
//! - `Diag` as a structured diagnostic (code + severity + message + notes)
//! - `DiagBag` to accumulate diags
//! - minimal renderers (plain text) without external deps
//!
//! The stable code registry lives in `codes.rs`.

use core::fmt;
use std::collections::BTreeMap;

pub mod codes;

pub use codes::{Category, DiagCode, Severity};

/// A stable identifier for a source file (path, virtual uri, etc.).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(pub String);

impl From<&str> for SourceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Byte offset span in a source buffer: [start, end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// Optional line/column mapping for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,   // 1-based
    pub column: u32, // 1-based (UTF-8 byte column by default)
}

impl LineCol {
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// A span location enriched with file and optional line/col.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
    pub line_col: Option<LineCol>,
}

impl Location {
    pub fn new(source: SourceId, span: Span) -> Self {
        Self {
            source,
            span,
            line_col: None,
        }
    }

    pub fn with_line_col(mut self, lc: LineCol) -> Self {
        self.line_col = Some(lc);
        self
    }
}

/// A label highlights a span and can carry a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub location: Location,
    pub message: Option<String>,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(location: Location) -> Self {
        Self {
            location,
            message: None,
            is_primary: true,
        }
    }

    pub fn secondary(location: Location) -> Self {
        Self {
            location,
            message: None,
            is_primary: false,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// A note/help entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub severity: Severity, // usually Note or Help
    pub message: String,
}

impl Note {
    pub fn note(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Note,
            message: msg.into(),
        }
    }

    pub fn help(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Help,
            message: msg.into(),
        }
    }
}

/// A structured diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub code: DiagCode,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<Note>,
    pub data: BTreeMap<String, String>, // machine-readable extra fields
}

impl Diag {
    pub fn new(code: DiagCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            data: BTreeMap::new(),
        }
    }

    pub fn from_code(code: DiagCode) -> Self {
        Self::new(code, code.default_message)
    }

    pub fn label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn note(mut self, note: Note) -> Self {
        self.notes.push(note);
        self
    }

    pub fn data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    pub fn severity(&self) -> Severity {
        self.code.severity
    }

    pub fn category(&self) -> Category {
        self.code.category
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.code, self.message)
    }
}

/// A bag/collector for diagnostics.
#[derive(Debug, Default)]
pub struct DiagBag {
    diags: Vec<Diag>,
}

impl DiagBag {
    pub fn new() -> Self {
        Self { diags: Vec::new() }
    }

    pub fn push(&mut self, d: Diag) {
        self.diags.push(d);
    }

    pub fn extend(&mut self, it: impl IntoIterator<Item = Diag>) {
        self.diags.extend(it);
    }

    pub fn into_vec(self) -> Vec<Diag> {
        self.diags
    }

    pub fn as_slice(&self) -> &[Diag] {
        &self.diags
    }

    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diags.len()
    }

    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.severity() == Severity::Error)
    }

    pub fn clear(&mut self) {
        self.diags.clear();
    }
}

/* ------------------------------- Rendering ------------------------------- */

/// A very small source map trait to support line/col + snippet rendering.
///
/// Implement this using your real source manager (paths + file contents).
pub trait SourceMap {
    /// Get full source text for a given file id.
    fn get(&self, id: &SourceId) -> Option<&str>;

    /// Map a span start to line/col. Default implementation uses `get()`.
    fn line_col(&self, id: &SourceId, byte_offset: u32) -> Option<LineCol> {
        let src = self.get(id)?;
        let off = byte_offset as usize;
        if off > src.len() {
            return None;
        }
        let mut line: u32 = 1;
        let mut col: u32 = 1;
        for (i, b) in src.as_bytes().iter().enumerate() {
            if i == off {
                break;
            }
            if *b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        Some(LineCol::new(line, col))
    }

    /// Extract a single line of context containing `byte_offset`.
    fn line_text(&self, id: &SourceId, byte_offset: u32) -> Option<(u32, String)> {
        let src = self.get(id)?;
        let off = byte_offset as usize;
        if off > src.len() {
            return None;
        }
        let mut line_no: u32 = 1;
        let mut line_start: usize = 0;
        for (i, b) in src.as_bytes().iter().enumerate() {
            if i == off {
                break;
            }
            if *b == b'\n' {
                line_no += 1;
                line_start = i + 1;
            }
        }
        let line_end = src[line_start..].find('\n').map(|x| line_start + x).unwrap_or(src.len());
        let line = src[line_start..line_end].to_string();
        Some((line_no, line))
    }
}

/// Plain text renderer (compact, single-line labels).
pub fn render_plain(diag: &Diag, sm: Option<&dyn SourceMap>) -> String {
    let mut out = String::new();

    // Header
    out.push_str(diag.severity().as_str());
    out.push_str("[");
    out.push_str(diag.code.code);
    out.push_str("] ");
    out.push_str(&diag.message);

    // First primary label location if present
    if let Some(lbl) = diag.labels.iter().find(|l| l.is_primary) {
        out.push_str("\n  --> ");
        out.push_str(&lbl.location.source.0);

        let lc = lbl
            .location
            .line_col
            .or_else(|| sm.and_then(|m| m.line_col(&lbl.location.source, lbl.location.span.start)));

        if let Some(lc) = lc {
            out.push(':');
            out.push_str(&lc.line.to_string());
            out.push(':');
            out.push_str(&lc.column.to_string());
        }
    }

    // Labels
    for lbl in &diag.labels {
        out.push('\n');
        out.push_str("  ");
        out.push_str(if lbl.is_primary { "= " } else { "- " });
        out.push_str(&lbl.location.source.0);

        let lc = lbl
            .location
            .line_col
            .or_else(|| sm.and_then(|m| m.line_col(&lbl.location.source, lbl.location.span.start)));

        if let Some(lc) = lc {
            out.push(':');
            out.push_str(&lc.line.to_string());
            out.push(':');
            out.push_str(&lc.column.to_string());
        }

        out.push_str("  ");
        out.push_str(&format!("[{}..{}]", lbl.location.span.start, lbl.location.span.end));

        if let Some(msg) = &lbl.message {
            out.push_str("  ");
            out.push_str(msg);
        }

        // Optional context line for primary
        if lbl.is_primary {
            if let Some(m) = sm {
                if let Some((_ln, line)) = m.line_text(&lbl.location.source, lbl.location.span.start) {
                    out.push('\n');
                    out.push_str("       ");
                    out.push_str(&line);
                }
            }
        }
    }

    // Notes
    for n in &diag.notes {
        out.push('\n');
        out.push_str("  ");
        out.push_str(n.severity.as_str());
        out.push_str(": ");
        out.push_str(&n.message);
    }

    // Data
    if !diag.data.is_empty() {
        out.push('\n');
        out.push_str("  data:");
        for (k, v) in &diag.data {
            out.push('\n');
            out.push_str("    ");
            out.push_str(k);
            out.push_str(": ");
            out.push_str(v);
        }
    }

    out
}

/* ------------------------------- Helpers -------------------------------- */

/// Convenience: create a diagnostic from a code string (lookup in registry).
pub fn diag_from_code_str(code: &str, message: Option<&str>) -> Option<Diag> {
    let c = codes::lookup(code)?;
    let mut d = Diag::new(*c, message.unwrap_or(c.default_message));
    if d.message.is_empty() {
        d.message = c.default_message.to_string();
    }
    Some(d)
}

/* ------------------------------- Tests ----------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    struct MiniSM {
        id: SourceId,
        src: String,
    }

    impl SourceMap for MiniSM {
        fn get(&self, id: &SourceId) -> Option<&str> {
            if *id == self.id {
                Some(&self.src)
            } else {
                None
            }
        }
    }

    #[test]
    fn render_plain_basic() {
        let sm = MiniSM {
            id: SourceId("file.muf".into()),
            src: "line1\nline2\nline3\n".into(),
        };

        let d = Diag::from_code(codes::MUF0001).label(
            Label::primary(Location::new(SourceId("file.muf".into()), Span::new(7, 12)))
                .with_message("here"),
        );

        let s = render_plain(&d, Some(&sm));
        assert!(s.contains("error[MUF0001]"));
        assert!(s.contains("--> file.muf:2:1"));
        assert!(s.contains("line2"));
    }

    #[test]
    fn bag_has_errors() {
        let mut b = DiagBag::new();
        b.push(Diag::from_code(codes::CLI1001)); // warning
        assert!(!b.has_errors());
        b.push(Diag::from_code(codes::MFF0001)); // error
        assert!(b.has_errors());
    }
}
