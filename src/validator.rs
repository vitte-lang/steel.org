// src/validator.rs
//
// Steel — validation primitives
//
// Purpose:
// - Provide a small, deterministic validation framework used across Steel.
// - Validate config, manifests, paths, identifiers, target specs, tool definitions, etc.
// - Aggregate issues as diagnostics-friendly structures (warnings/errors with optional spans).
//
// Design goals:
// - Zero heavy deps (no regex, no anyhow required).
// - Stable error codes and categories for CLI/CI.
// - Ergonomic builder-style API.
// - Usable both in "fail-fast" and "collect-all" modes.
//
// Typical usage:
//
//   let mut v = Validator::new("SteelConfig");
//   v.require(!name.is_empty(), "name must not be empty").at_line(12);
//   v.check_path_exists(&path, "missing input").error();
//   let report = v.finish();
//   if report.has_errors() { ... }
//
// Integration:
// - Convert ValidationReport into your diagnostics system.
// - Or print report via `report.format_human()`.
//
// Notes:
// - `Span` is intentionally simple (line/col ranges). If you already have a richer span type,
//   you can adapt the structs or add conversion impls.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/* ============================== span ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub line_end: u32,
    pub col_end: u32,
}

impl Span {
    pub fn single(line: u32, col: u32) -> Self {
        Self {
            line,
            col,
            line_end: line,
            col_end: col,
        }
    }

    pub fn lines(line: u32, line_end: u32) -> Self {
        Self {
            line,
            col: 0,
            line_end,
            col_end: 0,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            return Ok(());
        }
        if self.line == self.line_end && self.col == self.col_end {
            write!(f, "{}:{}", self.line, self.col)
        } else {
            write!(f, "{}:{}-{}:{}", self.line, self.col, self.line_end, self.col_end)
        }
    }
}

/* ============================== issue model ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Syntax,
    Semantic,
    IO,
    Policy,
    Toolchain,
    Security,
    DepGraph,
    Internal,
    Other,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Category::*;
        let s = match self {
            Syntax => "syntax",
            Semantic => "semantic",
            IO => "io",
            Policy => "policy",
            Toolchain => "toolchain",
            Security => "security",
            DepGraph => "depgraph",
            Internal => "internal",
            Other => "other",
        };
        f.write_str(s)
    }
}

/// Stable code for machine parsing (CI).
/// Use uppercase with underscores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueCode(pub &'static str);

impl IssueCode {
    pub const UNKNOWN: IssueCode = IssueCode("UNKNOWN");
    pub const REQUIRED: IssueCode = IssueCode("REQUIRED");
    pub const INVALID: IssueCode = IssueCode("INVALID");
    pub const DUPLICATE: IssueCode = IssueCode("DUPLICATE");
    pub const NOT_FOUND: IssueCode = IssueCode("NOT_FOUND");
    pub const UNSUPPORTED: IssueCode = IssueCode("UNSUPPORTED");
    pub const OUT_OF_RANGE: IssueCode = IssueCode("OUT_OF_RANGE");
    pub const SECURITY: IssueCode = IssueCode("SECURITY");
}

impl fmt::Display for IssueCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub category: Category,
    pub code: IssueCode,

    pub message: String,
    pub help: Option<String>,
    pub note: Option<String>,

    pub span: Option<Span>,

    /// Optional structured metadata (for UI/JSON).
    pub meta: BTreeMap<String, String>,
}

impl Issue {
    pub fn new<S: Into<String>>(severity: Severity, category: Category, code: IssueCode, message: S) -> Self {
        Self {
            severity,
            category,
            code,
            message: message.into(),
            help: None,
            note: None,
            span: None,
            meta: BTreeMap::new(),
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn help<S: Into<String>>(mut self, s: S) -> Self {
        self.help = Some(s.into());
        self
    }

    pub fn note<S: Into<String>>(mut self, s: S) -> Self {
        self.note = Some(s.into());
        self
    }

    pub fn meta<S: Into<String>>(mut self, k: S, v: S) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

/* ============================== report ============================== */

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub subject: String,
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Warning)
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let mut i = 0usize;
        let mut w = 0usize;
        let mut e = 0usize;
        for it in &self.issues {
            match it.severity {
                Severity::Info => i += 1,
                Severity::Warning => w += 1,
                Severity::Error => e += 1,
            }
        }
        (i, w, e)
    }

    pub fn format_human(&self) -> String {
        let mut out = String::new();
        let (i, w, e) = self.counts();
        out.push_str(&format!("validate {}: {} info, {} warnings, {} errors\n", self.subject, i, w, e));
        for it in &self.issues {
            out.push_str(&format_issue_human(it));
            out.push('\n');
        }
        out.trim_end().to_string()
    }

    pub fn to_json_like(&self) -> String {
        // Lightweight JSON-ish (not strict JSON escaping for all cases).
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"subject\": \"{}\",\n", escape_json(&self.subject)));
        out.push_str("  \"issues\": [\n");
        for (idx, it) in self.issues.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"severity\": \"{}\",\n", it.severity));
            out.push_str(&format!("      \"category\": \"{}\",\n", it.category));
            out.push_str(&format!("      \"code\": \"{}\",\n", it.code));
            out.push_str(&format!("      \"message\": \"{}\"", escape_json(&it.message)));

            if let Some(span) = it.span {
                out.push_str(&format!(",\n      \"span\": \"{}\"", escape_json(&span.to_string())));
            }
            if let Some(help) = &it.help {
                out.push_str(&format!(",\n      \"help\": \"{}\"", escape_json(help)));
            }
            if let Some(note) = &it.note {
                out.push_str(&format!(",\n      \"note\": \"{}\"", escape_json(note)));
            }

            if !it.meta.is_empty() {
                out.push_str(",\n      \"meta\": {");
                let mut first = true;
                for (k, v) in &it.meta {
                    if !first {
                        out.push_str(", ");
                    }
                    first = false;
                    out.push_str(&format!("\"{}\": \"{}\"", escape_json(k), escape_json(v)));
                }
                out.push('}');
            }

            out.push_str("\n    }");
            if idx + 1 != self.issues.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        out
    }
}

