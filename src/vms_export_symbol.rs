// C:\Users\gogin\Documents\GitHub\steel\src\vms_export_symbol.rs
//
// Steel — VMS (Virtual Steel System) utilities
// Export symbol normalization, validation, and formatting.
//
// Why:
// - The build pipeline may need stable exported symbol names for:
//   - dynamic plugins (dll/so/dylib)
//   - tool entrypoints
//   - runtime hooks / ABI surface
// - Humans provide names with separators / namespaces / punctuation.
// - Linkers/ABIs like conservative identifiers.
//
// What you get:
// - Strict validator (fails fast): C-like symbol charset.
// - Lenient canonicalizer (never fails): sanitize + fallback.
// - Profiles: c_abi(), steel_entrypoints().
// - Helpers: prefixing, component names, and small deterministic folding.
//
// Charset model (strict):
// - First char: [A-Za-z_] (optionally digit)
// - Next chars: [A-Za-z0-9_]
// Notes:
// - We intentionally avoid heavy Unicode normalization dependencies.
// - We implement a pragmatic Latin-1-ish fold for common diacritics.
// - Output is stable and deterministic.

#![allow(dead_code)]

use std::borrow::Cow;
use std::fmt;

pub const DEFAULT_MAX_LEN: usize = 128;
pub const DEFAULT_PREFIX: &str = "steel_";
pub const DEFAULT_FALLBACK: &str = "export";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExportSymbol {
    pub name: String,
    pub source: ExportSymbolSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportSymbolSource {
    AsIs,
    Sanitized,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSymbolError {
    Empty,
    TooLong { max_len: usize, actual: usize },
    InvalidFirstChar { found: char },
    InvalidChar { index: usize, found: char },
}

impl fmt::Display for ExportSymbolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportSymbolError::Empty => write!(f, "export symbol is empty"),
            ExportSymbolError::TooLong { max_len, actual } => {
                write!(f, "export symbol too long: {actual} > {max_len}")
            }
            ExportSymbolError::InvalidFirstChar { found } => {
                write!(f, "invalid first character for export symbol: '{found}'")
            }
            ExportSymbolError::InvalidChar { index, found } => {
                write!(f, "invalid character at index {index} for export symbol: '{found}'")
            }
        }
    }
}

impl std::error::Error for ExportSymbolError {}

#[derive(Debug, Clone)]
pub struct ExportSymbolOptions {
    pub prefix: &'static str,
    pub strict: bool,
    pub max_len: usize,

    pub allow_leading_digit: bool,
    pub allow_leading_underscore: bool,

    pub collapse_underscores: bool,
    pub trim_underscores: bool,

    pub separator: char,

    pub force_lowercase: bool,
    pub force_uppercase: bool,

    pub fallback: &'static str,
}

impl Default for ExportSymbolOptions {
    fn default() -> Self {
        Self {
            prefix: "",
            strict: false,
            max_len: DEFAULT_MAX_LEN,

            allow_leading_digit: false,
            allow_leading_underscore: true,

            collapse_underscores: true,
            trim_underscores: true,

            separator: '_',

            force_lowercase: false,
            force_uppercase: false,

            fallback: DEFAULT_FALLBACK,
        }
    }
}

impl ExportSymbolOptions {
    pub fn c_abi() -> Self {
        Self {
            prefix: "",
            strict: false,
            max_len: DEFAULT_MAX_LEN,
            allow_leading_digit: false,
            allow_leading_underscore: true,
            collapse_underscores: true,
            trim_underscores: true,
            separator: '_',
            force_lowercase: true,
            force_uppercase: false,
            fallback: DEFAULT_FALLBACK,
        }
    }

    pub fn steel_entrypoints() -> Self {
        Self {
            prefix: DEFAULT_PREFIX,
            strict: false,
            max_len: DEFAULT_MAX_LEN,
            allow_leading_digit: false,
            allow_leading_underscore: true,
            collapse_underscores: true,
            trim_underscores: true,
            separator: '_',
            force_lowercase: true,
            force_uppercase: false,
            fallback: "steel_export",
        }
    }

    pub fn strict_c_abi() -> Self {
        let mut s = Self::c_abi();
        s.strict = true;
        s
    }
}

