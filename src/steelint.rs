// src/steelint.rs
//
// Steel — "steelint" (internal integer utilities)
//
// Purpose:
// - Provide small, dependency-free integer helpers used across Steel:
//   - parsing ints with strict rules (no locale, no underscores by default)
//   - safe conversions (checked / saturating / clamped)
//   - human-friendly formatting (bytes, durations) when needed by CLI/output
//   - stable hashing helpers for numeric keys
//
// Naming:
// - "steelint" is an internal util module; adapt to your crate layout.
//
// Notes:
// - Prefer explicit behavior: return Result with clear errors.
// - No allocations on hot paths unless formatting/humanization is requested.

#![allow(dead_code)]

use std::fmt;
use std::num::{IntErrorKind, ParseIntError};

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntParseError {
    Empty,
    Invalid {
        input: String,
        reason: String,
    },
    Overflow {
        input: String,
    },
}

impl fmt::Display for IntParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntParseError::Empty => write!(f, "empty integer"),
            IntParseError::Invalid { input, reason } => write!(f, "invalid integer '{input}': {reason}"),
            IntParseError::Overflow { input } => write!(f, "integer overflow '{input}'"),
        }
    }
}

impl std::error::Error for IntParseError {}

/* ============================== strict parsing ============================== */

/// Parse i64 with strict rules:
/// - trims whitespace
/// - accepts optional leading +/-
/// - base 10 only (unless `0x`/`0b`/`0o` prefixes are present if `allow_prefix_base` = true)
/// - rejects underscores unless allow_underscores
pub fn parse_i64_strict(
    s: &str,
    allow_prefix_base: bool,
    allow_underscores: bool,
) -> Result<i64, IntParseError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(IntParseError::Empty);
    }

    let (neg, rest) = match t.as_bytes()[0] {
        b'+' => (false, &t[1..]),
        b'-' => (true, &t[1..]),
        _ => (false, t),
    };

    let (radix, digits) = if allow_prefix_base && rest.len() > 2 && &rest[..2] == "0x" {
        (16, &rest[2..])
    } else if allow_prefix_base && rest.len() > 2 && &rest[..2] == "0b" {
        (2, &rest[2..])
    } else if allow_prefix_base && rest.len() > 2 && &rest[..2] == "0o" {
        (8, &rest[2..])
    } else {
        (10, rest)
    };

    if digits.is_empty() {
        return Err(IntParseError::Empty);
    }

    if !allow_underscores && digits.contains('_') {
        return Err(IntParseError::Invalid {
            input: s.to_string(),
            reason: "underscores not allowed".to_string(),
        });
    }

    let cleaned;
    let digits = if allow_underscores {
        cleaned = digits.replace('_', "");
        cleaned.as_str()
    } else {
        digits
    };

    let unsigned = u64::from_str_radix(digits, radix).map_err(|e| map_parse_error(s, e))?;
    if neg {
        if unsigned > (i64::MAX as u64) + 1 {
            return Err(IntParseError::Overflow { input: s.to_string() });
        }
        if unsigned == (i64::MAX as u64) + 1 {
            return Ok(i64::MIN);
        }
        return Ok(-(unsigned as i64));
    }

    if unsigned > i64::MAX as u64 {
        return Err(IntParseError::Overflow { input: s.to_string() });
    }
    Ok(unsigned as i64)
}

fn map_parse_error(input: &str, err: ParseIntError) -> IntParseError {
    match err.kind() {
        IntErrorKind::Empty => IntParseError::Empty,
        IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => {
            IntParseError::Overflow { input: input.to_string() }
        }
        _ => IntParseError::Invalid {
            input: input.to_string(),
            reason: err.to_string(),
        },
    }
}
