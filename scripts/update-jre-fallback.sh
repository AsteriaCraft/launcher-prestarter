#!/usr/bin/env bash
# Regenerates src-tauri/src/jre/fallback.json: the JRE the prestarter installs when the Liberica API is down or answers
# nonsense (ADR 0005). For every target it asks the API for the newest GA jre-full with JavaFX, downloads the archive,
# checks it against the API's size and sha1 (api.bell-sw.com gives the hashes, github.com serves the file: two hosts),
# and records the sha256 the prestarter checks offline. Nothing is written unless all six targets verify.
#
# Usage: scripts/update-jre-fallback.sh [output-file]      (default: src-tauri/src/jre/fallback.json)
# Env:   LIBERICA_API (default https://api.bell-sw.com/v1/liberica/releases), JRE_FEATURE (default 25),
#        JRE_FALLBACK_WORKDIR (keeps the downloads there instead of a temporary directory).
# Needs: bash, curl, jq, sha1sum + sha256sum (or shasum). Runs on Linux CI (jre-watch.yml) and in Git Bash.
set -euo pipefail

cd "$(dirname "$0")/.."
out="${1:-src-tauri/src/jre/fallback.json}"
api="${LIBERICA_API:-https://api.bell-sw.com/v1/liberica/releases}"
feature="${JRE_FEATURE:-25}"
[[ "$feature" =~ ^[1-9][0-9]?$ ]] || { echo "update-jre-fallback: bad JRE_FEATURE: $feature" >&2; exit 2; }

for tool in curl jq; do command -v "$tool" >/dev/null || { echo "update-jre-fallback: missing tool: $tool" >&2; exit 1; }; done
hash_of() { # <algo 1|256> <file>
  if command -v "sha$1sum" >/dev/null; then "sha$1sum" "$2" | cut -d' ' -f1; else shasum -a "$1" "$2" | cut -d' ' -f1; fi
}

work="${JRE_FALLBACK_WORKDIR:-}"
if [ -z "$work" ]; then work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT; fi
mkdir -p "$work"

# key os api-arch package-type: the same table as src-tauri/src/jre/catalog.rs (windows-arm64 is the native JRE that
# is used only where Windows cannot emulate x64, ADR 0004).
targets=(
  "windows-x64 windows x86 zip"
  "windows-arm64 windows arm zip"
  "linux-x64 linux x86 tar.gz"
  "linux-arm64 linux arm tar.gz"
  "macos-x64 macos x86 tar.gz"
  "macos-arm64 macos arm tar.gz"
)

entries='{}'
versions=()
for row in "${targets[@]}"; do
  read -r key os arch pkg <<< "$row"
  url="$api?version-feature=$feature&version-modifier=latest&bitness=64&os=$os&arch=$arch&package-type=$pkg&bundle-type=jre-full"
  json="$(curl -fsS --retry 3 --max-time 30 "$url")"
  # Same selection as jre::api: GA, JavaFX, jre-full, this OS/arch/package; highest version first.
  record="$(jq -c --arg os "$os" --arg arch "$arch" --arg pkg "$pkg" '
    [ .[] | select(.GA == true and .FX == true and .bundleType == "jre-full" and .os == $os
                   and .architecture == $arch and .packageType == $pkg and .bitness == 64) ]
    | sort_by([.featureVersion, .interimVersion, .updateVersion, .patchVersion, .buildVersion])
    | last // empty' <<< "$json")"
  [ -n "$record" ] || { echo "update-jre-fallback: no GA jre-full with FX for $key" >&2; exit 1; }
  dl="$(jq -r .downloadUrl <<< "$record")"; name="$(jq -r .filename <<< "$record")"
  size="$(jq -r .size <<< "$record")"; sha1="$(jq -r .sha1 <<< "$record")"; version="$(jq -r .version <<< "$record")"
  [[ "$dl" == https://github.com/bell-sw/Liberica/releases/download/* ]] || { echo "update-jre-fallback: unexpected download host: $dl" >&2; exit 1; }
  [[ "$name" =~ ^[A-Za-z0-9._+-]+$ && "$dl" == */"$name" ]] || { echo "update-jre-fallback: bad file name: $name" >&2; exit 1; }
  [[ "$sha1" =~ ^[0-9a-f]{40}$ && "$size" =~ ^[1-9][0-9]*$ ]] || { echo "update-jre-fallback: bad hash or size for $key" >&2; exit 1; }
  file="$work/$name"
  if [ ! -f "$file" ] || [ "$(wc -c < "$file" | tr -d ' ')" != "$size" ]; then
    echo "update-jre-fallback: downloading $name ($size bytes)" >&2
    curl -fsSL --retry 3 --proto '=https' --tlsv1.2 -o "$file.part" "$dl"
    mv "$file.part" "$file"
  fi
  got_size="$(wc -c < "$file" | tr -d ' ')"
  [ "$got_size" = "$size" ] || { echo "update-jre-fallback: $name is $got_size bytes, the API says $size" >&2; exit 1; }
  got_sha1="$(hash_of 1 "$file")"
  [ "$got_sha1" = "$sha1" ] || { echo "update-jre-fallback: $name sha1 $got_sha1, the API says $sha1" >&2; exit 1; }
  sha256="$(hash_of 256 "$file")"
  versions+=("$version")
  entries="$(jq -c --arg k "$key" --arg url "$dl" --arg name "$name" --argjson size "$size" --arg sha1 "$sha1" \
    --arg sha256 "$sha256" --arg version "$version" --arg pkg "$pkg" \
    '. + {($k): {version: $version, url: $url, filename: $name, size: $size, packageType: $pkg, sha1: $sha1, sha256: $sha256}}' <<< "$entries")"
  echo "update-jre-fallback: $key $version $sha256" >&2
done

tmp="$out.tmp"
jq -n --argjson feature "$feature" --argjson targets "$entries" \
  '{schema: 1, featureVersion: $feature, targets: $targets}' > "$tmp"
mv "$tmp" "$out"
echo "update-jre-fallback: wrote $out (${#versions[@]} targets)" >&2
