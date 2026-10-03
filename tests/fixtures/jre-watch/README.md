# Windows JRE file lists

The `.dll` entries of Liberica 25.0.4.1+1 jre-full for Windows aarch64 and x64 (`unzip -Z1 <archive> | grep '\.dll$'`,
2026-10-02). `scripts/ci/jre_webkit_watch.py --names-file` must say "missing" for aarch64 (exit 1) and "present" for
x64 (exit 0): the x64 list stands in for the day BellSoft ships WebKit for Windows on ARM (ADR 0004).
