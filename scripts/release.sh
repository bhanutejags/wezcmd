#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: scripts/release.sh vX.Y.Z}"
case "$version" in v*) ;; *) echo "version must start with v" >&2; exit 2 ;; esac

cargo test --locked
jj tag set "$version"
jj git export
# jj doesn't push Git tags yet; exporting then pushing the tag lets the GitHub
# Actions release workflow build and publish assets.
git push origin "refs/tags/$version"
