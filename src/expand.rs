// /Users/vincent/Documents/Github/steel/src/expand.rs
//! expand — macro + variable expansion (std-only)
//!
//! This module implements a small, deterministic expansion engine used by Steel:
//! - `${var}` and `$var` variable references
//! - `$(env:NAME)` environment reads (optional)
//! - `$(path:join a b c)` path joins (lexical, platform-aware)
//! - `$(lower ...)`, `$(upper ...)` string transforms
//! - `$(if cond then else)` with simple truthiness
//!
//! The engine is intentionally conservative and does not execute shell.
//! Expansion is pure and side-effect free except optional env reads.
//!
//! Determinism:
//! - no random, no time, no filesystem calls
//! - variables are resolved from an explicit `Vars` map
//!
//! Error handling:
//! - `expand()` returns either expanded string or `ExpandError`
//! - you can choose strict vs best-effort behavior
//!
//! Syntax summary:
//! - literal text is copied as-is
//! - escape `$` with `$$`
//! - `${name}` expands variable `name`
//! - `$name` expands variable `name` where name matches `[A-Za-z_][A-Za-z0-9._-]*`
//! - `$(...)` calls a function:
//!     - `env:NAME`
//!     - `path:join <a> <b> ...`
//!     - `lower <text>`
//!     - `upper <text>`
//!     - `if <cond> <then> <else>`
//!
//! Nested expansions are supported inside `${...}` and function arguments.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

pub type Vars = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fail on first error.
    Strict,
    /// On errors, keep the original token as literal.
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandError {
    pub kind: ExpandErrorKind,
    pub span: (usize, usize),
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandErrorKind {
    Unterminated,
    UnknownVar,
    UnknownFunc,
    BadSyntax,
    RecursionLimit,
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} at {}..{}: {}",
            self.kind, self.span.0, self.span.1, self.message
        )
    }
}

impl std::error::Error for ExpandError {}

#[derive(Debug, Clone)]
pub struct ExpandOptions {
    pub mode: Mode,
    pub allow_env: bool,
    pub recursion_limit: usize,
    pub base_dir: Option<PathBuf>,
}

impl Default for ExpandOptions {
    fn default() -> Self {
        Self {
            mode: Mode::Strict,
            allow_env: true,
            recursion_limit: 32,
            base_dir: None,
        }
    }
}

/// Expand `input` using variables in `vars`.
pub fn expand(input: &str, vars: &Vars, opts: &ExpandOptions) -> Result<String, ExpandError> {
    expand_inner(input, vars, opts, 0)
}

fn expand_inner(input: &str, vars: &Vars, opts: &ExpandOptions, depth: usize) -> Result<String, ExpandError> {
    if depth > opts.recursion_limit {
        return Err(ExpandError {
            kind: ExpandErrorKind::RecursionLimit,
            span: (0, input.len()),
            message: format!("recursion limit exceeded ({})", opts.recursion_limit),
        });
    }

    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut out = String::with_capacity(input.len() + 8);

    while i < bytes.len() {
        let b = bytes[i];
        if b != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // '$' handling
        if i + 1 >= bytes.len() {
            // trailing '$' literal
            out.push('$');
            i += 1;
            continue;
        }

        let next = bytes[i + 1];
        match next {
            b'$' => {
                // escape $$ -> $
                out.push('$');
                i += 2;
            }
            b'{' => {
                // ${...}
                let (body, end) = read_balanced(input, i + 2, b'{', b'}')
                    .ok_or_else(|| ExpandError {
                        kind: ExpandErrorKind::Unterminated,
                        span: (i, input.len()),
                        message: "unterminated ${...}".to_string(),
                    })?;

                let expanded_key = expand_inner(body, vars, opts, depth + 1)?;
                let val = match vars.get(expanded_key.trim()) {
                    Some(v) => v.clone(),
                    None => {
                        if opts.mode == Mode::BestEffort {
                            // keep literal
                            out.push_str(&input[i..end]);
                            i = end;
                            continue;
                        }
                        return Err(ExpandError {
                            kind: ExpandErrorKind::UnknownVar,
                            span: (i, end),
                            message: format!("unknown var: {}", expanded_key.trim()),
                        });
                    }
                };

                out.push_str(&val);
                i = end;
            }
            b'(' => {
                // $(...)
                let (body, end) = read_balanced(input, i + 2, b'(', b')')
                    .ok_or_else(|| ExpandError {
                        kind: ExpandErrorKind::Unterminated,
                        span: (i, input.len()),
                        message: "unterminated $(...)".to_string(),
                    })?;

                let rep = match eval_func(body, vars, opts, depth + 1) {
                    Ok(s) => s,
                    Err(e) => {
                        if opts.mode == Mode::BestEffort {
                            out.push_str(&input[i..end]);
                            i = end;
                            continue;
                        }
                        return Err(e);
                    }
                };

                out.push_str(&rep);
                i = end;
            }
            _ => {
                // $name
                if is_ident_start(next as char) {
                    let start = i + 1;
                    let mut j = start;
                    while j < bytes.len() && is_ident_continue(bytes[j] as char) {
                        j += 1;
                    }
                    let key = &input[start..j];
                    if let Some(v) = vars.get(key) {
                        out.push_str(v);
                    } else if opts.mode == Mode::BestEffort {
                        out.push_str(&input[i..j]);
                    } else {
                        return Err(ExpandError {
                            kind: ExpandErrorKind::UnknownVar,
                            span: (i, j),
                            message: format!("unknown var: {key}"),
                        });
                    }
                    i = j;
                } else {
                    // '$' followed by non-special -> literal '$'
                    out.push('$');
                    i += 1;
                }
            }
        }
    }

    Ok(out)
}

