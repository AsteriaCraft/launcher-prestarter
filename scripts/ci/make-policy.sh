#!/usr/bin/env bash
# Writes the release's signed-policy asset (ADR 0001): policy/prestarter-policy.json (the values compiled into the
# binary) plus "latestWrapperVersion" = this release's version. release-manifest.sh lists it in release.json, so the
# release signature covers it; the LaunchServer mirrors it into downloads/ for the AppImage and the macOS app.
#
# Usage: make-policy.sh <version> <out-file>
set -euo pipefail
version="${1:?usage: make-policy.sh <version> <out-file>}"
out="${2:?usage: make-policy.sh <version> <out-file>}"
cd "$(dirname "$0")/../.."
# A pre-release gets a policy too; wrappers accept only policies of stable releases (policy::verify).
num='(0|[1-9][0-9]*)'
ident='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
[[ "$version" =~ ^$num\.$num\.$num(-$ident(\.$ident)*)?$ ]] || { echo "make-policy: not a semantic version: $version" >&2; exit 2; }
jq -e '.schema == 1 and (.jarUrl | startswith("https://")) and (.javaFeature | type == "number")
       and (.minWrapperVersion | test("^[0-9]+\\.[0-9]+\\.[0-9]+$")) and (.downloadPage | startswith("https://"))
       and (has("latestWrapperVersion") | not)' policy/prestarter-policy.json >/dev/null \
  || { echo "make-policy: policy/prestarter-policy.json is not a valid base policy" >&2; exit 1; }
min="$(jq -r .minWrapperVersion policy/prestarter-policy.json)"
[ "$(printf '%s\n%s\n' "$min" "$version" | sort -V | head -n 1)" = "$min" ] \
  || { echo "make-policy: minWrapperVersion $min is above the release version $version" >&2; exit 1; }
mkdir -p "$(dirname "$out")"
jq --arg v "$version" '. + {latestWrapperVersion: $v}' policy/prestarter-policy.json > "$out"
echo "make-policy: $out (latestWrapperVersion $version, minWrapperVersion $min)"
