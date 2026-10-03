//! Errors the player sees: a sentence (i18n key and parameters), whether "Try again" makes sense, the process exit
//! code (ADR 0006), and details for the log and support.

use serde::Serialize;

use crate::i18n::{self, Lang};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Store,
    JreDownload,
    JreInstall,
    NoSpace,
    JarDownload,
    JarCorrupt,
    EmbeddedCorrupt,
    MissingLibs,
    LaunchFailed,
    /// Windows: the jar lies under a name the ANSI code page cannot write, so Java would get `?` instead. Moving the
    /// file to another folder helps.
    PathEncoding,
    /// Windows: the prestarter's own store (the JRE) lies under such a name - it is in the user profile, so usually
    /// the Windows user name has those letters. Moving the file cannot help; only the code page (or UTF-8) can.
    StorePathEncoding,
    WrapperTooOld,
    Internal,
}

impl ErrorKind {
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorKind::JreDownload | ErrorKind::JreInstall | ErrorKind::NoSpace => 3,
            ErrorKind::JarDownload | ErrorKind::JarCorrupt | ErrorKind::EmbeddedCorrupt => 4,
            ErrorKind::MissingLibs => 5,
            ErrorKind::LaunchFailed | ErrorKind::PathEncoding | ErrorKind::StorePathEncoding => 6,
            ErrorKind::WrapperTooOld => 7,
            ErrorKind::Store | ErrorKind::Internal => 10,
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            ErrorKind::Store => "error.store",
            ErrorKind::JreDownload => "error.jreDownload",
            ErrorKind::JreInstall => "error.jreInstall",
            ErrorKind::NoSpace => "error.noSpace",
            ErrorKind::JarDownload => "error.jarDownload",
            ErrorKind::JarCorrupt => "error.jarCorrupt",
            ErrorKind::EmbeddedCorrupt => "error.embeddedCorrupt",
            ErrorKind::MissingLibs => "error.missingLibs",
            ErrorKind::LaunchFailed => "error.launchFailed",
            ErrorKind::PathEncoding => "error.pathEncoding",
            ErrorKind::StorePathEncoding => "error.storePathEncoding",
            ErrorKind::WrapperTooOld => "error.wrapperTooOld",
            ErrorKind::Internal => "error.internal",
        }
    }

    /// Whether "Try again" can help (a network hiccup can pass; a damaged file, an old wrapper or a folder name Java
    /// cannot read cannot).
    pub fn retryable(self) -> bool {
        !matches!(
            self,
            ErrorKind::EmbeddedCorrupt
                | ErrorKind::WrapperTooOld
                | ErrorKind::PathEncoding
                | ErrorKind::StorePathEncoding
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowError {
    pub kind: ErrorKind,
    /// Technical detail for the log (not translated).
    pub detail: String,
    /// Message parameters (`needed`, `path`, `libraries`).
    pub params: Vec<(String, String)>,
    /// A command to copy (Linux packages).
    pub command: Option<String>,
    /// The end of `launcher-start.log` when the launcher did not start.
    pub log_tail: Option<String>,
    /// A page to open (the download page for an old or damaged wrapper).
    pub link: Option<String>,
}

impl FlowError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, detail: detail.into(), params: Vec::new(), command: None, log_tail: None, link: None }
    }

    pub fn param(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params.push((name.to_owned(), value.into()));
        self
    }

    pub fn exit_code(&self) -> i32 {
        self.kind.exit_code()
    }

    /// The sentence for the player, with the error code (for support).
    pub fn message(&self, lang: Lang) -> String {
        let params: Vec<(&str, &str)> = self.params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let key = match (self.kind, &self.command) {
            (ErrorKind::MissingLibs, None) => "error.missingLibsUnknown",
            (kind, _) => kind.message_key(),
        };
        let mut text = i18n::t(lang, key, &params);
        if let Some(command) = &self.command {
            text.push_str("\n\n");
            text.push_str(command);
        }
        text.push_str("\n\n");
        text.push_str(&i18n::t(lang, "error.code", &[("code", &self.exit_code().to_string())]));
        text
    }
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} (exit {}): {}", self.kind, self.exit_code(), self.detail)
    }
}

