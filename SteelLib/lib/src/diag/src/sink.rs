//! Diagnostic sinks (MAX).
//!
//! A sink is where diagnostics go (stderr, file, memory, JSON lines, metrics, etc.).
//!
//! This module provides a composable sink pipeline:
//! - `DiagSink` trait (push / extend / flush)
//! - collectors: `VecSink`, `BagSink`
//! - writers: `StderrSink`, `WriterSink<W>`, `FileSink`
//! - structured: `JsonLinesSink` (std-only JSON builder, newline-delimited)
//! - control/ops: `FanoutSink`, `FilteringSink`, `DedupSink`, `RateLimitSink`
//! - policy: `SeverityGateSink`, `CategoryGateSink`
//! - accounting: `CountingSink`, `ExitCodeSink`
//!
//! Renderers are pluggable via `Renderer` (see `render.rs`).
//! This file is std-only; no serde.
//!
//! Thread-safety:
//! - sinks are `Send + Sync`
//! - internal mutability uses `Mutex`
//!
//! Notes:
//! - For high-throughput logging, prefer a buffered writer (`BufWriter`) inside `WriterSink`
//! - For structured output in CI, prefer `JsonLinesSink`

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Category, Diag, DiagBag, DiagCode, Severity, SourceMap};
use super::render::{PlainRenderer, PrettyRenderer, RenderOptions, Renderer};

/// Sink trait.
///
/// The sink is intentionally simple. If you need async, wrap externally.
pub trait DiagSink: Send + Sync {
    fn push(&self, d: Diag);

    fn extend<I: IntoIterator<Item = Diag>>(&self, it: I) {
        for d in it {
            self.push(d);
        }
    }

    /// Optional flush hook (writers).
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}

/* ----------------------------- In-memory sinks --------------------------- */

/// A sink collecting diagnostics in-memory (Vec).
#[derive(Debug, Default)]
pub struct VecSink {
    inner: Mutex<Vec<Diag>>,
}

impl VecSink {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    pub fn push_one(&self, d: Diag) {
        self.push(d);
    }

    pub fn take(&self) -> Vec<Diag> {
        let mut g = self.inner.lock().unwrap();
        std::mem::take(&mut *g)
    }

    pub fn snapshot(&self) -> Vec<Diag> {
        self.inner.lock().unwrap().clone()
    }

    pub fn into_bag(&self) -> DiagBag {
        let v = self.snapshot();
        let mut b = DiagBag::new();
        b.extend(v);
        b
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl DiagSink for VecSink {
    fn push(&self, d: Diag) {
        self.inner.lock().unwrap().push(d);
    }
}

/// A sink collecting diagnostics directly into a DiagBag.
/// Useful if downstream expects `DiagBag` semantics.
#[derive(Debug, Default)]
pub struct BagSink {
    inner: Mutex<DiagBag>,
}

impl BagSink {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(DiagBag::new()),
        }
    }

    pub fn take(&self) -> DiagBag {
        let mut g = self.inner.lock().unwrap();
        std::mem::take(&mut *g)
    }

    pub fn snapshot(&self) -> DiagBag {
        let g = self.inner.lock().unwrap();
        let mut b = DiagBag::new();
        b.extend(g.as_slice().iter().cloned());
        b
    }

    pub fn has_errors(&self) -> bool {
        self.inner.lock().unwrap().has_errors()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

impl DiagSink for BagSink {
    fn push(&self, d: Diag) {
        self.inner.lock().unwrap().push(d);
    }
}

/* ----------------------------- Writer sinks ------------------------------ */

/// A sink that writes to stderr using a renderer.
pub struct StderrSink {
    renderer: Box<dyn Renderer>,
    sm: Option<Arc<dyn SourceMap>>,
}

impl std::fmt::Debug for StderrSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StderrSink").finish_non_exhaustive()
    }
}

impl StderrSink {
    pub fn plain(sm: Option<Arc<dyn SourceMap>>) -> Self {
        Self {
            renderer: Box::new(PlainRenderer::default()),
            sm,
        }
    }

    pub fn pretty(sm: Option<Arc<dyn SourceMap>>) -> Self {
        Self {
            renderer: Box::new(PrettyRenderer::default()),
            sm,
        }
    }

