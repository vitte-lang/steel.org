// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\tool\src\lib.rs
//! steel_tool — MAX (std-only).
//!
//! Modules:
//! - `mod`    : core spec/runner/output types
//! - `probe`  : PATH discovery + version probing
//! - `runner` : richer runner backend (optional)
//!
//! Re-exports keep the crate API flat and ergonomic.

pub mod probe;
pub mod runner;

mod mod_; // avoid name collision with Rust keyword in some setups
pub use mod_::*;

