//! Glob utilities (glob.rs) — MAX (std-only).
//!
//! Steel uses globs for:
//! - selecting source files (e.g. `src/**/*.c`)
//! - selecting manifest inputs (e.g. `**/*.muf`)
//! - excluding generated output (e.g. `!build/**`)
//!
//! This module implements a small glob engine with these features:
//! - `*` matches any chars except path separator
//! - `?` matches one char except path separator
//! - `**` matches across path separators (zero or more segments)
//! - character classes: `[abc]`, `[a-z]`, `[^a-z]`
//! - braces: `{a,b,c}` simple alternation (no nesting)
//! - negation patterns with leading `!`
//! - normalization to forward slashes for stable matching
//!
//! Filesystem enumeration:
//! - `GlobSet::walk()` enumerates paths under a root and matches inclusions/exclusions
//! - Uses std::fs recursion; does not follow symlinks by default
//! - Deterministic ordering by sorting results
//!
//! Notes:
//! - This is intentionally not a full Bash glob implementation.
//! - Performance is fine for repo-sized trees; if you need more, plug in `globset` crate.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum GlobError {
    InvalidPattern(String),
    Io(std::io::Error),
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlobError::InvalidPattern(s) => write!(f, "invalid glob pattern: {s}"),
            GlobError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for GlobError {}

impl From<std::io::Error> for GlobError {
    fn from(e: std::io::Error) -> Self {
        GlobError::Io(e)
    }
}

/// A single glob pattern (with optional negation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobPattern {
    /// Original input string.
    pub raw: String,
    /// If true, pattern is an exclusion (`!pat`).
    pub is_negated: bool,
    /// Expanded patterns (brace alternation), each compiled to tokens.
    compiled: Vec<CompiledGlob>,
}

impl GlobPattern {
    pub fn parse(s: impl Into<String>) -> Result<Self, GlobError> {
        let raw0 = s.into();
        let (is_negated, raw) = if let Some(rest) = raw0.strip_prefix('!') {
            (true, rest.to_string())
        } else {
            (false, raw0.clone())
        };

        if raw.is_empty() {
            return Err(GlobError::InvalidPattern("empty".into()));
        }

        let expanded = expand_braces(&raw)?;
        let mut compiled = Vec::with_capacity(expanded.len());
        for p in expanded {
            compiled.push(compile_glob(&p)?);
        }

        Ok(Self {
            raw: raw0,
            is_negated,
            compiled,
        })
    }

    /// Match a normalized unix path string (forward slashes).
    pub fn matches_str(&self, norm_path: &str) -> bool {
        self.compiled.iter().any(|c| c.matches(norm_path))
    }

    /// Match a filesystem path by normalizing to forward slashes.
    pub fn matches_path(&self, p: &Path) -> bool {
        self.matches_str(&normalize_path(p))
    }
}

/// A set of patterns (includes/excludes).
#[derive(Debug, Clone, Default)]
pub struct GlobSet {
    patterns: Vec<GlobPattern>,
}

impl GlobSet {
    pub fn new() -> Self {
        Self { patterns: Vec::new() }
    }

    pub fn push(&mut self, pat: GlobPattern) {
        self.patterns.push(pat);
    }

    pub fn add(&mut self, s: impl Into<String>) -> Result<(), GlobError> {
        self.push(GlobPattern::parse(s)?);
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Determine if a path matches according to include/exclude rules.
    ///
    /// Semantics:
    /// - Start as not included.
    /// - For each pattern in order:
    ///   - If include pattern matches => included = true
    ///   - If exclude pattern matches => included = false
    ///
    /// If no include patterns exist, default include = true (then apply excludes).
    pub fn matches(&self, p: &Path) -> bool {
        let has_includes = self.patterns.iter().any(|x| !x.is_negated);
        let mut included = !has_includes;

        let s = normalize_path(p);
        for pat in &self.patterns {
            if pat.matches_str(&s) {
                included = !pat.is_negated;
            }
        }
        included
    }

    /// Walk filesystem under `root`, returning matching paths.
    ///
    /// - returns files only by default; directories can be included with `WalkOptions`.
    /// - does not follow symlinks by default.
    /// - deterministic ordering: results sorted.
    pub fn walk(&self, root: impl AsRef<Path>, opt: WalkOptions) -> Result<Vec<PathBuf>, GlobError> {
        let root = root.as_ref();
        let mut out: BTreeSet<PathBuf> = BTreeSet::new();
        walk_rec(self, root, root, &mut out, &opt)?;
        Ok(out.into_iter().collect())
    }
}

#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub include_files: bool,
    pub include_dirs: bool,
    pub follow_symlinks: bool,
    /// Skip entries whose file name starts with '.'.
    pub skip_hidden: bool,
    /// Max recursion depth (None = unlimited).
    pub max_depth: Option<usize>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            include_files: true,
            include_dirs: false,
            follow_symlinks: false,
            skip_hidden: false,
            max_depth: None,
        }
    }
}

/* ------------------------------- Walking -------------------------------- */

