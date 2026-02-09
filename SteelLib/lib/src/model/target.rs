// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\model\target.rs

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
};

/// Target triple (loosely validated string).
///
/// Examples:
/// - "x86_64-unknown-linux-gnu"
/// - "x86_64-pc-windows-msvc"
/// - "aarch64-apple-darwin"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TargetTriple(pub String);

impl TargetTriple {
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetTriple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Operating system (high-level classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Windows,
    Linux,
    Macos,
    Freebsd,
    Openbsd,
    Netbsd,
    Solaris,
    Illumos,
    Android,
    Ios,
    Wasi,
    Other,
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Os::Windows => f.write_str("windows"),
            Os::Linux => f.write_str("linux"),
            Os::Macos => f.write_str("macos"),
            Os::Freebsd => f.write_str("freebsd"),
            Os::Openbsd => f.write_str("openbsd"),
            Os::Netbsd => f.write_str("netbsd"),
            Os::Solaris => f.write_str("solaris"),
            Os::Illumos => f.write_str("illumos"),
            Os::Android => f.write_str("android"),
            Os::Ios => f.write_str("ios"),
            Os::Wasi => f.write_str("wasi"),
            Os::Other => f.write_str("other"),
        }
    }
}

/// CPU architecture (high-level classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Riscv64,
    Powerpc64,
    S390x,
    Wasm32,
    Wasm64,
    Other,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::X86 => f.write_str("x86"),
            Arch::X86_64 => f.write_str("x86_64"),
            Arch::Arm => f.write_str("arm"),
            Arch::Aarch64 => f.write_str("aarch64"),
            Arch::Riscv64 => f.write_str("riscv64"),
            Arch::Powerpc64 => f.write_str("powerpc64"),
            Arch::S390x => f.write_str("s390x"),
            Arch::Wasm32 => f.write_str("wasm32"),
            Arch::Wasm64 => f.write_str("wasm64"),
            Arch::Other => f.write_str("other"),
        }
    }
}

/// Target environment / ABI family (coarse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abi {
    Gnu,
    Musl,
    Msvc,
    Android,
    Eabi,
    Eabihf,
    Macos,
    Ios,
    Wasip1,
    Unknown,
}

impl fmt::Display for Abi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Abi::Gnu => f.write_str("gnu"),
            Abi::Musl => f.write_str("musl"),
            Abi::Msvc => f.write_str("msvc"),
            Abi::Android => f.write_str("android"),
            Abi::Eabi => f.write_str("eabi"),
            Abi::Eabihf => f.write_str("eabihf"),
            Abi::Macos => f.write_str("macos"),
            Abi::Ios => f.write_str("ios"),
            Abi::Wasip1 => f.write_str("wasip1"),
            Abi::Unknown => f.write_str("unknown"),
        }
    }
}

/// A resolved target descriptor.
///
/// This is what config resolution should produce (stable + serializable),
/// and what downstream build execution consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// Logical target name (stable).
    pub name: String,

    /// Full triple string (canonical identity).
    pub triple: TargetTriple,

    /// Optional coarse decomposition for quick filters in tooling/CI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<Os>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<Arch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi: Option<Abi>,

    /// Selected profile name (debug/release/custom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Deterministic target-scoped variables (e.g. CC, CFLAGS, SYSROOT, etc.)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,

    /// Deterministic metadata (e.g. host/target flags, discovery notes).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

impl Target {
    pub fn new(name: impl Into<String>, triple: impl Into<String>) -> Self {
        let triple = TargetTriple::new(triple);
        Self {
            name: name.into(),
            os: infer_os(triple.as_str()),
            arch: infer_arch(triple.as_str()),
            abi: infer_abi(triple.as_str()),
            triple,
            profile: None,
            vars: BTreeMap::new(),
            meta: BTreeMap::new(),
        }
    }

    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn with_var(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.vars.insert(k.into(), v.into());
        self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.meta.insert(k.into(), v.into());
        self
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prof = self.profile.as_deref().unwrap_or("-");
        write!(f, "Target{{name={}, triple={}, profile={}}}", self.name, self.triple, prof)
    }
}

// --- Best-effort inference helpers (do not hard-fail) ------------------------

fn infer_os(triple: &str) -> Option<Os> {
    let t = triple.to_ascii_lowercase();
    Some(if t.contains("windows") || t.contains("pc-windows") || t.contains("msvc") {
        Os::Windows
    } else if t.contains("apple-darwin") || t.contains("darwin") || t.contains("macos") {
        Os::Macos
    } else if t.contains("linux") {
        Os::Linux
    } else if t.contains("freebsd") {
        Os::Freebsd
    } else if t.contains("openbsd") {
        Os::Openbsd
    } else if t.contains("netbsd") {
        Os::Netbsd
    } else if t.contains("solaris") {
        Os::Solaris
    } else if t.contains("illumos") {
        Os::Illumos
    } else if t.contains("android") {
        Os::Android
    } else if t.contains("ios") {
        Os::Ios
    } else if t.contains("wasi") {
        Os::Wasi
    } else {
        return None;
    })
}

fn infer_arch(triple: &str) -> Option<Arch> {
    let t = triple.to_ascii_lowercase();
    Some(if t.starts_with("x86_64") || t.starts_with("amd64") {
        Arch::X86_64
    } else if t.starts_with("i386") || t.starts_with("i686") || t.starts_with("x86") {
        Arch::X86
    } else if t.starts_with("aarch64") || t.starts_with("arm64") {
        Arch::Aarch64
    } else if t.starts_with("arm") {
        Arch::Arm
    } else if t.starts_with("riscv64") {
        Arch::Riscv64
    } else if t.starts_with("powerpc64") || t.starts_with("ppc64") {
        Arch::Powerpc64
    } else if t.starts_with("s390x") {
        Arch::S390x
    } else if t.starts_with("wasm32") {
        Arch::Wasm32
    } else if t.starts_with("wasm64") {
        Arch::Wasm64
    } else {
        return None;
    })
}

fn infer_abi(triple: &str) -> Option<Abi> {
    let t = triple.to_ascii_lowercase();
    Some(if t.contains("msvc") {
        Abi::Msvc
    } else if t.contains("musl") {
        Abi::Musl
    } else if t.contains("gnu") {
        Abi::Gnu
    } else if t.contains("android") {
        Abi::Android
    } else if t.contains("eabihf") {
        Abi::Eabihf
    } else if t.contains("eabi") {
        Abi::Eabi
    } else if t.contains("apple-darwin") || t.contains("darwin") || t.contains("macos") {
        Abi::Macos
    } else if t.contains("ios") {
        Abi::Ios
    } else if t.contains("wasi") {
        Abi::Wasip1
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_infers_components() {
        let t = Target::new("host", "x86_64-pc-windows-msvc");
        assert_eq!(t.os, Some(Os::Windows));
        assert_eq!(t.arch, Some(Arch::X86_64));
        assert_eq!(t.abi, Some(Abi::Msvc));
    }

    #[test]
    fn unknown_triple_is_ok() {
        let t = Target::new("weird", "foo-bar-baz");
        assert!(t.os.is_none());
        assert!(t.arch.is_none());
        assert!(t.abi.is_none());
        assert_eq!(t.triple.as_str(), "foo-bar-baz");
    }
}
