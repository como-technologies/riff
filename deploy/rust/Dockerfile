# riff-rust, riff-base with Rust (01M5SNT57ANM99NQS5SNB8XPWV). It adds
# the C linker and one Rust channel with rustfmt and clippy. A project
# whose CI pins another version builds FROM riff-base and installs its
# version, or builds FROM this image and adds a toolchain.
#
# The channel is the channel of rust-toolchain.toml, and a test checks
# that they agree (01M5SNTDW9WVEXZW5D0TKGBA8Z). `deploy/contract.sh
# rust` checks the contract. The workflow "Session image" builds the
# image on each change of this folder, and checks that this FROM pins
# the digest of the riff-base of deploy/base/.
FROM ghcr.io/como-technologies/riff-base@sha256:2799cd2e7c6e23ab4b9b576ad1331e7315044378a490317624c93d3d984ad5b1

ARG RUST_CHANNEL=1.95

# gcc and libc6-dev: the linker cc of rustc, and the C code of a crate.
# make and pkg-config: the build of a crate with C code.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      gcc libc6-dev make pkg-config \
 && rm -rf /var/lib/apt/lists/*

# The toolchain is in the image, with no write for the session. The
# session gives cargo its own CARGO_HOME (a named volume).
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "$RUST_CHANNEL" \
        --component rustfmt --component clippy \
 && chmod -R a+rX "$RUSTUP_HOME" "$CARGO_HOME"
