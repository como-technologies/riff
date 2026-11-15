//! `riff run ITEM`: one work item in one rootless podman container,
//! through the riff daemon (#712, #716, #717).
//!
//! # Design
//!
//! The host does each riff step with the riff of today: the checks, the
//! claim, the verify request and the release. The daemon runs the
//! container of the session ([`crate::daemon`]). In the container, `riff
//! relay` makes the clone ([`crate::session_clone`], #2139), then starts
//! `claude` ([`crate::session_relay`]), and `claude` does the work
//! (01M4ZKX0NXN7E33VC3PSFGSPDT). The host makes no clone.
//!
//! `riff run` asks the daemon on its control socket: first for the
//! image, then for the session. It shows each event that the daemon
//! sends. With no daemon, `riff run` stops before the claim. On a host
//! that is not Linux, `riff run` starts nothing
//! (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as riff relay and claude -p (container)
//!     participant G as GitHub
//!     H->>D: the image: pull it when it is not here
//!     H->>S: start a worker session, claim ITEM
//!     H->>H: git ls-remote in the main clone: is the branch worktree-ITEM on origin?
//!     H->>D: start SPEC, with no token of the person
//!     D->>S: the token of the session on GitHub: no token of an App, no start
//!     D->>C: start riff relay, with the socket of the session and an empty volume
//!     C->>C: clone from the objects of MAIN/.git and origin-refs, no write: no object copied
//!     C->>G: git fetch origin, then the branch worktree-ITEM
//!     D->>C: the start text, one stream-json line on the socket
//!     C-->>D: the events, one line each, with their numbers
//!     D-->>H: each event, with no secret
//!     C->>S: the riff tools, through the socket and the daemon
//!     C->>G: push the branch, open the pull request
//!     D->>C: close the input after the result: claude ends
//!     D-->>H: the end of the session
//!     H->>G: find the pull request of the branch
//!     H->>S: a verify request to the lead, release ITEM, end the session
//! ```
//!
//! Before the claim, riff checks each fact that it needs: the image of
//! the session (the daemon pulls it when it must), the sign-in of `gh`,
//! the sign-in of `claude`, `HOME` and `USER`, and the milestone of the
//! issue. A failed check takes no claim (01M4Z81B3A324KKE8M8Z71TM4M).
//! The same read of the issue gives the link line of the frame of the
//! body in the start text: `Refs #N` for an issue with a line `Live:` in
//! "Done when", else `Closes #N` ([`Frame`], 01M6WW5WSSRMTBP882FQP3TS2Q).
//!
//! The image is the merged pin: the one that `riff.toml` on the default
//! branch of `origin` pins by its digest ([`session_image::merged`],
//! 01M51STPHXSSZMHF9T7Z85097R), or the one of `--image`, for example a
//! local build. When podman does not have the image, the
//! daemon pulls it before the claim (01M4Z81B5A456HDKW7G0WFGCY6).
//!
//! When `claude` ends, riff finds the open pull request of the branch,
//! sends a verify request to the lead, releases the item and ends the
//! session. A SIGINT or a SIGTERM asks the daemon to stop the container
//! first (01M4Z81B1913NF6HBZ5G38B3Q9).
//!
//! # A session with no GitHub
//!
//! No session gets the token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB):
//! with no token of an App, the daemon starts no container. `riff run
//! ITEM --no-github` runs an item that needs no GitHub
//! (01M6Z87JY83V3KR72E2AG8V8KG), for example the item of the
//! real-session test. The daemon asks for no token, and the container
//! has no `hosts.yml` in the folder `gh` of the session
//! ([`crate::daemon::gh_token::ActsAs::NoGitHub`]). The start text holds
//! the body of the issue ([`Frame::issue`],
//! [`text::run_start_no_github`]): the session commits, and pushes
//! nothing. Its clone fetches nothing from GitHub: it starts from the
//! main clone ([`crate::session_clone`]). So before the start, `riff
//! run` fetches the default branch, and the branch of the item when
//! `origin` has it, into the main clone ([`fetch_for_no_github`],
//! 01M7FDN99KZE54CX17DDEP787V). Its clone goes with its container, so
//! when `claude` ends the relay writes the commits of the branch in a
//! bundle in the folder [`OUT`] of the folder of the session, the one
//! bind of the session for it ([`crate::session_clone::Plan::bundle`]).
//! At the end, `riff run` takes the bundle with its checks
//! ([`take_bundle`]), also when the branch has an open pull request: a
//! copy in the folder of the session ([`bundle_copy`]). It names the copy
//! and the commands that push the branch from a clone of the person
//! ([`text::run_pushed_nothing`]), and exits with 0 when `claude` ended
//! with 0. With an open pull request, it also sends the verify request,
//! as for each session (01M7FDN9C91TDYPQSFBRHVHRAC). When riff refuses the
//! bundle, it says why ([`text::run_bundle_refused`]), and exits with 1.
//!
//! ```mermaid
//! flowchart LR
//!     R["riff run ITEM --no-github"] --> F["the main clone: git fetch origin BASE BRANCH"]
//!     F --> D["the daemon: no token, no GH_TOKEN"]
//!     D --> C["the session commits on its branch"]
//!     C --> B["the relay: out/BRANCH.bundle"]
//!     B --> T["riff run: a copy, BRANCH.bundle, with its checks"]
//!     T --> P["the person, in a clone:<br/>git fetch COPY BRANCH, git push"]
//! ```
//!
//! riff sends no verify request when the issue has the label
//! [`rollout::NO_VERIFY`], or when a lead holds `verify-ITEM`. It shows
//! why ([`no_verify`], 01M5157V0P92EBHF4M7N9K0RGR,
//! 01M5157TYDEHZHDXSC1AHCRDRN). It reads the label and the holds after
//! the session: a hold that the lead makes while the session works
//! counts.
//!
//! ```mermaid
//! flowchart LR
//!     P[an open pull request] --> L{the label no-verify?}
//!     L -->|yes| N[no verify request: riff shows why]
//!     L -->|no| H{a hold of verify-ITEM?}
//!     H -->|yes| N
//!     H -->|no| V[the verify request to the lead]
//! ```
//!
//! `claude` does not end while one of its background tasks runs. After
//! a result, `riff run` shows each task that `claude` waits for
//! ([`View`], 01M4ZSZX9H59T04QSJ01V4F9TE). The start text tells the
//! session to leave no background task (01M4ZSZXBXM6T6MGQ00ACQA66X).
//! When no event comes for the quiet limit after the result, the daemon
//! stops the container ([`daemon::QUIET_LIMIT`]). `riff run` shows why,
//! then does the steps of the end.
//!
//! When the daemon stops before the end of the session, the session
//! runs on: its relay keeps the events. `riff run` waits up to
//! [`DAEMON_WAIT`] for a new daemon, and follows the session again after
//! the last event that it showed ([`Ask::Follow`]). When no daemon comes
//! back, `riff run` says so, and does these steps too
//! (01M4ZKX0F25S284R0N7KZVTHNA).
//!
//! # A restart of riff-server
//!
//! The verify request and the release have the budget [`END_BUDGET`],
//! not the short budget of a command (01M4ZQVVGQYXTAVRB69C7YPC3Y). A
//! call with no reply, or with a fault, tries again with the waits of
//! the link ([`riff_client::link`]) while the budget lasts. So a new instance
//! of riff-server at the end of a run drops neither. A new instance
//! starts, loads the state, and waits 15 s for the lease before it
//! serves; the old one replies 503 in that time. A cold start or a long
//! load can take this past the short budget. The keep-alive of the
//! session runs until the release, so the claim stays while the calls
//! wait. The steps before the work keep the short budget: when they
//! fail, `riff run` started nothing.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     H->>S: the verify request
//!     Note over S: a new instance starts: no reply
//!     H->>H: no reply in the limit of the try: a new connection, a wait
//!     H->>S: the verify request again, with the same call ID
//!     S-->>H: posted
//!     H->>S: release ITEM
//!     S-->>H: released
//! ```
//!
//! # A signal in the end steps
//!
//! A SIGINT or a SIGTERM stops the wait of the verify request, of the
//! release and of the end of the session (01M4ZYFJFTX9ZEX7QT4QH7E9T6).
//! Tokio keeps its handler of a signal to the end of the process, so a
//! signal after the session does not end riff by itself: riff runs each
//! end step against the signals. After a signal, riff sends no more
//! step, names each step that it did not send, and ends with the code 1.
//! riff-server frees the claim when the session stops, as for each other
//! session.
//!
//! One set of signal streams listens from the start of the session to
//! the last end step (01M51JG5Y47184401EWT4HVEG0). A stream keeps a
//! signal that comes while riff waits for none, for example while `gh`
//! finds the pull request, so that signal stops the first end step. A
//! new stream would not see it. A second signal while the container
//! stops also stays for the end steps.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     H->>S: the verify request
//!     Note over S: no reply
//!     P->>H: Ctrl-C (SIGINT) or SIGTERM
//!     H->>H: stop the wait, send no more step
//!     H-->>P: the steps that riff did not send, the exit code 1
//!     Note over S: the session stops: the claim is free
//! ```
//!
//! # The folder of a session
//!
//! Each session has a folder of its own in [`sessions_dir`]: `ITEM-ID8`,
//! where `ID8` is the start of the session ID (01M4Z81ARSTAK8HR96S084SDFD).
//! The host holds the lock `ITEM-ID8.lock` beside it from before the
//! start to its end ([`crate::daemon::sweep::hold`]): the sweep of the
//! daemon keeps the folder while the start waits for room, before the
//! daemon has a record of the session.
//!
//! | Path | Holds |
//! |---|---|
//! | `repo` | The repository `OWNER/REPO` of the session. The host writes it before the start, and the sweep reads it ([`crate::daemon::sweep::note_repo`], 01M7DZ42EVHAT8N5MZEV97G3DW). |
//! | `clone/` | No folder of the host: the clone of the session is at this path in its container only, an empty volume of the container where the relay clones ([`crate::session_clone`], 01M7EY3P0Q8TQF5QRTRDWCYCTQ). It goes with the container. The host runs no git in the folder of a session ([`crate::host_git`]). |
//! | `origin-refs` | The branches of `origin` of the main clone, which the host writes before each start ([`refs_in`], [`crate::session_clone::write_refs`], 01M7FMJJB0TZM8875T7GQZAJEA). The container sees the file at the same path, with no write. |
//! | `out/` | With no GitHub only: the bundle of the branch of the session, which the relay writes when `claude` ends ([`crate::session_clone::Plan::bundle_file`], 01M7EY3WHQQP9330CZ736HK0FJ). The session can write it. |
//! | `BRANCH.bundle` | With no GitHub only: the copy of the bundle that `riff run` checked ([`bundle_copy`], [`take_bundle`]). No session sees it, and the person fetches from it. |
//! | `claude/` | The own Claude folder of the session ([`claude_folder`]): the first-run answers of `claude` with the trust of the clone, no attribution line of `claude`, and the auto mode settings ([`settings`]). |
//! | `events.jsonl` | Each event of `claude`, with no secret ([`Scrub`]). The daemon writes it. |
//!
//! # The spec of the container
//!
//! [`spec`] makes the [`SessionSpec`] of decision 10 of #699 from the
//! [`Facts`] of the host. [`crate::podman::run_args`] makes the
//! arguments of `podman`. riff builds no shell string: each value is one
//! argument (01M4Z60B8KDXD190RWC7D184Q3).
//!
//! | Part | Value | Requirement |
//! |---|---|---|
//! | The user | `--userns=keep-id` | 01M4Z60B7QDWEFQ96H448RE9N2 |
//! | The init | `--init`: PID 1 reaps each orphan process | 01M51117K3WXK35CE67KPRFMA8 |
//! | The home | a tmpfs at the path of `HOME`, `HOME`, `USER`, `LOGNAME` | 01M4Z60B7SHAMXXFK15BE3KZSP, 01M4Z60B7S8V9KKZPNBCQ6M99N |
//! | The folders of the home | a tmpfs of the session user at each folder above a bind, and at `~/.cache`, `~/.local`, `~/.local/share` and `~/.local/state` ([`SessionSpec::home_folders`]) | 01M748WY71EFVZR6SG5CCEQYE0 |
//! | The size of each tmpfs of the home | `tmpfs-size=1g` ([`crate::runtime::HOME_TMPFS_GIB`]): a larger write fails with no space left, and does not fill the memory | 01M7DYR99QK609NF61GGCN75QQ |
//! | The clone | an empty volume of the container at the path of the clone, the working folder ([`crate::runtime::Source::Empty`]), and the plan of the clone in `RIFF_CLONE` ([`crate::session_clone::Plan`]): the relay clones there. No git tree of the host has write | 01M4Z60B7PTJMB37578K8FV22Q, 01M7EY3P0Q8TQF5QRTRDWCYCTQ |
//! | The git dir of the main clone | its `objects/` only ([`crate::session_clone::OBJECTS`]): no write, at the same path. No config, no ref, no hook, no log, no worktree | 01M7EY3P0Q8TQF5QRTRDWCYCTQ, 01M7FMJJB0TZM8875T7GQZAJEA |
//! | The branches of `origin` of the main clone | the file `origin-refs` that the host wrote in the folder of the session before the start ([`refs_in`], [`crate::session_clone::write_refs`]): no write, at the same path. The relay clones from it and from the objects ([`crate::session_clone`]) | 01M7FMJJB0TZM8875T7GQZAJEA |
//! | `~/.gitconfig` | no write, when it exists. No `~/.config/gh` | 01M4Z60B7QDWEFQ96H448RE9N2, 01M6WPMP035A93SM6SAJGS6MTC |
//! | The sign-in of `gh` | no `GH_TOKEN`, never the token of the person: the daemon adds the folder of the token of the session and `GH_CONFIG_DIR` ([`crate::daemon::gh_token`]) | 01M6Z87A04EXTT1FFJSJ1EM6TB, 01M6WPMNXN1BSBEH74TG5DDRTF |
//! | No signature | `commit.gpgsign=false` by `GIT_CONFIG_*` ([`git_config`]) | 01M4Z60B89G8YNVMTEPGX7FQCV |
//! | The author of a commit | `user.name` the login of the bot and `user.email` its noreply address, by `GIT_CONFIG_*`: the daemon adds them with the token of an App ([`crate::daemon::gh_token::give_author`]). With no token, the identity of `~/.gitconfig` | 01M6Y6B8VBM3TKC3YHZKSA4PFZ, 01M6WZ2C8JXEA888XYMRXTS17V |
//! | The token of git | the credential helper `!gh auth git-credential` of `https://github.com` by `GIT_CONFIG_*`, after an empty one: no helper of `~/.gitconfig` | 01M6WPMNXN1BSBEH74TG5DDRTF |
//! | The Claude folder | with write | 01M4Z60B86N999JRN7Z1G52PG4 |
//! | The sign-in of `claude` | `-e CLAUDE_CODE_OAUTH_TOKEN`: the name only, when a token is set ([`crate::claude_token`]); else the sign-in file of the host with no write | 01M58B7H2HMYT6VTY0B8T2G0KW, 01M4Z60B869HWDTZMSYPVYACD8 |
//! | The env and the volumes of the repository | `[session.env]` and `[session.volumes]` of the merged `riff.toml` ([`crate::session_setup`]). riff sets no variable and no volume of a language or a tool | 01M5SMVVC4GQS310CVK3H9CMQ7, 01M5SMW28KT5D5HPRH7KAZ8TVC |
//! | The caps | `--memory` and `--cpus`: the defaults, and the daemon puts the caps of the host at the start ([`crate::room::Limit::caps`]) | 01M4Z60B7XT7462XMV49GJTVSJ, 01M56Q4AQ86V99RC8CA1KGCMSZ |
//! | `claude` | `--model` of the merged `riff.toml`, `opus` by default ([`Setup::claude`]), `-p` in stream-json, the riff session ID, the auto mode | 01M6VAQWECJ6N6VJF66KCMNYD1, 01M4Z60B82FHN2YG9YJHW8XZX1, 01M4Z81ATZK3FX9GRFTM236FE6 |
//! | The socket of the session | the daemon adds it: its folder at `/run/riff` with write, and `RIFF_SOCKET` | 01M4ZDTVSP0T1YENKAWGKA114D |

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{HoldInfo, Kind, StartReason, WORKER_ALIVE_EVERY};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::unix::{SignalKind, signal};

use crate::daemon::{self, Ask, Ended, Say};
use crate::runtime::{Bind, Env, SessionSpec, Source};
use crate::session_clone::{Branch, Checkout, Origin, Plan};
use crate::session_setup::Setup;
use crate::{dropped, git_credential, home, identity, pr, rollout, session_image, text};
use riff_client::api::{self, Api};

/// riff starts no session while the access token of `claude` has less
/// than this many minutes left (01M4Z60B869HWDTZMSYPVYACD8).
pub const MIN_MINUTES: i64 = 60;

/// The text that takes the place of a secret.
pub const HIDDEN: &str = "[secret]";

/// The budget of the verify request and of the release of `riff run`
/// (01M4ZQVVGQYXTAVRB69C7YPC3Y): longer than a restart of riff-server.
///
/// ```
/// use std::time::Duration;
/// use riff_client::link::SHORT_BUDGET;
///
/// assert_eq!(riff::run::END_BUDGET, Duration::from_secs(600));
/// assert!(riff::run::END_BUDGET > SHORT_BUDGET);
/// ```
pub const END_BUDGET: Duration = Duration::from_secs(600);

/// The facts of the host and of the session that make a [`SessionSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The riff session ID. `claude` takes it as its own session ID.
    pub session: String,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The image.
    pub image: String,
    /// The home of the person.
    pub home: PathBuf,
    /// The user name of the person.
    pub user: String,
    /// The plan of the clone of the session, which the relay makes in the
    /// container ([`crate::session_clone`]): the host has no folder at its
    /// path. The spec binds the objects of its main clone and its file of
    /// refs with no write, and with no GitHub the folder of its bundle
    /// ([`Plan::bundle_file`]).
    pub plan: Plan,
    /// The own Claude folder of the session.
    pub claude: PathBuf,
    /// How `claude` signs in.
    pub sign_in: ClaudeSignIn,
    /// `~/.gitconfig`, when it exists.
    pub gitconfig: Option<PathBuf>,
    /// The model, the env and the volumes of the merged `riff.toml`
    /// (01M6VAQWECJ6N6VJF66KCMNYD1, 01M5SMVVC4GQS310CVK3H9CMQ7).
    pub setup: Setup,
}

/// How `claude` in a container signs in. It holds no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeSignIn {
    /// The long-lived token in [`crate::claude_token::VAR`]
    /// (01M58B7H2HMYT6VTY0B8T2G0KW).
    Token,
    /// The sign-in file of `claude` on the host, with no write
    /// (01M4Z60B869HWDTZMSYPVYACD8).
    File(PathBuf),
}

