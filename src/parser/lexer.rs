//! MUF lexer.

use crate::parser::ast::{Position, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Int(i64),
    Str(String),
    Dot,
    Colon,
    Equal,
    Comma,
    LBracket,
    RBracket,
    Arrow,
    Eol,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub struct Lexer<'a> {
    iter: std::str::CharIndices<'a>,
    peeked: Option<(usize, char)>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            iter: src.char_indices(),
            peeked: None,
            line: 1,
            col: 1,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_ws();

        let start = self.pos();
        let Some(ch) = self.peek_char() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span {
                    start,
                    end: start,
                },
            });
        };

        if ch == '\n' {
            self.bump_char();
            let end = self.pos();
            return Ok(Token {
                kind: TokenKind::Eol,
                span: Span { start, end },
            });
        }

        if ch == '#' {
            self.skip_comment();
            return self.next_token();
        }

        if ch == '!' {
            self.bump_char();
            let Some(next) = self.peek_char() else {
                return Err(LexError {
                    message: "unexpected '!'".to_string(),
                    span: Span { start, end: self.pos() },
                });
            };
            if !is_ident_start(next) {
                return Err(LexError {
                    message: "expected identifier after '!'".to_string(),
                    span: Span { start, end: self.pos() },
                });
            }
        }

        if is_ident_start(ch) || ch == '!' {
            let mut ident = String::new();
            if ch != '!' {
                ident.push(ch);
                self.bump_char();
            }
            while let Some(c) = self.peek_char() {
                if is_ident_part(c) {
                    ident.push(c);
                    self.bump_char();
                } else {
                    break;
                }
            }
            let end = self.pos();
            return Ok(Token {
                kind: TokenKind::Ident(ident),
                span: Span { start, end },
            });
        }

        if ch.is_ascii_digit() {
            let mut digits = String::new();
            digits.push(ch);
            self.bump_char();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    digits.push(c);
                    self.bump_char();
                } else {
                    break;
                }
            }
            let end = self.pos();
            let val = digits.parse::<i64>().map_err(|e| LexError {
                message: format!("invalid int: {e}"),
                span: Span { start, end },
            })?;
            return Ok(Token {
                kind: TokenKind::Int(val),
                span: Span { start, end },
            });
        }

        if ch == '"' {
            self.bump_char();
            let mut out = String::new();
            loop {
                let Some(c) = self.peek_char() else {
                    return Err(LexError {
                        message: "unterminated string".to_string(),
                        span: Span {
                            start,
                            end: self.pos(),
                        },
                    });
                };
                if c == '"' {
                    self.bump_char();
                    break;
                }
                if c == '\\' {
                    self.bump_char();
                    let Some(esc) = self.peek_char() else {
                        return Err(LexError {
                            message: "unterminated escape".to_string(),
                            span: Span {
                                start,
                                end: self.pos(),
                            },
                        });
                    };
                    let mapped = match esc {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        other => {
                            return Err(LexError {
                                message: format!("invalid escape: \\{other}"),
                                span: Span {
                                    start,
                                    end: self.pos(),
                                },
                            });
                        }
                    };
                    out.push(mapped);
                    self.bump_char();
                    continue;
                }
                out.push(c);
                self.bump_char();
            }
            let end = self.pos();
            return Ok(Token {
                kind: TokenKind::Str(out),
                span: Span { start, end },
            });
        }

        let tok = match ch {
            '.' => {
                self.bump_char();
                TokenKind::Dot
            }
            ':' => {
                self.bump_char();
                TokenKind::Colon
            }
            '=' => {
                self.bump_char();
                TokenKind::Equal
            }
            ',' => {
                self.bump_char();
                TokenKind::Comma
            }
            '[' => {
                self.bump_char();
                TokenKind::LBracket
            }
            ']' => {
                self.bump_char();
                TokenKind::RBracket
            }
            '-' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    TokenKind::Arrow
                } else {
                    return Err(LexError {
                        message: "unexpected '-'".to_string(),
                        span: Span {
                            start,
                            end: self.pos(),
                        },
                    });
                }
            }
            other => {
                return Err(LexError {
                    message: format!("unexpected character: {other}"),
                    span: Span {
                        start,
                        end: self.pos(),
                    },
                });
            }
        };
        let end = self.pos();
        Ok(Token {
            kind: tok,
            span: Span { start, end },
        })
    }

    pub fn position(&self) -> Position {
        Position {
            line: self.line,
            col: self.col,
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.bump_char();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            self.bump_char();
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.peeked.is_none() {
            self.peeked = self.iter.next();
        }
        self.peeked.map(|(_, c)| c)
    }

    fn bump_char(&mut self) -> Option<char> {
        let next = if let Some((_, c)) = self.peeked.take() {
            Some(c)
        } else {
            self.iter.next().map(|(_, c)| c)
        };

        if let Some(c) = next {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        next
    }

    fn pos(&self) -> Position {
        Position {
            line: self.line,
            col: self.col,
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