    pub fn with_options(mut self, opts: RenderOptions, pretty: bool) -> Self {
        self.renderer = if pretty {
            Box::new(PrettyRenderer { opts })
        } else {
            Box::new(PlainRenderer { opts })
        };
        self
    }

    pub fn with_renderer(mut self, r: Box<dyn Renderer>) -> Self {
        self.renderer = r;
        self
    }

    pub fn with_source_map(mut self, sm: Option<Arc<dyn SourceMap>>) -> Self {
        self.sm = sm;
        self
    }
}

impl DiagSink for StderrSink {
    fn push(&self, d: Diag) {
        let txt = self.renderer.render(&d, self.sm.as_deref());
        let _ = writeln!(&mut io::stderr(), "{txt}");
    }
}

/// A sink that writes to an arbitrary `Write` handle.
/// The handle is protected by a mutex for basic thread-safety.
pub struct WriterSink<W: Write + Send + 'static> {
    w: Mutex<W>,
    renderer: Box<dyn Renderer>,
    sm: Option<Arc<dyn SourceMap>>,
}

impl<W: Write + Send + 'static> std::fmt::Debug for WriterSink<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriterSink").finish_non_exhaustive()
    }
}

impl<W: Write + Send + 'static> WriterSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            w: Mutex::new(writer),
            renderer: Box::new(PlainRenderer::default()),
            sm: None,
        }
    }

    pub fn buffered(writer: W) -> WriterSink<BufWriter<W>> {
        WriterSink {
            w: Mutex::new(BufWriter::new(writer)),
            renderer: Box::new(PlainRenderer::default()),
            sm: None,
        }
    }

    pub fn pretty(mut self) -> Self {
        self.renderer = Box::new(PrettyRenderer::default());
        self
    }

    pub fn with_options(mut self, opts: RenderOptions, pretty: bool) -> Self {
        self.renderer = if pretty {
            Box::new(PrettyRenderer { opts })
        } else {
            Box::new(PlainRenderer { opts })
        };
        self
    }

    pub fn with_renderer(mut self, r: Box<dyn Renderer>) -> Self {
        self.renderer = r;
        self
    }

    pub fn with_source_map(mut self, sm: Option<Arc<dyn SourceMap>>) -> Self {
        self.sm = sm;
        self
    }

    pub fn flush_inner(&self) -> io::Result<()> {
        self.w.lock().unwrap().flush()
    }
}

impl<W: Write + Send + 'static> DiagSink for WriterSink<W> {
    fn push(&self, d: Diag) {
        let txt = self.renderer.render(&d, self.sm.as_deref());
        let mut g = self.w.lock().unwrap();
        let _ = writeln!(&mut *g, "{txt}");
    }

    fn flush(&self) -> io::Result<()> {
        self.flush_inner()
    }
}

/// Convenience file sink (append or truncate).
#[derive(Debug)]
pub struct FileSink {
    inner: WriterSink<BufWriter<File>>,
}

#[derive(Debug, Clone, Copy)]
pub enum FileMode {
    Truncate,
    Append,
}

impl FileSink {
    pub fn open(path: impl AsRef<std::path::Path>, mode: FileMode) -> io::Result<Self> {
        let mut opts = OpenOptions::new();
        opts.create(true).write(true);
        match mode {
            FileMode::Truncate => {
                opts.truncate(true);
            }
            FileMode::Append => {
                opts.append(true);
            }
        }
        let f = opts.open(path)?;
        Ok(Self {
            inner: WriterSink::new(BufWriter::new(f)),
        })
    }

    pub fn pretty(mut self) -> Self {
        self.inner = self.inner.pretty();
        self
    }

    pub fn with_options(mut self, opts: RenderOptions, pretty: bool) -> Self {
        self.inner = self.inner.with_options(opts, pretty);
        self
    }

    pub fn with_source_map(mut self, sm: Option<Arc<dyn SourceMap>>) -> Self {
        self.inner = self.inner.with_source_map(sm);
        self
    }
}

