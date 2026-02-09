use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::args::{CStd, GccArgs};
use super::detect::{detect_cc, CcKind, CcTool, DetectError};

#[derive(Debug)]
pub enum DriverError {
    Detect(DetectError),
    Io(std::io::Error),
    Failed {
        exe: PathBuf,
        args: Vec<OsString>,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl From<std::io::Error> for DriverError {
    fn from(e: std::io::Error) -> Self {
        DriverError::Io(e)
    }
}

impl From<DetectError> for DriverError {
    fn from(e: DetectError) -> Self {
        DriverError::Detect(e)
    }
}

#[derive(Debug, Clone)]
pub struct CompileUnit {
    pub src: PathBuf,   // .c
    pub obj: PathBuf,   // .o
    pub dep: Option<PathBuf>, // .d
}

#[derive(Debug, Clone)]
pub struct LinkUnit {
    pub objects: Vec<PathBuf>,
    pub output: PathBuf,       // exe/so/dll
    pub lib_dirs: Vec<PathBuf>,// -L
    pub libs: Vec<String>,     // -l
    pub rpaths: Vec<PathBuf>,  // rpath
    pub extra_ldflags: Vec<OsString>,
}

#[derive(Debug, Clone)]
pub struct CBuildConfig {
    pub root: PathBuf,
    pub out_dir: PathBuf,  // base target dir
    pub target: Option<String>,
    pub profile: String,   // dev/release/...
    pub c_std: CStd,

    pub includes: Vec<PathBuf>,
    pub isystem: Vec<PathBuf>,
    pub defines: Vec<(String, Option<String>)>,
    pub undefines: Vec<String>,

    pub warnings: Vec<String>,
    pub werror: bool,

    pub debug: bool,
    pub opt_level: u8,
    pub lto: bool,
    pub pic: bool,
    pub pie: bool,

    pub extra_cflags: Vec<OsString>,
}

impl Default for CBuildConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            out_dir: PathBuf::from("target"),
            target: None,
            profile: "dev".to_string(),
            c_std: CStd::C17,
            includes: vec![],
            isystem: vec![],
            defines: vec![],
            undefines: vec![],
            warnings: vec!["-Wall".into(), "-Wextra".into()],
            werror: false,
            debug: true,
            opt_level: 0,
            lto: false,
            pic: false,
            pie: false,
            extra_cflags: vec![],
        }
    }
}

pub struct GccDriver {
    pub cc: CcTool,
}

impl GccDriver {
    pub fn new() -> Result<Self, DriverError> {
        let cc = detect_cc()?;
        Ok(Self { cc })
    }

    pub fn compile_one(
        &self,
        cfg: &CBuildConfig,
        unit: &CompileUnit,
        print_only: bool,
    ) -> Result<(), DriverError> {
        // Assurer les dossiers
        if let Some(p) = unit.obj.parent() {
            std::fs::create_dir_all(p)?;
        }
        if let Some(dep) = &unit.dep {
            if let Some(p) = dep.parent() {
                std::fs::create_dir_all(p)?;
            }
        }

        let mut a = GccArgs::compile();
        a.clang_like = self.cc.kind == CcKind::Clang;
        a.target = cfg.target.clone().or_else(|| self.cc.target_triple.clone());
        a.c_std = Some(cfg.c_std);
        a.debug = cfg.debug;
        a.opt_level = Some(cfg.opt_level);
        a.lto = cfg.lto;
        a.pic = cfg.pic;
        a.pie = cfg.pie;

        a.includes = cfg.includes.clone();
        a.isystem = cfg.isystem.clone();
        a.defines = cfg.defines.clone();
        a.undefines = cfg.undefines.clone();

        a.warnings = cfg.warnings.clone();
        a.werror = cfg.werror;

        a.extra_cflags = cfg.extra_cflags.clone();

        a.input = Some(unit.src.clone());
        a.output = Some(unit.obj.clone());
        a.depfile = unit.dep.clone();

        let argv = a.build_args();
        self.run(&self.cc.exe, &argv, print_only)
    }

    pub fn compile_all(
        &self,
        cfg: &CBuildConfig,
        units: &[CompileUnit],
        print_only: bool,
    ) -> Result<(), DriverError> {
        for u in units {
            self.compile_one(cfg, u, print_only)?;
        }
        Ok(())
    }

    pub fn link(
        &self,
        cfg: &CBuildConfig,
        link: &LinkUnit,
        print_only: bool,
    ) -> Result<(), DriverError> {
        if let Some(p) = link.output.parent() {
            std::fs::create_dir_all(p)?;
        }

        let mut a = GccArgs::link();
        a.clang_like = self.cc.kind == CcKind::Clang;
        a.target = cfg.target.clone().or_else(|| self.cc.target_triple.clone());
        a.opt_level = Some(cfg.opt_level);
        a.lto = cfg.lto;

        a.link_inputs = link.objects.clone();
        a.lib_dirs = link.lib_dirs.clone();
        a.libs = link.libs.clone();
        a.rpaths = link.rpaths.clone();
        a.extra_ldflags = link.extra_ldflags.clone();

        a.output = Some(link.output.clone());

        let argv = a.build_args();
        self.run(&self.cc.exe, &argv, print_only)
    }

    fn run(&self, exe: &Path, args: &[OsString], print_only: bool) -> Result<(), DriverError> {
        if print_only {
            // mode --print : ne pas exécuter, juste tracer
            // ici, pas de logger imposé: stdout suffit.
            print!("{}",
                format_command(exe, args)
            );
            println!();
            return Ok(());
        }

        let out = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        if !out.status.success() {
            return Err(DriverError::Failed {
                exe: exe.to_path_buf(),
                args: args.to_vec(),
                code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            });
        }

        Ok(())
    }
}

fn format_command(exe: &Path, args: &[OsString]) -> String {
    // formatting safe (best-effort)
    let mut s = String::new();
    s.push_str(&exe.display().to_string());
    for a in args {
        s.push(' ');
        let t = a.to_string_lossy();
        if t.contains(' ') || t.contains('\t') {
            s.push('"');
            s.push_str(&t.replace('"', "\\\""));
            s.push('"');
        } else {
            s.push_str(&t);
        }
    }
    s
}
