//! MUF v4.1 AST ("Bracket + Dot Ops", no `.end`)
//!
//! This module defines the canonical in-memory representation of a MUF file.
//! Parsing/lexing live in dedicated modules; the AST is stable and compiler-friendly.
//!
//! Surface recap (v4.1):
//! - Header:     `!muf 4`
//! - Block head: `[TAG name?]`
//! - Directive:  `.op arg1 arg2 ...`
//! - Close:      `..` closes the most recent open block
//! - Comment:    `;; ...` line comment (ignored by AST)

use core::fmt;

// -------------------------------------------------------------------------------------------------
// Positions & spans
// -------------------------------------------------------------------------------------------------

/// 1-based (line, column) position.
///
/// Column is byte-based. MUF syntax is ASCII by design; this keeps spans cheap.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    #[inline]
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Inclusive start, inclusive end.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    #[inline]
    pub const fn new(start: Pos, end: Pos) -> Self {
        Self { start, end }
    }

    #[inline]
    pub const fn point(line: usize, col: usize) -> Self {
        Self {
            start: Pos { line, col },
            end: Pos { line, col },
        }
    }

    #[inline]
    pub const fn line_range(line: usize, col: usize, col_end: usize) -> Self {
        Self {
            start: Pos { line, col },
            end: Pos {
                line,
                col: col_end,
            },
        }
    }
}

// -------------------------------------------------------------------------------------------------
// AST
// -------------------------------------------------------------------------------------------------

/// A parsed MUF file.
#[derive(Debug, Clone, PartialEq)]
pub struct MufFile {
    pub version: i64,
    pub blocks: Vec<Block>,
    pub span: Span,
}

impl MufFile {
    #[inline]
    pub fn new(version: i64, blocks: Vec<Block>, span: Span) -> Self {
        Self {
            version,
            blocks,
            span,
        }
    }

    /// Iterates all blocks recursively (DFS, pre-order).
    pub fn walk_blocks<'a>(&'a self) -> BlockWalk<'a> {
        BlockWalk::new(&self.blocks)
    }

    /// Finds the first top-level block with the given tag and optional name.
    pub fn find_block(&self, tag: &str, name: Option<&str>) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|b| b.tag == tag && name.map_or(true, |n| b.name.as_deref() == Some(n)))
    }

    /// Finds all top-level blocks with a given tag.
    pub fn find_blocks<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.blocks.iter().filter(move |b| b.tag == tag)
    }
}

/// A MUF block: `[TAG name?] ... ..`
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub tag: String,
    pub name: Option<String>,
    pub items: Vec<BlockItem>,
    pub span: Span,
}

impl Block {
    #[inline]
    pub fn new(tag: impl Into<String>, name: Option<impl Into<String>>, items: Vec<BlockItem>, span: Span) -> Self {
        Self {
            tag: tag.into(),
            name: name.map(Into::into),
            items,
            span,
        }
    }

    /// Returns nested blocks only.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|it| match it {
            BlockItem::Block(b) => Some(b),
            _ => None,
        })
    }

    /// Returns directives only.
    pub fn directives(&self) -> impl Iterator<Item = &Directive> {
        self.items.iter().filter_map(|it| match it {
            BlockItem::Directive(d) => Some(d),
            _ => None,
        })
    }

    /// Finds the first directive with a given opcode.
    pub fn find_dir(&self, op: &str) -> Option<&Directive> {
        self.directives().find(|d| d.op == op)
    }

    /// Finds all directives with a given opcode.
    pub fn find_dirs<'a>(&'a self, op: &'a str) -> impl Iterator<Item = &'a Directive> + 'a {
        self.directives().filter(move |d| d.op == op)
    }

    /// Finds the first nested block with a given tag and optional name.
    pub fn find_block(&self, tag: &str, name: Option<&str>) -> Option<&Block> {
        self.blocks()
            .find(|b| b.tag == tag && name.map_or(true, |n| b.name.as_deref() == Some(n)))
    }
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

impl Directive {
    #[inline]
    pub fn new(op: impl Into<String>, args: Vec<Atom>, span: Span) -> Self {
        Self {
            op: op.into(),
            args,
            span,
        }
    }

    /// Returns the nth argument if present.
    #[inline]
    pub fn arg(&self, i: usize) -> Option<&Atom> {
        self.args.get(i)
    }

