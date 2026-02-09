//! Prelude (prelude.rs) — MAX.
//!
//! Purpose:
//! - Provide a single glob-import path for common SteelLib types.
//! - Keep it conservative enough to avoid name conflicts, but ergonomic.
//!
//! Usage:
//! ```rust
//! use vittelib::prelude::*;
//! ```
//!
//! Notes:
//! - This file assumes the crate root re-exports modules/types as in `lib.rs`.
//! - If some modules are feature-gated, you can gate exports here too.

pub use crate::error::{SteelError, Result};

// --- span ---
pub use crate::span::{
    FileError, FileId, LineCol, Location, SourceFile, Span, SpanRange,
};

// --- store ---
pub use crate::store::{
    Cas, CasConfig, CasError, Digest, DigestAlgo, EntryKind, GcError, GcOptions, GcReport,
    IndexEntry, IndexError, StoreIndex,
};

// --- tool ---
pub use crate::tool::{
    ToolError, ToolOutput, ToolRunner, ToolSpec, ToolStatus,
};

// --- platform ---
pub use crate::platform::Platform;

// --- muf (AST/lexer/parser) ---
pub use crate::muf_ast::{
    Atom as MufAtom, Block as MufBlock, BlockItem as MufBlockItem, MufFile, Number as MufNumber,
    Pos as MufPos, RefPath as MufRefPath, Span as MufSpan,
};
pub use crate::muf_lexer::{LexError as MufLexError, Lexer as MufLexer, Token as MufToken, TokenKind as MufTokenKind};
pub use crate::muf_parser::{parse_muf, ParseError as MufParseError};
