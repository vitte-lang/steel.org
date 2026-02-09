//! JSON model + std-only encoding for Steel build graphs (MAX).
//!
//! This module provides a serializable JSON representation for `BakeGraph`
//! (nodes, artifacts, deps) and a std-only JSON encoder/decoder.
//!
//! Goals:
//! - deterministic output ordering (BTreeMap/BTreeSet iteration)
//! - no serde dependency
//! - stable schema versioning
//! - both "pretty" and "compact" output
//!
//! Notes:
//! - The decoder here is a minimal JSON parser for the subset we emit.
//!   If you need full JSON compliance, wire `serde_json` behind a feature.
//! - Hash/ID stability depends on bake.rs hashing choice.
//!
//! Suggested usage:
//!   let json = GraphJson::from_graph(&g).to_string_pretty();
//!   std::fs::write("bake.graph.json", json)?;
//!
//!   let parsed = GraphJson::parse(&json)?;
//!   let g2 = parsed.into_graph()?; // round-trip (best effort)

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use super::bake::{Action, Artifact, ArtifactId, ArtifactKind, BakeGraph, CacheKey, Node, NodeId};

pub const GRAPH_JSON_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug)]
pub enum JsonError {
    Msg(String),
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonError::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for JsonError {}

fn err(msg: impl Into<String>) -> JsonError {
    JsonError::Msg(msg.into())
}

/* ----------------------------- JSON Model -------------------------------- */

/// Top-level JSON representation.
#[derive(Debug, Clone)]
pub struct GraphJson {
    pub schema: String, // "steel.graph"
    pub version: String,
    pub meta: BTreeMap<String, String>,
    pub artifacts: Vec<ArtifactJson>,
    pub nodes: Vec<NodeJson>,
    pub deps: Vec<DepJson>,
}

impl GraphJson {
    pub fn new() -> Self {
        Self {
            schema: "steel.graph".into(),
            version: GRAPH_JSON_SCHEMA_VERSION.into(),
            meta: BTreeMap::new(),
            artifacts: Vec::new(),
            nodes: Vec::new(),
            deps: Vec::new(),
        }
    }

    pub fn from_graph(g: &BakeGraph) -> Self {
        let mut out = Self::new();

        // artifacts
        out.artifacts = g
            .artifacts
            .iter()
            .map(|(id, a)| ArtifactJson::from_artifact(*id, a))
            .collect();

        // nodes
        out.nodes = g
            .nodes
            .iter()
            .map(|(id, n)| NodeJson::from_node(*id, n))
            .collect();

        // deps: b depends on a => edge a -> b
        let mut edges = Vec::new();
        for (b, ds) in &g.deps {
            for a in ds {
                edges.push(DepJson {
                    from: a.0,
                    to: b.0,
                    kind: "node".into(),
                    meta: BTreeMap::new(),
                });
            }
        }
        // deterministic ordering
        edges.sort_by(|x, y| (x.from, x.to).cmp(&(y.from, y.to)));
        out.deps = edges;

        out
    }

    pub fn into_graph(self) -> Result<BakeGraph, JsonError> {
        if self.schema != "steel.graph" {
            return Err(err(format!("unsupported schema: {}", self.schema)));
        }

        let mut g = BakeGraph::new();

        // artifacts
        for a in self.artifacts {
            let art = a.into_artifact()?;
            g.artifacts.insert(art.id, art);
        }

        // nodes
        for n in self.nodes {
            let node = n.into_node()?;
            g.nodes.insert(node.id, node);
        }

        // deps
        for e in self.deps {
            let from = NodeId(e.from);
            let to = NodeId(e.to);
            g.deps.entry(to).or_default().insert(from);
            g.rdeps.entry(from).or_default().insert(to);
        }

        Ok(g)
    }

    pub fn to_string_compact(&self) -> String {
        let mut w = JsonWriter::new(false);
        self.write_json(&mut w);
        w.finish()
    }

    pub fn to_string_pretty(&self) -> String {
        let mut w = JsonWriter::new(true);
        self.write_json(&mut w);
        w.finish()
    }

    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_str("schema", &self.schema);
        w.kv_str("version", &self.version);

        w.key("meta");
        w.obj_begin();
        for (k, v) in &self.meta {
            w.kv_str(k, v);
        }
        w.obj_end();