/// The [`SessionSpec`] of a session: decision 10 of #699, with the
/// facts of #707.
///
/// ```
/// use riff::run::{ClaudeSignIn, Facts, spec};
/// use riff::runtime::{Env, Source};
/// use riff::session_clone::{Checkout, Origin, Plan};
///
/// let branch = |no_github| Checkout::Branch { name: "worktree-issue-12".parse().unwrap(), earlier: false, no_github };
/// let facts = Facts {
///     session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
///     item: "issue-12".into(),
///     image: "localhost/riff-session:latest".into(),
///     home: "/home/mike".into(),
///     user: "mike".into(),
///     plan: Plan {
///         path: "/data/riff/sessions/issue-12-1a2b3c4d/clone".into(),
///         origin: Origin::github("o/r")?,
///         main: "/src/riff/.git".into(),
///         refs: "/data/riff/sessions/issue-12-1a2b3c4d/origin-refs".into(),
///         base: "main".parse()?,
///         checkout: branch(false),
///     },
///     claude: "/data/riff/sessions/issue-12-1a2b3c4d/claude".into(),
///     sign_in: ClaudeSignIn::File("/home/mike/.claude/.credentials.json".into()),
///     gitconfig: Some("/home/mike/.gitconfig".into()),
///     setup: Default::default(),
/// };
/// let spec = spec(&facts);
/// let plan = &facts.plan;
/// assert_eq!(spec.name, "riff-issue-12-1a2b3c4d");
/// assert_eq!(spec.workdir, plan.path);
/// // The clone: an empty folder of the container, and no folder of the
/// // host. The relay clones there from the plan.
/// let clone = spec.binds.iter().find(|b| b.target == plan.path).unwrap();
/// assert_eq!((&clone.source, clone.write), (&Source::Empty, true));
/// assert_eq!(Plan::of_spec(&spec).as_ref(), Some(plan));
/// // The objects of the main clone and the list of its refs: at the same
/// // path, with no write. No other part of the main clone: no config, no
/// // ref, no hook, no log and no worktree.
/// for at in [plan.main.join("objects"), plan.refs.clone()] {
///     let bind = spec.binds.iter().find(|b| b.target == at).unwrap();
///     assert_eq!((&bind.source, bind.write), (&Source::Path(at.clone()), false));
/// }
/// let of_main = spec.binds.iter().filter(|b| b.target.starts_with(&plan.main)).count();
/// assert_eq!(of_main, 1);
/// // With no GitHub, the folder of the bundle has write.
/// let offline = Plan { checkout: branch(true), ..plan.clone() };
/// let offline = riff::run::spec(&Facts { plan: offline, ..facts.clone() });
/// let out = offline.binds.iter().find(|b| b.target.ends_with("out")).unwrap();
/// assert_eq!(out.target, std::path::Path::new("/data/riff/sessions/issue-12-1a2b3c4d/out"));
/// assert_eq!((out.source == Source::Path(out.target.clone()), out.write), (true, true));
/// // No token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB): the daemon adds
/// // the folder of the token of the session.
/// assert!(riff::daemon::gh_token::check_env(&spec).is_ok());
/// // No bind of the config of gh of the host (01M6WPMP035A93SM6SAJGS6MTC).
/// assert!(!spec.binds.iter().any(|b| b.target.ends_with(".config/gh")));
/// // git gets its token from gh (01M6WPMNXN1BSBEH74TG5DDRTF).
/// let helper = Env::Value("GIT_CONFIG_VALUE_2".into(), "!gh auth git-credential".into());
/// assert!(spec.env.contains(&helper));
/// assert!(spec.command.windows(2).any(|w| w == ["--session-id", facts.session.as_str()]));
/// // With a token, the name of the variable and no sign-in file.
/// let token = riff::run::spec(&Facts { sign_in: ClaudeSignIn::Token, ..facts.clone() });
/// assert!(token.env.contains(&Env::Host("CLAUDE_CODE_OAUTH_TOKEN".into())));
/// assert!(!token.binds.iter().any(|b| b.target.ends_with(".credentials.json")));
/// assert!(!spec.env.contains(&Env::Host("CLAUDE_CODE_OAUTH_TOKEN".into())));
///
/// // riff sets no variable and no volume of a language or a tool
/// // (01M5SMW28KT5D5HPRH7KAZ8TVC): the repository gives them.
/// let names: Vec<&str> = spec.env.iter().map(|e| match e {
///     Env::Value(n, _) | Env::Host(n) => n.as_str(),
/// }).collect();
/// assert!(!names.iter().any(|n| ["CARGO", "RUST", "SCCACHE"].iter().any(|t| n.contains(t))));
/// assert!(spec.binds.iter().all(|b| !matches!(b.source, Source::Volume(_))));
///
/// // The env and the volumes of the riff.toml come after, with `$HOME` in place.
/// let setup = riff::session_setup::Setup::of_config(
///     "[session.env]\nCARGO_HOME = \"$HOME/.cargo\"\n[session.volumes]\nriff-cargo = \"$HOME/.cargo\"\n",
///     "riff.toml",
/// )?;
/// let with = riff::run::spec(&Facts { setup, ..facts.clone() });
/// assert!(with.env.contains(&Env::Value("CARGO_HOME".into(), "/home/mike/.cargo".into())));
/// assert!(with.binds.iter().any(|b| {
///     b.source == Source::Volume("riff-cargo".into()) && b.target == std::path::Path::new("/home/mike/.cargo")
/// }));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn spec(facts: &Facts) -> SessionSpec {
    let home = &facts.home;
    let path = |p: &Path, write: bool| Bind {
        source: Source::Path(p.to_owned()),
        target: p.to_owned(),
        write,
    };
    let claude_dir = home.join(".claude");
    // The session clones in its container: no git tree of the host has
    // write (#2139).
    let plan = &facts.plan;
    let clone = Bind {
        source: Source::Empty,
        target: plan.path.clone(),
        write: true,
    };
    // Of the main clone, only its objects: its config can hold a token of
    // the person. The list of its refs is a file that the host wrote
    // (01M7FMJJB0TZM8875T7GQZAJEA).
    let objects = plan.main.join(crate::session_clone::OBJECTS);
    let mut binds = vec![clone, path(&objects, false), path(&plan.refs, false)];
    binds.extend(
        plan.bundle_file()
            .as_deref()
            .and_then(Path::parent)
            .map(|out| path(out, true)),
    );
    binds.extend(facts.gitconfig.iter().map(|p| Bind {
        source: Source::Path(p.clone()),
        target: home.join(".gitconfig"),
        write: false,
    }));
    binds.push(Bind {
        source: Source::Path(facts.claude.clone()),
        target: claude_dir.clone(),
        write: true,
    });
    if let ClaudeSignIn::File(credentials) = &facts.sign_in {
        binds.push(Bind {
            source: Source::Path(credentials.clone()),
            target: claude_dir.join(".credentials.json"),
            write: false,
        });
    }
    binds.extend(facts.setup.binds(home));
    let value = |name: &str, value: &str| Env::Value(name.into(), value.into());
    let shown = |p: &Path| p.to_string_lossy().into_owned();
    let mut env = vec![
        value("HOME", &shown(home)),
        value("USER", &facts.user),
        value("LOGNAME", &facts.user),
    ];
    git_config(&mut env, "commit.gpgsign", "false");
    // git asks gh for the token of the session: an empty helper first
    // drops each helper of `~/.gitconfig` (01M6WPMNXN1BSBEH74TG5DDRTF).
    git_config(&mut env, GIT_HELPER, "");
    git_config(&mut env, GIT_HELPER, "!gh auth git-credential");
    env.push(value("CLAUDE_CONFIG_DIR", &shown(&claude_dir)));
    env.push(value(crate::session_clone::VAR, &plan.to_var()));
    if facts.sign_in == ClaudeSignIn::Token {
        env.push(Env::Host(crate::claude_token::VAR.into()));
    }
    env.extend(facts.setup.env(home));
    let mut command = facts.setup.claude();
    command.extend(
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--session-id",
            &facts.session,
            "--permission-mode",
            "auto",
        ]
        .map(String::from),
    );
    let mut spec = SessionSpec {
        name: format!("riff-{}-{}", facts.item, short(&facts.session)),
        image: facts.image.clone(),
        home: home.clone(),
        workdir: plan.path.clone(),
        binds,
        env,
        memory: String::new(),
        cpus: 0,
        command,
    };
    crate::room::Limit::default().caps(&mut spec);
    spec
}

/// The folder of the bundle of a session with no GitHub, in the folder
/// of the session ([`crate::session_clone::Plan::bundle_file`]). The
/// session can write it.
pub const OUT: &str = "out";

/// The path of the clone of a session in its container, in the folder
/// `folder` of the session or of the lead: `clone/`. The host has no
/// folder there ([`crate::session_clone`]).
///
/// ```
/// let folder = std::path::Path::new("/d/riff/sessions/issue-12-1a2b3c4d");
/// assert_eq!(riff::run::clone_in(folder), folder.join("clone"));
/// assert_eq!(riff::run::claude_in(folder), folder.join("claude"));
/// assert_eq!(riff::run::refs_in(folder), folder.join("origin-refs"));
/// ```
pub fn clone_in(folder: &Path) -> PathBuf {
    folder.join("clone")
}

/// The own Claude folder of a session in the folder `folder` of the
/// session or of the lead: `claude/` ([`claude_folder`]).
pub fn claude_in(folder: &Path) -> PathBuf {
    folder.join("claude")
}

/// The file of the branches of `origin` of the main clone in the folder
/// `folder` of the session or of the lead ([`crate::session_clone::REFS`]).
/// The host writes it before each start of the container
/// ([`crate::session_clone::write_refs`]). No container can write
/// `folder`: the spec binds only folders in it with write.
pub fn refs_in(folder: &Path) -> PathBuf {
    folder.join(crate::session_clone::REFS)
}

/// The copy of the bundle of the branch `branch` that `riff run` checked
/// ([`take_bundle`] of [`crate::session_clone::Plan::bundle_file`]), in
/// the folder `folder` of the session: `BRANCH.bundle`. No session sees
/// it, and the person fetches from it.
///
/// ```
/// let folder = std::path::Path::new("/d/riff/sessions/issue-12-1a2b3c4d");
/// assert_eq!(
///     riff::run::bundle_copy(folder, &"worktree-issue-12".parse()?),
///     folder.join("worktree-issue-12.bundle")
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn bundle_copy(folder: &Path, branch: &Branch) -> PathBuf {
    folder.join(format!("{branch}.bundle"))
}

/// The key of the credential helper of git for GitHub.
pub const GIT_HELPER: &str = "credential.https://github.com.helper";

/// Adds the git setting `key` with `value` to `env` with `GIT_CONFIG_*`:
/// the next `GIT_CONFIG_KEY_N` and `GIT_CONFIG_VALUE_N`, and
/// `GIT_CONFIG_COUNT` one more. Each git call of the container gets it,
/// over each git config file.
///
/// ```
/// use riff::run::git_config;
/// use riff::runtime::Env;
///
/// let mut env = vec![Env::Value("HOME".into(), "/home/mike".into())];
/// git_config(&mut env, "commit.gpgsign", "false");
/// git_config(&mut env, "user.name", "riff-acme[bot]");
/// let value = |n: &str, v: &str| Env::Value(n.into(), v.into());
/// assert_eq!(
///     env,
///     [
///         value("HOME", "/home/mike"),
///         value("GIT_CONFIG_COUNT", "2"),
///         value("GIT_CONFIG_KEY_0", "commit.gpgsign"),
///         value("GIT_CONFIG_VALUE_0", "false"),
///         value("GIT_CONFIG_KEY_1", "user.name"),
///         value("GIT_CONFIG_VALUE_1", "riff-acme[bot]"),
///     ]
/// );
/// ```
pub fn git_config(env: &mut Vec<Env>, key: &str, value: &str) {
    const COUNT: &str = "GIT_CONFIG_COUNT";
    let count = env.iter_mut().find_map(|e| match e {
        Env::Value(name, count) if name == COUNT => Some(count),
        _ => None,
    });
    let n = match count {
        Some(count) => {
            let n: u32 = count.parse().unwrap_or(0);
            *count = (n + 1).to_string();
            n
        }
        None => {
            env.push(Env::Value(COUNT.into(), "1".into()));
            0
        }
    };
    env.push(Env::Value(format!("GIT_CONFIG_KEY_{n}"), key.into()));
    env.push(Env::Value(format!("GIT_CONFIG_VALUE_{n}"), value.into()));
}

/// The first 8 characters of a session ID.
///
/// ```
/// assert_eq!(riff::run::short("1a2b3c4d-0000"), "1a2b3c4d");
/// assert_eq!(riff::run::short("ab"), "ab");
/// ```
pub fn short(session: &str) -> &str {
    session.get(..8).unwrap_or(session)
}

/// The branch of the session of `item`: the branch name of a worker on
/// the host, so that its tools find the work (01M4Z81AX3ND5RX7V1GEY4HMF5).
///
/// ```
/// assert_eq!(riff::run::branch("issue-12"), "worktree-issue-12");
/// ```
pub fn branch(item: &str) -> String {
    format!("{}{item}", dropped::BRANCH)
}

/// The variable that names the data folder of riff in place of the
/// data folder of the machine (01M53YP97S9BDMFBN0X5G444MY). Only the
/// real-session test sets it, so that its riff daemon runs beside the
/// daemon of the machine, with a folder and a socket of its own. It
/// moves only the data folder: the settings, the local files and the
/// secrets stay where they are. An empty value counts as unset.
///
/// ```
/// assert_eq!(riff::run::DATA_VAR, "RIFF_DATA");
/// ```
pub const DATA_VAR: &str = "RIFF_DATA";

/// The data folder of riff: `RIFF_DATA` ([`DATA_VAR`]), else
/// `RIFF_HOME`, else `XDG_DATA_HOME/riff`, else
/// `HOME/.local/share/riff`. It holds the session folders and the folder
/// of the daemon ([`crate::daemon::dir`]).
///
/// ```
/// use riff::run::data_dir_from;
/// use std::path::Path;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(data_dir_from(some("/t"), some("/r"), some("/d"), some("/h"))?, Path::new("/t"));
/// assert_eq!(data_dir_from(some(""), some("/r"), some("/d"), some("/h"))?, Path::new("/r"));
/// assert_eq!(data_dir_from(None, None, some("/d"), some("/h"))?, Path::new("/d/riff"));
/// assert_eq!(
///     data_dir_from(None, None, some(""), some("/h"))?,
///     Path::new("/h/.local/share/riff")
/// );
/// assert!(data_dir_from(None, None, None, None).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn data_dir_from(
    riff_data: Option<OsString>,
    riff_home: Option<OsString>,
    data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    if let Some(dir) = home::dir_from(riff_data).or_else(|| home::dir_from(riff_home)) {
        return Ok(dir);
    }
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    match (set(data_home), set(home)) {
        (Some(data), _) => Ok(data.join("riff")),
        (None, Some(home)) => Ok(home.join(".local/share/riff")),
        (None, None) => bail!("set HOME or XDG_DATA_HOME"),
    }
}

/// [`data_dir_from`] with the values of this process.
pub fn data_dir() -> Result<PathBuf> {
    data_dir_from(
        std::env::var_os(DATA_VAR),
        std::env::var_os(home::VAR),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

/// True when the data folder is the data folder of the machine: neither
/// `RIFF_DATA` nor `RIFF_HOME` names another one. Only a daemon of the
/// data folder of the machine can be the daemon of the unit
/// (01M53YP9AHPZ9YPNQFJ1SYZGWF).
///
/// ```
/// use riff::run::machine_data_from;
///
/// assert!(machine_data_from(None, None));
/// assert!(machine_data_from(Some("".into()), Some("".into())));
/// assert!(!machine_data_from(Some("/t".into()), None));
/// assert!(!machine_data_from(None, Some("/r".into())));
/// ```
pub fn machine_data_from(riff_data: Option<OsString>, riff_home: Option<OsString>) -> bool {
    home::dir_from(riff_data).is_none() && home::dir_from(riff_home).is_none()
}

/// [`machine_data_from`] with the values of this process.
pub fn machine_data() -> bool {
    machine_data_from(std::env::var_os(DATA_VAR), std::env::var_os(home::VAR))
}

/// The folder of the session folders: `sessions` in [`data_dir`].
///
/// ```
/// use riff::run::sessions_dir_from;
/// use std::path::Path;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(
///     sessions_dir_from(some("/t"), some("/r"), some("/d"), some("/h"))?,
///     Path::new("/t/sessions")
/// );
/// assert_eq!(
///     sessions_dir_from(None, some("/r"), some("/d"), some("/h"))?,
///     Path::new("/r/sessions")
/// );
/// assert_eq!(
///     sessions_dir_from(None, None, some("/d"), some("/h"))?,
///     Path::new("/d/riff/sessions")
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sessions_dir_from(
    riff_data: Option<OsString>,
    riff_home: Option<OsString>,
    data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    Ok(data_dir_from(riff_data, riff_home, data_home, home)?.join("sessions"))
}

/// [`sessions_dir_from`] with the values of this process.
pub fn sessions_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("sessions"))
}

/// The `settings.json` of the own Claude folder: `claude` adds no
/// attribution line to a commit or a pull request, so the trailers end
/// each one (01M4ZD48DHAKJ1EHX6QHYAQ7ZM). It has the rules of the session
/// place of the permission table for the role `role`: the allow rules, so
/// the auto mode check does not refuse a step of the start text, and the
/// deny rules, so the role does no step that it must not do
/// ([`crate::permissions::session_rules`], 01M5084K4YQCZF20XA0P2485SG,
/// 01M6FZXZ9267YKMH97E6A2Y2EP).
/// It has the auto mode settings of `auto`: no `/auto-mode-setup`, and
/// the places of riff work ([`crate::permissions::auto_mode`],
/// 01M516HEX3GVTN9K2QS12A4PG3). It has the hooks `PermissionDenied`,
/// `UserPromptSubmit` and `Stop` ([`session_hooks`]), and the renderer
/// [`TUI`] (01M6XVBRT2T838TX53SSXR6HRH). It is a setting of riff for its
/// sessions, not a copy of a setting of the person.
///
/// ```
/// use riff::permissions::{Role, Trust, auto_mode, session_rules};
///
/// let auto = Trust { owner: Some("acme"), server: Some("http://h:7878"), sources: &[] };
/// let settings = riff::run::settings(&auto, Role::Worker);
/// assert_eq!(settings["includeCoAuthoredBy"], false);
/// assert_eq!(settings["tui"], "default");
/// assert_eq!(settings["hooks"], riff::run::session_hooks());
/// let allow = settings["permissions"]["allow"].as_array().unwrap();
/// assert!(allow.iter().any(|r| r == "Bash(gh issue view *)"));
/// assert!(!allow.iter().any(|r| r == "Bash(gh issue edit *)"));
/// assert_eq!(allow.len(), session_rules(Role::Worker).allow.len());
/// let deny = settings["permissions"]["deny"].as_array().unwrap();
/// assert!(deny.iter().any(|r| r == "Bash(gh*issue edit*--body*)"));
/// assert_eq!(deny.len(), session_rules(Role::Worker).deny.len());
///
/// // The lead edits the body of an issue: it has no deny rule for it.
/// let lead = riff::run::settings(&auto, Role::Lead);
/// let allow = lead["permissions"]["allow"].as_array().unwrap();
/// assert!(allow.iter().any(|r| r == "Bash(gh issue edit *)"));
/// assert!(lead["permissions"].get("deny").is_none());
///
/// assert_eq!(settings["skillOverrides"], auto_mode(&auto)["skillOverrides"]);
/// assert_eq!(settings["autoMode"], auto_mode(&auto)["autoMode"]);
/// ```
pub fn settings(auto: &crate::permissions::Trust, role: crate::permissions::Role) -> Value {
    let rules = crate::permissions::session_rules(role);
    let mut permissions = json!({ "allow": rules.allow });
    if !rules.deny.is_empty() {
        permissions["deny"] = json!(rules.deny);
    }
    let mut settings = json!({
        "includeCoAuthoredBy": false,
        "permissions": permissions,
        "hooks": session_hooks(),
        "tui": TUI,
    });
    if let (Some(settings), Value::Object(auto)) = (
        settings.as_object_mut(),
        crate::permissions::auto_mode(auto),
    ) {
        settings.extend(auto);
    }
    settings
}

/// The hooks of each container session: `PermissionDenied` runs
/// `/opt/riff/riff relay denied` for each tool (01M57XY3Q6HEYQG27FE0CEQ7HB,
/// [`crate::denied`]), `UserPromptSubmit` runs `/opt/riff/riff relay
/// prompt` (01M5GVQQ8NK3RD9CM3VTGXEHZQ), and `Stop` runs `/opt/riff/riff
/// relay idle` at the end of each turn (01M6WBX2FQEHW92Y1A5HM4H8BM).
///
/// A hook of riff never blocks `claude` (01M6WFN93ZDCSAVRTNYDX4MRJR).
/// `claude` runs the command of a hook in a shell, and each command ends
/// with `|| true`. So a hook exits with 0 also on an older riff, for
/// example after a rollback, that does not know the subcommand and exits
/// with 2. The output of the hook stays.
///
/// ```
/// let hooks = riff::run::session_hooks();
/// let hook = &hooks["PermissionDenied"][0];
/// assert_eq!(hook["matcher"], "*");
/// assert_eq!(hook["hooks"][0]["command"], "/opt/riff/riff relay denied || true");
/// let hook = &hooks["UserPromptSubmit"][0];
/// assert_eq!(hook["hooks"][0]["command"], "/opt/riff/riff relay prompt || true");
/// let hook = &hooks["Stop"][0];
/// assert_eq!(hook["hooks"][0]["command"], "/opt/riff/riff relay idle || true");
/// ```
pub fn session_hooks() -> Value {
    let hook = |subcommand: &str| {
        json!({
            "type": "command",
            "command": format!("{} relay {subcommand} || true", daemon::RELAY_BIN),
        })
    };
    json!({
        "PermissionDenied": [{ "matcher": "*", "hooks": [hook("denied")] }],
        "UserPromptSubmit": [{ "hooks": [hook("prompt")] }],
        "Stop": [{ "hooks": [hook("idle")] }],
    })
}

/// True when each hook of the group `group` of a settings file runs
/// `riff relay` at [`daemon::RELAY_BIN`]: a group of this riff or of an
/// older one.
fn of_riff(group: &Value) -> bool {
    let relay = format!("{} relay ", daemon::RELAY_BIN);
    group["hooks"].as_array().is_some_and(|hooks| {
        !hooks.is_empty()
            && hooks.iter().all(|hook| {
                hook["command"]
                    .as_str()
                    .is_some_and(|command| command.starts_with(&relay))
            })
    })
}

