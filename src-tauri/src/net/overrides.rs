//! The only four switches of the release binary (ADR 0008). Smoke tests run the shipped bytes against servers on
//! the loopback interface; nothing can point a player's prestarter at another host.
//!
//! | Variable | Effect | Accepted values |
//! |---|---|---|
//! | `ASTERIUM_PRESTARTER_JRE_API` | base URL of the Liberica API | `http(s)://127.0.0.1:<port>/…` or `http(s)://[::1]:<port>/…` |
//! | `ASTERIUM_PRESTARTER_LAUNCHER_URL` | URL of the launcher jar (copy mode) | the same |
//! | `ASTERIUM_PRESTARTER_STORE` | store directory | an absolute path |
//! | `ASTERIUM_PRESTARTER_NONINTERACTIVE` | OS dialogs go to the log instead of the screen | `1` or `0` |
//!
//! A value that breaks these rules is an error (exit code 2), never a silent fallback to the real address: a test
//! can never "pass" against production.

use std::ffi::OsString;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

use reqwest::Url;
use thiserror::Error;

pub const ENV_JRE_API: &str = "ASTERIUM_PRESTARTER_JRE_API";
pub const ENV_LAUNCHER_URL: &str = "ASTERIUM_PRESTARTER_LAUNCHER_URL";
pub const ENV_STORE: &str = "ASTERIUM_PRESTARTER_STORE";
pub const ENV_NONINTERACTIVE: &str = "ASTERIUM_PRESTARTER_NONINTERACTIVE";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    pub jre_api: Option<Url>,
    pub launcher_url: Option<Url>,
    pub store: Option<PathBuf>,
    pub noninteractive: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OverrideError {
    #[error("{name}: not a URL ({detail})")]
    NotUrl { name: &'static str, detail: String },
    #[error("{name}: only http and https are allowed, got {scheme}")]
    Scheme { name: &'static str, scheme: String },
    #[error("{name}: the host must be the literal loopback address 127.0.0.1 or [::1], got {host}")]
    NotLoopback { name: &'static str, host: String },
    #[error("{name}: an explicit port is required")]
    NoPort { name: &'static str },
    #[error("{name}: user names and passwords are not allowed")]
    Credentials { name: &'static str },
    #[error("{ENV_STORE}: must be an absolute path, got {0}")]
    RelativeStore(String),
    #[error("{name}: the value is not valid Unicode")]
    NotUnicode { name: &'static str },
    #[error("{ENV_NONINTERACTIVE}: expected 1 or 0, got {0}")]
    Flag(String),
}

impl Overrides {
    /// Reads the four variables through `lookup` (the environment snapshot taken at start).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Result<Self, OverrideError> {
        let text = |name: &'static str| -> Result<Option<String>, OverrideError> {
            match lookup(name) {
                None => Ok(None),
                Some(value) if value.is_empty() => Ok(None),
                Some(value) => value.into_string().map(Some).map_err(|_| OverrideError::NotUnicode { name }),
            }
        };
        let jre_api = text(ENV_JRE_API)?.map(|v| loopback_url(ENV_JRE_API, &v)).transpose()?;
        let launcher_url = text(ENV_LAUNCHER_URL)?.map(|v| loopback_url(ENV_LAUNCHER_URL, &v)).transpose()?;
        let store = match lookup(ENV_STORE).filter(|v| !v.is_empty()) {
            None => None,
            Some(value) => {
                let path = PathBuf::from(&value);
                if !path.is_absolute() {
                    return Err(OverrideError::RelativeStore(value.to_string_lossy().into_owned()));
                }
                Some(path)
            }
        };
        let noninteractive = match text(ENV_NONINTERACTIVE)?.as_deref() {
            None | Some("0") => false,
            Some("1") => true,
            Some(other) => return Err(OverrideError::Flag(other.to_owned())),
        };
        Ok(Self { jre_api, launcher_url, store, noninteractive })
    }

    /// Whether any override redirects network or storage: the window then shows a "test mode" badge.
    pub fn is_test_mode(&self) -> bool {
        self.jre_api.is_some() || self.launcher_url.is_some() || self.store.is_some()
    }

    /// One line per active override, for `WARN override …` in the log.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(url) = &self.jre_api {
            lines.push(format!("{ENV_JRE_API}={url}"));
        }
        if let Some(url) = &self.launcher_url {
            lines.push(format!("{ENV_LAUNCHER_URL}={url}"));
        }
        if let Some(path) = &self.store {
            lines.push(format!("{ENV_STORE}={}", path.display()));
        }
        if self.noninteractive {
            lines.push(format!("{ENV_NONINTERACTIVE}=1"));
        }
        lines
    }
}

fn loopback_url(name: &'static str, value: &str) -> Result<Url, OverrideError> {
    let url = Url::parse(value).map_err(|err| OverrideError::NotUrl { name, detail: err.to_string() })?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(OverrideError::Scheme { name, scheme: url.scheme().to_owned() });
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(OverrideError::Credentials { name });
    }
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip) == IpAddr::V4(Ipv4Addr::LOCALHOST),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip) == IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => false,
    };
    if !loopback {
        return Err(OverrideError::NotLoopback { name, host: url.host_str().unwrap_or("").to_owned() });
    }
    if url.port().is_none() {
        return Err(OverrideError::NoPort { name });
    }
    Ok(url)
}

