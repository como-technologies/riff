# The spike of O2: one item in a rootless podman container

This spike answers the open question O2 of the design of riff 3.0
(#699): can `claude -p` in stream-json do one work item from start to
push, in a rootless podman container on a real host? It also tests
the gaps of the lead review: the `gh` token, the signature of a
commit, the alternates of the clone, podman, and the limits. The
issue is #707.

The spike is a script, not riff code. It does not change riff.

## What the script does

1. It checks rootless podman: `--userns=keep-id`, `/etc/subuid`,
   `/etc/subgid`, and the cgroup v2 controllers of the user.
2. It builds the session image from `Dockerfile`: `claude`, `git`,
   `gh`, CA certificates, a shell, the Rust toolchain of
   `rust-toolchain.toml`, and the tools of `just check`.
3. It makes a clone for the session with `git clone --reference` of
   the main clone, in a new folder below
   `~/.local/share/riff/spike/`.
4. It tests each gap in a container:
   - `gh` signs in with `GH_TOKEN` from `gh auth token` on the host.
     A bind of `~/.config/gh` gives no token when the token is in the
     keyring.
   - A commit with the git config of the person fails when the config
     signs each commit. With `commit.gpgsign=false` from
     `GIT_CONFIG_*`, it works.
   - git reads the clone only when the objects of the main clone are
     bound at the same path, with no write.
   - `--memory` and `--cpus` set the limits of the container.
5. It runs `claude -p --input-format stream-json --output-format
   stream-json --verbose` in a container. `drive.py` writes two
   messages on stdin, one at a time, as the relay of riff does for
   two wakes. The first message is a tiny real item: one comment line
   in `docs/src/development.md` on the branch `spike-o2`, a commit
   and a push. The second message asks for the state of the commit.
6. It measures `just check` three times: in the new clone, in the
   same clone again, and in a second new clone with the caches warm.
7. It removes the branch `spike-o2` from GitHub, and the clones.

The container of a session has:

| Part | In the container |
|---|---|
| The home | A tmpfs at the path of your home. A write there goes when the container ends. |
| The clone | A bind with write, at the same path as on the host. |
| The objects of the main clone | A bind with no write, at the same path. |
| `~/.gitconfig` | A bind with no write. |
| The signature | `GIT_CONFIG_*` sets `commit.gpgsign=false`. |
| The sign-in of `gh` | `GH_TOKEN`, from `gh auth token` on the host. |
| The sign-in of `claude` | Its own Claude folder, with `~/.claude/.credentials.json` bound with no write. |
| The caches | The named volumes `riff-spike-o2-cargo` (`CARGO_HOME`) and `riff-spike-o2-sccache`. |
| The limits | `--memory 8g --cpus 4`, and `CARGO_BUILD_JOBS=4`. |

## Before you run it

- Linux, with podman 5 or later. Rootless podman needs a range in
  `/etc/subuid` and `/etc/subgid`.
- `gh` signed in on the host, and `claude` signed in on the host.
  The access token of `claude` must be good for 30 more minutes. To
  refresh it, run `claude` on the host one time.
- Python 3 on the host, for `drive.py`.
- About 10 GB of free disk, and 8 GB of free memory.

Install podman on Ubuntu:

```sh
sudo apt install podman
```

## Run the spike

Run it from a clone of riff:

```sh
spikes/o2/run.sh
```

It takes about 20 minutes. It prints each step with PASS, FAIL or
NOTE, and the times. The exit code is 0 when each step passed.

The script writes the results to the folder of the run:
`~/.local/share/riff/spike/o2-TIME/`.

| File | What it holds |
|---|---|
| `results.md` | Each step, with PASS, FAIL or NOTE |
| `events.md` | The summary of the stream-json events: init, the tool calls, the result and the usage of each turn |
| `events.jsonl` | Each stream-json event |
| `check-*.log` | The output of each `just check` |
| `build.log` | The output of `podman build` |

## Change the run

Set these variables to change the run:

| Variable | Default |
|---|---|
| `MEMORY` | `8g` |
| `CPUS` | `4` |
| `BRANCH` | `spike-o2` (it must start with `spike-`) |
| `SPIKE_DIR` | `~/.local/share/riff/spike` |
| `KEEP` | `0`; `1` keeps the clones |

For example, keep the clones:

```sh
KEEP=1 spikes/o2/run.sh
```

## Measure a cold cache again

The first `just check` is cold only when the cache volumes are new.
Remove them before the run:

```sh
podman volume rm riff-spike-o2-cargo riff-spike-o2-sccache
```

## Remove the spike

```sh
rm -rf ~/.local/share/riff/spike
podman volume rm riff-spike-o2-cargo riff-spike-o2-sccache
podman rmi localhost/riff-spike-o2:latest
```

## Secrets

The script prints no secret. podman gets `GH_TOKEN` by its name, not
by its value. `drive.py` looks for the tokens of `gh` and `claude` in
the events. When it finds one, it removes it from the log, and names
it in `events.md`.