fn format_issue_human(it: &Issue) -> String {
    let mut s = String::new();
    if let Some(span) = it.span {
        s.push_str(&format!("[{}:{}:{}] ", it.severity, it.category, it.code));
        if span.line != 0 {
            s.push_str(&format!("@{} ", span));
        }
    } else {
        s.push_str(&format!("[{}:{}:{}] ", it.severity, it.category, it.code));
    }
    s.push_str(&it.message);

    if let Some(help) = &it.help {
        s.push_str(&format!("\n  help: {help}"));
    }
    if let Some(note) = &it.note {
        s.push_str(&format!("\n  note: {note}"));
    }
    if !it.meta.is_empty() {
        s.push_str("\n  meta:");
        for (k, v) in &it.meta {
            s.push_str(&format!("\n    {k}: {v}"));
        }
    }

    s
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/* ============================== validator API ============================== */

#[derive(Debug, Clone)]
pub struct Validator {
    subject: String,
    issues: Vec<Issue>,
}

impl Validator {
    pub fn new<S: Into<String>>(subject: S) -> Self {
        Self {
            subject: subject.into(),
            issues: Vec::new(),
        }
    }

    pub fn push(&mut self, issue: Issue) {
        self.issues.push(issue);
    }

    pub fn info<S: Into<String>>(&mut self, category: Category, code: IssueCode, message: S) -> IssueBuilder<'_> {
        let issue = Issue::new(Severity::Info, category, code, message);
        IssueBuilder { v: self, issue }
    }

    pub fn warn<S: Into<String>>(&mut self, category: Category, code: IssueCode, message: S) -> IssueBuilder<'_> {
        let issue = Issue::new(Severity::Warning, category, code, message);
        IssueBuilder { v: self, issue }
    }

    pub fn error<S: Into<String>>(&mut self, category: Category, code: IssueCode, message: S) -> IssueBuilder<'_> {
        let issue = Issue::new(Severity::Error, category, code, message);
        IssueBuilder { v: self, issue }
    }

    /// Convenience: add an error if condition is false.
    pub fn require(&mut self, cond: bool, message: &str) -> RequireBuilder<'_> {
        RequireBuilder {
            v: self,
            cond,
            message: message.to_string(),
            category: Category::Semantic,
            code: IssueCode::REQUIRED,
            span: None,
        }
    }

    /// Validate non-empty string.
    pub fn require_non_empty(&mut self, val: &str, what: &str) -> RequireBuilder<'_> {
        self.require(!val.trim().is_empty(), &format!("{what} must not be empty"))
    }

    /// Validate identifier with a conservative charset.
    pub fn require_ident(&mut self, val: &str, what: &str) -> RequireBuilder<'_> {
        let ok = is_ident(val);
        self.require(ok, &format!("{what} must match [A-Za-z_][A-Za-z0-9_]*"))
            .with_category(Category::Semantic)
            .with_code(IssueCode::INVALID)
    }

    /// Validate file/directory exists.
    pub fn check_path_exists(&mut self, path: &Path, what: &str) -> RequireBuilder<'_> {
        let ok = path.exists();
        self.require(ok, &format!("{what}: {}", path.display()))
            .with_category(Category::IO)
            .with_code(IssueCode::NOT_FOUND)
    }

    /// Finish and return report.
    pub fn finish(self) -> ValidationReport {
        ValidationReport {
            subject: self.subject,
            issues: self.issues,
        }
    }
}

