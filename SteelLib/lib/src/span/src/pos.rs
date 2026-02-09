//! Position primitives (pos.rs) — MAX.
//!
//! This module provides small, reusable position types used by lexer/parser/diag.
//! It complements `span`:
//! - `Pos` is a single byte offset in a file.
//! - `Range` is a [start,end) byte range (file-free).
//! - `PosLC` is a (line,col) pair (1-based).
//!
//! Notes:
//! - Offsets are UTF-8 byte offsets.
//! - Line/col mapping is handled by `SourceFile` (span::file).

use std::fmt;

use crate::span::file::FileId;

/// A single byte offset in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub file: FileId,
    pub off: u32,
}

impl Pos {
    pub fn new(file: FileId, off: u32) -> Self {
        Self { file, off }
    }
}

/// A file-free byte range [start, end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Range {
    pub start: u32,
    pub end: u32,
}

impl Range {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub fn contains(&self, off: u32) -> bool {
        self.start <= off && off < self.end
    }
}

/// 1-based (line, col) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PosLC {
    pub line: u32,
    pub col: u32,
}

impl PosLC {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for PosLC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file={:?}@{}", self.file, self.off)
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}..{})", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_basics() {
        let r = Range::new(10, 15);
        assert_eq!(r.len(), 5);
        assert!(r.contains(10));
        assert!(!r.contains(15));
    }
}