        w.key("artifacts");
        w.arr_begin();
        for a in &self.artifacts {
            a.write_json(w);
        }
        w.arr_end();

        w.key("nodes");
        w.arr_begin();
        for n in &self.nodes {
            n.write_json(w);
        }
        w.arr_end();

        w.key("deps");
        w.arr_begin();
        for e in &self.deps {
            e.write_json(w);
        }
        w.arr_end();

        w.obj_end();
    }

    /// Minimal parser for the subset we emit.
    pub fn parse(s: &str) -> Result<Self, JsonError> {
        let mut p = JsonParser::new(s);
        let v = p.parse_value()?;
        let obj = v.as_object().ok_or_else(|| err("expected object"))?;

        let schema = obj.get("schema").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let version = obj.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();

        let mut out = GraphJson::new();
        out.schema = schema;
        out.version = version;

        if let Some(m) = obj.get("meta").and_then(|x| x.as_object()) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    out.meta.insert(k.clone(), s.to_string());
                }
            }
        }

        if let Some(arr) = obj.get("artifacts").and_then(|x| x.as_array()) {
            out.artifacts = arr.iter().map(ArtifactJson::from_value).collect::<Result<_, _>>()?;
        }

        if let Some(arr) = obj.get("nodes").and_then(|x| x.as_array()) {
            out.nodes = arr.iter().map(NodeJson::from_value).collect::<Result<_, _>>()?;
        }

        if let Some(arr) = obj.get("deps").and_then(|x| x.as_array()) {
            out.deps = arr.iter().map(DepJson::from_value).collect::<Result<_, _>>()?;
        }

        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct ArtifactJson {
    pub id: u64,
    pub kind: String,
    pub path: Option<String>,
    pub logical: Option<String>,
    pub meta: BTreeMap<String, String>,
}

impl ArtifactJson {
    pub fn from_artifact(id: ArtifactId, a: &Artifact) -> Self {
        Self {
            id: id.0,
            kind: artifact_kind_to_str(&a.kind).into(),
            path: a.path.as_ref().map(|p| p.to_string_lossy().to_string()),
            logical: a.logical.clone(),
            meta: a.meta.clone(),
        }
    }

    pub fn into_artifact(self) -> Result<Artifact, JsonError> {
        let kind = artifact_kind_from_str(&self.kind)?;
        Ok(Artifact {
            id: ArtifactId(self.id),
            kind,
            path: self.path.map(PathBuf::from),
            logical: self.logical,
            meta: self.meta,
        })
    }

    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_u64("id", self.id);
        w.kv_str("kind", &self.kind);
        w.kv_opt_str("path", self.path.as_deref());
        w.kv_opt_str("logical", self.logical.as_deref());

        w.key("meta");
        w.obj_begin();
        for (k, v) in &self.meta {
            w.kv_str(k, v);
        }
        w.obj_end();

        w.obj_end();
    }

    fn from_value(v: &JsonValue) -> Result<Self, JsonError> {
        let o = v.as_object().ok_or_else(|| err("artifact: expected object"))?;
        let id = o.get("id").and_then(|x| x.as_u64()).ok_or_else(|| err("artifact: missing id"))?;
        let kind = o.get("kind").and_then(|x| x.as_str()).ok_or_else(|| err("artifact: missing kind"))?.to_string();
        let path = o.get("path").and_then(|x| x.as_str()).map(|s| s.to_string());
        let logical = o.get("logical").and_then(|x| x.as_str()).map(|s| s.to_string());

        let mut meta = BTreeMap::new();
        if let Some(m) = o.get("meta").and_then(|x| x.as_object()) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    meta.insert(k.clone(), s.to_string());
                }
            }
        }

        Ok(Self { id, kind, path, logical, meta })
    }
}

#[derive(Debug, Clone)]
pub struct NodeJson {
    pub id: u64,
    pub name: String,
    pub inputs: Vec<u64>,
    pub outputs: Vec<u64>,
    pub action: ActionJson,
    pub cache: Option<CacheKeyJson>,
    pub meta: BTreeMap<String, String>,
}

