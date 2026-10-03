#!/usr/bin/env bash
# Signs <dist>/release.json with Ed25519 -> <dist>/release.json.sig (64 raw bytes), then verifies the signature
# against the committed public key, so a wrong or half-rotated key fails here and not on the LaunchServer.
#
# Key source (first match):
#   RELEASE_SIGNING_KEY       - CI: the secret of the GitHub Environment `release`, which only publish.yml's publish
#                               job (branch `release`) can read; never a repository secret (publish.yml and ci.yml
#                               refuse to go on while one exists). ONE line of base64 of the DER private key (what
#                               scripts/setup-repository.sh and make-release-signing-key.sh store), or base64 of the
#                               PEM, or the PEM itself.
#   RELEASE_SIGNING_KEY_FILE  - offline signing by the owner: path to the PEM private key.
# Public key for the self-check: RELEASE_SIGNING_PUBKEY (default .github/release-signing.pub.pem).
#
# Exit codes: 0 signed, 3 no key available (the caller decides whether that is fatal), anything else = error.
# The private key only ever exists in 0600 temp files that are removed on exit; it is never echoed.
set -euo pipefail

dist="${1:?usage: sign-release.sh <dist-dir>}"
pub="${RELEASE_SIGNING_PUBKEY:-.github/release-signing.pub.pem}"
[ -s "$dist/release.json" ] || { echo "sign-release: $dist/release.json is missing" >&2; exit 1; }
rm -f "$dist/release.json.sig" # never leave a signature of something else behind

umask 077
key="$(mktemp)"
raw="$(mktemp)"
trap 'rm -f "$key" "$raw"' EXIT
if [ -n "${RELEASE_SIGNING_KEY:-}" ]; then
  case "$RELEASE_SIGNING_KEY" in
    *"-----BEGIN"*) printf '%s\n' "$RELEASE_SIGNING_KEY" > "$key" ;;
    *)
      printf '%s' "$RELEASE_SIGNING_KEY" | tr -d ' \r\n' | base64 -d > "$raw" 2>/dev/null \
        || { echo "sign-release: RELEASE_SIGNING_KEY is not base64 (nor PEM)" >&2; exit 1; }
      if grep -q -- "-----BEGIN" "$raw"; then
        cat "$raw" > "$key"
      elif ! openssl pkey -inform DER -in "$raw" -out "$key" 2>/dev/null; then
        echo "sign-release: RELEASE_SIGNING_KEY is neither a DER nor a PEM private key" >&2
        exit 1
      fi
      ;;
  esac
elif [ -n "${RELEASE_SIGNING_KEY_FILE:-}" ]; then
  cat "$RELEASE_SIGNING_KEY_FILE" > "$key"
else
  exit 3
fi

if ! openssl pkeyutl -sign -rawin -inkey "$key" -in "$dist/release.json" -out "$dist/release.json.sig" 2>/dev/null \
    || [ "$(stat -c %s "$dist/release.json.sig")" != 64 ]; then
  rm -f "$dist/release.json.sig"
  echo "sign-release: the key is not a usable Ed25519 private key" >&2
  exit 1
fi
if [ ! -s "$pub" ]; then
  echo "sign-release: public key $pub is missing - it is committed with the key (scripts/make-release-signing-key.sh)" >&2
  exit 1
fi
if ! openssl pkeyutl -verify -rawin -pubin -inkey "$pub" -in "$dist/release.json" -sigfile "$dist/release.json.sig" >/dev/null 2>&1; then
  rm -f "$dist/release.json.sig"
  echo "sign-release: the signature does not verify with $pub - the secret and the committed public key do not match" >&2
  exit 1
fi
fingerprint="$(openssl pkey -pubin -in "$pub" -outform DER | sha256sum | cut -c1-16)"
echo "sign-release: release.json signed and verified (key ed25519:$fingerprint)"