fn walk_rec(
    set: &GlobSet,
    root: &Path,
    cur: &Path,
    out: &mut BTreeSet<PathBuf>,
    opt: &WalkOptions,
) -> Result<(), GlobError> {
    let depth = cur.strip_prefix(root).ok().map(|p| p.components().count()).unwrap_or(0);
    if let Some(max) = opt.max_depth {
        if depth > max {
            return Ok(());
        }
    }

    let md = std::fs::symlink_metadata(cur)?;
    let ft = md.file_type();

    if ft.is_symlink() && !opt.follow_symlinks {
        return Ok(());
    }

    let is_dir = ft.is_dir();
    let is_file = ft.is_file();

    // filter hidden
    if opt.skip_hidden {
        if let Some(name) = cur.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && cur != root {
                return Ok(());
            }
        }
    }

    // evaluate current path relative to root (for matching), but match using full relative
    let rel = cur.strip_prefix(root).unwrap_or(cur);

    if is_dir && opt.include_dirs && set.matches(rel) {
        out.insert(cur.to_path_buf());
    }
    if is_file && opt.include_files && set.matches(rel) {
        out.insert(cur.to_path_buf());
    }

    if is_dir {
        // deterministic: read_dir then sort
        let mut kids: Vec<PathBuf> = Vec::new();
        for e in std::fs::read_dir(cur)? {
            let e = e?;
            kids.push(e.path());
        }
        kids.sort();
        for k in kids {
            walk_rec(set, root, &k, out, opt)?;
        }
    }

    Ok(())
}

/* ------------------------------ Matching -------------------------------- */

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Lit(String),
    Star,
    Qmark,
    Slash,
    DoubleStar,
    Class(CharClass),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CharClass {
    negate: bool,
    ranges: Vec<(char, char)>,
    singles: Vec<char>,
}

impl CharClass {
    fn matches(&self, c: char) -> bool {
        let mut ok = self.singles.contains(&c);
        if !ok {
            for (a, b) in &self.ranges {
                if *a <= c && c <= *b {
                    ok = true;
                    break;
                }
            }
        }
        if self.negate { !ok } else { ok }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledGlob {
    raw: String,
    toks: Vec<Token>,
}

impl CompiledGlob {
    fn matches(&self, s: &str) -> bool {
        // DP backtracking matcher over tokens and input chars
        // `**` can match across slashes.
        let chars: Vec<char> = s.chars().collect();
        match_tokens(&self.toks, &chars, 0, 0)
    }
}

fn match_tokens(toks: &[Token], chars: &[char], ti: usize, ci: usize) -> bool {
    if ti == toks.len() {
        return ci == chars.len();
    }

    match &toks[ti] {
        Token::Lit(l) => {
            let lit: Vec<char> = l.chars().collect();
            if ci + lit.len() > chars.len() {
                return false;
            }
            if &chars[ci..ci + lit.len()] == lit.as_slice() {
                match_tokens(toks, chars, ti + 1, ci + lit.len())
            } else {
                false
            }
        }
        Token::Slash => {
            if ci < chars.len() && chars[ci] == '/' {
                match_tokens(toks, chars, ti + 1, ci + 1)
            } else {
                false
            }
        }
        Token::Qmark => {
            if ci < chars.len() && chars[ci] != '/' {
                match_tokens(toks, chars, ti + 1, ci + 1)
            } else {
                false
            }
        }
        Token::Star => {
            // match any sequence without '/'
            let mut j = ci;
            while j <= chars.len() {
                if j > ci && (j - 1) < chars.len() && chars[j - 1] == '/' {
                    break;
                }
                if match_tokens(toks, chars, ti + 1, j) {
                    return true;
                }
                if j == chars.len() {
                    break;
                }
                if chars[j] == '/' {
                    break;
                }
                j += 1;
            }
            false
        }
        Token::DoubleStar => {
            // match any sequence including '/'
            // Special-case: allow "**/" to match zero segments (skip the slash).
            if toks.get(ti + 1) == Some(&Token::Slash) {
                if match_tokens(toks, chars, ti + 2, ci) {
                    return true;
                }
            }
            for j in ci..=chars.len() {
                if match_tokens(toks, chars, ti + 1, j) {
                    return true;
                }
            }
            false
        }
        Token::Class(cc) => {
            if ci < chars.len() && chars[ci] != '/' && cc.matches(chars[ci]) {
                match_tokens(toks, chars, ti + 1, ci + 1)
            } else {
                false
            }
        }
    }
}

fn compile_glob(raw: &str) -> Result<CompiledGlob, GlobError> {
    let mut toks = Vec::new();
    let mut lit = String::new();

    let mut it = raw.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '/' => {
                flush_lit(&mut lit, &mut toks);
                toks.push(Token::Slash);
            }
            '?' => {
                flush_lit(&mut lit, &mut toks);
                toks.push(Token::Qmark);
            }
            '*' => {
                flush_lit(&mut lit, &mut toks);
                if it.peek() == Some(&'*') {
                    it.next();
                    toks.push(Token::DoubleStar);
                } else {
                    toks.push(Token::Star);
                }
            }
            '[' => {
                flush_lit(&mut lit, &mut toks);
                let cc = parse_class(&mut it)?;
                toks.push(Token::Class(cc));
            }
            '\\' => {
                // escape next char
                if let Some(n) = it.next() {
                    lit.push(n);
                } else {
                    return Err(GlobError::InvalidPattern("dangling escape".into()));
                }
            }
            '{' | '}' | ',' => {
                // braces are expanded before compile; treat literals if present now
                lit.push(ch);
            }
            _ => lit.push(ch),
        }
    }

