//! Steel: Declarative configuration layer for Vitte build system
//!
//! Steel parses, validates, and resolves a workspace configuration (packages, profiles,
//! toolchains, targets), then generates a stable configuration artifact `Steelconfig.mff`.
//!
//! # Architecture
//!
//! The architecture follows a "Freeze then Build" principle:
//! - **Phase 1**: Configuration (validation + resolution)
//! - **Phase 2**: Construction (DAG execution via runner)
//!
//! # Modules
//!
//! - `parser` — Lexical and syntactic analysis of Steel files
//! - `validator` — Coherence checking and constraint validation
//! - `resolver` — Profile inheritance, variable interpolation, dependency resolution
//! - `generator` — Serialization to Steelconfig.mff and exports
//! - `model` — Core data structures (Workspace, Package, Profile, Target, Toolchain)
//! - `interface` — Runtime abstraction and CLI interface
//! - `commands` — CLI command implementations

// ============================================================================
// PARSER MODULE
// ============================================================================

/// Lexical and syntactic analysis of Steel files
pub mod parser;

// ============================================================================
// RESOLVER MODULE
// ============================================================================

/// Resolution: profile inheritance, variable expansion, implicit rule resolution
pub mod resolver {
    pub use crate::variable;         // Variable interpolation and scope
    pub use crate::expand;           // Macro and variable expansion
    pub use crate::implicit;         // Implicit rule resolution
    pub use crate::default;          // Default value application
}

// ============================================================================
// MODEL MODULE
// ============================================================================

/// Core data structures: Workspace, Package, Profile, Target, Toolchain
pub mod model {
    pub use crate::steelint;       // Steel internal API
    pub use crate::def_target_file;  // Target file definitions
    pub use crate::rule;             // Rule model
}

// ============================================================================
// RUNTIME & INTERFACE MODULE
// ============================================================================

/// Runtime abstractions: filesystem, process, logging
pub mod runtime {
    pub use crate::os;               // OS-specific implementations
    pub use crate::posixos;          // POSIX compliance layer
    pub use crate::job;              // Job/process management
    pub use crate::debug;            // Debug and logging utilities
}

// ============================================================================
// CLI & COMMANDS MODULE
// ============================================================================

/// Command-line interface and command implementations
pub mod cli {
    pub use crate::commands;         // Command implementations
    pub use crate::interface;        // CLI interface
}

// Minimal MUF runner (exec tools from SteelConfig.muf)
pub mod run_muf;

// ============================================================================
// UTILITIES MODULE
// ============================================================================

/// Utility functions and helpers
pub mod utils {
    pub use crate::misc;             // Miscellaneous utilities
    pub use crate::hash;             // Hash utilities
    pub use crate::strcache;         // String cache
    pub use crate::directory;        // Directory utilities
    pub use crate::vpath;            // Virtual path resolution
}

// ============================================================================
// VERSION MODULE
// ============================================================================

/// Version and metadata
pub mod metadata {
    pub use crate::version;          // Version information
    pub use crate::warning;          // Warning utilities
}

// ============================================================================
// LEGACY/PLATFORM-SPECIFIC MODULES
// ============================================================================

/// Platform-specific implementations (VMS, remote execution, etc.)
pub mod platform {
    pub use crate::vms_exit;
    pub use crate::vms_export_symbol;
    pub use crate::vms_progname;
    pub use crate::vmsdir;
    pub use crate::vmsfunctions;
    pub use crate::vmsify;
    pub use crate::vmsjobs;
    pub use crate::posixos;
    pub use crate::remote_cstms;
    pub use crate::remote_stub;
}

// ============================================================================
// RE-EXPORTS FOR CONVENIENCE
// ============================================================================

pub use crate::loadapi::*;     // Main public API (parse, resolve, emit)
pub use crate::build_muf::*;   // Build steel command

// Leaf modules.
pub mod arscan;
pub mod build_muf;
pub mod builder;
pub mod commands;
pub mod compiler;
pub mod config;
pub mod debug;
pub mod def_target_file;
pub mod default;
pub mod dependancies;
pub mod directory;
pub mod error;
pub mod expand;
pub mod externs;
mod editor_setup;
pub mod generator;
pub mod gettext;
pub mod hash;
pub mod implicit;
pub mod interface;
pub mod job;
pub mod load;
pub mod loadapi;
pub mod misc;
pub mod ninja;
pub mod steelcustom;
pub mod steelint;
pub mod os;
pub mod output;
pub mod posixos;
pub mod read;
pub mod remake;
#[path = "remote-cstms.rs"]
pub mod remote_cstms;
#[path = "remote-stub.rs"]
pub mod remote_stub;
pub mod rule;
pub mod shuffle;
pub mod signame;
pub mod strcache;
pub mod target_file;
pub mod validator;
pub mod variable;
pub mod version;
pub mod vms_exit;
pub mod vms_export_symbol;
pub mod vms_progname;
pub mod vmsdir;
pub mod vmsfunctions;
pub mod vmsify;
pub mod vmsjobs;
pub mod vpath;
pub mod warning;

pub use config::Config;
pub use compiler::Compiler;
pub use validator::Validator;
pub use builder::Builder;
pub use generator::Generator;
pub use error::SteelError;
