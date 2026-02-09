//! span.rs 
//!
//! Gestion des positions/spans multi-fichiers, avec:
//! - offsets bytes (stable) + mapping ligne/colonne (1-based)
//! - colonnes bytes + colonnes “char” (approx UTF-8) + option tab width
//! - table de fichiers (FileMap) + index lignes (line_starts)
//! - extraction de snippets (1..N lignes) + rendu type diagnostic (gutter + carets)
//! - fusion/intersection/clamp, conversions utilitaires
//! - API déterministe (BTreeMap/BTreeSet côté caller) et std-only
//!
//! Convention:
//! - Span: [lo, hi) en bytes (hi exclusif)
//! - Pos: byte offset
//! - Line/col: 1-based
//! - col_byte: colonne en bytes dans la ligne
//! - col_char: colonne en “chars” (approx Unicode scalar), pas grapheme-cluster
//!
//! NOTE: Pour un rendu “rustc-like” complet (labels multiples, codes d’erreur, etc.),
//! ce module fournit les briques (SnippetBlock + render). Le “style” final est
//! à gérer dans diag.rs.

use std::cmp::{max, min};
use std::fmt;

/// ------------------------------------------------------------
/// Core ids / positions
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub file: FileId,
    pub lo: Pos,
    pub hi: Pos,
}

impl Span {
    pub const fn new(file: FileId, lo: Pos, hi: Pos) -> Self {
        Self { file, lo, hi }
    }

    pub const fn empty(file: FileId, at: Pos) -> Self {
        Self { file, lo: at, hi: at }
    }

    pub fn is_empty(&self) -> bool {
        self.lo.0 == self.hi.0
    }

    pub fn len(&self) -> u32 {
        self.hi.0.saturating_sub(self.lo.0)
    }

    pub fn contains(&self, p: Pos) -> bool {
        self.lo.0 <= p.0 && p.0 < self.hi.0
    }

    pub fn overlaps(&self, other: Span) -> bool {
        self.file == other.file && self.lo.0 < other.hi.0 && other.lo.0 < self.hi.0
    }

    pub fn union(self, other: Span) -> Span {
        if self.file != other.file {
            return self;
        }
        Span::new(
            self.file,
            Pos(min(self.lo.0, other.lo.0)),
            Pos(max(self.hi.0, other.hi.0)),
        )
    }

    pub fn intersect(self, other: Span) -> Option<Span> {
        if self.file != other.file {
            return None;
        }
        let lo = max(self.lo.0, other.lo.0);
        let hi = min(self.hi.0, other.hi.0);
        if lo < hi {
            Some(Span::new(self.file, Pos(lo), Pos(hi)))
        } else {
            None
        }
    }

    pub fn clamp(self, len: u32) -> Span {
        let lo = min(self.lo.0, len);
        let hi = min(max(self.hi.0, lo), len);
        Span::new(self.file, Pos(lo), Pos(hi))
    }

    pub fn shift(self, delta: i64) -> Span {
        fn add(p: u32, d: i64) -> u32 {
            if d >= 0 {
                p.saturating_add(d as u32)
            } else {
                p.saturating_sub((-d) as u32)
            }
        }
        Span::new(self.file, Pos(add(self.lo.0, delta)), Pos(add(self.hi.0, delta)))
    }
}

/// ------------------------------------------------------------
/// Line/col model
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    pub line: u32, // 1-based
    pub col: u32,  // 1-based
}

impl LineCol {
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPos {
    pub file: FileId,
    pub pos: Pos,
    pub line_col_byte: LineCol,
    pub line_col_char: LineCol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locus {
    pub file: FileId,
    pub path: String,
    pub span: Span,
    pub lo: ResolvedPos,
    pub hi: ResolvedPos,
}

impl fmt::Display for Locus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: path:line:col
        write!(f, "{}:{}:{}", self.path, self.lo.line_col_byte.line, self.lo.line_col_byte.col)
    }
}

/// ------------------------------------------------------------
/// File entries + index lines
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub id: FileId,
    pub path: String,
    pub text: String,

    // byte offsets for start of each line
    line_starts: Vec<u32>,
}

impl FileEntry {
    pub fn new(id: FileId, path: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut e = Self { id, path: path.into(), text, line_starts: Vec::new() };
        e.rebuild_line_index();
        e
    }

    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn rebuild_line_index(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);

