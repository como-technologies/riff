#!/usr/bin/env bash
# Prints the build of this checkout as RIFF_BUILD and RIFF_BUILD_TIME
# lines (01M3JEE7YXQPWS65FBVTASAEBX). RIFF_BUILD names the code: the
# hash of the trees of `crates`, `Cargo.toml` and `Cargo.lock`, so each
# commit with the same code has the same RIFF_BUILD. RIFF_BUILD_TIME is
# the UTC time of the last commit that changed the code. It runs the git
# commands of crates/riff-core/build.rs, for an image build that has no
# git.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
id=$(git ls-tree HEAD -- crates Cargo.toml Cargo.lock | git hash-object --stdin)
time=$(TZ=UTC git log -1 --date=format-local:%Y-%m-%dT%H:%M:%SZ --format=%cd \
    -- crates Cargo.toml Cargo.lock)
echo "RIFF_BUILD=${id:0:12}"
echo "RIFF_BUILD_TIME=$time"