    flush_lit(&mut lit, &mut toks);

    Ok(CompiledGlob {
        raw: raw.to_string(),
        toks,
    })
}

fn flush_lit(lit: &mut String, toks: &mut Vec<Token>) {
    if !lit.is_empty() {
        toks.push(Token::Lit(std::mem::take(lit)));
    }
}

fn parse_class<I>(it: &mut std::iter::Peekable<I>) -> Result<CharClass, GlobError>
where
    I: Iterator<Item = char>,
{
    let mut negate = false;
    if it.peek() == Some(&'^') {
        it.next();
        negate = true;
    }

    let mut singles = Vec::new();
    let mut ranges = Vec::new();

    let mut prev: Option<char> = None;
    while let Some(ch) = it.next() {
        if ch == ']' {
            // end
            return Ok(CharClass { negate, ranges, singles });
        }
        if ch == '-' && prev.is_some() && it.peek().is_some() && it.peek() != Some(&']') {
            // range
            let a = prev.take().unwrap();
            let b = it.next().ok_or_else(|| GlobError::InvalidPattern("unterminated range".into()))?;
            ranges.push((a, b));
        } else {
            if let Some(p) = prev.take() {
                singles.push(p);
            }
            prev = Some(ch);
        }
    }

    Err(GlobError::InvalidPattern("unterminated class".into()))
}

/* ------------------------------ Braces ---------------------------------- */

fn expand_braces(raw: &str) -> Result<Vec<String>, GlobError> {
    // Simple one-level brace expansion: "a{b,c}d" -> ["abd","acd"]
    // No nesting; multiple braces supported sequentially via iterative expansion.
    let mut acc = vec![raw.to_string()];

    loop {
        let mut changed = false;
        let mut next_acc = Vec::new();

        for s in acc {
            if let Some((pre, alts, post)) = find_first_brace(&s)? {
                changed = true;
                for a in alts {
                    next_acc.push(format!("{pre}{a}{post}"));
                }
            } else {
                next_acc.push(s);
            }
        }

        acc = next_acc;
        if !changed {
            break;
        }
    }

    Ok(acc)
}

fn find_first_brace(s: &str) -> Result<Option<(String, Vec<String>, String)>, GlobError> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                return Err(GlobError::InvalidPattern("unterminated brace".into()));
            }
            let pre = s[..start].to_string();
            let body = &s[start + 1..j];
            let post = s[j + 1..].to_string();

            let alts: Vec<String> = body.split(',').map(|x| x.to_string()).collect();
            if alts.is_empty() {
                return Err(GlobError::InvalidPattern("empty brace".into()));
            }
            return Ok(Some((pre, alts, post)));
        }
        i += 1;
    }
    Ok(None)
}

/* ------------------------------ Normalize -------------------------------- */

fn normalize_path(p: &Path) -> String {
    // convert to forward slashes, drop prefixes, no leading slash
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::Prefix(_) => {}
            std::path::Component::RootDir => {}
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
        }
    }
    parts.join("/")
}

/* --------------------------------- Tests -------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star() {
        let p = GlobPattern::parse("src/*.rs").unwrap();
        assert!(p.matches_str("src/main.rs"));
        assert!(!p.matches_str("src/a/b.rs"));
    }

    #[test]
    fn glob_doublestar() {
        let p = GlobPattern::parse("src/**/*.rs").unwrap();
        assert!(p.matches_str("src/main.rs"));
        assert!(p.matches_str("src/a/b.rs"));
        assert!(!p.matches_str("tests/a.rs"));
    }

    #[test]
    fn glob_class() {
        let p = GlobPattern::parse("a/[a-c].txt").unwrap();
        assert!(p.matches_str("a/b.txt"));
        assert!(!p.matches_str("a/d.txt"));
    }

    #[test]
    fn glob_brace_expand() {
        let p = GlobPattern::parse("a/{b,c}.txt").unwrap();
        assert!(p.matches_str("a/b.txt"));
        assert!(p.matches_str("a/c.txt"));
        assert!(!p.matches_str("a/d.txt"));
    }

    #[test]
    fn globset_includes_excludes() {
        let mut gs = GlobSet::new();
        gs.add("src/**/*.rs").unwrap();
        gs.add("!src/**/gen_*.rs").unwrap();

        assert!(gs.matches(Path::new("src/main.rs")));
        assert!(!gs.matches(Path::new("src/x/gen_a.rs")));
    }
}