/// Strict validation (does not mutate).
pub fn validate_export_symbol(name: &str, opts: &ExportSymbolOptions) -> Result<(), ExportSymbolError> {
    if name.is_empty() {
        return Err(ExportSymbolError::Empty);
    }

    if opts.max_len > 0 {
        let actual = name.chars().count();
        if actual > opts.max_len {
            return Err(ExportSymbolError::TooLong { max_len: opts.max_len, actual });
        }
    }

    let mut it = name.chars().enumerate();
    let Some((_, first)) = it.next() else { return Err(ExportSymbolError::Empty) };

    let first_ok = is_ascii_alpha(first)
        || (first == '_' && opts.allow_leading_underscore)
        || (is_ascii_digit(first) && opts.allow_leading_digit);

    if !first_ok {
        return Err(ExportSymbolError::InvalidFirstChar { found: first });
    }

    for (i, c) in it {
        if !(is_ascii_alpha(c) || is_ascii_digit(c) || c == '_') {
            return Err(ExportSymbolError::InvalidChar { index: i, found: c });
        }
    }

    Ok(())
}

/// Full entrypoint: strict => validate, lenient => sanitize (never errors).
pub fn resolve_export_symbol(input: &str, opts: &ExportSymbolOptions) -> Result<ExportSymbol, ExportSymbolError> {
    let input = input.trim();

    if opts.strict {
        let mut name = String::new();
        if !opts.prefix.is_empty() {
            // Prefix must already be clean in strict usage. We still apply case rules.
            name.push_str(opts.prefix);
        }
        name.push_str(input);

        let name = apply_case(name, opts);
        validate_export_symbol(&name, opts)?;
        return Ok(ExportSymbol { name, source: ExportSymbolSource::AsIs });
    }

    Ok(sanitize_export_symbol(input, opts))
}

/// Lenient canonicalizer: always returns an ExportSymbol.
/// - Applies prefix (sanitized).
/// - Sanitizes input.
/// - Collapses underscores, trims, enforces leading rules, case, max len.
/// - If empty => fallback.
pub fn sanitize_export_symbol(input: &str, opts: &ExportSymbolOptions) -> ExportSymbol {
    let mut out = String::new();
    let mut changed = false;

    // Prefix
    if !opts.prefix.is_empty() {
        let before = out.clone();
        push_sanitized_str(&mut out, opts.prefix, opts);
        if out != before {
            // if prefix had any odd chars, changed would be true; push_* will normalize.
            // We conservatively mark changed only if prefix differs from raw (heuristic).
            // In practice prefix is a constant, but keep it robust.
            changed |= opts.prefix.chars().any(|c| !is_directly_accepted_char(c));
        }
    }

    // Body
    for c in input.chars() {
        let before_len = out.len();
        push_sanitized_char(&mut out, c, opts);
        if out.len() == before_len {
            changed = true; // dropped
        } else if !is_directly_accepted_char(c) {
            changed = true; // replaced/folded
        }
    }

    // underscore normalization
    let before = out.clone();
    if opts.collapse_underscores {
        out = collapse_char_runs(&out, '_');
    }
    if opts.trim_underscores {
        out = trim_char(&out, '_');
    }
    if out != before {
        changed = true;
    }

    // fallback if empty
    if out.is_empty() {
        let mut name = String::new();
        if !opts.prefix.is_empty() {
            push_sanitized_str(&mut name, opts.prefix, opts);
        }
        // fallback itself
        let fb = apply_case(opts.fallback.to_string(), opts);
        push_sanitized_str(&mut name, &fb, opts);

        name = enforce_leading_rules(name, opts);
        name = enforce_max_len(name, opts);

        return ExportSymbol { name, source: ExportSymbolSource::Fallback };
    }

    // case rules
    let before = out.clone();
    out = apply_case(out, opts);
    if out != before {
        changed = true;
    }

    // leading rules
    let before = out.clone();
    out = enforce_leading_rules(out, opts);
    if out != before {
        changed = true;
    }

    // max len
    let before = out.clone();
    out = enforce_max_len(out, opts);
    if out != before {
        changed = true;
    }

    ExportSymbol {
        name: out,
        source: if changed { ExportSymbolSource::Sanitized } else { ExportSymbolSource::AsIs },
    }
}

pub fn steel_export_symbol(input: &str) -> ExportSymbol {
    sanitize_export_symbol(input, &ExportSymbolOptions::steel_entrypoints())
}