    /// Returns the first argument as a string if it is a string atom.
    #[inline]
    pub fn arg_str(&self, i: usize) -> Option<&str> {
        match self.args.get(i) {
            Some(Atom::Str(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the first argument as a name if it is a name atom.
    #[inline]
    pub fn arg_name(&self, i: usize) -> Option<&str> {
        match self.args.get(i) {
            Some(Atom::Name(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the first argument as an integer if it is an int number.
    #[inline]
    pub fn arg_i64(&self, i: usize) -> Option<i64> {
        match self.args.get(i) {
            Some(Atom::Number(Number::Int { value, .. })) => Some(*value),
            _ => None,
        }
    }

    /// Returns the first argument as float if it is a float number.
    #[inline]
    pub fn arg_f64(&self, i: usize) -> Option<f64> {
        match self.args.get(i) {
            Some(Atom::Number(Number::Float { value, .. })) => Some(*value),
            _ => None,
        }
    }

    /// Returns the first argument as ref if it is a ref atom.
    #[inline]
    pub fn arg_ref(&self, i: usize) -> Option<&RefPath> {
        match self.args.get(i) {
            Some(Atom::Ref(r)) => Some(r),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Atom {
    Ref(RefPath),
    Str(String),
    Number(Number),
    Name(String),
}

impl Atom {
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Atom::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    #[inline]
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Atom::Name(s) => Some(s.as_str()),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ref(&self) -> Option<&RefPath> {
        match self {
            Atom::Ref(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Atom::Number(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefPath {
    pub segments: Vec<String>,
}

impl RefPath {
    #[inline]
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    #[inline]
    pub fn first(&self) -> Option<&str> {
        self.segments.first().map(|s| s.as_str())
    }

    #[inline]
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    pub fn to_slash_string(&self) -> String {
        let mut out = String::new();
        out.push('~');
        for (i, s) in self.segments.iter().enumerate() {
            if i != 0 {
                out.push('/');
            }
            out.push_str(s);
        }
        out
    }
}

impl fmt::Display for RefPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_slash_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    Int { raw: String, value: i64 },
    Float { raw: String, value: f64 },
}

impl Number {
    #[inline]
    pub fn raw(&self) -> &str {
        match self {
            Number::Int { raw, .. } => raw,
            Number::Float { raw, .. } => raw,
        }
    }

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Number::Int { value, .. } => Some(*value),
            _ => None,
        }
    }

    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Number::Float { value, .. } => Some(*value),
            Number::Int { value, .. } => Some(*value as f64),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Walk iterator
// -------------------------------------------------------------------------------------------------

/// DFS pre-order walk over a list of root blocks.
pub struct BlockWalk<'a> {
    stack: Vec<&'a Block>,
}

impl<'a> BlockWalk<'a> {
    fn new(roots: &'a [Block]) -> Self {
        let mut stack = Vec::new();
        // push in reverse so first root is visited first
        for b in roots.iter().rev() {
            stack.push(b);
        }
        Self { stack }
    }
}

impl<'a> Iterator for BlockWalk<'a> {
    type Item = &'a Block;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.stack.pop()?;
        // push children in reverse
        let mut children = Vec::new();
        for it in &b.items {
            if let BlockItem::Block(ch) = it {
                children.push(ch);
            }
        }
        for ch in children.into_iter().rev() {
            self.stack.push(ch);
        }
        Some(b)
    }
}

// -------------------------------------------------------------------------------------------------
// Pretty (diagnostic) display helpers
// -------------------------------------------------------------------------------------------------

impl fmt::Display for MufFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "!muf {}", self.version)?;
        for b in &self.blocks {
            write_block(f, b, 0)?;
        }
        Ok(())
    }
}

fn write_block(f: &mut fmt::Formatter<'_>, b: &Block, depth: usize) -> fmt::Result {
    indent(f, depth)?;
    match &b.name {
        Some(n) => writeln!(f, "[{} {}]", b.tag, n)?,
        None => writeln!(f, "[{}]", b.tag)?,
    }

    for it in &b.items {
        match it {
            BlockItem::Directive(d) => {
                indent(f, depth + 1)?;
                write!(f, ".{}", d.op)?;
                for a in &d.args {
                    write!(f, " {}", atom_as_surface(a))?;
                }
                writeln!(f)?;
            }
            BlockItem::Block(ch) => {
                write_block(f, ch, depth + 1)?;
            }
        }
    }

    indent(f, depth)?;
    writeln!(f, "..")
}

fn indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        f.write_str("  ")?;
    }
    Ok(())
}

fn atom_as_surface(a: &Atom) -> String {
    match a {
        Atom::Ref(r) => r.to_slash_string(),
        Atom::Str(s) => {
            let mut out = String::new();
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\0' => out.push_str("\\0"),
                    _ => out.push(ch),
                }
            }
            out.push('"');
            out
        }
        Atom::Number(n) => n.raw().to_string(),
        Atom::Name(s) => s.clone(),
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_blocks_visits_all() {
        let ast = MufFile {
            version: 4,
            span: Span::point(1, 1),
            blocks: vec![Block {
                tag: "BK".into(),
                name: Some("app".into()),
                span: Span::point(2, 1),
                items: vec![
                    BlockItem::Directive(Directive::new(
                        "x",
                        vec![Atom::Name("a".into())],
                        Span::point(3, 1),
                    )),
                    BlockItem::Block(Block {
                        tag: "IN".into(),
                        name: Some("src".into()),
                        span: Span::point(4, 1),
                        items: vec![],
                    }),
                ],
            }],
        };

        let tags: Vec<String> = ast.walk_blocks().map(|b| b.tag.clone()).collect();
        assert_eq!(tags, vec!["BK".to_string(), "IN".to_string()]);
    }

    #[test]
    fn ref_path_display() {
        let r = RefPath::new(vec!["tool".into(), "vittec".into()]);
        assert_eq!(r.to_string(), "~tool/vittec");
    }

    #[test]
    fn pretty_print_roundtrip_shape() {
        let ast = MufFile {
            version: 4,
            span: Span::point(1, 1),
            blocks: vec![Block {
                tag: "WS".into(),
                name: Some("m".into()),
                span: Span::point(2, 1),
                items: vec![BlockItem::Directive(Directive::new(
                    "root",
                    vec![Atom::Str("./".into())],
                    Span::point(3, 3),
                ))],
            }],
        };

        let s = ast.to_string();
        assert!(s.contains("!muf 4"));
        assert!(s.contains("[WS m]"));
        assert!(s.contains(".root"));
        assert!(s.contains(".."));
    }
}
