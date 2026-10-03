#!/usr/bin/env python3
"""Every job that calls a reusable workflow of this repository grants at least what the called workflow asks for.

GitHub caps a called workflow at its caller job's GITHUB_TOKEN permissions, and when a called job asks for more it
refuses to start the whole run (startup_failure, no logs). publish.yml runs only on `release`, so the pull request CI
never sees such a mistake: v0.3.0-rc.1 was lost to it on 2026-10-03. This check reads the workflow files instead.

Usage: check-workflow-permissions.py <.github/workflows directory>   (exit 1 and one line per gap)
"""
import pathlib
import sys

import yaml

LEVELS = {"none": 0, "read": 1, "write": 2}
WHOLE = {"read-all": 1, "write-all": 2}
ALL = "every scope (read-all/write-all)"


def level_of(block, scope):
    """The level a permissions block gives one scope; None when the block is not set (the default applies)."""
    if block is None:
        return None
    if isinstance(block, str):
        return WHOLE.get(block, 0)
    return LEVELS.get(block.get(scope, "none"), 0)


def scopes(block):
    if isinstance(block, dict):
        return set(block)
    return set()


def asked_for(called):
    """What a called workflow's jobs ask for: each job's own block, else the workflow's, scope by scope."""
    top = called.get("permissions")
    need = {}
    for job in (called.get("jobs") or {}).values():
        block = job.get("permissions", top)
        for scope in scopes(block) | scopes(top):
            lvl = level_of(block, scope) or 0
            need[scope] = max(need.get(scope, 0), lvl)
        if isinstance(block, str):
            need[ALL] = max(need.get(ALL, 0), WHOLE.get(block, 0))
    return {s: l for s, l in need.items() if l > 0}


def main(directory):
    root = pathlib.Path(directory)
    gaps = []
    for path in sorted(root.glob("*.y*ml")):
        workflow = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        top = workflow.get("permissions")
        for name, job in (workflow.get("jobs") or {}).items():
            uses = job.get("uses", "")
            if not uses.startswith("./.github/workflows/"):
                continue
            called_path = root / pathlib.PurePosixPath(uses).name
            called = yaml.safe_load(called_path.read_text(encoding="utf-8")) or {}
            granted = job.get("permissions", top)
            if granted is None:
                continue  # neither the job nor the workflow sets permissions: the repository default applies
            for scope, need in sorted(asked_for(called).items()):
                if scope == ALL:
                    have = WHOLE.get(granted, 0) if isinstance(granted, str) else 0
                else:
                    have = level_of(granted, scope)
                if (have or 0) < need:
                    want = [k for k, v in LEVELS.items() if v == need][0]
                    gaps.append(f"{path.name}: job '{name}' calls {called_path.name}, which asks for {scope}: {want}, "
                                f"but the job grants {scope}: {[k for k, v in LEVELS.items() if v == (have or 0)][0]}")
    for gap in gaps:
        print(gap)
    return 1 if gaps else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