impl NodeJson {
    pub fn from_node(id: NodeId, n: &Node) -> Self {
        Self {
            id: id.0,
            name: n.name.clone(),
            inputs: n.inputs.iter().map(|x| x.0).collect(),
            outputs: n.outputs.iter().map(|x| x.0).collect(),
            action: ActionJson::from_action(&n.action),
            cache: n.cache.as_ref().map(CacheKeyJson::from_cache),
            meta: n.meta.clone(),
        }
    }

    pub fn into_node(self) -> Result<Node, JsonError> {
        Ok(Node {
            id: NodeId(self.id),
            name: self.name,
            inputs: self.inputs.into_iter().map(ArtifactId).collect(),
            outputs: self.outputs.into_iter().map(ArtifactId).collect(),
            action: self.action.into_action(),
            cache: self.cache.map(|c| c.into_cache()),
            meta: self.meta,
        })
    }

    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_u64("id", self.id);
        w.kv_str("name", &self.name);

        w.key("inputs");
        w.arr_begin();
        for x in &self.inputs {
            w.u64(*x);
        }
        w.arr_end();

        w.key("outputs");
        w.arr_begin();
        for x in &self.outputs {
            w.u64(*x);
        }
        w.arr_end();

        w.key("action");
        self.action.write_json(w);

        w.key("cache");
        if let Some(c) = &self.cache {
            c.write_json(w);
        } else {
            w.null();
        }

        w.key("meta");
        w.obj_begin();
        for (k, v) in &self.meta {
            w.kv_str(k, v);
        }
        w.obj_end();

        w.obj_end();
    }

    fn from_value(v: &JsonValue) -> Result<Self, JsonError> {
        let o = v.as_object().ok_or_else(|| err("node: expected object"))?;
        let id = o.get("id").and_then(|x| x.as_u64()).ok_or_else(|| err("node: missing id"))?;
        let name = o.get("name").and_then(|x| x.as_str()).ok_or_else(|| err("node: missing name"))?.to_string();

        let inputs = o
            .get("inputs")
            .and_then(|x| x.as_array())
            .ok_or_else(|| err("node: missing inputs"))?
            .iter()
            .map(|x| x.as_u64().ok_or_else(|| err("node: inputs: expected number")))
            .collect::<Result<Vec<_>, _>>()?;

        let outputs = o
            .get("outputs")
            .and_then(|x| x.as_array())
            .ok_or_else(|| err("node: missing outputs"))?
            .iter()
            .map(|x| x.as_u64().ok_or_else(|| err("node: outputs: expected number")))
            .collect::<Result<Vec<_>, _>>()?;

        let action = o.get("action").ok_or_else(|| err("node: missing action"))?;
        let action = ActionJson::from_value(action)?;

        let cache = match o.get("cache") {
            Some(JsonValue::Null) | None => None,
            Some(v) => Some(CacheKeyJson::from_value(v)?),
        };

        let mut meta = BTreeMap::new();
        if let Some(m) = o.get("meta").and_then(|x| x.as_object()) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    meta.insert(k.clone(), s.to_string());
                }
            }
        }

        Ok(Self { id, name, inputs, outputs, action, cache, meta })
    }
}

#[derive(Debug, Clone)]
pub struct ActionJson {
    pub tool: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub description: Option<String>,
}

impl ActionJson {
    pub fn from_action(a: &Action) -> Self {
        Self {
            tool: a.tool.clone(),
            argv: a.argv.clone(),
            env: a.env.clone(),
            cwd: a.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            description: a.description.clone(),
        }
    }

    pub fn into_action(self) -> Action {
        Action {
            tool: self.tool,
            argv: self.argv,
            env: self.env,
            cwd: self.cwd.map(PathBuf::from),
            description: self.description,
        }
    }

    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_str("tool", &self.tool);

        w.key("argv");
        w.arr_begin();
        for a in &self.argv {
            w.str(a);
        }
        w.arr_end();

        w.key("env");
        w.obj_begin();
        for (k, v) in &self.env {
            w.kv_str(k, v);
        }
        w.obj_end();

        w.kv_opt_str("cwd", self.cwd.as_deref());
        w.kv_opt_str("description", self.description.as_deref());

