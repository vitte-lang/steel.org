//! Source file model for spans (file.rs) — MAX.
//!
//! This module defines `SourceFile`, an in-memory representation of a text file
//! used for diagnostics/spans.
//!
//! Responsibilities:
//! - store file path + full text
//! - compute line start offsets (for O(log N) line/col mapping)
//! - provide span slicing helpers
//! - support incremental creation from disk/bytes/string
//! - stable file id generation (best-effort, std-only)
//!
//! Notes:
//! - Span types themselves are typically in `span::{Span, FileId, ...}`.
//! - This file assumes a small `FileId` newtype exists; if not, define it here.
//! - For performance, we store line starts as byte offsets into UTF-8 text.
//!   Column is returned as (byte index in line) and optionally as UTF-16 code units.

use std::fmt;
use std::path::{Path, PathBuf};

/// Stable-ish file identifier.
/// If you already have `FileId` elsewhere, remove this and `pub use` it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u64);

impl FileId {
    pub fn new(v: u64) -> Self {
        Self(v)
    }
}

#[derive(Debug)]
pub enum FileError {
    Io(std::io::Error),
    Utf8(std::string::FromUtf8Error),
    Invalid(&'static str),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "io: {e}"),
            FileError::Utf8(e) => write!(f, "utf8: {e}"),
            FileError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        FileError::Io(e)
    }
}

impl From<std::string::FromUtf8Error> for FileError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        FileError::Utf8(e)
    }
}

/// 1-based line/column pair (human-facing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32, // 1-based
    pub col: u32,  // 1-based (byte column by default)
}

impl LineCol {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// A resolved location in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file_id: FileId,
    pub path: Option<PathBuf>,
    pub offset: usize, // byte offset in file
    pub line_col: LineCol,
}

/// Represents a text file with precomputed line start offsets.
#[derive(Debug, Clone)]
pub struct SourceFile {
    id: FileId,
    path: Option<PathBuf>,
    text: String,
    /// Byte offsets where each line starts. Always includes 0.
    /// For N lines, length is N. Example: "a\nb\n" => [0,2,4]
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// Create a SourceFile from UTF-8 text.
    pub fn new(id: FileId, path: Option<PathBuf>, text: String) -> Self {
        let line_starts = compute_line_starts(&text);
        Self { id, path, text, line_starts }
    }

    /// Create with automatic ID derived from path + content (best-effort).
    pub fn from_string(path: Option<PathBuf>, text: String) -> Self {
        let id = FileId(hash64_path_text(path.as_deref(), &text));
        Self::new(id, path, text)
    }

    /// Read from disk (UTF-8).
    pub fn read(path: impl AsRef<Path>) -> Result<Self, FileError> {
        let path = path.as_ref().to_path_buf();
        let bytes = std::fs::read(&path)?;
        let text = String::from_utf8(bytes)?;
        Ok(Self::from_string(Some(path), text))
    }

    /// Create from raw bytes (UTF-8).
    pub fn from_bytes(path: Option<PathBuf>, bytes: Vec<u8>) -> Result<Self, FileError> {
        let text = String::from_utf8(bytes)?;
        Ok(Self::from_string(path, text))
    }

    /// File id.
    pub fn id(&self) -> FileId {
        self.id
    }

    /// Path (if known).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Full text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// File length in bytes.
    pub fn len_bytes(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of lines (1-based). Empty file has 1 line by convention.
    pub fn line_count(&self) -> usize {
        // If text is empty, compute_line_starts returns [0], so 1 line.
        self.line_starts.len()
    }

    /// Return line starts table (byte offsets).
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    /// Map byte offset -> (1-based line, 1-based column in bytes).
    pub fn line_col_at(&self, offset: usize) -> Result<LineCol, FileError> {
        if offset > self.text.len() {
            return Err(FileError::Invalid("offset out of bounds"));
        }

        // Find greatest line_start <= offset
        let idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };

        let line_start = self.line_starts[idx];
        let line = (idx as u32) + 1;
        let col0 = offset - line_start; // 0-based byte col
        Ok(LineCol::new(line, (col0 as u32) + 1))
    }

    /// Map byte offset -> Location with resolved line/col.
    pub fn location_at(&self, offset: usize) -> Result<Location, FileError> {
        let lc = self.line_col_at(offset)?;
        Ok(Location {
            file_id: self.id,
            path: self.path.clone(),
            offset,
            line_col: lc,
        })
    }

    /// Return the (start,end) byte offsets for the given 1-based line number.
    /// End is exclusive. End will stop at '\n' if present, else EOF.
    pub fn line_range(&self, line1: u32) -> Result<(usize, usize), FileError> {
        if line1 == 0 {
            return Err(FileError::Invalid("line is 1-based"));
        }
        let idx = (line1 - 1) as usize;
        if idx >= self.line_starts.len() {
            return Err(FileError::Invalid("line out of bounds"));
        }
        let start = self.line_starts[idx];
        let end = if idx + 1 < self.line_starts.len() {
            self.line_starts[idx + 1]
        } else {
            self.text.len()
        };

        // If end includes newline, trim it for "line content" style range.
        let mut end2 = end;
        if end2 > start && self.text.as_bytes()[end2.saturating_sub(1)] == b'\n' {
            end2 -= 1;
            if end2 > start && self.text.as_bytes()[end2.saturating_sub(1)] == b'\r' {
                end2 -= 1; // CRLF
            }
        }

        Ok((start, end2))
    }

