# Signed release manifests

`release.json` and `release.json.sig` are the real assets of prestarter release v0.2.0 (2026-09-29), downloaded with
`gh release download v0.2.0 -R AsteriaCraft/launcher-prestarter -p release.json -p release.json.sig`. The signature
verifies with `.github/release-signing.pub.pem` (`openssl pkeyutl -verify -rawin -pubin ...`). `policy::verify`
tests use them to prove that the compiled-in public key accepts a real release; the release has no
`prestarter-policy.json`, so the policy step itself is tested with a key generated in the test. The files are
byte-exact (marked binary in `.gitattributes`).
