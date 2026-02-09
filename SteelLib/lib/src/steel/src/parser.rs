

//! MUF v4.1 parser ("Bracket + Dot Ops", no `.end`)
//!
//! Spec summary:
//! - Header:     `!muf 4` (first non-empty, non-comment line)
//! - Block head: `[TAG name?]` (line)
//! - Directive:  `.op arg1 arg2 ...` (line, inside a block)
//! - Close:      `..` (line, closes the most recent open block)
//! - Comment:    `;; ...` (line comment, ignored)
//!
//! Parsing is line-oriented and stack-based (push on `[ ... ]`, pop on `..`).

use std::fmt;

// -------------------------------------------------------------------------------------------------
// AST
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct MufFile {
    pub version: i64,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub tag: String,
    pub name: Option<String>,
    pub items: Vec<BlockItem>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockItem {
    Block(Block),
    Directive(Directive),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub op: String,
    pub args: Vec<Atom>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Atom {
    Ref(RefPath),
    Str(String),
    Number(Number),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefPath {
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    Int { raw: String, value: i64 },
    Float { raw: String, value: f64 },
}

// -------------------------------------------------------------------------------------------------
// Spans / diagnostics
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Span {
    /// 1-based line index
    pub line: usize,
    /// 1-based column index (byte-based, UTF-8 safe for ASCII syntax)
    pub col: usize,
    /// 1-based line index
    pub line_end: usize,
    /// 1-based column index
    pub col_end: usize,
}

impl Span {
    pub fn point(line: usize, col: usize) -> Self {
        Self {
            line,
            col,
            line_end: line,
            col_end: col,
        }
    }

    pub fn line_range(line: usize, col: usize, col_end: usize) -> Self {
        Self {
            line,
            col,
            line_end: line,
            col_end,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (L{}:C{})", self.message, self.span.line, self.span.col)
    }
}

impl std::error::Error for ParseError {}

// -------------------------------------------------------------------------------------------------
// Public API
// -------------------------------------------------------------------------------------------------

pub fn parse_muf(source: &str) -> Result<MufFile, ParseError> {
    Parser::new(source).parse_file()
}

// -------------------------------------------------------------------------------------------------
// Parser
// -------------------------------------------------------------------------------------------------

struct Parser<'a> {
    _src: &'a str,
    lines: Vec<&'a str>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        // Keep line endings out; normalize `\r\n` by trimming trailing `\r` per-line.
        let lines: Vec<&str> = src.split('\n').collect();
        Self { _src: src, lines }
    }

    fn parse_file(&self) -> Result<MufFile, ParseError> {
        let mut version: Option<i64> = None;
        let mut blocks: Vec<Block> = Vec::new();
        let mut stack: Vec<Block> = Vec::new();

        let mut saw_header = false;

        for (i, raw_line) in self.lines.iter().enumerate() {
            let line_no = i + 1;
            let line = trim_cr(raw_line);

            // Ignore blank lines
            if is_blank(line) {
                continue;
            }

            // Ignore full-line comments
            if is_comment_line(line) {
                continue;
            }

            if !saw_header {
                // First real line must be header
                let (v, span) = parse_header(line, line_no)?;
                version = Some(v);
                saw_header = true;
                // header must be version 4 for v4.1
                if v != 4 {
                    return Err(ParseError {
                        message: format!("unsupported MUF version: {} (expected 4)", v),
                        span,
                    });
                }
                continue;
            }

            // After header: blocks only at top-level
            if is_block_close(line) {
                // pop
                let Some(mut finished) = stack.pop() else {
                    return Err(ParseError {
                        message: "unexpected block close `..` with empty stack".to_string(),
                        span: Span::line_range(line_no, first_non_ws_col(line), first_non_ws_col(line) + 1),
                    });
                };

                // finalize span end
                finished.span.line_end = line_no;
                finished.span.col_end = line_trimmed_len(line) + 1;

                if let Some(parent) = stack.last_mut() {
                    parent.items.push(BlockItem::Block(finished));
                } else {
                    blocks.push(finished);
                }
                continue;
            }

            if is_block_head(line) {
                let (tag, name, span) = parse_block_head(line, line_no)?;
                let blk = Block {
                    tag,
                    name,
                    items: Vec::new(),
                    span,
                };
                stack.push(blk);
                continue;
            }

            if is_directive(line) {
                let (dir, _span) = parse_directive(line, line_no)?;
                let Some(parent) = stack.last_mut() else {
                    return Err(ParseError {
                        message: "directive outside of any block".to_string(),
                        span: dir.span,
                    });
                };
                parent.items.push(BlockItem::Directive(dir));
                continue;
            }

            // Unknown line kind
            return Err(ParseError {
                message: "unexpected line (expected block head `[ ... ]`, directive `.op`, comment `;;`, or close `..`)".to_string(),
                span: Span::line_range(line_no, first_non_ws_col(line), line_trimmed_len(line) + 1),
            });
        }

        if !saw_header {
            return Err(ParseError {
                message: "missing header `!muf 4`".to_string(),
                span: Span::point(1, 1),
            });
        }

        if let Some(unclosed) = stack.last() {
            return Err(ParseError {
                message: format!(
                    "unclosed block `[{}{}]` (missing `..`)",
                    unclosed.tag,
                    unclosed
                        .name
                        .as_ref()
                        .map(|n| format!(" {}", n))
                        .unwrap_or_default()
                ),
                span: unclosed.span,
            });
        }

        Ok(MufFile {
            version: version.unwrap_or(4),
            blocks,
        })
    }
}

// -------------------------------------------------------------------------------------------------
// Line classification helpers
// -------------------------------------------------------------------------------------------------

fn trim_cr(s: &str) -> &str {
    if let Some(stripped) = s.strip_suffix('\r') {
        stripped
    } else {
        s
    }
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');
    t.starts_with(";;")
}

fn is_block_close(line: &str) -> bool {
    let t = line.trim_matches(|c| c == ' ' || c == '\t');
    t == ".."
}

fn is_block_head(line: &str) -> bool {
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');
    t.starts_with('[')
}

fn is_directive(line: &str) -> bool {
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');
    t.starts_with('.') && !t.starts_with("..")
}

fn first_non_ws_col(line: &str) -> usize {
    let mut col = 1;
    for b in line.bytes() {
        match b {
            b' ' | b'\t' => col += 1,
            _ => break,
        }
    }
    col
}

fn line_trimmed_len(line: &str) -> usize {
    line.trim_end_matches(|c| c == ' ' || c == '\t').len()
}

// -------------------------------------------------------------------------------------------------
// Header
// -------------------------------------------------------------------------------------------------

fn parse_header(line: &str, line_no: usize) -> Result<(i64, Span), ParseError> {
    let col0 = first_non_ws_col(line);
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');

    if !t.starts_with("!muf") {
        return Err(ParseError {
            message: "expected header `!muf 4`".to_string(),
            span: Span::line_range(line_no, col0, col0 + 4),
        });
    }

    let mut p = LineParser::new(t, line_no, col0);
    p.expect_lit("!muf")?;
    p.skip_ws1()?;
    let (v, v_span) = p.parse_int()?;
    p.skip_ws0();
    if !p.eof() {
        return Err(ParseError {
            message: "unexpected trailing tokens after header".to_string(),
            span: Span::line_range(line_no, p.col(), p.col_end()),
        });
    }

    Ok((v, v_span))
}

// -------------------------------------------------------------------------------------------------
// Block head
// -------------------------------------------------------------------------------------------------

fn parse_block_head(line: &str, line_no: usize) -> Result<(String, Option<String>, Span), ParseError> {
    let col0 = first_non_ws_col(line);
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');

    let mut p = LineParser::new(t, line_no, col0);
    p.expect_char('[')?;
    p.skip_ws0();
    let (tag, _tag_span) = p.parse_name()?;

    // optional name
    let name = if p.peek_ws1() {
        p.skip_ws1()?;
        let (nm, _nm_span) = p.parse_name()?;
        Some(nm)
    } else {
        None
    };

    p.skip_ws0();
    p.expect_char(']')?;
    p.skip_ws0();

    if !p.eof() {
        return Err(ParseError {
            message: "unexpected trailing tokens after block head".to_string(),
            span: Span::line_range(line_no, p.col(), p.col_end()),
        });
    }

    let span = Span::line_range(line_no, col0, col0 + line_trimmed_len(line).saturating_sub(col0 - 1));

    Ok((tag, name, span))
}

// -------------------------------------------------------------------------------------------------
// Directive
// -------------------------------------------------------------------------------------------------

fn parse_directive(line: &str, line_no: usize) -> Result<(Directive, Span), ParseError> {
    let col0 = first_non_ws_col(line);
    let t = line.trim_start_matches(|c| c == ' ' || c == '\t');

    let mut p = LineParser::new(t, line_no, col0);
    p.expect_char('.')?;
    let (op, _op_span) = p.parse_name()?;

    let mut args = Vec::new();
    while !p.eof() {
        if p.peek_ws1() {
            p.skip_ws1()?;
        } else {
            // no separator => invalid (two atoms glued)
            return Err(ParseError {
                message: "expected whitespace separator between directive tokens".to_string(),
                span: Span::point(line_no, p.col()),
            });
        }

        if p.eof() {
            break;
        }

        let atom = p.parse_atom()?;
        args.push(atom);
    }

    let span = Span::line_range(line_no, col0, col0 + line_trimmed_len(line).saturating_sub(col0 - 1));

    Ok((
        Directive {
            op,
            args,
            span,
        },
        span,
    ))
}

// -------------------------------------------------------------------------------------------------
// Line parser (token parsing within a single line)
// -------------------------------------------------------------------------------------------------

struct LineParser<'a> {
    s: &'a str,
    bytes: &'a [u8],
    idx: usize,
    line: usize,
    col0: usize, // starting column of s in original line
}

impl<'a> LineParser<'a> {
    fn new(s: &'a str, line: usize, col0: usize) -> Self {
        Self {
            s,
            bytes: s.as_bytes(),
            idx: 0,
            line,
            col0,
        }
    }

    fn eof(&self) -> bool {
        self.idx >= self.bytes.len()
    }

    fn col(&self) -> usize {
        self.col0 + self.idx
    }

    fn col_end(&self) -> usize {
        self.col0 + self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.idx).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.idx += 1;
        Some(b)
    }

    fn expect_char(&mut self, ch: char) -> Result<(), ParseError> {
        let want = ch as u8;
        match self.bump() {
            Some(got) if got == want => Ok(()),
            _ => Err(ParseError {
                message: format!("expected `{}`", ch),
                span: Span::point(self.line, self.col()),
            }),
        }
    }

    fn expect_lit(&mut self, lit: &str) -> Result<(), ParseError> {
        if self.s[self.idx..].starts_with(lit) {
            self.idx += lit.len();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("expected `{}`", lit),
                span: Span::point(self.line, self.col()),
            })
        }
    }

    fn skip_ws0(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.idx += 1;
        }
    }

    fn peek_ws1(&self) -> bool {
        matches!(self.peek(), Some(b' ' | b'\t'))
    }

    fn skip_ws1(&mut self) -> Result<(), ParseError> {
        if !self.peek_ws1() {
            return Err(ParseError {
                message: "expected whitespace".to_string(),
                span: Span::point(self.line, self.col()),
            });
        }
        self.skip_ws0();
        Ok(())
    }

    fn parse_name(&mut self) -> Result<(String, Span), ParseError> {
        let start = self.idx;
        let start_col = self.col();

        let Some(b0) = self.peek() else {
            return Err(ParseError {
                message: "expected name".to_string(),
                span: Span::point(self.line, start_col),
            });
        };

        if !is_ident_start(b0) {
            return Err(ParseError {
                message: "expected name (identifier)".to_string(),
                span: Span::point(self.line, start_col),
            });
        }

        self.idx += 1;
        while let Some(b) = self.peek() {
            if is_ident_cont(b) {
                self.idx += 1;
            } else {
                break;
            }
        }

        let raw = &self.s[start..self.idx];
        Ok((raw.to_string(), Span::line_range(self.line, start_col, self.col())))
    }

    fn parse_int(&mut self) -> Result<(i64, Span), ParseError> {
        let start = self.idx;
        let start_col = self.col();

        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.idx += 1;
        }

        let d0 = self.peek().ok_or_else(|| ParseError {
            message: "expected integer".to_string(),
            span: Span::point(self.line, self.col()),
        })?;

        if !is_digit(d0) {
            return Err(ParseError {
                message: "expected integer digits".to_string(),
                span: Span::point(self.line, self.col()),
            });
        }

        self.idx += 1;
        while let Some(b) = self.peek() {
            if is_digit(b) {
                self.idx += 1;
            } else {
                break;
            }
        }

        let raw = &self.s[start..self.idx];
        let value = raw.parse::<i64>().map_err(|_| ParseError {
            message: "integer out of range".to_string(),
            span: Span::line_range(self.line, start_col, self.col()),
        })?;

        Ok((value, Span::line_range(self.line, start_col, self.col())))
    }

    fn parse_number(&mut self) -> Result<Number, ParseError> {
        let start = self.idx;
        let start_col = self.col();

        // sign
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.idx += 1;
        }

        // digits
        let mut saw_digit = false;
        while let Some(b) = self.peek() {
            if is_digit(b) {
                saw_digit = true;
                self.idx += 1;
            } else {
                break;
            }
        }

        if !saw_digit {
            return Err(ParseError {
                message: "expected number".to_string(),
                span: Span::point(self.line, self.col()),
            });
        }

        // float part?
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            // lookahead: '.' must be followed by digit to be float (otherwise it's invalid here)
            if self.bytes.get(self.idx + 1).copied().map(is_digit).unwrap_or(false) {
                is_float = true;
                self.idx += 1; // '.'
                while let Some(b) = self.peek() {
                    if is_digit(b) {
                        self.idx += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        // exponent?
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.idx += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.idx += 1;
            }
            let Some(b0) = self.peek() else {
                return Err(ParseError {
                    message: "expected exponent digits".to_string(),
                    span: Span::point(self.line, self.col()),
                });
            };
            if !is_digit(b0) {
                return Err(ParseError {
                    message: "expected exponent digits".to_string(),
                    span: Span::point(self.line, self.col()),
                });
            }
            self.idx += 1;
            while let Some(b) = self.peek() {
                if is_digit(b) {
                    self.idx += 1;
                } else {
                    break;
                }
            }
        }

        let raw = self.s[start..self.idx].to_string();

        if is_float {
            let value = raw.parse::<f64>().map_err(|_| ParseError {
                message: "invalid float".to_string(),
                span: Span::line_range(self.line, start_col, self.col()),
            })?;
            Ok(Number::Float { raw, value })
        } else {
            let value = raw.parse::<i64>().map_err(|_| ParseError {
                message: "integer out of range".to_string(),
                span: Span::line_range(self.line, start_col, self.col()),
            })?;
            Ok(Number::Int { raw, value })
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        let start_col = self.col();
        self.expect_char('"')?;

        let mut out = String::new();
        while !self.eof() {
            let b = self.bump().ok_or_else(|| ParseError {
                message: "unterminated string".to_string(),
                span: Span::point(self.line, start_col),
            })?;

            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.bump().ok_or_else(|| ParseError {
                        message: "unterminated escape".to_string(),
                        span: Span::point(self.line, self.col()),
                    })?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'0' => out.push('\0'),
                        b'x' => {
                            let h1 = self.bump().ok_or_else(|| ParseError {
                                message: "expected hex digit".to_string(),
                                span: Span::point(self.line, self.col()),
                            })?;
                            let h2 = self.bump().ok_or_else(|| ParseError {
                                message: "expected hex digit".to_string(),
                                span: Span::point(self.line, self.col()),
                            })?;
                            let v = (hex_val(h1)? << 4) | hex_val(h2)?;
                            out.push(v as char);
                        }
                        b'u' => {
                            let mut v: u32 = 0;
                            for _ in 0..4 {
                                let h = self.bump().ok_or_else(|| ParseError {
                                    message: "expected hex digit".to_string(),
                                    span: Span::point(self.line, self.col()),
                                })?;
                                v = (v << 4) | (hex_val(h)? as u32);
                            }
                            let ch = char::from_u32(v).ok_or_else(|| ParseError {
                                message: "invalid unicode escape".to_string(),
                                span: Span::point(self.line, self.col()),
                            })?;
                            out.push(ch);
                        }
                        _ => {
                            return Err(ParseError {
                                message: "unknown escape".to_string(),
                                span: Span::point(self.line, self.col()),
                            })
                        }
                    }
                }
                b'\n' | b'\r' => {
                    return Err(ParseError {
                        message: "newline in string literal".to_string(),
                        span: Span::point(self.line, self.col()),
                    })
                }
                _ => out.push(b as char),
            }
        }

        Err(ParseError {
            message: "unterminated string".to_string(),
            span: Span::point(self.line, start_col),
        })
    }

    fn parse_ref(&mut self) -> Result<RefPath, ParseError> {
        self.expect_char('~')?;
        let (a, _) = self.parse_name()?;
        self.expect_char('/')?;
        let (b, _) = self.parse_name()?;
        let mut segs = vec![a, b];
        while self.peek() == Some(b'/') {
            self.idx += 1;
            let (n, _) = self.parse_name()?;
            segs.push(n);
        }
        Ok(RefPath { segments: segs })
    }

    fn parse_atom(&mut self) -> Result<Atom, ParseError> {
        match self.peek() {
            Some(b'~') => Ok(Atom::Ref(self.parse_ref()?)),
            Some(b'"') => Ok(Atom::Str(self.parse_string()?)),
            Some(b'+' | b'-') => {
                // could be number (preferred) or name (not allowed by spec)
                Ok(Atom::Number(self.parse_number()?))
            }
            Some(b) if is_digit(b) => Ok(Atom::Number(self.parse_number()?)),
            Some(_) => {
                let (n, _) = self.parse_name()?;
                Ok(Atom::Name(n))
            }
            None => Err(ParseError {
                message: "expected atom".to_string(),
                span: Span::point(self.line, self.col()),
            }),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Small character helpers
// -------------------------------------------------------------------------------------------------

fn is_digit(b: u8) -> bool {
    b'0' <= b && b <= b'9'
}

fn is_ident_start(b: u8) -> bool {
    (b'A' <= b && b <= b'Z') || (b'a' <= b && b <= b'z') || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    is_ident_start(b) || is_digit(b)
}

fn hex_val(b: u8) -> Result<u8, ParseError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(ParseError {
            message: "expected hex digit".to_string(),
            span: Span::point(0, 0),
        }),
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let src = "!muf 4\n[WS x]\n  .root \".\"\n..\n";
        let f = parse_muf(src).unwrap();
        assert_eq!(f.version, 4);
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.blocks[0].tag, "WS");
        assert_eq!(f.blocks[0].name.as_deref(), Some("x"));
    }

    #[test]
    fn parses_nested_blocks() {
        let src = r#"!muf 4
[BK app]
  [IN src]
    .kind files
  ..
  [STP compile]
    .run ~tool/vittec
    .take src "--src"
    .emit exe "--out"
  ..
..
"#;
        let f = parse_muf(src).unwrap();
        assert_eq!(f.blocks.len(), 1);
        let bk = &f.blocks[0];
        assert_eq!(bk.tag, "BK");
        assert_eq!(bk.name.as_deref(), Some("app"));
        assert!(bk.items.iter().any(|it| matches!(it, BlockItem::Block(_))));
    }

    #[test]
    fn errors_on_unmatched_close() {
        let src = "!muf 4\n..\n";
        assert!(parse_muf(src).is_err());
    }

    #[test]
    fn errors_on_missing_close() {
        let src = "!muf 4\n[WS x]\n  .root \".\"\n";
        assert!(parse_muf(src).is_err());
    }
}
