#!/usr/bin/env bash
# Developer ID signing and notarization of Asterium.app and its DMG (ADR 0003). Runs only in publish.yml's sign-macos
# job (environment `release`, the only place the Apple secrets exist) and, as a self-test, in ci.yml.
#
# Usage:
#   macos-signing.sh auto <Asterium.app> <out.dmg> [--dry-run]
#       All five secrets set  -> sign the app (hardened runtime) -> notarize -> staple -> DMG -> sign -> notarize -> staple
#       None set              -> keep the ad hoc app, build the DMG, print a ::notice:: (the release ships unsigned)
#       Some but not all set  -> error before anything is signed (a half-configured setup must not ship unsigned)
#       --dry-run             -> print the commands instead of running notarytool (no Apple account needed)
#   macos-signing.sh self-test <Asterium.app> <out.dmg>
#       Signs a copy with a self-signed code-signing certificate in a temporary keychain and verifies the app and the
#       DMG with codesign: the import, identity lookup and signing path work without an Apple account.
#
# Secrets (environment): APPLE_CERTIFICATE (base64 .p12), APPLE_CERTIFICATE_PASSWORD, APPLE_ID, APPLE_TEAM_ID,
# APPLE_APP_PASSWORD (an app-specific password; also exported as APPLE_PASSWORD, the name Tauri uses).
# Nothing secret is ever printed; the keychain and the .p12 live in a temporary directory removed on exit.
set -euo pipefail

mode="${1:-}"
app="${2:-}"
dmg="${3:-}"
dry_run=0
[ "${4:-}" = "--dry-run" ] && dry_run=1
here="$(cd "$(dirname "$0")" && pwd)"
SECRETS=(APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD)

die() { echo "::error::macos-signing: $*" >&2; exit 1; }
usage() { echo "usage: $0 auto|self-test <Asterium.app> <out.dmg> [--dry-run] | check-secrets" >&2; exit 2; }

# "none" or "all"; some-but-not-all is an error naming the missing ones (never their values).
secrets_state() {
  local name count=0 missing=()
  for name in "${SECRETS[@]}"; do
    if [ -n "${!name:-}" ]; then count=$((count + 1)); else missing+=("$name"); fi
  done
  if [ "$count" = 0 ]; then
    echo none
  elif [ "$count" = "${#SECRETS[@]}" ]; then
    echo all
  else
    die "only some Apple secrets are set; missing: ${missing[*]}. Set all five or none (environment 'release')."
  fi
}

# Works on any OS: publish.yml checks the secrets before anything is built.
if [ "$mode" = check-secrets ]; then
  secrets_state
  exit 0
fi

[ -n "$mode" ] && [ -n "$app" ] && [ -n "$dmg" ] || usage
[ -d "$app/Contents/MacOS" ] || die "$app is not an app bundle"
[ "$(uname -s)" = Darwin ] || die "macOS only"

work="$(mktemp -d)"
keychain="$work/asterium-signing.keychain-db"
cleanup() {
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

# Imports a .p12 into a temporary keychain that codesign can use without prompts; prints nothing secret.
import_identity() { # <p12 file> <p12 password>
  local password
  password="$(openssl rand -hex 24)"
  security create-keychain -p "$password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$password" "$keychain"
  security import "$1" -k "$keychain" -P "$2" -T /usr/bin/codesign >/dev/null
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
  # shellcheck disable=SC2046 # the existing keychain list is a whitespace-separated set of quoted paths
  security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
}

notarize() { # <file>
  local file="$1" submit="$1"
  if [ -d "$file" ]; then
    submit="$work/$(basename "$file").zip"
    ditto -c -k --keepParent "$file" "$submit"
  fi
  if [ "$dry_run" = 1 ]; then
    echo "dry-run: xcrun notarytool submit $(basename "$submit") --apple-id *** --team-id $APPLE_TEAM_ID --password *** --wait --timeout 30m"
    echo "dry-run: xcrun stapler staple $(basename "$file")"
    return
  fi
  xcrun notarytool submit "$submit" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" \
    --wait --timeout 30m
  xcrun stapler staple "$file"
  xcrun stapler validate "$file"
}

sign_developer_id() {
  export APPLE_PASSWORD="$APPLE_APP_PASSWORD"
  printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$work/certificate.p12"
  import_identity "$work/certificate.p12" "$APPLE_CERTIFICATE_PASSWORD"
  local identity team
  identity="$(security find-identity -v -p codesigning "$keychain" | sed -n 's/.*"\(Developer ID Application: .*\)".*/\1/p' | head -n 1)"
  [ -n "$identity" ] || die "APPLE_CERTIFICATE holds no Developer ID Application identity"
  team="$(sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p' <<< "$identity")"
  [ "$team" = "$APPLE_TEAM_ID" ] || die "the certificate's team ($team) is not APPLE_TEAM_ID"
  echo "macos-signing: signing with the Developer ID of team $team"
  codesign --force --options runtime --timestamp --sign "$identity" "$app"
  codesign --verify --strict --deep -vv "$app"
  notarize "$app"
  bash "$here/make-dmg.sh" "$app" "$dmg"
  codesign --force --timestamp --sign "$identity" "$dmg"
  codesign --verify -vv "$dmg"
  notarize "$dmg"
  if [ "$dry_run" = 0 ]; then
    spctl -a -vv -t exec "$app"
    spctl -a -vv -t open --context context:primary-signature "$dmg"
  fi
}

self_test() {
  local copy="$work/Asterium.app" pass=selftest
  ditto "$app" "$copy"
  cat > "$work/openssl.cnf" <<'EOF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Asterium Self-Test Code Signing
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF
  /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 1 -config "$work/openssl.cnf" \
    -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
  /usr/bin/openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" -out "$work/cert.p12" -passout "pass:$pass"
  import_identity "$work/cert.p12" "$pass"
  # A self-signed certificate is only usable once trusted for code signing (CI runners allow this with sudo).
  sudo security add-trusted-cert -d -r trustRoot -p codeSign -k "$keychain" "$work/cert.pem"
  local identity
  identity="$(security find-identity -v -p codesigning "$keychain" | sed -n 's/.*"\(Asterium Self-Test Code Signing\)".*/\1/p' | head -n 1)"
  [ -n "$identity" ] || { security find-identity -p codesigning "$keychain"; die "the self-test identity is not usable"; }
  codesign --force --options runtime --timestamp=none --sign "$identity" "$copy"
  codesign --verify --strict --deep -vv "$copy"
  codesign -dv "$copy" 2>&1 | grep -q "Authority=Asterium Self-Test Code Signing" || die "the copy is not signed by the test identity"
  bash "$here/make-dmg.sh" "$copy" "$dmg"
  codesign --force --timestamp=none --sign "$identity" "$dmg"
  codesign --verify -vv "$dmg"
  sudo security remove-trusted-cert -d "$work/cert.pem" >/dev/null 2>&1 || true
  echo "macos-signing: self-test passed (import, identity, app and DMG signatures verified)"
}

case "$mode" in
  auto)
    state="$(secrets_state)" || exit 1
    if [ "$state" = none ]; then
      echo "::notice::macOS build is NOT signed with a Developer ID: APPLE_CERTIFICATE and the other APPLE_* secrets are not set (environment 'release'). Players open it via System Settings > Privacy & Security > Open Anyway."
      [ -f "$dmg" ] || bash "$here/make-dmg.sh" "$app" "$dmg"
    else
      rm -f "$dmg"
      sign_developer_id
    fi
    ;;
  self-test) self_test ;;
  *) usage ;;
esac
