//! token.rs 
//!
//! Tokens pour MCFG (parser `.mff` / `.muff`) et, plus largement, entrée Steel (buildfile).
//!
//! Objectifs :
//! - std-only, ultra déterministe
//! - tokens avec Span (multi-fichiers) + trivia (whitespace/comment) optionnel
//! - support: ident, keywords, string, int, bool, punctuation, operators, path-ish
//! - utilities: pretty debug, keyword table, classification, tests
//!
//! Dépend de : crate::span::{Span, FileId, Pos} (ou équivalent).
//! Optionnel: crate::diag si tu veux pousser erreurs lexicales ailleurs.

use std::cmp::min;
use std::fmt;

use crate::span::{FileId, Pos, Span};

/// ------------------------------------------------------------
/// Token model
/// ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: Option<String>, // pour ident/string (sinon None)
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span, text: None }
    }
    pub fn with_text(kind: TokenKind, span: Span, text: impl Into<String>) -> Self {
        Self { kind, span, text: Some(text.into()) }
    }

    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Whitespace | TokenKind::Newline | TokenKind::Comment)
    }

    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }
}

/// Le core: kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // --- Trivia
    Whitespace, // spaces/tabs (no newline)
    Newline,    // \n or \r\n
    Comment,    // # ... (line comment)

    // --- Literals
    Ident,
    IntLit,
    BoolLit,
    StringLit,

    // --- Keywords (MCFG/Steel family)
    KwFlan,
    KwBake,
    KwStore,
    KwCapsule,
    KwVar,
    KwProfile,
    KwTool,
    KwPlan,
    KwSwitch,
    KwSet,
    KwRun,
    KwExports,
    KwWire,
    KwExport,
    KwIn,
    KwOut,
    KwMake,
    KwTakes,
    KwEmits,
    KwOutput,
    KwAt,
    KwCache,
    KwMode,
    KwPath,
    KwEnv,
    KwFs,
    KwNet,
    KwTime,
    KwAllow,
    KwDeny,
    KwAllowRead,
    KwAllowWrite,
    KwAllowWriteExact,
    KwStable,

    // --- Punctuation / operators
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]
    LBrace,     // {
    RBrace,     // }
    Comma,      // ,
    Dot,        // .
    Colon,      // :
    Semi,       // ;
    Eq,         // =
    Arrow,      // ->
    Slash,      // /
    Star,       // *
    Plus,       // +
    Minus,      // -
    Pipe,       // |
    Amp,        // &
    Lt,         // <
    Gt,         // >
    Bang,       // !
    Question,   // ?
    Underscore, // _

    // --- Special
    EndBlock, // `.end` (token unique)
    Unknown(char),
    Eof,
}

