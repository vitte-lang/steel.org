// src/output.rs
//
// Steel — output / logging / console rendering
//
// Purpose:
// - Provide a centralized output layer for Steel:
//   - structured events (info/warn/error/debug/trace)
//   - colored terminal rendering (ANSI), with auto-disable
//   - progress + spinners (optional, minimal, no external deps)
//   - stable formatting for CI logs
//   - capture mode for tests
//
// Notes:
// - No external crates.
// - Windows ANSI: assumes modern Windows terminals support VT; we provide a switch to disable.
// - This module does not implement full TTY detection; integrate with your `cli/ansi` module if present.
//
// Integration:
// - Replace println!/eprintln! calls by `out.info(...)`, `out.warn(...)`, etc.
// - For diagnostics, use `emit_diag` with file/line spans.
//
// Threading:
// - `Output` is Send + Sync via internal Mutex.
// - You can clone `Output` and use it across threads.
//
// Design:
// - OutputSink trait (stdout/stderr/capture).
// - OutputConfig toggles level + color + timestamps.
// - Event carries metadata (target, module, job id, rule id, etc).

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::hash::Hasher;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/* ============================== config/types ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub level: Level,
    pub color: ColorMode,
    pub timestamps: bool,
    pub show_target: bool,
    pub show_kv: bool,
    pub line_buffered: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            level: Level::Info,
            color: ColorMode::Auto,
            timestamps: false,
            show_target: true,
            show_kv: true,
            line_buffered: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

/* ============================== events ============================== */

#[derive(Debug, Clone)]
pub struct Event {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub kv: BTreeMap<String, String>,
    pub time: Option<SystemTime>,
    pub stream: Stream,
}

