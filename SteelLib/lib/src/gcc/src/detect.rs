use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcKind {
    Gcc,
    Clang,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct CcTool {
    /// Exécutable résolu (peut être juste "gcc" si dans PATH).
    pub exe: PathBuf,
    pub kind: CcKind,

    /// Version brute, ex: "gcc (Rev...) 13.2.0" ou "clang version 17.0.6 ..."
    pub version_text: String,

    /// Triple, ex: "x86_64-w64-mingw32", "x86_64-linux-gnu", ...
    pub target_triple: Option<String>,

    /// S’il faut passer des args fixes au driver (rare), sinon vide.
    pub fixed_args: Vec<OsString>,
}

#[derive(Debug)]
pub enum DetectError {
    NotFound(String),
    ToolFailed { exe: PathBuf, msg: String },
    Io(std::io::Error),
}

impl From<std::io::Error> for DetectError {
    fn from(e: std::io::Error) -> Self {
        DetectError::Io(e)
    }
}

pub fn detect_cc() -> Result<CcTool, DetectError> {
    // 1) Respecter CC si défini
    if let Ok(cc) = env::var("CC") {
        if !cc.trim().is_empty() {
            if let Ok(tool) = probe_cc(&PathBuf::from(cc.trim())) {
                return Ok(tool);
            }
        }
    }

    // 2) Essayer des candidats standards
    let candidates = default_candidates();

    for cand in candidates {
        if let Ok(tool) = probe_cc(&cand) {
            return Ok(tool);
        }
    }

    Err(DetectError::NotFound(
        "No C compiler found (tried CC env, then gcc/clang/cc).".to_string(),
    ))
}

fn default_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();

    // Windows: mingw-w64 souvent fournit gcc/clang
    // MSVC cl.exe n’est pas géré ici (à faire dans un autre module msvc).
    v.push(PathBuf::from("gcc"));
    v.push(PathBuf::from("clang"));
    v.push(PathBuf::from("cc"));

    // Options explicites fréquentes
    v.push(PathBuf::from("x86_64-w64-mingw32-gcc"));
    v.push(PathBuf::from("aarch64-linux-gnu-gcc"));
    v.push(PathBuf::from("arm-linux-gnueabihf-gcc"));

    v
}

fn probe_cc(exe: &Path) -> Result<CcTool, DetectError> {
    // A) version text
    let version_out = run_capture(exe, &["--version"])
        .or_else(|_| run_capture(exe, &["-v"]))
        .map_err(|e| DetectError::ToolFailed {
            exe: exe.to_path_buf(),
            msg: format!("Failed to run compiler for version: {e}"),
        })?;

    let kind = classify_compiler(&version_out);

    // B) target triple (gcc: -dumpmachine / clang: -dumpmachine ou -print-target-triple)
    let target = match kind {
        CcKind::Gcc => run_capture(exe, &["-dumpmachine"]).ok().map(first_line_trim),
        CcKind::Clang => {
            // clang supporte souvent -dumpmachine (driver compatible)
            run_capture(exe, &["-dumpmachine"])
                .ok()
                .map(first_line_trim)
                .or_else(|| run_capture(exe, &["-print-target-triple"]).ok().map(first_line_trim))
        }
        CcKind::Unknown => {
            run_capture(exe, &["-dumpmachine"]).ok().map(first_line_trim)
        }
    };

    Ok(CcTool {
        exe: exe.to_path_buf(),
        kind,
        version_text: version_out,
        target_triple: target.filter(|s| !s.is_empty()),
        fixed_args: Vec::new(),
    })
}

fn classify_compiler(version_text: &str) -> CcKind {
    let v = version_text.to_ascii_lowercase();
    if v.contains("clang") {
        CcKind::Clang
    } else if v.contains("gcc") || v.contains("gnu compiler") || v.contains("free software foundation") {
        CcKind::Gcc
    } else {
        CcKind::Unknown
    }
}

fn first_line_trim(s: String) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn run_capture(exe: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let out = match out {
        Ok(o) => o,
        Err(e) => return Err(format!("{e}")),
    };

    if !out.status.success() {
        let mut msg = String::new();
        if !out.stdout.is_empty() {
            msg.push_str(&String::from_utf8_lossy(&out.stdout));
        }
        if !out.stderr.is_empty() {
            if !msg.is_empty() {
                msg.push('\n');
            }
            msg.push_str(&String::from_utf8_lossy(&out.stderr));
        }
        return Err(msg);
    }

    // Version/target peut sortir sur stdout ou stderr selon toolchain
    let mut s = String::new();
    if !out.stdout.is_empty() {
        s.push_str(&String::from_utf8_lossy(&out.stdout));
    }
    if !out.stderr.is_empty() {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    Ok(s.trim().to_string())
}
