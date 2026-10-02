# Changelog

All notable changes to the Asterium prestarter. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/). Player-facing notes for each release live in
`release-notes/<version>.json` (uk, en); a release is refused while its heading here says "Unreleased"
(`scripts/ci/release-checks.sh`).

## [0.3.0] - Unreleased

### Added
- Linux x86_64 and ARM64 builds: a single file (for the LaunchServer to append the launcher jar) and an AppImage.
- macOS universal (Intel and Apple Silicon) `Asterium.app` in a DMG with first-start instructions; signed ad hoc,
  with Developer ID signing and notarization prepared behind the `APPLE_*` secrets (environment `release`).
- Windows ARM64 build, built and tested in every release (handed to players once the native JRE has WebKit).
- Copy mode for the AppImage and macOS: the launcher jar is fetched over HTTPS from the launcher host (Mozilla roots,
  same-origin redirects, 64 MiB cap, jar check) and updated by Gravit itself.
- A signed wrapper policy (`prestarter-policy.json` in every release): jar URL, Java version, newest and minimum
  wrapper version for the AppImage and macOS app.
- Checks before the launch: the jar, the JRE, and on Linux the JavaFX system libraries with the install command for
  the player's distribution.
- "Play now" during a JRE update; a "test mode" badge; interface in Belarusian, English, Polish, Russian and
  Ukrainian.
- A mode without a window when WebView2 or a display is missing (native dialogs on Windows).
- Four loopback-only test overrides (`ASTERIUM_PRESTARTER_*`), documented in the README.
- CI for pull requests and branches (lint, unit and integration tests on five runners, all seven artifacts, smoke
  tests on native runners) without secrets or publishing.

### Changed
- Java: Liberica 25 jre-full chosen from the API by OS and CPU (`arch=arm` for ARM), checked against the API's size
  and sha1, installed atomically in versioned directories, with a pinned sha256 fallback table refreshed weekly.
- Own store per OS (`%LOCALAPPDATA%\Asterium\Prestarter`, `~/.local/share/asterium/prestarter`,
  `~/Library/Application Support/Asterium/Prestarter`) instead of the shared, roaming `GravitLauncherStore`.
- The launcher starts detached with the environment the prestarter got (no AppImage or WebKitGTK variables), with
  the prestarter's arguments; in the embedded mode the prestarter exits at once so Gravit can update the file.
- Tauri 2.12 without the tao fork, rustls instead of OpenSSL, Rust pinned to 1.98.1; the code is laid out by role.
- Releases: `release.json` lists every asset with `os`, `arch`, `format` and `role`; build provenance is attested.

### Fixed
- ARM64 always fell back to an old built-in JRE (`arch=aarch64` is refused by the API).
- Archives were unpacked without protection against `../` paths; a downloaded JRE was never checked.
- A missing WebView2 or display made the prestarter exit silently.
- The progress throttle did not work; the window showed "v1.0.0 Alpha" instead of the real version.
- Windows: from a folder whose name the ANSI code page cannot write (for example Cyrillic with an English
  "Language for non-Unicode programs") Java failed with "Unable to access jarfile ???"; the prestarter now says
  so and what to do (exit code 6) before downloading anything.
- Windows: a virus scanner or the x64 emulator holding the fresh JRE's files open no longer fails the installation.