impl TokenKind {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::KwFlan
                | TokenKind::KwBake
                | TokenKind::KwStore
                | TokenKind::KwCapsule
                | TokenKind::KwVar
                | TokenKind::KwProfile
                | TokenKind::KwTool
                | TokenKind::KwPlan
                | TokenKind::KwSwitch
                | TokenKind::KwSet
                | TokenKind::KwRun
                | TokenKind::KwExports
                | TokenKind::KwWire
                | TokenKind::KwExport
                | TokenKind::KwIn
                | TokenKind::KwOut
                | TokenKind::KwMake
                | TokenKind::KwTakes
                | TokenKind::KwEmits
                | TokenKind::KwOutput
                | TokenKind::KwAt
                | TokenKind::KwCache
                | TokenKind::KwMode
                | TokenKind::KwPath
                | TokenKind::KwEnv
                | TokenKind::KwFs
                | TokenKind::KwNet
                | TokenKind::KwTime
                | TokenKind::KwAllow
                | TokenKind::KwDeny
                | TokenKind::KwAllowRead
                | TokenKind::KwAllowWrite
                | TokenKind::KwAllowWriteExact
                | TokenKind::KwStable
        )
    }

    pub fn display_name(&self) -> &'static str {
        use TokenKind::*;
        match self {
            Whitespace => "whitespace",
            Newline => "newline",
            Comment => "comment",
            Ident => "ident",
            IntLit => "int",
            BoolLit => "bool",
            StringLit => "string",
            KwFlan => "steel",
            KwBake => "bake",
            KwStore => "store",
            KwCapsule => "capsule",
            KwVar => "var",
            KwProfile => "profile",
            KwTool => "tool",
            KwPlan => "plan",
            KwSwitch => "switch",
            KwSet => "set",
            KwRun => "run",
            KwExports => "exports",
            KwWire => "wire",
            KwExport => "export",
            KwIn => "in",
            KwOut => "out",
            KwMake => "make",
            KwTakes => "takes",
            KwEmits => "emits",
            KwOutput => "output",
            KwAt => "at",
            KwCache => "cache",
            KwMode => "mode",
            KwPath => "path",
            KwEnv => "env",
            KwFs => "fs",
            KwNet => "net",
            KwTime => "time",
            KwAllow => "allow",
            KwDeny => "deny",
            KwAllowRead => "allow_read",
            KwAllowWrite => "allow_write",
            KwAllowWriteExact => "allow_write_exact",
            KwStable => "stable",
            LParen => "(",
            RParen => ")",
            LBracket => "[",
            RBracket => "]",
            LBrace => "{",
            RBrace => "}",
            Comma => ",",
            Dot => ".",
            Colon => ":",
            Semi => ";",
            Eq => "=",
            Arrow => "->",
            Slash => "/",
            Star => "*",
            Plus => "+",
            Minus => "-",
            Pipe => "|",
            Amp => "&",
            Lt => "<",
            Gt => ">",
            Bang => "!",
            Question => "?",
            Underscore => "_",
            EndBlock => ".end",
            Unknown(_) => "unknown",
            Eof => "eof",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Unknown(c) => write!(f, "unknown({:?})", c),
            _ => write!(f, "{}", self.display_name()),
        }
    }
}

/// ------------------------------------------------------------
/// Keyword table
/// ------------------------------------------------------------

pub fn keyword_kind(s: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match s {
        "steel" => KwFlan,
        "bake" => KwBake,
        "store" => KwStore,
        "capsule" => KwCapsule,
        "var" => KwVar,
        "profile" => KwProfile,
        "tool" => KwTool,
        "plan" => KwPlan,
        "switch" => KwSwitch,
        "set" => KwSet,
        "run" => KwRun,
        "exports" => KwExports,
        "wire" => KwWire,
        "export" => KwExport,
        "in" => KwIn,
        "out" => KwOut,
        "make" => KwMake,
        "takes" => KwTakes,
        "emits" => KwEmits,
        "output" => KwOutput,
        "at" => KwAt,
        "cache" => KwCache,
        "mode" => KwMode,
        "path" => KwPath,
        "env" => KwEnv,
        "fs" => KwFs,
        "net" => KwNet,
        "time" => KwTime,
        "allow" => KwAllow,
        "deny" => KwDeny,
        "allow_read" => KwAllowRead,
        "allow_write" => KwAllowWrite,
        "allow_write_exact" => KwAllowWriteExact,
        "stable" => KwStable,
        _ => return None,
    })
}

/// ------------------------------------------------------------
/// Token stream + cursor helpers
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TokenStream {
    pub file: FileId,
    pub tokens: Vec<Token>,
}

impl TokenStream {
    pub fn new(file: FileId) -> Self {
        Self { file, tokens: Vec::new() }
    }

    pub fn push(&mut self, tok: Token) {
        self.tokens.push(tok);
    }

    pub fn eof_span(&self) -> Span {
        if let Some(t) = self.tokens.last() {
            let hi = t.span.hi;
            Span::new(self.file, hi, hi)
        } else {
            Span::new(self.file, Pos(0), Pos(0))
        }
    }

    pub fn ensure_eof(&mut self) {
        if self.tokens.last().map(|t| t.kind.clone()) != Some(TokenKind::Eof) {
            let sp = self.eof_span();
            self.tokens.push(Token::new(TokenKind::Eof, sp));
        }
    }
}

/// Cursor sur tokens (parser)
#[derive(Debug, Clone)]
pub struct TokCursor<'a> {
    pub ts: &'a TokenStream,
    pub i: usize,
}

impl<'a> TokCursor<'a> {
    pub fn new(ts: &'a TokenStream) -> Self {
        Self { ts, i: 0 }
    }