/// The settings text `text` with the hooks of [`session_hooks`], or
/// `None` when it has them. A group of riff that is not one of them goes,
/// as the hook of an older riff: the new group takes its place
/// (01M6WFN93ZDCSAVRTNYDX4MRJR). Each other key and each other hook
/// stays.
///
/// ```
/// let text = r#"{"model":"opus","hooks":{"SessionEnd":[]}}"#;
/// let new = riff::run::with_session_hooks(text)?.expect("the hooks are new");
/// assert_eq!(riff::run::with_session_hooks(&new)?, None, "the hooks are there");
/// let settings: serde_json::Value = serde_json::from_str(&new)?;
/// assert_eq!(settings["model"], "opus");
/// assert_eq!(settings["hooks"]["SessionEnd"], serde_json::json!([]));
/// let hooks = riff::run::session_hooks();
/// assert_eq!(settings["hooks"]["PermissionDenied"], hooks["PermissionDenied"]);
/// assert_eq!(settings["hooks"]["UserPromptSubmit"], hooks["UserPromptSubmit"]);
/// assert_eq!(settings["hooks"]["Stop"], hooks["Stop"]);
///
/// // The hook of an older riff gets the new command.
/// let old = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/opt/riff/riff relay idle"}]}]}}"#;
/// let new = riff::run::with_session_hooks(old)?.expect("the hook is old");
/// let settings: serde_json::Value = serde_json::from_str(&new)?;
/// assert_eq!(settings["hooks"]["Stop"], hooks["Stop"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_session_hooks(text: &str) -> Result<Option<String>> {
    let mut settings: Value = serde_json::from_str(text).context("the settings are no JSON")?;
    let Some(root) = settings.as_object_mut() else {
        bail!("the settings are no JSON object");
    };
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        bail!("`hooks` in the settings is no object");
    };
    let mut changed = false;
    let Value::Object(wanted) = session_hooks() else {
        unreachable!("the session hooks are an object");
    };
    for (event, list) in wanted {
        let have = hooks.entry(event.as_str()).or_insert_with(|| json!([]));
        let Some(have) = have.as_array_mut() else {
            bail!("`hooks.{event}` in the settings is no list");
        };
        let list = list.as_array().cloned().unwrap_or_default();
        let before = have.len();
        have.retain(|group| !of_riff(group) || list.contains(group));
        changed |= have.len() != before;
        for group in list {
            if !have.contains(&group) {
                have.push(group);
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string_pretty(&settings)?))
}

/// The renderer of `claude` in each session, the key `tui` of its
/// settings: `default`, the classic renderer of the main screen
/// (01M6XVBRT2T838TX53SSXR6HRH).
///
/// The cause of the mouse reports in the terminal of an attach: with no
/// `tui`, `claude` can pick its fullscreen renderer by itself, for
/// example in a new Claude folder, as for a new install. That renderer
/// takes the alternate screen and turns on the mouse reports `?1000`,
/// `?1002`, `?1003` and `?1006`. Then the terminal of the person does not
/// select text, and after a detach its shell prints each move of the
/// mouse as text. The classic renderer turns on no mouse report.
pub const TUI: &str = "default";

/// The settings text `text` with the key `tui` of [`TUI`], or `None`
/// when it has a `tui`. A `tui` that is there stays: the person chose it,
/// for example with `/tui fullscreen` in a takeover.
///
/// ```
/// let new = riff::run::with_tui(r#"{"model":"opus"}"#)?.expect("no key");
/// assert_eq!(riff::run::with_tui(&new)?, None, "the key is there");
/// let settings: serde_json::Value = serde_json::from_str(&new)?;
/// assert_eq!(settings["tui"], "default");
/// assert_eq!(settings["model"], "opus");
///
/// // The choice of the person stays.
/// assert_eq!(riff::run::with_tui(r#"{"tui":"fullscreen"}"#)?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_tui(text: &str) -> Result<Option<String>> {
    let mut settings: Value = serde_json::from_str(text).context("the settings are no JSON")?;
    let Some(root) = settings.as_object_mut() else {
        bail!("the settings are no JSON object");
    };
    if root.contains_key("tui") {
        return Ok(None);
    }
    root.insert("tui".into(), json!(TUI));
    Ok(Some(serde_json::to_string_pretty(&settings)?))
}

/// The name of the Claude config in the Claude folder of a session.
pub const CLAUDE_JSON: &str = ".claude.json";

/// The name of the settings in the Claude folder of a session.
pub const SETTINGS_JSON: &str = "settings.json";

/// Writes the own Claude folder `dir` of a session: the lead, a session
/// of `riff run` and a session of the daemon (01M51JACH6TQP318ZFG6NQ9GRM).
///
/// Its `.claude.json` has the first-run answers of `claude`, so an
/// interactive `claude`, for example a takeover, asks no question
/// (01M501P9SDGVWG3MTVCW5BDRQQ): the onboarding, the theme, and the
/// trust of `clone`, as the person trusts the main clone
/// (01M4Z60B874MBWZWDP8YDEM3HR). The theme is the `theme` of the
/// `.claude.json` of the person at `person`, else `auto`. A key that is
/// there stays, so the answers of the person stay.
///
/// It writes the `settings.json` of [`settings`] with `auto` and `role`
/// when it is not there. When it is there, it adds each rule of
/// [`crate::permissions::session_rules`] for `role` that it lacks
/// (01M5084K4YQCZF20XA0P2485SG, 01M6FZXZ9267YKMH97E6A2Y2EP), and the auto
/// mode settings of `auto` that it lacks
/// ([`crate::permissions::with_auto_mode`], 01M516HEX3GVTN9K2QS12A4PG3),
/// the hooks of riff ([`with_session_hooks`]), and the key `tui` when it
/// has none ([`with_tui`], 01M6XVBRT2T838TX53SSXR6HRH).
///
/// The session writes `dir`, so each read and write there goes through
/// [`crate::host_file`]: a link there gives an error that names it, and
/// the host reads and writes no file of the person through it.
///
/// ```
/// use riff::permissions::{Role, Trust};
///
/// let dir = tempfile::tempdir()?;
/// let claude = dir.path().join("claude");
/// let person = dir.path().join("person.json");
/// std::fs::write(&person, r#"{"theme":"dark"}"#)?;
/// let auto = Trust { owner: Some("acme"), server: Some("http://h:7878"), sources: &[] };
/// riff::run::claude_folder(&claude, "/data/clone".as_ref(), &person, &auto, Role::Worker)?;
/// let read = |name: &str| -> serde_json::Value {
///     serde_json::from_slice(&std::fs::read(claude.join(name)).unwrap()).unwrap()
/// };
/// let json = read(".claude.json");
/// assert_eq!(json["hasCompletedOnboarding"], true);
/// assert_eq!(json["theme"], "dark");
/// assert_eq!(json["projects"]["/data/clone"]["hasTrustDialogAccepted"], true);
/// assert_eq!(read("settings.json"), riff::run::settings(&auto, Role::Worker));
///
/// // With no config of the person, the theme is auto.
/// let other = dir.path().join("other");
/// riff::run::claude_folder(&other, "/data/clone".as_ref(), &dir.path().join("none"), &auto, Role::Verifier)?;
/// let json: serde_json::Value = serde_json::from_slice(&std::fs::read(other.join(".claude.json"))?)?;
/// assert_eq!(json["theme"], "auto");
///
/// // A folder of an earlier start gets the missing rules and auto mode
/// // settings, and keeps its keys.
/// std::fs::write(claude.join("settings.json"), r#"{"includeCoAuthoredBy":false,"model":"opus"}"#)?;
/// riff::run::claude_folder(&claude, "/data/clone".as_ref(), &person, &auto, Role::Worker)?;
/// let settings = read("settings.json");
/// assert_eq!(settings["model"], "opus");
/// assert_eq!(settings["permissions"], riff::run::settings(&auto, Role::Worker)["permissions"]);
/// assert_eq!(settings["skillOverrides"], riff::run::settings(&auto, Role::Worker)["skillOverrides"]);
/// assert_eq!(settings["autoMode"], riff::run::settings(&auto, Role::Worker)["autoMode"]);
/// assert_eq!(settings["hooks"], riff::run::session_hooks());
/// assert_eq!(settings["tui"], riff::run::TUI);
///
/// // A link that the session made: an error that names it, and the file
/// // of the person stays as it was.
/// std::fs::remove_file(claude.join("settings.json"))?;
/// std::os::unix::fs::symlink(&person, claude.join("settings.json"))?;
/// let error = riff::run::claude_folder(&claude, "/data/clone".as_ref(), &person, &auto, Role::Worker)
///     .unwrap_err();
/// assert_eq!(format!("{error:#}"), riff::text::host_file_link(&claude.join("settings.json")));
/// assert_eq!(std::fs::read_to_string(&person)?, r#"{"theme":"dark"}"#);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn claude_folder(
    dir: &Path,
    clone: &Path,
    person: &Path,
    auto: &crate::permissions::Trust,
    role: crate::permissions::Role,
) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let config = Path::new(CLAUDE_JSON);
    let mut root = match crate::host_file::read(dir, config)?
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    root.entry("hasCompletedOnboarding").or_insert(json!(true));
    // The config of the person is out of each folder of a session.
    let theme = std::fs::read(person)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|p| p.get("theme").cloned())
        .unwrap_or_else(|| json!("auto"));
    root.entry("theme").or_insert(theme);
    let projects = root
        .entry("projects")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("`projects` in the Claude config is not an object")?;
    let project = projects
        .entry(clone.to_string_lossy().into_owned())
        .or_insert_with(|| json!({}));
    if let Some(project) = project.as_object_mut() {
        project
            .entry("hasTrustDialogAccepted")
            .or_insert(json!(true));
    }
    crate::host_file::write(
        dir,
        config,
        &serde_json::to_vec_pretty(&Value::Object(root))?,
    )?;
    let name = Path::new(SETTINGS_JSON);
    let new = match crate::host_file::read_to_string(dir, name)? {
        Some(text) => session_settings(&text, auto, role)
            .with_context(|| dir.join(name).display().to_string())?,
        None => Some(serde_json::to_string_pretty(&settings(auto, role))?),
    };
    if let Some(new) = new {
        crate::host_file::write(dir, name, new.as_bytes())?;
    }
    Ok(())
}

/// The settings text `text` of an earlier start with what
/// [`claude_folder`] adds, or `None` when it has each part.
fn session_settings(
    text: &str,
    auto: &crate::permissions::Trust,
    role: crate::permissions::Role,
) -> Result<Option<String>> {
    let rules = crate::permissions::session_rules(role);
    let missing = crate::permissions::missing(&rules, &[text]);
    let mut new = crate::permissions::with_rules(text, &missing)?;
    if let Some(changed) = crate::permissions::with_auto_mode(new.as_deref().unwrap_or(text), auto)?
    {
        new = Some(changed);
    }
    if let Some(changed) = with_session_hooks(new.as_deref().unwrap_or(text))? {
        new = Some(changed);
    }
    if let Some(changed) = with_tui(new.as_deref().unwrap_or(text))? {
        new = Some(changed);
    }
    Ok(new)
}

/// The sign-in of `claude` in its `.credentials.json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    claude_ai_oauth: OAuth,
}

/// What riff reads from the sign-in file of `claude`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    /// The minutes that the access token has left.
    pub minutes: i64,
    /// The tokens: they must not go into a log or onto the terminal.
    pub secrets: Vec<String>,
}

/// Reads the sign-in file of `claude` at the time `now_ms`.
///
/// ```
/// let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","refreshToken":"sk-r","expiresAt":7200000}}"#;
/// let sign_in = riff::run::sign_in(file, 0)?;
/// assert_eq!(sign_in.minutes, 120);
/// assert_eq!(sign_in.secrets, ["sk-a", "sk-r"]);
/// assert!(riff::run::sign_in("{}", 0).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sign_in(file: &str, now_ms: i64) -> Result<SignIn> {
    let credentials: Credentials =
        serde_json::from_str(file).context("the sign-in file of claude has no claudeAiOauth")?;
    let oauth = credentials.claude_ai_oauth;
    Ok(SignIn {
        minutes: (oauth.expires_at - now_ms) / 60_000,
        secrets: [oauth.access_token, oauth.refresh_token]
            .into_iter()
            .flatten()
            .collect(),
    })
}

/// Takes each secret out of a text (01M4Z60B8KDXD190RWC7D184Q3), and
/// each token of GitHub: one of [`TOKEN_STARTS`] and the letters, digits
/// and `_` after it (01M6WPMX18BECQPTQNSTZMBN16,
/// 01M6ZAEH3B92HKV0YTM2WTF1T2). The daemon renews the token of a session
/// after the start of its relay, so no list holds it. riff reads no token
/// of the person for a session (01M6Z87A04EXTT1FFJSJ1EM6TB), so no list
/// holds that one either.
///
/// The scrub finds the range of each match in the text first, then
/// hides their union ([`Scrub::found`], [`hide`],
/// 01M7G01XCJGVYZBHW20GVGWG6K). So a short secret in a long one does not
/// cut the long one, in each order of the list.
///
/// ```
/// use riff::run::Scrub;
///
/// let mcp = Scrub::new(["live".to_owned(), "sk_live_51HxyzAbCdEf".to_owned()]);
/// assert_eq!(mcp.clean("stripe refused sk_live_51HxyzAbCdEf"), "stripe refused [secret]");
/// let scrub = Scrub::new(["sk-ant-oat01-secret1".to_owned(), "".to_owned()]);
/// assert_eq!(scrub.clean("token sk-ant-oat01-secret1 here"), "token [secret] here");
/// assert_eq!(scrub.clean("no token"), "no token");
/// assert_eq!(
///     Scrub::default().clean("oauth_token: ghs_16C7e42F292c6912E7710c838347Ae178B4a, end"),
///     "oauth_token: [secret], end"
/// );
/// assert_eq!(Scrub::default().clean("ghs_ alone, and ghs_a1"), "ghs_ alone, and [secret]");
/// assert_eq!(
///     Scrub::default().clean("gho_a1 ghp_b2 ghu_c3 ghr_d4 github_pat_11AB_cd"),
///     "[secret] [secret] [secret] [secret] [secret]"
/// );
/// ```
#[derive(Debug, Clone, Default)]
pub struct Scrub {
    secrets: Vec<String>,
}

impl Scrub {
    /// A scrub of `secrets`. It ignores a value of fewer than 4
    /// characters: such a value is no token.
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        Self {
            secrets: secrets.into_iter().filter(|s| s.len() >= 4).collect(),
        }
    }

    /// `text` with one [`HIDDEN`] in place of each run of the union of
    /// [`Scrub::found`] ([`hide`]).
    pub fn clean<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let found = self.found(text);
        if found.is_empty() {
            Cow::Borrowed(text)
        } else {
            Cow::Owned(hide(text, found))
        }
    }

    /// The range in `text` of each match of each secret, also of two
    /// matches that overlap, and of each token of GitHub ([`tokens`]).
    ///
    /// ```
    /// use riff::run::Scrub;
    ///
    /// let scrub = Scrub::new(["aaaa".to_owned()]);
    /// assert_eq!(scrub.found("x aaaaa ghs_b1"), [2..6, 3..7, 8..14]);
    /// let scrub = Scrub::new(["é-secret-1".to_owned()]);
    /// assert_eq!(scrub.found("a é-secret-1 b é-secret-1"), [2..13, 16..27]);
    /// ```
    pub fn found(&self, text: &str) -> Vec<Range<usize>> {
        let part = |c: char| c.is_ascii_alphanumeric() || c == '_';
        self.secrets
            .iter()
            .flat_map(|secret| matches(text, secret))
            .chain(
                TOKEN_STARTS
                    .iter()
                    .flat_map(|start| tokens(text, start, part)),
            )
            .collect()
    }
}

/// The starts of the tokens of GitHub: of a GitHub App, of OAuth, of a
/// person, of a user to server, of a refresh, and a fine-grained one.
pub const TOKEN_STARTS: [&str; 6] = ["ghs_", "gho_", "ghp_", "ghu_", "ghr_", "github_pat_"];

/// The range of each match of `secret` in `text`, also of two matches
/// that overlap. `secret` is not empty.
fn matches<'a>(text: &'a str, secret: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    let step = secret.chars().next().map_or(1, char::len_utf8);
    let mut from = 0;
    std::iter::from_fn(move || {
        let at = from + text.get(from..)?.find(secret)?;
        from = at + step;
        Some(at..at + secret.len())
    })
}

/// The range of each token in `text` that starts with `start`: `start`
/// and each character after it that `part` takes. A `start` with no such
/// character after it is no token. A `start` that ends in the last token
/// that this scan found gives no range: its token is in the last token.
/// So `part` looks at each character of `text` at most once, and the
/// time grows with the length of `text` (01M7G3CSVWSV6RTGJY1WMFGV7V).
/// An empty `start` gives each run of characters that `part` takes.
///
/// ```
/// use riff::run::tokens;
///
/// let part = |c: char| c.is_ascii_alphanumeric() || c == '-';
/// let found: Vec<_> = tokens("a=ghs_A-9,ghs_ end", "ghs_", part).collect();
/// assert_eq!(found, [2..9]);
/// let part = |c: char| c.is_ascii_alphanumeric() || c == '_';
/// let found: Vec<_> = tokens("ghs_ghs_ghs_a1", "ghs_", part).collect();
/// assert_eq!(found, [0..14]);
/// let found: Vec<_> = tokens("ab c", "", part).collect();
/// assert_eq!(found, [0..2, 3..4]);
/// ```
pub fn tokens<'a>(
    text: &'a str,
    start: &'a str,
    part: fn(char) -> bool,
) -> impl Iterator<Item = Range<usize>> + 'a {
    let mut last_end = None;
    text.match_indices(start).filter_map(move |(at, _)| {
        let from = at + start.len();
        if last_end.is_some_and(|end| from <= end) {
            return None;
        }
        let after = &text[from..];
        let end = from + after.find(|c: char| !part(c)).unwrap_or(after.len());
        last_end = Some(end);
        (end > from).then_some(at..end)
    })
}

/// `text` with one [`HIDDEN`] in place of each run of the union of
/// `found`: two ranges that overlap or touch make one run. Each range
/// starts and ends at a character of `text`.
///
/// ```
/// use riff::run::hide;
///
/// assert_eq!(hide("a b c d e", vec![6..7, 2..3, 2..5]), "a [secret] [secret] e");
/// assert_eq!(hide("abcdef", vec![0..2, 2..4]), "[secret]ef");
/// assert_eq!(hide("no secret", Vec::new()), "no secret");
/// ```
pub fn hide(text: &str, mut found: Vec<Range<usize>>) -> String {
    found.sort_by_key(|range| range.start);
    let mut runs: Vec<Range<usize>> = Vec::new();
    for range in found {
        match runs.last_mut() {
            Some(run) if range.start <= run.end => run.end = run.end.max(range.end),
            _ => runs.push(range),
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut shown = 0;
    for run in runs {
        out.push_str(&text[shown..run.start]);
        out.push_str(HIDDEN);
        shown = run.end;
    }
    out.push_str(&text[shown..]);
    out
}

/// The one stream-json line that gives `claude` the text of a person.
///
/// ```
/// let line = riff::run::user_line("Do it.");
/// assert_eq!(
///     line,
///     r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Do it."}]}}"#
/// );
/// ```
pub fn user_line(text: &str) -> String {
    json!({
        "type": "user",
        "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
    })
    .to_string()
}

/// The most characters of one shown part of an event.
const SHOWN: usize = 160;

/// The first line of `text`, at most [`SHOWN`] characters.
fn first(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("");
    match line.char_indices().nth(SHOWN) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// A short form of the input of a tool call.
fn tool_input(input: &Value) -> String {
    for key in [
        "command",
        "file_path",
        "pattern",
        "url",
        "description",
        "prompt",
    ] {
        if let Some(value) = input[key].as_str() {
            return first(value);
        }
    }
    String::new()
}

/// The lines that the terminal shows for one event of `claude`, or none.
///
/// ```
/// use riff::run::event_lines;
/// use serde_json::json;
///
/// let init = json!({"type":"system","subtype":"init","model":"claude-opus-5-5","permissionMode":"auto"});
/// assert_eq!(event_lines(&init), ["start: model claude-opus-5-5, permission mode auto"]);
///
/// let call = json!({"type":"assistant","message":{"content":[
///     {"type":"text","text":"I read the issue.\nThen I edit."},
///     {"type":"tool_use","name":"Bash","input":{"command":"gh issue view 12"}}]}});
/// assert_eq!(event_lines(&call), ["I read the issue.", "> Bash: gh issue view 12"]);
///
/// let failed = json!({"type":"user","message":{"content":[
///     {"type":"tool_result","is_error":true,"content":"exit 1"}]}});
/// assert_eq!(event_lines(&failed), ["! the tool failed: exit 1"]);
///
/// let result = json!({"type":"result","subtype":"success","num_turns":8,
///     "duration_ms":22700,"total_cost_usd":0.169});
/// assert_eq!(event_lines(&result), ["end: success, 8 turns, 22 s, 0.17 USD"]);
///
/// assert!(event_lines(&json!({"type":"rate_limit_event"})).is_empty());
/// ```
pub fn event_lines(event: &Value) -> Vec<String> {
    let str_of = |key: &str| event[key].as_str().unwrap_or("?");
    match (str_of("type"), event["subtype"].as_str()) {
        ("system", Some("init")) => vec![format!(
            "start: model {}, permission mode {}",
            str_of("model"),
            str_of("permissionMode")
        )],
        ("system", Some(subtype @ ("permission_denied" | "vcs_state_changed"))) => {
            let mut parts: Vec<String> = event
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| !matches!(k.as_str(), "type" | "subtype" | "session_id" | "uuid"))
                .filter_map(|(k, v)| v.as_str().map(|v| format!("{k} {}", first(v))))
                .collect();
            parts.sort();
            vec![format!("{subtype}: {}", parts.join(", "))]
        }
        ("assistant", _) => blocks(event)
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => Some(first(block["text"].as_str().unwrap_or(""))),
                Some("tool_use") => Some(format!(
                    "> {}: {}",
                    block["name"].as_str().unwrap_or("?"),
                    tool_input(&block["input"])
                )),
                _ => None,
            })
            .filter(|line| !line.is_empty())
            .collect(),
        ("user", _) => blocks(event)
            .filter(|block| block["type"] == "tool_result" && block["is_error"] == true)
            .map(|block| {
                format!(
                    "! the tool failed: {}",
                    first(&content_text(&block["content"]))
                )
            })
            .collect(),
        ("result", _) => vec![format!(
            "end: {}, {} turns, {} s, {:.2} USD",
            str_of("subtype"),
            event["num_turns"].as_u64().unwrap_or(0),
            event["duration_ms"].as_u64().unwrap_or(0) / 1000,
            event["total_cost_usd"].as_f64().unwrap_or(0.0)
        )],
        _ => Vec::new(),
    }
}

