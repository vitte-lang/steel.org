//! serializer.rs 
//!
//! Sérialisation / désérialisation std-only pour le schéma MCFG (fichiers `.mff` / `.muff`).
//!
//! - Format texte déterministe, orienté lignes :
//!   - `key = value`
//!   - sections `[name]`
//!   - commentaires `# ...`
//!   - values : string, int, bool, list, object `{ k="v", ... }` (subset)
//!
//! - Objectifs :
//!   - 0 dépendance (pas de serde)
//!   - stable / diff-friendly / portable
//!   - diagnostics “best-effort” via `DiagBag`
//!
//! Dépend de :
//! - crate::schema::* (RootConfig, UnitConfig, Artifact, NormalPath, etc.)
//! - crate::diag::*
//!
//! Intégration :
//! - `schema.rs` peut garder les types + validate_*.
//! - `serializer.rs` centralise parsing/printing canonique.

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{DiagBag, Diagnostic};
use crate::schema::{
    validate_root, validate_unit, Artifact, ArtifactId, ArtifactKind, HostOs, NormalPath, RootConfig, TargetTriple, UnitConfig,
    MCFG_SCHEMA_VERSION,
};

/// ------------------------------------------------------------
/// Public API
/// ------------------------------------------------------------

pub fn serialize_root(root: &RootConfig) -> String {
    CanonWriter::new().write_root(root)
}

pub fn serialize_unit(unit: &UnitConfig) -> String {
    CanonWriter::new().write_unit(unit)
}

pub fn deserialize_root(input: &str, diags: &mut DiagBag) -> Option<RootConfig> {
    let mut r = Reader::new(input);
    let doc = r.parse_document(diags)?;
    let root = r.to_root(doc, diags)?;
    let ok = validate_root(&root, diags);
    if !ok {
        // on renvoie quand même le root si parsé (best-effort), mais Some(...)
        // ici on conserve Some(root) : le caller check diags.has_error()
    }
    Some(root)
}

pub fn deserialize_unit(input: &str, diags: &mut DiagBag) -> Option<UnitConfig> {
    let mut r = Reader::new(input);
    let doc = r.parse_document(diags)?;
    let unit = r.to_unit(doc, diags)?;
    let _ = validate_unit(&unit, diags);
    Some(unit)
}

/// ------------------------------------------------------------
/// Canonical writer
/// ------------------------------------------------------------

#[derive(Default)]
struct CanonWriter {
    // placeholder for future knobs (pretty printing, sorting policies)
}

impl CanonWriter {
    fn new() -> Self {
        Self::default()
    }

    fn write_root(&self, root: &RootConfig) -> String {
        let mut out = String::new();

        push_kv_num(&mut out, "mcfg.schema", root.schema_version);
        push_kv_str(&mut out, "root.workspace", root.workspace_root.as_posix());
        if let Some(entry) = &root.entry_unit {
            push_kv_str(&mut out, "root.entry", &entry.0);
        }

        out.push('\n');
        push_section(&mut out, "units");
        for (k, v) in &root.units {
            push_kv_str_quoted_key(&mut out, &k.0, v.as_posix());
        }

        if !root.compiler_defaults.is_empty() {
            out.push('\n');
            push_section(&mut out, "compiler_defaults");
            for (k, v) in &root.compiler_defaults {
                push_kv_str_quoted_key(&mut out, k, v);
            }
        }

        if !root.build.is_empty() {
            out.push('\n');
            push_section(&mut out, "build");
            for (k, v) in &root.build {
                push_kv_str_quoted_key(&mut out, k, v);
            }
        }

        if !root.meta.is_empty() {
            out.push('\n');
            push_section(&mut out, "meta");
            for (k, v) in &root.meta {
                push_kv_str_quoted_key(&mut out, k, v);
            }
        }

        out
    }