        let bytes = self.text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                let next = i + 1;
                if next <= bytes.len() {
                    self.line_starts.push(next as u32);
                }
            }
            i += 1;
        }
    }

    pub fn line_starts(&self) -> &[u32] {
        &self.line_starts
    }

    /// Convert byte offset -> line index (0-based)
    pub fn line_index(&self, pos: Pos) -> usize {
        let p = min(pos.0, self.len());
        match self.line_starts.binary_search(&p) {
            Ok(i) => i,
            Err(ins) => ins.saturating_sub(1),
        }
    }

    /// Convert byte offset -> LineCol bytes (1-based)
    pub fn line_col_byte(&self, pos: Pos) -> LineCol {
        let p = min(pos.0, self.len());
        let idx = self.line_index(Pos(p));
        let line_start = self.line_starts.get(idx).copied().unwrap_or(0);
        LineCol::new(idx as u32 + 1, (p - line_start) + 1)
    }

    /// Convert byte offset -> LineCol chars (1-based), approximation Unicode scalar.
    /// - slice line_start..pos and count .chars()
    pub fn line_col_char(&self, pos: Pos) -> LineCol {
        let p = min(pos.0, self.len());
        let idx = self.line_index(Pos(p));
        let _line_start = self.line_starts.get(idx).copied().unwrap_or(0);
        let (ls, _le) = self.line_bounds(Pos(p));

        let ls_usize = ls as usize;
        let p_usize = p as usize;
        let line_prefix = &self.text[ls_usize..min(p_usize, self.text.len())];
        let col_char = line_prefix.chars().count() as u32 + 1;

        let line = idx as u32 + 1;
        LineCol::new(line, col_char)
    }

    /// Retourne (line_start, line_end) bytes de la ligne contenant pos.
    /// line_end est exclusif, sans inclure '\n'.
    pub fn line_bounds(&self, pos: Pos) -> (u32, u32) {
        let p = min(pos.0, self.len());
        let idx = self.line_index(Pos(p));

        let start = self.line_starts[idx];
        let end_excl = if idx + 1 < self.line_starts.len() {
            // exclude trailing '\n' if present
            let next = self.line_starts[idx + 1];
            if next > 0 { next - 1 } else { 0 }
        } else {
            self.len()
        };
        (start, min(end_excl, self.len()))
    }

    /// Accès slice (clamp)
    pub fn slice(&self, span: Span) -> &str {
        let s = span.clamp(self.len());
        let lo = s.lo.0 as usize;
        let hi = s.hi.0 as usize;
        &self.text[lo..hi]
    }

    /// Ligne (sans '\n') par index 0-based.
    pub fn line_text(&self, line_idx: usize) -> &str {
        let start = *self.line_starts.get(line_idx).unwrap_or(&0) as usize;
        let end = if line_idx + 1 < self.line_starts.len() {
            (self.line_starts[line_idx + 1].saturating_sub(1)) as usize
        } else {
            self.text.len()
        };
        &self.text[start..min(end, self.text.len())]
    }

    /// Snippet sur N lignes autour d’un span.
    pub fn snippet_block(&self, span: Span, cfg: SnippetCfg) -> SnippetBlock {
        let s = span.clamp(self.len());
        let lo_line = self.line_index(s.lo);
        let hi_line = self.line_index(s.hi);

        let start_line = lo_line.saturating_sub(cfg.context_lines);
        let end_line = min(hi_line + cfg.context_lines, self.line_starts.len().saturating_sub(1));

        let mut lines: Vec<SnippetLine> = Vec::new();

        for li in start_line..=end_line {
            let text = self.line_text(li).to_string();

            // compute caret coverage for this line in byte columns
            let line_start = self.line_starts[li];
            let line_end = {
                let (ls, le) = self.line_bounds(Pos(line_start));
                let _ = ls;
                le
            };

            let seg_lo = max(s.lo.0, line_start);
            let seg_hi = min(s.hi.0, line_end);

            let (caret_lo_byte, caret_hi_byte) = if seg_lo < seg_hi {
                (seg_lo - line_start, seg_hi - line_start)
            } else if li == lo_line && cfg.caret_for_empty && s.is_empty() {
                // caret at position
                let at = s.lo.0.saturating_sub(line_start);
                (at, at)
            } else {
                (0, 0)
            };

            let caret = Caret {
                lo_byte: caret_lo_byte,
                hi_byte: caret_hi_byte,
            };

            lines.push(SnippetLine { line_idx: li as u32, text, caret });
        }

        SnippetBlock {
            file: self.id,
            path: self.path.clone(),
            span: s,
            start_line: start_line as u32 + 1, // 1-based
            lines,
        }
    }

    /// Rendu texte (gutter + carets). Couleurs gérées ailleurs.
    pub fn render_snippet(&self, block: &SnippetBlock, cfg: RenderCfg) -> String {
        let mut out = String::new();

        let last_line_num = block.start_line + (block.lines.len().saturating_sub(1) as u32);
        let gutter_w = num_width(last_line_num);

        // header
        if cfg.include_header {
            let loc = self.line_col_byte(block.span.lo);
            out.push_str(&format!(
                "--> {}:{}:{}\n",
                self.path, loc.line, loc.col
            ));
        }

        for (i, sl) in block.lines.iter().enumerate() {
            let line_no = block.start_line + i as u32;

            // line text
            out.push_str(&format!(
                "{:>width$} | {}\n",
                line_no,
                sl.text,
                width = gutter_w
            ));

            // caret line
            if cfg.show_carets && (sl.caret.is_active() || (cfg.caret_for_empty && block.span.is_empty())) {
                let caret = caret_string(&sl.text, sl.caret, cfg.tab_width, cfg.min_caret_width);
                if !caret.trim().is_empty() {
                    out.push_str(&format!(
                        "{:>width$} | {}\n",
                        "",
                        caret,
                        width = gutter_w
                    ));
                }
            }
        }

        out
    }
}