/// The content blocks of the message of an event.
fn blocks(event: &Value) -> impl Iterator<Item = &Value> {
    event["message"]["content"].as_array().into_iter().flatten()
}

/// The text of the content of a tool result: a text, or a list of
/// text blocks.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// The issue number of an item `issue-N`. `riff run` takes only an
/// issue (01M4Z81AZ88ZS0HPHV5KGQF3HV).
///
/// ```
/// assert_eq!(riff::run::issue_of("issue-12")?, 12);
/// assert!(riff::run::issue_of("verify-issue-12").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn issue_of(item: &str) -> Result<u64> {
    item.strip_prefix("issue-")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| anyhow!("{}", text::run_not_an_issue(item)))
}

/// How often [`output_within`] looks whether its program ended.
const WITHIN_POLL: Duration = Duration::from_millis(20);

/// Runs `program ARGS` in `dir`, and gives its stdout, with the time
/// limit `limit` (#1858). An error names the program and its stderr.
/// At the end of the limit, riff stops the program and each process that
/// it started, and the error says so. A step of the host that hangs, for
/// example a `podman` that waits for a lock, holds up no loop of the
/// daemon.
pub(crate) fn output_within(
    program: &str,
    dir: &Path,
    args: &[&str],
    limit: Duration,
) -> Result<String> {
    use std::os::unix::process::CommandExt;
    let end = std::time::Instant::now() + limit;
    let mut child = std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own: the stop at the end of the limit also stops
        // each process that the program started.
        .process_group(0)
        .spawn()
        .with_context(|| text::no_program(program))?;
    // The pipes go on threads: a program that writes much blocks no step
    // of the wait.
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            let _ = tx.send(bytes);
        });
        rx
    };
    let stdout = read(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = read(child.stderr.take().map(|p| Box::new(p) as _));
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= end {
            if let Ok(group) = i32::try_from(child.id()) {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(group),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            let _ = child.wait();
            bail!(text::no_answer(program, &args.join(" "), limit));
        }
        std::thread::sleep(WITHIN_POLL);
    };
    // A process that the program left behind can hold a pipe open: the
    // read waits for it up to the end of the limit, and not longer.
    let text = |pipe: std::sync::mpsc::Receiver<Vec<u8>>| {
        let left = end.saturating_duration_since(std::time::Instant::now());
        let bytes = pipe.recv_timeout(left.max(WITHIN_POLL)).unwrap_or_default();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    };
    let (out, err) = (text(stdout), text(stderr));
    if !status.success() {
        bail!("{program} {}: {err}", args.join(" "));
    }
    Ok(out)
}

/// The facts of the host that `riff run` checks before the claim.
struct Checked {
    frame: Frame,
    host: Host,
}

/// The facts of the host that each session needs: the home and the user
/// of the person, and the sign-in of `claude` (01M4Z81B3A324KKE8M8Z71TM4M).
/// riff checks the sign-in of `gh` too, for the steps on the host, and
/// keeps no token of it: no session gets the token of the person
/// (01M6Z87A04EXTT1FFJSJ1EM6TB).
pub(crate) struct Host {
    pub(crate) home: PathBuf,
    pub(crate) user: String,
    pub(crate) claude: Claude,
}

impl Host {
    /// The values of the variables of the host in the spec: the token of
    /// `claude` when it is set.
    pub(crate) fn values(&self) -> Vec<(String, String)> {
        match &self.claude {
            Claude::Token(token) => vec![(crate::claude_token::VAR.to_owned(), token.clone())],
            Claude::File { .. } => Vec::new(),
        }
    }

    /// The secrets that no event shows.
    pub(crate) fn secrets(&self) -> Vec<String> {
        self.claude.secrets()
    }
}

/// The sign-in of `claude` on the host for the container sessions. It
/// has no `Debug`: it holds a secret.
pub enum Claude {
    /// The long-lived token of `riff claude-token`
    /// (01M58B7H50RCG2VXXV9XX5Y7HM).
    Token(String),
    /// No token: the sign-in file of `claude`, with [`MIN_MINUTES`] or
    /// more left.
    File {
        /// The path of the sign-in file.
        path: PathBuf,
        /// What riff reads in it.
        sign_in: SignIn,
    },
}

impl Claude {
    /// How the container signs in, with no secret.
    pub fn sign_in(&self) -> ClaudeSignIn {
        match self {
            Claude::Token(_) => ClaudeSignIn::Token,
            Claude::File { path, .. } => ClaudeSignIn::File(path.clone()),
        }
    }

    /// The secrets that no event shows.
    pub fn secrets(&self) -> Vec<String> {
        match self {
            Claude::Token(token) => vec![token.clone()],
            Claude::File { sign_in, .. } => sign_in.secrets.clone(),
        }
    }
}

/// The sign-in of `claude` for the sessions of the daemon in the folder
/// `daemon`, at the time `now_ms`: the token of `riff claude-token` in
/// the OS keyring when it is set, with no rule of [`MIN_MINUTES`]. Else
/// the sign-in file `credentials`, with the rule
/// (01M58B7H50RCG2VXXV9XX5Y7HM). A token that `claude` refused is a
/// fault (01M58B7H7HD0H48A0C79F8M4CR). A keyring that is locked, or that
/// does not answer, is a fault too, with no fall back to the file
/// (01M58XZF928T82YAMF4DSJBKBF).
///
/// ```standalone_crate
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use riff::run::{Claude, HostFault, claude_of};
///
/// let dir = tempfile::tempdir()?;
/// let credentials = dir.path().join(".credentials.json");
/// // The access token has 10 minutes left.
/// let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","expiresAt":600000}}"#;
/// std::fs::write(&credentials, file)?;
/// let daemon = dir.path().join("daemon");
/// assert!(matches!(claude_of(&daemon, &credentials, 0), Err(HostFault::SignInEnds(10))));
/// riff::claude_token::store(&daemon, "sk-ant-oat01-x")?;
/// assert!(matches!(claude_of(&daemon, &credentials, 0), Ok(Claude::Token(t)) if t == "sk-ant-oat01-x"));
/// std::fs::create_dir_all(&daemon)?;
/// riff::claude_token::refuse(&daemon)?;
/// assert!(matches!(claude_of(&daemon, &credentials, 0), Err(HostFault::TokenRefused)));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn claude_of(daemon: &Path, credentials: &Path, now_ms: i64) -> Result<Claude, HostFault> {
    match crate::claude_token::load() {
        Ok(Some(token)) => {
            if crate::claude_token::refused(daemon) {
                return Err(HostFault::TokenRefused);
            }
            return Ok(Claude::Token(token));
        }
        Ok(None) => {}
        Err(e) => {
            return Err(match e.downcast_ref::<riff_client::secrets::Locked>() {
                Some(locked) => HostFault::KeyringLocked(locked.host.clone()),
                None => HostFault::Missing(format!("{e:#}")),
            });
        }
    }
    let missing = HostFault::Missing;
    let file = std::fs::read_to_string(credentials)
        .map_err(|e| missing(format!("{}: {e}", text::run_no_sign_in(credentials))))?;
    let sign_in = sign_in(&file, now_ms).map_err(|e| missing(format!("{e:#}")))?;
    if let Some(fault) = HostFault::of_minutes(sign_in.minutes) {
        return Err(fault);
    }
    Ok(Claude::File {
        path: credentials.to_owned(),
        sign_in,
    })
}

/// A fact of the host that stops each start of a session, whatever the
/// item (01M57A7A1W2K9440WZA1QZ3M9T). Its text is the text of `riff
/// run`. [`HostFault::wave_text`] is the text of a wave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostFault {
    /// The sign-in of `claude` has less than [`MIN_MINUTES`] left: the
    /// minutes that it has.
    SignInEnds(i64),
    /// `claude` refused the token of `riff claude-token`
    /// (01M58B7H7HD0H48A0C79F8M4CR).
    TokenRefused,
    /// The OS keyring of the host is locked, or does not answer: the
    /// host. The daemon cannot read the token
    /// (01M58XZF928T82YAMF4DSJBKBF).
    KeyringLocked(String),
    /// The riff daemon did not run `gh`: the OS keyring of the host holds
    /// the token of `gh` and keeps it locked, or does not answer
    /// (01M77AB7XZV8KG8RRZ8C6R5A8X). The text says so.
    GhLocked(String),
    /// Another fact is missing: the text that says the fix.
    Missing(String),
}

/// True when `error` comes of a locked OS keyring, or of one that does
/// not answer: a [`riff_client::secrets::Locked`], or a [`HostFault`]
/// that ends at the unlock ([`HostFault::locked`]). Then the step works
/// again after the unlock (01M7BN2EPE81NEN3YZ78VSSG3W).
///
/// ```
/// use riff::run::{HostFault, keyring_locked};
/// use riff_client::secrets::Locked;
///
/// let locked = anyhow::Error::msg(Locked { host: "thelio".into() });
/// assert!(keyring_locked(&locked.context("riff cannot find your user")));
/// assert!(keyring_locked(&HostFault::GhLocked("riff did not run gh auth token".into()).into()));
/// assert!(keyring_locked(&HostFault::KeyringLocked("thelio".into()).into()));
/// assert!(!keyring_locked(&HostFault::Missing("set HOME".into()).into()));
/// assert!(!keyring_locked(&anyhow::anyhow!("the image is broken")));
/// ```
pub fn keyring_locked(error: &anyhow::Error) -> bool {
    riff_client::secrets::is_locked(error)
        || error
            .downcast_ref::<HostFault>()
            .is_some_and(HostFault::locked)
}

impl HostFault {
    /// True when the OS keyring of the host is locked, or does not
    /// answer: the fault ends at the unlock.
    ///
    /// ```
    /// use riff::run::HostFault;
    ///
    /// assert!(HostFault::KeyringLocked("thelio".into()).locked());
    /// assert!(HostFault::GhLocked("riff did not run gh auth token".into()).locked());
    /// assert!(!HostFault::TokenRefused.locked());
    /// assert!(!HostFault::Missing("set HOME".into()).locked());
    /// ```
    pub fn locked(&self) -> bool {
        matches!(self, HostFault::KeyringLocked(_) | HostFault::GhLocked(_))
    }

    /// The fault of a sign-in of `claude` with `minutes` left, or `None`
    /// when it has [`MIN_MINUTES`] or more.
    ///
    /// ```
    /// use riff::run::HostFault;
    ///
    /// assert_eq!(HostFault::of_minutes(42), Some(HostFault::SignInEnds(42)));
    /// assert_eq!(HostFault::of_minutes(60), None);
    /// ```
    pub fn of_minutes(minutes: i64) -> Option<HostFault> {
        (minutes < MIN_MINUTES).then_some(HostFault::SignInEnds(minutes))
    }

    /// The text for the lead of a wave. The wave tries again by itself,
    /// so the text does not tell the person to run `riff run`.
    ///
    /// ```
    /// use riff::run::HostFault;
    ///
    /// let text = HostFault::SignInEnds(42).wave_text();
    /// assert!(text.contains("ends in 42 minutes"), "{text}");
    /// assert!(!text.contains("riff run"), "{text}");
    /// assert!(HostFault::SignInEnds(42).to_string().contains("riff run again"));
    /// let refused = HostFault::TokenRefused.wave_text();
    /// assert!(refused.contains("claude setup-token"), "{refused}");
    /// ```
    pub fn wave_text(&self) -> String {
        match self {
            HostFault::SignInEnds(minutes) => text::wave_sign_in_ends(*minutes),
            HostFault::TokenRefused => text::CLAUDE_TOKEN_REFUSED.to_owned(),
            HostFault::KeyringLocked(host) => text::claude_token_locked(host),
            HostFault::GhLocked(text) | HostFault::Missing(text) => text.clone(),
        }
    }
}

impl std::fmt::Display for HostFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostFault::SignInEnds(minutes) => f.write_str(&text::run_sign_in_ends(*minutes)),
            HostFault::TokenRefused => f.write_str(text::CLAUDE_TOKEN_REFUSED),
            HostFault::KeyringLocked(host) => f.write_str(&text::claude_token_locked(host)),
            HostFault::GhLocked(text) | HostFault::Missing(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for HostFault {}

/// The sign-in file of `claude` for the home `home`: in
/// `CLAUDE_CONFIG_DIR`, else in `.claude` of the home.
///
/// ```
/// let path = riff::run::credentials_in(std::path::Path::new("/home/mike"));
/// assert!(path.ends_with(".credentials.json"));
/// ```
pub fn credentials_in(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from)
        .join(".credentials.json")
}

/// Reads the [`Host`] facts, with `gh` ([`host_gh`]), for the daemon of
/// this process ([`daemon::dir`]). A fact that is missing is a
/// [`HostFault`] that says the fix.
pub(crate) fn host_facts() -> Result<Host, HostFault> {
    let missing = HostFault::Missing;
    gh_signed_in(&host_gh())?;
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| missing("set HOME".into()))?;
    let user = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| missing("set USER".into()))?;
    let dir = daemon::dir().map_err(|e| missing(format!("{e:#}")))?;
    let claude = claude_of(&dir, &credentials_in(&home), now_ms())?;
    Ok(Host { home, user, claude })
}

/// The `gh` of the facts of the host. Each call has the time limit
/// [`GH_LIMIT`](daemon::sweep::GH_LIMIT), as the look of a wave has, so a
/// `gh` that hangs stops no wave (01M77AB80MEFD9K8Y7FPDATY3D).
///
/// ```
/// use riff::daemon::sweep::GH_LIMIT;
///
/// assert_eq!(riff::run::host_gh().limit(), Some(GH_LIMIT));
/// ```
pub fn host_gh() -> pr::Gh {
    pr::Gh::in_daemon("token").with_limit(daemon::sweep::GH_LIMIT)
}

/// The fact of the host that `gh` has a sign-in, with `gh auth token`.
/// riff keeps no token of it (01M6Z87A04EXTT1FFJSJ1EM6TB). A `gh` that
/// the riff daemon does not run, because the keyring is locked
/// (01M77AB7XZV8KG8RRZ8C6R5A8X), is the fault [`HostFault::GhLocked`]: it
/// says to unlock it at the desktop, and it ends at the unlock
/// (01M7BN2EPE81NEN3YZ78VSSG3W). A `gh` with no answer in its time limit
/// is a fault that says so (01M77AB80MEFD9K8Y7FPDATY3D).
///
/// ```standalone_crate
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use keyring_core::{Entry, Error, mock};
/// use riff::run::{HostFault, gh_signed_in};
///
/// let dir = tempfile::tempdir()?;
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), "#!/bin/sh\necho gho_not_real\n"));
/// # if !riff::pr::token_given() {
/// // The keyring holds the token of gh, and keeps it locked: the daemon runs no gh.
/// let entry = Entry::new(riff::pr::GH_KEYRING, "")?;
/// entry.set_password("gho_not_real")?;
/// let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
/// cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other("locked"))));
/// riff_client::secrets::ask_no_prompt();
/// let fault = gh_signed_in(&gh).unwrap_err();
/// assert!(matches!(fault, HostFault::GhLocked(_)) && fault.locked(), "{fault:?}");
/// let text = fault.to_string();
/// assert!(text.starts_with("riff did not run gh auth token: the OS keyring of "), "{text}");
/// // The keyring is open again: gh runs.
/// assert_eq!(gh_signed_in(&gh), Ok(()));
/// # }
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff::run::{HostFault, gh_signed_in};
///
/// let dir = tempfile::tempdir()?;
/// let gh = |script: &str| riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// assert_eq!(gh_signed_in(&gh("#!/bin/sh\necho gho_not_real\n")), Ok(()));
/// let out = gh_signed_in(&gh("#!/bin/sh\necho 'not logged in' >&2\nexit 1\n"));
/// assert_eq!(
///     out,
///     Err(HostFault::Missing(
///         "riff needs the sign-in of gh. Run: gh auth login: gh auth token: not logged in".into()
///     ))
/// );
/// // A gh that hangs ends at its limit, and the fault says why.
/// let start = Instant::now();
/// let hangs = gh("#!/bin/sh\nsleep 600\n").with_limit(Duration::from_millis(500));
/// let Err(HostFault::Missing(why)) = gh_signed_in(&hangs) else { panic!("no fault") };
/// assert!(why.ends_with("gh auth token: no answer in 0.5 s. riff stopped it."), "{why}");
/// assert!(start.elapsed() < Duration::from_secs(10));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn gh_signed_in(gh: &pr::Gh) -> Result<(), HostFault> {
    match gh.run(&["auth", "token"], None) {
        Ok(token) => {
            drop(zeroize::Zeroizing::new(token));
            Ok(())
        }
        Err(e) if riff_client::secrets::is_locked(&e) => Err(HostFault::GhLocked(format!("{e:#}"))),
        Err(e) => Err(HostFault::Missing(format!("{}: {e:#}", text::RUN_NO_GH))),
    }
}

/// The trailers of the pull request of the session `session` in its
/// start text (01M6WZ2CB56PNSYKEX4E0ZJV3Y): the session, and the person
/// that `gh` reads on the host (01M6WZ2CG9DFB45E3BQCGQQYNY). When the
/// read fails, it says so in one line, and the frame has no line
/// `Co-authored-by:`. `riff run` and the wave of the daemon use it.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let reply = r#"{"login":"mona","id":583231,"name":"Mona Lisa"}"#;
/// let gh = isolated::write_program(dir.path().join("gh"), &format!("#!/bin/sh\necho '{reply}'\n"));
/// let made = riff::run::made_by(&riff::pr::Gh::at(gh), "21c026a1-ee2a");
/// assert_eq!(made.session, "21c026a1-ee2a");
/// assert_eq!(made.person.unwrap().login, "mona");
///
/// let broken = isolated::write_program(dir.path().join("broken"), "#!/bin/sh\nexit 1\n");
/// assert_eq!(riff::run::made_by(&riff::pr::Gh::at(broken), "21c026a1-ee2a").person, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn made_by(gh: &pr::Gh, session: &str) -> pr::MadeBy {
    let person = pr::read_user(gh)
        .inspect_err(|e| println!("{}", text::no_co_author(&format!("{e:#}"))))
        .ok();
    pr::MadeBy {
        session: session.to_owned(),
        person,
    }
}

/// What the start text of a build takes from its issue: the milestone,
/// the link line of the frame of the body of the pull request, and the
/// body of the issue for a session with no GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The milestone of the issue.
    pub milestone: String,
    /// The link line: `Refs #N` for an issue with a line `Live:` in "Done
    /// when", else `Closes #N` ([`pr::Link::of`],
    /// 01M6WW5WSSRMTBP882FQP3TS2Q).
    pub link: pr::Link,
    /// The body of the issue: the start text of `riff run --no-github`
    /// holds it (01M6Z87JY83V3KR72E2AG8V8KG).
    pub issue: String,
}

impl Frame {
    /// The frame of the body of the pull request: [`pr::body`] with the
    /// summary `SUMMARY`.
    pub fn body(&self) -> Result<String> {
        pr::body(self.link, "SUMMARY", &self.milestone)
    }
}

