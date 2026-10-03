#!/usr/bin/env bash
# Makes the GitHub repository match ADR 0009. Run by the OWNER, once, on their own machine (Git Bash on Windows, Linux
# or macOS with openssl 3 and the GitHub CLI logged in as an admin of the repository) - never in CI. Safe to run again.
#
#   scripts/setup-repository.sh AsteriaCraft/launcher-prestarter --release-key <offline backup .pem> --dry-run
#   scripts/setup-repository.sh AsteriaCraft/launcher-prestarter --release-key <offline backup .pem>
#
# 1. Environment `release`, deployable from the branch `release` only (publish.yml: sign-macos and publish).
# 2. RELEASE_SIGNING_KEY into that environment from the EXISTING private key - the offline backup that signed 0.2.0 -
#    and only then the repository-wide secret deleted, so no workflow on any other branch can read the key the
#    LaunchServer trusts. A key whose public half is not .github/release-signing.pub.pem is refused before anything is
#    uploaded: that public key is compiled into the prestarter and listed in the LaunchServer's signingPublicKeys, so a
#    new key would fail every release. (Rotation is scripts/make-release-signing-key.sh plus the LaunchServer's
#    config, never this script.)
# 3. Protection of `main` and `release` from .github/protect.json: changes only through a pull request with the check
#    "CI result" (ci.yml, from GitHub Actions) green and up to date, for admins too; no approval required, because the
#    only maintainer cannot approve their own pull request; no force pushes, no deletions.
# 4. "Allow GitHub Actions to create and approve pull requests" (jre-watch.yml opens its own), token read-only by
#    default.
# Then it prints what is set: secret names, never a value.
#
# --dry-run checks the key and prints every call that would change something instead of making it.
# RELEASE_SIGNING_PUBKEY overrides the public key to compare with (tests).
set -euo pipefail

usage() {
  echo "usage: setup-repository.sh <owner/repo> --release-key <private key .pem> [--dry-run]" >&2
  exit 2
}
fail() {
  echo "setup-repository: $*" >&2
  exit 1
}

repo="${1:-}"
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || usage
shift
key=""
dry=false
while [ $# -gt 0 ]; do
  case "$1" in
    --release-key)
      [ $# -ge 2 ] || usage
      key="$2"
      shift 2
      ;;
    --dry-run)
      dry=true
      shift
      ;;
    *) usage ;;
  esac
done
[ -n "$key" ] || usage

root="$(cd "$(dirname "$0")/.." && pwd)"
pub="${RELEASE_SIGNING_PUBKEY:-$root/.github/release-signing.pub.pem}"
protect="$root/.github/protect.json"
[ -s "$pub" ] || fail "$pub is missing"
[ -s "$protect" ] || fail "$protect is missing"

# Changes go through here: printed under --dry-run, made otherwise. Reads always run.
change() {
  if $dry; then
    printf 'would run: %s\n' "$*"
  else
    "$@"
  fi
}
names() { # <gh secret list arguments...>: the secret names, one per line
  gh secret list "$@" | awk -F '\t' 'NF { print $1 }'
}
has_secret() { # <name> <gh secret list arguments...> (the list is read whole: grep -q would cut a pipe short)
  local list
  list="$(names "${@:2}")"
  grep -qx "$1" <<< "$list"
}

# 0. The key is the one the LaunchServer and every prestarter trust.
[ -r "$key" ] || fail "cannot read $key"
want="$(openssl pkey -pubin -in "$pub" -outform DER | base64 | tr -d '\r\n')"
have="$(openssl pkey -in "$key" -pubout -outform DER 2>/dev/null | base64 | tr -d '\r\n')" \
  || fail "$key is not a private key openssl can read"
if [ "$have" != "$want" ]; then
  fail "the public half of $key is not $pub. Use the offline backup of the key that signed the published releases; a new key would fail every release (sign-release.sh) and every LaunchServer would refuse it. Nothing was changed."
fi
fingerprint="$(openssl pkey -pubin -in "$pub" -outform DER | sha256sum | cut -c1-16)"
echo "1/4 key ed25519:$fingerprint matches $(basename "$pub")"

# 1. Environment `release`, from the branch `release` only.
change gh api --method PUT "repos/$repo/environments/release" --silent \
  -F 'deployment_branch_policy[protected_branches]=false' -F 'deployment_branch_policy[custom_branch_policies]=true'
policies="$(gh api "repos/$repo/environments/release/deployment-branch-policies" \
  --jq '.branch_policies[] | "\(.type // "branch"):\(.name)"' 2>/dev/null || true)"
if ! grep -qx 'branch:release' <<< "$policies"; then
  change gh api --method POST "repos/$repo/environments/release/deployment-branch-policies" --silent \
    -f name=release -f type=branch
fi
echo "2/4 environment release: deployable from the branch release only"

# 2. The key into the environment; the repository-wide copy deleted only once the environment has it.
if $dry; then
  printf '%s\n' "would run: openssl pkey -in $key -outform DER | base64 | tr -d '\\r\\n' | gh secret set RELEASE_SIGNING_KEY --env release -R $repo"
else
  # Piped, so the key never appears in a process list or in shell history.
  openssl pkey -in "$key" -outform DER | base64 | tr -d '\r\n' | gh secret set RELEASE_SIGNING_KEY --env release -R "$repo"
  has_secret RELEASE_SIGNING_KEY --env release -R "$repo" \
    || fail "the environment release does not list RELEASE_SIGNING_KEY after setting it; the repository secret was kept"
fi
if has_secret RELEASE_SIGNING_KEY -R "$repo"; then
  change gh secret delete RELEASE_SIGNING_KEY -R "$repo"
fi
echo "3/4 RELEASE_SIGNING_KEY: environment release only"

# 3 and 4. Branch protection and the Actions token.
for branch in main release; do
  change gh api --method PUT "repos/$repo/branches/$branch/protection" --silent --input "$protect"
done
change gh api --method PUT "repos/$repo/actions/permissions/workflow" --silent \
  -f default_workflow_permissions=read -F can_approve_pull_request_reviews=true
echo "4/4 main and release protected by $(basename "$protect"); Actions may open pull requests"

$dry && exit 0
echo
echo "Repository secrets:          $(names -R "$repo" | tr '\n' ' ')"
echo "Environment release secrets: $(names --env release -R "$repo" | tr '\n' ' ')"
for branch in main release; do
  echo "Required checks on $branch:   $(gh api "repos/$repo/branches/$branch/protection" \
    --jq '[.required_status_checks.checks[].context] | join(", ")')"
done
if has_secret RELEASE_SIGNING_KEY -R "$repo"; then
  fail "RELEASE_SIGNING_KEY is still a repository secret"
fi
echo "Done. Re-run the check \"Release key only in environment release\" on an open pull request into release."