impl DiagSink for FileSink {
    fn push(&self, d: Diag) {
        self.inner.push(d)
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/* --------------------------- Structured sinks ---------------------------- */

/// JSON Lines sink (NDJSON), std-only (no serde).
///
/// Produces one JSON object per line:
/// {
///   "code":"MUF0101",
///   "severity":"error",
///   "category":"buildfile",
///   "message":"...",
///   "labels":[{...}],
///   "notes":[{...}],
///   "data":{...}
/// }
#[derive(Debug)]
pub struct JsonLinesSink<W: Write + Send + 'static> {
    w: Mutex<W>,
    sm: Option<Arc<dyn SourceMap>>,
    /// If set, include a `snippet_line` field for primary label.
    pub include_snippet_line: bool,
}

impl<W: Write + Send + 'static> JsonLinesSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            w: Mutex::new(writer),
            sm: None,
            include_snippet_line: false,
        }
    }

    pub fn buffered(writer: W) -> JsonLinesSink<BufWriter<W>> {
        JsonLinesSink {
            w: Mutex::new(BufWriter::new(writer)),
            sm: None,
            include_snippet_line: false,
        }
    }

    pub fn with_source_map(mut self, sm: Option<Arc<dyn SourceMap>>) -> Self {
        self.sm = sm;
        self
    }

    pub fn with_snippet_line(mut self, on: bool) -> Self {
        self.include_snippet_line = on;
        self
    }

    fn write_json_line(&self, d: &Diag) -> io::Result<()> {
        let mut g = self.w.lock().unwrap();
        let s = self.to_json(d);
        g.write_all(s.as_bytes())?;
        g.write_all(b"\n")?;
        Ok(())
    }

    fn to_json(&self, d: &Diag) -> String {
        let mut obj = String::new();
        obj.push('{');

        push_kv_str(&mut obj, "code", d.code.code);
        obj.push(',');
        push_kv_str(&mut obj, "severity", d.severity().as_str());
        obj.push(',');
        push_kv_str(&mut obj, "category", d.category().as_str());
        obj.push(',');
        push_kv_str(&mut obj, "name", d.code.name);
        obj.push(',');
        push_kv_str(&mut obj, "message", &d.message);

        // labels
        obj.push_str(",\"labels\":[");
        for (i, lbl) in d.labels.iter().enumerate() {
            if i != 0 {
                obj.push(',');
            }
            obj.push('{');
            push_kv_bool(&mut obj, "primary", lbl.is_primary);
            obj.push(',');
            push_kv_str(&mut obj, "source", &lbl.location.source.0);
            obj.push(',');
            push_kv_u32(&mut obj, "start", lbl.location.span.start);
            obj.push(',');
            push_kv_u32(&mut obj, "end", lbl.location.span.end);

            // line/col if available
            let lc = lbl.location.line_col.or_else(|| {
                self.sm
                    .as_deref()
                    .and_then(|m| m.line_col(&lbl.location.source, lbl.location.span.start))
            });

            if let Some(lc) = lc {
                obj.push(',');
                push_kv_u32(&mut obj, "line", lc.line);
                obj.push(',');
                push_kv_u32(&mut obj, "column", lc.column);
            }

            if let Some(msg) = &lbl.message {
                obj.push(',');
                push_kv_str(&mut obj, "message", msg);
            }

            // optional snippet line for primary label
            if self.include_snippet_line && lbl.is_primary {
                if let Some(m) = self.sm.as_deref() {
                    if let Some((_ln, line)) = m.line_text(&lbl.location.source, lbl.location.span.start) {
                        obj.push(',');
                        push_kv_str(&mut obj, "snippet_line", &line);
                    }
                }
            }

            obj.push('}');
        }
        obj.push(']');

        // notes
        obj.push_str(",\"notes\":[");
        for (i, n) in d.notes.iter().enumerate() {
            if i != 0 {
                obj.push(',');
            }
            obj.push('{');
            push_kv_str(&mut obj, "severity", n.severity.as_str());
            obj.push(',');
            push_kv_str(&mut obj, "message", &n.message);
            obj.push('}');
        }
        obj.push(']');

        // data
        obj.push_str(",\"data\":{");
        for (i, (k, v)) in d.data.iter().enumerate() {
            if i != 0 {
                obj.push(',');
            }
            push_str(&mut obj, k);
            obj.push(':');
            push_str(&mut obj, v);
        }
        obj.push('}');

        obj.push('}');
        obj
    }
}