    pub fn peek(&self) -> &'a Token {
        self.ts.tokens.get(self.i).unwrap_or_else(|| {
            // safe: assume ensure_eof called; fallback
            self.ts.tokens.last().expect("empty token stream")
        })
    }

    pub fn nth(&self, n: usize) -> &'a Token {
        self.ts.tokens.get(self.i + n).unwrap_or_else(|| self.peek())
    }

    pub fn bump(&mut self) -> &'a Token {
        let t = self.peek();
        self.i = min(self.i + 1, self.ts.tokens.len());
        t
    }

    pub fn eat_trivia(&mut self) {
        while self.peek().is_trivia() {
            self.bump();
        }
    }

    pub fn at(&self, k: &TokenKind) -> bool {
        &self.peek().kind == k
    }

    pub fn at_any(&self, ks: &[TokenKind]) -> bool {
        ks.iter().any(|k| *k == self.peek().kind)
    }

    pub fn span_here(&self) -> Span {
        self.peek().span
    }
}

/// ------------------------------------------------------------
/// Mini lexer (std-only) — optionnel mais utile
/// ------------------------------------------------------------
///
/// Le lexer produit les tokens utilisés par parser.rs.
/// Règles:
// - comments: `#` jusqu’à fin de ligne
// - strings: " ... " avec escapes \n \r \t \" \\
// - ints: -? [0-9]+ (parser décidera si allowed)
// - bool: true/false => BoolLit
// - `.end` => EndBlock (token unique)
// - ident: [A-Za-z_][A-Za-z0-9_.-]* (inclut '.' '-' pour types/ref path)
// - punct: ()[]{} , . : ; = -> / * + - | & < > ! ? _

pub fn lex(file: FileId, input: &str) -> TokenStream {
    let mut lx = Lexer::new(file, input);
    lx.tokenize()
}

