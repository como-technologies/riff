#!/usr/bin/env bash
# Prints the build of this checkout as RIFF_COMMIT and RIFF_COMMIT_TIME
# lines: the last commit that changed the code, and its UTC time
# (01M3JEE7YXQPWS65FBVTASAEBX). It runs the git command of
# crates/riff-core/build.rs, for an image build that has no git.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
line=$(TZ=UTC git log -1 --abbrev=12 --date=format-local:%Y-%m-%dT%H:%M:%SZ \
    --format='%h %cd' -- crates Cargo.toml Cargo.lock)
echo "RIFF_COMMIT=${line%% *}"
echo "RIFF_COMMIT_TIME=${line#* }"