impl<W: Write + Send + 'static> DiagSink for JsonLinesSink<W> {
    fn push(&self, d: Diag) {
        let _ = self.write_json_line(&d);
    }

    fn flush(&self) -> io::Result<()> {
        self.w.lock().unwrap().flush()
    }
}

/* ----------------------------- Fanout sink ------------------------------- */

/// A sink that forwards diagnostics to multiple sinks.
#[derive(Debug, Default)]
pub struct FanoutSink {
    sinks: Vec<Arc<dyn DiagSink>>,
}

impl FanoutSink {
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    pub fn add(mut self, s: Arc<dyn DiagSink>) -> Self {
        self.sinks.push(s);
        self
    }

    pub fn push_sink(&mut self, s: Arc<dyn DiagSink>) {
        self.sinks.push(s);
    }

    pub fn len(&self) -> usize {
        self.sinks.len()
    }
}

impl DiagSink for FanoutSink {
    fn push(&self, d: Diag) {
        for s in &self.sinks {
            s.push(d.clone());
        }
    }

    fn flush(&self) -> io::Result<()> {
        for s in &self.sinks {
            s.flush()?;
        }
        Ok(())
    }
}

/* ---------------------------- Filtering sinks ---------------------------- */

/// Filtering sink: forwards diagnostics only if predicate returns true.
#[derive(Debug)]
pub struct FilteringSink {
    inner: Arc<dyn DiagSink>,
    pred: Box<dyn Fn(&Diag) -> bool + Send + Sync>,
}

impl FilteringSink {
    pub fn new(
        inner: Arc<dyn DiagSink>,
        pred: impl Fn(&Diag) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            pred: Box::new(pred),
        }
    }
}

