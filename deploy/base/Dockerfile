# riff-base, the base of each session image (01M5SNT57ANM99NQS5SNB8XPWV).
# It has what a session of riff needs and no more: bash, CA
# certificates, git, curl, procps, gh and claude. It has no language, no
# build tool, no D-Bus and no keyring. It holds no riff and no secret.
# riff binds its own binary into each session at run time.
#
# A project builds its image FROM a digest of this image, and adds its
# toolchain. `deploy/contract.sh base` checks the contract, and the
# workflow "Session image" builds the image on each change of this
# folder. See "Change the session image" in docs/src/development.md.
FROM docker.io/library/debian:bookworm-slim

# The version of each tool is the minimum that riff needs: the daemon
# tells the lead of a project whose image has an older one
# (01M5SNTPHHXP54YBTV09JNRY3F). Change a version only when riff needs a
# newer tool, or for a fix of security. The versions are the versions on
# pangolin on 2026-10-15.
ARG CLAUDE_VERSION=2.1.300
ARG GH_VERSION=2.102.0

# procps: ps, for the scripts of a project that look at its processes.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bash ca-certificates curl git procps \
 && rm -rf /var/lib/apt/lists/*

# gh goes to /usr/bin/gh: the git config of a person names
# `!/usr/bin/gh auth git-credential` as the credential helper.
RUN curl -fsSLo /tmp/gh.deb \
      "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.deb" \
 && dpkg -i /tmp/gh.deb \
 && rm /tmp/gh.deb

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
