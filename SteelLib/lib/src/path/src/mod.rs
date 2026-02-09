//! `path` crate/module root (mod.rs) — MAX.
//!
//! This module centralizes path utilities used across SteelLib:
//! - canonicalization and normalization (`canon`)
//! - globbing (`glob`)
//!
//! Re-exports provide a compact, stable API surface for downstream crates.

pub mod canon;
pub mod glob;

pub use canon::{
    join_under_base, normalize_portable, normalize_rel_unix, portable_to_native, reject_traversal_str, CanonError,
    CanonPath,
};

pub use glob::{GlobError, GlobPattern, GlobSet, WalkOptions};
