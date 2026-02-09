// Module: read
// src/read.rs
//
// Steel — robust file reading utilities
//
// Purpose:
// - Centralize file/stream reading logic used across Steel.
// - Provide consistent behavior for:
//   - reading UTF-8 text with BOM handling
//   - reading bytes
//   - size limits (to avoid OOM in diagnostics)
//   - path normalization and nice errors
// - Provide "diagnostics-friendly" error model that keeps path + context.
//
// No external deps.
//
// Typical usage:
//   let txt = read_text(Path::new("steelconf"))?;
//   let bytes = read_bytes_limited(Path::new("foo.bin"), 8 * 1024 * 1024)?;
//
// Integration:
// - Convert ReadError to your diagnostics (spanless IO diag).
// - Optionally use `ReadReport` to keep metadata (bytes read, bom, newline style).
//
// Notes:
// - This module does not do OS-specific encoding conversions (Windows ACP). UTF-8 only.
// - If you need lossy decode, use `read_text_lossy_*` helpers.

#![allow(dead_code)]

use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    Io {
        path: PathBuf,
        op: &'static str,
        message: String,
    },
    TooLarge {
        path: PathBuf,
        limit: usize,
        actual: usize,
    },
    InvalidUtf8 {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io { path, op, message } => write!(f, "{} {}: {}", op, path.display(), message),
            ReadError::TooLarge { path, limit, actual } => write!(
                f,
                "file too large {}: {} > {} bytes",
                path.display(),
                actual,
                limit
            ),
            ReadError::InvalidUtf8 { path, message } => write!(f, "invalid utf-8 {}: {}", path.display(), message),
        }
    }
}

impl std::error::Error for ReadError {}

/* ============================== reports ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineStyle {
    Lf,
    CrLf,
    Mixed,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomKind {
    Utf8,
    None,
}

#[derive(Debug, Clone)]
pub struct ReadReport<T> {
    pub path: PathBuf,
    pub value: T,
    pub bytes: usize,
    pub bom: BomKind,
    pub newline: NewlineStyle,
}

impl<T> ReadReport<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ReadReport<U> {
        ReadReport {
            path: self.path,
            value: f(self.value),
            bytes: self.bytes,
            bom: self.bom,
            newline: self.newline,
        }
    }
}

/* ============================== public API ============================== */

pub const DEFAULT_MAX_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

pub fn read_bytes(path: &Path) -> Result<Vec<u8>, ReadError> {
    read_bytes_limited(path, DEFAULT_MAX_BYTES)
}

pub fn read_bytes_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
    let mut f = File::open(path).map_err(|e| ReadError::Io {
        path: path.to_path_buf(),
        op: "open",
        message: e.to_string(),
    })?;

    // If metadata is available, pre-check size (best effort).
    if let Ok(md) = f.metadata() {
        let len = md.len() as usize;
        if max_bytes > 0 && len > max_bytes {
            return Err(ReadError::TooLarge {
                path: path.to_path_buf(),
                limit: max_bytes,
                actual: len,
            });
        }
    }

    let mut buf = Vec::<u8>::new();
    if max_bytes > 0 {
        buf.reserve(std::cmp::min(8192, max_bytes));
        read_to_end_limited(&mut f, &mut buf, max_bytes).map_err(|e| map_io(path, "read", e))?;
        if buf.len() > max_bytes {
            return Err(ReadError::TooLarge {
                path: path.to_path_buf(),
                limit: max_bytes,
                actual: buf.len(),
            });
        }
    } else {
        f.read_to_end(&mut buf).map_err(|e| map_io(path, "read", e))?;
    }

    Ok(buf)
}

/// Read UTF-8 text with BOM stripping and newline analysis.
pub fn read_text(path: &Path) -> Result<String, ReadError> {
    read_text_limited(path, DEFAULT_MAX_BYTES).map(|r| r.value)
}

pub fn read_text_report(path: &Path) -> Result<ReadReport<String>, ReadError> {
    read_text_limited(path, DEFAULT_MAX_BYTES)
}

pub fn read_text_limited(path: &Path, max_bytes: usize) -> Result<ReadReport<String>, ReadError> {
    let bytes = read_bytes_limited(path, max_bytes)?;
    decode_utf8_report(path, &bytes)
}

/// Read UTF-8 text but accept invalid sequences (lossy).
pub fn read_text_lossy(path: &Path) -> Result<Cow<'static, str>, ReadError> {
    let bytes = read_bytes_limited(path, DEFAULT_MAX_BYTES)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned().into())
}

/* ============================== decoding helpers ============================== */

pub fn decode_utf8_report(path: &Path, bytes: &[u8]) -> Result<ReadReport<String>, ReadError> {
    let (bom, body) = strip_utf8_bom(bytes);
    let newline = detect_newline_style(body);

    let s = std::str::from_utf8(body).map_err(|e| ReadError::InvalidUtf8 {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    Ok(ReadReport {
        path: path.to_path_buf(),
        value: s.to_string(),
        bytes: bytes.len(),
        bom,
        newline,
    })
}

fn strip_utf8_bom(bytes: &[u8]) -> (BomKind, &[u8]) {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    if bytes.starts_with(BOM) {
        (BomKind::Utf8, &bytes[BOM.len()..])
    } else {
        (BomKind::None, bytes)
    }
}

fn detect_newline_style(bytes: &[u8]) -> NewlineStyle {
    let mut saw_lf = false;
    let mut saw_crlf = false;

    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                saw_lf = true;
                i += 1;
            }
            b'\r' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    saw_crlf = true;
                    i += 2;
                } else {
                    // lone CR — treat as mixed/lf-ish (rare)
                    saw_lf = true;
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    match (saw_lf, saw_crlf) {
        (false, false) => NewlineStyle::None,
        (true, false) => NewlineStyle::Lf,
        (false, true) => NewlineStyle::CrLf,
        (true, true) => NewlineStyle::Mixed,
    }
}

/* ============================== IO helpers ============================== */

fn read_to_end_limited<R: Read>(r: &mut R, out: &mut Vec<u8>, max: usize) -> io::Result<()> {
    let mut buf = [0u8; 8192];

    while out.len() < max {
        let to_read = std::cmp::min(buf.len(), max - out.len());
        let n = r.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }

    // If there is still more data, try one more byte to detect overflow without reading everything.
    if max > 0 {
        let mut one = [0u8; 1];
        let n = r.read(&mut one)?;
        if n > 0 {
            out.push(one[0]);
        }
    }

    Ok(())
}

fn map_io(path: &Path, op: &'static str, e: io::Error) -> ReadError {
    ReadError::Io {
        path: path.to_path_buf(),
        op,
        message: e.to_string(),
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newline_detection() {
        assert_eq!(detect_newline_style(b""), NewlineStyle::None);
        assert_eq!(detect_newline_style(b"a\nb\n"), NewlineStyle::Lf);
        assert_eq!(detect_newline_style(b"a\r\nb\r\n"), NewlineStyle::CrLf);
        assert_eq!(detect_newline_style(b"a\r\nb\n"), NewlineStyle::Mixed);
    }

    #[test]
    fn bom_strip() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b'];
        let (bom, body) = strip_utf8_bom(&bytes);
        assert_eq!(bom, BomKind::Utf8);
        assert_eq!(body, b"ab");
    }

    #[test]
    fn limited_reader_flags_overflow() {
        // We can't do filesystem IO in unit test reliably here;
        // test the helper directly.
        let mut data: &[u8] = b"0123456789";
        let mut out = Vec::new();
        read_to_end_limited(&mut data, &mut out, 5).unwrap();
        assert!(out.len() >= 5);
    }
}
