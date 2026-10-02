//! Where the prestarter keeps its files (ADR 0005): its own store, not the shared, roaming `GravitLauncherStore`.
//!
//! ```text
//! <store>/
//!   state.json                      schema 1 (store::state)
//!   jre/liberica-25-<target>-<version>/
//!   launcher/Asterium.jar           only for the copy mode (AppImage, macOS)
//!   logs/prestarter-<1..5>.log      the last five runs
//!   logs/launcher-start.log         stdout/stderr of Gravit's wrapper JVM
//!   tmp/                            partial downloads, cleaned at start under the install lock
//!   .install.lock
//! ```

use std::path::{Path, PathBuf};

use crate::platform::Os;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePaths {
    root: PathBuf,
}

impl StorePaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn jre_dir(&self) -> PathBuf {
        self.root.join("jre")
    }

    pub fn jre_home(&self, dir_name: &str) -> PathBuf {
        self.jre_dir().join(dir_name)
    }

    pub fn launcher_dir(&self) -> PathBuf {
        self.root.join("launcher")
    }

    pub fn launcher_jar(&self) -> PathBuf {
        self.launcher_dir().join("Asterium.jar")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn launch_log(&self) -> PathBuf {
        self.logs_dir().join("launcher-start.log")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.root.join(".install.lock")
    }
}

/// The default store for `os`, from the user's data directories as `dirs` reports them:
/// Windows `%LOCALAPPDATA%\Asterium\Prestarter` (Local, so ~300 MB of Java does not roam with the profile),
/// Linux `${XDG_DATA_HOME:-~/.local/share}/asterium/prestarter`, macOS `~/Library/Application Support/Asterium/Prestarter`.
pub fn default_root(os: Os, local_data_dir: Option<PathBuf>, data_dir: Option<PathBuf>) -> Option<PathBuf> {
    match os {
        Os::Windows => local_data_dir.map(|d| d.join("Asterium").join("Prestarter")),
        Os::Linux => data_dir.map(|d| d.join("asterium").join("prestarter")),
        Os::MacOs => data_dir.map(|d| d.join("Asterium").join("Prestarter")),
    }
}

/// [`default_root`] for this machine.
pub fn default_root_here() -> Option<PathBuf> {
    default_root(crate::platform::Host::current().os, dirs::data_local_dir(), dirs::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roots_per_os() {
        let local = Some(PathBuf::from("C:/Users/p/AppData/Local"));
        let data = Some(PathBuf::from("/home/p/.local/share"));
        assert_eq!(
            default_root(Os::Windows, local.clone(), data.clone()),
            Some(PathBuf::from("C:/Users/p/AppData/Local/Asterium/Prestarter"))
        );
        assert_eq!(
            default_root(Os::Linux, local.clone(), data.clone()),
            Some(PathBuf::from("/home/p/.local/share/asterium/prestarter"))
        );
        let mac = Some(PathBuf::from("/Users/p/Library/Application Support"));
        assert_eq!(
            default_root(Os::MacOs, None, mac),
            Some(PathBuf::from("/Users/p/Library/Application Support/Asterium/Prestarter"))
        );
        assert_eq!(default_root(Os::Windows, None, data), None);
    }

    #[test]
    fn layout_under_the_root() {
        let paths = StorePaths::new(PathBuf::from("/s"));
        assert_eq!(
            paths.jre_home("liberica-25-linux-x64-25.0.4.1+1"),
            Path::new("/s/jre/liberica-25-linux-x64-25.0.4.1+1")
        );
        assert_eq!(paths.launcher_jar(), Path::new("/s/launcher/Asterium.jar"));
        assert_eq!(paths.launch_log(), Path::new("/s/logs/launcher-start.log"));
        assert_eq!(paths.state_file(), Path::new("/s/state.json"));
        assert_eq!(paths.lock_file(), Path::new("/s/.install.lock"));
        assert_eq!(paths.tmp_dir(), Path::new("/s/tmp"));
    }

    #[test]
    fn this_machine_has_a_default_root() {
        assert!(default_root_here().is_some());
    }
}