    fn write_unit(&self, unit: &UnitConfig) -> String {
        let mut out = String::new();

        push_kv_num(&mut out, "mcfg.schema", unit.schema_version);
        push_kv_str(&mut out, "unit.id", &unit.unit.0);
        push_kv_str(&mut out, "workspace.root", unit.workspace_root.as_posix());
        push_kv_str(&mut out, "unit.dir", unit.unit_dir.as_posix());
        push_kv_str(&mut out, "host.os", unit.host.as_str());
        push_kv_str(&mut out, "target.triple", &unit.target.triple);

        out.push('\n');
        push_section(&mut out, "sources");
        push_kv_raw(&mut out, "vit", &fmt_list_str(unit.sources_vit.iter().map(|p| p.as_posix())));
        push_kv_raw(&mut out, "extra", &fmt_list_str(unit.extra_inputs.iter().map(|p| p.as_posix())));

        out.push('\n');
        push_section(&mut out, "deps");
        push_kv_raw(&mut out, "units", &fmt_list_str(unit.deps_units.iter().map(|u| u.0.as_str())));

        if !unit.outputs.is_empty() {
            out.push('\n');
            push_section(&mut out, "outputs");
            // outputs is an object subset: "id" = { kind="va", path="x", tags=[...], meta={...} }
            for a in &unit.outputs {
                let obj = fmt_artifact_obj(a);
                push_kv_obj_quoted_key(&mut out, &a.id.0, &obj);
            }
        }

        if !unit.compiler.is_empty() {
            out.push('\n');
            push_section(&mut out, "compiler");
            for (k, v) in &unit.compiler {
                push_kv_str_quoted_key(&mut out, k, v);
            }
        }

        if !unit.features.is_empty() {
            out.push('\n');
            push_section(&mut out, "features");
            push_kv_raw(&mut out, "set", &fmt_list_str(unit.features.iter().map(|s| s.as_str())));
        }

        if !unit.exports.is_empty() {
            out.push('\n');
            push_section(&mut out, "exports");
            for (k, v) in &unit.exports {
                push_kv_str_quoted_key(&mut out, k, &v.0);
            }
        }

        if !unit.meta.is_empty() {
            out.push('\n');
            push_section(&mut out, "meta");
            for (k, v) in &unit.meta {
                push_kv_str_quoted_key(&mut out, k, v);
            }
        }

        out
    }
}

fn fmt_artifact_obj(a: &Artifact) -> String {
    // subset object format; values are quoted strings and list of strings
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!("kind=\"{}\"", a.kind.as_ext()));
    parts.push(format!("path=\"{}\"", escape(a.path.as_posix())));

    if !a.tags.is_empty() {
        parts.push(format!(
            "tags={}",
            fmt_list_str(a.tags.iter().map(|s| s.as_str()))
        ));
    }

    if !a.meta.is_empty() {
        let mut kv = String::new();
        kv.push('{');
        let mut first = true;
        for (k, v) in &a.meta {
            if !first {
                kv.push_str(", ");
            }
            first = false;
            kv.push('"');
            kv.push_str(&escape(k));
            kv.push_str("\"=\"");
            kv.push_str(&escape(v));
            kv.push('"');
        }
        kv.push('}');
        parts.push(format!("meta={}", kv));
    }

    format!("{{ {} }}", parts.join(", "))
}

/// ------------------------------------------------------------
/// Reader: parse into a generic document (kv + sections)
/// ------------------------------------------------------------