fn eval_func(body: &str, vars: &Vars, opts: &ExpandOptions, depth: usize) -> Result<String, ExpandError> {
    // Trim, then parse as tokens split by whitespace, BUT preserve quoted strings.
    let tokens = split_tokens(body);

    if tokens.is_empty() {
        return Err(ExpandError {
            kind: ExpandErrorKind::BadSyntax,
            span: (0, body.len()),
            message: "empty function call".to_string(),
        });
    }

    // Support `env:NAME` in a single token
    if let Some((head, rest)) = tokens[0].split_once(':') {
        match head {
            "env" => {
                if rest.trim().is_empty() {
                    return Err(ExpandError {
                        kind: ExpandErrorKind::BadSyntax,
                        span: (0, body.len()),
                        message: "env:NAME requires NAME".to_string(),
                    });
                }
                if !opts.allow_env {
                    return Err(ExpandError {
                        kind: ExpandErrorKind::BadSyntax,
                        span: (0, body.len()),
                        message: "env expansion disabled".to_string(),
                    });
                }
                return Ok(env::var(rest).unwrap_or_default());
            }
            "path" => {
                // path:<op> ...
                let op = rest;
                return eval_path_func(op, &tokens[1..], vars, opts, depth, body);
            }
            _ => {}
        }
    }

    match tokens[0].as_str() {
        "env" => {
            if tokens.len() != 2 {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "env NAME".to_string(),
                });
            }
            if !opts.allow_env {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "env expansion disabled".to_string(),
                });
            }
            Ok(env::var(&tokens[1]).unwrap_or_default())
        }
        "lower" => {
            let s = join_rest(&tokens[1..]);
            let s = expand_inner(&s, vars, opts, depth)?;
            Ok(s.to_ascii_lowercase())
        }
        "upper" => {
            let s = join_rest(&tokens[1..]);
            let s = expand_inner(&s, vars, opts, depth)?;
            Ok(s.to_ascii_uppercase())
        }
        "if" => {
            // if <cond> <then> <else>
            if tokens.len() < 4 {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "if <cond> <then> <else>".to_string(),
                });
            }
            let cond = expand_inner(&tokens[1], vars, opts, depth)?;
            let then_s = expand_inner(&tokens[2], vars, opts, depth)?;
            let else_s = expand_inner(&tokens[3], vars, opts, depth)?;
            Ok(if truthy(&cond) { then_s } else { else_s })
        }
        "path" => {
            // path join a b c
            if tokens.len() < 2 {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "path <op> ...".to_string(),
                });
            }
            eval_path_func(&tokens[1], &tokens[2..], vars, opts, depth, body)
        }
        other => Err(ExpandError {
            kind: ExpandErrorKind::UnknownFunc,
            span: (0, tokens[0].len()),
            message: format!("unknown func: {other}"),
        }),
    }
}