        w.obj_end();
    }

    fn from_value(v: &JsonValue) -> Result<Self, JsonError> {
        let o = v.as_object().ok_or_else(|| err("action: expected object"))?;
        let tool = o.get("tool").and_then(|x| x.as_str()).ok_or_else(|| err("action: missing tool"))?.to_string();

        let argv = o
            .get("argv")
            .and_then(|x| x.as_array())
            .ok_or_else(|| err("action: missing argv"))?
            .iter()
            .map(|x| x.as_str().ok_or_else(|| err("action: argv: expected string")).map(|s| s.to_string()))
            .collect::<Result<Vec<_>, _>>()?;

        let mut env = BTreeMap::new();
        if let Some(m) = o.get("env").and_then(|x| x.as_object()) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    env.insert(k.clone(), s.to_string());
                }
            }
        }

        let cwd = o.get("cwd").and_then(|x| x.as_str()).map(|s| s.to_string());
        let description = o.get("description").and_then(|x| x.as_str()).map(|s| s.to_string());

        Ok(Self { tool, argv, env, cwd, description })
    }
}

#[derive(Debug, Clone)]
pub struct CacheKeyJson {
    pub inputs_hash: u64,
    pub config_hash: u64,
    pub salt: Option<String>,
}

impl CacheKeyJson {
    pub fn from_cache(c: &CacheKey) -> Self {
        Self {
            inputs_hash: c.inputs_hash,
            config_hash: c.config_hash,
            salt: c.salt.clone(),
        }
    }

    pub fn into_cache(self) -> CacheKey {
        CacheKey {
            inputs_hash: self.inputs_hash,
            config_hash: self.config_hash,
            salt: self.salt,
        }
    }

    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_u64("inputs_hash", self.inputs_hash);
        w.kv_u64("config_hash", self.config_hash);
        w.kv_opt_str("salt", self.salt.as_deref());
        w.obj_end();
    }

    fn from_value(v: &JsonValue) -> Result<Self, JsonError> {
        let o = v.as_object().ok_or_else(|| err("cache: expected object"))?;
        let inputs_hash = o.get("inputs_hash").and_then(|x| x.as_u64()).ok_or_else(|| err("cache: missing inputs_hash"))?;
        let config_hash = o.get("config_hash").and_then(|x| x.as_u64()).ok_or_else(|| err("cache: missing config_hash"))?;
        let salt = o.get("salt").and_then(|x| x.as_str()).map(|s| s.to_string());
        Ok(Self { inputs_hash, config_hash, salt })
    }
}

#[derive(Debug, Clone)]
pub struct DepJson {
    pub from: u64,
    pub to: u64,
    pub kind: String,
    pub meta: BTreeMap<String, String>,
}

impl DepJson {
    fn write_json(&self, w: &mut JsonWriter) {
        w.obj_begin();
        w.kv_u64("from", self.from);
        w.kv_u64("to", self.to);
        w.kv_str("kind", &self.kind);

        w.key("meta");
        w.obj_begin();
        for (k, v) in &self.meta {
            w.kv_str(k, v);
        }
        w.obj_end();

        w.obj_end();
    }

    fn from_value(v: &JsonValue) -> Result<Self, JsonError> {
        let o = v.as_object().ok_or_else(|| err("dep: expected object"))?;
        let from = o.get("from").and_then(|x| x.as_u64()).ok_or_else(|| err("dep: missing from"))?;
        let to = o.get("to").and_then(|x| x.as_u64()).ok_or_else(|| err("dep: missing to"))?;
        let kind = o.get("kind").and_then(|x| x.as_str()).unwrap_or("node").to_string();

        let mut meta = BTreeMap::new();
        if let Some(m) = o.get("meta").and_then(|x| x.as_object()) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    meta.insert(k.clone(), s.to_string());
                }
            }
        }

        Ok(Self { from, to, kind, meta })
    }
}

/* ----------------------------- JSON Writer ------------------------------- */

/// Minimal JSON writer with deterministic formatting.
#[derive(Debug)]
struct JsonWriter {
    pretty: bool,
    out: String,
    indent: usize,
    need_comma_stack: Vec<bool>,
    suppress_comma_once: bool,
}