pub fn try_parse_export_symbol(input: &str, opts: &ExportSymbolOptions) -> Result<ExportSymbol, ExportSymbolError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ExportSymbolError::Empty);
    }
    let s = apply_case(s.to_string(), opts);
    validate_export_symbol(&s, opts)?;
    Ok(ExportSymbol { name: s, source: ExportSymbolSource::AsIs })
}

/// Formats a symbol for diagnostics.
pub fn format_symbol(sym: &ExportSymbol) -> String {
    sym.name.clone()
}

/// Formats "prefix + symbol" (safe: ensures exactly one underscore between when desired).
pub fn format_prefixed(prefix: &str, symbol: &str) -> String {
    let p = prefix.trim();
    let s = symbol.trim();
    if p.is_empty() {
        return s.to_string();
    }
    if s.is_empty() {
        return p.to_string();
    }

    if p.ends_with('_') || s.starts_with('_') {
        format!("{p}{s}")
    } else {
        format!("{p}_{s}")
    }
}

/// Formats "prog[component] symbol" style for logs.
pub fn format_component_symbol(component: &str, symbol: &str) -> String {
    let c = component.trim();
    let s = symbol.trim();
    if c.is_empty() {
        s.to_string()
    } else if s.is_empty() {
        c.to_string()
    } else {
        format!("{c}:{s}")
    }
}

/* ========================== internals ========================== */

fn push_sanitized_str(out: &mut String, s: &str, opts: &ExportSymbolOptions) {
    for c in s.chars() {
        push_sanitized_char(out, c, opts);
    }
}

fn push_sanitized_char(out: &mut String, c: char, opts: &ExportSymbolOptions) {
    if is_ascii_alpha(c) || is_ascii_digit(c) || c == '_' {
        out.push(c);
        return;
    }

    // separators => underscore
    if c.is_whitespace()
        || matches!(
            c,
            ':' | '.' | '/' | '\\' | '-' | '+' | '*' | '=' | '@' | '#' | '$' | '%' | '^' | '&' | '|' | '!' | '?' | ',' | ';'
                | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '~'
        )
    {
        out.push(opts.separator);
        return;
    }

    // quotes => drop
    if matches!(c, '"' | '\'' | '`') {
        return;
    }

    // fold some latin chars
    if let Some(folded) = fold_latin_char(c) {
        for fc in folded.chars() {
            if is_ascii_alpha(fc) || is_ascii_digit(fc) || fc == '_' {
                out.push(fc);
            } else {
                out.push(opts.separator);
            }
        }
        return;
    }

    // otherwise drop
}

fn fold_latin_char(c: char) -> Option<Cow<'static, str>> {
    let s = match c {
        // lower
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'ď' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' => "e",
        'ğ' | 'ģ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => "i",
        'ñ' | 'ń' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ŕ' | 'ř' => "r",
        'ś' | 'š' => "s",
        'ť' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ž' => "z",

        // upper
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => "A",
        'Ç' | 'Ć' | 'Č' => "C",
        'Ď' => "D",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' => "E",
        'Ğ' | 'Ģ' => "G",
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Į' => "I",
        'Ñ' | 'Ń' => "N",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' => "O",
        'Ŕ' | 'Ř' => "R",
        'Ś' | 'Š' => "S",
        'Ť' => "T",
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => "U",
        'Ý' => "Y",
        'Ź' | 'Ž' => "Z",

        // ligatures / special
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'ß' => "ss",

        _ => return None,
    };
    Some(Cow::Borrowed(s))
}

fn apply_case(s: String, opts: &ExportSymbolOptions) -> String {
    if opts.force_lowercase && opts.force_uppercase {
        return s.to_ascii_lowercase();
    }
    if opts.force_lowercase {
        return s.to_ascii_lowercase();
    }
    if opts.force_uppercase {
        return s.to_ascii_uppercase();
    }
    s
}

fn enforce_leading_rules(mut s: String, opts: &ExportSymbolOptions) -> String {
    if s.is_empty() {
        return s;
    }

    let first = s.chars().next().unwrap();

    if is_ascii_digit(first) && !opts.allow_leading_digit {
        s.insert(0, '_');
    }

    if s.starts_with('_') && !opts.allow_leading_underscore {
        s.remove(0);
        s.insert(0, 'x');
    }

    s
}

fn enforce_max_len(s: String, opts: &ExportSymbolOptions) -> String {
    if opts.max_len == 0 {
        return s;
    }
    let n = s.chars().count();
    if n <= opts.max_len {
        return s;
    }
    s.chars().take(opts.max_len).collect()
}

