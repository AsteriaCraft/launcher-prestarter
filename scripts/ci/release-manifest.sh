#!/usr/bin/env bash
# Writes the release metadata that the AsteriumReleases LaunchServer module verifies:
#   <dist>/SHA256SUMS.txt  - `sha256sum` of every asset (for humans: `sha256sum -c SHA256SUMS.txt`)
#   <dist>/release.json    - the manifest that gets SIGNED (scripts/ci/sign-release.sh). Unlike bare checksums it
#                            binds the assets to the repository, component, tag, version, channel and commit, so a
#                            signed build cannot be republished under another tag, as another component, or as a
#                            stable release when it was built as a pre-release.
#
# Usage: release-manifest.sh <dist-dir> <owner/repo> <component> <version> <commit-sha> <asset>...
#   asset: <name>, or <name>:<os>:<arch>:<format>:<role> to add those optional fields to its entry ("-" = leave one
#   out), e.g. Prestarter-linux-aarch64:linux:aarch64:elf:prestarter. Plain names give the same output as before.
#   version: MAJOR.MINOR.PATCH[-PRERELEASE] (no leading "v"); the tag is v<version>; a -PRERELEASE suffix makes the
#   channel "prerelease", otherwise "stable".
# The same script is used by every Asterium release workflow (copy it verbatim into other repositories).
set -euo pipefail

if [ "$#" -lt 6 ]; then
  echo "usage: $0 <dist-dir> <owner/repo> <component> <version> <commit-sha> <asset>[:os:arch:format:role]..." >&2
  exit 2
fi
dist="$1"; repo="$2"; component="$3"; version="$4"; commit="$5"; shift 5

# Same rules as the module (ReleaseManifest.java): anything else is refused there, so refuse it here first.
[[ "$repo" =~ ^[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9._-]{1,100}$ ]] || { echo "invalid repo: $repo" >&2; exit 2; }
[[ "$component" =~ ^[a-z0-9][a-z0-9-]{0,63}$ ]] || { echo "invalid component: $component" >&2; exit 2; }
num='(0|[1-9][0-9]*)'
ident='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
[[ "$version" =~ ^$num\.$num\.$num(-$ident(\.$ident)*)?$ ]] || { echo "not a semantic version: $version" >&2; exit 2; }
commit="$(printf '%s' "$commit" | tr 'A-F' 'a-f')"
[[ "$commit" =~ ^([0-9a-f]{40}|[0-9a-f]{64})$ ]] || { echo "invalid commit: $commit" >&2; exit 2; }
case "$version" in *-*) channel=prerelease ;; *) channel=stable ;; esac

cd "$dist"
: > SHA256SUMS.txt
entries=()
meta_re='^[a-z0-9][a-z0-9_-]{0,31}$'
for spec in "$@"; do
  # <name> or <name>:<os>:<arch>:<format>:<role>; "-" leaves a field out (ADR 0009).
  # shellcheck disable=SC2034 # os, arch and format are read through ${!field} below
  IFS=: read -r asset os arch format role extra <<< "$spec"
  if [ "$spec" != "$asset" ] && { [ -n "${extra:-}" ] || [ -z "${role:-}" ]; }; then
    echo "invalid asset spec (want name or name:os:arch:format:role): $spec" >&2; exit 2
  fi
  meta=""
  for field in os arch format role; do
    value="${!field:-}"
    [ -z "$value" ] || [ "$value" = "-" ] && continue
    [[ "$value" =~ $meta_re ]] || { echo "invalid $field in $spec: $value" >&2; exit 2; }
    meta+="$(printf ', "%s": "%s"' "$field" "$value")"
  done
  [[ "$asset" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$ && "$asset" != *..* ]] || { echo "invalid asset name: $asset" >&2; exit 2; }
  case "$asset" in release.json|release.json.sig|SHA256SUMS.txt) echo "reserved asset name: $asset" >&2; exit 2 ;; esac
  if [ ! -f "$asset" ] || [ -L "$asset" ] || [ ! -s "$asset" ]; then echo "missing or empty asset: $asset" >&2; exit 2; fi
  sha="$(sha256sum "$asset" | cut -d' ' -f1)"
  size="$(stat -c %s "$asset")"
  printf '%s  %s\n' "$sha" "$asset" >> SHA256SUMS.txt
  entries+=("$(printf '    {"name": "%s", "size": %s, "sha256": "%s"%s}' "$asset" "$size" "$sha" "$meta")")
done

{
  printf '{\n'
  printf '  "schema": 1,\n'
  printf '  "repo": "%s",\n' "$repo"
  printf '  "component": "%s",\n' "$component"
  printf '  "tag": "v%s",\n' "$version"
  printf '  "version": "%s",\n' "$version"
  printf '  "channel": "%s",\n' "$channel"
  printf '  "commit": "%s",\n' "$commit"
  printf '  "assets": [\n'
  last=$(( ${#entries[@]} - 1 ))
  for i in "${!entries[@]}"; do
    if [ "$i" -lt "$last" ]; then printf '%s,\n' "${entries[$i]}"; else printf '%s\n' "${entries[$i]}"; fi
  done
  printf '  ]\n'
  printf '}\n'
} > release.json

echo "release.json (v$version, $channel, ${#entries[@]} asset(s)):"
cat release.json
