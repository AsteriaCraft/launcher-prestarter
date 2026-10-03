//! The launcher jar: embedded in the prestarter's own file, or a copy in the store (ADR 0001).

pub mod fetch;
pub mod inspect;
pub mod source;

pub use inspect::{JarError, JarInfo};
pub use source::JarSource;

/// Where the copy mode fetches the launcher when no signed policy says otherwise.
pub const DEFAULT_JAR_URL: &str = "https://launcher.asterium.pro/Asterium.jar";
