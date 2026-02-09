//! MUF v4.1 lexer ("Bracket + Dot Ops", no `.end`)
//!
//! Surface tokens:
//! - Header keyword: `!muf`
//! - Block head: `[TAG name?]`
//! - Directive: `.op arg...`
//! - Block close: `..` (POP marker)
//! - Comment: `;; ...` (to end-of-line)
//! - Refs: `~name/name/...`
//! - Strings: `"..."` with escapes
//! - Numbers: int/float with optional sign and exponent

use std::fmt;

// -------------------------------------------------------------------------------------------------
// Spans
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Pos {
    /// 1-based
    pub line: usize,
    /// 1-based (byte-based; MUF syntax is ASCII by design)
    pub col: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub fn point(line: usize, col: usize) -> Self {
        Self {
            start: Pos { line, col },
            end: Pos { line, col },
        }
    }

    pub fn new(start: Pos, end: Pos) -> Self {
        Self { start, end }
    }
}

// -------------------------------------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (L{}:C{})",
            self.message, self.span.start.line, self.span.start.col
        )
    }
}

impl std::error::Error for LexError {}

// -------------------------------------------------------------------------------------------------
// Tokens
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // structure
    HeaderMuf,   // !muf
    LBracket,    // [
    RBracket,    // ]
    Dot,         // .
    Close,       // ..

    // atoms / words
    Name(String),
    Int { raw: String, value: i64 },
    Float { raw: String, value: f64 },
    Str(String),

    // path/ref
    Tilde, // ~
    Slash, // /

    // trivia
    Newline,
    Comment(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

// -------------------------------------------------------------------------------------------------
// Lexer
// -------------------------------------------------------------------------------------------------

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    i: usize,
    line: usize,
    col: usize,
    pub emit_comments: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            i: 0,
            line: 1,
            col: 1,
            emit_comments: false,
        }
    }

    pub fn with_comments(mut self, emit: bool) -> Self {
        self.emit_comments = emit;
        self
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        loop {
            self.skip_ws0();

            let start = self.pos();

            if self.eof() {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(start, start),
                });
            }

            // newline (\n or \r\n)
            if self.peek() == Some(b'\n') || self.peek() == Some(b'\r') {
                let span = self.lex_newline_span();
                return Ok(Token {
                    kind: TokenKind::Newline,
                    span,
                });
            }

            // comment line: `;; ...` (not in-string)
            if self.starts_with(b";;") {
                let (text, span) = self.lex_comment()?;
                if self.emit_comments {
                    return Ok(Token {
                        kind: TokenKind::Comment(text),
                        span,
                    });
                }
                // skip comment and continue
                continue;
            }

            // special: `..` close marker
            if self.starts_with(b"..") {
                let span = self.take_n(2);
                return Ok(Token {
                    kind: TokenKind::Close,
                    span,
                });
            }

            // header keyword `!muf`
            if self.starts_with(b"!muf") {
                let span = self.take_n(4);
                return Ok(Token {
                    kind: TokenKind::HeaderMuf,
                    span,
                });
            }

            // single-char tokens
            match self.peek().unwrap() {
                b'[' => {
                    let span = self.take_n(1);
                    return Ok(Token {
                        kind: TokenKind::LBracket,
                        span,
                    });
                }
                b']' => {
                    let span = self.take_n(1);
                    return Ok(Token {
                        kind: TokenKind::RBracket,
                        span,
                    });
                }
                b'.' => {
                    let span = self.take_n(1);
                    return Ok(Token {
                        kind: TokenKind::Dot,
                        span,
                    });
                }
                b'~' => {
                    let span = self.take_n(1);
                    return Ok(Token {
                        kind: TokenKind::Tilde,
                        span,
                    });
                }
                b'/' => {
                    let span = self.take_n(1);
                    return Ok(Token {
                        kind: TokenKind::Slash,
                        span,
                    });
                }
                b'"' => {
                    let (s, span) = self.lex_string()?;
                    return Ok(Token {
                        kind: TokenKind::Str(s),
                        span,
                    });
                }
                b'+' | b'-' => {
                    // number only (MUF doesn't allow signed names)
                    let (nk, span) = self.lex_number()?;
                    return Ok(Token { kind: nk, span });
                }
                b'0'..=b'9' => {
                    let (nk, span) = self.lex_number()?;
                    return Ok(Token { kind: nk, span });
                }
                _ => {}
            }

            // name
            if let Some(b) = self.peek() {
                if is_ident_start(b) {
                    let (name, span) = self.lex_name();
                    return Ok(Token {
                        kind: TokenKind::Name(name),
                        span,
                    });
                }
            }

            // unknown
            return Err(LexError {
                message: "unexpected byte".to_string(),
                span: Span::point(start.line, start.col),
            });
        }
    }

    pub fn lex_all(&mut self) -> Result<Vec<Token>, LexError> {
        let mut out = Vec::new();
        loop {
            let t = self.next_token()?;
            let is_eof = matches!(t.kind, TokenKind::Eof);
            out.push(t);
            if is_eof {
                break;
            }
        }
        Ok(out)
    }

    // ---------------------------------------------------------------------------------------------
    // internals
    // ---------------------------------------------------------------------------------------------

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn starts_with(&self, pat: &[u8]) -> bool {
        self.bytes.get(self.i..).map(|s| s.starts_with(pat)).unwrap_or(false)
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        self.col += 1;
        Some(b)
    }

    fn take_n(&mut self, n: usize) -> Span {
        let start = self.pos();
        for _ in 0..n {
            self.bump();
        }
        let end = Pos {
            line: self.line,
            col: self.col.saturating_sub(1),
        };
        Span::new(start, end)
    }

    fn skip_ws0(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.bump();
        }
    }

    fn lex_newline_span(&mut self) -> Span {
        let start = self.pos();
        if self.peek() == Some(b'\r') {
            self.bump();
            if self.peek() == Some(b'\n') {
                self.bump();
            }
        } else {
            self.bump();
        }
        // line increment and reset col
        self.line += 1;
        self.col = 1;
        let end = Pos {
            line: start.line,
            col: start.col,
        };
        Span::new(start, end)
    }

    fn lex_comment(&mut self) -> Result<(String, Span), LexError> {
        let start = self.pos();
        // consume leading `;;`
        self.bump();
        self.bump();

        let mut buf = String::new();
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b'\r' {
                break;
            }
            buf.push(b as char);
            self.bump();
        }
        let end = Pos {
            line: self.line,
            col: self.col.saturating_sub(1),
        };
        Ok((buf, Span::new(start, end)))
    }

    fn lex_name(&mut self) -> (String, Span) {
        let start = self.pos();
        let mut s = String::new();

        // first
        if let Some(b) = self.peek() {
            s.push(b as char);
            self.bump();
        }
        // rest
        while let Some(b) = self.peek() {
            if is_ident_cont(b) {
                s.push(b as char);
                self.bump();
            } else {
                break;
            }
        }

        let end = Pos {
            line: self.line,
            col: self.col.saturating_sub(1),
        };
        (s, Span::new(start, end))
    }

    fn lex_number(&mut self) -> Result<(TokenKind, Span), LexError> {
        let start = self.pos();
        let start_i = self.i;

        // sign
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.bump();
        }

        // digits
        let mut saw_digit = false;
        while let Some(b) = self.peek() {
            if (b'0'..=b'9').contains(&b) {
                saw_digit = true;
                self.bump();
            } else {
                break;
            }
        }

        if !saw_digit {
            return Err(LexError {
                message: "expected digits".to_string(),
                span: Span::point(start.line, start.col),
            });
        }

        // fraction
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            // lookahead digit (avoid eating close marker `..`)
            if self.bytes.get(self.i + 1).copied().map(|d| (b'0'..=b'9').contains(&d)).unwrap_or(false) {
                is_float = true;
                self.bump(); // '.'
                while let Some(b) = self.peek() {
                    if (b'0'..=b'9').contains(&b) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }

        // exponent
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            let d0 = self.peek().ok_or_else(|| LexError {
                message: "expected exponent digits".to_string(),
                span: Span::point(self.line, self.col),
            })?;
            if !(b'0'..=b'9').contains(&d0) {
                return Err(LexError {
                    message: "expected exponent digits".to_string(),
                    span: Span::point(self.line, self.col),
                });
            }
            while let Some(b) = self.peek() {
                if (b'0'..=b'9').contains(&b) {
                    self.bump();
                } else {
                    break;
                }
            }
        }

        let end = Pos {
            line: self.line,
            col: self.col.saturating_sub(1),
        };
        let raw = self.src[start_i..self.i].to_string();

        if is_float {
            let v = raw.parse::<f64>().map_err(|_| LexError {
                message: "invalid float".to_string(),
                span: Span::new(start, end),
            })?;
            Ok((TokenKind::Float { raw, value: v }, Span::new(start, end)))
        } else {
            let v = raw.parse::<i64>().map_err(|_| LexError {
                message: "invalid int".to_string(),
                span: Span::new(start, end),
            })?;
            Ok((TokenKind::Int { raw, value: v }, Span::new(start, end)))
        }
    }

    fn lex_string(&mut self) -> Result<(String, Span), LexError> {
        let start = self.pos();
        // opening quote
        self.bump();

        let mut out = String::new();
        while let Some(b) = self.peek() {
            match b {
                b'"' => {
                    self.bump();
                    let end = Pos {
                        line: self.line,
                        col: self.col.saturating_sub(1),
                    };
                    return Ok((out, Span::new(start, end)));
                }
                b'\n' | b'\r' => {
                    return Err(LexError {
                        message: "newline in string".to_string(),
                        span: Span::point(self.line, self.col),
                    });
                }
                b'\\' => {
                    self.bump();
                    let esc = self.peek().ok_or_else(|| LexError {
                        message: "unterminated escape".to_string(),
                        span: Span::point(self.line, self.col),
                    })?;
                    match esc {
                        b'"' => {
                            out.push('"');
                            self.bump();
                        }
                        b'\\' => {
                            out.push('\\');
                            self.bump();
                        }
                        b'n' => {
                            out.push('\n');
                            self.bump();
                        }
                        b'r' => {
                            out.push('\r');
                            self.bump();
                        }
                        b't' => {
                            out.push('\t');
                            self.bump();
                        }
                        b'0' => {
                            out.push('\0');
                            self.bump();
                        }
                        b'x' => {
                            self.bump();
                            let h1 = self.bump().ok_or_else(|| LexError {
                                message: "expected hex digit".to_string(),
                                span: Span::point(self.line, self.col),
                            })?;
                            let h2 = self.bump().ok_or_else(|| LexError {
                                message: "expected hex digit".to_string(),
                                span: Span::point(self.line, self.col),
                            })?;
                            let v = (hex_val(h1)? << 4) | hex_val(h2)?;
                            out.push(v as char);
                        }
                        b'u' => {
                            self.bump();
                            let mut v: u32 = 0;
                            for _ in 0..4 {
                                let h = self.bump().ok_or_else(|| LexError {
                                    message: "expected hex digit".to_string(),
                                    span: Span::point(self.line, self.col),
                                })?;
                                v = (v << 4) | (hex_val(h)? as u32);
                            }
                            let ch = char::from_u32(v).ok_or_else(|| LexError {
                                message: "invalid unicode escape".to_string(),
                                span: Span::point(self.line, self.col),
                            })?;
                            out.push(ch);
                        }
                        _ => {
                            return Err(LexError {
                                message: "unknown escape".to_string(),
                                span: Span::point(self.line, self.col),
                            });
                        }
                    }
                }
                _ => {
                    out.push(b as char);
                    self.bump();
                }
            }
        }

        Err(LexError {
            message: "unterminated string".to_string(),
            span: Span::point(start.line, start.col),
        })
    }
}