/// ------------------------------------------------------------
/// FileMap: multi-fichiers
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileMap {
    next_id: u32,
    files: Vec<FileEntry>,
}

impl Default for FileMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FileMap {
    pub fn new() -> Self {
        Self { next_id: 1, files: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn add_file(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let id = FileId(self.next_id);
        self.next_id += 1;
        self.files.push(FileEntry::new(id, path, text));
        id
    }

    pub fn add_file_entry(&mut self, entry: FileEntry) -> FileId {
        let id = entry.id;
        self.next_id = max(self.next_id, id.0 + 1);
        self.files.push(entry);
        id
    }

    pub fn get(&self, id: FileId) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.id == id)
    }

    pub fn get_mut(&mut self, id: FileId) -> Option<&mut FileEntry> {
        self.files.iter_mut().find(|f| f.id == id)
    }

    pub fn path(&self, id: FileId) -> Option<&str> {
        self.get(id).map(|f| f.path.as_str())
    }

    pub fn locus(&self, span: Span) -> Option<Locus> {
        let f = self.get(span.file)?;
        let s = span.clamp(f.len());

        let lo = ResolvedPos {
            file: span.file,
            pos: s.lo,
            line_col_byte: f.line_col_byte(s.lo),
            line_col_char: f.line_col_char(s.lo),
        };
        let hi = ResolvedPos {
            file: span.file,
            pos: s.hi,
            line_col_byte: f.line_col_byte(s.hi),
            line_col_char: f.line_col_char(s.hi),
        };

        Some(Locus {
            file: span.file,
            path: f.path.clone(),
            span: s,
            lo,
            hi,
        })
    }

    pub fn snippet(&self, span: Span, cfg: SnippetCfg) -> Option<SnippetBlock> {
        let f = self.get(span.file)?;
        Some(f.snippet_block(span, cfg))
    }

    pub fn render(&self, span: Span, snip: SnippetCfg, render: RenderCfg) -> Option<String> {
        let f = self.get(span.file)?;
        let block = f.snippet_block(span, snip);
        Some(f.render_snippet(&block, render))
    }
}

/// ------------------------------------------------------------
/// Snippet structures
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    /// caret lo/hi en bytes dans la ligne (0-based). [lo, hi)
    pub lo_byte: u32,
    pub hi_byte: u32,
}

