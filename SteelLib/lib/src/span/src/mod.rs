//! `span` crate/module root (mod.rs) — MAX.
//!
//! This module provides foundational span/file primitives used by diagnostics.
//! Typical dependency graph:
//! - `span` has no heavy deps and is used by `diag`, `parser`, `steel`, etc.
//!
//! Contents:
//! - `file`: `SourceFile`, line/col mapping, `FileId`
//! - `span`: `Span`, `SpanRange`, helpers (inline here to keep minimal)
//!
//! If you already have Span/FileId elsewhere, align types and re-exports.

pub mod file;

pub use file::{FileError, FileId, LineCol, Location, SourceFile};

use std::fmt;

/// A byte span in a file: [start, end) offsets in UTF-8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
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

    pub fn join(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file);
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file={:?} [{}..{})", self.file, self.start, self.end)
    }
}

/// A file-free span range (useful for buffers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpanRange {
    pub start: u32,
    pub end: u32,
}

impl SpanRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_join() {
        let f = FileId(1);
        let a = Span::new(f, 10, 20);
        let b = Span::new(f, 18, 30);
        let c = a.join(b);
        assert_eq!(c.start, 10);
        assert_eq!(c.end, 30);
    }
}