#[derive(Debug, Clone)]
struct Document {
    globals: BTreeMap<String, Value>,
    sections: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Document {
    fn new() -> Self {
        Self { globals: BTreeMap::new(), sections: BTreeMap::new() }
    }

    fn section_mut(&mut self, name: &str) -> &mut BTreeMap<String, Value> {
        self.sections.entry(name.to_string()).or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    List(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl Value {
    fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
    fn as_int(&self) -> Option<i64> {
        if let Value::Int(x) = self { Some(*x) } else { None }
    }
    fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }
}

struct Reader<'a> {
    input: &'a str,
}

impl<'a> Reader<'a> {
    fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn parse_document(&mut self, diags: &mut DiagBag) -> Option<Document> {
        let mut doc = Document::new();
        let mut cur_section: Option<String> = None;

        for (lineno0, raw) in self.input.lines().enumerate() {
            let lineno = lineno0 + 1;
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let name = line[1..line.len() - 1].trim().to_string();
                if name.is_empty() {
                    diags.push(Diagnostic::warning(format!("empty section header at line {}", lineno)));
                    cur_section = None;
                } else {
                    cur_section = Some(name);
                }
                continue;
            }

            let (k_raw, v_raw) = match split_kv(line) {
                Some(x) => x,
                None => {
                    diags.push(Diagnostic::warning(format!("ignored line {}: {}", lineno, raw)));
                    continue;
                }
            };

            let key = parse_key(k_raw, diags, lineno);
            let val = match parse_value(v_raw, diags, lineno) {
                Some(v) => v,
                None => {
                    diags.push(Diagnostic::warning(format!("invalid value at line {}", lineno)));
                    continue;
                }
            };

            if let Some(sec) = &cur_section {
                let sec_map = doc.section_mut(sec);
                if sec_map.contains_key(&key) {
                    diags.push(Diagnostic::warning(format!(
                        "duplicate key `{}` in section [{}] at line {} (last-wins)",
                        key, sec, lineno
                    )));
                }
                sec_map.insert(key, val);
            } else {
                if doc.globals.contains_key(&key) {
                    diags.push(Diagnostic::warning(format!(
                        "duplicate global key `{}` at line {} (last-wins)",
                        key, lineno
                    )));
                }
                doc.globals.insert(key, val);
            }
        }

        Some(doc)
    }

    fn to_root(&mut self, doc: Document, diags: &mut DiagBag) -> Option<RootConfig> {
        let schema = doc
            .globals
            .get("mcfg.schema")
            .and_then(|v| v.as_int())
            .unwrap_or(MCFG_SCHEMA_VERSION as i64) as u32;

        let ws = match doc.globals.get("root.workspace").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                diags.push(Diagnostic::error("root.workspace missing"));
                String::new()
            }
        };

        let mut root = RootConfig::new(NormalPath { posix: ws, native: None });
        root.schema_version = schema;

        if let Some(v) = doc.globals.get("root.entry").and_then(|v| v.as_str()) {
            root.entry_unit = Some(crate::schema::UnitId(v.to_string()));
        }

