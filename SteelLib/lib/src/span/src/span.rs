//! Span model (span.rs) — MAX.
//!
//! This file is the canonical "span" layer used by Steel diagnostics.
//!
//! Design:
//! - UTF-8 byte offsets
//! - `Span` always includes a `FileId`
//! - `SpanRange` is file-free
//! - helpers to join/cover, map, clamp, split
//! - display formatting
//!
//! Integration:
//! - `SourceFile` in `file.rs` maps offsets -> line/col.
//! - Diagnostics use `Span` to attach primary/secondary ranges.
//!
//! If you already defined `Span` in `mod.rs`, choose ONE source of truth:
//! - either re-export from here, and remove the duplicate from `mod.rs`
//! - or keep the one in `mod.rs` and make this file just helpers.
//!
//! This implementation assumes `crate::span::file::FileId` exists.

use std::fmt;

use crate::span::file::FileId;

/// A byte span in a file: [start, end) offsets in UTF-8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[inline]
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    #[inline]
    pub fn empty_at(file: FileId, off: u32) -> Self {
        Self { file, start: off, end: off }
    }

    #[inline]
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    #[inline]
    pub fn contains(&self, off: u32) -> bool {
        self.start <= off && off < self.end
    }

    #[inline]
    pub fn contains_inclusive_end(&self, off: u32) -> bool {
        self.start <= off && off <= self.end
    }

    /// Return a span covering `self` and `other`. Requires same file.
    #[inline]
    pub fn cover(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file);
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Join adjacent spans (where self.end == other.start) into one.
    /// If not adjacent, returns cover (still merges).
    #[inline]
    pub fn join(self, other: Span) -> Span {
        self.cover(other)
    }

    /// Clamp this span inside [min, max] (inclusive boundaries on range),
    /// producing a possibly-empty span.
    #[inline]
    pub fn clamp(self, min: u32, max: u32) -> Span {
        let s = self.start.clamp(min, max);
        let e = self.end.clamp(min, max);
        if e < s {
            Span::new(self.file, s, s)
        } else {
            Span::new(self.file, s, e)
        }
    }

    /// Shift by signed delta (saturating at 0).
    #[inline]
    pub fn shift(self, delta: i64) -> Span {
        fn shift_u32(x: u32, d: i64) -> u32 {
            if d >= 0 {
                x.saturating_add(d as u32)
            } else {
                x.saturating_sub((-d) as u32)
            }
        }
        Span {
            file: self.file,
            start: shift_u32(self.start, delta),
            end: shift_u32(self.end, delta),
        }
    }

    /// Split at an offset (relative to file). If split outside, returns (self, empty_at(end)).
    pub fn split_at(self, off: u32) -> (Span, Span) {
        if off <= self.start {
            return (Span::empty_at(self.file, self.start), self);
        }
        if off >= self.end {
            return (self, Span::empty_at(self.file, self.end));
        }
        (
            Span::new(self.file, self.start, off),
            Span::new(self.file, off, self.end),
        )
    }

    /// Convert to file-free range.
    #[inline]
    pub fn as_range(self) -> SpanRange {
        SpanRange::new(self.start, self.end)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file={:?} [{}..{})", self.file, self.start, self.end)
    }
}

/// A file-free byte range [start, end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpanRange {
    pub start: u32,
    pub end: u32,
}

impl SpanRange {
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[inline]
    pub fn empty_at(off: u32) -> Self {
        Self { start: off, end: off }
    }

    #[inline]
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    #[inline]
    pub fn contains(&self, off: u32) -> bool {
        self.start <= off && off < self.end
    }

    #[inline]
    pub fn cover(self, other: SpanRange) -> SpanRange {
        SpanRange {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[inline]
    pub fn clamp(self, min: u32, max: u32) -> SpanRange {
        let s = self.start.clamp(min, max);
        let e = self.end.clamp(min, max);
        if e < s {
            SpanRange::new(s, s)
        } else {
            SpanRange::new(s, e)
        }
    }

    #[inline]
    pub fn shift(self, delta: i64) -> SpanRange {
        fn shift_u32(x: u32, d: i64) -> u32 {
            if d >= 0 {
                x.saturating_add(d as u32)
            } else {
                x.saturating_sub((-d) as u32)
            }
        }
        SpanRange {
            start: shift_u32(self.start, delta),
            end: shift_u32(self.end, delta),
        }
    }

    pub fn split_at(self, off: u32) -> (SpanRange, SpanRange) {
        if off <= self.start {
            return (SpanRange::empty_at(self.start), self);
        }
        if off >= self.end {
            return (self, SpanRange::empty_at(self.end));
        }
        (
            SpanRange::new(self.start, off),
            SpanRange::new(off, self.end),
        )
    }
}

impl fmt::Display for SpanRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}..{})", self.start, self.end)
    }
}

/* ------------------------------ Utilities ------------------------------ */

/// Cover all spans in an iterator (same file required).
pub fn cover_all<I>(mut it: I) -> Option<Span>
where
    I: Iterator<Item = Span>,
{
    let first = it.next()?;
    let mut acc = first;
    for s in it {
        debug_assert_eq!(acc.file, s.file);
        acc = acc.cover(s);
    }
    Some(acc)
}

/// Convert a file-free range into a file span.
#[inline]
pub fn range_in_file(file: FileId, r: SpanRange) -> Span {
    Span::new(file, r.start, r.end)
}

/// Clamp a span to a file length.
#[inline]
pub fn clamp_to_len(s: Span, file_len: u32) -> Span {
    s.clamp(0, file_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_basic() {
        let f = FileId(1);
        let a = Span::new(f, 10, 20);
        let b = Span::new(f, 18, 30);
        let c = a.cover(b);
        assert_eq!(c.start, 10);
        assert_eq!(c.end, 30);
    }

    #[test]
    fn split_at_inside() {
        let f = FileId(1);
        let a = Span::new(f, 10, 20);
        let (l, r) = a.split_at(15);
        assert_eq!(l, Span::new(f, 10, 15));
        assert_eq!(r, Span::new(f, 15, 20));
    }

    #[test]
    fn range_cover() {
        let a = SpanRange::new(1, 2);
        let b = SpanRange::new(10, 20);
        let c = a.cover(b);
        assert_eq!(c.start, 1);
        assert_eq!(c.end, 20);
    }

    #[test]
    fn cover_all_iter() {
        let f = FileId(1);
        let v = vec![Span::new(f, 1, 2), Span::new(f, 5, 6), Span::new(f, 2, 4)];
        let c = cover_all(v.into_iter()).unwrap();
        assert_eq!(c.start, 1);
        assert_eq!(c.end, 6);
    }
}
