#!/usr/bin/env bash
# Creates the Ed25519 release-signing key of ONE repository. Run by the OWNER on their own machine (Git Bash on
# Windows, Linux or macOS with openssl 3 and the GitHub CLI logged in) - never in CI. Run it again to rotate.
#
#   scripts/make-release-signing-key.sh AsteriaCraft/launcher-prestarter prestarter
#   scripts/make-release-signing-key.sh AsteriaCraft/asterium-launcher runtime
#   (add --no-upload to only create the files and set the secret yourself)
#
# One key per repository: a leaked prestarter key cannot sign a runtime and vice versa.
# Output, in the current directory:
#   <component>-release-<date>.pem   PRIVATE key (0600). Stored in the repository secret RELEASE_SIGNING_KEY as ONE line
#                                    of base64 of its DER form (the format both release workflows read); then back it
#                                    up OFFLINE (password manager) and delete it here.
#   release-signing.pub.pem          public key -> commit it as <repo>/.github/release-signing.pub.pem (CI self-check)
#   stdout                           the base64 SPKI for the LaunchServer: config/AsteriumReleases/Config.json,
#                                    components.<component>.signingPublicKeys
# Rotation without downtime: run this again (new secret + commit the new .pub.pem), ADD the new public key to
# signingPublicKeys next to the old one, and remove the old one after the first release signed with the new key
# was installed.
set -euo pipefail

repo="${1:?usage: make-release-signing-key.sh <owner/repo> <component> [--no-upload]}"
component="${2:?usage: make-release-signing-key.sh <owner/repo> <component> [--no-upload]}"
upload=true
[ "${3:-}" = "--no-upload" ] && upload=false

key="${component}-release-$(date +%Y%m%d-%H%M%S).pem"
umask 077
openssl genpkey -algorithm ed25519 -out "$key"
openssl pkey -in "$key" -pubout -out release-signing.pub.pem
spki="$(openssl pkey -in "$key" -pubout -outform DER | base64 | tr -d '\r\n')"
fingerprint="$(openssl pkey -in "$key" -pubout -outform DER | sha256sum | cut -c1-16)"

if $upload; then
  # Piped, so the key never appears in a process list or in shell history.
  openssl pkey -in "$key" -outform DER | base64 | tr -d '\r\n' | gh secret set RELEASE_SIGNING_KEY -R "$repo"
  echo "Repository secret RELEASE_SIGNING_KEY of $repo set (key ed25519:$fingerprint)."
else
  printf '%s\n' "Set it yourself: openssl pkey -in $key -outform DER | base64 | tr -d '\\r\\n' | gh secret set RELEASE_SIGNING_KEY -R $repo"
fi
echo
echo "1. Commit the public key:  cp release-signing.pub.pem <$repo checkout>/.github/release-signing.pub.pem"
echo "2. LaunchServer config (components.$component.signingPublicKeys):"
echo "   \"$spki\""
echo "3. Back up $key OFFLINE (password manager), then delete it here: shred -u $key"
