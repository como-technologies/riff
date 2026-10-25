#!/usr/bin/env bash
# Checks the contract of an image of riff in a container
# (01M5SNTE280ABA4RYEPK445P3A):
#
#   deploy/contract.sh base IMAGE
#   deploy/contract.sh rust IMAGE
#
# base: riff-base has bash, CA certificates, git, curl, ps, gh at
# /usr/bin/gh and claude, at the versions of deploy/base/Dockerfile. It
# has no language, no build tool, no D-Bus and no keyring.
# rust: riff-rust has the tools of riff-base, and rustc, cargo, rustfmt
# and clippy-driver at the channel of rust-toolchain.toml, and a linker
# that builds a program. It has no other language.
#
# The container runs as a user that is not root, with no network, as a
# session does. IMAGE is a reference that the engine can run. The engine
# is podman, else docker, or the program in RIFF_ENGINE.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."

if [ $# -ne 2 ] || { [ "$1" != base ] && [ "$1" != rust ]; }; then
    echo "usage: deploy/contract.sh base|rust IMAGE" >&2
    exit 2
fi
kind=$1
image=$2

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
    echo "contract.sh: the check needs podman or docker. Install one." >&2
    exit 1
fi

# The value of ARG NAME=VALUE in the Dockerfile at path $2.
arg() {
    sed -n "s/^ARG $1=\(.*\)\$/\1/p" "$2"
}
claude=$(arg CLAUDE_VERSION deploy/base/Dockerfile)
gh=$(arg GH_VERSION deploy/base/Dockerfile)
channel=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
if [ -z "$claude" ] || [ -z "$gh" ] || [ -z "$channel" ]; then
    echo "contract.sh: cannot read the versions of the Dockerfile and rust-toolchain.toml." >&2
    exit 1
fi

# The checks run in the container. Each failed check prints one line.
inside='
bad=0
fail() { echo "contract: $*"; bad=1; }
has() { command -v "$1" >/dev/null 2>&1; }
for tool in bash git curl ps gh claude; do
    has "$tool" || fail "the image has no $tool"
done
[ "$(command -v gh)" = /usr/bin/gh ] || fail "gh is not at /usr/bin/gh"
[ -s /etc/ssl/certs/ca-certificates.crt ] || fail "the image has no CA certificates"
case "$(claude --version 2>&1)" in
"$1 "*) ;;
*) fail "claude is not at version $1: $(claude --version 2>&1)" ;;
esac
case "$(gh --version 2>&1 | head -n 1)" in
"gh version $2 "*) ;;
*) fail "gh is not at version $2: $(gh --version 2>&1 | head -n 1)" ;;
esac
for tool in node npm python3 go java ruby php dotnet; do
    ! has "$tool" || fail "the image has $tool: it has no other language"
done
for tool in dbus-daemon gnome-keyring-daemon; do
    ! has "$tool" || fail "the image has $tool: it has no D-Bus and no keyring"
done
[ ! -e /opt/riff ] || fail "the image has /opt/riff: riff binds its own binary"
case "$4" in
base)
    for tool in cc gcc make rustc cargo; do
        ! has "$tool" || fail "riff-base has $tool: it has no language and no build tool"
    done
    ;;
rust)
    for tool in rustc cargo rustfmt clippy-driver cc make pkg-config; do
        has "$tool" || fail "the image has no $tool"
    done
    case "$(rustc --version 2>&1)" in
    "rustc $3."*) ;;
    *) fail "rustc is not at channel $3: $(rustc --version 2>&1)" ;;
    esac
    printf "fn main() {}\n" > /tmp/contract.rs
    rustc -o /tmp/contract /tmp/contract.rs 2>&1 && /tmp/contract \
        || fail "rustc builds no program"
    ;;
esac
exit "$bad"
'

echo "contract.sh: $kind in $image, with $ENGINE."
exec "$ENGINE" run --rm --network=none --user 12345:12345 -e HOME=/tmp \
    --entrypoint bash "$image" -c "$inside" contract "$claude" "$gh" "$channel" "$kind"