impl JsonWriter {
    fn new(pretty: bool) -> Self {
        Self {
            pretty,
            out: String::new(),
            indent: 0,
            need_comma_stack: Vec::new(),
            suppress_comma_once: false,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn push_indent(&mut self) {
        if self.pretty {
            self.out.push('\n');
            self.out.push_str(&"  ".repeat(self.indent));
        }
    }

    fn comma_if_needed(&mut self) {
        if self.suppress_comma_once {
            self.suppress_comma_once = false;
            return;
        }
        if let Some(top) = self.need_comma_stack.last_mut() {
            if *top {
                self.out.push(',');
            } else {
                *top = true;
            }
        }
        if self.pretty {
            self.push_indent();
        }
    }

    fn obj_begin(&mut self) {
        self.comma_if_needed();
        self.out.push('{');
        self.indent += 1;
        self.need_comma_stack.push(false);
    }

    fn obj_end(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        let _ = self.need_comma_stack.pop();
        if self.pretty {
            self.push_indent();
        }
        self.out.push('}');
    }

    fn arr_begin(&mut self) {
        self.comma_if_needed();
        self.out.push('[');
        self.indent += 1;
        self.need_comma_stack.push(false);
    }

    fn arr_end(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        let _ = self.need_comma_stack.pop();
        if self.pretty {
            self.push_indent();
        }
        self.out.push(']');
    }

    fn key(&mut self, k: &str) {
        self.comma_if_needed();
        push_str(&mut self.out, k);
        self.out.push(':');
        if self.pretty {
            self.out.push(' ');
        }
        // Suppress comma insertion for the immediate value (objects/arrays).
        self.suppress_comma_once = true;
    }

    fn kv_str(&mut self, k: &str, v: &str) {
        self.key(k);
        self.str(v);
    }

    fn kv_opt_str(&mut self, k: &str, v: Option<&str>) {
        self.key(k);
        match v {
            Some(s) => self.str(s),
            None => self.null(),
        }
    }

    fn kv_u64(&mut self, k: &str, v: u64) {
        self.key(k);
        self.u64(v);
    }

    fn str(&mut self, s: &str) {
        self.comma_if_needed();
        push_str(&mut self.out, s);
    }

    fn u64(&mut self, v: u64) {
        self.comma_if_needed();
        self.out.push_str(&v.to_string());
    }

    fn null(&mut self) {
        self.comma_if_needed();
        self.out.push_str("null");
    }
}

/* ----------------------------- JSON Value -------------------------------- */

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum JsonValue {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonValue>),
    Obj(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Obj(m) => Some(m),
            _ => None,
        }
    }
    fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Arr(a) => Some(a),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }
    fn as_u64(&self) -> Option<u64> {
        match self {
            JsonValue::Num(n) if *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }
}

/* ----------------------------- JSON Parser -------------------------------- */

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            s: input.as_bytes(),
            i: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_obj(),
            Some(b'[') => self.parse_arr(),
            Some(b'"') => self.parse_str().map(JsonValue::Str),
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(JsonValue::Null)
            }
            Some(b'-') | Some(b'0'..=b'9') => self.parse_num(),
            Some(c) => Err(err(format!("unexpected byte: {}", c as char))),
            None => Err(err("unexpected EOF")),
        }
    }

    fn parse_obj(&mut self) -> Result<JsonValue, JsonError> {
        self.expect(b'{')?;
        let mut m = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.bump();
            return Ok(JsonValue::Obj(m));
        }
        loop {
            self.skip_ws();
            let k = self.parse_str()?;
            self.skip_ws();
            self.expect(b':')?;
            let v = self.parse_value()?;
            m.insert(k, v);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                    continue;
                }
                Some(b'}') => {
                    self.bump();
                    break;
                }
                _ => return Err(err("object: expected ',' or '}'")),
            }
        }
        Ok(JsonValue::Obj(m))
    }

    fn parse_arr(&mut self) -> Result<JsonValue, JsonError> {
        self.expect(b'[')?;
        let mut a = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.bump();
            return Ok(JsonValue::Arr(a));
        }
        loop {
            let v = self.parse_value()?;
            a.push(v);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                    continue;
                }
                Some(b']') => {
                    self.bump();
                    break;
                }
                _ => return Err(err("array: expected ',' or ']'")),
            }
        }
        Ok(JsonValue::Arr(a))
    }

    fn parse_str(&mut self) -> Result<String, JsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(b) = self.bump() {
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.bump().ok_or_else(|| err("string: EOF in escape"))?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0C'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            // \uXXXX
                            let code = self.read_hex4()?;
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                            } else {
                                return Err(err("string: invalid unicode scalar"));
                            }
                        }
                        _ => return Err(err("string: invalid escape")),
                    }
                }
                c => out.push(c as char),
            }
        }
        Err(err("string: unexpected EOF"))
    }

    fn parse_num(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        let s = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| err("num: invalid utf8"))?;
        let n: f64 = s.parse().map_err(|_| err("num: parse failed"))?;
        Ok(JsonValue::Num(n))
    }

    fn read_hex4(&mut self) -> Result<u32, JsonError> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let b = self.bump().ok_or_else(|| err("unicode: EOF"))?;
            v <<= 4;
            v |= match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a' + 10) as u32,
                b'A'..=b'F' => (b - b'A' + 10) as u32,
                _ => return Err(err("unicode: invalid hex digit")),
            };
        }
        Ok(v)
    }

    fn expect(&mut self, b: u8) -> Result<(), JsonError> {
        self.skip_ws();
        let got = self.bump().ok_or_else(|| err("unexpected EOF"))?;
        if got != b {
            return Err(err(format!("expected '{}'", b as char)));
        }
        Ok(())
    }

    fn expect_bytes(&mut self, lit: &[u8]) -> Result<(), JsonError> {
        for &b in lit {
            let got = self.bump().ok_or_else(|| err("unexpected EOF"))?;
            if got != b {
                return Err(err("unexpected literal"));
            }
        }
        Ok(())
    }
}

