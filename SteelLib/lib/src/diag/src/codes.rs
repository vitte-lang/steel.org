//! Diagnostic codes registry.
//!
//! This module centralizes diagnostic code definitions for SteelLib.
//!
//! Goals:
//! - stable string codes for tooling (LSP, JSON outputs, CI parsing)
//! - consistent severity mapping
//! - machine-readable metadata (category, default message)
//!
//! Conventions (recommended):
//! - Prefix by subsystem:
//!   - MFF / manifest:   MFFxxxx
//!   - MUF / buildfile:  MUFxxxx
//!   - CAPS / capsule:   CAPSxxx
//!   - CLI:              CLIxxxx
//!   - FS:               FSxxxx
//!   - NET:              NETxxxx
//!   - COMP:             COMPxxx
//!   - REG / registry:   REGxxxx
//! - Numeric ranges by stage:
//!   - 0xxx: parse/lex
//!   - 1xxx: validation/semantic
//!   - 2xxx: resolution/link
//!   - 3xxx: execution/runtime
//!
//! This file is intentionally std-only.

use core::fmt;

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Something is invalid; execution should fail.
    Error,
    /// Something is suspicious; execution may continue but is risky.
    Warning,
    /// Informational message.
    Note,
    /// Optional help / hint (often paired with Error/Warning).
    Help,
}

impl Severity {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Help => "help",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// High-level category for grouping diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Manifest,
    BuildFile,
    Capsule,
    Cli,
    Fs,
    Net,
    Compiler,
    Registry,
    Runtime,
    Internal,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Manifest => "manifest",
            Category::BuildFile => "buildfile",
            Category::Capsule => "capsule",
            Category::Cli => "cli",
            Category::Fs => "fs",
            Category::Net => "net",
            Category::Compiler => "compiler",
            Category::Registry => "registry",
            Category::Runtime => "runtime",
            Category::Internal => "internal",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stable diagnostic code and its metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagCode {
    pub code: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub name: &'static str,
    pub default_message: &'static str,
}

impl DiagCode {
    pub const fn new(
        code: &'static str,
        severity: Severity,
        category: Category,
        name: &'static str,
        default_message: &'static str,
    ) -> Self {
        Self {
            code,
            severity,
            category,
            name,
            default_message,
        }
    }
}

impl fmt::Display for DiagCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.code, self.name)
    }
}

/* ----------------------------- Registry ---------------------------------- */

/// Manifest (.mff) parse error.
pub const MFF0001: DiagCode = DiagCode::new(
    "MFF0001",
    Severity::Error,
    Category::Manifest,
    "manifest_parse_error",
    "Failed to parse manifest.",
);

/// Manifest missing required field.
pub const MFF0101: DiagCode = DiagCode::new(
    "MFF0101",
    Severity::Error,
    Category::Manifest,
    "manifest_missing_field",
    "Manifest is missing a required field.",
);

/// Manifest invalid field value.
pub const MFF0102: DiagCode = DiagCode::new(
    "MFF0102",
    Severity::Error,
    Category::Manifest,
    "manifest_invalid_value",
    "Manifest contains an invalid value.",
);

/// Buildfile (.muf) parse error.
pub const MUF0001: DiagCode = DiagCode::new(
    "MUF0001",
    Severity::Error,
    Category::BuildFile,
    "buildfile_parse_error",
    "Failed to parse build file.",
);

/// Buildfile semantic/validation error.
pub const MUF0101: DiagCode = DiagCode::new(
    "MUF0101",
    Severity::Error,
    Category::BuildFile,
    "buildfile_validation_failed",
    "Build file validation failed.",
);

/// Unknown target triple.
pub const MUF0201: DiagCode = DiagCode::new(
    "MUF0201",
    Severity::Error,
    Category::BuildFile,
    "unknown_target",
    "Unknown or unsupported target.",
);

/// Capsule policy parse/validation error.
pub const CAPS0001: DiagCode = DiagCode::new(
    "CAPS0001",
    Severity::Error,
    Category::Capsule,
    "capsule_policy_error",
    "Invalid capsule policy.",
);

/// Capsule denial at runtime.
pub const CAPS0301: DiagCode = DiagCode::new(
    "CAPS0301",
    Severity::Error,
    Category::Capsule,
    "capsule_denied",
    "Operation denied by capsule policy.",
);