/// Whether `url` points at the loopback interface (the only place plain `http` is accepted).
pub fn is_loopback(url: &Url) -> bool {
    matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip == Ipv4Addr::LOCALHOST)
        || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip == Ipv6Addr::LOCALHOST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn read(pairs: &[(&str, &str)]) -> Result<Overrides, OverrideError> {
        let map: HashMap<String, OsString> = pairs.iter().map(|(k, v)| (k.to_string(), OsString::from(v))).collect();
        Overrides::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn nothing_set_is_production() {
        let o = read(&[]).unwrap();
        assert_eq!(o, Overrides::default());
        assert!(!o.is_test_mode());
        assert!(o.describe().is_empty());
    }

    #[test]
    fn accepts_loopback_with_port() {
        let o = read(&[
            (ENV_JRE_API, "http://127.0.0.1:8080/v1/liberica/releases"),
            (ENV_LAUNCHER_URL, "https://[::1]:8443/Asterium.jar"),
        ])
        .unwrap();
        assert_eq!(o.jre_api.unwrap().as_str(), "http://127.0.0.1:8080/v1/liberica/releases");
        assert_eq!(o.launcher_url.unwrap().port(), Some(8443));
    }

    #[test]
    fn refuses_everything_else() {
        let bad = [
            "http://localhost:8080/",
            "http://127.0.0.1.nip.io:8080/",
            "http://10.0.0.1:8080/",
            "http://127.0.0.2:8080/",
            "http://user@127.0.0.1@evil:8080/",
            "http://user:pw@127.0.0.1:8080/",
            "http://127.0.0.1/",
            "http://127.0.0.1:/",
            "ftp://127.0.0.1:21/",
            "file:///etc/passwd",
            "launcher.asterium.pro",
            "https://launcher.asterium.pro/Asterium.jar",
        ];
        for value in bad {
            assert!(read(&[(ENV_LAUNCHER_URL, value)]).is_err(), "{value} must be refused");
            assert!(read(&[(ENV_JRE_API, value)]).is_err(), "{value} must be refused");
        }
    }

    #[test]
    fn store_must_be_absolute() {
        assert!(matches!(read(&[(ENV_STORE, "relative/dir")]), Err(OverrideError::RelativeStore(_))));
        let absolute = if cfg!(windows) { r"C:\asterium-test" } else { "/tmp/asterium-test" };
        let o = read(&[(ENV_STORE, absolute)]).unwrap();
        assert_eq!(o.store.as_deref(), Some(std::path::Path::new(absolute)));
        assert!(o.is_test_mode());
    }

    #[test]
    fn noninteractive_flag() {
        assert!(read(&[(ENV_NONINTERACTIVE, "1")]).unwrap().noninteractive);
        assert!(!read(&[(ENV_NONINTERACTIVE, "0")]).unwrap().noninteractive);
        assert!(read(&[(ENV_NONINTERACTIVE, "yes")]).is_err());
        assert!(!read(&[(ENV_NONINTERACTIVE, "1")]).unwrap().is_test_mode(), "it redirects nothing");
    }

    #[test]
    fn describes_each_override() {
        let o = read(&[(ENV_JRE_API, "http://127.0.0.1:1/"), (ENV_NONINTERACTIVE, "1")]).unwrap();
        assert_eq!(o.describe(), [format!("{ENV_JRE_API}=http://127.0.0.1:1/"), format!("{ENV_NONINTERACTIVE}=1")]);
    }

    #[test]
    fn loopback_check() {
        assert!(is_loopback(&Url::parse("http://127.0.0.1:1/").unwrap()));
        assert!(is_loopback(&Url::parse("http://[::1]:1/").unwrap()));
        assert!(!is_loopback(&Url::parse("https://launcher.asterium.pro/").unwrap()));
    }
}