/// The milestone, the link line and the body of the issue `issue`, with
/// `gh`. An issue with no milestone, or with one that `text::forge`
/// changes, is refused: the milestone goes into the start text and into
/// a command of the session, as in `riff pr open`.
pub(crate) fn frame(gh: &pr::Gh, issue: u64) -> Result<Frame> {
    let found = pr::view_issue(gh, issue)?;
    let milestone = found
        .issue
        .milestone
        .map(|m| m.title)
        .ok_or_else(|| anyhow!("{}", text::run_no_milestone(issue)))?;
    if text::forge(&milestone) != milestone {
        bail!("{}", text::run_no_milestone(issue));
    }
    Ok(Frame {
        milestone,
        link: pr::Link::of(issue, &found.body, false),
        issue: found.body,
    })
}

/// True when the issue `issue` has the label [`rollout::NO_VERIFY`], with
/// `gh`.
pub(crate) fn labelled(gh: &pr::Gh, issue: u64) -> Result<bool> {
    let found: rollout::Issue = gh.json(&[
        "issue",
        "view",
        &issue.to_string(),
        "--json",
        "number,labels",
    ])?;
    Ok(found.no_verify())
}

/// Why riff sends no verify request for `item`, the issue `issue`, or
/// `None` to send it: the issue has the label [`rollout::NO_VERIFY`]
/// (`labelled`, 01M5157V0P92EBHF4M7N9K0RGR), or a lead holds
/// `verify-ITEM` (01M5157TYDEHZHDXSC1AHCRDRN). `holds` are the holds of
/// the repository thread. `riff run` and the wave of the daemon
/// ([`crate::wave`], 01M51KH0H7J35PEKMHP5KQB84T) use it.
///
/// ```
/// use std::collections::BTreeMap;
/// use riff::run::no_verify;
/// use riff_core::wire::HoldInfo;
///
/// let hold = |reason: &str| HoldInfo { reason: reason.into(), by: None, at_ms: 1_790_000_000_000 };
/// let mut holds = BTreeMap::new();
/// assert_eq!(no_verify("issue-12", 12, false, &holds), None);
/// // A hold of the item itself does not stop the verify request.
/// holds.insert("issue-12".to_owned(), hold("later"));
/// assert_eq!(no_verify("issue-12", 12, false, &holds), None);
/// holds.insert("verify-issue-12".to_owned(), hold("a scratch item"));
/// assert_eq!(
///     no_verify("issue-12", 12, false, &holds).unwrap(),
///     "verify-issue-12 is held by the lead since 2026-09-21T14:13:20Z: a scratch item"
/// );
/// assert_eq!(
///     no_verify("issue-12", 12, true, &BTreeMap::new()).unwrap(),
///     "issue #12 has the label no-verify."
/// );
/// ```
pub fn no_verify(
    item: &str,
    issue: u64,
    labelled: bool,
    holds: &BTreeMap<String, HoldInfo>,
) -> Option<String> {
    if labelled {
        return Some(text::no_verify_label(issue));
    }
    let verify = format!("verify-{item}");
    let hold = holds.get(&verify)?;
    Some(riff_state::state::plan::held_text(&verify, hold))
}

/// [`no_verify`] now: the labels of the issue with `gh`, then the holds
/// of `thread` from riff-server, as `me`. riff asks riff-server only for
/// an issue with no label. A fault of `gh` shows on stderr, and riff
/// goes on to the holds.
pub(crate) async fn no_verify_now(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    gh: pr::Gh,
    item: &str,
    issue: u64,
) -> Result<Option<String>> {
    let labelled = tokio::task::spawn_blocking(move || labelled(&gh, issue))
        .await
        .map_err(|e| anyhow!("{e}"))
        .and_then(|read| read)
        .unwrap_or_else(|e| {
            eprintln!("riff: {e:#}");
            false
        });
    if labelled {
        return Ok(no_verify(item, issue, true, &BTreeMap::new()));
    }
    let holds = api.plan_show(me, thread).await?.holds;
    Ok(no_verify(item, issue, false, &holds))
}

/// Checks each fact that a session needs, before the claim. The daemon
/// in the folder `daemon` pulls the image when this machine does not
/// have it (01M4Z81B5A456HDKW7G0WFGCY6).
async fn check(daemon: &Path, issue: u64, image: &str) -> Result<Checked> {
    daemon::image(daemon, image, |line| println!("{line}")).await?;
    let host = host_facts()?;
    Ok(Checked {
        frame: frame(&pr::Gh::here("run"), issue)?,
        host,
    })
}

/// The time now in milliseconds since the epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The main clone of the working folder `dir`: its top, and its git dir.
pub fn main_clone(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let common = PathBuf::from(crate::host_git::output(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let top = common
        .parent()
        .ok_or_else(|| anyhow!("the git dir {} has no parent", common.display()))?
        .to_owned();
    Ok((top, common))
}

/// True when `origin` has the branch `branch`: the session of the item
/// goes on from it (01M4Z81AX3ND5RX7V1GEY4HMF5). The host asks `origin`
/// from the main clone `main`, with `git ls-remote`, and makes no clone:
/// the session clones in its container ([`crate::session_clone`],
/// 01M7DZ42C8SY2GV5ZQJZHM2VTV).
pub fn earlier(main: &Path, branch: &Branch) -> Result<bool> {
    let remote = crate::host_git::output(main, &["remote", "get-url", "origin"])?;
    let head = format!("refs/heads/{branch}");
    Ok(
        !git_credential::run(main, &remote, &["ls-remote", "--", &remote, &head])?
            .trim()
            .is_empty(),
    )
}

/// What the host asks `origin` before the start of a session of `riff
/// run` or of the lead ([`on_origin`], 01M7G29PSP7X56X8E60A3KRV4V).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnOrigin {
    /// The default branch of `origin`: the branch of its `HEAD`. A rename
    /// of the default branch on GitHub does not change `origin/HEAD` of
    /// the main clone, so riff does not read that ref.
    pub base: Branch,
    /// `origin` has the branch of the item: the session goes on from it
    /// (01M4Z81AX3ND5RX7V1GEY4HMF5).
    pub earlier: bool,
}

impl OnOrigin {
    /// The facts in `listed`, the output of `git ls-remote --symref --
    /// URL HEAD refs/heads/BRANCH`, with `branch` the branch of the item.
    /// A `HEAD` that names no branch, as on an empty repository, is an
    /// error.
    ///
    /// ```
    /// use riff::run::OnOrigin;
    ///
    /// let branch = "worktree-issue-12".parse()?;
    /// let listed = "ref: refs/heads/trunk\tHEAD\n1a2b\tHEAD\n3c4d\trefs/heads/worktree-issue-12\n";
    /// let on = OnOrigin::of_listed(listed, Some(&branch))?;
    /// assert_eq!(on, OnOrigin { base: "trunk".parse()?, earlier: true });
    /// assert!(!OnOrigin::of_listed(listed, None)?.earlier);
    /// // Only the branch of the item counts.
    /// let other = "ref: refs/heads/main\tHEAD\n3c4d\trefs/heads/x/worktree-issue-12\n";
    /// assert!(!OnOrigin::of_listed(other, Some(&branch))?.earlier);
    /// assert!(OnOrigin::of_listed("3c4d\trefs/heads/worktree-issue-12\n", Some(&branch)).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of_listed(listed: &str, branch: Option<&Branch>) -> Result<OnOrigin> {
        let item = branch.map(|b| format!("refs/heads/{b}"));
        let mut base = None;
        let mut earlier = false;
        for line in listed.lines() {
            match line.split_once('\t') {
                Some((target, "HEAD")) => {
                    base = target.strip_prefix("ref: refs/heads/").or(base);
                }
                Some((_, name)) => earlier |= item.as_deref() == Some(name),
                None => {}
            }
        }
        let base = base.ok_or_else(|| anyhow!("{}", text::ORIGIN_NO_DEFAULT))?;
        Ok(OnOrigin {
            base: base.parse()?,
            earlier,
        })
    }
}

/// Asks `origin` of the main clone `main` with one `git ls-remote
/// --symref`: its default branch, and with `branch`, whether it has the
/// branch of the item (01M7G29PSP7X56X8E60A3KRV4V). The host makes no
/// clone for it.
pub fn on_origin(main: &Path, branch: Option<&Branch>) -> Result<OnOrigin> {
    let remote = crate::host_git::output(main, &["remote", "get-url", "origin"])?;
    let item = branch.map(|b| format!("refs/heads/{b}"));
    let mut args = vec!["ls-remote", "--symref", "--", &remote, "HEAD"];
    args.extend(item.as_deref());
    OnOrigin::of_listed(&git_credential::run(main, &remote, &args)?, branch)
}

/// Before the start of a session of `riff run --no-github`: fetches the
/// default branch `base`, and the branch `branch` of the item when the
/// look on `origin` found it (`earlier`, [`earlier`]), from `origin` into
/// `refs/remotes/origin` of the main clone `main`, with the git of the
/// person (01M7FDN99KZE54CX17DDEP787V). The clone of such a session
/// fetches nothing from GitHub, and starts from the view of the main
/// clone ([`crate::session_clone`]): so it goes on from the branch on
/// `origin` too.
pub fn fetch_for_no_github(
    main: &Path,
    base: &Branch,
    branch: &Branch,
    earlier: bool,
) -> Result<()> {
    let remote = crate::host_git::output(main, &["remote", "get-url", "origin"])?;
    let spec = |name: &Branch| format!("+refs/heads/{name}:refs/remotes/origin/{name}");
    let mut args = vec![
        "fetch".to_owned(),
        "-q".into(),
        "--no-tags".into(),
        "--".into(),
        remote.clone(),
        spec(base),
    ];
    args.extend(earlier.then(|| spec(branch)));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git_credential::run(main, &remote, &args).map(drop)
}

/// What the terminal shows of the events of one session: the lines of
/// each event ([`event_lines`]), and a line when `claude` waits for its
/// background tasks after the `result` of a turn
/// (01M4ZSZX9H59T04QSJ01V4F9TE).
///
/// ```
/// use riff::run::View;
/// use serde_json::json;
///
/// let mut view = View::default();
/// let tasks = json!({"type":"system","subtype":"background_tasks_changed",
///     "tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]});
/// assert!(view.lines(&tasks).is_empty());
/// let result = json!({"type":"result","subtype":"success","num_turns":2,"duration_ms":1000});
/// assert_eq!(
///     view.lines(&result),
///     ["end: success, 2 turns, 1 s, 0.00 USD", "wait: claude waits for 1 background task: wait for just check"]
/// );
/// let none = json!({"type":"system","subtype":"background_tasks_changed","tasks":[]});
/// assert_eq!(view.lines(&none), ["wait: each background task ended"]);
/// ```
#[derive(Debug, Default)]
pub struct View {
    /// The description of each background task that runs.
    tasks: Vec<String>,
    /// True after a `result`, until the next turn starts.
    after_result: bool,
}

impl View {
    /// The lines that the terminal shows for `event`.
    pub fn lines(&mut self, event: &Value) -> Vec<String> {
        let mut lines = event_lines(event);
        match (event["type"].as_str(), event["subtype"].as_str()) {
            (Some("system"), Some("init")) => self.after_result = false,
            (Some("system"), Some("background_tasks_changed")) => {
                self.tasks = event["tasks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|task| first(task["description"].as_str().unwrap_or("?")))
                    .collect();
                if self.after_result {
                    lines.push(self.waits());
                }
            }
            (Some("result"), _) => {
                self.after_result = true;
                if !self.tasks.is_empty() {
                    lines.push(self.waits());
                }
            }
            _ => {}
        }
        lines
    }

    /// The line of the background tasks that `claude` waits for.
    fn waits(&self) -> String {
        match self.tasks.len() {
            0 => "wait: each background task ended".to_owned(),
            1 => format!(
                "wait: claude waits for 1 background task: {}",
                self.tasks[0]
            ),
            n => format!(
                "wait: claude waits for {n} background tasks: {}",
                self.tasks.join("; ")
            ),
        }
    }

    /// Shows one line of the output of the session ([`View::shown`]).
    pub fn show(&mut self, line: &str) {
        for shown in self.shown(line) {
            println!("{shown}");
        }
    }

    /// The lines that the terminal shows for one line of the output of
    /// the session: the lines of its event, or the line itself when it
    /// is not JSON.
    ///
    /// ```
    /// let mut view = riff::run::View::default();
    /// assert_eq!(view.shown("not json"), ["not json"]);
    /// assert!(view.shown(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#).is_empty());
    /// ```
    pub fn shown(&mut self, line: &str) -> Vec<String> {
        match serde_json::from_str::<Value>(line) {
            Ok(event) => self.lines(&event),
            Err(_) => vec![line.to_owned()],
        }
    }
}

/// After the daemon went away, the longest wait of `riff run` for a new
/// daemon (01M4ZKX0F25S284R0N7KZVTHNA).
pub const DAEMON_WAIT: Duration = Duration::from_secs(600);

/// The time between two tries of `riff run` to reach a new daemon.
const DAEMON_RETRY: Duration = Duration::from_millis(250);

/// SIGINT and SIGTERM: the signals that stop `riff run`.
struct Signals {
    streams: [tokio::signal::unix::Signal; 2],
}

impl Signals {
    /// Listens for SIGTERM and SIGINT.
    fn new() -> std::io::Result<Signals> {
        Signals::of(SignalKind::terminate(), SignalKind::interrupt())
    }

    /// Listens for the signals `first` and `second`. From here on, such
    /// a signal does not end riff: tokio keeps its handler to the end of
    /// the process, so riff must act on each signal itself. The streams
    /// keep a signal that comes while riff waits for none, for example
    /// while `gh` runs, for the next [`Signals::recv`].
    fn of(first: SignalKind, second: SignalKind) -> std::io::Result<Signals> {
        Ok(Signals {
            streams: [signal(first)?, signal(second)?],
        })
    }

    /// Waits for the next signal.
    async fn recv(&mut self) {
        let [first, second] = &mut self.streams;
        tokio::select! {
            _ = first.recv() => {}
            _ = second.recv() => {}
        }
    }
}

/// The end steps of `riff run`: the verify request, the release and the
/// end of the session (01M4ZYFJFTX9ZEX7QT4QH7E9T6). A SIGINT or a SIGTERM
/// stops the wait of the step that runs, and riff sends no step after
/// it. One set of signal streams listens from the start of the session
/// to the last end step ([`EndSteps::signals`]): [`follow`] acts on a
/// signal while the session runs, and a signal after the end of the
/// session stops the next end step (01M51JG5Y47184401EWT4HVEG0).
#[derive(Default)]
struct EndSteps {
    /// The streams of the signals, from the start of the session on.
    signals: Option<Signals>,
    /// The steps that riff did not send, after a signal.
    not_sent: Vec<&'static str>,
}

impl EndSteps {
    /// The streams of the signals. The first call makes them.
    fn signals(&mut self) -> std::io::Result<&mut Signals> {
        let signals = match self.signals.take() {
            Some(signals) => signals,
            None => Signals::new()?,
        };
        Ok(self.signals.insert(signals))
    }

    /// Runs `call`, the end step `step`, until it ends or a signal comes.
    /// It gives the result of the call, or `None` when a signal stopped
    /// it or an earlier step. A signal that came before the step stops
    /// it too.
    async fn send<T>(
        &mut self,
        step: &'static str,
        call: impl Future<Output = Result<T>>,
    ) -> Option<Result<T>> {
        if self.not_sent.is_empty() {
            let signals = self.signals().ok();
            let stop = async {
                match signals {
                    Some(signals) => signals.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;
                () = stop => {}
                done = call => return Some(done),
            }
        }
        self.not_sent.push(step);
        None
    }
}

/// Asks the daemon in the folder `daemon` to run the session of
/// `start`, shows its events, and waits for its end ([`follow`]). The
/// streams of `signals` listen from here on.
async fn run_session(daemon: &Path, start: daemon::Start, signals: &mut Signals) -> Result<Ended> {
    let name = start.spec.name.clone();
    let mut view = View::default();
    follow(
        daemon,
        Ask::Start(Box::new(start)),
        &name,
        DAEMON_WAIT,
        signals,
        |line| view.show(line),
    )
    .await
}

/// Sends `ask` to the daemon in the folder `daemon`, gives `show` each
/// event of the session `name`, and waits for its end. A SIGINT or a
/// SIGTERM on `signals` asks the daemon to stop the container. A
/// signal after that one stays on `signals` for the end steps
/// ([`EndSteps`]). When the daemon goes away, the session runs on
/// ([`crate::session_relay`]): riff tries the daemon again for up to
/// `wait`, and follows the session again after the last event that it
/// showed. So it shows each event one time, and loses none
/// (01M4ZKX0F25S284R0N7KZVTHNA). When no daemon comes back, the session
/// failed for `riff run`.
async fn follow(
    daemon: &Path,
    ask: Ask,
    name: &str,
    wait: Duration,
    signals: &mut Signals,
    mut show: impl FnMut(&str),
) -> Result<Ended> {
    let mut name = name.to_owned();
    let mut client = daemon::Client::connect(daemon).await?;
    client.ask(ask).await?;
    let mut stopped = false;
    let mut seen = 0;
    loop {
        let say = tokio::select! {
            say = client.next() => say,
            () = signals.recv(), if !stopped => {
                stopped = true;
                println!("{}", text::run_stopping(&name));
                client.stop().await?;
                continue;
            }
        };
        match say {
            Ok(Some(Say::Started { name: started, .. })) => name = started,
            Ok(Some(Say::Event { seq, line })) if seq > seen => {
                seen = seq;
                show(&line);
            }
            Ok(Some(Say::Event { .. })) => {}
            Ok(Some(Say::Stderr { line })) => eprintln!("{line}"),
            Ok(Some(Say::Line { text })) => println!("{text}"),
            Ok(Some(Say::Ended(ended))) => return Ok(ended),
            Ok(Some(Say::Failed { error })) => bail!("{error}"),
            Ok(Some(_)) => bail!("{}", text::DAEMON_GONE),
            Ok(None) | Err(_) => {
                println!("{}", text::run_daemon_away(&name));
                let Some(again) = connect_again(daemon, wait, signals).await else {
                    println!("{}", text::run_daemon_gone(&name));
                    return Ok(Ended {
                        code: None,
                        failed: true,
                    });
                };
                client = again;
                client
                    .ask(Ask::Follow {
                        name: name.clone(),
                        after: seen,
                    })
                    .await?;
                println!("{}", text::run_daemon_back(&name, seen));
                if stopped {
                    client.stop().await?;
                }
            }
        }
    }
}

/// Tries the daemon in the folder `daemon` again each
/// [`DAEMON_RETRY`], for up to `wait`. A SIGINT or a SIGTERM ends the
/// wait. It gives `None` when no daemon answered.
async fn connect_again(
    daemon: &Path,
    wait: Duration,
    signals: &mut Signals,
) -> Option<daemon::Client> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if let Ok(client) = daemon::Client::connect(daemon).await {
            return Some(client);
        }
        tokio::select! {
            () = tokio::time::sleep(DAEMON_RETRY) => {}
            () = tokio::time::sleep_until(deadline) => return None,
            () = signals.recv() => return None,
        }
    }
}

/// A pull request as `gh pr view --json number,url,headRefOid,state`
/// gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pull {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) head_ref_oid: String,
    pub(crate) state: String,
}

/// `riff run ITEM`: one work item in one container, from the claim to
/// the pull request. With no `image`, it takes the image that the main
/// clone pins. With `no_github`, the session has no GitHub, and pushes
/// nothing (01M6Z87JY83V3KR72E2AG8V8KG). It gives the exit code of the
/// command. On Linux, it runs [`run_with`] with the daemon of this
/// machine ([`daemon::dir`]). On a host that is not Linux, it starts
/// nothing (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
pub async fn run(
    api: Api,
    here: &Place,
    server: &str,
    item: &str,
    image: Option<&str>,
    no_github: bool,
) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        run_with(&daemon::dir()?, api, here, server, item, image, no_github).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (api, here, server, item, image, no_github);
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}

