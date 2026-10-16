# riff 3.0

This page is the design of riff 3.0. riff 3.0 is one binary on the
machine of each person. A daemon of riff runs each work item in its
own container. Later, the daemons of different people connect peer to
peer.

## The goal of isolation

riff is not a security tool. It does not make Claude Code more secure
than a plain `claude` that you start. The permission modes and the
settings of Claude Code do that, and you choose them.

riff isolates sessions, so that they do not step on each other or on
your machine: files, git state, processes, `/tmp`, ports, CPU and
memory.

riff does not make Claude less secure. A session gets no power that a
plain `claude` lacks, other than the riff tools.

## The picture

```mermaid
flowchart LR
    subgraph M["your machine"]
        V["riff: CLI and ratatui view"] --> D["riff daemon<br/>systemd user unit"]
        D -- "podman" --> C1
        D -- "podman" --> C2
        subgraph C1["container: issue-12 (conmon)"]
            R1["riff relay"] --> K1["claude -p"]
        end
        subgraph C2["container: verify of issue-9 (conmon)"]
            R2["riff relay"] --> K2["claude -p"]
        end
        R1 -- "its own socket" --> D
        R2 -- "its own socket" --> D
        I[("session image<br/>pinned by digest")] -.-> C1
        I -.-> C2
    end
    D <-- "iroh, later" --> P["the daemon of another person"]
    D -- "claims" --> G["GitHub"]
    C1 -- "push with your sign-in" --> G
```

- The daemon is a systemd user unit. It owns the state, the keys and
  the sessions.
- Each work item is one session in one rootless podman container. A
  verify is an item too. The session starts, does its item and ends.
- `riff relay` is the entrypoint of the container. It starts `claude`
  and talks to the daemon through a socket that only that session
  sees.
- Each repository names its session image, pinned by digest, in the
  file `riff.toml` at its root. See
  [Change the session image](development.md#change-the-session-image).
- The daemon makes the claims on GitHub. The session pushes its work
  with your sign-in.

## What sessions share

Each row is a thing that two sessions on one machine share. The last
column is honest: it says what a session can still do to another
session. The rows use the result of the spike of one item in a
container,
[#707](https://github.com/como-technologies/riff/issues/699#issuecomment-6186136739),
and the result of the spike of the claims,
[#708](https://github.com/como-technologies/riff/issues/699#issuecomment-6184805987).
The [requirements of riff 3.0](requirements.md#riff-30) hold each
decision of the design.

| What sessions share | How riff keeps them apart | What a session can still do to another |
|---|---|---|
| The files of your machine | Each session is a rootless podman container (`--userns=keep-id`). It writes only its binds: its own clone, its own Claude folder, the folder of its socket and the cache volumes. Each other write goes to the copy-on-write layer of the container, and goes when the session ends. | Nothing outside its binds. See the rows of git and of the caches. |
| git: the clone and its alternates | Each session has its own clone, made with `git clone --reference MAIN` in a folder of riff. The container sees the objects of the main clone at the same path, with no write. No session writes the git dir of the main clone or of another session. The clone goes when its item ends, after its push. | Nothing in the clone of another session. A `git gc` of the main clone can remove an object that a session clone reads. |
| git: the config | `~/.gitconfig` is bound with no write, or as an overlay whose changes go when the session ends. riff writes no git config of yours. A session writes only the config of its own clone. | Nothing. |
| git: the commit | A commit has your git identity and a `Riff-Session:` trailer. A session commits with no signature: riff sets `commit.gpgsign=false` in the environment of the container, so it applies to each git call of the session. The squash merge on GitHub makes the commit on `main`. No gpg agent goes into a container. | Nothing. Only the trailer tells the commits of two sessions apart. |
| git: the repository on GitHub | Each session pushes its own branch from its own clone. The daemon makes the claims, not the session: see the next row. | With your sign-in, a session can push to the branch of another session, or delete it. |
| The claims | A claim is the ref `refs/riff/claims/ITEM` on GitHub, for example `refs/riff/claims/issue-12`. It is not a branch. Each claim is a new commit: the empty tree, no parent, no signature, and a message with the item, the user, the session and the time. The daemon creates the ref with a lease (the old value: no ref), and deletes it with the lease of its commit. GitHub compares the old value on the server, so only one session gets an item. A failed lease means that another session holds the item. No ruleset covers `refs/riff/**`, so only the lease that riff uses keeps a claim. In a public repository, each claim ref and each claim commit is public, also after the release. A claim commit holds no secret. A claim ref has no time limit. The lead deletes the claim of a session that is gone, with the lease of its claim commit (decision 35). | With your sign-in, a session can delete or replace a claim ref with no lease, for example with `git push --force`. |
| The processes | Each container has its own processes. A `pkill` in one session does not hit another. `podman stop` ends a session and each of its processes. | Nothing: it does not see the processes of another container. |
| `/tmp` | Each container has its own `/tmp`. It goes when the session ends. | Nothing. |
| The ports | Each container has its own loopback. A test port of one session does not hit another. | It can reach a port that listens on the network of your machine, as each program on it can. |
| The caches: `CARGO_HOME` and sccache | One named volume holds `CARGO_HOME`, and one holds the folder of sccache. Both are bound with write. The tools lock them themselves. The toolchain is in the image, with no write. A new clone has no `target/`: a warm sccache makes its build short. | It can write a bad file in a cache, and a later build of another session uses it. |
| Your home | The home of the container is an empty tmpfs at the path of your home. riff binds your `~/.gitconfig` and the Claude folder of the session on top of it. Each other write in that home goes when the session ends. riff writes nothing in your home outside its own folders. It does not write `~/.claude`, `~/.claude.json`, a git config or a shell file. | It reads `~/.gitconfig` and `~/.config/gh`. |
| The CPU | Each container gets `--cpus`, 4 by default. The daemon keeps one limit for the host: the number of live sessions, at most 3 by default. | A build of one session makes the builds of the others slower, up to its limit. |
| The memory | Each container gets `--memory`, 8 GB by default. A session past its limit loses its own processes, not those of another session. The daemon starts no new session while the free memory is below a floor, 8 GB by default. | It takes memory, up to its limit, that a new session needs. |
| The network | Each session reaches the internet, as a plain `claude` does. No proxy and no filter. | It uses the same connection and reaches each service that your machine reaches. |
| The sign-in of `gh` | The daemon reads `gh auth token` on the host and gives the container `GH_TOKEN`. podman gets only the name of the variable, so the value is in no command line. The image has `gh` at `/usr/bin/gh`. A session has your rights on GitHub, as a plain `claude` has. No GitHub App. No secret goes into a log line, a message or a file of a clone. | It can do on GitHub each thing that you can do, also to the issues and the pull requests of another session. |
| The sign-in of `claude` | Each session has its own Claude folder in a folder of riff. riff trusts the clone of the session there, as you trust the main clone. The daemon gives the session the long-lived token of `claude setup-token` in `CLAUDE_CODE_OAUTH_TOKEN`. Until that works, riff binds your `~/.claude/.credentials.json` with no write, and starts no session while its access token has less than 60 minutes left. riff never changes your own Claude config. | It uses the limits of your plan that another session needs. |
| The daemon and the riff tools | Each relay talks to the daemon through its own Unix socket, in a folder that only that session sees. The socket tells the daemon which session asks. A session gets only the riff tools that talk: `whoami`, `who`, `threads`, `read`, `post`, `tell`, `status` and `blocked`. No riff tool of a session acts on a folder of your machine or on a claim. The owner and admin steps need a person at the CLI. No riff tool does them. | It can send messages to other sessions through the riff tools. |

Real-session test 1792112663.