fn eval_path_func(
    op: &str,
    args: &[String],
    vars: &Vars,
    opts: &ExpandOptions,
    depth: usize,
    body: &str,
) -> Result<String, ExpandError> {
    match op {
        "join" => {
            if args.is_empty() {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "path:join requires at least one arg".to_string(),
                });
            }
            let mut parts = Vec::new();
            for a in args {
                let ex = expand_inner(a, vars, opts, depth)?;
                if !ex.trim().is_empty() {
                    parts.push(ex);
                }
            }
            let mut pb = PathBuf::new();
            for p in parts {
                pb.push(Path::new(&p));
            }
            if let Some(base) = &opts.base_dir {
                if !pb.is_absolute() {
                    pb = base.join(pb);
                }
            }
            Ok(pb.to_string_lossy().to_string())
        }
        "file" => {
            // path:file <path> => file name
            if args.len() != 1 {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "path:file <path>".to_string(),
                });
            }
            let p = expand_inner(&args[0], vars, opts, depth)?;
            let pb = PathBuf::from(p);
            Ok(pb.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
        }
        "dir" => {
            // path:dir <path> => parent dir
            if args.len() != 1 {
                return Err(ExpandError {
                    kind: ExpandErrorKind::BadSyntax,
                    span: (0, body.len()),
                    message: "path:dir <path>".to_string(),
                });
            }
            let p = expand_inner(&args[0], vars, opts, depth)?;
            let pb = PathBuf::from(p);
            Ok(pb.parent().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
        }
        _ => Err(ExpandError {
            kind: ExpandErrorKind::UnknownFunc,
            span: (0, body.len()),
            message: format!("unknown path op: {op}"),
        }),
    }
}

fn truthy(s: &str) -> bool {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    !matches!(t.as_str(), "0" | "false" | "no" | "off" | "null" | "none")
}

fn split_tokens(s: &str) -> Vec<String> {
    // whitespace split with basic quoting: "..." or '...'
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        if let Some(q) = in_quote {
            if ch == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(match n {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '"' => '"',
                        '\'' => '\'',
                        other => other,
                    });
                }
                continue;
            }
            if ch == q {
                in_quote = None;
                continue;
            }
            cur.push(ch);
            continue;
        }

        match ch {
            '"' | '\'' => {
                in_quote = Some(ch);
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                }
            }
            _ => cur.push(ch),
        }
    }

    if !cur.is_empty() {
        out.push(cur);
    }

    out
}

fn join_rest(args: &[String]) -> String {
    let mut s = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(a);
    }
    s
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// Read a balanced bracket expression starting at `start` (right after opening bracket).
/// Returns (slice_inside, end_index_after_closing).
fn read_balanced(input: &str, start: usize, open: u8, close: u8) -> Option<(&str, usize)> {
    let bytes = input.as_bytes();
    let mut depth = 1usize;
    let mut i = start;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            // skip escaped char
            i += 2;
            continue;
        }
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                // body is start..i
                return Some((&input[start..i], i + 1));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_vars() {
        let mut v = Vars::new();
        v.insert("name".into(), "world".into());
        let opts = ExpandOptions::default();
        assert_eq!(expand("hello $name", &v, &opts).unwrap(), "hello world");
        assert_eq!(expand("hello ${name}", &v, &opts).unwrap(), "hello world");
        assert_eq!(expand("$$", &v, &opts).unwrap(), "$");
    }

    #[test]
    fn expands_functions() {
        let mut v = Vars::new();
        v.insert("A".into(), "TeSt".into());
        let mut opts = ExpandOptions::default();
        opts.allow_env = false;

        assert_eq!(expand("$(lower $A)", &v, &opts).unwrap(), "test");
        assert_eq!(expand("$(upper ${A})", &v, &opts).unwrap(), "TEST");
        assert_eq!(expand("$(if 1 yes no)", &v, &opts).unwrap(), "yes");
        assert_eq!(expand("$(if false yes no)", &v, &opts).unwrap(), "no");
    }

    #[test]
    fn best_effort_keeps_literal() {
        let v = Vars::new();
        let mut opts = ExpandOptions::default();
        opts.mode = Mode::BestEffort;

        assert_eq!(expand("x=$missing", &v, &opts).unwrap(), "x=$missing");
        assert_eq!(expand("y=$(nope 1 2)", &v, &opts).unwrap(), "y=$(nope 1 2)");
    }
}