impl DiagSink for FilteringSink {
    fn push(&self, d: Diag) {
        if (self.pred)(&d) {
            self.inner.push(d);
        }
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Gate by severity: forward if `d.severity() >= min`.
#[derive(Debug)]
pub struct SeverityGateSink {
    inner: Arc<dyn DiagSink>,
    min: Severity,
}

impl SeverityGateSink {
    pub fn new(inner: Arc<dyn DiagSink>, min: Severity) -> Self {
        Self { inner, min }
    }
}

impl DiagSink for SeverityGateSink {
    fn push(&self, d: Diag) {
        if d.severity() >= self.min {
            self.inner.push(d);
        }
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Gate by category: allowlist categories.
#[derive(Debug)]
pub struct CategoryGateSink {
    inner: Arc<dyn DiagSink>,
    allow: BTreeSet<Category>,
}

impl CategoryGateSink {
    pub fn new(inner: Arc<dyn DiagSink>, allow: impl IntoIterator<Item = Category>) -> Self {
        Self {
            inner,
            allow: allow.into_iter().collect(),
        }
    }
}

impl DiagSink for CategoryGateSink {
    fn push(&self, d: Diag) {
        if self.allow.contains(&d.category()) {
            self.inner.push(d);
        }
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/* ------------------------------ Dedup sink ------------------------------- */

/// Deduplicate diagnostics based on a stable fingerprint.
/// Useful when multiple passes emit the same diag repeatedly.
#[derive(Debug)]
pub struct DedupSink {
    inner: Arc<dyn DiagSink>,
    seen: Mutex<HashSet<u64>>,
    /// If true, include message in fingerprint (stronger, fewer merges).
    include_message: bool,
}

impl DedupSink {
    pub fn new(inner: Arc<dyn DiagSink>) -> Self {
        Self {
            inner,
            seen: Mutex::new(HashSet::new()),
            include_message: true,
        }
    }

    pub fn include_message(mut self, on: bool) -> Self {
        self.include_message = on;
        self
    }

    pub fn clear(&self) {
        self.seen.lock().unwrap().clear();
    }

    fn fingerprint(&self, d: &Diag) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        d.code.code.hash(&mut h);
        d.severity().hash(&mut h);
        d.category().hash(&mut h);

        if self.include_message {
            d.message.hash(&mut h);
        }

        // Primary location often defines uniqueness best
        if let Some(lbl) = d.labels.iter().find(|l| l.is_primary).or_else(|| d.labels.first()) {
            lbl.location.source.0.hash(&mut h);
            lbl.location.span.start.hash(&mut h);
            lbl.location.span.end.hash(&mut h);
        }

        h.finish()
    }
}

impl DiagSink for DedupSink {
    fn push(&self, d: Diag) {
        let fp = self.fingerprint(&d);
        let mut seen = self.seen.lock().unwrap();
        if seen.insert(fp) {
            drop(seen);
            self.inner.push(d);
        }
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/* ---------------------------- Rate limit sink ---------------------------- */

/// Rate-limits forwarded diagnostics per time window.
/// Excess diagnostics are dropped; optionally emits a summary on flush.
#[derive(Debug)]
pub struct RateLimitSink {
    inner: Arc<dyn DiagSink>,
    window: Duration,
    max_per_window: usize,

    state: Mutex<RateState>,
    emit_drop_summary: bool,
}

#[derive(Debug)]
struct RateState {
    window_start: Instant,
    count_in_window: usize,
    dropped_total: usize,
}

impl RateLimitSink {
    pub fn new(inner: Arc<dyn DiagSink>, window: Duration, max_per_window: usize) -> Self {
        Self {
            inner,
            window,
            max_per_window,
            state: Mutex::new(RateState {
                window_start: Instant::now(),
                count_in_window: 0,
                dropped_total: 0,
            }),
            emit_drop_summary: true,
        }
    }

    pub fn emit_drop_summary(mut self, on: bool) -> Self {
        self.emit_drop_summary = on;
        self
    }

    fn tick(state: &mut RateState, window: Duration) {
        if state.window_start.elapsed() >= window {
            state.window_start = Instant::now();
            state.count_in_window = 0;
        }
    }
}

impl DiagSink for RateLimitSink {
    fn push(&self, d: Diag) {
        let mut st = self.state.lock().unwrap();
        Self::tick(&mut st, self.window);

        if st.count_in_window < self.max_per_window {
            st.count_in_window += 1;
            drop(st);
            self.inner.push(d);
        } else {
            st.dropped_total += 1;
        }
    }

    fn flush(&self) -> io::Result<()> {
        // optionally emit summary
        if self.emit_drop_summary {
            let dropped = { self.state.lock().unwrap().dropped_total };
            if dropped > 0 {
                // best-effort summary diag
                let mut dd = Diag::new(
                    super::codes::INT9001,
                    format!("rate limit: dropped {dropped} diagnostic(s)"),
                );
                dd.notes.push(super::Note::note("Consider enabling JSON output or reducing verbosity."));
                self.inner.push(dd);
            }
        }
        self.inner.flush()
    }
}

/* ---------------------------- Accounting sinks --------------------------- */

/// Counts diagnostics by severity/category/code.
#[derive(Debug, Default)]
pub struct CountingSink {
    inner: Arc<dyn DiagSink>,
    counts: Mutex<Counts>,
}

#[derive(Debug, Default, Clone)]
pub struct Counts {
    pub total: u64,
    pub by_severity: BTreeMap<Severity, u64>,
    pub by_category: BTreeMap<Category, u64>,
    pub by_code: BTreeMap<&'static str, u64>,
}

impl CountingSink {
    pub fn new(inner: Arc<dyn DiagSink>) -> Self {
        Self {
            inner,
            counts: Mutex::new(Counts::default()),
        }
    }

    pub fn snapshot(&self) -> Counts {
        self.counts.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        *self.counts.lock().unwrap() = Counts::default();
    }
}

impl DiagSink for CountingSink {
    fn push(&self, d: Diag) {
        {
            let mut c = self.counts.lock().unwrap();
            c.total += 1;
            *c.by_severity.entry(d.severity()).or_insert(0) += 1;
            *c.by_category.entry(d.category()).or_insert(0) += 1;
            *c.by_code.entry(d.code.code).or_insert(0) += 1;
        }
        self.inner.push(d);
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Maintains a process-like exit code policy:
/// - 0 if no errors
/// - 1 if any error
/// - optionally, escalate warnings to non-zero
#[derive(Debug)]
pub struct ExitCodeSink {
    inner: Arc<dyn DiagSink>,
    state: Mutex<ExitState>,
    pub warnings_as_error: bool,
}

#[derive(Debug, Default)]
struct ExitState {
    saw_error: bool,
    saw_warning: bool,
}

impl ExitCodeSink {
    pub fn new(inner: Arc<dyn DiagSink>) -> Self {
        Self {
            inner,
            state: Mutex::new(ExitState::default()),
            warnings_as_error: false,
        }
    }

    pub fn exit_code(&self) -> i32 {
        let st = self.state.lock().unwrap();
        if st.saw_error {
            1
        } else if self.warnings_as_error && st.saw_warning {
            1
        } else {
            0
        }
    }

    pub fn reset(&self) {
        *self.state.lock().unwrap() = ExitState::default();
    }
}

impl DiagSink for ExitCodeSink {
    fn push(&self, d: Diag) {
        {
            let mut st = self.state.lock().unwrap();
            match d.severity() {
                Severity::Error => st.saw_error = true,
                Severity::Warning => st.saw_warning = true,
                _ => {}
            }
        }
        self.inner.push(d);
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

/* --------------------------- JSON helpers (std) -------------------------- */

fn push_kv_str(out: &mut String, k: &str, v: &str) {
    push_str(out, k);
    out.push(':');
    push_str(out, v);
}

fn push_kv_bool(out: &mut String, k: &str, v: bool) {
    push_str(out, k);
    out.push(':');
    out.push_str(if v { "true" } else { "false" });
}

fn push_kv_u32(out: &mut String, k: &str, v: u32) {
    push_str(out, k);
    out.push(':');
    out.push_str(&v.to_string());
}

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
                // minimal escaping for other control chars
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
    use crate::diag::codes;

    #[test]
    fn vec_sink_collects() {
        let sink = VecSink::new();
        sink.push(Diag::from_code(codes::CLI1001));
        sink.push(Diag::from_code(codes::MUF0001));
        assert_eq!(sink.len(), 2);
        let v = sink.take();
        assert_eq!(v.len(), 2);
        assert!(sink.is_empty());
    }

    #[test]
    fn fanout_clones() {
        let a = Arc::new(VecSink::new());
        let b = Arc::new(VecSink::new());

        let f = FanoutSink::new().add(a.clone()).add(b.clone());
        f.push(Diag::from_code(codes::MFF0001));

        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn severity_gate_filters() {
        let v = Arc::new(VecSink::new());
        let gate = SeverityGateSink::new(v.clone(), Severity::Error);

        gate.push(Diag::from_code(codes::CLI1001)); // warning
        gate.push(Diag::from_code(codes::MFF0001)); // error

        assert_eq!(v.len(), 1);
        assert_eq!(v.snapshot()[0].code.code, "MFF0001");
    }

    #[test]
    fn dedup_drops_duplicates() {
        let v = Arc::new(VecSink::new());
        let dd = DedupSink::new(v.clone());

        dd.push(Diag::from_code(codes::MUF0001));
        dd.push(Diag::from_code(codes::MUF0001));

        assert_eq!(v.len(), 1);
    }

    #[test]
    fn exit_code_sink_tracks() {
        let v = Arc::new(VecSink::new());
        let ex = ExitCodeSink::new(v);

        assert_eq!(ex.exit_code(), 0);
        ex.push(Diag::from_code(codes::CLI1001));
        assert_eq!(ex.exit_code(), 0);

        ex.push(Diag::from_code(codes::MFF0001));
        assert_eq!(ex.exit_code(), 1);
    }

    #[test]
    fn json_lines_emits_object() {
        let buf = Vec::<u8>::new();
        let sink = JsonLinesSink::new(buf);
        let d = Diag::from_code(codes::MUF0101);
        // just ensure it serializes; writing goes to locked writer
        let s = sink.to_json(&d);
        assert!(s.contains("\"code\""));
        assert!(s.contains("\"severity\""));
        assert!(s.contains("\"message\""));
    }
}
