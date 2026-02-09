//! AST for MUF buildfiles.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Bool(bool),
    Ident(String),
    List(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineToken {
    pub kind: LineTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineTokenKind {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub tokens: Vec<LineToken>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Set {
        key: String,
        value: Value,
        span: Span,
    },
    Var {
        name: String,
        ty: TypeRef,
        value: Value,
        span: Span,
    },
    Tool {
        block: BlockStmt,
    },
    Bake {
        block: BlockStmt,
    },
    Capsule {
        block: BlockStmt,
    },
    Plan {
        block: BlockStmt,
    },
    Profile {
        block: BlockStmt,
    },
    Target {
        block: BlockStmt,
    },
    Store {
        block: BlockStmt,
    },
    Switch {
        block: BlockStmt,
    },
    Block {
        block: BlockStmt,
    },
    Line {
        line: Line,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockStmt {
    pub keyword: String,
    pub name: Option<String>,
    pub line: Line,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub segments: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub header: Option<Header>,
    pub stmts: Vec<Stmt>,
}