        // units
        if let Some(sec) = doc.sections.get("units") {
            for (k, v) in sec {
                let path = match v.as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        diags.push(Diagnostic::warning(format!("units[{}] must be string", k)));
                        continue;
                    }
                };
                root.units.insert(crate::schema::UnitId(k.clone()), NormalPath { posix: path, native: None });
            }
        } else {
            diags.push(Diagnostic::warning("root: [units] section missing"));
        }

        // compiler_defaults/build/meta
        if let Some(sec) = doc.sections.get("compiler_defaults") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    root.compiler_defaults.insert(k.clone(), s.to_string());
                } else {
                    diags.push(Diagnostic::warning(format!("[compiler_defaults] `{}` must be string", k)));
                }
            }
        }
        if let Some(sec) = doc.sections.get("build") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    root.build.insert(k.clone(), s.to_string());
                } else {
                    diags.push(Diagnostic::warning(format!("[build] `{}` must be string", k)));
                }
            }
        }
        if let Some(sec) = doc.sections.get("meta") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    root.meta.insert(k.clone(), s.to_string());
                } else {
                    diags.push(Diagnostic::warning(format!("[meta] `{}` must be string", k)));
                }
            }
        }

        Some(root)
    }

    fn to_unit(&mut self, doc: Document, diags: &mut DiagBag) -> Option<UnitConfig> {
        let schema = doc
            .globals
            .get("mcfg.schema")
            .and_then(|v| v.as_int())
            .unwrap_or(MCFG_SCHEMA_VERSION as i64) as u32;

        let unit_id = doc
            .globals
            .get("unit.id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let ws = doc
            .globals
            .get("workspace.root")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let ud = doc
            .globals
            .get("unit.dir")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let host = doc
            .globals
            .get("host.os")
            .and_then(|v| v.as_str())
            .map(HostOs::parse)
            .unwrap_or(HostOs::Unknown);

        let target = doc
            .globals
            .get("target.triple")
            .and_then(|v| v.as_str())
            .map(|s| TargetTriple::new(s.to_string()))
            .unwrap_or_else(|| TargetTriple::new("unknown"));

        let mut unit = UnitConfig::new(
            unit_id,
            NormalPath { posix: ws, native: None },
            NormalPath { posix: ud, native: None },
        );
        unit.schema_version = schema;
        unit.host = host;
        unit.target = target;

        // [sources]
        if let Some(sec) = doc.sections.get("sources") {
            if let Some(v) = sec.get("vit") {
                unit.sources_vit = value_list_to_paths(v, diags, "sources.vit");
            }
            if let Some(v) = sec.get("extra") {
                unit.extra_inputs = value_list_to_paths(v, diags, "sources.extra");
            }
        } else {
            diags.push(Diagnostic::warning("unit: [sources] section missing"));
        }

        // [deps]
        if let Some(sec) = doc.sections.get("deps") {
            if let Some(v) = sec.get("units") {
                unit.deps_units = value_list_to_strings(v, diags, "deps.units")
                    .into_iter()
                    .map(crate::schema::UnitId)
                    .collect();
            }
        }

        // [outputs]
        if let Some(sec) = doc.sections.get("outputs") {
            for (id, v) in sec {
                match parse_artifact_value(id, v, diags) {
                    Some(a) => unit.outputs.push(a),
                    None => diags.push(Diagnostic::warning(format!("invalid output `{}`", id))),
                }
            }
        }

        // [compiler]
        if let Some(sec) = doc.sections.get("compiler") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    unit.compiler.insert(k.clone(), s.to_string());
                } else {
                    diags.push(Diagnostic::warning(format!("[compiler] `{}` must be string", k)));
                }
            }
        }

        // [features]
        if let Some(sec) = doc.sections.get("features") {
            if let Some(v) = sec.get("set") {
                for s in value_list_to_strings(v, diags, "features.set") {
                    unit.features.insert(s);
                }
            }
        }

        // [exports]
        if let Some(sec) = doc.sections.get("exports") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    unit.exports.insert(k.clone(), ArtifactId(s.to_string()));
                } else {
                    diags.push(Diagnostic::warning(format!("[exports] `{}` must be string", k)));
                }
            }
        }

        // [meta]
        if let Some(sec) = doc.sections.get("meta") {
            for (k, v) in sec {
                if let Some(s) = v.as_str() {
                    unit.meta.insert(k.clone(), s.to_string());
                } else {
                    diags.push(Diagnostic::warning(format!("[meta] `{}` must be string", k)));
                }
            }
        }

        Some(unit)
    }
}

/// ------------------------------------------------------------
/// Artifact parsing from Value
/// ------------------------------------------------------------

fn parse_artifact_value(id: &str, v: &Value, diags: &mut DiagBag) -> Option<Artifact> {
    // expected: Obj { kind="va", path="...", tags=[...], meta={...} }
    let obj = match v {
        Value::Obj(m) => m,
        Value::Str(s) => {
            // compat: allow "path" only
            let mut a = Artifact::new(id.to_string(), ArtifactKind::File, NormalPath { posix: s.clone(), native: None });
            a.tags = BTreeSet::new();
            a.meta = BTreeMap::new();
            return Some(a);
        }
        _ => {
            diags.push(Diagnostic::warning(format!("output `{}` must be object", id)));
            return None;
        }
    };

    let kind = match obj.get("kind").and_then(|x| x.as_str()) {
        Some(ext) => ArtifactKind::parse_ext(ext).unwrap_or(ArtifactKind::File),
        None => ArtifactKind::File,
    };

    let path = match obj.get("path").and_then(|x| x.as_str()) {
        Some(p) => p.to_string(),
        None => {
            diags.push(Diagnostic::warning(format!("output `{}` missing path", id)));
            String::new()
        }
    };

    let mut a = Artifact::new(id.to_string(), kind, NormalPath { posix: path, native: None });

    if let Some(tags) = obj.get("tags") {
        for s in value_list_to_strings(tags, diags, "outputs.tags") {
            a.tags.insert(s);
        }
    }

    if let Some(meta) = obj.get("meta") {
        if let Value::Obj(m) = meta {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    a.meta.insert(k.clone(), s.to_string());
                }
            }
        }
    }

    Some(a)
}

