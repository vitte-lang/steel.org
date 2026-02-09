//! Diagnostic renderers.
//!
//! This module provides a couple of render backends:
//! - `PlainRenderer`: simple text (single-line-ish, good for logs)
//! - `PrettyRenderer`: multi-line with basic source snippet + caret underline
//!
//! No external deps (no ansi crate). If you want colors, wire this into your
//! `cli::ansi` utilities (or add a feature flag).

use core::fmt;

use super::{Diag, Label, LineCol, Severity, SourceId, SourceMap, Span};

/// Rendering options.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub show_code: bool,
    pub show_category: bool,
    pub show_source_snippet: bool,
    pub max_snippet_width: usize,
    pub tab_width: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            show_code: true,
            show_category: false,
            show_source_snippet: true,
            max_snippet_width: 160,
            tab_width: 4,
        }
    }
}

/// Trait implemented by all diagnostic renderers.
pub trait Renderer {
    fn render(&self, d: &Diag, sm: Option<&dyn SourceMap>) -> String;
}

/* ------------------------------- Plain ---------------------------------- */

#[derive(Debug, Clone)]
pub struct PlainRenderer {
    pub opts: RenderOptions,
}

impl Default for PlainRenderer {
    fn default() -> Self {
        Self {
            opts: RenderOptions::default(),
        }
    }
}

impl Renderer for PlainRenderer {
    fn render(&self, d: &Diag, sm: Option<&dyn SourceMap>) -> String {
        super::render_plain(d, sm)
    }
}

/* ------------------------------- Pretty --------------------------------- */

#[derive(Debug, Clone)]
pub struct PrettyRenderer {
    pub opts: RenderOptions,
}

impl Default for PrettyRenderer {
    fn default() -> Self {
        Self {
            opts: RenderOptions::default(),
        }
    }
}

impl Renderer for PrettyRenderer {
    fn render(&self, d: &Diag, sm: Option<&dyn SourceMap>) -> String {
        render_pretty(d, sm, &self.opts)
    }
}

/// Pretty rendering implementation.
pub fn render_pretty(d: &Diag, sm: Option<&dyn SourceMap>, opts: &RenderOptions) -> String {
    let mut out = String::new();

    // Header
    out.push_str(d.severity().as_str());

    if opts.show_code {
        out.push('[');
        out.push_str(d.code.code);
        out.push(']');
    }

    if opts.show_category {
        out.push('(');
        out.push_str(d.category().as_str());
        out.push(')');
    }

    out.push_str(": ");
    out.push_str(&d.message);

    // Choose primary label
    let primary = d.labels.iter().find(|l| l.is_primary).or_else(|| d.labels.first());

    if let Some(lbl) = primary {
        let loc = &lbl.location;
        out.push('\n');
        out.push_str("  --> ");
        out.push_str(&loc.source.0);

        let lc = loc
            .line_col
            .or_else(|| sm.and_then(|m| m.line_col(&loc.source, loc.span.start)));

        if let Some(lc) = lc {
            out.push(':');
            push_u32(&mut out, lc.line);
            out.push(':');
            push_u32(&mut out, lc.column);
        }

        // Snippet + caret
        if opts.show_source_snippet {
            if let Some(m) = sm {
                if let Some((line_no, line_text)) = m.line_text(&loc.source, loc.span.start) {
                    let lc = lc.or_else(|| m.line_col(&loc.source, loc.span.start));
                    let col = lc.map(|x| x.column).unwrap_or(1);

                    out.push('\n');
                    out.push_str("   |\n");

                    // line
                    out.push_str(" ");
                    push_u32(&mut out, line_no);
                    out.push_str(" | ");

                    let (rendered_line, col_rendered) =
                        normalize_tabs_and_clip(&line_text, col as usize, opts.tab_width, opts.max_snippet_width);
                    out.push_str(&rendered_line);
                    out.push('\n');

                    // underline
                    out.push_str("   | ");
                    out.push_str(&" ".repeat(col_rendered.saturating_sub(1)));
                    out.push('^');

                    let underline_len = underline_len_bytes(loc.span, &line_text, col as usize).min(64);
                    if underline_len > 1 {
                        out.push_str(&"~".repeat(underline_len - 1));
                    }

                    if let Some(msg) = &lbl.message {
                        out.push(' ');
                        out.push_str(msg);
                    }

                    out.push('\n');
                    out.push_str("   |\n");
                }
            }
        }
    }

    // Secondary labels
    for lbl in d.labels.iter().filter(|l| !l.is_primary) {
        out.push_str("  = note: ");
        out.push_str(&format_label(lbl, sm));
        out.push('\n');
    }

    // Notes/help
    for n in &d.notes {
        out.push_str("  ");
        out.push_str(n.severity.as_str());
        out.push_str(": ");
        out.push_str(&n.message);
        out.push('\n');
    }

    // Data
    if !d.data.is_empty() {
        out.push_str("  data:\n");
        for (k, v) in &d.data {
            out.push_str("    ");
            out.push_str(k);
            out.push_str(": ");
            out.push_str(v);
            out.push('\n');
        }
    }

    // Trim trailing newline
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }

    out
}