pub struct IssueBuilder<'a> {
    v: &'a mut Validator,
    issue: Issue,
}

impl<'a> IssueBuilder<'a> {
    pub fn at(mut self, span: Span) -> Self {
        self.issue = self.issue.at(span);
        self
    }

    pub fn at_line(mut self, line: u32) -> Self {
        self.issue = self.issue.at(Span::single(line, 0));
        self
    }

    pub fn help<S: Into<String>>(mut self, s: S) -> Self {
        self.issue = self.issue.help(s);
        self
    }

    pub fn note<S: Into<String>>(mut self, s: S) -> Self {
        self.issue = self.issue.note(s);
        self
    }

    pub fn meta<S: Into<String>>(mut self, k: S, v: S) -> Self {
        self.issue = self.issue.meta(k, v);
        self
    }

    pub fn emit(self) {
        self.v.push(self.issue);
    }
}

pub struct RequireBuilder<'a> {
    v: &'a mut Validator,
    cond: bool,
    message: String,
    category: Category,
    code: IssueCode,
    span: Option<Span>,
}

impl<'a> RequireBuilder<'a> {
    pub fn with_category(mut self, c: Category) -> Self {
        self.category = c;
        self
    }

    pub fn with_code(mut self, code: IssueCode) -> Self {
        self.code = code;
        self
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn at_line(mut self, line: u32) -> Self {
        self.span = Some(Span::single(line, 0));
        self
    }

    pub fn help(mut self, help: &str) -> Self {
        // If condition fails, attach help to the emitted issue.
        if !self.cond {
            let msg = self.message.clone();
            let code = self.code.clone();
            let mut issue = Issue::new(Severity::Error, self.category, code, msg);
            if let Some(span) = self.span {
                issue = issue.at(span);
            }
            issue = issue.help(help.to_string());
            self.v.push(issue);
            // Mark as handled.
            self.cond = true;
        }
        self
    }

    pub fn emit(self) {
        if !self.cond {
            let mut issue = Issue::new(Severity::Error, self.category, self.code, self.message);
            if let Some(span) = self.span {
                issue = issue.at(span);
            }
            self.v.push(issue);
        }
    }
}

/* ============================== utilities ============================== */

pub fn is_ident(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    for c in it {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    true
}

pub fn is_ascii_path_safe_component(s: &str) -> bool {
    // conservative: alnum, underscore, dash, dot
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_display() {
        assert_eq!(Span::single(12, 3).to_string(), "12:3");
        assert_eq!(Span::lines(1, 4).to_string(), "1:0-4:0");
    }

    #[test]
    fn ident_validation() {
        assert!(is_ident("A"));
        assert!(is_ident("_x1"));
        assert!(!is_ident("1x"));
        assert!(!is_ident("a-b"));
        assert!(!is_ident(""));
    }

    #[test]
    fn validator_collects() {
        let mut v = Validator::new("test");
        v.require_non_empty("", "name").at_line(2).emit();
        v.require_ident("a-b", "id").at_line(3).emit();
        let r = v.finish();
        assert!(r.has_errors());
        let (_, _, e) = r.counts();
        assert_eq!(e, 2);
    }

    #[test]
    fn check_path_exists_reports() {
        let mut v = Validator::new("paths");
        v.check_path_exists(Path::new("this-path-should-not-exist-xyz"), "missing").emit();
        let r = v.finish();
        assert!(r.has_errors());
    }

    #[test]
    fn report_format_human_contains_counts() {
        let mut v = Validator::new("x");
        v.warn(Category::Policy, IssueCode::UNSUPPORTED, "something").at_line(1).emit();
        let r = v.finish();
        let s = r.format_human();
        assert!(s.contains("warnings"));
        assert!(s.contains("validate x"));
    }
}
