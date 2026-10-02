#!/usr/bin/env bash
# Release gate (ADR 0009, 0012): one version in tauri.conf.json, Cargo.toml and package.json; a CHANGELOG.md heading
# for it; release-notes/<version>.json in schema 1 with Ukrainian and English notes.
#
# Usage: release-checks.sh [--release]
#   without --release (every branch and PR): the heading may say "Unreleased" and the notes' date may be null
#   --release (PRs into `release` and publish.yml): the heading must be dated `## [X.Y.Z] - YYYY-MM-DD` and the notes'
#             date must be that same date
# Prints the version on success.
set -euo pipefail
cd "$(dirname "$0")/../.."
release=0
[ "${1:-}" = "--release" ] && release=1

fail() { echo "::error::release-checks: $*" >&2; exit 1; }

version="$(jq -r .version src-tauri/tauri.conf.json)"
cargo_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -n 1)"
package_version="$(jq -r .version package.json)"
num='(0|[1-9][0-9]*)'
ident='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
[[ "$version" =~ ^$num\.$num\.$num(-$ident(\.$ident)*)?$ ]] || fail "tauri.conf.json version '$version' is not MAJOR.MINOR.PATCH[-PRERELEASE]"
if [ "$cargo_version" != "$version" ] || [ "$package_version" != "$version" ]; then
  fail "versions differ: tauri.conf.json $version, src-tauri/Cargo.toml $cargo_version, package.json $package_version"
fi
grep -q "^name = \"Prestarter\"" src-tauri/Cargo.toml || fail "src-tauri/Cargo.toml lost the Prestarter package name"
cargo_lock_version="$(awk '/^name = "Prestarter"$/{getline; print}' src-tauri/Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')"
[ "$cargo_lock_version" = "$version" ] || fail "src-tauri/Cargo.lock has Prestarter $cargo_lock_version, not $version (run cargo update -p Prestarter)"

core="${version%%-*}"
heading="$(grep -E "^## \[${core//./\\.}\] - " CHANGELOG.md | head -n 1 || true)"
[ -n "$heading" ] || fail "CHANGELOG.md has no '## [$core] - …' heading"
date="${heading##* - }"
if [ "$release" = 1 ]; then
  [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || fail "CHANGELOG.md: '$heading' is not dated (YYYY-MM-DD); date it in the release PR"
else
  [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ || "$date" = Unreleased ]] || fail "CHANGELOG.md: '$heading' must end in a date or Unreleased"
fi

notes="release-notes/$core.json"
[ -f "$notes" ] || fail "$notes is missing"
jq -e --arg v "$core" '
  def lines: type == "array" and length <= 20 and all(.[]; type == "string" and length > 0 and length <= 300);
  def section: type == "object" and (.added | lines) and (.changed | lines) and (.fixed | lines)
               and ((.added + .changed + .fixed) | length > 0);
  .schema == 1 and .component == "prestarter" and .version == $v
  and (.date == null or (.date | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")))
  and (.notes.uk | section) and (.notes.en | section)
' "$notes" >/dev/null || fail "$notes is not schema 1 with uk and en notes for $core (≤ 20 lines of ≤ 300 characters per section)"
notes_date="$(jq -r '.date // "null"' "$notes")"
if [ "$release" = 1 ] && [ "$notes_date" != "$date" ]; then
  fail "$notes date is $notes_date, CHANGELOG.md says $date"
fi
echo "$version"