/// [`run`] with the daemon in the folder `daemon`.
pub async fn run_with(
    daemon: &Path,
    api: Api,
    here: &Place,
    server: &str,
    item: &str,
    image: Option<&str>,
    no_github: bool,
) -> Result<i32> {
    let issue = issue_of(item)?;
    let dir = identity::working_dir()?;
    let (main, common) = main_clone(&dir)?;
    // With --image, the image is the one that the person gives, and the
    // lines about the pin say nothing. The model, the env and the volumes
    // still come from the merged riff.toml (01M6VAQWECJ6N6VJF66KCMNYD1,
    // 01M5SMVVC4GQS310CVK3H9CMQ7).
    let merged = match image {
        Some(_) => session_image::merged_config(&main, |_| {})?,
        None => session_image::merged_config(&main, |line| println!("{line}"))?,
    };
    let image = image.map_or_else(|| merged.image.to_string(), str::to_owned);
    let image = image.as_str();
    let checked = check(daemon, issue, image).await?;
    let Some(thread) = here.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };

    let session = identity::new_session_id();
    let place = Place::new(here.host(), here.repo().clone(), Some(item))?;
    let me: SessionUri = identity::agent(&place, &session, server)?;
    let api = api.signed_in(Some(&session))?;
    api.start(&me, StartReason::Process, true).await?;
    let claimed = api.claim(&me, &thread, item).await?;
    println!("{}", text::claimed(&claimed, &thread, item));
    if !claimed.granted {
        let _ = api.end(&me).await;
        return Ok(1);
    }
    let alive = {
        let (api, me) = (api.clone(), me.clone());
        tokio::spawn(
            async move { api::keep_alive_in_container(&api, &me, WORKER_ALIVE_EVERY).await },
        )
    };
    if let Some(line) = dropped::at_claim(&main, item).await {
        println!("{line}");
    }
    if let Some(line) = rollout::at_claim(&main, &here.repo_text(), item).await {
        println!("{line}");
    }

    let mut steps = EndSteps::default();
    let worked = work(
        daemon,
        server,
        &api,
        &me,
        &thread,
        &main,
        &common,
        &checked,
        &session,
        item,
        issue,
        (image, &merged.setup, no_github),
        &mut steps,
    )
    .await;
    let ending = api.with_budget(END_BUDGET);
    let release = ending.release(&me, &thread, item);
    if let Some(Err(e)) = steps.send(text::RUN_RELEASE, release).await {
        eprintln!("riff: {e:#}");
    }
    alive.abort();
    let _ = steps.send(text::RUN_END, api.end(&me)).await;
    if steps.not_sent.is_empty() {
        return worked;
    }
    println!("{}", text::run_not_sent(item, &steps.not_sent));
    worked.map(|_| 1)
}

/// The part of [`run`] between the claim and the release.
#[allow(clippy::too_many_arguments)]
async fn work(
    daemon: &Path,
    server: &str,
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    main: &Path,
    common: &Path,
    checked: &Checked,
    session: &str,
    item: &str,
    issue: u64,
    (image, setup, no_github): (&str, &Setup, bool),
    steps: &mut EndSteps,
) -> Result<i32> {
    let folder = sessions_dir()?.join(format!("{item}-{}", short(session)));
    // The sweep of the daemon keeps the folder while this lock is held
    // (01M53NJ5V27BCHDNHK2THQQZZQ).
    let _held = daemon::sweep::hold(&folder)
        .with_context(|| format!("cannot lock {}", folder.display()))?;
    // The sweep reads the repository here, not in the clone
    // (01M7DZ42EVHAT8N5MZEV97G3DW).
    daemon::sweep::note_repo(&folder, &me.place().repo_text())?;
    let clone = clone_in(&folder);
    let claude = claude_in(&folder);
    let sources = session_image::merged_sources(main)?;
    let auto = crate::permissions::Trust::of(me.place().repo(), server, &sources);
    let home = &checked.host.home;
    claude_folder(
        &claude,
        &clone,
        &home.join(".claude.json"),
        &auto,
        crate::permissions::Role::Worker,
    )?;
    let branch: Branch = branch(item).parse()?;
    // One look on `origin`: its default branch, and the branch of the item
    // (01M7G29PSP7X56X8E60A3KRV4V).
    let OnOrigin { base, earlier } = on_origin(main, Some(&branch))?;
    let repo = me.place().repo_text();
    let plan = Plan {
        path: clone,
        origin: Origin::github(&repo)?,
        main: common.to_owned(),
        refs: refs_in(&folder),
        base,
        checkout: Checkout::Branch {
            name: branch.clone(),
            earlier,
            no_github,
        },
    };
    // With no GitHub, the clone of the session fetches nothing: the main
    // clone gets the branches first (01M7FDN99KZE54CX17DDEP787V). The
    // relay gives the branch in a bundle, and `riff run` takes a copy
    // (01M7EY3WHQQP9330CZ736HK0FJ).
    let bundle = plan
        .bundle_file()
        .map(|out| (out, bundle_copy(&folder, &branch)));
    if let Some((out, _)) = &bundle {
        fetch_for_no_github(main, &plan.base, &branch, earlier)?;
        if let Some(out) = out.parent() {
            std::fs::create_dir_all(out)
                .with_context(|| format!("cannot make {}", out.display()))?;
        }
    }
    // The list of the branches of `origin`, after the fetch: the clone of
    // the session starts from it (01M7FMJJB0TZM8875T7GQZAJEA).
    crate::session_clone::write_refs(common, &folder)?;
    let existing = |p: PathBuf| p.exists().then_some(p);
    let facts = Facts {
        session: session.to_owned(),
        item: item.to_owned(),
        image: image.to_owned(),
        home: home.clone(),
        user: checked.host.user.clone(),
        plan,
        claude,
        sign_in: checked.host.claude.sign_in(),
        gitconfig: existing(home.join(".gitconfig")),
        setup: setup.clone(),
    };
    let spec = spec(&facts);
    let base = &facts.plan.base;
    println!(
        "{}",
        text::run_started(item, session, &facts.plan.path, image, &spec.name)
    );
    let branch = branch.as_str();
    let start = if no_github {
        // The session has no GitHub: the start text holds the issue
        // (01M6Z87JY83V3KR72E2AG8V8KG).
        let body = &checked.frame.issue;
        text::run_start_no_github(&repo, issue, branch, session, body, earlier)
    } else {
        let frame = checked.frame.body()?;
        let frame = pr::with_session(&frame, &made_by(&pr::Gh::here("run"), session))?;
        text::run_start(
            &repo,
            base.as_str(),
            issue,
            branch,
            session,
            &frame,
            &checked.frame.milestone,
            earlier,
        )
    };
    let secrets = checked.host.secrets();
    let values = checked.host.values();
    // The handlers come first: a signal before them ends riff with no
    // release. The same streams serve the end steps.
    let signals = steps.signals()?;
    let ended = run_session(
        daemon,
        daemon::Start {
            session: session.to_owned(),
            item: item.to_owned(),
            folder,
            spec,
            start,
            values,
            secrets,
            server: server.to_owned(),
            me: me.clone(),
            terminal: false,
            cause: None,
        },
        signals,
    )
    .await?;

    let pull = pr::Gh::here("run")
        .json::<Pull>(&[
            "pr",
            "view",
            branch,
            "--json",
            "number,url,headRefOid,state",
        ])
        .ok()
        .filter(|p| p.state == "OPEN");
    if let Some(pull) = &pull {
        println!("{}", text::run_pull(pull.number, &pull.url));
        let body = text::run_verify_request(
            item,
            pull.number,
            branch,
            &pull.head_ref_oid,
            short(session),
        );
        let to = [Selector::lead(&repo)];
        let ending = api.with_budget(END_BUDGET);
        let ask = async {
            let gh = pr::Gh::here("run");
            if let Some(why) = no_verify_now(&ending, me, thread, gh, item, issue).await? {
                println!("{}", text::run_no_verify(item, &why));
                return Ok(());
            }
            ending
                .post(me, Some(thread), &to, &body, Kind::Message)
                .await
                .map(drop)
        };
        if let Some(Err(e)) = steps.send(text::RUN_VERIFY_REQUEST, ask).await {
            eprintln!("riff: {e:#}");
        }
    }
    let delivered = match &bundle {
        // A session with no GitHub pushed nothing: the person pushes the
        // branch from the copy of its bundle that riff checked, also with
        // an open pull request (01M7EY3WHQQP9330CZ736HK0FJ,
        // 01M7FDN9C91TDYPQSFBRHVHRAC).
        Some((out, copy)) => match take_bundle(out, copy, common) {
            Ok(taken) => {
                println!("{}", text::run_pushed_nothing(taken.as_deref(), branch));
                true
            }
            Err(e) => {
                println!("{}", text::run_bundle_refused(out, &format!("{e:#}")));
                false
            }
        },
        None if pull.is_some() => true,
        None => {
            println!("{}", text::run_no_pull(branch));
            false
        }
    };
    Ok(i32::from(
        !(ended.code == Some(0) && !ended.failed && delivered),
    ))
}

/// The length of the first line of a git bundle that riff takes
/// ([`BUNDLE_HEADS`]).
pub const BUNDLE_HEAD: usize = 16;

/// The first lines of a git bundle that riff takes: of version 2 and of
/// version 3 ([`take_bundle`]).
pub const BUNDLE_HEADS: [&[u8; BUNDLE_HEAD]; 2] = [b"# v2 git bundle\n", b"# v3 git bundle\n"];

/// The most bytes of a bundle that `riff run` copies ([`take_bundle`]):
/// 1 GiB (01M7FDN9EWZ28AQS4CKJQD3A3B).
pub const BUNDLE_LIMIT: u64 = 1 << 30;

/// Takes the bundle `out` that a session with no GitHub wrote
/// ([`crate::session_clone::Plan::bundle`], 01M7EY3WHQQP9330CZ736HK0FJ).
/// The session can write the folder of `out`, so riff trusts no file
/// there, and runs no git on it (#2169). These are the checks, in this
/// order:
///
/// 1. It opens `out` with no link (`O_NOFOLLOW`), and only a plain file
///    counts: no link, no folder, no pipe.
/// 2. The file has at most [`BUNDLE_LIMIT`] bytes.
/// 3. Its first [`BUNDLE_HEAD`] bytes, read from the open file, are one
///    of [`BUNDLE_HEADS`]: a `gitdir:` file is no bundle, riff copies no
///    more of it, and git never reads it as a repository.
/// 4. It copies the rest of that open file to the new file `copy`, in the
///    folder of the session, which no session sees, up to the limit: a
///    file that grows over the limit during the copy is refused. A new
///    file in the place of `out` after the open does not change the copy.
/// 5. `git bundle unbundle` of the copy, in the git dir `main` of the
///    main clone, into a new folder of objects beside the copy, with the
///    objects of the main clone as its alternates
///    (01M7FMJJDJN9Q5ZPA7ECTGK1MD). git checks the refs and the
///    prerequisites of the bundle, as `git bundle verify` does, and each
///    object of its pack, as `git index-pack` does: a pack that is cut or
///    zeroed fails here. The main clone gets no object, and the folder of
///    objects goes.
///
/// It gives the copy, or `None` with no file at `out`: no commit to
/// give. On each fault, the copy goes.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let (out, copy) = (dir.path().join("out.bundle"), dir.path().join("copy.bundle"));
/// assert_eq!(riff::run::take_bundle(&out, &copy, dir.path())?, None);
/// // A gitdir file is no bundle: riff runs no git on it.
/// std::fs::write(&out, "gitdir: /tmp/elsewhere\n")?;
/// let error = riff::run::take_bundle(&out, &copy, dir.path()).unwrap_err();
/// assert_eq!(error.to_string(), "it is no git bundle");
/// assert!(!copy.exists());
/// // A link is no plain file.
/// std::fs::remove_file(&out)?;
/// std::os::unix::fs::symlink(dir.path().join("elsewhere"), &out)?;
/// assert!(riff::run::take_bundle(&out, &copy, dir.path()).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn take_bundle(out: &Path, copy: &Path, main: &Path) -> Result<Option<PathBuf>> {
    match Opened::of(out, BUNDLE_LIMIT)? {
        None => Ok(None),
        Some(opened) => opened.take(copy, main, BUNDLE_LIMIT).map(Some),
    }
}

/// A bundle that [`take_bundle`] opened, with the limit of its size: a
/// plain file with no link, and its first line.
struct Opened {
    /// The path of the file in `out/`.
    out: PathBuf,
    /// The open file, after its first line.
    file: std::fs::File,
    /// Its first line: one of [`BUNDLE_HEADS`].
    head: [u8; BUNDLE_HEAD],
}

impl Opened {
    /// The checks 1 to 3 of [`take_bundle`] of the file `out`, with the
    /// limit `limit`. `None` with no file at `out`.
    fn of(out: &Path, limit: u64) -> Result<Option<Opened>> {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(out);
        let mut file = match opened {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) if e.raw_os_error() == Some(nix::libc::ELOOP) => bail!("it is a link"),
            Err(e) => return Err(e).with_context(|| format!("cannot open {}", out.display())),
            Ok(file) => file,
        };
        let meta = file
            .metadata()
            .with_context(|| format!("cannot read {}", out.display()))?;
        if !meta.is_file() {
            bail!("it is no plain file");
        }
        if meta.len() > limit {
            bail!("{}", text::run_bundle_too_large(limit));
        }
        let mut head = [0; BUNDLE_HEAD];
        match file.read_exact(&mut head) {
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => bail!("it is no git bundle"),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", out.display())),
            Ok(()) => {}
        }
        if !BUNDLE_HEADS.contains(&&head) {
            bail!("it is no git bundle");
        }
        Ok(Some(Opened {
            out: out.to_owned(),
            file,
            head,
        }))
    }

    /// The checks 4 and 5 of [`take_bundle`], with the limit `limit`: the
    /// new file `copy`, and `git bundle unbundle` of it in the git dir
    /// `main`. It gives the copy. On each fault, the copy goes.
    fn take(self, copy: &Path, main: &Path, limit: u64) -> Result<PathBuf> {
        let checked = self
            .copy_to(copy, limit)
            .and_then(|()| unbundle(copy, main));
        match checked {
            Ok(()) => Ok(copy.to_owned()),
            Err(e) => {
                let _ = std::fs::remove_file(copy);
                Err(e)
            }
        }
    }

    /// Copies the first line and the rest of the open file to the new file
    /// `copy`, up to `limit` bytes: one byte more refuses the file.
    fn copy_to(mut self, copy: &Path, limit: u64) -> Result<()> {
        use std::io::{Read, Write};
        match std::fs::remove_file(copy) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("cannot remove {}", copy.display()));
            }
            _ => {}
        }
        let mut to = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(copy)
            .with_context(|| format!("cannot make {}", copy.display()))?;
        to.write_all(&self.head)
            .with_context(|| format!("cannot write {}", copy.display()))?;
        let head = BUNDLE_HEAD as u64;
        let rest = (limit + 1).saturating_sub(head);
        let copied = std::io::copy(&mut (&mut self.file).take(rest), &mut to)
            .with_context(|| format!("cannot copy {} to {}", self.out.display(), copy.display()))?;
        if copied + head > limit {
            bail!("{}", text::run_bundle_too_large(limit));
        }
        Ok(())
    }
}

