# The session image of the spike of O2 (#707, #699). It is a test
# image, not the session image of riff 3.0. It has what decision 5 of
# #699 names: claude, git, gh, CA certificates, a POSIX shell and the
# Rust toolchain of rust-toolchain.toml. It also has the tools of
# `just check`. It holds no riff and no secret.
FROM docker.io/library/debian:bookworm-slim

# run.sh gives the channel of rust-toolchain.toml. The other versions
# are the versions on pangolin on 2026-10-15.
ARG RUST_CHANNEL=1.95
ARG CLAUDE_VERSION=2.1.300
ARG GH_VERSION=2.102.0
ARG JUST_VERSION=1.58.0
ARG MDBOOK_VERSION=0.5.4
ARG MDBOOK_MERMAID_VERSION=0.17.1
ARG MDBOOK_GRUVBOX_VERSION=1.2.0
ARG SCCACHE_VERSION=0.18.0

# procps: ps, for crates/hygiene/ci-lock.sh. gcc and make: the C code
# of aws-lc-sys.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bash ca-certificates curl gcc git libc6-dev make pkg-config procps \
 && rm -rf /var/lib/apt/lists/*

# The toolchain is in the image, with no write for the session. The
# session gives cargo its own CARGO_HOME (a named volume).
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "$RUST_CHANNEL" \
        --component rustfmt --component clippy --component rust-analyzer \
 && chmod -R a+rX "$RUSTUP_HOME" "$CARGO_HOME"

# gh goes to /usr/bin/gh: the git config of a person names
# `!/usr/bin/gh auth git-credential` as the credential helper.
RUN curl -fsSLo /tmp/gh.deb \
      "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.deb" \
 && dpkg -i /tmp/gh.deb \
 && rm /tmp/gh.deb

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

# claude, from the installer of Anthropic, at a fixed version. The
# session does not update it.
RUN curl -fsSL https://claude.ai/install.sh | bash -s "$CLAUDE_VERSION" \
 && install -m 0755 "$(readlink -f /root/.local/bin/claude)" /usr/local/bin/claude \
 && rm -rf /root/.local /root/.claude /root/.claude.json
ENV DISABLE_AUTOUPDATER=1 \
    GH_NO_UPDATE_NOTIFIER=1 \
    GH_PROMPT_DISABLED=1

# /tmp of the container is its own: a write goes to the copy-on-write
# layer, and goes when the container ends.
CMD ["bash"]
