//! HTTP clients (reqwest, blocking, rustls with the ring provider). Two trust settings (ADR 0001, 0007):
//!
//! - **Liberica** (API and JRE archives): the OS certificate store through rustls-platform-verifier, so a TLS proxy
//!   whose CA the player installed still works; the archive is checked against the API's sha1 and size anyway.
//! - **Launcher host** (the jar copy and the signed policy): Mozilla's root certificates only (webpki-root-certs).
//!   A CA added to the OS cannot hand the prestarter a different jar; the launcher JVM trusts the same set (the JRE's
//!   `cacerts`), so this does not lock out anyone the launcher would let in. Redirects must stay on the same origin.
//!
//! Plain `http` is accepted only for loopback test servers (ADR 0008).

use std::sync::Once;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use reqwest::{Certificate, Url};
use thiserror::Error;

pub const USER_AGENT: &str = concat!("Asterium-Prestarter/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Per read for bodies (reqwest blocking applies it to every read), total for headers.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub enum Roots {
    /// The operating system's trust store.
    Platform,
    /// Mozilla's root certificates compiled into the binary.
    Mozilla,
    /// Exactly these roots (integration tests with a test CA).
    Only(Vec<Certificate>),
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("cannot build the HTTP client: {0}")]
    Build(#[from] reqwest::Error),
    #[error("bad root certificate: {0}")]
    Root(String),
}

/// Installs ring as the process-wide rustls provider (reqwest is built with `rustls-no-provider`). Idempotent.
pub fn install_crypto_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn mozilla_roots() -> Result<Vec<Certificate>, ClientError> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|der| Certificate::from_der(der.as_ref()).map_err(|e| ClientError::Root(e.to_string())))
        .collect()
}

fn base(roots: Roots) -> Result<reqwest::blocking::ClientBuilder, ClientError> {
    install_crypto_provider();
    let builder = Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(READ_TIMEOUT)
        .tls_backend_rustls();
    Ok(match roots {
        Roots::Platform => builder,
        Roots::Mozilla => builder.tls_certs_only(mozilla_roots()?),
        Roots::Only(certs) => builder.tls_certs_only(certs),
    })
}

/// Client for the Liberica API and the JRE download (redirects to the CDN are normal there).
pub fn liberica(roots: Roots) -> Result<Client, ClientError> {
    Ok(base(roots)?.redirect(Policy::limited(10)).build()?)
}

/// Client for the launcher host: redirects only within the same scheme, host and port.
pub fn launcher_host(roots: Roots) -> Result<Client, ClientError> {
    let policy = Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many redirects");
        }
        let first = &attempt.previous()[0];
        if same_origin(first, attempt.url()) { attempt.follow() } else { attempt.stop() }
    });
    Ok(base(roots)?.redirect(policy).build()?)
}

pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default()
}

/// `https`, or `http` to a loopback test server.
pub fn is_allowed_scheme(url: &Url) -> bool {
    url.scheme() == "https" || (url.scheme() == "http" && super::overrides::is_loopback(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        let a = Url::parse("https://launcher.asterium.pro/Asterium.jar").unwrap();
        assert!(same_origin(&a, &Url::parse("https://launcher.asterium.pro:443/x").unwrap()));
        assert!(!same_origin(&a, &Url::parse("http://launcher.asterium.pro/x").unwrap()));
        assert!(!same_origin(&a, &Url::parse("https://evil.example/x").unwrap()));
        assert!(!same_origin(&a, &Url::parse("https://launcher.asterium.pro:8443/x").unwrap()));
    }

    #[test]
    fn schemes() {
        assert!(is_allowed_scheme(&Url::parse("https://example.org/").unwrap()));
        assert!(is_allowed_scheme(&Url::parse("http://127.0.0.1:9/").unwrap()));
        assert!(!is_allowed_scheme(&Url::parse("http://example.org/").unwrap()));
        assert!(!is_allowed_scheme(&Url::parse("ftp://127.0.0.1:9/").unwrap()));
    }

    #[test]
    fn mozilla_roots_load() {
        assert!(mozilla_roots().unwrap().len() > 100);
    }

    #[test]
    fn clients_build() {
        liberica(Roots::Platform).unwrap();
        launcher_host(Roots::Mozilla).unwrap();
    }
}
