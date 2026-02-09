//! Capsule module entry.
//!
//! A “capsule” is Steel’s sandbox contract: a policy + runtime enforcement surface.
//!
//! Public surface:
//! - `policy`: data model + compilation/normalization helpers
//! - `sandbox`: enforcement façade (gated FS/ENV/NET/TIME/PROC + soft limits)
//!
//! The capsule module is intentionally std-only. OS-specific sandboxes (seccomp,
//! Landlock, AppContainer, job objects, etc.) can be integrated behind the
//! backend traits in `sandbox`.

pub mod policy;
pub mod sandbox;

pub use policy::{
    CapsulePolicy, CompiledPolicy, Decision, DefaultMode, EnvOp, FsOp, Limits, NetEndpoint, NetOp,
    PathPattern, PolicyError, PolicyPreset, ProcOp, StrPattern, TimeOp,
};

pub use sandbox::{
    Clock, HostClock, HostEnv, HostFs, HostProc, NullNet, Sandbox, SandboxBuilder, SandboxDenied,
    SandboxDomain, SandboxError, EnvBackend, FsBackend, NetBackend, ProcBackend,
};
