//! Store module root (mod.rs) — MAX.
//!
//! This module groups store-related primitives:
//! - CAS (content-addressable storage) for blobs/artifacts
//! - index for mapping logical keys -> digests
//! - GC to reclaim unreachable blobs
//!
//! Expected layout:
//! - src/store/cas.rs
//! - src/store/index.rs
//! - src/store/gc.rs
//!
//! Re-exports provide a stable API surface for downstream crates.

pub mod cas;
pub mod gc;
pub mod index;

pub use cas::{Cas, CasConfig, CasError, Digest, DigestAlgo};
pub use gc::{run_gc, run_gc_with_provider, FsRootsProvider, GcError, GcOptions, GcReport, RootsProvider};
pub use index::{EntryKind, IndexEntry, IndexError, StoreIndex};
