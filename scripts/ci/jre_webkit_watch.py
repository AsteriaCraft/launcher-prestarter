"""Does Liberica's Windows ARM64 jre-full ship JavaFX WebKit yet? (ADR 0004, jre-watch.yml)

The prestarter installs the x64 JRE on Windows on ARM because the native jre-full has no jfxwebkit.dll. This reads
only the zip's central directory over HTTP Range requests (two small requests instead of a 50 MB download) and
answers with the exit code: 0 = WebKit is there (time to switch), 1 = still missing, 2 = error.

    python3 scripts/ci/jre_webkit_watch.py                      # ask the API, inspect the archive
    python3 scripts/ci/jre_webkit_watch.py --names-file <file>  # offline: one archive entry name per line (tests)
"""

import argparse
import json
import struct
import sys
import urllib.request

API = (
    "https://api.bell-sw.com/v1/liberica/releases?version-feature=25&version-modifier=latest&bitness=64"
    "&os=windows&arch=arm&package-type=zip&bundle-type=jre-full"
)
REQUIRED = ("jfxwebkit.dll",)


def has_webkit(names):
    files = {name.rsplit("/", 1)[-1].lower() for name in names}
    return all(required in files for required in REQUIRED)


def fetch(url, byte_range=None):
    request = urllib.request.Request(url, headers={"User-Agent": "asterium-jre-watch"})
    if byte_range:
        request.add_header("Range", byte_range)
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def central_directory_names(url, size):
    tail = fetch(url, f"bytes={max(0, size - 65_557)}-{size - 1}")
    at = tail.rfind(b"PK\x05\x06")
    if at < 0:
        raise ValueError("no end of central directory record")
    entries, cen_size, cen_offset = struct.unpack_from("<HII", tail, at + 10)
    cen = fetch(url, f"bytes={cen_offset}-{cen_offset + cen_size - 1}")
    names, pos = [], 0
    for _ in range(entries):
        if cen[pos:pos + 4] != b"PK\x01\x02":
            raise ValueError("central directory out of place")
        name_len, extra_len, comment_len = struct.unpack_from("<HHH", cen, pos + 28)
        names.append(cen[pos + 46:pos + 46 + name_len].decode("utf-8", "replace"))
        pos += 46 + name_len + extra_len + comment_len
    return names


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names-file")
    args = parser.parse_args()
    try:
        if args.names_file:
            with open(args.names_file, encoding="utf-8") as handle:
                names = [line.strip() for line in handle if line.strip()]
            source = args.names_file
        else:
            records = json.loads(fetch(API))
            record = max(
                (r for r in records if r.get("GA") and r.get("FX") and r.get("bundleType") == "jre-full"),
                key=lambda r: (r["featureVersion"], r["interimVersion"], r["updateVersion"], r["patchVersion"], r["buildVersion"]),
            )
            names = central_directory_names(record["downloadUrl"], record["size"])
            source = f'{record["filename"]} ({len(names)} entries)'
    except Exception as error:  # noqa: BLE001 - any failure is "could not check", exit 2
        print(f"jre-watch: cannot check: {error}", file=sys.stderr)
        return 2
    if has_webkit(names):
        print(f"jre-watch: {source} HAS {', '.join(REQUIRED)}: the native Windows ARM64 JRE can be used (ADR 0004)")
        return 0
    print(f"jre-watch: {source} still has no {', '.join(REQUIRED)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