impl Caret {
    pub fn is_active(&self) -> bool {
        self.lo_byte != 0 || self.hi_byte != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetLine {
    pub line_idx: u32, // 0-based index
    pub text: String,
    pub caret: Caret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetBlock {
    pub file: FileId,
    pub path: String,
    pub span: Span,
    pub start_line: u32, // 1-based
    pub lines: Vec<SnippetLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnippetCfg {
    pub context_lines: usize,
    pub caret_for_empty: bool,
}

impl Default for SnippetCfg {
    fn default() -> Self {
        Self { context_lines: 1, caret_for_empty: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderCfg {
    pub include_header: bool,
    pub show_carets: bool,
    pub tab_width: usize,
    pub min_caret_width: usize,
    pub caret_for_empty: bool,
}

impl Default for RenderCfg {
    fn default() -> Self {
        Self {
            include_header: true,
            show_carets: true,
            tab_width: 4,
            min_caret_width: 1,
            caret_for_empty: true,
        }
    }
}

/// caret string builder:
/// - respects tabs by expanding to tab_width spaces for alignment
fn caret_string(line: &str, caret: Caret, tab_width: usize, min_width: usize) -> String {
    let lo = caret.lo_byte as usize;
    let hi = caret.hi_byte as usize;

    // if empty span => caret at lo
    let lo2 = lo;
    let mut hi2 = hi;
    if lo2 == hi2 {
        hi2 = lo2 + min_width;
    } else if hi2 < lo2 {
        hi2 = lo2 + min_width;
    }

    // Expand the prefix into visual columns, treating tabs as tab stops.
    let mut out = String::new();
    let mut col = 0usize;

    // Build spaces until lo2
    let bytes = line.as_bytes();
    let limit = min(lo2, bytes.len());
    for &b in &bytes[..limit] {
        if b == b'\t' {
            let next = ((col / tab_width) + 1) * tab_width;
            let n = next.saturating_sub(col);
            for _ in 0..n {
                out.push(' ');
            }
            col = next;
        } else {
            out.push(' ');
            col += 1;
        }
    }

    // Caret length in bytes (approx); clamp
    let len = min(hi2, bytes.len()).saturating_sub(lo2);
    let caret_len = max(len, min_width);

    for _ in 0..caret_len {
        out.push('^');
    }

    out
}

fn num_width(mut n: u32) -> usize {
    if n == 0 {
        return 1;
    }
    let mut w = 0usize;
    while n > 0 {
        w += 1;
        n /= 10;
    }
    w
}

/// ------------------------------------------------------------
/// Utilities construction spans
/// ------------------------------------------------------------

pub fn span_from_range(file: FileId, lo: usize, hi: usize) -> Span {
    Span::new(file, Pos(lo as u32), Pos(hi as u32))
}

pub fn span_at(file: FileId, at: usize) -> Span {
    Span::empty(file, Pos(at as u32))
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_and_col() {
        let mut fm = FileMap::new();
        let id = fm.add_file("x", "a\nbb\nccc\n");
        let f = fm.get(id).unwrap();

        assert_eq!(f.line_col_byte(Pos(0)), LineCol::new(1, 1));
        assert_eq!(f.line_col_byte(Pos(1)), LineCol::new(1, 2)); // '\n'
        assert_eq!(f.line_col_byte(Pos(2)), LineCol::new(2, 1));
        assert_eq!(f.line_col_byte(Pos(3)), LineCol::new(2, 2));
        assert_eq!(f.line_col_byte(Pos(5)), LineCol::new(3, 1));
    }

    #[test]
    fn union_intersect() {
        let file = FileId(1);
        let a = Span::new(file, Pos(2), Pos(5));
        let b = Span::new(file, Pos(4), Pos(7));
        assert!(a.overlaps(b));

        let u = a.union(b);
        assert_eq!(u.lo.0, 2);
        assert_eq!(u.hi.0, 7);

        let i = a.intersect(b).unwrap();
        assert_eq!(i.lo.0, 4);
        assert_eq!(i.hi.0, 5);
    }

    #[test]
    fn snippet_block_single_line() {
        let mut fm = FileMap::new();
        let id = fm.add_file("x.muf", "hello world\n");
        let f = fm.get(id).unwrap();

        let sp = Span::new(id, Pos(6), Pos(11)); // world
        let block = f.snippet_block(sp, SnippetCfg { context_lines: 0, caret_for_empty: true });
        assert_eq!(block.lines.len(), 1);
        assert_eq!(block.lines[0].text, "hello world");
        let rendered = f.render_snippet(&block, RenderCfg { include_header: false, ..Default::default() });
        assert!(rendered.contains("^^^^^"));
    }

    #[test]
    fn render_with_context() {
        let mut fm = FileMap::new();
        let id = fm.add_file("x.vit", "line1\nline2\nline3\n");
        let txt = fm.render(
            Span::new(id, Pos(7), Pos(12)), // "line2"
            SnippetCfg { context_lines: 1, caret_for_empty: true },
            RenderCfg { include_header: true, ..Default::default() },
        ).unwrap();
        assert!(txt.contains("--> x.vit:2:"));
        assert!(txt.contains("2 | line2"));
    }

    #[test]
    fn tabs_alignment() {
        let mut fm = FileMap::new();
        let id = fm.add_file("x", "\t\tabc\n");
        let f = fm.get(id).unwrap();
        let sp = Span::new(id, Pos(2), Pos(5)); // within bytes (approx)
        let block = f.snippet_block(sp, SnippetCfg { context_lines: 0, caret_for_empty: true });
        let rendered = f.render_snippet(&block, RenderCfg { include_header: false, tab_width: 4, ..Default::default() });
        assert!(rendered.contains("^"));
    }
}
