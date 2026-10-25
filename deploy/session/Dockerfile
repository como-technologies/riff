# The session image of this repository (01M5SNTDW9WVEXZW5D0TKGBA8Z).
# It is riff-rust, which is riff-base with Rust, and it adds what only
# riff needs: rust-analyzer, the tools of `just check`, and a D-Bus with
# GNOME Keyring for the tests. riff-base has claude, git, gh, CA
# certificates and a POSIX shell. The image holds no riff and no secret.
#
# The workflow "Session image" builds it on each change of this folder,
# checks that the FROM below pins the riff-rust of deploy/rust/, and
# checks that riff.toml pins this image (01M5SNTDZ95BRDCPQEKDTYD2PY).
# See "Change the session image" in docs/src/development.md.
FROM ghcr.io/como-technologies/riff-rust@sha256:eb0c0920825ef44e72e4183bb8a4a93a5add5a09422a4f3e73ec61cde19ccfe1

# The versions are the versions on pangolin on 2026-10-15.
ARG JUST_VERSION=1.58.0
ARG MDBOOK_VERSION=0.5.4
ARG MDBOOK_MERMAID_VERSION=0.17.1
ARG MDBOOK_GRUVBOX_VERSION=1.2.0
ARG SCCACHE_VERSION=0.18.0

# dbus-bin, dbus-daemon and gnome-keyring: the private keyring of the
# tests of the keyring store (01M556QPPRSQ91HDED128G2X7G).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      dbus-bin dbus-daemon gnome-keyring \
 && rm -rf /var/lib/apt/lists/*

# The component of rust-toolchain.toml that riff-rust does not have. The
# toolchain stays in the image, with no write for the session.
RUN rustup component add rust-analyzer \
 && chmod -R a+rX "$RUSTUP_HOME" "$CARGO_HOME"

# The tools of `just check`, from their releases.
RUN set -eu; \
    get() { \
      mkdir -p /tmp/get && curl -fsSL "$2" | tar -xz -C /tmp/get \
      && install -m 0755 "$(find /tmp/get -type f -name "$1" | head -n 1)" /usr/local/bin/ \
      && rm -rf /tmp/get; \
    }; \
    get just "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get mdbook "https://github.com/rust-lang/mdBook/releases/download/v${MDBOOK_VERSION}/mdbook-v${MDBOOK_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get mdbook-mermaid "https://github.com/badboy/mdbook-mermaid/releases/download/v${MDBOOK_MERMAID_VERSION}/mdbook-mermaid-v${MDBOOK_MERMAID_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get sccache "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz"
RUN cargo install --locked --root /usr/local "mdbook-gruvbox@${MDBOOK_GRUVBOX_VERSION}" \
 && rm -rf "$CARGO_HOME/registry" "$CARGO_HOME/git"

# /tmp of the container is its own: a write goes to the copy-on-write
# layer, and goes when the container ends.
CMD ["bash"]
