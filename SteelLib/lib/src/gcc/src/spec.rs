use std::path::PathBuf;

use crate::gcc::args::CStd;

/// Spécification C indépendante du compilateur
#[derive(Debug, Clone)]
pub struct CSpec {
    /// Standard C
    pub c_std: CStd,

    /// Profil logique Steel
    pub profile: BuildProfile,

    /// Options générales
    pub debug: bool,
    pub opt_level: u8,
    pub lto: bool,

    /// Warnings
    pub warnings: Vec<String>,
    pub werror: bool,

    /// Codegen
    pub pic: bool,
    pub pie: bool,

    /// Includes / defines globaux
    pub includes: Vec<PathBuf>,
    pub isystem: Vec<PathBuf>,
    pub defines: Vec<(String, Option<String>)>,
    pub undefines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Dev,
    Release,
    Custom,
}

impl CSpec {
    /// Profil par défaut Steel (dev)
    pub fn dev() -> Self {
        Self {
            c_std: CStd::C17,
            profile: BuildProfile::Dev,
            debug: true,
            opt_level: 0,
            lto: false,
            warnings: vec![
                "-Wall".into(),
                "-Wextra".into(),
            ],
            werror: false,
            pic: false,
            pie: false,
            includes: vec![],
            isystem: vec![],
            defines: vec![("DEBUG".into(), None)],
            undefines: vec![],
        }
    }

    /// Profil release canonique Steel
    pub fn release() -> Self {
        Self {
            c_std: CStd::C17,
            profile: BuildProfile::Release,
            debug: false,
            opt_level: 2,
            lto: true,
            warnings: vec![
                "-Wall".into(),
                "-Wextra".into(),
            ],
            werror: false,
            pic: true,
            pie: false,
            includes: vec![],
            isystem: vec![],
            defines: vec![("NDEBUG".into(), None)],
            undefines: vec!["DEBUG".into()],
        }
    }

    /// Appliquer des overrides venant de steelconf
    pub fn apply_overrides(&mut self, o: CSpecOverrides) {
        if let Some(std) = o.c_std {
            self.c_std = std;
        }
        if let Some(debug) = o.debug {
            self.debug = debug;
        }
        if let Some(opt) = o.opt_level {
            self.opt_level = opt;
        }
        if let Some(lto) = o.lto {
            self.lto = lto;
        }
        if let Some(werror) = o.werror {
            self.werror = werror;
        }
        if let Some(pic) = o.pic {
            self.pic = pic;
        }
        if let Some(pie) = o.pie {
            self.pie = pie;
        }

        self.warnings.extend(o.warnings);
        self.includes.extend(o.includes);
        self.isystem.extend(o.isystem);
        self.defines.extend(o.defines);
        self.undefines.extend(o.undefines);
    }
}

/// Overrides optionnels (issus du steelconf)
#[derive(Debug, Default, Clone)]
pub struct CSpecOverrides {
    pub c_std: Option<CStd>,
    pub debug: Option<bool>,
    pub opt_level: Option<u8>,
    pub lto: Option<bool>,
    pub werror: Option<bool>,
    pub pic: Option<bool>,
    pub pie: Option<bool>,

    pub warnings: Vec<String>,
    pub includes: Vec<PathBuf>,
    pub isystem: Vec<PathBuf>,
    pub defines: Vec<(String, Option<String>)>,
    pub undefines: Vec<String>,
}