/// ------------------------------------------------------------
/// Value parsing (mini parser)
/// ------------------------------------------------------------

fn parse_key(k: &str, diags: &mut DiagBag, lineno: usize) -> String {
    let t = k.trim();
    if t.starts_with('"') {
        match parse_string_token(t) {
            Ok((s, _rest)) => s,
            Err(e) => {
                diags.push(Diagnostic::warning(format!("invalid quoted key at line {}: {}", lineno, e)));
                t.to_string()
            }
        }
    } else {
        t.to_string()
    }
}

fn parse_value(s: &str, diags: &mut DiagBag, lineno: usize) -> Option<Value> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }

    // string
    if t.starts_with('"') {
        match parse_string_token(t) {
            Ok((s, rest)) => {
                let rest = rest.trim();
                if !rest.is_empty() {
                    diags.push(Diagnostic::warning(format!("trailing content after string at line {}", lineno)));
                }
                return Some(Value::Str(s));
            }
            Err(e) => {
                diags.push(Diagnostic::error(format!("string parse error at line {}: {}", lineno, e)));
                return None;
            }
        }
    }

    // bool
    if t == "true" {
        return Some(Value::Bool(true));
    }
    if t == "false" {
        return Some(Value::Bool(false));
    }

    // int
    if looks_like_int(t) {
        if let Ok(x) = t.parse::<i64>() {
            return Some(Value::Int(x));
        }
    }

    // list
    if t.starts_with('[') {
        return parse_list(t, diags, lineno);
    }

    // object
    if t.starts_with('{') {
        return parse_obj(t, diags, lineno);
    }

    // fallback: bare ident as string
    Some(Value::Str(t.to_string()))
}

fn parse_list(t: &str, diags: &mut DiagBag, lineno: usize) -> Option<Value> {
    let mut p = Cursor::new(t);
    p.consume_ws();

    if !p.consume_char('[') {
        return None;
    }
    p.consume_ws();

    let mut items: Vec<Value> = Vec::new();

    if p.peek_char() == Some(']') {
        p.consume_char(']');
        return Some(Value::List(items));
    }

    loop {
        p.consume_ws();
        let v = parse_value_cursor(&mut p, diags, lineno)?;
        items.push(v);
        p.consume_ws();

        if p.consume_char(',') {
            p.consume_ws();
            continue;
        }
        if p.consume_char(']') {
            break;
        }
        diags.push(Diagnostic::error(format!("list parse error at line {}: expected `,` or `]`", lineno)));
        return None;
    }

    Some(Value::List(items))
}

fn parse_obj(t: &str, diags: &mut DiagBag, lineno: usize) -> Option<Value> {
    let mut p = Cursor::new(t);
    p.consume_ws();

    if !p.consume_char('{') {
        return None;
    }
    p.consume_ws();

    let mut map: BTreeMap<String, Value> = BTreeMap::new();

    if p.peek_char() == Some('}') {
        p.consume_char('}');
        return Some(Value::Obj(map));
    }

    loop {
        p.consume_ws();

        let key = if p.peek_char() == Some('"') {
            let s = p.read_string(diags, lineno)?;
            s
        } else {
            p.read_ident()
                .ok_or_else(|| {
                    diags.push(Diagnostic::error(format!("object parse error at line {}: expected key", lineno)));
                })
                .ok()?
        };

        p.consume_ws();
        if !p.consume_char('=') {
            diags.push(Diagnostic::error(format!("object parse error at line {}: expected `=`", lineno)));
            return None;
        }
        p.consume_ws();

        let val = parse_value_cursor(&mut p, diags, lineno)?;
        if map.contains_key(&key) {
            diags.push(Diagnostic::warning(format!(
                "duplicate object key `{}` at line {} (last-wins)",
                key, lineno
            )));
        }
        map.insert(key, val);

        p.consume_ws();
        if p.consume_char(',') {
            p.consume_ws();
            continue;
        }
        if p.consume_char('}') {
            break;
        }

        diags.push(Diagnostic::error(format!("object parse error at line {}: expected `,` or `}`", lineno)));
        return None;
    }

    Some(Value::Obj(map))
}

