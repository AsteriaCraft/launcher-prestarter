#!/usr/bin/env bash
# Builds Asterium.app as one universal binary (Intel + Apple Silicon) and packs it into
# dist/Asterium-macos-universal.dmg (ADR 0003, 0009). The app is signed ad hoc (tauri.conf.json signingIdentity "-"):
# Apple Silicon refuses unsigned arm64 code. Developer ID signing happens later, in publish.yml's sign-macos job
# (scripts/ci/macos-signing.sh), never here, so this script needs no secrets.
#
# Usage: build-macos.sh       (on macOS, with rustup, Node.js + corepack, python3)
# Output: dist/Asterium.app, dist/Asterium-macos-universal.dmg
set -euo pipefail

cd "$(dirname "$0")/../.."
[ "$(uname -s)" = Darwin ] || { echo "build-macos: macOS only" >&2; exit 1; }

rustup target add x86_64-apple-darwin aarch64-apple-darwin
corepack enable
yarn install --frozen-lockfile
node_modules/.bin/tauri build --target universal-apple-darwin --bundles app

app="${CARGO_TARGET_DIR:-src-tauri/target}/universal-apple-darwin/release/bundle/macos/Asterium.app"
[ -d "$app" ] || { echo "build-macos: $app was not produced" >&2; exit 1; }
binary="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"

archs="$(lipo -archs "$binary")"
case " $archs " in *" x86_64 "*) ;; *) echo "build-macos: no x86_64 slice ($archs)" >&2; exit 1 ;; esac
case " $archs " in *" arm64 "*) ;; *) echo "build-macos: no arm64 slice ($archs)" >&2; exit 1 ;; esac
minos="$(otool -l -arch arm64 "$binary" | awk '/LC_BUILD_VERSION/{f=1} f && /minos/{print $2; exit}')"
[ "$minos" = "11.0" ] || { echo "build-macos: minos $minos, expected 11.0" >&2; exit 1; }
codesign --verify --strict --deep -vv "$app"
codesign -dv "$app" 2>&1 | grep -q "Signature=adhoc" || { echo "build-macos: the app is not signed ad hoc" >&2; exit 1; }

rm -rf dist/Asterium.app
mkdir -p dist
ditto "$app" dist/Asterium.app
bash scripts/ci/make-dmg.sh dist/Asterium.app dist/Asterium-macos-universal.dmg
echo "build-macos: $(lipo -info "$binary"); minos $minos"
