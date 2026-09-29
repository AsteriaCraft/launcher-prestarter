#!/usr/bin/env bash
# Prints "present" or "absent" for the tag <tag> of <owner/repo> and FAILS on any other answer (network error, auth,
# rate limit, outage). "The tag does not exist" must be a definite 404, never a guess: a failed lookup read as
# "absent" would let the publish job attach a new build to an existing tag instead of refusing it.
#
# Usage: tag-state.sh <owner/repo> <tag>      (GH_TOKEN with contents: read)
set -euo pipefail

repo="${1:?usage: tag-state.sh <owner/repo> <tag>}"
tag="${2:?usage: tag-state.sh <owner/repo> <tag>}"

if out="$(gh api "repos/$repo/git/ref/tags/$tag" --jq .ref 2>&1)"; then
  if [ "$out" = "refs/tags/$tag" ]; then
    echo present
    exit 0
  fi
  echo "tag-state: unexpected answer while looking up $tag: $out" >&2
  exit 1
fi
case "$out" in
  *"(HTTP 404)"*) echo absent ;;
  *) echo "tag-state: cannot tell whether $tag exists: $out" >&2; exit 1 ;;
esac
