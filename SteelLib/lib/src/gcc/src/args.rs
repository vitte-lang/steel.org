use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GccMode {
    Compile, // src.c -> obj.o
    Link,    // objs -> exe|dll|so
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CStd {
    C11,
    C17,
    C23,
    Gnu11,
    Gnu17,
    Gnu23,
}

impl CStd {
    pub fn as_flag(self) -> &'static str {
        match self {
            CStd::C11 => "-std=c11",
            CStd::C17 => "-std=c17",
            CStd::C23 => "-std=c23",
            CStd::Gnu11 => "-std=gnu11",
            CStd::Gnu17 => "-std=gnu17",
            CStd::Gnu23 => "-std=gnu23",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    // utile si tu pilotes mingw/msvc via clang etc.
    Default,
    Static,
}

#[derive(Debug, Clone)]
pub struct GccArgs {
    pub mode: GccMode,

    // Tooling nuance: clang accepte --target, gcc “pur” non (sauf wrappers).
    pub clang_like: bool,
    pub target: Option<String>,
    pub sysroot: Option<PathBuf>,

    pub c_std: Option<CStd>,

    pub debug: bool,          // -g
    pub opt_level: Option<u8>,// -O0..-O3
    pub lto: bool,            // -flto
    pub pic: bool,            // -fPIC
    pub pie: bool,            // -fPIE (compile) / -pie (link)

    pub warnings: Vec<String>, // "-Wall", "-Wextra", ...
    pub werror: bool,

    pub includes: Vec<PathBuf>, // -I
    pub isystem: Vec<PathBuf>,  // -isystem

    pub defines: Vec<(String, Option<String>)>, // -D K[=V]
    pub undefines: Vec<String>,                 // -U K

    // Compile deps
    pub depfile: Option<PathBuf>, // -MMD -MF <dep>

    // Inputs/outputs
    pub input: Option<PathBuf>,    // compile: .c ; link: unused
    pub output: Option<PathBuf>,   // -o <...>
    pub extra_cflags: Vec<OsString>,
    pub extra_ldflags: Vec<OsString>,

    // Link
    pub link_inputs: Vec<PathBuf>, // .o, .a, .so, ...
    pub lib_dirs: Vec<PathBuf>,    // -L
    pub libs: Vec<String>,         // -lfoo
    pub rpaths: Vec<PathBuf>,      // -Wl,-rpath,<path>

    pub runtime: Runtime,

    // Response file
    pub rsp_threshold_chars: usize, // si args "estimés" dépassent => écrire @file
}

impl Default for GccArgs {
    fn default() -> Self {
        Self {
            mode: GccMode::Compile,
            clang_like: false,
            target: None,
            sysroot: None,
            c_std: None,
            debug: false,
            opt_level: None,
            lto: false,
            pic: false,
            pie: false,
            warnings: vec![],
            werror: false,
            includes: vec![],
            isystem: vec![],
            defines: vec![],
            undefines: vec![],
            depfile: None,
            input: None,
            output: None,
            extra_cflags: vec![],
            extra_ldflags: vec![],
            link_inputs: vec![],
            lib_dirs: vec![],
            libs: vec![],
            rpaths: vec![],
            runtime: Runtime::Default,
            rsp_threshold_chars: 24_000, // safe-ish sur Windows
        }
    }
}

impl GccArgs {
    pub fn compile() -> Self {
        Self { mode: GccMode::Compile, ..Self::default() }
    }

    pub fn link() -> Self {
        Self { mode: GccMode::Link, ..Self::default() }
    }

    pub fn build_args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = Vec::new();

        // Mode
        match self.mode {
            GccMode::Compile => a.push(OsString::from("-c")),
            GccMode::Link => {}
        }

        // Toolchain selection options
        if self.clang_like {
            if let Some(t) = &self.target {
                a.push(OsString::from(format!("--target={}", t)));
            }
        }
        if let Some(sr) = &self.sysroot {
            a.push(OsString::from("--sysroot"));
            a.push(sr.as_os_str().to_os_string());
        }

        // Standard
        if let Some(std) = self.c_std {
            a.push(OsString::from(std.as_flag()));
        }

        // Debug/opt
        if self.debug {
            a.push(OsString::from("-g"));
        }
        if let Some(o) = self.opt_level {
            a.push(OsString::from(format!("-O{}", o)));
        }
        if self.lto {
            a.push(OsString::from("-flto"));
        }

        // PIC/PIE
        if self.pic {
            a.push(OsString::from("-fPIC"));
        }
        if self.pie {
            match self.mode {
                GccMode::Compile => a.push(OsString::from("-fPIE")),
                GccMode::Link => a.push(OsString::from("-pie")),
            }
        }

        // Warnings
        for w in &self.warnings {
            a.push(OsString::from(w));
        }
        if self.werror {
            a.push(OsString::from("-Werror"));
        }

        // Includes
        for p in &self.includes {
            a.push(OsString::from("-I"));
            a.push(p.as_os_str().to_os_string());
        }
        for p in &self.isystem {
            a.push(OsString::from("-isystem"));
            a.push(p.as_os_str().to_os_string());
        }

        // Defines
        for (k, v) in &self.defines {
            match v {
                Some(val) => a.push(OsString::from(format!("-D{}={}", k, val))),
                None => a.push(OsString::from(format!("-D{}", k))),
            }
        }
        for k in &self.undefines {
            a.push(OsString::from(format!("-U{}", k)));
        }

        // Depfile
        if self.mode == GccMode::Compile {
            if let Some(dep) = &self.depfile {
                a.push(OsString::from("-MMD"));
                a.push(OsString::from("-MF"));
                a.push(dep.as_os_str().to_os_string());
            }
        }

        // Extra flags
        a.extend(self.extra_cflags.iter().cloned());

        // Link section
        if self.mode == GccMode::Link {
            a.extend(self.extra_ldflags.iter().cloned());

            for p in &self.lib_dirs {
                a.push(OsString::from("-L"));
                a.push(p.as_os_str().to_os_string());
            }

            for rp in &self.rpaths {
                // gcc/clang: -Wl,-rpath,<path>
                // On pousse en une seule OsString pour éviter split ambigu.
                let mut s = OsString::from("-Wl,-rpath,");
                s.push(rp.as_os_str());
                a.push(s);
            }

            // Inputs deterministes
            // (si tu veux un build reproductible strict: trie)
            for p in &self.link_inputs {
                a.push(p.as_os_str().to_os_string());
            }
            for l in &self.libs {
                a.push(OsString::from(format!("-l{}", l)));
            }

            // runtime static (optionnel)
            if let Runtime::Static = self.runtime {
                // MinGW: -static / Linux: -static (selon tes besoins)
                a.push(OsString::from("-static"));
            }
        }

        // Output
        if let Some(out) = &self.output {
            a.push(OsString::from("-o"));
            a.push(out.as_os_str().to_os_string());
        }

        // Input (compile)
        if self.mode == GccMode::Compile {
            if let Some(inp) = &self.input {
                a.push(inp.as_os_str().to_os_string());
            }
        }

        a
    }

    /// Calcule une estimation de taille pour décider si on bascule en @rsp.
    fn estimate_chars(args: &[OsString]) -> usize {
        // Estimation naïve: somme des longueurs + espaces
        args.iter()
            .map(|s| s.to_string_lossy().len() + 1)
            .sum()
    }

    /// Écrit un response file si nécessaire, renvoie soit args, soit vec!["@file.rsp"].
    pub fn build_args_or_rsp(&self, rsp_path: &Path) -> std::io::Result<Vec<OsString>> {
        let args = self.build_args();
        let est = Self::estimate_chars(&args);

        if est < self.rsp_threshold_chars {
            return Ok(args);
        }

        // Écriture rsp: une option par ligne, quoting minimal.
        // GCC/Clang rsp supporte généralement:
        // - espaces => quoting "..."
        // - backslash windows => ok, mais mieux en raw.
        let mut out = String::new();
        for a in &args {
            let s = a.to_string_lossy();
            if s.contains(' ') || s.contains('\t') {
                out.push('"');
                // escape des guillemets internes
                out.push_str(&s.replace('"', "\\\""));
                out.push('"');
            } else {
                out.push_str(&s);
            }
            out.push('\n');
        }

        if let Some(parent) = rsp_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(rsp_path, out.as_bytes())?;

        Ok(vec![OsString::at_rsp(rsp_path)])
    }
}

/// Petit helper stable pour fabriquer "@path" proprement.
trait AtRsp {
    fn at_rsp(path: &Path) -> OsString;
}
impl AtRsp for OsString {
    fn at_rsp(path: &Path) -> OsString {
        let mut s = OsString::from("@");
        s.push(path.as_os_str());
        s
    }
}