fn parse_value_cursor(p: &mut Cursor<'_>, diags: &mut DiagBag, lineno: usize) -> Option<Value> {
    p.consume_ws();
    let rest = p.rest();

    // Delegate on prefix using cursor itself
    if p.peek_char() == Some('"') {
        let s = p.read_string(diags, lineno)?;
        return Some(Value::Str(s));
    }
    if p.peek_char() == Some('[') {
        // parse list from cursor slice
        let slice = rest;
        let v = parse_list(slice, diags, lineno)?;
        // advance cursor by consumed len: we re-parse; easiest is to consume manually:
        // here we do a controlled “scan to matching bracket” to keep in sync.
        let consumed = scan_balanced(slice, '[', ']')?;
        p.advance(consumed);
        return Some(v);
    }
    if p.peek_char() == Some('{') {
        let slice = rest;
        let v = parse_obj(slice, diags, lineno)?;
        let consumed = scan_balanced(slice, '{', '}')?;
        p.advance(consumed);
        return Some(v);
    }

    // bool/int/ident
    if let Some(tok) = p.read_token_simple() {
        match tok.as_str() {
            "true" => return Some(Value::Bool(true)),
            "false" => return Some(Value::Bool(false)),
            _ => {}
        }
        if looks_like_int(&tok) {
            if let Ok(x) = tok.parse::<i64>() {
                return Some(Value::Int(x));
            }
        }
        return Some(Value::Str(tok));
    }

    None
}

/// scan balanced bracket expression length starting at 0.
/// returns number of bytes consumed until including matching close.
/// Note: expects slice starts with open.
fn scan_balanced(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut in_str = false;
    let mut esc = false;

    for (idx, ch) in s.char_indices() {
        i = idx;

        if in_str {
            if esc {
                esc = false;
                continue;
            }
            if ch == '\\' {
                esc = true;
                continue;
            }
            if ch == '"' {
                in_str = false;
            }
            continue;
        }

        if ch == '"' {
            in_str = true;
            continue;
        }

        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                // include this char
                return Some(idx + close.len_utf8());
            }
        }
    }
    None
}

fn value_list_to_strings(v: &Value, diags: &mut DiagBag, ctx: &str) -> Vec<String> {
    match v {
        Value::List(items) => items
            .iter()
            .filter_map(|x| match x {
                Value::Str(s) => Some(s.clone()),
                Value::Int(n) => Some(n.to_string()),
                Value::Bool(b) => Some(if *b { "true".into() } else { "false".into() }),
                _ => {
                    diags.push(Diagnostic::warning(format!("{}: list element not scalar", ctx)));
                    None
                }
            })
            .collect(),
        Value::Str(s) => vec![s.clone()],
        _ => {
            diags.push(Diagnostic::warning(format!("{}: expected list", ctx)));
            Vec::new()
        }
    }
}

fn value_list_to_paths(v: &Value, diags: &mut DiagBag, ctx: &str) -> Vec<NormalPath> {
    value_list_to_strings(v, diags, ctx)
        .into_iter()
        .map(|p| NormalPath { posix: p, native: None })
        .collect()
}

/// ------------------------------------------------------------
/// Cursor + string parsing
/// ------------------------------------------------------------

struct Cursor<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, i: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn advance(&mut self, n: usize) {
        self.i = (self.i + n).min(self.s.len());
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn consume_char(&mut self, ch: char) -> bool {
        if self.peek_char() == Some(ch) {
            self.advance(ch.len_utf8());
            true
        } else {
            false
        }
    }

