#!/usr/bin/env bash
# Measures the size that iroh adds to the static riff (#697). It builds
# two copies of the committed tree, as deploy/static/build.sh builds
# riff: riff as it is, and riff that links the spike in and runs it
# when RIFF_IROH_PROBE is set. Then it prints both sizes.
#
#   riff-size.sh
#
# It runs podman, else docker, or the engine in RIFF_ENGINE. The copies
# go to a new temporary folder, and the script removes it at the end.
# The two builds of the release profile take some minutes.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
ROOT=$(git rev-parse --show-toplevel)

ENGINE="${RIFF_ENGINE:-}"
if [ -z "$ENGINE" ]; then
    for name in podman docker; do
        if command -v "$name" >/dev/null; then
            ENGINE="$name"
            break
        fi
    done
fi
if [ -z "$ENGINE" ]; then
    echo "riff-size.sh: the static build needs podman or docker. Install one." >&2
    exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
for tree in plain probe; do
    mkdir "$WORK/$tree"
    git -C "$ROOT" archive HEAD | tar -x -C "$WORK/$tree"
done

# The probe: the spike is a member of the riff workspace, riff depends
# on it, and the main of riff runs it first when RIFF_IROH_PROBE is set.
PROBE="$WORK/probe"
sed -i '/^\[workspace\]$/d' "$PROBE/spikes/iroh/Cargo.toml"
sed -i '/^\[dependencies\]$/a iroh-spike = { path = "../../spikes/iroh" }' \
    "$PROBE/crates/riff/Cargo.toml"
grep -q '^async fn main() -> Result<()> {$' "$PROBE/crates/riff/src/main.rs"
sed -i '/^async fn main() -> Result<()> {$/a\
    if std::env::var_os("RIFF_IROH_PROBE").is_some() {\
        let code = iroh_spike::main(std::env::args().skip(1).collect()).await;\
        std::process::exit(i32::from(code != std::process::ExitCode::SUCCESS));\
    }' "$PROBE/crates/riff/src/main.rs"

CHANNEL=$(sed -n 's/^channel *= *"\(.*\)"$/\1/p' "$ROOT/rust-toolchain.toml")
IMAGE="docker.io/library/rust:$CHANNEL-alpine"
while IFS='=' read -r name value; do
    export "$name=$value"
done < <("$ROOT/deploy/build-id.sh")

# The plain tree builds with its lock file. The probe adds iroh, so
# cargo adds the crates of iroh to its lock file.
build() {
    "$ENGINE" run --rm \
        -v "$1:/src" -w /src \
        -v riff-static-cargo:/usr/local/cargo/registry \
        -e CARGO_TARGET_DIR=/src/target/static \
        -e CARGO_BUILD_JOBS \
        -e RIFF_COMMIT -e RIFF_COMMIT_TIME \
        "$IMAGE" \
        sh -euc '
            apk add -q --no-cache musl-dev linux-headers
            RUSTUP_TOOLCHAIN=$(rustup default | cut -d" " -f1)
            export RUSTUP_TOOLCHAIN
            cargo build $1 --profile release -p riff
        ' sh "$2"
}
build "$WORK/plain" --locked
build "$PROBE" ""

PLAIN_BIN="$WORK/plain/target/static/release/riff"
PROBE_BIN="$PROBE/target/static/release/riff"
# The probe runs the spike: so iroh is in the binary.
RIFF_IROH_PROBE=1 "$PROBE_BIN" --help | head -1
plain=$(stat -c %s "$PLAIN_BIN")
probe=$(stat -c %s "$PROBE_BIN")
echo "riff:            $plain bytes"
echo "riff with iroh:  $probe bytes"
echo "iroh adds:       $((probe - plain)) bytes"
