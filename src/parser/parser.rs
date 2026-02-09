//! MUF parser.

use crate::parser::ast::{
    BlockStmt, File, Header, Line, LineToken, LineTokenKind, Span, Stmt, TypeRef, Value,
};
use crate::parser::lexer::{LexError, Lexer, Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        ParseError {
            message: err.message,
            span: err.span,
        }
    }
}

pub fn parse_muf(src: &str) -> Result<File, ParseError> {
    let mut parser = Parser::new(src);
    parser.parse_file()
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    pending_line: Option<Line>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            lexer: Lexer::new(src),
            lookahead: None,
            pending_line: None,
        }
    }

    fn parse_file(&mut self) -> Result<File, ParseError> {
        let mut header = None;
        self.skip_eol()?;
        if let Some(line) = self.next_line()? {
            if is_header_line(&line) {
                header = Some(self.parse_header_line(line)?);
            } else {
                self.pending_line = Some(line);
            }
        }

        let stmts = self.parse_block_body(None)?;
        Ok(File { header, stmts })
    }

    fn parse_header_line(&mut self, line: Line) -> Result<Header, ParseError> {
        let mut ts = TokenStream::new(&line.tokens);
        ts.expect_ident("muf")?;
        let version = match ts.next() {
            Some(LineToken { kind: LineTokenKind::Int(v), .. }) => v as u32,
            Some(tok) => {
                return Err(ParseError {
                    message: "expected version number".to_string(),
                    span: tok.span,
                });
            }
            None => {
                return Err(ParseError {
                    message: "missing version number".to_string(),
                    span: line.span,
                });
            }
        };
        if let Some(tok) = ts.next() {
            return Err(ParseError {
                message: "unexpected token after header".to_string(),
                span: tok.span,
            });
        }
        if version != 4 {
            return Err(ParseError {
                message: format!("unsupported MUF version {version} (expected 4)"),
                span: line.span,
            });
        }
        Ok(Header {
            version,
            span: line.span,
        })
    }

    fn parse_block_body(&mut self, until_end: Option<Span>) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            let Some(line) = self.next_line()? else {
                if let Some(span) = until_end {
                    return Err(ParseError {
                        message: "missing .end".to_string(),
                        span,
                    });
                }
                break;
            };

            if is_end_line(&line) {
                self.consume_line()?;
                if until_end.is_some() {
                    break;
                }
                return Err(ParseError {
                    message: "unexpected .end".to_string(),
                    span: line.span,
                });
            }

            let stmt = self.parse_stmt_from_line(line)?;
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    fn parse_stmt_from_line(&mut self, line: Line) -> Result<Stmt, ParseError> {
        let Some(first) = line.tokens.first() else {
            return Ok(Stmt::Line { line });
        };

        let keyword = match &first.kind {
            LineTokenKind::Ident(s) => s.as_str(),
            _ => {
                return Err(ParseError {
                    message: "expected keyword".to_string(),
                    span: first.span,
                });
            }
        };

        if keyword == "set" {
            return self.parse_set(line);
        }

        if keyword == "var" {
            return self.parse_var(line);
        }

        if is_block_keyword(keyword) {
            let span = line.span;
            let body = self.parse_block_body(Some(span))?;
            let name = block_name_from_line(&line);
            let block = BlockStmt {
                keyword: keyword.to_string(),
                name,
                line,
                body,
                span,
            };
            return Ok(match block.keyword.as_str() {
                "tool" => Stmt::Tool { block },
                "bake" => Stmt::Bake { block },
                "capsule" => Stmt::Capsule { block },
                "plan" => Stmt::Plan { block },
                "profile" => Stmt::Profile { block },
                "target" => Stmt::Target { block },
                "store" => Stmt::Store { block },
                "switch" => Stmt::Switch { block },
                _ => Stmt::Block { block },
            });
        }

        Ok(Stmt::Line { line })
    }

    fn parse_set(&self, line: Line) -> Result<Stmt, ParseError> {
        let mut ts = TokenStream::new(&line.tokens);
        ts.expect_ident("set")?;
        let key = match ts.next() {
            Some(LineToken {
                kind: LineTokenKind::Ident(s),
                ..
            }) => s,
            Some(tok) => {
                return Err(ParseError {
                    message: "expected key".to_string(),
                    span: tok.span,
                });
            }
            None => {
                return Err(ParseError {
                    message: "missing key".to_string(),
                    span: line.span,
                });
            }
        };
        let value = parse_value(&mut ts)?;
        if let Some(tok) = ts.next() {
            return Err(ParseError {
                message: "unexpected token after value".to_string(),
                span: tok.span,
            });
        }
        Ok(Stmt::Set {
            key,
            value,
            span: line.span,
        })
    }

    fn parse_var(&self, line: Line) -> Result<Stmt, ParseError> {
        let mut ts = TokenStream::new(&line.tokens);
        ts.expect_ident("var")?;
        let name = match ts.next() {
            Some(LineToken {
                kind: LineTokenKind::Ident(s),
                ..
            }) => s,
            Some(tok) => {
                return Err(ParseError {
                    message: "expected variable name".to_string(),
                    span: tok.span,
                });
            }
            None => {
                return Err(ParseError {
                    message: "missing variable name".to_string(),
                    span: line.span,
                });
            }
        };
        ts.expect_punct(LineTokenKind::Colon, "expected ':'")?;
        let ty = parse_type_ref(&mut ts)?;
        ts.expect_punct(LineTokenKind::Equal, "expected '='")?;
        let value = parse_value(&mut ts)?;
        if let Some(tok) = ts.next() {
            return Err(ParseError {
                message: "unexpected token after value".to_string(),
                span: tok.span,
            });
        }
        Ok(Stmt::Var {
            name,
            ty,
            value,
            span: line.span,
        })
    }

    fn consume_line(&mut self) -> Result<Line, ParseError> {
        let mut tokens = Vec::new();
        let mut start = None;
        let mut end = None;

        loop {
            let tok = self.next_token()?;
            match tok.kind {
                TokenKind::Eol => {
                    break;
                }
                TokenKind::Eof => {
                    break;
                }
                _ => {
                    if start.is_none() {
                        start = Some(tok.span.start);
                    }
                    end = Some(tok.span.end);
                    tokens.push(LineToken::from(tok));
                }
            }
        }

        let start = start.unwrap_or_else(|| self.lexer.position());
        let end = end.unwrap_or(start);
        Ok(Line {
            tokens,
            span: Span { start, end },
        })
    }

    fn next_line(&mut self) -> Result<Option<Line>, ParseError> {
        if let Some(line) = self.pending_line.take() {
            return Ok(Some(line));
        }

        loop {
            self.skip_eol()?;
            let tok = self.peek_token()?;
            if matches!(tok.kind, TokenKind::Eof) {
                return Ok(None);
            }
            let line = self.consume_line()?;
            if line.tokens.is_empty() {
                continue;
            }
            return Ok(Some(line));
        }
    }

    fn skip_eol(&mut self) -> Result<(), ParseError> {
        loop {
            let tok = self.peek_token()?;
            match tok.kind {
                TokenKind::Eol => {
                    self.next_token()?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn peek_token(&mut self) -> Result<Token, ParseError> {
        if let Some(tok) = self.lookahead.clone() {
            return Ok(tok);
        }
        let tok = self.lexer.next_token()?;
        self.lookahead = Some(tok.clone());
        Ok(tok)
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        if let Some(tok) = self.lookahead.take() {
            return Ok(tok);
        }
        Ok(self.lexer.next_token()?)
    }

}

impl From<Token> for LineToken {
    fn from(tok: Token) -> Self {
        let kind = match tok.kind {
            TokenKind::Ident(s) => LineTokenKind::Ident(s),
            TokenKind::Int(v) => LineTokenKind::Int(v),
            TokenKind::Str(s) => LineTokenKind::Str(s),
            TokenKind::Dot => LineTokenKind::Dot,
            TokenKind::Colon => LineTokenKind::Colon,
            TokenKind::Equal => LineTokenKind::Equal,
            TokenKind::Comma => LineTokenKind::Comma,
            TokenKind::LBracket => LineTokenKind::LBracket,
            TokenKind::RBracket => LineTokenKind::RBracket,
            TokenKind::Arrow => LineTokenKind::Arrow,
            TokenKind::Eol | TokenKind::Eof => LineTokenKind::Ident("".to_string()),
        };
        LineToken {
            kind,
            span: tok.span,
        }
    }
}

fn is_header_line(line: &Line) -> bool {
    if line.tokens.len() != 2 {
        return false;
    }
    matches!(&line.tokens[0].kind, LineTokenKind::Ident(s) if s == "muf")
        && matches!(line.tokens[1].kind, LineTokenKind::Int(_))
}

fn is_end_line(line: &Line) -> bool {
    if line.tokens.len() != 2 {
        return false;
    }
    matches!(line.tokens[0].kind, LineTokenKind::Dot)
        && matches!(&line.tokens[1].kind, LineTokenKind::Ident(s) if s == "end")
}

fn is_block_keyword(kw: &str) -> bool {
    matches!(
        kw,
        "store"
            | "capsule"
            | "profile"
            | "tool"
            | "bake"
            | "plan"
            | "switch"
            | "project"
            | "stores"
            | "capsules"
            | "bakes"
            | "vars"
            | "tools"
            | "targets"
            | "profiles"
            | "target"
            | "exports"
            | "wires"
            | "dirs"
            | "defaults"
            | "host"
            | "paths"
            | "takes"
            | "emits"
            | "do"
            | "run"
            | "args"
            | "cmd"
            | "make"
            | "node"
            | "port"
            | "in"
            | "out"
            | "flag"
            | "env"
            | "fs"
            | "time"
    )
}

fn block_name_from_line(line: &Line) -> Option<String> {
    line.tokens.get(1).and_then(|tok| match &tok.kind {
        LineTokenKind::Ident(name) => Some(name.clone()),
        _ => None,
    })
}

struct TokenStream<'a> {
    tokens: &'a [LineToken],
    pos: usize,
}

impl<'a> TokenStream<'a> {
    fn new(tokens: &'a [LineToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn next(&mut self) -> Option<LineToken> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn peek(&self) -> Option<&LineToken> {
        self.tokens.get(self.pos)
    }

    fn expect_ident(&mut self, expected: &str) -> Result<(), ParseError> {
        match self.next() {
            Some(LineToken {
                kind: LineTokenKind::Ident(s),
                span: _span,
            }) if s == expected => Ok(()),
            Some(tok) => Err(ParseError {
                message: format!("expected '{expected}'"),
                span: tok.span,
            }),
            None => Err(ParseError {
                message: format!("expected '{expected}'"),
                span: Span {
                    start: self.last_pos(),
                    end: self.last_pos(),
                },
            }),
        }
    }

    fn expect_punct(&mut self, kind: LineTokenKind, msg: &str) -> Result<(), ParseError> {
        match self.next() {
            Some(tok) if tok.kind == kind => Ok(()),
            Some(tok) => Err(ParseError {
                message: msg.to_string(),
                span: tok.span,
            }),
            None => Err(ParseError {
                message: msg.to_string(),
                span: Span {
                    start: self.last_pos(),
                    end: self.last_pos(),
                },
            }),
        }
    }

    fn last_pos(&self) -> crate::parser::ast::Position {
        if let Some(tok) = self.tokens.get(self.pos.saturating_sub(1)) {
            tok.span.end
        } else if let Some(tok) = self.tokens.first() {
            tok.span.start
        } else {
            crate::parser::ast::Position { line: 1, col: 1 }
        }
    }
}

fn parse_value(ts: &mut TokenStream<'_>) -> Result<Value, ParseError> {
    let Some(tok) = ts.next() else {
        return Err(ParseError {
            message: "expected value".to_string(),
            span: Span {
                start: ts.last_pos(),
                end: ts.last_pos(),
            },
        });
    };
    match tok.kind {
        LineTokenKind::Str(s) => Ok(Value::String(s)),
        LineTokenKind::Int(v) => Ok(Value::Int(v)),
        LineTokenKind::Ident(s) if s == "true" => Ok(Value::Bool(true)),
        LineTokenKind::Ident(s) if s == "false" => Ok(Value::Bool(false)),
        LineTokenKind::Ident(s) => Ok(Value::Ident(s)),
        LineTokenKind::LBracket => parse_list(ts, tok.span),
        _ => Err(ParseError {
            message: "expected value".to_string(),
            span: tok.span,
        }),
    }
}

fn parse_list(ts: &mut TokenStream<'_>, start: Span) -> Result<Value, ParseError> {
    let mut items = Vec::new();
    loop {
        match ts.peek() {
            Some(LineToken {
                kind: LineTokenKind::RBracket,
                ..
            }) => {
                ts.next();
                break;
            }
            None => {
                return Err(ParseError {
                    message: "unterminated list".to_string(),
                    span: start,
                });
            }
            _ => {
                let val = parse_value(ts)?;
                items.push(val);
                if let Some(LineToken {
                    kind: LineTokenKind::Comma,
                    ..
                }) = ts.peek()
                {
                    ts.next();
                }
            }
        }
    }
    Ok(Value::List(items))
}

fn parse_type_ref(ts: &mut TokenStream<'_>) -> Result<TypeRef, ParseError> {
    let mut segments = Vec::new();
    let first = match ts.next() {
        Some(LineToken {
            kind: LineTokenKind::Ident(s),
            span,
        }) => {
            segments.push(s);
            span
        }
        Some(tok) => {
            return Err(ParseError {
                message: "expected type name".to_string(),
                span: tok.span,
            });
        }
        None => {
            return Err(ParseError {
                message: "expected type name".to_string(),
                span: Span {
                    start: ts.last_pos(),
                    end: ts.last_pos(),
                },
            });
        }
    };

    let mut end = first.end;
    loop {
        match ts.peek() {
            Some(LineToken {
                kind: LineTokenKind::Dot,
                ..
            }) => {
                ts.next();
                match ts.next() {
                    Some(LineToken {
                        kind: LineTokenKind::Ident(s),
                        span,
                    }) => {
                        segments.push(s);
                        end = span.end;
                    }
                    Some(tok) => {
                        return Err(ParseError {
                            message: "expected type segment".to_string(),
                            span: tok.span,
                        });
                    }
                    None => {
                        return Err(ParseError {
                            message: "expected type segment".to_string(),
                            span: Span {
                                start: ts.last_pos(),
                                end: ts.last_pos(),
                            },
                        });
                    }
                }
            }
            _ => break,
        }
    }
    Ok(TypeRef {
        segments,
        span: Span {
            start: first.start,
            end,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_set() {
        let src = "muf 4\nset profile \"debug\"\n";
        let file = parse_muf(src).unwrap();
        assert_eq!(file.header.as_ref().unwrap().version, 4);
        assert_eq!(file.stmts.len(), 1);
        match &file.stmts[0] {
            Stmt::Set { key, .. } => assert_eq!(key, "profile"),
            _ => panic!("expected set"),
        }
    }

    #[test]
    fn parses_var_and_list() {
        let src = "var flags: text = [\"a\", \"b\"]\n";
        let file = parse_muf(src).unwrap();
        assert!(file.header.is_none());
        assert_eq!(file.stmts.len(), 1);
        match &file.stmts[0] {
            Stmt::Var { name, value, .. } => {
                assert_eq!(name, "flags");
                match value {
                    Value::List(items) => assert_eq!(items.len(), 2),
                    _ => panic!("expected list"),
                }
            }
            _ => panic!("expected var"),
        }
    }

    #[test]
    fn parses_block() {
        let src = "bake build\n  set profile \"debug\"\n.end\n";
        let file = parse_muf(src).unwrap();
        assert_eq!(file.stmts.len(), 1);
        match &file.stmts[0] {
            Stmt::Bake { block } => assert_eq!(block.body.len(), 1),
            _ => panic!("expected block"),
        }
    }
}
