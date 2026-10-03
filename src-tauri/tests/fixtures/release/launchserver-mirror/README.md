# What a LaunchServer mirrors into `downloads/`

The three files the AppImage and the macOS app fetch for the signed policy (ADR 0001), exactly as nginx served them
from `downloads/` in the gravit-docker e2e (`modules/asterium-releases/e2e/run-e2e.sh`, section X2) on 2026-10-03:

- `prestarter-release.json`, `prestarter-release.json.sig`: the release.json of v0.3.0 written by this repository's
  `scripts/ci/release-manifest.sh` with the asset list of `.github/workflows/publish.yml` (the artifacts of CI run
  37058653061 at 8db5a4c, commit field 8db5a4c5836e9f55864badfa68e23f8f6ce4054b) and signed by
  `scripts/ci/sign-release.sh`, installed and mirrored byte for byte by AsteriumReleases 2.3.0;
- `prestarter-policy.json`: `scripts/ci/make-policy.sh 0.3.0`, mirrored byte for byte;
- `signing.pub.pem`: the public half of that e2e run's throwaway Ed25519 key. Its private half was never kept, so
  nothing here can sign anything; the release key (`.github/release-signing.pub.pem`) refuses these files.

`tests/https.rs` (`the_files_a_launchserver_mirrors_into_downloads_verify`) serves them like the LaunchServer and
verifies them with `policy::fetch`. The files are byte-exact (marked binary in `.gitattributes`).
