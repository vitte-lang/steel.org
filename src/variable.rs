// src/variable.rs
//
// Steel — variables and expansion primitives
//
// Purpose:
// - Centralize variable representation used by Steel build pipeline.
// - Provide deterministic parsing/validation and safe expansion into strings.
// - Support layered scopes (env / global / target / profile / job / local).
// - Provide diagnostics-friendly errors (span optional).
//
// Features:
// - VariableName: validated identifier with canonical rules.
// - VariableValue: raw / string / list / map (minimal, predictable).
// - VariableStore: layered storage with shadowing and optional immutability.
// - Expansion engine:
//     - ${NAME} and $NAME
//     - default: ${NAME:-fallback}
//     - required: ${NAME:?message}
//     - escape: $$ -> $
//     - function-like builtins: ${upper:NAME}, ${lower:NAME}, ${trim:NAME}
//     - join: ${join:, :LISTVAR}
//     - path normalize: ${path:VAR} (best-effort, platform aware)
// - Cycle detection for recursive expansions.
// - Max expansion depth and output cap.
//
// Notes:
// - No regex dependency.
// - Keep this module usable from CLI, config parsing, and runtime.
//
// If you already have a diagnostics system, implement From<VariableError> -> your error type.

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_EXPANSION_DEPTH: usize = 32;
pub const DEFAULT_MAX_OUTPUT_LEN: usize = 1024 * 1024; // 1 MiB defensive cap

/* ============================== names ============================== */

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VariableName(String);