#[derive(Debug)]
struct Lexer<'a> {
    file: FileId,
    s: &'a str,
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(file: FileId, s: &'a str) -> Self {
        Self { file, s, i: 0 }
    }

    fn tokenize(&mut self) -> TokenStream {
        let mut ts = TokenStream::new(self.file);

        while !self.eof() {
            let c = self.peek();

            // newline
            if c == '\n' {
                let lo = self.pos();
                self.bump_char();
                ts.push(Token::new(TokenKind::Newline, Span::new(self.file, lo, self.pos())));
                continue;
            }
            // \r\n
            if c == '\r' {
                let lo = self.pos();
                self.bump_char();
                if !self.eof() && self.peek() == '\n' {
                    self.bump_char();
                }
                ts.push(Token::new(TokenKind::Newline, Span::new(self.file, lo, self.pos())));
                continue;
            }

            // whitespace (spaces/tabs)
            if c == ' ' || c == '\t' {
                let lo = self.pos();
                self.bump_while(|ch| ch == ' ' || ch == '\t');
                ts.push(Token::new(TokenKind::Whitespace, Span::new(self.file, lo, self.pos())));
                continue;
            }

            // comment '# ...'
            if c == '#' {
                let lo = self.pos();
                self.bump_char();
                self.bump_while(|ch| ch != '\n' && ch != '\r');
                ts.push(Token::new(TokenKind::Comment, Span::new(self.file, lo, self.pos())));
                continue;
            }

            // string
            if c == '"' {
                let lo = self.pos();
                let text = self.read_string();
                let sp = Span::new(self.file, lo, self.pos());
                ts.push(Token::with_text(TokenKind::StringLit, sp, text));
                continue;
            }

            // `.end`
            if c == '.' && self.peek_str(".end") {
                let lo = self.pos();
                self.i += 4;
                let sp = Span::new(self.file, lo, self.pos());
                ts.push(Token::new(TokenKind::EndBlock, sp));
                continue;
            }

            // arrow ->
            if c == '-' && self.peek_str("->") {
                let lo = self.pos();
                self.i += 2;
                ts.push(Token::new(TokenKind::Arrow, Span::new(self.file, lo, self.pos())));
                continue;
            }

            // int (optional leading -)
            if c.is_ascii_digit() || (c == '-' && self.peek_next_is_digit()) {
                let lo = self.pos();
                if c == '-' {
                    self.bump_char();
                }
                self.bump_while(|ch| ch.is_ascii_digit());
                let sp = Span::new(self.file, lo, self.pos());
                let txt = &self.s[lo.0 as usize..self.pos().0 as usize];
                ts.push(Token::with_text(TokenKind::IntLit, sp, txt.to_string()));
                continue;
            }

            // ident/keyword/bool
            if is_ident_start(c) {
                let lo = self.pos();
                self.bump_char();
                self.bump_while(|ch| is_ident_continue(ch));
                let sp = Span::new(self.file, lo, self.pos());
                let txt = &self.s[lo.0 as usize..self.pos().0 as usize];

                if txt == "true" || txt == "false" {
                    ts.push(Token::with_text(TokenKind::BoolLit, sp, txt.to_string()));
                } else if let Some(kw) = keyword_kind(txt) {
                    ts.push(Token::new(kw, sp));
                } else {
                    ts.push(Token::with_text(TokenKind::Ident, sp, txt.to_string()));
                }
                continue;
            }

            // punctuation single-char
            let lo = self.pos();
            let kind = match c {
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '[' => TokenKind::LBracket,
                ']' => TokenKind::RBracket,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                ',' => TokenKind::Comma,
                '.' => TokenKind::Dot,
                ':' => TokenKind::Colon,
                ';' => TokenKind::Semi,
                '=' => TokenKind::Eq,
                '/' => TokenKind::Slash,
                '*' => TokenKind::Star,
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
                '|' => TokenKind::Pipe,
                '&' => TokenKind::Amp,
                '<' => TokenKind::Lt,
                '>' => TokenKind::Gt,
                '!' => TokenKind::Bang,
                '?' => TokenKind::Question,
                '_' => TokenKind::Underscore,
                other => TokenKind::Unknown(other),
            };
            self.bump_char();
            ts.push(Token::new(kind, Span::new(self.file, lo, self.pos())));
        }

        let at = self.pos();
        ts.push(Token::new(TokenKind::Eof, Span::new(self.file, at, at)));
        ts
    }

    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn pos(&self) -> Pos {
        Pos(self.i as u32)
    }

    fn peek(&self) -> char {
        self.s[self.i..].chars().next().unwrap_or('\0')
    }

    fn bump_char(&mut self) {
        if self.eof() {
            return;
        }
        let ch = self.peek();
        self.i += ch.len_utf8();
    }

    fn bump_while<F: Fn(char) -> bool>(&mut self, f: F) {
        while !self.eof() {
            let ch = self.peek();
            if !f(ch) {
                break;
            }
            self.bump_char();
        }
    }

    fn peek_str(&self, pat: &str) -> bool {
        self.s[self.i..].starts_with(pat)
    }

    fn peek_next_is_digit(&self) -> bool {
        if self.i + 1 >= self.s.len() {
            return false;
        }
        self.s[self.i + 1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
    }

    fn read_string(&mut self) -> String {
        // assumes current is '"'
        self.bump_char();
        let mut out = String::new();
        let mut esc = false;

        while !self.eof() {
            let ch = self.peek();
            self.bump_char();

            if esc {
                match ch {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    x => out.push(x),
                }
                esc = false;
                continue;
            }

            if ch == '\\' {
                esc = true;
                continue;
            }

            if ch == '"' {
                break;
            }

            out.push(ch);
        }

        out
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' // allow dotted paths
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{FileMap};

    #[test]
    fn lex_keywords_and_endblock() {
        let mut fm = FileMap::new();
        let fid = fm.add_file("steelconf", r#"steel bake 2
store a
  path "x"
.end
"#);

        let f = fm.get(fid).unwrap();
        let ts = lex(fid, &f.text);

        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::KwFlan));
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::KwBake));
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::KwStore));
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::EndBlock));
        assert_eq!(ts.tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn lex_string_and_comment() {
        let fid = FileId(1);
        let ts = lex(fid, r#"path "a\nb" # comment
"#);

        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::StringLit));
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn lex_arrow_and_int() {
        let fid = FileId(1);
        let ts = lex(fid, "wire a.b -> c.d\nvar x: int = -42\n");
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::Arrow));
        assert!(ts.tokens.iter().any(|t| t.kind == TokenKind::IntLit));
    }

    #[test]
    fn keyword_table() {
        assert_eq!(keyword_kind("store"), Some(TokenKind::KwStore));
        assert_eq!(keyword_kind("allow_write_exact"), Some(TokenKind::KwAllowWriteExact));
        assert_eq!(keyword_kind("nope"), None);
    }
}
