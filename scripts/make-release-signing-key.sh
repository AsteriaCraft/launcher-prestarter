#!/usr/bin/env bash
# Creates a NEW Ed25519 release-signing key for this repository. Run by the OWNER on their own machine (Git Bash on
# Windows, Linux or macOS with openssl 3 and the GitHub CLI logged in) - never in CI.
#
# Only for a first key or a planned ROTATION. The key that signed the published releases is already trusted: its
# public half is .github/release-signing.pub.pem, compiled into every prestarter and listed in the LaunchServer's
# signingPublicKeys. To (re)store THAT key in GitHub, use scripts/setup-repository.sh with its offline backup; a new
# key alone fails every release (sign-release.sh) until the steps below are all done.
#
#   scripts/make-release-signing-key.sh AsteriaCraft/launcher-prestarter prestarter
#   (add --no-upload to only create the files and set the secret yourself)
#
# The secret goes into the GitHub Environment `release` (deployable from the branch `release` only; create it first
# with scripts/setup-repository.sh), never into the repository: a repository secret is readable by a workflow on any
# branch. One key per repository: a leaked prestarter key cannot sign a runtime and vice versa.
# Output, in the current directory:
#   <component>-release-<date>.pem   PRIVATE key (0600). Stored in the environment secret RELEASE_SIGNING_KEY as ONE
#                                    line of base64 of its DER form (the format sign-release.sh reads); then back it up
#                                    OFFLINE (password manager) and delete it here.
#   release-signing.pub.pem          public key -> commit it as <repo>/.github/release-signing.pub.pem (CI self-check,
#                                    and the key the prestarter checks its policy with)
#   stdout                           the base64 SPKI for the LaunchServer: config/AsteriumReleases/Config.json,
#                                    components.<component>.signingPublicKeys
# Rotation without downtime: run this (new secret + commit the new .pub.pem in the same release PR), ADD the new public
# key to signingPublicKeys next to the old one before that release, and remove the old one after the first release
# signed with the new key was installed.
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
  openssl pkey -in "$key" -outform DER | base64 | tr -d '\r\n' | gh secret set RELEASE_SIGNING_KEY --env release -R "$repo"
  echo "Secret RELEASE_SIGNING_KEY of the environment release of $repo set (key ed25519:$fingerprint)."
else
  printf '%s\n' "Set it yourself: openssl pkey -in $key -outform DER | base64 | tr -d '\\r\\n' | gh secret set RELEASE_SIGNING_KEY --env release -R $repo"
fi
echo
echo "1. Commit the public key:  cp release-signing.pub.pem <$repo checkout>/.github/release-signing.pub.pem"
echo "2. LaunchServer config (components.$component.signingPublicKeys), next to the current key:"
echo "   \"$spki\""
echo "3. Back up $key OFFLINE (password manager), then delete it here: shred -u $key"