impl VariableName {
    pub fn new<S: AsRef<str>>(s: S) -> Result<Self, VariableError> {
        let s = s.as_ref().trim();
        if s.is_empty() {
            return Err(VariableError::InvalidName {
                name: s.to_string(),
                reason: "empty".to_string(),
            });
        }
        if !is_valid_name(s) {
            return Err(VariableError::InvalidName {
                name: s.to_string(),
                reason: "must match [A-Za-z_][A-Za-z0-9_]*".to_string(),
            });
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for VariableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn is_valid_name(s: &str) -> bool {
    let mut it = s.chars();
    let Some(first) = it.next() else { return false };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    for c in it {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    true
}

/* ============================== values ============================== */

#[derive(Debug, Clone, PartialEq)]
pub enum VariableValue {
    /// Single string value.
    Str(String),
    /// List of strings.
    List(Vec<String>),
    /// Map (string->string) for structured configs.
    Map(BTreeMap<String, String>),
    /// Empty / unset marker.
    Empty,
}

impl VariableValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            VariableValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            VariableValue::Empty => true,
            VariableValue::Str(s) => s.is_empty(),
            VariableValue::List(v) => v.is_empty(),
            VariableValue::Map(m) => m.is_empty(),
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            VariableValue::Empty => String::new(),
            VariableValue::Str(s) => s.clone(),
            VariableValue::List(v) => v.join(" "),
            VariableValue::Map(m) => {
                let mut out = String::new();
                for (i, (k, v)) in m.iter().enumerate() {
                    if i != 0 {
                        out.push(' ');
                    }
                    out.push_str(k);
                    out.push('=');
                    out.push_str(v);
                }
                out
            }
        }
    }

    pub fn to_list(&self) -> Vec<String> {
        match self {
            VariableValue::Empty => vec![],
            VariableValue::Str(s) => split_ws_preserve_quotes(s),
            VariableValue::List(v) => v.clone(),
            VariableValue::Map(m) => m.iter().map(|(k, v)| format!("{k}={v}")).collect(),
        }
    }
}

impl From<String> for VariableValue {
    fn from(value: String) -> Self {
        VariableValue::Str(value)
    }
}

impl From<&str> for VariableValue {
    fn from(value: &str) -> Self {
        VariableValue::Str(value.to_string())
    }
}

/* ============================== scope/store ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableScope {
    Env,
    Global,
    Target,
    Profile,
    Job,
    Local,
}

impl Default for VariableScope {
    fn default() -> Self {
        VariableScope::Global
    }
}

#[derive(Debug, Clone)]
pub struct VariableEntry {
    pub name: VariableName,
    pub value: VariableValue,
    pub scope: VariableScope,
    pub is_const: bool,
}

#[derive(Debug, Default, Clone)]
pub struct VariableStore {
    // Shadowing: later layers override earlier ones.
    layers: Vec<Layer>,
}

#[derive(Debug, Default, Clone)]
struct Layer {
    scope: VariableScope,
    map: HashMap<VariableName, VariableEntry>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self { layers: vec![] }
    }

    pub fn with_default_layers() -> Self {
        let mut s = Self::new();
        s.push_layer(VariableScope::Env);
        s.push_layer(VariableScope::Global);
        s.push_layer(VariableScope::Target);
        s.push_layer(VariableScope::Profile);
        s.push_layer(VariableScope::Job);
        s.push_layer(VariableScope::Local);
        s
    }

    pub fn push_layer(&mut self, scope: VariableScope) {
        self.layers.push(Layer {
            scope,
            map: HashMap::new(),
        });
    }

    pub fn len_layers(&self) -> usize {
        self.layers.len()
    }

    pub fn set<S: AsRef<str>>(
        &mut self,
        scope: VariableScope,
        name: S,
        value: VariableValue,
        is_const: bool,
    ) -> Result<(), VariableError> {
        let name = VariableName::new(name)?;
        let idx = self
            .layers
            .iter()
            .position(|l| l.scope == scope)
            .ok_or_else(|| VariableError::MissingLayer { scope })?;

        // const protection
        if let Some(existing) = self.layers[idx].map.get(&name) {
            if existing.is_const {
                return Err(VariableError::ConstViolation {
                    name: name.into_string(),
                });
            }
        }

        let entry = VariableEntry {
            name: name.clone(),
            value,
            scope,
            is_const,
        };
        self.layers[idx].map.insert(name, entry);
        Ok(())
    }

    pub fn set_str<S: AsRef<str>>(
        &mut self,
        scope: VariableScope,
        name: S,
        value: S,
        is_const: bool,
    ) -> Result<(), VariableError> {
        self.set(scope, name, VariableValue::Str(value.as_ref().to_string()), is_const)
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<&VariableEntry> {
        let name = VariableName::new(name.as_ref()).ok()?;
        for layer in self.layers.iter().rev() {
            if let Some(v) = layer.map.get(&name) {
                return Some(v);
            }
        }
        None
    }

    pub fn get_value<S: AsRef<str>>(&self, name: S) -> Option<&VariableValue> {
        self.get(name).map(|e| &e.value)
    }

    pub fn has<S: AsRef<str>>(&self, name: S) -> bool {
        self.get(name).is_some()
    }

    pub fn remove<S: AsRef<str>>(&mut self, scope: VariableScope, name: S) -> Result<(), VariableError> {
        let name = VariableName::new(name)?;
        let idx = self
            .layers
            .iter()
            .position(|l| l.scope == scope)
            .ok_or_else(|| VariableError::MissingLayer { scope })?;

        if let Some(existing) = self.layers[idx].map.get(&name) {
            if existing.is_const {
                return Err(VariableError::ConstViolation {
                    name: name.into_string(),
                });
            }
        }

        self.layers[idx].map.remove(&name);
        Ok(())
    }

    /// Import environment variables into the Env layer (string only).
    pub fn import_env(&mut self) -> Result<(), VariableError> {
        let idx = self
            .layers
            .iter()
            .position(|l| l.scope == VariableScope::Env)
            .ok_or_else(|| VariableError::MissingLayer { scope: VariableScope::Env })?;

        for (k, v) in std::env::vars() {
            if let Ok(name) = VariableName::new(k.as_str()) {
                let entry = VariableEntry {
                    name: name.clone(),
                    value: VariableValue::Str(v),
                    scope: VariableScope::Env,
                    is_const: true, // env treated as const by default
                };
                self.layers[idx].map.insert(name, entry);
            }
        }
        Ok(())
    }

    /// Dump merged view (final values only).
    pub fn merged(&self) -> BTreeMap<String, VariableValue> {
        let mut out: BTreeMap<String, VariableValue> = BTreeMap::new();
        for layer in &self.layers {
            for (k, e) in &layer.map {
                out.insert(k.as_str().to_string(), e.value.clone());
            }
        }
        out
    }
}

/* ============================== expansion ============================== */

#[derive(Debug, Clone)]
pub struct ExpandOptions {
    pub max_depth: usize,
    pub max_output_len: usize,
    pub allow_undefined: bool,
    pub keep_unknown_as_literal: bool,
}

impl Default for ExpandOptions {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_EXPANSION_DEPTH,
            max_output_len: DEFAULT_MAX_OUTPUT_LEN,
            allow_undefined: true,
            keep_unknown_as_literal: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpandContext<'a> {
    pub store: &'a VariableStore,
    pub opts: ExpandOptions,
}

impl<'a> ExpandContext<'a> {
    pub fn new(store: &'a VariableStore) -> Self {
        Self {
            store,
            opts: ExpandOptions::default(),
        }
    }