/* ----------------------------- Kind mapping ------------------------------ */

fn artifact_kind_to_str(k: &ArtifactKind) -> &'static str {
    match k {
        ArtifactKind::Source => "source",
        ArtifactKind::Intermediate => "intermediate",
        ArtifactKind::Output => "output",
        ArtifactKind::Meta => "meta",
        ArtifactKind::External => "external",
    }
}

fn artifact_kind_from_str(s: &str) -> Result<ArtifactKind, JsonError> {
    match s {
        "source" => Ok(ArtifactKind::Source),
        "intermediate" => Ok(ArtifactKind::Intermediate),
        "output" => Ok(ArtifactKind::Output),
        "meta" => Ok(ArtifactKind::Meta),
        "external" => Ok(ArtifactKind::External),
        _ => Err(err(format!("unknown artifact kind: {s}"))),
    }
}

/* ----------------------------- String escape ----------------------------- */

fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let v = c as u32;
                out.push_str("\\u");
                out.push_str(&format!("{:04x}", v));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/* -------------------------------- Tests --------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::bake::{Action, Artifact, ArtifactKind, BakeGraph, Node};

    #[test]
    fn json_roundtrip_basic() {
        let mut g = BakeGraph::new();

        let src = Artifact::source("src/main.c");
        let obj = Artifact::logical("obj/main.o", ArtifactKind::Intermediate);
        let exe = Artifact::output("bin/app");

        g.add_artifact(src.clone());
        g.add_artifact(obj.clone());
        g.add_artifact(exe.clone());

        let n1 = Node::new("compile", Action::new("clang").arg("-c"))
            .input(&src)
            .output(&obj);
        let n2 = Node::new("link", Action::new("clang"))
            .input(&obj)
            .output(&exe);

        g.add_node(n1);
        g.add_node(n2);
        g.infer_deps_from_artifacts().unwrap();

        let j = GraphJson::from_graph(&g);
        let s = j.to_string_pretty();

        let parsed = GraphJson::parse(&s).unwrap();
        let g2 = parsed.into_graph().unwrap();

        assert_eq!(g2.nodes.len(), g.nodes.len());
        assert_eq!(g2.artifacts.len(), g.artifacts.len());
        let deps1: usize = g.deps.values().map(|v| v.len()).sum();
        let deps2: usize = g2.deps.values().map(|v| v.len()).sum();
        assert_eq!(deps2, deps1);
    }

    #[test]
    fn json_compact_is_single_line() {
        let j = GraphJson::new();
        let s = j.to_string_compact();
        assert!(!s.contains('\n'));
        assert!(s.starts_with('{'));
        assert!(s.ends_with('}'));
    }
}