/* ------------------------------- Helpers -------------------------------- */

fn format_label(lbl: &Label, sm: Option<&dyn SourceMap>) -> String {
    let mut s = String::new();
    s.push_str(&lbl.location.source.0);

    let lc = lbl
        .location
        .line_col
        .or_else(|| sm.and_then(|m| m.line_col(&lbl.location.source, lbl.location.span.start)));

    if let Some(lc) = lc {
        s.push(':');
        push_u32(&mut s, lc.line);
        s.push(':');
        push_u32(&mut s, lc.column);
    }
    s.push_str(" ");
    s.push_str(&format!("[{}..{}]", lbl.location.span.start, lbl.location.span.end));
    if let Some(m) = &lbl.message {
        s.push_str(" — ");
        s.push_str(m);
    }
    s
}

fn push_u32(out: &mut String, v: u32) {
    out.push_str(&v.to_string());
}

/// Expand tabs to spaces, clip to max width, and adjust caret column accordingly.
/// Returns (rendered_line, adjusted_column_1_based_in_rendered_line).
fn normalize_tabs_and_clip(
    line: &str,
    col_1: usize,
    tab_width: usize,
    max_width: usize,
) -> (String, usize) {
    // Expand tabs while tracking mapping from original byte column to rendered column.
    // This is approximate: our LineCol uses byte columns, not grapheme clusters.
    let mut rendered = String::new();
    let mut rendered_col_at_target = 1usize;

    let mut cur_rendered_col = 1usize;
    let mut cur_byte_col = 1usize;

    for ch in line.chars() {
        if ch == '\t' {
            let spaces = tab_width - ((cur_rendered_col - 1) % tab_width);
            if cur_byte_col == col_1 {
                rendered_col_at_target = cur_rendered_col;
            }
            for _ in 0..spaces {
                rendered.push(' ');
                cur_rendered_col += 1;
            }
            cur_byte_col += 1;
        } else {
            if cur_byte_col == col_1 {
                rendered_col_at_target = cur_rendered_col;
            }
            rendered.push(ch);
            cur_rendered_col += 1;
            cur_byte_col += 1;
        }
    }

    // If caret column points after end, clamp.
    if rendered_col_at_target > cur_rendered_col {
        rendered_col_at_target = cur_rendered_col;
    }

    // Clip to max width (simple right clip).
    if rendered.len() > max_width {
        rendered.truncate(max_width);
        if rendered_col_at_target > max_width {
            rendered_col_at_target = max_width;
        }
    }

    (rendered, rendered_col_at_target)
}

/// Estimate underline length for a span within a line.
fn underline_len_bytes(span: Span, line: &str, col_1: usize) -> usize {
    // We don't know line_start offset here; approximate by using span length in bytes.
    // If span is empty, underline is 1.
    let len = span.len() as usize;
    if len == 0 {
        return 1;
    }

    // Try to keep underline within the current line boundary.
    let line_len = line.as_bytes().len().max(1);
    let max = line_len.saturating_sub(col_1.saturating_sub(1)).max(1);
    len.min(max).max(1)
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::codes;

    struct SM {
        id: SourceId,
        src: String,
    }

    impl SourceMap for SM {
        fn get(&self, id: &SourceId) -> Option<&str> {
            if *id == self.id {
                Some(&self.src)
            } else {
                None
            }
        }
    }

    #[test]
    fn pretty_renders_with_caret() {
        let sm = SM {
            id: SourceId("x.muf".into()),
            src: "hello\tworld\nsecond\n".into(),
        };

        let d = Diag::from_code(codes::MUF0001).label(
            Label::primary(super::super::Location::new(SourceId("x.muf".into()), Span::new(1, 5)))
                .with_message("bad token"),
        );

        let r = PrettyRenderer::default().render(&d, Some(&sm));
        assert!(r.contains("^"));
        assert!(r.contains("bad token"));
    }

    #[test]
    fn normalize_tabs_adjusts_column() {
        let (line, col) = normalize_tabs_and_clip("a\tb", 2, 4, 80);
        assert_eq!(line, "a   b");
        assert_eq!(col, 2);
    }
}