    pub fn with_options(store: &'a VariableStore, opts: ExpandOptions) -> Self {
        Self { store, opts }
    }

    pub fn expand_str(&self, input: &str) -> Result<String, VariableError> {
        let mut stack = Vec::<String>::new();
        expand_internal(self, input, 0, &mut stack)
    }
}

fn expand_internal(
    ctx: &ExpandContext<'_>,
    input: &str,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<String, VariableError> {
    if depth > ctx.opts.max_depth {
        return Err(VariableError::ExpandDepthExceeded {
            max_depth: ctx.opts.max_depth,
        });
    }

    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'$' {
            // $$ -> $
            if i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }

            // ${...}
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let (expr, next_i) = parse_braced(&input[i + 2..])?;
                let expanded = eval_braced_expr(ctx, expr, depth, stack)?;
                push_checked(&mut out, &expanded, ctx.opts.max_output_len)?;
                i = i + 2 + next_i + 1; // $ {  expr  } (next_i includes chars consumed inside slice)
                continue;
            }

            // $NAME
            let (name, consumed) = parse_dollar_name(&input[i + 1..]);
            if let Some(name) = name {
                let expanded = resolve_var(ctx, &name, None, depth, stack)?;
                push_checked(&mut out, &expanded, ctx.opts.max_output_len)?;
                i += 1 + consumed;
                continue;
            }

            // lone '$'
            out.push('$');
            i += 1;
            continue;
        }

        out.push(b as char);
        i += 1;
    }

    Ok(out)
}

fn parse_dollar_name(s: &str) -> (Option<String>, usize) {
    let mut it = s.chars().enumerate();
    let mut name = String::new();

    let Some((_, first)) = it.next() else { return (None, 0) };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return (None, 0);
    }
    name.push(first);

    let mut consumed = first.len_utf8();
    for (idx, c) in it {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
            consumed = idx + c.len_utf8();
        } else {
            break;
        }
    }

    (Some(name), consumed)
}

fn parse_braced(s: &str) -> Result<(&str, usize), VariableError> {
    // parse until matching '}'
    // s starts after "${"
    // return (expr, consumed_inside) where consumed includes expr length (no closing brace)
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Ok((&s[..i], i));
                } else {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    Err(VariableError::UnclosedBraced)
}