impl Event {
    pub fn new(level: Level, target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            target: target.into(),
            message: message.into(),
            kv: BTreeMap::new(),
            time: None,
            stream: Stream::Stdout,
        }
    }

    pub fn kv(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.kv.insert(k.into(), v.into());
        self
    }

    pub fn stderr(mut self) -> Self {
        self.stream = Stream::Stderr;
        self
    }

    pub fn with_time(mut self, t: SystemTime) -> Self {
        self.time = Some(t);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/* ============================== sinks ============================== */

pub trait OutputSink: Send {
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

pub struct StdIoSink {
    out: io::Stdout,
    err: io::Stderr,
}

impl StdIoSink {
    pub fn new() -> Self {
        Self {
            out: io::stdout(),
            err: io::stderr(),
        }
    }
}

impl Default for StdIoSink {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputSink for StdIoSink {
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> io::Result<()> {
        match stream {
            Stream::Stdout => self.out.write_all(bytes),
            Stream::Stderr => self.err.write_all(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()?;
        self.err.flush()?;
        Ok(())
    }
}

/// Capture sink for tests.
#[derive(Default)]
pub struct CaptureSink {
    pub out: Vec<u8>,
    pub err: Vec<u8>,
}

impl CaptureSink {
    pub fn take_stdout(&mut self) -> String {
        let s = String::from_utf8_lossy(&self.out).to_string();
        self.out.clear();
        s
    }

    pub fn take_stderr(&mut self) -> String {
        let s = String::from_utf8_lossy(&self.err).to_string();
        self.err.clear();
        s
    }
}

impl OutputSink for CaptureSink {
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> io::Result<()> {
        match stream {
            Stream::Stdout => self.out.extend_from_slice(bytes),
            Stream::Stderr => self.err.extend_from_slice(bytes),
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/* ============================== output core ============================== */

struct OutputInner {
    cfg: OutputConfig,
    sink: Box<dyn OutputSink>,
    color_enabled: bool,

    // progress
    progress: Option<ProgressState>,
}

#[derive(Clone)]
pub struct Output {
    inner: Arc<Mutex<OutputInner>>,
}

impl Output {
    pub fn new() -> Self {
        Self::with_sink(OutputConfig::default(), Box::new(StdIoSink::new()))
    }

    pub fn with_sink(cfg: OutputConfig, sink: Box<dyn OutputSink>) -> Self {
        let color_enabled = match cfg.color {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => true, // best-effort; integrate real TTY detection elsewhere
        };
        Self {
            inner: Arc::new(Mutex::new(OutputInner {
                cfg,
                sink,
                color_enabled,
                progress: None,
            })),
        }
    }

    pub fn config(&self) -> OutputConfig {
        self.inner.lock().unwrap().cfg.clone()
    }

    pub fn set_config(&self, cfg: OutputConfig) {
        let mut g = self.inner.lock().unwrap();
        g.color_enabled = match cfg.color {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => true,
        };
        g.cfg = cfg;
    }

    pub fn flush(&self) {
        let _ = self.inner.lock().unwrap().sink.flush();
    }

    pub fn emit(&self, mut ev: Event) {
        let mut g = self.inner.lock().unwrap();

        if ev.level > g.cfg.level {
            return;
        }

        if g.cfg.timestamps && ev.time.is_none() {
            ev.time = Some(SystemTime::now());
        }

        // If progress is active, clear it before writing a normal line.
        if g.progress.is_some() {
            let _ = g.sink.write(Stream::Stdout, b"\r");
            let _ = g.sink.write(Stream::Stdout, b"\x1b[2K"); // clear line
        }

        let line = format_event_line(&ev, &g.cfg, g.color_enabled);
        let _ = g.sink.write(ev.stream, line.as_bytes());
        let _ = g.sink.write(ev.stream, b"\n");

        // Re-render progress
        if let Some(p) = &g.progress {
            let s = p.render(g.color_enabled);
            let _ = g.sink.write(Stream::Stdout, s.as_bytes());
        }
    }

    /* ---------- convenience ---------- */

    pub fn error(&self, target: impl Into<String>, msg: impl Into<String>) {
        self.emit(Event::new(Level::Error, target, msg).stderr());
    }
    pub fn warn(&self, target: impl Into<String>, msg: impl Into<String>) {
        self.emit(Event::new(Level::Warn, target, msg).stderr());
    }
    pub fn info(&self, target: impl Into<String>, msg: impl Into<String>) {
        self.emit(Event::new(Level::Info, target, msg));
    }
    pub fn debug(&self, target: impl Into<String>, msg: impl Into<String>) {
        self.emit(Event::new(Level::Debug, target, msg));
    }
    pub fn trace(&self, target: impl Into<String>, msg: impl Into<String>) {
        self.emit(Event::new(Level::Trace, target, msg));
    }

    /* ---------- progress ---------- */

    pub fn progress_start(&self, label: impl Into<String>) {
        let mut g = self.inner.lock().unwrap();
        g.progress = Some(ProgressState::new(label.into()));
        let s = g.progress.as_ref().unwrap().render(g.color_enabled);
        let _ = g.sink.write(Stream::Stdout, s.as_bytes());
    }

    pub fn progress_tick(&self, current: u64, total: Option<u64>, message: Option<String>) {
        let mut g = self.inner.lock().unwrap();
        let color = g.color_enabled;
        if let Some(p) = g.progress.as_mut() {
            p.current = current;
            p.total = total;
            if let Some(m) = message {
                p.message = m;
            }
            let s = p.render(color);
            let _ = g.sink.write(Stream::Stdout, s.as_bytes());
        }
    }

    pub fn progress_end(&self, final_msg: Option<String>) {
        let mut g = self.inner.lock().unwrap();
        if let Some(mut p) = g.progress.take() {
            if let Some(m) = final_msg {
                p.message = m;
            }
            // clear line then print final line
            let _ = g.sink.write(Stream::Stdout, b"\r\x1b[2K");
            let s = p.render_done(g.color_enabled);
            let _ = g.sink.write(Stream::Stdout, s.as_bytes());
            let _ = g.sink.write(Stream::Stdout, b"\n");
        }
    }
}

/* ============================== formatting ============================== */

fn format_event_line(ev: &Event, cfg: &OutputConfig, color: bool) -> String {
    let mut s = String::new();

    if cfg.timestamps {
        if let Some(t) = ev.time {
            s.push_str(&format_time(t));
            s.push(' ');
        }
    }

    // level
    if color {
        s.push_str(level_color(ev.level));
        s.push_str(&ev.level.to_string());
        s.push_str(ansi_reset());
    } else {
        s.push_str(&ev.level.to_string());
    }

    // target
    if cfg.show_target {
        s.push(' ');
        s.push('[');
        s.push_str(&ev.target);
        s.push(']');
    }

    s.push(' ');
    s.push_str(&ev.message);

    if cfg.show_kv && !ev.kv.is_empty() {
        for (k, v) in &ev.kv {
            s.push(' ');
            if color {
                s.push_str("\x1b[2m"); // dim
            }
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            if color {
                s.push_str(ansi_reset());
            }
        }
    }

    s
}

fn format_time(t: SystemTime) -> String {
    // Simple epoch-based timestamp to keep deps zero.
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::from_secs(0));
    format!("{:>10}.{:03}", d.as_secs(), d.subsec_millis())
}

/* ============================== ANSI helpers ============================== */

fn ansi_reset() -> &'static str {
    "\x1b[0m"
}

fn level_color(l: Level) -> &'static str {
    match l {
        Level::Error => "\x1b[31;1m", // bright red
        Level::Warn => "\x1b[33;1m",  // bright yellow
        Level::Info => "\x1b[32;1m",  // bright green
        Level::Debug => "\x1b[36m",   // cyan
        Level::Trace => "\x1b[2m",    // dim
    }
}

/* ============================== progress ============================== */

#[derive(Debug, Clone)]
struct ProgressState {
    label: String,
    message: String,
    current: u64,
    total: Option<u64>,
    spinner: Spinner,
}

impl ProgressState {
    fn new(label: String) -> Self {
        Self {
            label,
            message: String::new(),
            current: 0,
            total: None,
            spinner: Spinner::new(),
        }
    }

    fn render(&self, color: bool) -> String {
        let sp = self.spinner.frame(self.current);
        let mut s = String::new();
        s.push('\r');
        s.push_str("\x1b[2K"); // clear line

        if color {
            s.push_str("\x1b[35m"); // magenta
        }
        s.push_str(sp);
        if color {
            s.push_str(ansi_reset());
        }

        s.push(' ');
        s.push_str(&self.label);

        if let Some(t) = self.total {
            s.push_str(&format!(" [{}/{}]", self.current, t));
        } else {
            s.push_str(&format!(" [{}]", self.current));
        }

        if !self.message.is_empty() {
            s.push(' ');
            s.push_str(&self.message);
        }

        s
    }

    fn render_done(&self, color: bool) -> String {
        let mut s = String::new();
        if color {
            s.push_str("\x1b[32;1m"); // green
        }
        s.push_str("OK");
        if color {
            s.push_str(ansi_reset());
        }
        s.push(' ');
        s.push_str(&self.label);
        if let Some(t) = self.total {
            s.push_str(&format!(" [{}/{}]", self.current, t));
        } else {
            s.push_str(&format!(" [{}]", self.current));
        }
        if !self.message.is_empty() {
            s.push(' ');
            s.push_str(&self.message);
        }
        s
    }
}

#[derive(Debug, Clone)]
struct Spinner;

impl Spinner {
    fn new() -> Self {
        Self
    }

    fn frame(&self, tick: u64) -> &'static str {
        const FRAMES: &[&str] = &["|", "/", "-", "\\"];
        FRAMES[(tick as usize) % FRAMES.len()]
    }
}

/* ============================== diagnostics helper ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub line_end: u32,
    pub col_end: u32,
}

pub fn emit_diag(out: &Output, level: Level, path: &str, span: Span, msg: &str) {
    let mut ev = Event::new(level, "diag", msg.to_string());
    ev = ev.kv("file", path.to_string());
    if span.line != 0 {
        ev = ev.kv("line", span.line.to_string()).kv("col", span.col.to_string());
    }
    if level <= Level::Warn {
        ev.stream = Stream::Stderr;
    }
    out.emit(ev);
}

/* ============================== hashing (for stable ids) ============================== */

#[derive(Default)]
struct Fnv1aHasher {
    state: u64,
}

impl Hasher for Fnv1aHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.state == 0 { 0xcbf29ce484222325 } else { self.state };
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.state = hash;
    }

    fn finish(&self) -> u64 {
        if self.state == 0 {
            0xcbf29ce484222325
        } else {
            self.state
        }
    }
}

/* ============================== tests ============================== */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_sink_collects() {
        let sink = Box::new(CaptureSink::default());
        let out = Output::with_sink(OutputConfig { level: Level::Trace, ..Default::default() }, sink);

        out.info("test", "hello");
        out.warn("test", "warn");
        out.error("test", "err");

        out.flush();
        // Can't downcast sink from Output; this test ensures no panic and basic formatting.
    }

    #[test]
    fn event_format_includes_target() {
        let ev = Event::new(Level::Info, "x", "hello").kv("k", "v");
        let s = format_event_line(&ev, &OutputConfig::default(), false);
        assert!(s.contains("[x]"));
        assert!(s.contains("k=v"));
    }
}
