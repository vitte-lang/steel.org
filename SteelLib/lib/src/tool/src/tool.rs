//! Tool high-level API (tool.rs) — MAX (std-only).
//!
//! This file provides a stable public surface for tool execution and probing.
//! It typically sits at `src/tool.rs` (or `src/tool/tool.rs`) and re-exports
//! submodules:
//! - `tool::mod.rs` => core types (ToolSpec, ToolRunner, ToolOutput, errors)
//! - `tool::probe.rs` => PATH discovery + version probing
//! - `tool::runner.rs` => richer runner backend (optional)
//
//! If your crate structure is `tool/src/*.rs`, you can keep this as `lib.rs`
//! re-exporting modules.
//!
//! This file assumes modules are located under `crate::tool::*`.
//! Adapt `mod` paths if your layout differs.

pub mod tool {
    pub mod probe;
    pub mod runner;

    // If you already have `tool/mod.rs` as the root module, you may not need this nesting.
    // In that case, delete this wrapper and use direct `pub mod probe; pub mod runner;`.
}

pub use crate::tool::{
    ToolError, ToolOutput, ToolRunner, ToolSpec, ToolStatus,
};

pub use crate::tool::probe::{
    probe_many, probe_tool, probe_tool_path, which, which_all, ProbeError, ToolCandidate, ToolProbe,
};

pub use crate::tool::runner::{
    Capture, RunOptions, RunResult, RunTrace, Runner, RunnerError,
};