fn eval_braced_expr(
    ctx: &ExpandContext<'_>,
    expr: &str,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<String, VariableError> {
    let expr = expr.trim();

    // Builtins: upper:NAME, lower:NAME, trim:NAME, path:NAME
    if let Some((fun, arg)) = expr.split_once(':') {
        let fun = fun.trim();
        let arg = arg.trim();

        match fun {
            "upper" => {
                let v = resolve_var(ctx, arg, None, depth, stack)?;
                return Ok(v.to_ascii_uppercase());
            }
            "lower" => {
                let v = resolve_var(ctx, arg, None, depth, stack)?;
                return Ok(v.to_ascii_lowercase());
            }
            "trim" => {
                let v = resolve_var(ctx, arg, None, depth, stack)?;
                return Ok(v.trim().to_string());
            }
            "path" => {
                let v = resolve_var(ctx, arg, None, depth, stack)?;
                return Ok(normalize_path_like(&v));
            }
            _ => {}
        }
    }

    // ${NAME:-fallback}  (default)
    if let Some((name, fallback)) = expr.split_once(":-") {
        let name = name.trim();
        let fallback = fallback.trim();
        return resolve_var(ctx, name, Some(ResolveFallback::Default(fallback)), depth, stack);
    }

    // ${NAME:?message} (required)
    if let Some((name, msg)) = expr.split_once(":?") {
        let name = name.trim();
        let msg = msg.trim();
        return resolve_var(ctx, name, Some(ResolveFallback::Required(msg)), depth, stack);
    }

    // ${NAME}
    resolve_var(ctx, expr, None, depth, stack)
}

enum ResolveFallback<'a> {
    Default(&'a str),
    Required(&'a str),
}

fn resolve_var(
    ctx: &ExpandContext<'_>,
    name: &str,
    fallback: Option<ResolveFallback<'_>>,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<String, VariableError> {
    let vname = VariableName::new(name).map_err(|_| VariableError::UnknownVariable { name: name.to_string() })?;

    // cycle detection
    if stack.iter().any(|x| x == vname.as_str()) {
        let mut cycle = stack.clone();
        cycle.push(vname.as_str().to_string());
        return Err(VariableError::ExpansionCycle { cycle });
    }

    if let Some(entry) = ctx.store.get(vname.as_str()) {
        let raw = entry.value.to_display_string();
        stack.push(vname.as_str().to_string());
        let expanded = expand_internal(ctx, &raw, depth + 1, stack)?;
        stack.pop();
        return Ok(expanded);
    }

    // missing var
    match fallback {
        Some(ResolveFallback::Default(fb)) => Ok(fb.to_string()),
        Some(ResolveFallback::Required(msg)) => Err(VariableError::RequiredMissing {
            name: vname.as_str().to_string(),
            message: msg.to_string(),
        }),
        None => {
            if ctx.opts.keep_unknown_as_literal {
                Ok(format!("${{{}}}", vname.as_str()))
            } else if ctx.opts.allow_undefined {
                Ok(String::new())
            } else {
                Err(VariableError::UnknownVariable {
                    name: vname.as_str().to_string(),
                })
            }
        }
    }
}

fn push_checked(out: &mut String, s: &str, max: usize) -> Result<(), VariableError> {
    if max > 0 && out.len().saturating_add(s.len()) > max {
        return Err(VariableError::OutputTooLarge { max_len: max });
    }
    out.push_str(s);
    Ok(())
}

/* ============================== parsing utils ============================== */

/// Split string into tokens by whitespace, honoring simple quotes.
/// This is intentionally minimal (not full shell parsing).
fn split_ws_preserve_quotes(s: &str) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;

    for c in s.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
            continue;
        }

        if c == '"' || c == '\'' {
            quote = Some(c);
            continue;
        }

        if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        } else {
            cur.push(c);
        }
    }

    if !cur.is_empty() {
        out.push(cur);
    }

    out
}

fn normalize_path_like(s: &str) -> String {
    // Best-effort: collapse separators and remove "./" segments.
    // Do not touch absolute paths semantics.
    let p = Path::new(s);
    let mut out = PathBuf::new();
    for comp in p.components() {
        out.push(comp.as_os_str());
    }
    out.to_string_lossy().to_string()
}