    /// Get line string slice for given line number (without trailing newline).
    pub fn line_str(&self, line1: u32) -> Result<&str, FileError> {
        let (s, e) = self.line_range(line1)?;
        Ok(&self.text[s..e])
    }

    /// Slice a byte range (start..end) from file text.
    pub fn slice(&self, start: usize, end: usize) -> Result<&str, FileError> {
        if start > end || end > self.text.len() {
            return Err(FileError::Invalid("slice out of bounds"));
        }
        // Ensure we slice on UTF-8 boundaries.
        if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
            return Err(FileError::Invalid("slice not on UTF-8 boundary"));
        }
        Ok(&self.text[start..end])
    }

    /// Compute a UTF-16 column (1-based) for a given byte offset (best-effort).
    /// Useful for LSP.
    pub fn utf16_col_at(&self, offset: usize) -> Result<u32, FileError> {
        let lc = self.line_col_at(offset)?;
        let (ls, _) = self.line_range(lc.line)?;
        let line_start = ls;
        let byte_col0 = (offset - line_start) as usize;

        // Count UTF-16 code units in the prefix of the line up to byte_col0.
        let prefix = &self.text[line_start..(line_start + byte_col0)];
        let mut units = 0u32;
        for ch in prefix.chars() {
            let u = ch as u32;
            units += if u >= 0x10000 { 2 } else { 1 };
        }
        Ok(units + 1)
    }

    /// Return a "snippet" around an offset, for diagnostics.
    pub fn snippet_around(&self, offset: usize, context: usize) -> Result<&str, FileError> {
        if offset > self.text.len() {
            return Err(FileError::Invalid("offset out of bounds"));
        }
        let start = offset.saturating_sub(context);
        let end = (offset + context).min(self.text.len());

        // Adjust to UTF-8 boundaries
        let mut s = start;
        while s > 0 && !self.text.is_char_boundary(s) {
            s -= 1;
        }
        let mut e = end;
        while e < self.text.len() && !self.text.is_char_boundary(e) {
            e += 1;
        }
        Ok(&self.text[s..e])
    }
}

/* ------------------------------- Internals ------------------------------- */

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    starts.push(0);

    // Record start index after every '\n'
    for (i, b) in text.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            let next = i + 1;
            if next <= text.len() {
                starts.push(next);
            }
        }
    }

    if starts.is_empty() {
        starts.push(0);
    }
    starts
}

fn hash64_path_text(path: Option<&Path>, text: &str) -> u64 {
    // Simple FNV-1a 64-bit over path + 0 byte + content length + content.
    // Not cryptographic; just stable-ish.
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    let mut h = FNV_OFFSET;

    if let Some(p) = path {
        for b in p.to_string_lossy().as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    }

    h ^= 0;
    h = h.wrapping_mul(FNV_PRIME);

    let len = text.len() as u64;
    for b in len.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }

    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }

    h
}

/* --------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_basic() {
        let f = SourceFile::from_string(None, "a\nb\nc".to_string());
        assert_eq!(f.line_count(), 3);
        assert_eq!(f.line_str(1).unwrap(), "a");
        assert_eq!(f.line_str(2).unwrap(), "b");
        assert_eq!(f.line_str(3).unwrap(), "c");
    }

    #[test]
    fn line_col_mapping() {
        let f = SourceFile::from_string(None, "ab\ncd\nef".to_string());
        // offset 0 => line1 col1
        assert_eq!(f.line_col_at(0).unwrap(), LineCol::new(1, 1));
        // offset 1 => line1 col2
        assert_eq!(f.line_col_at(1).unwrap(), LineCol::new(1, 2));
        // offset 3 => 'c' line2 col1 (since "ab\n" is 3 bytes)
        assert_eq!(f.line_col_at(3).unwrap(), LineCol::new(2, 1));
    }

    #[test]
    fn slice_utf8_boundary() {
        let f = SourceFile::from_string(None, "é\n".to_string());
        // 'é' is 2 bytes; slicing at 1 is invalid boundary
        assert!(f.slice(0, 1).is_err());
        assert_eq!(f.slice(0, 2).unwrap(), "é");
    }

    #[test]
    fn utf16_col() {
        let f = SourceFile::from_string(None, "a🙂b\n".to_string());
        // "a" 1 unit, "🙂" is surrogate pair => 2 units, so "b" starts at utf16 col 4
        let off_b = "a🙂".as_bytes().len();
        assert_eq!(f.utf16_col_at(off_b).unwrap(), 4);
    }
}
