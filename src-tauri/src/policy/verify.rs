//! Verifying a policy (ADR 0001): the release manifest's Ed25519 signature with a compiled-in key, its binding to
//! this repository, component and the stable channel, and the policy file's size and sha256 as listed in it.
//! The same key and manifest format as AsteriumReleases on the LaunchServer (`scripts/ci/sign-release.sh`).

use base64::Engine;
use ring::signature::{ED25519, UnparsedPublicKey};
use semver::Version;
use serde::Deserialize;
use thiserror::Error;

use super::model::{Policy, PolicyError};
use crate::net::download::sha256_bytes;

pub const EXPECTED_REPO: &str = "AsteriaCraft/launcher-prestarter";
pub const EXPECTED_COMPONENT: &str = "prestarter";
pub const POLICY_ASSET: &str = "prestarter-policy.json";

/// The release signing public key(s): `.github/release-signing.pub.pem` today; a backup key is added here (and in
/// AsteriumReleases' `signingPublicKeys`) before it is ever used.
const KEYS_PEM: &[&str] = &[include_str!("../../../.github/release-signing.pub.pem")];

/// DER prefix of an Ed25519 SubjectPublicKeyInfo; the raw 32-byte key follows.
const ED25519_SPKI_PREFIX: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("the release manifest signature does not verify with any known key")]
    BadSignature,
    #[error("the release manifest is not valid: {0}")]
    Manifest(String),
    #[error("the release manifest is for {0}, not for this prestarter")]
    Binding(String),
    #[error("the policy file does not match the signed manifest: {0}")]
    Asset(String),
    #[error(transparent)]
    Policy(#[from] PolicyError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPolicy {
    pub release_version: Version,
    pub policy: Policy,
    /// Exactly the signed bytes, as text (stored in `state.json`).
    pub policy_json: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    repo: String,
    component: String,
    tag: String,
    version: String,
    channel: String,
    assets: Vec<ManifestAsset>,
}

#[derive(Debug, Deserialize)]
struct ManifestAsset {
    name: String,
    size: u64,
    sha256: String,
}

/// Raw 32-byte Ed25519 keys from PEM `PUBLIC KEY` blocks.
pub fn parse_public_key_pem(pem: &str) -> Option<[u8; 32]> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
    let der = base64::engine::general_purpose::STANDARD.decode(body).ok()?;
    let raw = der.strip_prefix(&ED25519_SPKI_PREFIX)?;
    raw.try_into().ok()
}

pub fn builtin_keys() -> Vec<[u8; 32]> {
    KEYS_PEM.iter().map(|pem| parse_public_key_pem(pem).expect("compiled-in key is a valid Ed25519 PEM")).collect()
}

pub fn signature_valid(manifest: &[u8], signature: &[u8], keys: &[[u8; 32]]) -> bool {
    signature.len() == 64
        && keys.iter().any(|key| UnparsedPublicKey::new(&ED25519, key).verify(manifest, signature).is_ok())
}