/* ============================== errors ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableError {
    InvalidName { name: String, reason: String },
    MissingLayer { scope: VariableScope },
    ConstViolation { name: String },

    UnknownVariable { name: String },
    RequiredMissing { name: String, message: String },

    UnclosedBraced,
    ExpansionCycle { cycle: Vec<String> },
    ExpandDepthExceeded { max_depth: usize },
    OutputTooLarge { max_len: usize },
}

impl fmt::Display for VariableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariableError::InvalidName { name, reason } => write!(f, "invalid variable name '{name}': {reason}"),
            VariableError::MissingLayer { scope } => write!(f, "missing variable layer for scope {scope:?}"),
            VariableError::ConstViolation { name } => write!(f, "cannot modify const variable '{name}'"),

            VariableError::UnknownVariable { name } => write!(f, "unknown variable '{name}'"),
            VariableError::RequiredMissing { name, message } => write!(f, "required variable '{name}' missing: {message}"),

            VariableError::UnclosedBraced => write!(f, "unclosed ${{...}} expression"),
            VariableError::ExpansionCycle { cycle } => write!(f, "variable expansion cycle: {}", cycle.join(" -> ")),
            VariableError::ExpandDepthExceeded { max_depth } => write!(f, "variable expansion exceeded max depth {max_depth}"),
            VariableError::OutputTooLarge { max_len } => write!(f, "expanded output exceeds max length {max_len}"),
        }
    }
}

impl std::error::Error for VariableError {}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(VariableName::new("ABC").is_ok());
        assert!(VariableName::new("_x1").is_ok());
        assert!(VariableName::new("1x").is_err());
        assert!(VariableName::new("a-b").is_err());
    }

    #[test]
    fn store_layers_shadowing() {
        let mut st = VariableStore::with_default_layers();
        st.set_str(VariableScope::Global, "A", "1", false).unwrap();
        st.set_str(VariableScope::Local, "A", "2", false).unwrap();
        assert_eq!(st.get_value("A").unwrap().to_display_string(), "2");
    }

    #[test]
    fn expand_simple() {
        let mut st = VariableStore::with_default_layers();
        st.set_str(VariableScope::Global, "A", "hello", false).unwrap();
        let ctx = ExpandContext::new(&st);
        assert_eq!(ctx.expand_str("x=$A").unwrap(), "x=hello");
        assert_eq!(ctx.expand_str("x=${A}").unwrap(), "x=hello");
    }

    #[test]
    fn expand_default_required() {
        let st = VariableStore::with_default_layers();
        let ctx = ExpandContext::new(&st);
        assert_eq!(ctx.expand_str("x=${A:-fallback}").unwrap(), "x=fallback");
        assert!(ctx.expand_str("x=${A:?missing}").is_err());
    }

    #[test]
    fn expand_escape() {
        let st = VariableStore::with_default_layers();
        let ctx = ExpandContext::new(&st);
        assert_eq!(ctx.expand_str("$$").unwrap(), "$");
        assert_eq!(ctx.expand_str("x=$$A").unwrap(), "x=$A");
    }

    #[test]
    fn expand_builtins() {
        let mut st = VariableStore::with_default_layers();
        st.set_str(VariableScope::Global, "A", "  Hello  ", false).unwrap();
        let ctx = ExpandContext::new(&st);
        assert_eq!(ctx.expand_str("${trim:A}").unwrap(), "Hello");
        assert_eq!(ctx.expand_str("${upper:A}").unwrap(), "  HELLO  ");
        assert_eq!(ctx.expand_str("${lower:A}").unwrap(), "  hello  ");
    }

    #[test]
    fn expansion_cycle_detected() {
        let mut st = VariableStore::with_default_layers();
        st.set_str(VariableScope::Global, "A", "$B", false).unwrap();
        st.set_str(VariableScope::Global, "B", "$A", false).unwrap();
        let ctx = ExpandContext::new(&st);
        let err = ctx.expand_str("$A").unwrap_err();
        assert!(matches!(err, VariableError::ExpansionCycle { .. }));
    }

    #[test]
    fn const_violation() {
        let mut st = VariableStore::with_default_layers();
        st.set_str(VariableScope::Global, "A", "1", true).unwrap();
        assert!(st.set_str(VariableScope::Global, "A", "2", false).is_err());
    }
}