/// CLI argument parse error.
pub const CLI0001: DiagCode = DiagCode::new(
    "CLI0001",
    Severity::Error,
    Category::Cli,
    "cli_parse_error",
    "Failed to parse CLI arguments.",
);

/// CLI deprecated option.
pub const CLI1001: DiagCode = DiagCode::new(
    "CLI1001",
    Severity::Warning,
    Category::Cli,
    "cli_deprecated_option",
    "This CLI option is deprecated.",
);

/// Filesystem error (I/O).
pub const FS0301: DiagCode = DiagCode::new(
    "FS0301",
    Severity::Error,
    Category::Fs,
    "fs_io_error",
    "Filesystem operation failed.",
);

/// Network backend unavailable / denied.
pub const NET0301: DiagCode = DiagCode::new(
    "NET0301",
    Severity::Error,
    Category::Net,
    "net_unavailable_or_denied",
    "Network operation unavailable or denied.",
);

/// Compiler invocation failed.
pub const COMP0301: DiagCode = DiagCode::new(
    "COMP0301",
    Severity::Error,
    Category::Compiler,
    "compiler_failed",
    "Compiler invocation failed.",
);

/// Registry signature invalid.
pub const REG0101: DiagCode = DiagCode::new(
    "REG0101",
    Severity::Error,
    Category::Registry,
    "registry_signature_invalid",
    "Registry signature is invalid.",
);

/// Internal invariant violated.
pub const INT9001: DiagCode = DiagCode::new(
    "INT9001",
    Severity::Error,
    Category::Internal,
    "internal_invariant",
    "Internal invariant violated.",
);

/// All known diagnostic codes (registry).
///
/// Keep this list ordered by code for predictable iteration.
pub const ALL: &[DiagCode] = &[
    CAPS0001,
    CAPS0301,
    CLI0001,
    CLI1001,
    COMP0301,
    FS0301,
    INT9001,
    MFF0001,
    MFF0101,
    MFF0102,
    MUF0001,
    MUF0101,
    MUF0201,
    NET0301,
    REG0101,
];

/* ----------------------------- Lookup ------------------------------------ */

/// Lookup a diagnostic code by its string (e.g. `"MUF0101"`).
pub fn lookup(code: &str) -> Option<&'static DiagCode> {
    // Small list: linear scan is fine; if this grows, use phf or a perfect hash generator.
    ALL.iter().find(|c| c.code == code)
}

/// Lookup by `name` (e.g. `"buildfile_validation_failed"`).
pub fn lookup_name(name: &str) -> Option<&'static DiagCode> {
    ALL.iter().find(|c| c.name == name)
}

/* ----------------------------- Formatting -------------------------------- */

/// Render a concise single-line form.
pub fn format_one_line(code: &DiagCode, message: Option<&str>) -> String {
    let msg = message.unwrap_or(code.default_message);
    format!("{}: {}", code.code, msg)
}

/// Render a stable JSON-like map (no serde dependency).
pub fn to_kv_map(code: &DiagCode) -> [(&'static str, &'static str); 4] {
    [
        ("code", code.code),
        ("severity", code.severity.as_str()),
        ("category", code.category.as_str()),
        ("name", code.name),
    ]
}

/* ----------------------------- Tests ------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known() {
        let c = lookup("MUF0101").unwrap();
        assert_eq!(c.name, "buildfile_validation_failed");
        assert_eq!(c.severity, Severity::Error);
        assert_eq!(c.category, Category::BuildFile);
    }

    #[test]
    fn lookup_unknown() {
        assert!(lookup("NOPE").is_none());
    }

    #[test]
    fn names_unique() {
        let mut set = std::collections::BTreeSet::new();
        for c in ALL {
            assert!(set.insert(c.name), "duplicate name: {}", c.name);
        }
    }

    #[test]
    fn codes_unique() {
        let mut set = std::collections::BTreeSet::new();
        for c in ALL {
            assert!(set.insert(c.code), "duplicate code: {}", c.code);
        }
    }

    #[test]
    fn format_one_line_works() {
        let s = format_one_line(&MFF0001, None);
        assert!(s.starts_with("MFF0001: "));
    }
}