pub fn verify(
    manifest: &[u8],
    signature: &[u8],
    policy: &[u8],
    keys: &[[u8; 32]],
) -> Result<VerifiedPolicy, VerifyError> {
    if !signature_valid(manifest, signature, keys) {
        return Err(VerifyError::BadSignature);
    }
    let parsed: Manifest = serde_json::from_slice(manifest).map_err(|e| VerifyError::Manifest(e.to_string()))?;
    if parsed.schema != 1 {
        return Err(VerifyError::Manifest(format!("schema {}", parsed.schema)));
    }
    if parsed.repo != EXPECTED_REPO || parsed.component != EXPECTED_COMPONENT {
        return Err(VerifyError::Binding(format!("{} / {}", parsed.repo, parsed.component)));
    }
    if parsed.channel != "stable" {
        return Err(VerifyError::Binding(format!("channel {}", parsed.channel)));
    }
    let version = Version::parse(&parsed.version).map_err(|e| VerifyError::Manifest(format!("version: {e}")))?;
    if parsed.tag != format!("v{}", parsed.version) || !version.pre.is_empty() {
        return Err(VerifyError::Binding(format!("tag {} / version {}", parsed.tag, parsed.version)));
    }
    let asset = parsed
        .assets
        .iter()
        .find(|a| a.name == POLICY_ASSET)
        .ok_or_else(|| VerifyError::Asset(format!("{POLICY_ASSET} is not listed")))?;
    if asset.size != policy.len() as u64 || !asset.sha256.eq_ignore_ascii_case(&sha256_bytes(policy)) {
        return Err(VerifyError::Asset("size or sha256 differs".into()));
    }
    let text = std::str::from_utf8(policy).map_err(|e| VerifyError::Asset(e.to_string()))?;
    let parsed_policy = Policy::parse(text)?;
    Ok(VerifiedPolicy { release_version: version, policy: parsed_policy, policy_json: text.to_owned() })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    const REAL_MANIFEST: &[u8] = include_bytes!("../../tests/fixtures/release/release.json");
    const REAL_SIGNATURE: &[u8] = include_bytes!("../../tests/fixtures/release/release.json.sig");

    pub(crate) struct TestKey {
        pair: Ed25519KeyPair,
    }

    impl TestKey {
        pub(crate) fn new() -> Self {
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            Self { pair: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap() }
        }

        pub(crate) fn public(&self) -> [u8; 32] {
            self.pair.public_key().as_ref().try_into().unwrap()
        }

        pub(crate) fn sign(&self, bytes: &[u8]) -> Vec<u8> {
            self.pair.sign(bytes).as_ref().to_vec()
        }
    }

    pub(crate) fn policy_bytes(latest: &str, min: &str) -> Vec<u8> {
        format!(
            "{{\n  \"schema\": 1,\n  \"jarUrl\": \"https://launcher.asterium.pro/Asterium.jar\",\n  \"javaFeature\": 25,\n  \"minWrapperVersion\": \"{min}\",\n  \"latestWrapperVersion\": \"{latest}\",\n  \"downloadPage\": \"https://asterium.pro/launcher\"\n}}\n"
        )
        .into_bytes()
    }

    pub(crate) fn manifest_bytes(version: &str, policy: &[u8], repo: &str, channel: &str) -> Vec<u8> {
        format!(
            "{{\"schema\":1,\"repo\":\"{repo}\",\"component\":\"prestarter\",\"tag\":\"v{version}\",\"version\":\"{version}\",\"channel\":\"{channel}\",\"commit\":\"{}\",\"assets\":[{{\"name\":\"Prestarter.exe\",\"size\":1,\"sha256\":\"{}\"}},{{\"name\":\"{POLICY_ASSET}\",\"size\":{},\"sha256\":\"{}\"}}]}}",
            "a".repeat(40),
            "b".repeat(64),
            policy.len(),
            sha256_bytes(policy)
        )
        .into_bytes()
    }

    #[test]
    fn the_compiled_in_key_verifies_the_real_v0_2_0_release() {
        let keys = builtin_keys();
        assert_eq!(keys.len(), 1);
        assert!(signature_valid(REAL_MANIFEST, REAL_SIGNATURE, &keys));
        let mut tampered = REAL_MANIFEST.to_vec();
        tampered[20] ^= 1;
        assert!(!signature_valid(&tampered, REAL_SIGNATURE, &keys));
        // v0.2.0 predates the policy: the signature is fine, the asset is missing.
        assert!(matches!(verify(REAL_MANIFEST, REAL_SIGNATURE, b"{}", &keys), Err(VerifyError::Asset(_))));
    }

    #[test]
    fn accepts_a_correct_release() {
        let key = TestKey::new();
        let policy = policy_bytes("0.3.1", "0.3.0");
        let manifest = manifest_bytes("0.3.1", &policy, EXPECTED_REPO, "stable");
        let verified = verify(&manifest, &key.sign(&manifest), &policy, &[key.public()]).unwrap();
        assert_eq!(verified.release_version, Version::new(0, 3, 1));
        assert_eq!(verified.policy.latest_wrapper_version.as_deref(), Some("0.3.1"));
        assert_eq!(verified.policy_json.as_bytes(), policy.as_slice());
    }

    #[test]
    fn refuses_what_it_must() {
        let key = TestKey::new();
        let other = TestKey::new();
        let policy = policy_bytes("0.3.1", "0.3.0");
        let good = manifest_bytes("0.3.1", &policy, EXPECTED_REPO, "stable");
        let keys = [key.public()];

        assert_eq!(verify(&good, &other.sign(&good), &policy, &keys), Err(VerifyError::BadSignature));
        assert_eq!(verify(&good, &key.sign(&good)[..63], &policy, &keys), Err(VerifyError::BadSignature));

        let foreign = manifest_bytes("0.3.1", &policy, "Evil/launcher-prestarter", "stable");
        assert!(matches!(verify(&foreign, &key.sign(&foreign), &policy, &keys), Err(VerifyError::Binding(_))));

        let pre = manifest_bytes("0.3.1", &policy, EXPECTED_REPO, "prerelease");
        assert!(matches!(verify(&pre, &key.sign(&pre), &policy, &keys), Err(VerifyError::Binding(_))));

        let mut swapped = policy.clone();
        swapped[0] = b' ';
        assert!(matches!(verify(&good, &key.sign(&good), &swapped, &keys), Err(VerifyError::Asset(_))));

        let bad_policy = br#"{"schema":1,"jarUrl":"http://x/A.jar","javaFeature":25,"minWrapperVersion":"0.3.0","downloadPage":"https://x/"}"#;
        let manifest = manifest_bytes("0.3.1", bad_policy, EXPECTED_REPO, "stable");
        assert!(matches!(verify(&manifest, &key.sign(&manifest), bad_policy, &keys), Err(VerifyError::Policy(_))));
    }

    #[test]
    fn pem_parsing() {
        assert!(parse_public_key_pem("-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----\n").is_none());
        let key = parse_public_key_pem(include_str!("../../../.github/release-signing.pub.pem")).unwrap();
        assert_eq!(key.len(), 32);
    }
}