    fn consume_ws(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance(c.len_utf8());
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self, diags: &mut DiagBag, lineno: usize) -> Option<String> {
        let r = self.rest();
        match parse_string_token(r) {
            Ok((s, rest)) => {
                let consumed = r.len() - rest.len();
                self.advance(consumed);
                Some(s)
            }
            Err(e) => {
                diags.push(Diagnostic::error(format!("string parse error at line {}: {}", lineno, e)));
                None
            }
        }
    }

    fn read_ident(&mut self) -> Option<String> {
        let mut out = String::new();
        let mut it = self.rest().char_indices();

        let (mut last_i, first) = it.next()?;
        if !(first.is_ascii_alphabetic() || first == '_' ) {
            return None;
        }
        out.push(first);
        last_i += first.len_utf8();

        for (idx, ch) in it {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                out.push(ch);
                last_i = idx + ch.len_utf8();
            } else {
                break;
            }
        }

        self.advance(last_i);
        Some(out)
    }

    fn read_token_simple(&mut self) -> Option<String> {
        // read until whitespace or delimiter , ] }.
        let mut out = String::new();
        let mut last = 0usize;

        for (idx, ch) in self.rest().char_indices() {
            if ch.is_whitespace() || ch == ',' || ch == ']' || ch == '}' {
                break;
            }
            out.push(ch);
            last = idx + ch.len_utf8();
        }

        if out.is_empty() {
            None
        } else {
            self.advance(last);
            Some(out)
        }
    }
}

fn parse_string_token(s: &str) -> Result<(String, &str), &'static str> {
    let mut it = s.chars();
    if it.next() != Some('"') {
        return Err("expected '\"'");
    }

    let mut out = String::new();
    let mut esc = false;
    let mut consumed = 1usize;

    for ch in it {
        consumed += ch.len_utf8();
        if esc {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                x => out.push(x),
            }
            esc = false;
            continue;
        }

        if ch == '\\' {
            esc = true;
            continue;
        }

        if ch == '"' {
            let rest = &s[consumed..];
            return Ok((out, rest));
        }

        out.push(ch);
    }

    Err("unterminated string")
}

fn strip_comment(line: &str) -> &str {
    // remove everything after first # not in string (cheap but safe enough)
    let mut in_str = false;
    let mut esc = false;

    for (idx, ch) in line.char_indices() {
        if in_str {
            if esc {
                esc = false;
                continue;
            }
            if ch == '\\' {
                esc = true;
                continue;
            }
            if ch == '"' {
                in_str = false;
            }
            continue;
        } else {
            if ch == '"' {
                in_str = true;
                continue;
            }
            if ch == '#' {
                return &line[..idx];
            }
        }
    }

    line
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut it = line.splitn(2, '=');
    let k = it.next()?.trim();
    let v = it.next()?.trim();
    if k.is_empty() || v.is_empty() {
        return None;
    }
    Some((k, v))
}

fn looks_like_int(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = 0usize;
    if b[0] == b'-' {
        if b.len() == 1 {
            return false;
        }
        i = 1;
    }
    while i < b.len() {
        if !(b[i] as char).is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    true
}

/// ------------------------------------------------------------
/// Writer helpers
/// ------------------------------------------------------------

fn push_section(out: &mut String, name: &str) {
    out.push('[');
    out.push_str(name);
    out.push_str("]\n");
}

fn push_kv_raw(out: &mut String, k: &str, raw_value: &str) {
    out.push_str(k);
    out.push_str(" = ");
    out.push_str(raw_value);
    out.push('\n');
}

fn push_kv_num(out: &mut String, k: &str, v: u32) {
    out.push_str(k);
    out.push_str(" = ");
    out.push_str(&v.to_string());
    out.push('\n');
}

fn push_kv_str(out: &mut String, k: &str, v: &str) {
    out.push_str(k);
    out.push_str(" = \"");
    out.push_str(&escape(v));
    out.push_str("\"\n");
}

fn push_kv_str_quoted_key(out: &mut String, k: &str, v: &str) {
    out.push('"');
    out.push_str(&escape(k));
    out.push_str("\" = \"");
    out.push_str(&escape(v));
    out.push_str("\"\n");
}

fn push_kv_obj_quoted_key(out: &mut String, k: &str, obj: &str) {
    out.push('"');
    out.push_str(&escape(k));
    out.push_str("\" = ");
    out.push_str(obj);
    out.push('\n');
}

fn escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            _ => o.push(ch),
        }
    }
    o
}