impl std::error::Error for FlowError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_adr() {
        assert_eq!(ErrorKind::JreDownload.exit_code(), 3);
        assert_eq!(ErrorKind::NoSpace.exit_code(), 3);
        assert_eq!(ErrorKind::JarDownload.exit_code(), 4);
        assert_eq!(ErrorKind::EmbeddedCorrupt.exit_code(), 4);
        assert_eq!(ErrorKind::MissingLibs.exit_code(), 5);
        assert_eq!(ErrorKind::LaunchFailed.exit_code(), 6);
        assert_eq!(ErrorKind::PathEncoding.exit_code(), 6);
        assert_eq!(ErrorKind::StorePathEncoding.exit_code(), 6);
        assert_eq!(ErrorKind::WrapperTooOld.exit_code(), 7);
        assert_eq!(ErrorKind::Internal.exit_code(), 10);
    }

    #[test]
    fn messages_are_translated_and_carry_the_code() {
        let err = FlowError::new(ErrorKind::NoSpace, "x").param("needed", "420 МБ").param("path", "C:\\");
        let text = err.message(Lang::Uk);
        assert!(text.starts_with("Не вистачає місця на диску: потрібно ще 420 МБ (C:\\)."), "{text}");
        assert!(text.ends_with("Код помилки: 3"));
        let libs = FlowError::new(ErrorKind::MissingLibs, "").param("libraries", "libgtk-3.so.0");
        let unknown_distro = libs.message(Lang::En);
        assert!(
            unknown_distro.starts_with("The launcher needs system libraries that are missing: libgtk-3.so.0."),
            "{unknown_distro}"
        );
        let libs = FlowError { command: Some("sudo apt install libgtk-3-0".into()), ..libs };
        assert!(libs.message(Lang::En).contains(":\n\nsudo apt install libgtk-3-0\n\n"));
    }

    #[test]
    fn retry_only_where_it_can_help() {
        assert!(ErrorKind::JarDownload.retryable());
        assert!(!ErrorKind::EmbeddedCorrupt.retryable());
        assert!(!ErrorKind::WrapperTooOld.retryable());
        assert!(!ErrorKind::PathEncoding.retryable());
        assert!(!ErrorKind::StorePathEncoding.retryable());
    }

    #[test]
    fn a_path_java_cannot_read_names_the_path_and_the_way_out() {
        let err = FlowError::new(ErrorKind::PathEncoding, "x").param("path", r"D:\Ігри\Asterium.exe");
        let text = err.message(Lang::En);
        assert!(text.contains(r"D:\Ігри\Asterium.exe"), "{text}");
        assert!(text.contains(r"C:\Games\Asterium"), "{text}");
        assert!(text.ends_with("Error code: 6"), "{text}");
    }

    /// The store lies in the user profile, so moving Asterium cannot help: in every language the sentence names the
    /// folder and the two settings that do help (the code page, or UTF-8), and never sends the player to C:\Games.
    #[test]
    fn a_store_java_cannot_read_never_says_move_asterium() {
        let store = r"C:\Users\Максим\AppData\Local\Asterium\Prestarter\jre";
        for lang in crate::i18n::ALL {
            let text = FlowError::new(ErrorKind::StorePathEncoding, "x").param("path", store).message(lang);
            assert!(text.contains(store), "{}: {text}", lang.code());
            assert!(!text.contains(r"C:\Games"), "{}: {text}", lang.code());
            assert!(text.contains("UTF-8"), "{}: {text}", lang.code());
            let code = i18n::t(lang, "error.code", &[("code", "6")]);
            assert!(text.ends_with(&code), "{}: {text}", lang.code());
        }
        let en = FlowError::new(ErrorKind::StorePathEncoding, "x").param("path", store).message(Lang::En);
        assert!(en.contains("Moving Asterium to another folder will not help"), "{en}");
    }
}
