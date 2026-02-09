//! GCC / Clang driver integration for Steel (in-tree module).

pub mod detect;
pub mod args;
pub mod driver;

pub use detect::{detect_cc, CcKind, CcTool, DetectError};
pub use args::{GccArgs, GccMode, CStd};
pub use driver::{GccDriver, CBuildConfig, CompileUnit, LinkUnit, DriverError};