fn fmt_list_str<'a, I>(iter: I) -> String
where
    I: Iterator<Item = &'a str>,
{
    let mut out = String::from("[");
    let mut first = true;
    for s in iter {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push('"');
        out.push_str(&escape(s));
        out.push('"');
    }
    out.push(']');
    out
}

/// ------------------------------------------------------------
/// Tests
/// ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{RootConfig, UnitConfig, UnitId};

    #[test]
    fn roundtrip_root_smoke() {
        let mut root = RootConfig::new(NormalPath { posix: ".".into(), native: None });
        root.schema_version = MCFG_SCHEMA_VERSION;
        root.entry_unit = Some(UnitId("src/in/a".into()));
        root.units.insert(UnitId("src/in/a".into()), NormalPath { posix: "steel/a.muff".into(), native: None });
        root.compiler_defaults.insert("opt".into(), "3".into());
        root.build.insert("jobs".into(), "16".into());
        root.meta.insert("gen".into(), "steel".into());

        let text = serialize_root(&root);

        let mut diags = DiagBag::new();
        let parsed = deserialize_root(&text, &mut diags).unwrap();
        assert!(!diags.has_error());
        assert_eq!(parsed.units.len(), 1);
        assert_eq!(parsed.entry_unit.unwrap().0, "src/in/a");
    }

    #[test]
    fn roundtrip_unit_with_outputs() {
        let mut u = UnitConfig::new(
            "src/in/a",
            NormalPath { posix: ".".into(), native: None },
            NormalPath { posix: "src/in/a".into(), native: None },
        );
        u.schema_version = MCFG_SCHEMA_VERSION;
        u.host = HostOs::Linux;
        u.target = TargetTriple::new("x86_64-unknown-linux-gnu");
        u.sources_vit.push(NormalPath { posix: "src/program/lib.vit".into(), native: None });
        u.extra_inputs.push(NormalPath { posix: "assets/logo.png".into(), native: None });
        u.deps_units.push(UnitId("src/in/b".into()));
        u.features.insert("debug".into());
        u.compiler.insert("opt".into(), "0".into());
        u.exports.insert("default".into(), ArtifactId("exe".into()));

        let mut art = Artifact::new("exe".to_string(), ArtifactKind::Exe, NormalPath { posix: "src/out/bin/a.exe".into(), native: None });
        art.tags.insert("profile:debug".into());
        art.meta.insert("hash".into(), "deadbeef".into());
        u.outputs.push(art);

        let text = serialize_unit(&u);

        let mut diags = DiagBag::new();
        let parsed = deserialize_unit(&text, &mut diags).unwrap();
        assert!(!diags.has_error());
        assert_eq!(parsed.outputs.len(), 1);
        assert_eq!(parsed.outputs[0].id.0, "exe");
    }

    #[test]
    fn parser_supports_inline_objects_and_lists() {
        let src = r#"
mcfg.schema = 1
unit.id = "x"
workspace.root = "."
unit.dir = "x"
host.os = "linux"
target.triple = "x86_64-unknown-linux-gnu"

[outputs]
"lib" = { kind="va", path="src/out/lib/x.va", tags=["a","b"], meta={"k"="v"} }
"#;

        let mut diags = DiagBag::new();
        let unit = deserialize_unit(src, &mut diags).unwrap();
        assert!(!diags.has_error());
        assert_eq!(unit.outputs.len(), 1);
        assert_eq!(unit.outputs[0].kind, ArtifactKind::Va);
        assert!(unit.outputs[0].tags.contains("a"));
        assert_eq!(unit.outputs[0].meta.get("k").cloned().unwrap_or_default(), "v");
    }
}