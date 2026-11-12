#!/usr/bin/env bash
# Lints the code for a Mac with Apple silicon on each host, with no link
# (01M77NFYCGDD3GKCE4RXDPKRNE). `just lint-macos` runs it with the
# crates and the flags of the clippy of `just test-macos`, and `just
# check` runs that recipe.
#
#   deploy/lint-macos.sh ARGS
#
# ARGS go to `cargo clippy --target aarch64-apple-darwin`, for example
# `-p riff --all-targets -- -D warnings`. The script needs the Rust target
# aarch64-apple-darwin. When it is missing, the script names the command
# that adds it, and exits with 1. The build script of aws-lc-sys compiles
# C for the target, and only a Mac has the C compiler and the SDK for it.
# clippy links nothing, so a links override takes the place of that build
# script for each aws-lc-sys of Cargo.lock: the cfg universal_prefixed
# picks the bindings of a Mac, and the key include gives aws-lc-rs its
# DEP_AWS_LC_*_INCLUDE.
set -euo pipefail

target=aarch64-apple-darwin
if [ "$#" -eq 0 ]; then
    echo "usage: deploy/lint-macos.sh ARGS, for example: deploy/lint-macos.sh -p riff --all-targets -- -D warnings" >&2
    exit 2
fi

libdir=$("${RUSTC:-rustc}" --print target-libdir --target "$target")
if [ ! -d "$libdir" ]; then
    echo "lint-macos: the Rust target $target is missing. Add it: rustup target add $target" >&2
    exit 1
fi

config=()
if [ -f Cargo.lock ]; then
    for version in $(awk '$0 == "name = \"aws-lc-sys\"" { getline; gsub(/^version = "|"$/, ""); print }' Cargo.lock); do
        links="target.$target.aws_lc_${version//./_}"
        config+=(--config "$links.rustc-cfg=[\"universal_prefixed\"]" --config "$links.include=\"\"")
    done
fi
exec cargo clippy --target "$target" ${config[@]+"${config[@]}"} "$@"
