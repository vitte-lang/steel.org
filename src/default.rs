// /Users/vincent/Documents/Github/steel/src/default.rs
//! default — apply defaults to workspace / resolved config (std-only)
//!
//! This module centralizes *defaulting logic* so it stays deterministic and auditable.
//! It is used by the resolver layer to fill missing values (profile, target, dirs,
//! toolchain entries), and to normalize / sanitize optional settings.
//!
//! The rule: defaults are applied only when a field is missing or empty.
//!
//! Scope in this repository:
//! - `build_muf::BuildMufOptions` defaults (if desired by higher layers)
//! - `build_muf::ResolvedConfig` default fill-in helpers
//! - low-level helpers: profile, target triple, paths, toolchain, variables
//!
//! Non-goals:
//! - parsing steelconf syntax
//! - dependency resolution

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use crate::build_muf;

/// Defaults policy knobs.
#[derive(Debug, Clone)]
pub struct DefaultPolicy {
    /// Default profile when missing.
    pub default_profile: String,
    /// Default target triple when missing.
    pub default_target: String,

    /// Default relative build/dist/cache dir names.
    pub build_dir_name: String,
    pub dist_dir_name: String,
    pub cache_dir_name: String,

    /// Fill toolchain tools from env (CC/CXX/AR/LD/RUSTC).
    pub fill_toolchain_from_env: bool,

    /// If true, add common resolved vars into cfg.vars.
    pub fill_common_vars: bool,
}

impl Default for DefaultPolicy {
    fn default() -> Self {
        Self {
            default_profile: env::var("MUFFIN_PROFILE").unwrap_or_else(|_| "debug".to_string()),
            default_target: env::var("MUFFIN_TARGET").unwrap_or_else(|_| host_triple_best_effort()),
            build_dir_name: "build".to_string(),
            dist_dir_name: "dist".to_string(),
            cache_dir_name: ".steel-cache".to_string(),
            fill_toolchain_from_env: true,
            fill_common_vars: true,
        }
    }
}

/// Apply defaults to build options (safe and conservative).
pub fn apply_defaults_to_options(o: &mut build_muf::BuildMufOptions, policy: &DefaultPolicy) {
    // profile/target defaults are resolved later in build_muf, but we can pre-fill.
    if o.profile.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        o.profile = Some(policy.default_profile.clone());
    }
    if o.target.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        o.target = Some(policy.default_target.clone());
    }

    if o.max_depth == 0 {
        o.max_depth = 16;
    }
}

/// Apply defaults to a resolved config *in-place*.
///
/// This is idempotent: calling it multiple times does not change the config after first application.
pub fn apply_defaults_to_resolved(cfg: &mut build_muf::ResolvedConfig, policy: &DefaultPolicy) {
    if cfg.schema_version == 0 {
        cfg.schema_version = 1;
    }

    if cfg.profile.trim().is_empty() {
        cfg.profile = policy.default_profile.clone();
    }
    if cfg.target.trim().is_empty() {
        cfg.target = policy.default_target.clone();
    }

    // Paths: if any are empty, fill them under root using configured names.
    cfg.paths.build_dir = default_path(&cfg.project_root, &cfg.paths.build_dir, &policy.build_dir_name);
    cfg.paths.dist_dir = default_path(&cfg.project_root, &cfg.paths.dist_dir, &policy.dist_dir_name);
    cfg.paths.cache_dir = default_path(&cfg.project_root, &cfg.paths.cache_dir, &policy.cache_dir_name);

    // Toolchain: fill missing entries from env if requested.
    if policy.fill_toolchain_from_env {
        fill_toolchain_from_env(&mut cfg.toolchain);
    }

    // Vars: ensure common vars exist.
    if policy.fill_common_vars {
        ensure_common_vars(&cfg.project_root, &cfg.steelfile_path, &cfg.profile, &cfg.target, &mut cfg.vars);
    }

    // Map toolchain overrides into env-style vars for downstream execution.
    sync_toolchain_env_vars(&cfg.toolchain, &mut cfg.vars);
}

/// Ensure a config has at least these keys:
/// - steel.root, steel.file, steel.profile, steel.target
pub fn ensure_common_vars(
    root: &Path,
    steelfile: &Path,
    profile: &str,
    target: &str,
    vars: &mut BTreeMap<String, String>,
) {
    vars.entry("steel.root".to_string())
        .or_insert_with(|| root.to_string_lossy().to_string());
    vars.entry("steel.file".to_string())
        .or_insert_with(|| steelfile.to_string_lossy().to_string());
    vars.entry("steel.profile".to_string())
        .or_insert_with(|| profile.to_string());
    vars.entry("steel.target".to_string())
        .or_insert_with(|| target.to_string());
}

/// Fill toolchain tools from env vars if missing/empty.
pub fn fill_toolchain_from_env(tc: &mut build_muf::ToolchainInfo) {
    if tc.cc.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.cc = env::var("CC").ok();
    }
    if tc.cxx.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.cxx = env::var("CXX").ok();
    }
    if tc.ar.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.ar = env::var("AR").ok();
    }
    if tc.ld.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.ld = env::var("LD").ok();
    }
    if tc.rustc.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.rustc = env::var("RUSTC").ok().or_else(|| Some("rustc".to_string()));
    }
    if tc.python.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.python = env::var("PYTHON").ok();
    }
    if tc.ocaml.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.ocaml = env::var("OCAMLPATH").ok();
    }
    if tc.ghc.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        tc.ghc = env::var("GHC_PACKAGE_PATH").ok();
    }
}

fn sync_toolchain_env_vars(tc: &build_muf::ToolchainInfo, vars: &mut BTreeMap<String, String>) {
    if let Some(v) = &tc.python {
        vars.entry("PYTHON".to_string()).or_insert_with(|| v.clone());
    }
    if let Some(v) = &tc.ocaml {
        vars.entry("OCAMLPATH".to_string()).or_insert_with(|| v.clone());
    }
    if let Some(v) = &tc.ghc {
        vars.entry("GHC_PACKAGE_PATH".to_string()).or_insert_with(|| v.clone());
    }
}

/// Compute a default path under root if the given `p` is empty.
/// If `p` is absolute, it is kept.
/// If `p` is relative, it is joined under root.
fn default_path(root: &Path, p: &Path, name: &str) -> PathBuf {
    if p.as_os_str().is_empty() {
        return root.join(name);
    }
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Best-effort host triple (std-only).
pub fn host_triple_best_effort() -> String {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    let triple = match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => "unknown-unknown-unknown",
    };

    triple.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_empty_paths_and_vars() {
        let mut cfg = build_muf::generate_default_mcfg("/tmp/project");
        cfg.profile = "".to_string();
        cfg.target = "".to_string();
        cfg.paths.build_dir = PathBuf::new();
        cfg.paths.dist_dir = PathBuf::new();
        cfg.paths.cache_dir = PathBuf::new();
        cfg.vars.clear();

        let policy = DefaultPolicy::default();
        apply_defaults_to_resolved(&mut cfg, &policy);

        assert!(!cfg.profile.trim().is_empty());
        assert!(!cfg.target.trim().is_empty());
        assert!(cfg.vars.contains_key("steel.root"));
        assert!(cfg.vars.contains_key("steel.file"));
        assert!(cfg.paths.build_dir.to_string_lossy().contains("build"));
    }
}