fn collapse_char_runs(s: &str, ch: char) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_is = false;
    for c in s.chars() {
        if c == ch {
            if !prev_is {
                out.push(c);
                prev_is = true;
            }
        } else {
            out.push(c);
            prev_is = false;
        }
    }
    out
}

fn trim_char(s: &str, ch: char) -> String {
    let mut start = 0usize;
    let mut end = s.len();

    for (i, c) in s.char_indices() {
        if c == ch {
            start = i + c.len_utf8();
        } else {
            break;
        }
    }

    for (i, c) in s.char_indices().rev() {
        if c == ch {
            end = i;
        } else {
            break;
        }
    }

    if start >= end {
        String::new()
    } else {
        s[start..end].to_string()
    }
}

#[inline]
fn is_ascii_alpha(c: char) -> bool {
    c.is_ascii_alphabetic()
}

#[inline]
fn is_ascii_digit(c: char) -> bool {
    c.is_ascii_digit()
}

#[inline]
fn is_directly_accepted_char(c: char) -> bool {
    is_ascii_alpha(c) || is_ascii_digit(c) || c == '_'
}

/* ============================ tests ============================ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_validation_accepts_basic() {
        let opts = ExportSymbolOptions::strict_c_abi();
        validate_export_symbol("steel_plugin_init", &opts).unwrap();
    }

    #[test]
    fn strict_validation_rejects_dash() {
        let opts = ExportSymbolOptions::strict_c_abi();
        let err = validate_export_symbol("steel-plugin", &opts).unwrap_err();
        assert!(matches!(err, ExportSymbolError::InvalidChar { .. }));
    }

    #[test]
    fn sanitize_replaces_separators() {
        let opts = ExportSymbolOptions::c_abi();
        let sym = sanitize_export_symbol("steel.plugin:init", &opts);
        assert_eq!(sym.name, "steel_plugin_init");
        assert_eq!(sym.source, ExportSymbolSource::Sanitized);
    }

    #[test]
    fn sanitize_folds_accents() {
        let opts = ExportSymbolOptions::c_abi();
        let sym = sanitize_export_symbol("éxport-œuvre", &opts);
        assert_eq!(sym.name, "export_oeuvre");
    }

    #[test]
    fn sanitize_prefix_entrypoint() {
        let opts = ExportSymbolOptions::steel_entrypoints();
        let sym = sanitize_export_symbol("plugin.init", &opts);
        assert_eq!(sym.name, "steel_plugin_init");
    }

    #[test]
    fn leading_digit_gets_prefixed() {
        let opts = ExportSymbolOptions::c_abi();
        let sym = sanitize_export_symbol("123start", &opts);
        assert_eq!(sym.name, "_123start");
    }

    #[test]
    fn collapse_and_trim_underscores() {
        let opts = ExportSymbolOptions::c_abi();
        let sym = sanitize_export_symbol("..a---b..", &opts);
        assert_eq!(sym.name, "a_b");
    }

    #[test]
    fn fallback_when_empty() {
        let opts = ExportSymbolOptions::c_abi();
        let sym = sanitize_export_symbol("$$$###", &opts);
        assert_eq!(sym.name, "export");
        assert_eq!(sym.source, ExportSymbolSource::Fallback);
    }

    #[test]
    fn max_len_enforced() {
        let mut opts = ExportSymbolOptions::c_abi();
        opts.max_len = 5;
        let sym = sanitize_export_symbol("abcdef", &opts);
        assert_eq!(sym.name, "abcde");
    }

    #[test]
    fn steel_export_symbol_helper() {
        let sym = steel_export_symbol("Build::Plan");
        assert_eq!(sym.name, "steel_build_plan");
    }

    #[test]
    fn format_prefixed_joins_cleanly() {
        assert_eq!(format_prefixed("steel", "init"), "steel_init");
        assert_eq!(format_prefixed("steel_", "init"), "steel_init");
        assert_eq!(format_prefixed("steel", "_init"), "steel_init");
    }

    #[test]
    fn format_component_symbol_basic() {
        assert_eq!(format_component_symbol("vms", "sym"), "vms:sym");
        assert_eq!(format_component_symbol("", "sym"), "sym");
        assert_eq!(format_component_symbol("vms", ""), "vms");
    }
}