/// The check 5 of [`take_bundle`]: `git bundle unbundle` of the copy
/// `copy` in the git dir `main` of the main clone, into a new folder of
/// objects beside the copy that goes after, with the objects of `main` as
/// its alternates.
fn unbundle(copy: &Path, main: &Path) -> Result<()> {
    let folder = copy.parent().unwrap_or_else(|| Path::new("."));
    let objects = tempfile::Builder::new()
        .prefix(".unbundle-")
        .tempdir_in(folder)
        .with_context(|| format!("cannot make a folder of objects in {}", folder.display()))?;
    let at = copy.to_string_lossy();
    let own = main.join(crate::session_clone::OBJECTS);
    let vars = [
        ("GIT_OBJECT_DIRECTORY", objects.path()),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", own.as_path()),
    ];
    crate::host_git::output_with(main, &["bundle", "unbundle", &at], &vars).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The branch `text`.
    fn name(text: &str) -> Branch {
        text.parse().unwrap()
    }

    /// Runs git in `dir` with no config of the person.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "riff test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "riff test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// #2169, 01M7EY3WHQQP9330CZ736HK0FJ: a session writes a `gitdir:` file
    /// in the place of its bundle. It names a git dir whose config names a
    /// program. `riff run` refuses it, runs no git on it, and the program
    /// does not run. A link is no plain file. A real bundle gives its copy,
    /// with its checks, and a clone fetches the branch from the copy. The
    /// main clone gets no object of the bundle.
    #[test]
    fn riff_run_refuses_a_gitdir_file_in_the_place_of_the_bundle() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        let main = root.join("main");
        let git_dir = main.join(".git");
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        let mark = root.join("mark");
        let hostile = root.join("hostile");
        crate::host_git::hostile_clone(&hostile, &mark);
        let out = root.join("out/worktree-issue-12.bundle");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        let gitdir = format!("gitdir: {}\n", hostile.join(".git").display());
        std::fs::write(&out, gitdir).unwrap();
        let copy = root.join("worktree-issue-12.bundle");

        let error = take_bundle(&out, &copy, &git_dir).unwrap_err();
        assert_eq!(error.to_string(), "it is no git bundle");
        assert!(!copy.exists());
        assert!(!mark.exists(), "the program of the git dir ran");

        let link = root.join("out/link.bundle");
        std::os::unix::fs::symlink(&out, &link).unwrap();
        let error = take_bundle(&link, &copy, &git_dir).unwrap_err();
        assert!(error.to_string().contains("a link"), "{error}");

        let other = root.join("other");
        git(root, &["clone", "-q", "origin.git", "other"]);
        git(&other, &["switch", "-q", "-c", "worktree-issue-12"]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", "work"]);
        let at = out.to_string_lossy();
        let range = "origin/main..worktree-issue-12";
        git(&other, &["bundle", "create", "-q", &at, range]);
        let objects = git(&main, &["count-objects", "-v"]);
        assert_eq!(
            take_bundle(&out, &copy, &git_dir).unwrap(),
            Some(copy.clone())
        );
        assert_eq!(git(&main, &["count-objects", "-v"]), objects);
        let left: Vec<_> = std::fs::read_dir(root).unwrap().flatten().collect();
        assert!(
            left.iter()
                .all(|e| !e.file_name().to_string_lossy().starts_with(".unbundle")),
            "{left:?}"
        );
        git(
            &main,
            &["fetch", "-q", &copy.to_string_lossy(), "worktree-issue-12"],
        );
        assert_eq!(
            git(&main, &["log", "-1", "--format=%s", "FETCH_HEAD"]),
            "work"
        );
    }

    /// 01M7FDN9EWZ28AQS4CKJQD3A3B: `riff run` refuses a file of more than
    /// [`BUNDLE_LIMIT`] with the first line of a bundle: here the large
    /// file is sparse, so the test writes no 1 GiB. It refuses a file
    /// whose first line is no bundle before the copy: here the folder of
    /// the copy is not there, and the error names the first line. A file
    /// that is shorter than the first line is no bundle.
    #[test]
    fn riff_run_refuses_a_bundle_over_the_limit_and_a_bad_first_line() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let (out, copy) = (root.join("out.bundle"), root.join("copy.bundle"));
        let large = std::fs::File::create(&out).unwrap();
        std::io::Write::write_all(&mut &large, BUNDLE_HEADS[0]).unwrap();
        large.set_len(BUNDLE_LIMIT + 1).unwrap();
        let error = take_bundle(&out, &copy, root).unwrap_err();
        assert_eq!(error.to_string(), text::run_bundle_too_large(BUNDLE_LIMIT));
        assert!(!copy.exists());
        drop(large);
        let nowhere = root.join("none/copy.bundle");
        std::fs::write(&out, "# v4 git bundle\nmore bytes of no bundle\n").unwrap();
        let error = take_bundle(&out, &nowhere, root).unwrap_err();
        assert_eq!(error.to_string(), "it is no git bundle");
        std::fs::write(&out, "# v2").unwrap();
        let error = take_bundle(&out, &copy, root).unwrap_err();
        assert_eq!(error.to_string(), "it is no git bundle");
        assert!(!copy.exists());
    }

    /// 01M7FDN9EWZ28AQS4CKJQD3A3B: the session writes more in its bundle
    /// after riff opened it, far over the limit. The copy stops one byte
    /// over the limit: the copy has `limit + 1` bytes, not the whole file.
    /// riff refuses the file, and the copy goes. Here the limit is 64
    /// bytes, and the file grows to 4 KiB.
    #[test]
    fn riff_run_refuses_a_bundle_that_grows_over_the_limit_during_the_copy() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let (out, copy) = (root.join("out.bundle"), root.join("copy.bundle"));
        let limit = 64;
        std::fs::write(&out, [&BUNDLE_HEADS[0][..], &[b'x'; 32]].concat()).unwrap();
        let copied = Opened::of(&out, limit).unwrap().unwrap();
        let taken = Opened::of(&out, limit).unwrap().unwrap();
        let mut more = std::fs::OpenOptions::new().append(true).open(&out).unwrap();
        more.write_all(&[b'x'; 4096]).unwrap();

        let error = copied.copy_to(&copy, limit).unwrap_err();
        assert_eq!(error.to_string(), text::run_bundle_too_large(limit));
        assert_eq!(std::fs::metadata(&copy).unwrap().len(), limit + 1);

        let error = taken.take(&copy, root, limit).unwrap_err();
        assert_eq!(error.to_string(), text::run_bundle_too_large(limit));
        assert!(!copy.exists());
        // At the open, the larger file is refused.
        assert!(Opened::of(&out, limit).is_err());
    }

    /// 01M7FDN9EWZ28AQS4CKJQD3A3B, 01M7FMJJDJN9Q5ZPA7ECTGK1MD: `riff run`
    /// takes a bundle of version 3, and a bundle of exactly the limit:
    /// here the limit is the size of the bundle. A limit of one byte less
    /// refuses it at the open. A file that grows to exactly the limit
    /// after the open gives the whole bundle.
    #[test]
    fn riff_run_takes_a_bundle_of_version_3_and_of_exactly_the_limit() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "-b", "main", "main"]);
        git(root, &["init", "-q", "-b", "main", "other"]);
        let (main, other) = (root.join("main"), root.join("other"));
        let git_dir = main.join(".git");
        git(&other, &["commit", "-q", "--allow-empty", "-m", "work"]);
        let (out, copy) = (root.join("out.bundle"), root.join("copy.bundle"));
        let at = out.to_string_lossy();
        git(
            &other,
            &["bundle", "create", "-q", "--version=3", &at, "main"],
        );
        let bundle = std::fs::read(&out).unwrap();
        assert!(bundle.starts_with(BUNDLE_HEADS[1]), "{bundle:?}");
        let limit = u64::try_from(bundle.len()).unwrap();

        let error = Opened::of(&out, limit - 1).err().unwrap();
        assert_eq!(error.to_string(), text::run_bundle_too_large(limit - 1));
        let opened = Opened::of(&out, limit).unwrap().unwrap();
        assert_eq!(opened.take(&copy, &git_dir, limit).unwrap(), copy);
        assert_eq!(std::fs::read(&copy).unwrap(), bundle);

        let (last, start) = bundle.split_last().unwrap();
        std::fs::write(&out, start).unwrap();
        let opened = Opened::of(&out, limit).unwrap().unwrap();
        let mut more = std::fs::OpenOptions::new().append(true).open(&out).unwrap();
        more.write_all(&[*last]).unwrap();
        assert_eq!(opened.take(&copy, &git_dir, limit).unwrap(), copy);
        assert_eq!(std::fs::read(&copy).unwrap(), bundle);
        git(&main, &["fetch", "-q", &copy.to_string_lossy(), "main"]);
        assert_eq!(
            git(&main, &["log", "-1", "--format=%s", "FETCH_HEAD"]),
            "work"
        );
    }

    /// 01M7EY3WHQQP9330CZ736HK0FJ: after riff opened the bundle, the
    /// session puts a new file in its place, then a link to a file that
    /// the session must not give. The copy is the bundle that riff opened
    /// each time, and it unbundles.
    #[test]
    fn a_new_file_or_a_link_in_the_place_of_the_bundle_after_the_open_does_not_change_the_copy() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        git(root, &["clone", "-q", "origin.git", "other"]);
        let (main, other) = (root.join("main"), root.join("other"));
        let git_dir = main.join(".git");
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        git(&other, &["pull", "-q", "origin", "main"]);
        git(&other, &["switch", "-q", "-c", "worktree-issue-12"]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", "work"]);
        let out = root.join("out.bundle");
        let at = out.to_string_lossy();
        let range = "origin/main..worktree-issue-12";
        git(&other, &["bundle", "create", "-q", &at, range]);
        let bundle = std::fs::read(&out).unwrap();
        let secret = root.join("secret");
        std::fs::write(&secret, [&BUNDLE_HEADS[0][..], b"a secret\n"].concat()).unwrap();
        let copy = root.join("copy.bundle");

        let new_file = |out: &Path| {
            std::fs::remove_file(out).unwrap();
            std::fs::write(out, [&BUNDLE_HEADS[0][..], b"other bytes\n"].concat()).unwrap();
        };
        let link = |out: &Path| {
            std::fs::remove_file(out).unwrap();
            std::os::unix::fs::symlink(&secret, out).unwrap();
        };
        let swaps: [&dyn Fn(&Path); 2] = [&new_file, &link];
        for swap in swaps {
            std::fs::write(&out, &bundle).unwrap();
            let opened = Opened::of(&out, BUNDLE_LIMIT).unwrap().unwrap();
            swap(&out);
            assert_eq!(
                opened.take(&copy, &git_dir, BUNDLE_LIMIT).unwrap(),
                copy.clone()
            );
            assert_eq!(std::fs::read(&copy).unwrap(), bundle);
            std::fs::remove_file(&out).unwrap();
        }
    }

    /// The attacker of #2139: a session writes a bundle whose second line
    /// holds the C1 control U+009B and an escape sequence. `git bundle
    /// unbundle` refuses it, and its error quotes the line. The text that
    /// `riff run` prints holds no control character of the line.
    #[test]
    fn the_text_of_a_refused_bundle_holds_no_control_character_of_the_session() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "-b", "main", "main"]);
        let git_dir = root.join("main/.git");
        let (out, copy) = (root.join("out.bundle"), root.join("copy.bundle"));
        let line = "\u{9b}2J\x1b]0;title\x07\x1b[31mred";
        std::fs::write(&out, format!("# v2 git bundle\n{line}\n\n")).unwrap();

        let error = format!("{:#}", take_bundle(&out, &copy, &git_dir).unwrap_err());
        assert!(error.contains('\u{9b}'), "git quotes the line: {error:?}");
        let shown = text::run_bundle_refused(&out, &error);
        assert!(shown.contains("bundle unbundle"), "{shown}");
        let control = |c: char| c.is_control() && c != '\n';
        assert!(!shown.chars().any(control), "{shown:?}");
        assert!(!copy.exists());
    }

    /// 01M7EY3WHQQP9330CZ736HK0FJ: a folder and a pipe in the place of the
    /// bundle are no plain file. A bundle that `git bundle unbundle`
    /// refuses gives no copy: one whose commit the main clone does not
    /// have, one whose pack is cut after the header, and one whose pack
    /// is zeroed. `git bundle verify` takes the last two.
    #[test]
    fn riff_run_refuses_a_folder_a_pipe_and_a_bundle_that_does_not_unbundle() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let copy = root.join("copy.bundle");
        let folder = root.join("folder.bundle");
        std::fs::create_dir(&folder).unwrap();
        let error = take_bundle(&folder, &copy, root).unwrap_err();
        assert!(error.to_string().contains("no plain file"), "{error}");
        let pipe = root.join("pipe.bundle");
        nix::unistd::mkfifo(&pipe, nix::sys::stat::Mode::S_IRWXU).unwrap();
        let error = take_bundle(&pipe, &copy, root).unwrap_err();
        assert!(error.to_string().contains("no plain file"), "{error}");

        git(root, &["init", "-q", "-b", "main", "main"]);
        git(root, &["init", "-q", "-b", "main", "other"]);
        let (main, other) = (root.join("main"), root.join("other"));
        let git_dir = main.join(".git");
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", "one"]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", "two"]);
        let out = root.join("out.bundle");
        let at = out.to_string_lossy();
        git(&other, &["bundle", "create", "-q", &at, "HEAD~1..HEAD"]);
        let error = take_bundle(&out, &copy, &git_dir).unwrap_err();
        assert!(
            format!("{error:#}").contains("bundle unbundle"),
            "{error:#}"
        );
        assert!(!copy.exists());

        git(&main, &["fetch", "-q", &other.to_string_lossy(), "main"]);
        git(&other, &["bundle", "create", "-q", &at, "HEAD~1..HEAD"]);
        let bundle = std::fs::read(&out).unwrap();
        let pack = bundle.windows(2).position(|w| w == b"\n\n").unwrap() + 2;
        let cut = [&bundle[..pack], &bundle[pack..pack + 12]].concat();
        let zeroed = [&bundle[..pack], &vec![0; bundle.len() - pack][..]].concat();
        for broken in [cut, zeroed] {
            std::fs::write(&out, broken).unwrap();
            git(&main, &["bundle", "verify", "-q", &at]);
            let error = take_bundle(&out, &copy, &git_dir).unwrap_err();
            assert!(
                format!("{error:#}").contains("bundle unbundle"),
                "{error:#}"
            );
            assert!(!copy.exists());
        }
        std::fs::write(&out, bundle).unwrap();
        assert_eq!(
            take_bundle(&out, &copy, &git_dir).unwrap(),
            Some(copy.clone())
        );
    }

    /// 01M7FDN99KZE54CX17DDEP787V: when the fetch of `riff run
    /// --no-github` fails, the error names the fetch: here `origin` has
    /// no branch `gone`.
    #[test]
    fn a_fetch_for_no_github_that_fails_names_the_fetch() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        let main = root.join("main");
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        let error = fetch_for_no_github(&main, &name("gone"), &name("worktree-issue-12"), false)
            .unwrap_err();
        assert!(format!("{error:#}").contains("fetch"), "{error:#}");
        assert!(format!("{error:#}").contains("gone"), "{error:#}");
    }

    /// 01M7FDN99KZE54CX17DDEP787V: before a session of `riff run
    /// --no-github`, the main clone gets the default branch and the branch
    /// of the item from `origin`, which another clone pushed. With no
    /// branch on `origin`, it gets the default branch only.
    #[test]
    fn with_no_github_the_main_clone_gets_the_branches_of_origin_first() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        git(root, &["clone", "-q", "origin.git", "other"]);
        let (main, other) = (root.join("main"), root.join("other"));
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        git(&other, &["pull", "-q", "origin", "main"]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", "newer"]);
        git(&other, &["push", "-q", "origin", "HEAD:main"]);
        let seen = |name: &str| {
            git(
                &main,
                &[
                    "for-each-ref",
                    "--format=%(subject)",
                    &format!("refs/remotes/origin/{name}"),
                ],
            )
        };
        fetch_for_no_github(&main, &name("main"), &name("worktree-issue-12"), false).unwrap();
        assert_eq!(seen("main"), "newer");
        assert_eq!(seen("worktree-issue-12"), "");
        git(
            &other,
            &["commit", "-q", "--allow-empty", "-m", "WIP: earlier"],
        );
        git(&other, &["push", "-q", "origin", "HEAD:worktree-issue-12"]);
        let earlier = earlier(&main, &name("worktree-issue-12")).unwrap();
        assert!(earlier);
        fetch_for_no_github(&main, &name("main"), &name("worktree-issue-12"), earlier).unwrap();
        assert_eq!(seen("worktree-issue-12"), "WIP: earlier");
    }

    /// 01M7DZ42C8SY2GV5ZQJZHM2VTV: the host looks for the branch of the
    /// item on `origin` from the main clone, with `git ls-remote`. It
    /// makes no clone: the session clones in its container
    /// ([`crate::session_clone`]).
    #[test]
    fn the_host_looks_for_the_branch_on_origin_from_the_main_clone() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        let main = root.join("main");
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        assert!(!earlier(&main, &name("worktree-issue-12")).unwrap());
        git(&main, &["push", "-q", "origin", "HEAD:worktree-issue-12"]);
        assert!(earlier(&main, &name("worktree-issue-12")).unwrap());
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 2);
    }

    /// 01M7G29PSP7X56X8E60A3KRV4V: one `git ls-remote --symref` gives the
    /// default branch of `origin` and the branch of the item. The default
    /// branch moves on `origin` to `trunk`, and `origin/HEAD` of the main
    /// clone stays on `main`: the host names `trunk`. When the `HEAD` of
    /// `origin` names no branch, no session starts, and the error says why.
    #[test]
    fn the_host_asks_origin_for_its_default_branch() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "main"]);
        let (main, origin) = (root.join("main"), root.join("origin.git"));
        git(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
        git(&main, &["push", "-q", "origin", "HEAD:main"]);
        git(&main, &["remote", "set-head", "origin", "main"]);
        let item = name("worktree-issue-12");
        let on = |base: &str, earlier| OnOrigin {
            base: name(base),
            earlier,
        };
        assert_eq!(on_origin(&main, Some(&item)).unwrap(), on("main", false));

        git(
            &main,
            &[
                "push",
                "-q",
                "origin",
                "HEAD:trunk",
                "HEAD:worktree-issue-12",
            ],
        );
        git(&origin, &["symbolic-ref", "HEAD", "refs/heads/trunk"]);
        assert_eq!(on_origin(&main, Some(&item)).unwrap(), on("trunk", true));
        assert_eq!(on_origin(&main, None).unwrap(), on("trunk", false));
        let stale = git(&main, &["symbolic-ref", "refs/remotes/origin/HEAD"]);
        assert_eq!(stale, "refs/remotes/origin/main");

        git(&origin, &["symbolic-ref", "HEAD", "refs/heads/gone"]);
        let error = on_origin(&main, Some(&item)).unwrap_err();
        assert_eq!(error.to_string(), text::ORIGIN_NO_DEFAULT);
    }

    /// #1858, 01M749XKWD72JYB3QD8RYNHKTV: a step of the host with a time
    /// limit gives its output, its error, or, at the end of the limit, an
    /// error that names it. riff stops it, and each process that it
    /// started.
    #[test]
    fn a_step_of_the_host_that_hangs_ends_at_its_limit() {
        let dir = Path::new("/");
        let limit = Duration::from_secs(5);
        assert_eq!(
            output_within("sh", dir, &["-c", "echo 5.2.1"], limit).unwrap(),
            "5.2.1"
        );
        let failed = output_within("sh", dir, &["-c", "echo no >&2; exit 3"], limit);
        assert_eq!(
            failed.unwrap_err().to_string(),
            "sh -c echo no >&2; exit 3: no"
        );

        let short = Duration::from_millis(300);
        let root = tempfile::tempdir().unwrap();
        let mark = root.path().join("child");
        let script = format!("sleep 30 & echo $! > {}; wait", mark.display());
        let start = std::time::Instant::now();
        let hung = output_within("sh", dir, &["-c", &script], short).unwrap_err();
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(
            hung.to_string(),
            text::no_answer("sh", &format!("-c {script}"), short)
        );
        // The stop ends the child of the program too: it is gone, or a
        // zombie that nobody reaped yet.
        let child = std::fs::read_to_string(&mark).unwrap().trim().to_owned();
        let gone = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            let state =
                output_within("ps", dir, &["-o", "stat=", "-p", &child], limit).unwrap_or_default();
            state.is_empty() || state.starts_with('Z')
        });
        assert!(gone, "the child {child} still runs");

        let none = output_within("no-such-program-riff", dir, &[], limit).unwrap_err();
        assert_eq!(none.to_string(), text::no_program("no-such-program-riff"));
    }

    /// #1747, 01M6WW5WSSRMTBP882FQP3TS2Q: the frame of the body in the
    /// start text of a build has `Refs #12` for an issue with a line
    /// `Live:` in "Done when", and `Closes #12` for another issue. An
    /// issue with no milestone has no frame.
    #[test]
    fn the_frame_of_an_issue_with_a_live_line_has_refs() {
        let dir = tempfile::tempdir().unwrap();
        let gh = |body: &str| {
            let program = isolated::write_program(
                dir.path().join("gh"),
                format!(
                    "#!/bin/sh\n\
                     [ \"$*\" = 'issue view 12 --json number,state,milestone,body' ] || exit 1\n\
                     printf '%s' '{body}'\n"
                ),
            );
            pr::Gh::at(program)
        };
        let wave = r#""number":12,"state":"OPEN","milestone":{"title":"Wave 3"}"#;
        let live = format!(
            r#"{{{wave},"body":"Text.\n\n## Done when:\n\n- A test.\n- Live: it runs on pangolin.\n"}}"#
        );
        let built = frame(&gh(&live), 12).unwrap();
        assert_eq!(built.link, pr::Link::Live(12));
        assert_eq!(
            built.body().unwrap(),
            "Refs #12\n\nSUMMARY\n\nIssue: #12\nMilestone: Wave 3\n"
        );
        let plain = format!(r#"{{{wave},"body":"Text.\n\n## Done when:\n\n- A test.\n"}}"#);
        let built = frame(&gh(&plain), 12).unwrap();
        assert_eq!(
            built.body().unwrap(),
            "Closes #12\n\nSUMMARY\n\nIssue: #12\nMilestone: Wave 3\n"
        );
        let none = r#"{"number":12,"state":"OPEN","milestone":null,"body":"Done when: x."}"#;
        let error = frame(&gh(none), 12).unwrap_err().to_string();
        assert_eq!(error, text::run_no_milestone(12));
    }

    // podman runs the sessions on Linux only.
    #[cfg(target_os = "linux")]
    #[test]
    fn with_a_token_podman_gets_the_name_of_the_variable_and_no_sign_in_file() {
        let token = "sk-ant-oat01-secret1";
        let facts = Facts {
            sign_in: ClaudeSignIn::Token,
            ..facts()
        };
        let spec = spec(&facts);
        let host = Host {
            home: facts.home.clone(),
            user: facts.user.clone(),
            claude: Claude::Token(token.into()),
        };
        assert!(spec.check_values(&host.values()).is_ok());
        assert!(host.secrets().contains(&token.to_owned()));
        let args: Vec<String> = crate::podman::run_args(&spec)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let at = args
            .iter()
            .position(|a| a == crate::claude_token::VAR)
            .unwrap();
        assert_eq!(args[at - 1], "-e");
        assert!(!args.iter().any(|a| a.contains(token)), "{args:?}");
        assert!(
            !args.iter().any(|a| a.contains(".credentials.json")),
            "{args:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn with_no_token_podman_binds_the_sign_in_file_and_gets_no_token_variable() {
        let args: Vec<String> = crate::podman::run_args(&spec(&facts()))
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert!(
            args.iter().any(|a| a.contains(".credentials.json")),
            "{args:?}"
        );
        assert!(!args.iter().any(|a| a.contains(crate::claude_token::VAR)));
    }

    #[test]
    fn a_token_starts_a_session_with_10_minutes_left_and_no_token_does_not() {
        let _token = crate::claude_token::test_token();
        let dir = tempfile::tempdir().unwrap();
        let credentials = dir.path().join(".credentials.json");
        let file =
            r#"{"claudeAiOauth":{"accessToken":"sk-a","refreshToken":"sk-r","expiresAt":600000}}"#;
        std::fs::write(&credentials, file).unwrap();
        let daemon = dir.path().join("daemon");
        assert!(matches!(
            claude_of(&daemon, &credentials, 0),
            Err(HostFault::SignInEnds(10))
        ));
        crate::claude_token::store(&daemon, "sk-ant-oat01-x").unwrap();
        let claude = claude_of(&daemon, &credentials, 0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(claude.sign_in(), ClaudeSignIn::Token);
        assert_eq!(claude.secrets(), ["sk-ant-oat01-x"]);
        // With no sign-in file at all, the token still holds.
        std::fs::remove_file(&credentials).unwrap();
        assert!(claude_of(&daemon, &credentials, 0).is_ok());
    }

    /// 01M58XZF928T82YAMF4DSJBKBF: a locked keyring is a fault of the
    /// host, also with a sign-in file that would do. The daemon does not
    /// fall back to the file.
    #[test]
    fn a_locked_keyring_stops_the_start_and_does_not_fall_back_to_the_file() {
        let _token = crate::claude_token::test_token();
        let dir = tempfile::tempdir().unwrap();
        let credentials = dir.path().join(".credentials.json");
        let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","refreshToken":"sk-r","expiresAt":99999999999999}}"#;
        std::fs::write(&credentials, file).unwrap();
        let daemon = dir.path().join("daemon");
        assert!(claude_of(&daemon, &credentials, 0).is_ok());
        crate::claude_token::test_lock();
        let fault = claude_of(&daemon, &credentials, 0).err().unwrap();
        assert!(matches!(fault, HostFault::KeyringLocked(_)), "{fault:?}");
        assert!(fault.wave_text().contains("is locked"), "{fault}");
    }

    fn facts() -> Facts {
        Facts {
            session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
            item: "issue-12".into(),
            image: "localhost/riff-session:latest".into(),
            home: "/home/mike".into(),
            user: "mike".into(),
            plan: Plan {
                path: "/r/sessions/issue-12-1a2b3c4d/clone".into(),
                origin: Origin::github("o/r").unwrap(),
                main: "/src/riff/.git".into(),
                refs: "/r/sessions/issue-12-1a2b3c4d/origin-refs".into(),
                base: "main".parse().unwrap(),
                checkout: Checkout::Branch {
                    name: name("worktree-issue-12"),
                    earlier: false,
                    no_github: false,
                },
            },
            claude: "/r/sessions/issue-12-1a2b3c4d/claude".into(),
            sign_in: ClaudeSignIn::File("/home/mike/.claude/.credentials.json".into()),
            gitconfig: Some("/home/mike/.gitconfig".into()),
            setup: Setup::default(),
        }
    }

    /// The model, the env and the volumes of the `riff.toml` of riff.
    fn riff_setup() -> Setup {
        Setup::of_config(include_str!("../../../riff.toml"), "riff.toml").unwrap()
    }

    /// A bind as `SOURCE -> TARGET`, with ` (no write)` when the session
    /// cannot write it.
    fn shown(bind: &Bind) -> String {
        let source = match &bind.source {
            Source::Path(path) => path.display().to_string(),
            Source::Volume(name) => format!("volume {name}"),
            Source::Empty => "an empty volume".to_owned(),
        };
        let mode = if bind.write { "" } else { " (no write)" };
        format!("{source} -> {}{mode}", bind.target.display())
    }

    /// The variables of a spec as `NAME=VALUE`, or `NAME` for a value of
    /// the host.
    fn variables(spec: &SessionSpec) -> Vec<String> {
        spec.env
            .iter()
            .map(|env| match env {
                Env::Value(name, value) => format!("{name}={value}"),
                Env::Host(name) => name.clone(),
            })
            .collect()
    }

    /// After a signal, riff runs no more end step, and names each step
    /// that it did not send (01M4ZYFJFTX9ZEX7QT4QH7E9T6).
    #[tokio::test]
    async fn after_a_signal_no_end_step_runs() {
        let mut steps = EndSteps {
            signals: None,
            not_sent: vec![text::RUN_VERIFY_REQUEST],
        };
        let release = async { Err::<(), _>(anyhow!("the release ran")) };
        assert!(steps.send(text::RUN_RELEASE, release).await.is_none());
        let end = async { Err::<(), _>(anyhow!("the end ran")) };
        assert!(steps.send(text::RUN_END, end).await.is_none());
        assert!(steps.signals.is_none(), "riff listens after the stop");
        assert_eq!(
            steps.not_sent,
            [text::RUN_VERIFY_REQUEST, text::RUN_RELEASE, text::RUN_END]
        );
    }

    /// The end steps use the streams of the session. They keep a signal
    /// that came before an end step, while riff waited for no signal, and
    /// that signal stops the step (#770, 01M51JG5Y47184401EWT4HVEG0).
    #[tokio::test]
    async fn a_signal_before_an_end_step_stops_it() {
        // Two signals that riff does not use, so that no other test of
        // this process sees them.
        let signals = Signals::of(SignalKind::user_defined1(), SignalKind::user_defined2());
        let mut steps = EndSteps {
            signals: Some(signals.unwrap()),
            not_sent: Vec::new(),
        };
        nix::sys::signal::raise(nix::sys::signal::Signal::SIGUSR2).unwrap();
        let verify = std::future::pending::<Result<()>>();
        let sent = steps.send(text::RUN_VERIFY_REQUEST, verify);
        let sent = tokio::time::timeout(Duration::from_secs(30), sent).await;
        assert!(matches!(sent, Ok(None)), "the signal stops the step");
        assert_eq!(steps.not_sent, [text::RUN_VERIFY_REQUEST]);
    }

    #[test]
    fn the_spec_has_each_bind_of_a_session_and_writes_only_its_own_folders() {
        let spec = spec(&facts());
        let binds: Vec<String> = spec.binds.iter().map(shown).collect();
        assert_eq!(
            binds,
            [
                "an empty volume -> /r/sessions/issue-12-1a2b3c4d/clone",
                "/src/riff/.git/objects -> /src/riff/.git/objects (no write)",
                "/r/sessions/issue-12-1a2b3c4d/origin-refs -> /r/sessions/issue-12-1a2b3c4d/origin-refs \
                 (no write)",
                "/home/mike/.gitconfig -> /home/mike/.gitconfig (no write)",
                "/r/sessions/issue-12-1a2b3c4d/claude -> /home/mike/.claude",
                "/home/mike/.claude/.credentials.json -> /home/mike/.claude/.credentials.json \
                 (no write)",
            ]
        );
        // Each path of the person on the host has no write.
        for bind in &spec.binds {
            if let Source::Path(path) = &bind.source
                && path.starts_with("/home/mike")
            {
                assert!(!bind.write, "{bind:?}");
            }
        }
    }

    #[test]
    fn the_claude_folder_of_the_spec_has_the_first_run_answers_and_no_dialog() {
        let root = tempfile::tempdir().unwrap();
        let facts = Facts {
            claude: root.path().join("claude"),
            plan: Plan {
                path: root.path().join("clone"),
                ..facts().plan
            },
            ..facts()
        };
        let auto = crate::permissions::Trust {
            owner: Some("acme"),
            server: Some("http://h:7878"),
            sources: &[],
        };
        // The config of the person: their theme, and keys that are not an
        // answer of the first run.
        let person = root.path().join("person.json");
        let config = r#"{"theme":"light-daltonized","numStartups":7}"#;
        std::fs::write(&person, config).unwrap();
        claude_folder(
            &facts.claude,
            &facts.plan.path,
            &person,
            &auto,
            crate::permissions::Role::Worker,
        )
        .unwrap();

        // claude in the container reads this folder as its config folder.
        let spec = spec(&facts);
        let bind = spec
            .binds
            .iter()
            .find(|b| b.source == Source::Path(facts.claude.clone()))
            .unwrap();
        assert_eq!(bind.target, Path::new("/home/mike/.claude"));
        assert!(variables(&spec).contains(&"CLAUDE_CONFIG_DIR=/home/mike/.claude".to_owned()));

        let read = |name: &str| -> Value {
            serde_json::from_str(&std::fs::read_to_string(facts.claude.join(name)).unwrap())
                .unwrap()
        };
        assert_eq!(
            read("settings.json"),
            settings(&auto, crate::permissions::Role::Worker)
        );
        assert_eq!(read("settings.json")["includeCoAuthoredBy"], false);
        // No dialog of auto mode, and the places of riff work.
        assert_eq!(
            read("settings.json")["skillOverrides"]["auto-mode-setup"],
            "off"
        );
        let environment = read("settings.json")["autoMode"]["environment"].clone();
        assert_eq!(environment[0], "$defaults");
        assert!(environment.to_string().contains("github.com/acme "));

        // The first-run answers: an interactive claude, for example a
        // takeover, asks no question (01M51JACH6TQP318ZFG6NQ9GRM).
        let json = read(".claude.json");
        assert_eq!(json["hasCompletedOnboarding"], true);
        assert_eq!(json["theme"], "light-daltonized");
        let clone = facts.plan.path.display().to_string();
        assert_eq!(json["projects"][&clone]["hasTrustDialogAccepted"], true);
        assert_eq!(json.get("numStartups"), None);
        // The config of the person does not change.
        assert_eq!(std::fs::read_to_string(&person).unwrap(), config);
    }

    #[test]
    fn the_spec_has_the_home_the_limits_and_the_values_of_a_session() {
        let spec = spec(&facts());
        assert_eq!(spec.name, "riff-issue-12-1a2b3c4d");
        assert_eq!(spec.image, "localhost/riff-session:latest");
        assert_eq!(spec.home, Path::new("/home/mike"));
        assert_eq!(
            spec.workdir,
            Path::new("/r/sessions/issue-12-1a2b3c4d/clone")
        );
        assert_eq!((spec.memory.as_str(), spec.cpus), ("8g", 8));
        assert_eq!(
            variables(&spec),
            [
                "HOME=/home/mike",
                "USER=mike",
                "LOGNAME=mike",
                "GIT_CONFIG_COUNT=3",
                "GIT_CONFIG_KEY_0=commit.gpgsign",
                "GIT_CONFIG_VALUE_0=false",
                "GIT_CONFIG_KEY_1=credential.https://github.com.helper",
                "GIT_CONFIG_VALUE_1=",
                "GIT_CONFIG_KEY_2=credential.https://github.com.helper",
                "GIT_CONFIG_VALUE_2=!gh auth git-credential",
                "CLAUDE_CONFIG_DIR=/home/mike/.claude",
                "RIFF_CLONE={\"path\":\"/r/sessions/issue-12-1a2b3c4d/clone\",\
                 \"origin\":\"https://github.com/o/r.git\",\"main\":\"/src/riff/.git\",\
                 \"refs\":\"/r/sessions/issue-12-1a2b3c4d/origin-refs\",\"base\":\"main\",\
                 \"checkout\":{\"branch\":{\"name\":\"worktree-issue-12\",\
                 \"earlier\":false,\"no_github\":false}}}",
            ]
        );
        assert_eq!(
            spec.command.join(" "),
            "claude --model opus -p --input-format stream-json --output-format stream-json \
             --verbose --session-id 1a2b3c4d-0000-4000-8000-000000000000 --permission-mode auto"
        );
        // No token of the person, and no value of the host with no token
        // of claude (01M6Z87A04EXTT1FFJSJ1EM6TB).
        assert!(crate::daemon::gh_token::check_env(&spec).is_ok());
        assert!(spec.check_values(&[]).is_ok());
    }

    /// 01M5SMW28KT5D5HPRH7KAZ8TVC: a `riff.toml` with no `env` and no
    /// `volumes` gives a session no variable and no volume of a language
    /// or a tool, and no `CARGO_BUILD_JOBS`.
    #[test]
    fn a_spec_of_a_riff_toml_with_no_env_has_no_variable_and_no_volume_of_a_tool() {
        let setup = Setup::of_config("[session]\nimage = \"x\"\n", "riff.toml").unwrap();
        let spec = spec(&Facts { setup, ..facts() });
        let names: Vec<String> = variables(&spec)
            .iter()
            .map(|v| v.split('=').next().unwrap().to_owned())
            .collect();
        for tool in [
            "RUSTC_WRAPPER",
            "CARGO_BUILD_JOBS",
            "CARGO_HOME",
            "SCCACHE_DIR",
        ] {
            assert!(!names.iter().any(|n| n == tool), "{tool} in {names:?}");
        }
        assert!(!names.iter().any(|n| n.contains("SCCACHE")), "{names:?}");
        assert!(
            spec.binds
                .iter()
                .all(|b| !matches!(b.source, Source::Volume(_))),
            "{:?}",
            spec.binds
        );
        // The caps are only `--memory` and `--cpus`.
        assert_eq!((spec.memory.as_str(), spec.cpus), ("8g", 8));
    }

    /// 01M5SMVVC4GQS310CVK3H9CMQ7: the env and the volumes of the
    /// `riff.toml` are in the spec, after what riff sets, with `$HOME` in
    /// place.
    #[test]
    fn a_spec_of_a_riff_toml_with_env_and_volumes_has_them_with_the_home_in_place() {
        let setup = Setup::of_config(
            "[session.env]\nPIP_CACHE_DIR = \"$HOME/.pip\"\nPLAIN = \"x\"\n\n\
             [session.volumes]\nriff-pip = \"$HOME/.pip\"\nshared = \"/opt/shared\"\n",
            "riff.toml",
        )
        .unwrap();
        let spec = spec(&Facts { setup, ..facts() });
        let binds: Vec<String> = spec.binds.iter().map(shown).collect();
        assert_eq!(
            binds[binds.len() - 2..],
            [
                "volume riff-pip -> /home/mike/.pip",
                "volume shared -> /opt/shared"
            ]
        );
        assert!(spec.binds.iter().rev().take(2).all(|b| b.write));
        let vars = variables(&spec);
        assert_eq!(
            vars[vars.len() - 2..],
            ["PIP_CACHE_DIR=/home/mike/.pip", "PLAIN=x"]
        );
        // What riff sets stays before them.
        assert_eq!(vars[0], "HOME=/home/mike");
    }

    /// 01M5SMW2DQ8G919EJ4P0GXF9Y3: the `riff.toml` of riff keeps its
    /// build fast: the cargo home, sccache and their volumes.
    #[test]
    fn the_riff_toml_of_riff_sets_the_build_cache_of_its_sessions() {
        let spec = spec(&Facts {
            setup: riff_setup(),
            ..facts()
        });
        let binds: Vec<String> = spec.binds.iter().map(shown).collect();
        assert_eq!(
            binds[binds.len() - 2..],
            [
                "volume riff-cargo -> /home/mike/.cargo",
                "volume riff-sccache -> /home/mike/.cache/sccache"
            ]
        );
        let vars = variables(&spec);
        assert_eq!(
            vars[vars.len() - 6..],
            [
                "CARGO_HOME=/home/mike/.cargo",
                "SCCACHE_DIR=/home/mike/.cache/sccache",
                "SCCACHE_CACHE_SIZE=40G",
                "RUSTC_WRAPPER=sccache",
                "SCCACHE_IDLE_TIMEOUT=0",
                "SCCACHE_IGNORE_SERVER_IO_ERROR=1",
            ]
        );
    }

    /// 01M6VAQWMKDX4VY8QRPA8EYFN9, 01M6W7VC9REZKE87T6YT5Z47A2: the
    /// `riff.toml` of riff runs each session on Opus, with `model` in
    /// `[session]` only, and with no `ANTHROPIC_MODEL`.
    #[test]
    fn the_riff_toml_of_riff_runs_each_session_on_opus() {
        let text = include_str!("../../../riff.toml");
        let config: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(config["session"]["model"].as_str(), Some("opus"));
        assert!(!text.contains("ANTHROPIC_MODEL ="), "{text}");
        let spec = spec(&Facts {
            setup: riff_setup(),
            ..facts()
        });
        assert_eq!(spec.command[..3], ["claude", "--model", "opus"]);
        let vars = variables(&spec);
        assert!(
            !vars.iter().any(|var| var.starts_with("ANTHROPIC_MODEL=")),
            "{vars:?}"
        );
    }

    /// 01M6VAQWECJ6N6VJF66KCMNYD1: the command of a worker starts with
    /// `claude --model MODEL`, from the `riff.toml`, and `opus` with no
    /// `model`.
    #[test]
    fn the_command_of_a_worker_has_the_model_of_the_riff_toml() {
        let setup = Setup::of_config("[session]\nmodel = \"sonnet\"\n", "riff.toml").unwrap();
        let with = spec(&Facts { setup, ..facts() });
        assert_eq!(with.command[..4], ["claude", "--model", "sonnet", "-p"]);
        let none = spec(&facts());
        assert_eq!(none.command[..4], ["claude", "--model", "opus", "-p"]);
        assert_eq!(
            none.command.iter().filter(|a| *a == "--model").count(),
            1,
            "{:?}",
            none.command
        );
    }

    #[test]
    fn the_spec_binds_no_git_config_that_the_host_does_not_have_and_never_the_gh_config() {
        let spec = spec(&Facts {
            gitconfig: None,
            ..facts()
        });
        let all: Vec<String> = spec.binds.iter().map(shown).collect();
        let all = all.join("\n");
        assert!(!all.contains(".gitconfig"), "{all}");
        assert!(!all.contains(".config/gh"), "{all}");
    }

    #[test]
    fn a_denied_call_and_a_push_show_their_text_fields() {
        let denied = json!({"type":"system","subtype":"permission_denied","session_id":"s",
            "tool_name":"Bash","reason":"the rules refuse it","tool_use_id":"t1"});
        assert_eq!(
            event_lines(&denied),
            ["permission_denied: reason the rules refuse it, tool_name Bash, tool_use_id t1"]
        );
        let push = json!({"type":"system","subtype":"vcs_state_changed","uuid":"u",
            "action":"push","branch":"worktree-issue-12","count":1});
        assert_eq!(
            event_lines(&push),
            ["vcs_state_changed: action push, branch worktree-issue-12"]
        );
    }

    #[test]
    fn a_long_text_shows_its_first_line_cut_at_the_limit() {
        let long = "a".repeat(SHOWN + 5);
        let event = json!({"type":"assistant","message":{"content":[
            {"type":"text","text": format!("{long}\nsecond")}]}});
        assert_eq!(event_lines(&event), [format!("{}…", "a".repeat(SHOWN))]);
    }

    #[test]
    fn a_tool_result_with_text_parts_shows_them() {
        let event = json!({"type":"user","message":{"content":[
            {"type":"tool_result","is_error":true,
             "content":[{"type":"text","text":"no"},{"type":"text","text":"file"}]},
            {"type":"tool_result","is_error":false,"content":"fine"}]}});
        assert_eq!(event_lines(&event), ["! the tool failed: no file"]);
    }

    #[test]
    fn the_scrub_takes_out_each_secret_also_twice_in_one_line() {
        let scrub = Scrub::new(["gho_one".to_owned(), "sk-two".to_owned(), "ab".to_owned()]);
        assert_eq!(
            scrub.clean("gho_one sk-two gho_one ab"),
            "[secret] [secret] [secret] ab"
        );
    }

    /// #2156, 01M7G01XCJGVYZBHW20GVGWG6K: a secret that starts with a
    /// character of two bytes goes each time: twice in one line, and in
    /// two matches that overlap.
    #[test]
    fn a_secret_that_starts_with_a_wide_character_goes_each_time() {
        let scrub = Scrub::new(["é-secret-1".to_owned()]);
        assert_eq!(scrub.found("a é-secret-1 b é-secret-1"), [2..13, 16..27]);
        assert_eq!(
            scrub.clean("a é-secret-1 b é-secret-1"),
            "a [secret] b [secret]"
        );
        let scrub = Scrub::new(["éaéa".to_owned()]);
        assert_eq!(scrub.found("éaéaéa"), [0..6, 3..9]);
        assert_eq!(scrub.clean("x éaéaéa y"), "x [secret] y");
    }

    /// #2156, 01M7G3CSVWSV6RTGJY1WMFGV7V: in a long run of token starts,
    /// `part` looks at each character at most once, so a long line of a
    /// session holds the daemon for no long time. A start that only
    /// touches the end of the last token gives its own token.
    #[test]
    fn tokens_looks_at_each_character_at_most_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static LOOKS: AtomicUsize = AtomicUsize::new(0);
        fn part(c: char) -> bool {
            LOOKS.fetch_add(1, Ordering::Relaxed);
            c.is_ascii_alphanumeric() || c == '_'
        }
        let text = "ghs_".repeat(2_000);
        let found: Vec<_> = tokens(&text, "ghs_", part).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], 0..text.len());
        assert!(LOOKS.load(Ordering::Relaxed) <= text.len());
        assert_eq!(Scrub::default().clean(&text), HIDDEN);
        let alnum = |c: char| c.is_ascii_alphanumeric();
        let found: Vec<_> = tokens("sk-absk-cd", "sk-", alnum).collect();
        assert_eq!(found, [0..7, 5..10]);
    }

    /// #2156: an empty start gives each run of characters that `part`
    /// takes, also the run at the start of the text, and no empty range.
    #[test]
    fn tokens_with_an_empty_start_gives_each_run_of_parts() {
        let alnum = |c: char| c.is_ascii_alphanumeric();
        let found: Vec<_> = tokens("ab c", "", alnum).collect();
        assert_eq!(found, [0..2, 3..4]);
        let found: Vec<_> = tokens(" é1 b", "", alnum).collect();
        assert_eq!(found, [3..4, 5..6]);
        assert_eq!(tokens("", "", alnum).count(), 0);
    }

    #[test]
    fn a_sign_in_that_ended_has_minutes_below_zero() {
        let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","expiresAt":0}}"#;
        let sign_in = sign_in(file, 120 * 60_000).unwrap();
        assert_eq!(sign_in.minutes, -120);
        assert_eq!(sign_in.secrets, ["sk-a"]);
    }

    /// The command of each hook of [`session_hooks`].
    fn commands(hooks: &Value) -> Vec<String> {
        let groups = hooks.as_object().unwrap().values();
        groups
            .flat_map(|list| list.as_array().unwrap().clone())
            .flat_map(|group| group["hooks"].as_array().unwrap().clone())
            .map(|hook| hook["command"].as_str().unwrap().to_owned())
            .collect()
    }

    /// 01M6WFN93ZDCSAVRTNYDX4MRJR: each hook command ends with `|| true`,
    /// so an older riff that does not know the subcommand blocks no turn
    /// and no prompt.
    #[test]
    fn each_hook_of_riff_exits_with_0_also_on_an_older_riff() {
        let commands = commands(&session_hooks());
        assert_eq!(commands.len(), 3);
        for command in commands {
            assert!(command.starts_with("/opt/riff/riff relay "), "{command}");
            assert!(command.ends_with(" || true"), "{command}");
        }
    }

    /// 01M6WFN93ZDCSAVRTNYDX4MRJR: the Claude folder of an earlier start
    /// gets the new command in the place of each hook of the older riff.
    /// A hook of the person stays.
    #[test]
    fn the_hooks_of_an_older_claude_folder_get_the_new_command() {
        let group = |command: &str| json!({"hooks": [{"type": "command", "command": command}]});
        let own = group("notify-send done");
        let old = json!({
            "model": "opus",
            "hooks": {
                "PermissionDenied": [{
                    "matcher": "*",
                    "hooks": [{"type": "command", "command": "/opt/riff/riff relay denied"}],
                }],
                "UserPromptSubmit": [group("/opt/riff/riff relay prompt")],
                "Stop": [group("/opt/riff/riff relay idle"), own.clone()],
            },
        });
        let new = with_session_hooks(&old.to_string()).unwrap().unwrap();
        assert_eq!(with_session_hooks(&new).unwrap(), None, "the hooks are new");

        let settings: Value = serde_json::from_str(&new).unwrap();
        let hooks = session_hooks();
        assert_eq!(settings["model"], "opus");
        assert_eq!(
            settings["hooks"]["PermissionDenied"],
            hooks["PermissionDenied"]
        );
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"],
            hooks["UserPromptSubmit"]
        );
        assert_eq!(settings["hooks"]["Stop"], json!([own, hooks["Stop"][0]]));
        assert!(
            commands(&settings["hooks"])
                .iter()
                .filter(|c| c.starts_with("/opt/riff/riff "))
                .all(|c| c.ends_with(" || true")),
            "{settings}"
        );
    }
}