// -------------------------------------------------------------------------------------------------
// helpers
// -------------------------------------------------------------------------------------------------

fn is_ident_start(b: u8) -> bool {
    (b'A'..=b'Z').contains(&b) || (b'a'..=b'z').contains(&b) || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    is_ident_start(b) || (b'0'..=b'9').contains(&b)
}

fn hex_val(b: u8) -> Result<u8, LexError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(LexError {
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
    fn lexes_header_and_block() {
        let src = "!muf 4\n[WS x]\n  .root \".\"\n..\n";
        let mut lx = Lexer::new(src);
        let toks = lx.lex_all().unwrap();
        assert!(matches!(toks[0].kind, TokenKind::HeaderMuf));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::LBracket)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::Close)));
    }

    #[test]
    fn lexes_comment_and_skips_by_default() {
        let src = "!muf 4\n;; hello\n[WS x]\n..\n";
        let mut lx = Lexer::new(src);
        let toks = lx.lex_all().unwrap();
        assert!(!toks.iter().any(|t| matches!(t.kind, TokenKind::Comment(_))));
    }

    #[test]
    fn lexes_string_escapes() {
        let src = "!muf 4\n[WS x]\n  .t \"a\\n\\t\\\"b\"\n..\n";
        let mut lx = Lexer::new(src);
        let toks = lx.lex_all().unwrap();
        let mut saw = false;
        for t in toks {
            if let TokenKind::Str(s) = t.kind {
                saw = true;
                assert!(s.contains('\n'));
                assert!(s.contains('\t'));
                assert!(s.contains('"'));
            }
        }
        assert!(saw);
    }
}
