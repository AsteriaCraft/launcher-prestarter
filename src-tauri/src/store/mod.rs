//! The prestarter's own files: locations, `state.json`, the install lock and the logs (ADR 0005).

pub mod atomic;
pub mod lock;
pub mod logs;
pub mod paths;
pub mod state;

pub use lock::InstallLock;
pub use paths::StorePaths;
pub use state::State;
