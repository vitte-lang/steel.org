// C:\Users\gogin\Documents\GitHub\steel\SteelLib\lib\src\runner\sandbox.rs

//! Sandbox layer (MVP).
//!
//! Goal: provide a uniform "policy gate" for executing tools.
//! This module is OS-agnostic and currently implements:
//! - a policy model (fs/net/time/process) used by the runner
//! - a best-effort preflight validation (paths, allowlists)
//!
//! Notes:
//! - True isolation is platform-specific (Windows Job Objects/AppContainer,
//!   Linux namespaces/seccomp, macOS sandbox-exec / seatbelt / entitlements).
//! - This file intentionally does NOT attempt full isolation yet.
//! - The immediate value is: a stable config surface + consistent checks +
//!   wiring points for future OS backends.

use crate::error::SteelError;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// High-level sandbox policy.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// If true, sandbox is enabled (otherwise policy is ignored).
    pub enabled: bool,

    /// Allowed filesystem read roots.
    pub fs_read_roots: Vec<PathBuf>,

    /// Allowed filesystem write roots.
    pub fs_write_roots: Vec<PathBuf>,

    /// Allow network access.
    pub allow_net: bool,

    /// Allow reading time (wallclock, monotonic).
    pub allow_time: bool,

    /// Allow spawning child processes (usually required for tool execution).
    pub allow_process: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            fs_read_roots: Vec::new(),
            fs_write_roots: Vec::new(),
            allow_net: false,
            allow_time: true,
            allow_process: true,
        }
    }
}

/// Execution context for sandbox checks.
#[derive(Debug, Clone)]
pub struct SandboxCtx {
    pub root: PathBuf,
    pub target_dir: PathBuf,
}

impl SandboxCtx {
    pub fn new(root: impl Into<PathBuf>, target_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            target_dir: target_dir.into(),
        }
    }
}

/// Sandbox "handle".
///
/// In MVP it only runs checks. Later it can host OS-specific backend state.
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub policy: SandboxPolicy,
    pub ctx: SandboxCtx,
}

impl Sandbox {
    pub fn new(policy: SandboxPolicy, ctx: SandboxCtx) -> Self {
        Self { policy, ctx }
    }

    /// Apply defaults based on workspace conventions.
    ///
    /// - read: workspace root
    /// - write: target dir
    pub fn with_default_roots(mut self) -> Self {
        if self.policy.fs_read_roots.is_empty() {
            self.policy.fs_read_roots.push(self.ctx.root.clone());
        }
        if self.policy.fs_write_roots.is_empty() {
            self.policy.fs_write_roots.push(self.ctx.target_dir.clone());
        }
        self
    }

    /// Best-effort validation of the policy.
    ///
    /// This does not enforce OS-level restrictions; it ensures the policy makes sense
    /// and that expected directories exist / can be created.
    pub fn preflight(&self) -> Result<(), SteelError> {
        if !self.policy.enabled {
            return Ok(());
        }

        // Normalize / dedup roots in a deterministic way.
        let read = dedup_roots(&self.policy.fs_read_roots);
        let write = dedup_roots(&self.policy.fs_write_roots);

        if read.is_empty() {
            return Err(SteelError::ValidationFailed(
                "sandbox enabled but fs_read_roots is empty".into(),
            ));
        }
        if write.is_empty() {
            return Err(SteelError::ValidationFailed(
                "sandbox enabled but fs_write_roots is empty".into(),
            ));
        }

        // Ensure all roots are absolute or workspace-relative (MVP rule).
        for p in read.iter().chain(write.iter()) {
            if p.as_os_str().is_empty() {
                return Err(SteelError::ValidationFailed(
                    "sandbox root path is empty".into(),
                ));
            }
        }

        // Ensure write roots exist or can be created.
        for w in &write {
            if !w.exists() {
                std::fs::create_dir_all(w).map_err(|e| {
                    SteelError::ExecutionFailed(format!(
                        "sandbox cannot create write root '{}': {}",
                        w.display(),
                        e
                    ))
                })?;
            }
        }

        Ok(())
    }

    /// Check if a read path is allowed by the policy.
    pub fn can_read(&self, path: &Path) -> bool {
        if !self.policy.enabled {
            return true;
        }
        is_under_any_root(path, &self.policy.fs_read_roots)
            || is_under_any_root(path, &self.policy.fs_write_roots)
    }

    /// Check if a write path is allowed by the policy.
    pub fn can_write(&self, path: &Path) -> bool {
        if !self.policy.enabled {
            return true;
        }
        is_under_any_root(path, &self.policy.fs_write_roots)
    }

    /// Enforce a read access check (returns SteelError).
    pub fn ensure_read(&self, path: &Path) -> Result<(), SteelError> {
        if self.can_read(path) {
            Ok(())
        } else {
            Err(SteelError::ExecutionFailed(format!(
                "sandbox deny read: {}",
                path.display()
            )))
        }
    }

    /// Enforce a write access check (returns SteelError).
    pub fn ensure_write(&self, path: &Path) -> Result<(), SteelError> {
        if self.can_write(path) {
            Ok(())
        } else {
            Err(SteelError::ExecutionFailed(format!(
                "sandbox deny write: {}",
                path.display()
            )))
        }
    }
}

// --- helpers ----------------------------------------------------------------

fn dedup_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut set = BTreeSet::new();
    for r in roots {
        set.insert(r.to_string_lossy().to_string());
    }
    set.into_iter().map(PathBuf::from).collect()
}

fn is_under_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    // MVP: string-prefix check on normalized-ish paths.
    // For strict behavior, use canonicalize with careful symlink policy.
    let p = path.to_string_lossy().replace('\\', "/");
    for r in roots {
        let rr = r.to_string_lossy().replace('\\', "/");
        if p == rr || p.starts_with(&(rr.clone() + "/")) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_allows_paths_under_roots() {
        let ctx = SandboxCtx::new("C:/repo", "C:/repo/target");
        let mut pol = SandboxPolicy::default();
        pol.enabled = true;
        pol.fs_read_roots = vec![PathBuf::from("C:/repo")];
        pol.fs_write_roots = vec![PathBuf::from("C:/repo/target")];

        let sb = Sandbox::new(pol, ctx);

        assert!(sb.can_read(Path::new("C:/repo/src/main.c")));
        assert!(sb.can_write(Path::new("C:/repo/target/out.exe")));
        assert!(!sb.can_write(Path::new("C:/repo/src/main.c")));
    }
}
