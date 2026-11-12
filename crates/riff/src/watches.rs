//! The watches of the async operations (01M5RGCW5NCKQYA61RM63914TX,
//! 01M5RGD0RTXYA7FF5K1TH72MGX).
//!
//! # Design
//!
//! No session has to remember to wait. A riff command that starts an
//! async operation registers a watch in the same call. The riff daemon
//! of the host looks at each watch each [`every`]. When
//! the operation ends, with a success or a failure, the daemon posts one
//! message to the lead of the repository, with the result, the failed
//! jobs and the next step. Then the watch goes.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff release test, cut,<br/>approve, retry, rollback"] -- "register" --> F["watches.json<br/>in the folder of the daemon"]
//!     W["riff host join"] -- "one watch for the wave" --> F
//!     D["riff daemon:<br/>a look each minute"] -- "read" --> F
//!     D -- "gh, git" --> G["the forge"]
//!     D -- "one message at the end" --> L["the lead"]
//!     D -- "a line while a deploy waits" --> O["each admin in riff chat"]
//! ```
//!
//! | Command | [`Op`] | Ends when |
//! |---|---|---|
//! | `riff release cut` | [`Op::Pull`] | the pull request of the next version merges or closes; a failed Gate gets a message, and the watch stays (01M76SBPGH2218K3ED569T051D) |
//! | `riff release test --host HOST` | [`Op::Test`] | the real-session test passes or fails |
//! | `riff release cut`, `approve`, `retry`, `rollback` | [`Op::Deploy`] | the job `Deploy` of the tag fails, or it passes and the job `Check the deploy` sets its status or ends (see "A deploy" below) |
//! | `riff host join` | [`Op::Wave`] | never: it posts each time the last open item of the current wave closes |
//! | the end of the watch of a deploy | [`Op::Check`] | the check of the lead ends the release, or another deploy follows (see "The check" below) |
//!
//! The same watch is one watch: a second command that registers the watch
//! of the same operation registers nothing new ([`Registered::Kept`]).
//!
//! A wave can run on more hosts, and the daemon of each host has the
//! watch of the wave. Its message "Wave N is complete" has a key
//! ([`wave_complete_key`], 01M6FZY6NABXGXVRYF6W1509XE): `riff-server` drops
//! a post with the key of a message that it keeps from the same user in
//! the thread, so the lead gets it one time. The other watches run on the
//! host of their command: their messages have no key.
//!
//! While the current wave of a repository with the table `[release]` has
//! no open item and no release issue, the watch of the wave keeps it, with
//! why its cut waits ([`Seen::complete`], 01M75N6YNHQQHGS0VGKTKSNJTR): the
//! full check of `main` runs, it failed, or it passed and the lead cuts
//! next. The daemon gives it to the facts of its host, so `riff who` and
//! the TUI say that the release waits for its cut ([`crate::view::release`]).
//! The release issue that `riff release test` or the cut makes in the wave
//! ends it.
//!
//! From the release issue to the check, the watch of the wave reads the
//! release of the newest release issue of the wave, as `riff release status`
//! does ([`Seen::release`], [`cycle::stage`], 01M75R6FPVZA3JA5C48PQ26A85,
//! 01M7642H27T4862WZENFAN4277). A read costs about ten calls of `gh`, so it
//! comes each [`read_every`] at most, and at once for a new release issue.
//! Before the tag, the read needs the last commit of `origin/main`, its
//! release build and its tests: the look fetches `origin` first. The daemon
//! gives the release at its step to the facts of its host. Of one tag, the latest step that a host saw
//! wins ([`riff_client::daemons::release`]), so the watch of a deploy and the
//! watch of a check, which look each minute, give the newer step.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Merged: riff release test makes the release issue
//!     Merged --> TestRuns: riff release test
//!     TestRuns --> TestFailed
//!     TestFailed --> TestRuns: the fix merges, the test runs again
//!     TestRuns --> Tested
//!     Tested --> Published: riff release cut
//!     Published --> Waits: the job Deploy waits for production
//!     Waits --> [*]: the steps of the deploy and of the check
//! ```
//!
//! # The file
//!
//! The daemon keeps the watches in [`FILE`] of its folder
//! ([`crate::daemon::dir`]), so a restart keeps them. A command on the
//! host writes the file itself, with a lock ([`Watches::register`]). A
//! lead in a container asks the daemon on its socket
//! ([`crate::daemon::ServerCall::Watch`]), because the container has no
//! access to the folder. The file holds the repository, the [`Op`], the
//! start, and what the watch saw ([`Seen`]). The daemon finds the clone
//! and the riff-server of a repository in `waves.json`, or in
//! `served.json` ([`wave_of`]). The watches of a check ([`Op::Check`])
//! are in [`CHECKS`] of the same folder, under the same lock: a daemon of
//! an older riff reads [`FILE`] only.
//!
//! # A look
//!
//! One look ([`Watches::look_all`]) reads the file and groups the watches
//! by repository. For each repository, one forge ([`Lookout::forge`])
//! serves all of its watches: `gh` with the sign-in of the person on the
//! host, and `git` in the main clone. [`look`] asks the forge for the
//! state of one watch:
//!
//! ```mermaid
//! flowchart TD
//!     R["read the forge"] --> E{"fault?"}
//!     E -- "yes, a new one" --> F["one message to the lead for the repository:<br/>the fault, with each watch that failed"]
//!     E -- "yes, the same" --> K["nothing: try again at the next look"]
//!     E -- no --> S{"the operation ended?"}
//!     S -- no --> K
//!     S -- yes --> P["message to the lead:<br/>the result, the failed jobs, next: ..."]
//!     P --> G["the watch goes"]
//! ```
//!
//! A message that does not go out stays: the next look posts it. The
//! watches of a repository that fail with the same fault give one
//! message, and the message has a key ([`watch_fault_key`]), so the
//! fault comes one time for each repository, until the fault changes.
//!
//! While a limit of GitHub is out ([`crate::ghcalls`], #1557), a look makes
//! no call and tells no fault: the wave tells the lead of the limit one
//! time, with the reset, and the watches look again after it.
//!
//! # A deploy
//!
//! The watch of a deploy follows the CI run of the tag ([`Op::Deploy`]).
//! The job `Deploy` waits for `production` only after the jobs before it
//! passed, so a run that still builds posts nothing
//! (01M5RGD0XTR4X1MEV726HMCEGQ):
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Builds: riff release cut
//!     Builds --> Waits: the job Deploy waits for production
//!     Builds --> Failed: a job failed
//!     Waits --> Waits: 4 hours: a reminder to each admin
//!     Waits --> Runs: a person runs riff release approve
//!     Runs --> Passed: the job Deploy passed
//!     Runs --> Failed: the job Deploy failed
//!     Waits --> Failed: a job failed
//!     Passed --> Passed: a daemon or the lead runs an old riff, up to 30 minutes
//!     Passed --> Passed: the job Check the deploy runs, up to 30 minutes
//!     Passed --> Check: the new riff, and the status riff/deploy-check or the end of the job: message to the lead: next: riff release check
//!     Failed --> Check: message to the lead: the jobs, next: riff release retry
//!     Check --> [*]: the watch of the check (see "The check" below)
//! ```
//!
//! At "Passed" the message waits for the new riff
//! (01M6WBX2G1PX11D89HNBAZ6MCM): the lead runs `riff release check` in
//! its container, and an old riff there cannot read each answer of a new
//! daemon. The daemon gives each look its own facts ([`Riffs`]): its
//! version, whether `riff update` replaced its binary, `update.auto`, and
//! whether each of its leads runs an old riff. The message goes when the
//! daemon runs the release, or a newer riff, and the lead of the
//! repository on this host runs the riff of the daemon
//! ([`Riffs::waits_for`]). A host with `update.auto` off does not wait for
//! its daemon, and a host with no lead of the repository does not wait for
//! a lead.
//!
//! The message also waits for the daemon of each other host of the riff
//! with `update.auto` on (01M6WFNDS76NESPXHMQPGHHQSS): `riff release
//! check` reads the build of each host. Each daemon sends its build and
//! its `update.auto` in its facts ([`riff_client::daemons::Facts`]), and
//! the look reads them from `who` ([`Lookout::hosts`],
//! [`Riffs::hosts_of`]).
//!
//! The message also waits for the CI job `Check the deploy`
//! (01M77BT0489T9R084WEFHPMCR5): `riff release check` takes the four
//! checks of the Cloud Run log from the status `riff/deploy-check` that
//! the job sets, about 6 minutes after the end of the job `Deploy`
//! ([`cycle::check_waits`]). The message goes when the tag has the status
//! of this deploy, or when the job ended. Only then the look reads the
//! status, with one call of `gh`. A status `failure` puts the failed
//! checks in the message, and a status `error`, or a job that ended with
//! no status, the reason ([`cycle::check_none`],
//! [`crate::text::deploy_check_found`]). A run by hand, as of a rollback,
//! has no such job. After [`DEPLOY_SETTLE`] the message goes all the same,
//! and says what it waited for, for example the host that still runs an
//! old riff, or the job that did not end.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as the watch of the deploy
//!     participant D as riff daemon (the host)
//!     participant O as riff daemon (each other host)
//!     participant C as the job Check the deploy
//!     participant L as the lead (its container)
//!     Note over W: the job Deploy passed: the message waits
//!     D->>D: riff update, then a new start on the release
//!     O->>O: riff update, then a new start on the release
//!     O->>W: its facts in who: the build of the release
//!     L->>D: the end of a turn (the Stop hook)
//!     D->>L: a new container on the release
//!     C->>W: the status riff/deploy-check of the tag, or the end of the job
//!     W->>L: the deploy passed, your container runs the release. next: riff release check
//! ```
//!
//! At "Waits" the lead gets one message, and each admin gets the line of
//! riff-bot "waits for your approval" in `riff chat`: the watch tells
//! riff-server the run ([`BotFact::ReleaseWaits`]), and riff-server writes
//! the line with each hold of the version (01M5SNV7BC5J58WRZRJ5HS4NSJ).
//! After `riff release retry` or `rollback`, the CI run still has its old
//! state for a moment. So the command gives the state that it saw
//! ([`Stale`]), and the watch ignores that state until it moves.
//!
//! At "Waits", "Passed" and "Failed" the watch also posts one note to
//! each other lead of the riff ([`Post::Leads`], [`crate::hold`],
//! 01M5SNV75DRFHR26BT2M8243VV): the lead of the release has the message
//! above, and the other leads have the note, with what comes next for
//! them. The note comes after the message and the line to the admins, so
//! a post that fails leaves the message to the lead as the first fact.
//!
//! # The check
//!
//! With its message of the end of the deploy, the watch of the deploy goes,
//! and the watch of the check of the lead takes its place in the same write
//! ([`Outcome::then`], [`Op::Check`], 01M6XTS7RGXB30MFM6DZ7PVZ75). The watch
//! of the check posts nothing and fetches nothing: each look reads the
//! releases and the release issue with `gh`. It gives the step of the release
//! to the facts of the host, so `riff who` and the TUI say who acts next
//! ([`Watch::release`], [`crate::view::release`]). Only what comes after its
//! start counts: the watch keeps the count of the comments of the release
//! issue and of the newer releases at its start. So the check of a failed
//! deploy before a retry, and the newer release and the closed issue of an
//! older release after a rollback, end no watch.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Waits: the job Deploy passed or failed
//!     Waits --> Failed: riff release check v3.3.0 failed
//!     Failed --> Failed: an admin resumes, or the lead fixes
//!     Waits --> [*]: a passed check, the check of a failed deploy, a closed issue
//!     Failed --> [*]: a passed check, a rollback, a newer release
//!     Waits --> [*]: a new watch of a deploy of the repository
//!     Failed --> [*]: a new watch of a deploy of the repository
//! ```
//!
//! | The watch | Its step in the facts |
//! |---|---|
//! | [`Op::Deploy`], the job waits for `production` | [`ReleaseStep::Waits`] |
//! | [`Op::Deploy`], the job runs | [`ReleaseStep::Runs`] |
//! | [`Op::Deploy`], the job passed, the message waits for the new riff or for the job `Check the deploy` | [`ReleaseStep::Passed`] |
//! | [`Op::Check`] after a pass | [`ReleaseStep::Passed`] |
//! | [`Op::Check`] after a failure | [`ReleaseStep::DeployFailed`] |
//! | [`Op::Check`], the last check failed | [`ReleaseStep::CheckFailed`] |
//! | [`Op::Wave`], the state `built` | [`ReleaseStep::Merged`], [`ReleaseStep::TestRuns`] or [`ReleaseStep::TestFailed`] |
//! | [`Op::Wave`], the states `tested` and `published` | [`ReleaseStep::Tested`], [`ReleaseStep::Published`] |
//! | [`Op::Wave`], the states `waiting`, `deploying`, `deployed` and `failed` | the step of the same state of the watch of the deploy or of the check |
//! | [`Op::Wave`], the states `building`, `checked` and `rolled back` | none |
//!
//! A new watch of a deploy ends each watch of a check of its repository
//! ([`Watches::register`]): a retry, a rollback and the next release
//! follow their own deploy.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use nix::fcntl::{Flock, FlockArg};
use riff_client::daemons::{self, Host, ReleaseStep};
use riff_core::build::Semver;
use riff_core::wire::{BotFact, Kind};
use serde::{Deserialize, Serialize};

use crate::cycle::{self, DeployState, Forge, State, TestState};
use crate::daemon::remote;
use crate::hold::{self, Step};
use crate::pr::Gate;
use crate::wave::{BoxFuture, Wave};

/// The file of the watches, in the folder of the daemon.
pub const FILE: &str = "watches.json";

/// The file of the watches of a check ([`Op::Check`]), in the folder of the
/// daemon (01M6XTS7RGXB30MFM6DZ7PVZ75). A daemon of an older riff reads
/// only [`FILE`], and refuses a file with a kind of watch that it does not
/// know: after a rollback of riff, it still reads its watches.
pub const CHECKS: &str = "checks.json";

/// The file that locks [`FILE`] and [`CHECKS`] between the daemon and a
/// command.
const LOCK: &str = "watches.lock";

/// The watches in the file `path`. With no file, none.
fn read_file(path: &Path) -> Result<Vec<Watch>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the watches in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes `watches` to the file `path` in one rename.
fn write_file(path: &Path, watches: &[&Watch]) -> Result<()> {
    let tmp = path.with_extension("json.new");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
    file.write_all(&serde_json::to_vec_pretty(watches)?)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}

/// The variable that gives the time between two looks at the watches in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::watches::EVERY_VAR, "RIFF_WATCHES_EVERY_MS");
/// ```
pub const EVERY_VAR: &str = "RIFF_WATCHES_EVERY_MS";

/// How many times longer an idle loop of the daemon waits between two
/// looks (#1549): the look of a wave that has no work, no session and no
/// change, and the look at the watch of a wave when it is the only watch.
/// With a look each minute, an idle loop looks each 5 minutes.
pub const IDLE_FACTOR: u32 = 5;

/// The time between two looks at the watches: [`crate::look::LOOK_EVERY`],
/// or the milliseconds of [`EVERY_VAR`].
pub fn every() -> Duration {
    std::env::var(EVERY_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(crate::look::LOOK_EVERY, Duration::from_millis)
}

/// The time between two lines to the admins while a deploy waits for the
/// approval (01M58KMM3NFGJRW4CCV50KJQB2, 01M5RGD0XTR4X1MEV726HMCEGQ).
pub const REMIND_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

/// The longest wait of the message of a deploy that passed for the new riff
/// of the daemon and of the lead (01M6WBX2G1PX11D89HNBAZ6MCM). After it,
/// the watch posts all the same, and says what it waited for.
pub const DEPLOY_SETTLE: Duration = Duration::from_secs(30 * 60);

/// What the daemon of the watches and its leads run, for the time of the
/// message of a deploy that passed (01M6WBX2G1PX11D89HNBAZ6MCM). The
/// default waits for nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Riffs {
    /// The version of the riff of the daemon.
    pub daemon: Option<riff_core::build::Semver>,
    /// True when `riff update` replaced the binary of the daemon: it starts
    /// again on the new riff after its last session.
    pub replaced: bool,
    /// True when the host updates riff by itself (`update.auto`), so the
    /// daemon gets the release.
    pub auto: bool,
    /// The repository of each lead on the host, and true when the lead runs
    /// an old riff ([`crate::lead::old`]): the daemon starts it again at the
    /// end of its turn.
    pub leads: BTreeMap<String, bool>,
    /// The version of the riff daemon of each other host of the riff with
    /// `update.auto` on, by `USER@HOST` ([`Riffs::hosts_of`],
    /// 01M6WFNDS76NESPXHMQPGHHQSS).
    pub hosts: BTreeMap<String, Semver>,
}

impl Riffs {
    /// The version of the riff daemon of each host of `hosts`, the facts
    /// of `who`, by `USER@HOST` (01M6WFNDS76NESPXHMQPGHHQSS). It skips the
    /// host `host` of the user `user`: the daemon of the watch knows its
    /// own riff better. It skips a host with `update.auto` off, and a build
    /// that riff cannot read. A daemon of an older riff tells no
    /// `update.auto`: its host counts as on.
    ///
    /// ```
    /// use riff::watches::Riffs;
    /// use riff_client::daemons::{Facts, Host};
    ///
    /// let host = |user: &str, host: &str, build: &str, auto| Host {
    ///     user: user.into(),
    ///     host: host.into(),
    ///     age_secs: 10,
    ///     facts: Facts { build: build.into(), auto, ..Facts::default() },
    /// };
    /// let hosts = [
    ///     host("mike", "thelio", "4.12.0 7213825ab1c2 2026-11-08T10:00:00Z", Some(true)),
    ///     host("mike", "pangolin", "4.11.0 929605821e54 2026-11-01T10:00:00Z", Some(true)),
    ///     host("bob", "thelio", "4.11.0", None),
    ///     host("bob", "laptop", "4.11.0", Some(false)),
    ///     host("ann", "box", "next", Some(true)),
    /// ];
    /// let found = Riffs::hosts_of(&hosts, "mike", "thelio");
    /// let found: Vec<(&str, String)> = found.iter().map(|(h, v)| (h.as_str(), v.to_string())).collect();
    /// assert_eq!(found, [("bob@thelio", "4.11.0".into()), ("mike@pangolin", "4.11.0".into())]);
    /// ```
    pub fn hosts_of(hosts: &[Host], user: &str, host: &str) -> BTreeMap<String, Semver> {
        hosts
            .iter()
            .filter(|h| !(h.user == user && h.host == host) && h.facts.auto != Some(false))
            .filter_map(|h| {
                let version = h.facts.build.split_whitespace().next()?.parse().ok()?;
                Some((format!("{}@{}", h.user, h.host), version))
            })
            .collect()
    }

    /// What the message of the passed deploy of `tag` in `repo` waits for,
    /// or `None` when it goes now. A host with `update.auto` off does not
    /// wait for its daemon, and a host with no lead of `repo` does not
    /// wait for a lead. Each other host of [`Riffs::hosts`] waits for its
    /// daemon (01M6WFNDS76NESPXHMQPGHHQSS).
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff::watches::Riffs;
    ///
    /// let riffs = Riffs {
    ///     daemon: Some("4.11.0".parse().unwrap()),
    ///     replaced: false,
    ///     auto: true,
    ///     leads: BTreeMap::from([("o/r".to_owned(), false)]),
    ///     hosts: BTreeMap::new(),
    /// };
    /// assert_eq!(riffs.waits_for("o/r", "v4.11.0"), None);
    /// // A rollback to an older release waits for no daemon.
    /// assert_eq!(riffs.waits_for("o/r", "v4.10.0"), None);
    /// let why = riffs.waits_for("o/r", "v4.12.0").unwrap();
    /// assert!(why.contains("still runs v4.11.0, not v4.12.0"), "{why}");
    /// let off = Riffs { auto: false, ..riffs.clone() };
    /// assert_eq!(off.waits_for("o/r", "v4.12.0"), None);
    ///
    /// let old = Riffs { leads: BTreeMap::from([("o/r".to_owned(), true)]), ..riffs.clone() };
    /// let why = old.waits_for("o/r", "v4.11.0").unwrap();
    /// assert!(why.contains("your container still runs an old riff"), "{why}");
    /// assert_eq!(old.waits_for("o/other", "v4.11.0"), None);
    ///
    /// let replaced = Riffs { replaced: true, ..riffs.clone() };
    /// let why = replaced.waits_for("o/r", "v4.11.0").unwrap();
    /// assert!(why.contains("starts again on the new riff"), "{why}");
    ///
    /// // Another host with `update.auto` on still runs an older riff.
    /// let version = |v: &str| v.parse().unwrap();
    /// let hosts = BTreeMap::from([("bob@pangolin".to_owned(), version("4.10.0"))]);
    /// let other = Riffs { hosts, ..riffs.clone() };
    /// let why = other.waits_for("o/r", "v4.11.0").unwrap();
    /// assert_eq!(why, "the riff daemon of bob@pangolin still runs v4.10.0, not v4.11.0");
    /// assert_eq!(other.waits_for("o/r", "v4.10.0"), None);
    /// let mut two = other.clone();
    /// two.hosts.insert("ann@box".into(), version("4.9.0"));
    /// assert_eq!(
    ///     two.waits_for("o/r", "v4.11.0").unwrap(),
    ///     "the riff daemons of ann@box (v4.9.0), bob@pangolin (v4.10.0) still run an older \
    ///      riff than v4.11.0"
    /// );
    /// ```
    pub fn waits_for(&self, repo: &str, tag: &str) -> Option<String> {
        if self.replaced {
            return Some(
                "the riff daemon of this host still runs an old riff: it starts again on the new \
                 riff after its last session"
                    .to_owned(),
            );
        }
        let release = cycle::tag_version(tag).ok();
        if let (true, Some(daemon), Some(release)) = (self.auto, self.daemon, release)
            && daemon < release
        {
            return Some(format!(
                "the riff daemon of this host still runs v{daemon}, not {tag}"
            ));
        }
        let old: Vec<(&String, &Semver)> = self
            .hosts
            .iter()
            .filter(|(_, version)| release.is_some_and(|release| **version < release))
            .collect();
        match old[..] {
            [] => {}
            [(host, version)] => {
                return Some(format!(
                    "the riff daemon of {host} still runs v{version}, not {tag}"
                ));
            }
            _ => {
                let hosts: Vec<String> = old.iter().map(|(h, v)| format!("{h} (v{v})")).collect();
                return Some(format!(
                    "the riff daemons of {} still run an older riff than {tag}",
                    hosts.join(", ")
                ));
            }
        }
        if self.leads.get(repo) == Some(&true) {
            return Some(
                "your container still runs an old riff: riff starts it again on the riff of the \
                 daemon at the end of your turn"
                    .to_owned(),
            );
        }
        None
    }
}

/// The state of a CI run that a command saw before it started an
/// operation: the watch ignores it until the state moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stale {
    /// The ID of the CI run.
    pub run: u64,
    /// Where its job `Deploy` was.
    pub state: DeployState,
}

/// An async operation that a command starts.
///
/// ```
/// use riff::watches::Op;
///
/// let op = Op::Pull { tag: "v4.3.0".into(), number: 1140 };
/// assert_eq!(serde_json::to_string(&op)?, r#"{"kind":"pull","tag":"v4.3.0","number":1140}"#);
/// assert_eq!(op.key(), "pull-1140");
/// assert_eq!(Op::Wave.key(), "wave");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    /// The pull request `number` of the next version after the release
    /// `tag`: its Gate, then its merge (`riff release cut`,
    /// 01M76428R8GY6J323AWG3JV8HA, 01M76SBPGH2218K3ED569T051D).
    Pull {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The number of the pull request.
        number: u64,
    },
    /// The real-session test of `tag` on `commit` (`riff release test`).
    Test {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The commit that the test runs on.
        commit: String,
    },
    /// The CI run of `tag` up to the end of the job `Deploy`, and after a
    /// pass up to the end of the job `Check the deploy`
    /// (`riff release cut`, `approve`, `retry`, `rollback`).
    Deploy {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The state that the command saw before it started the
        /// operation, when the tag had a CI run then.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stale: Option<Stale>,
    },
    /// The current wave of the repository: the last open item closes.
    Wave,
    /// The check of the lead after the end of the job `Deploy` of `tag`
    /// (01M6XTS7RGXB30MFM6DZ7PVZ75): the watch of the deploy leaves it at its
    /// end. It posts nothing. It gives the step of the release to the facts
    /// of the host ([`Watch::release`]).
    Check {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// True when the job `Deploy` passed, false when it failed.
        deployed: bool,
        /// The comments of the release issue at the start of the watch:
        /// only a later comment counts, for example after a retry.
        #[serde(default)]
        comments: usize,
        /// The published releases newer than `tag` at the start of the
        /// watch: one or more for the deploy of an older release, a
        /// rollback.
        #[serde(default)]
        newer: usize,
    },
}

impl Op {
    /// The name of the watch in its repository. Two watches with the
    /// same key are one watch.
    ///
    /// ```
    /// use riff::watches::Op;
    ///
    /// assert_eq!(Op::Check { tag: "v4.3.0".into(), deployed: true, comments: 0, newer: 0 }.key(), "check-v4.3.0");
    /// ```
    pub fn key(&self) -> String {
        match self {
            Op::Pull { number, .. } => format!("pull-{number}"),
            Op::Test { commit, .. } => format!("test-{commit}"),
            Op::Deploy { tag, .. } => format!("deploy-{tag}"),
            Op::Wave => "wave".to_owned(),
            Op::Check { tag, .. } => format!("check-{tag}"),
        }
    }

    /// What the watch waits for, in words for a line to the lead.
    ///
    /// ```
    /// use riff::watches::Op;
    ///
    /// let test = Op::Test { tag: "v4.3.0".into(), commit: "abc1234".into() };
    /// assert_eq!(test.what(), "the real-session test of v4.3.0 on abc1234");
    /// assert_eq!(Op::Wave.what(), "the current wave");
    /// let check = Op::Check { tag: "v4.3.0".into(), deployed: true, comments: 0, newer: 0 };
    /// assert_eq!(check.what(), "the check of v4.3.0 after its deploy");
    /// ```
    pub fn what(&self) -> String {
        match self {
            Op::Pull { number, .. } => format!("the pull request #{number} of the release"),
            Op::Test { tag, commit } => format!("the real-session test of {tag} on {commit}"),
            Op::Deploy { tag, .. } => format!("the deploy of {tag}"),
            Op::Wave => "the current wave".to_owned(),
            Op::Check { tag, .. } => format!("the check of {tag} after its deploy"),
        }
    }
}

/// What a watch saw at the looks before: it tells each fact one time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// [`Op::Deploy`]: the state moved from [`Stale`].
    pub moved: bool,
    /// [`Op::Pull`]: the lead got the news that the Gate failed, until the
    /// Gate runs again.
    pub gate_failed: bool,
    /// [`Op::Deploy`]: the job `Deploy` waits, and the lead got the news.
    pub waits: bool,
    /// [`Op::Deploy`]: the last line to the admins, in milliseconds since
    /// the epoch.
    pub reminded_ms: Option<i64>,
    /// [`Op::Wave`]: the title of the wave that had an open item.
    pub wave: Option<String>,
    /// [`Op::Deploy`]: the first look that saw the job `Deploy` passed, in
    /// milliseconds since the epoch. The message waits for the new riff
    /// from then on ([`Riffs::waits_for`], 01M6WBX2G1PX11D89HNBAZ6MCM).
    pub deployed_ms: Option<i64>,
    /// The last fault that the lead got, until a look reads the forge.
    pub fault: Option<String>,
    /// [`Op::Deploy`]: where the job `Deploy` was at the last look, from
    /// the look that saw the state move ([`Watch::release`]).
    pub state: Option<DeployState>,
    /// [`Op::Check`]: the last check on the release issue failed.
    pub check_failed: bool,
    /// [`Op::Wave`]: the current wave when it has no open item and its
    /// release waits for its cut, with why it waits: the full check of
    /// `main` (01M75N6YNHQQHGS0VGKTKSNJTR). The daemon gives it to the
    /// facts of its host.
    pub complete: Option<daemons::Complete>,
    /// [`Op::Wave`]: the last read of the release of the newest release
    /// issue of the current wave ([`ReleaseRead`],
    /// 01M75R6FPVZA3JA5C48PQ26A85). The daemon gives its release to the
    /// facts of its host.
    pub release: Option<ReleaseRead>,
}

/// The last read of the release of the current wave by the watch of the
/// wave (01M75R6FPVZA3JA5C48PQ26A85): the stage of the newest release issue
/// of the wave, as `riff release status` reads it ([`cycle::stage`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRead {
    /// The title of the release issue, for example `Release v4.17.0`.
    pub issue: String,
    /// The time of the read, in milliseconds since the epoch.
    pub at_ms: i64,
    /// The release at its step, or `None`: a checked release, a release
    /// after a rollback, and a title with no version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<daemons::Release>,
}

impl ReleaseRead {
    /// True when the watch reads the release again at `now_ms`: a read
    /// costs about ten calls of `gh`, so it comes each [`read_every`] at
    /// most.
    ///
    /// ```
    /// use riff::watches::{ReleaseRead, read_every};
    ///
    /// let read = ReleaseRead { issue: "Release v4.17.0".into(), at_ms: 0, release: None };
    /// let every = read_every().as_millis() as i64;
    /// assert!(!read.due(every - 1));
    /// assert!(read.due(every));
    /// ```
    pub fn due(&self, now_ms: i64) -> bool {
        let every = i64::try_from(read_every().as_millis()).unwrap_or(i64::MAX);
        now_ms.saturating_sub(self.at_ms) >= every
    }

    /// True when the read at `now_ms` needs a fetch of `origin` first: the
    /// stage of a release before its tag reads the last commit of
    /// `origin/main` ([`cycle::head`]). A read with no release, for
    /// example of the state `building` or of a main clone that lags, reads
    /// again after a fetch too.
    fn fetches(&self, now_ms: i64) -> bool {
        self.due(now_ms)
            && self
                .release
                .as_ref()
                .is_none_or(|r| r.step < ReleaseStep::Published)
    }
}

/// The time between two reads of the release of a wave by the watch of the
/// wave (01M75R6FPVZA3JA5C48PQ26A85): [`IDLE_FACTOR`] times [`every`]. A
/// new release issue in the wave gets its read at once.
pub fn read_every() -> Duration {
    Watches::wait(every(), true)
}

/// A watch in [`FILE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The operation.
    pub op: Op,
    /// The start of the watch, in milliseconds since the epoch.
    pub since_ms: i64,
    /// What the watch saw.
    #[serde(default)]
    pub seen: Seen,
}

impl Watch {
    /// The release that the watch follows, at the step that it saw last
    /// (01M6XTS7RGXB30MFM6DZ7PVZ75), or `None`: a watch of no deploy and of
    /// no check, a watch of a deploy before the state moved, and a CI run
    /// that builds. The watch of a wave gives the release of its last read
    /// ([`Seen::release`], 01M75R6FPVZA3JA5C48PQ26A85).
    ///
    /// ```
    /// use riff::cycle::DeployState;
    /// use riff::watches::{Op, Seen, Watch};
    /// use riff_client::daemons::ReleaseStep;
    ///
    /// let watch = |op: Op, seen: Seen| Watch { repo: "o/r".into(), op, since_ms: 0, seen };
    /// let deploy = Op::Deploy { tag: "v3.3.0".into(), stale: None };
    /// let step = |watch: Watch| watch.release().map(|r| r.step);
    /// assert_eq!(step(watch(deploy.clone(), Seen::default())), None);
    /// let at = |state| Seen { moved: true, state: Some(state), ..Seen::default() };
    /// assert_eq!(step(watch(deploy.clone(), at(DeployState::Before))), None);
    /// assert_eq!(step(watch(deploy.clone(), at(DeployState::Waiting))), Some(ReleaseStep::Waits));
    /// assert_eq!(step(watch(deploy.clone(), at(DeployState::Running))), Some(ReleaseStep::Runs));
    /// assert_eq!(step(watch(deploy.clone(), at(DeployState::Passed))), Some(ReleaseStep::Passed));
    /// let check = |deployed| Op::Check { tag: "v3.3.0".into(), deployed, comments: 0, newer: 0 };
    /// assert_eq!(step(watch(check(true), Seen::default())), Some(ReleaseStep::Passed));
    /// assert_eq!(step(watch(check(false), Seen::default())), Some(ReleaseStep::DeployFailed));
    /// let failed = Seen { check_failed: true, ..Seen::default() };
    /// assert_eq!(step(watch(check(true), failed)), Some(ReleaseStep::CheckFailed));
    /// assert_eq!(watch(check(true), Seen::default()).release().unwrap().repo, "o/r");
    /// assert_eq!(watch(Op::Wave, Seen::default()).release(), None);
    ///
    /// use riff::watches::ReleaseRead;
    /// let release = riff_client::daemons::Release {
    ///     repo: "o/r".into(),
    ///     tag: "v3.3.0".into(),
    ///     step: ReleaseStep::TestRuns,
    /// };
    /// let read = ReleaseRead { issue: "Release v3.3.0".into(), at_ms: 0, release: Some(release.clone()) };
    /// let seen = Seen { release: Some(read), ..Seen::default() };
    /// assert_eq!(watch(Op::Wave, seen).release(), Some(release));
    /// ```
    pub fn release(&self) -> Option<daemons::Release> {
        let (tag, step) = match &self.op {
            Op::Wave => return self.seen.release.as_ref()?.release.clone(),
            Op::Deploy { tag, .. } if self.seen.moved => match self.seen.state? {
                DeployState::Waiting => (tag, ReleaseStep::Waits),
                DeployState::Running => (tag, ReleaseStep::Runs),
                DeployState::Passed => (tag, ReleaseStep::Passed),
                DeployState::Before | DeployState::Failed => return None,
            },
            Op::Check { tag, .. } if self.seen.check_failed => (tag, ReleaseStep::CheckFailed),
            Op::Check { tag, deployed, .. } => match deployed {
                true => (tag, ReleaseStep::Passed),
                false => (tag, ReleaseStep::DeployFailed),
            },
            _ => return None,
        };
        Some(daemons::Release {
            repo: self.repo.clone(),
            tag: tag.clone(),
            step,
        })
    }
}

/// What [`look`] decided about one watch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The posts, in order.
    pub posts: Vec<Post>,
    /// True when the operation ended: the watch goes.
    pub end: bool,
    /// The watch that takes the place of the watch at its end: the watch
    /// of the check after a deploy ([`Op::Check`], 01M6XTS7RGXB30MFM6DZ7PVZ75).
    pub then: Option<Op>,
}

/// A post of a watch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Post {
    /// A message that wakes the lead of the repository.
    Lead(String),
    /// A message that wakes the lead of the repository, with a key
    /// (01M6FZY6NABXGXVRYF6W1509XE): the watch of each host makes the
    /// same message for the same facts, and riff-server drops the second
    /// post.
    Once {
        /// The text.
        body: String,
        /// The key of the post.
        key: String,
    },
    /// A note to each other lead of the riff: the step of the release
    /// that the watch saw ([`crate::hold::tell_leads`],
    /// 01M5SNV75DRFHR26BT2M8243VV).
    Leads(String),
    /// A line of riff-bot to each admin in `riff chat`: the deploy of `tag`
    /// waits for the approval ([`BotFact::ReleaseWaits`]).
    Admins {
        /// The tag of the release.
        tag: String,
        /// The ID of the CI run.
        run: u64,
        /// True for a reminder.
        reminder: bool,
    },
}

/// What a watch needs of the host: the forge of a repository, and the
/// posts. The daemon uses [`Here`]; a test gives a fake.
pub trait Lookout: Send + Sync {
    /// The forge of the main clone of `wave`. It runs `gh` and `git`, so
    /// the caller calls it on a thread that may block. With `fetch`, it
    /// fetches `origin` and its tags first: the watch of a wave needs only
    /// `gh`, but for the read of a release before its tag
    /// ([`ReleaseRead`]), and the main clone of a person stays as it is.
    fn forge(&self, wave: &Wave, fetch: bool) -> Result<Box<dyn Forge>>;

    /// Posts `body` as a message to the lead of the repository of
    /// `wave`, as the person. The post has `key`, when there is one
    /// (01M5TKG2K1AYK1KE7C2AA2W9DS).
    fn tell<'a>(
        &'a self,
        wave: &'a Wave,
        body: &'a str,
        key: Option<&'a str>,
    ) -> BoxFuture<'a, Result<()>>;

    /// Tells riff-server the fact `fact`, as the person: riff-server
    /// writes the line of riff-bot for it in `riff chat`
    /// ([`riff_core::wire::BotPost`]). The line names each admin with
    /// `@USER`, so it wakes the terminals of each admin
    /// (01M6Y1A5PTCVZ6GRJ0FXMART2T), and shows each hold of the version.
    fn bot<'a>(&'a self, wave: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>>;

    /// Posts `body` as a message to each lead of the riff, but the lead of
    /// the person in the repository of `wave`, as the person
    /// (01M5SNV75DRFHR26BT2M8243VV). That lead gets the message of the
    /// watch.
    fn leads<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>>;

    /// The riff of the daemon of each other host of the riff of `wave`
    /// with `update.auto` on, from `who` as the person ([`Riffs::hosts_of`],
    /// 01M6WFNDS76NESPXHMQPGHHQSS).
    fn hosts<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<BTreeMap<String, Semver>>>;

    /// The limit of GitHub that waits for its reset now (#1557). The watches
    /// make no call then. The default has none.
    fn limit(&self) -> Option<crate::ghcalls::Wait> {
        None
    }
}

/// The [`Lookout`] of the daemon: `gh` with the sign-in of the person,
/// and riff-server.
#[derive(Debug, Default, Clone, Copy)]
pub struct Here;

impl Lookout for Here {
    fn forge(&self, wave: &Wave, fetch: bool) -> Result<Box<dyn Forge>> {
        let gh = crate::daemon::sweep::gh(&wave.repo).by("watches");
        Ok(match fetch {
            true => Box::new(cycle::GitHub::new(gh, &wave.main)?),
            false => Box::new(cycle::GitHub::at(gh, &wave.main)),
        })
    }

    fn tell<'a>(
        &'a self,
        wave: &'a Wave,
        body: &'a str,
        key: Option<&'a str>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(crate::wave::post_as_person(
            wave,
            None,
            body,
            Kind::Message,
            key,
        ))
    }

    fn bot<'a>(&'a self, wave: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let person = crate::identity::person(&wave.place(None)?, &wave.server)?;
            let api = riff_client::api::Api::new(&wave.server).signed_in(None)?;
            api.bot(&person, fact).await
        })
    }

    fn leads<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let person = crate::identity::person(&wave.place(None)?, &wave.server)?;
            let api = riff_client::api::Api::new(&wave.server).signed_in(None)?;
            crate::hold::tell_leads(&api, &person, true, body).await?;
            Ok(())
        })
    }

    fn hosts<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<BTreeMap<String, Semver>>> {
        Box::pin(async move {
            let person = crate::identity::person(&wave.place(None)?, &wave.server)?;
            let api = riff_client::api::Api::new(&wave.server).signed_in(None)?;
            let sessions = api.who(&person, true).await?;
            let hosts = riff_client::daemons::hosts(&sessions);
            Ok(Riffs::hosts_of(&hosts, person.who().user(), &wave.host))
        })
    }

    fn limit(&self) -> Option<crate::ghcalls::Wait> {
        crate::ghcalls::Calls::global().limit_wait(crate::daemon::now_ms())
    }
}

/// The wave of the repository `repo` that the daemon in `dir` knows: the
/// wave that it runs, else the repository that it serves.
pub fn wave_of(dir: &Path, repo: &str) -> Result<Wave> {
    if let Some(wave) = crate::wave::read_waves(dir)?.remove(repo) {
        return Ok(wave);
    }
    remote::read_served(dir)?
        .remove(repo)
        .map(|served| served.wave)
        .ok_or_else(|| anyhow!(crate::text::watch_no_wave(repo)))
}

/// What [`Watches::register`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Registered {
    /// A new watch.
    New,
    /// The repository had this watch already: nothing changed.
    Kept,
}

/// The watches in [`FILE`] of the folder of the daemon.
#[derive(Debug, Clone)]
pub struct Watches {
    dir: PathBuf,
}

impl Watches {
    /// The watches in the folder `dir` of the daemon.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// Takes the lock of the file: the daemon and a command change it one
    /// at a time.
    fn lock(&self) -> Result<Flock<std::fs::File>> {
        let path = self.dir.join(LOCK);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, e)| anyhow!("cannot lock {}: {e}", path.display()))
    }

    /// Each watch of [`FILE`] in the order of their start, then each watch
    /// of [`CHECKS`]. With no file, none.
    pub fn read(&self) -> Result<Vec<Watch>> {
        let mut watches = read_file(&self.path())?;
        watches.extend(read_file(&self.dir.join(CHECKS))?);
        Ok(watches)
    }

    /// Writes the watches of a check to [`CHECKS`], and the others to
    /// [`FILE`].
    fn write(&self, watches: &[Watch]) -> Result<()> {
        let (checks, others): (Vec<&Watch>, Vec<&Watch>) = watches
            .iter()
            .partition(|w| matches!(w.op, Op::Check { .. }));
        write_file(&self.path(), &others)?;
        write_file(&self.dir.join(CHECKS), &checks)
    }

    /// Registers the watch of `op` in `repo`, started at `now_ms`. It
    /// fails when the daemon knows no clone of `repo`
    /// ([`wave_of`]): the watch could never look at the forge.
    ///
    /// ```
    /// use riff::watches::{Op, Registered, Watches};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let watches = Watches::new(dir.path());
    /// // The daemon runs no wave of o/r yet.
    /// assert!(watches.register("o/r", Op::Wave, 0).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// A new watch of a deploy ends each watch of a check of the same
    /// repository (01M6XTS7RGXB30MFM6DZ7PVZ75): a retry, a rollback or the
    /// next release follows its own deploy.
    pub fn register(&self, repo: &str, op: Op, now_ms: i64) -> Result<Registered> {
        wave_of(&self.dir, repo)?;
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let key = op.key();
        if watches.iter().any(|w| w.repo == repo && w.op.key() == key) {
            return Ok(Registered::Kept);
        }
        if matches!(op, Op::Deploy { .. }) {
            watches.retain(|w| w.repo != repo || !matches!(w.op, Op::Check { .. }));
        }
        watches.push(Watch {
            repo: repo.to_owned(),
            op,
            since_ms: now_ms,
            seen: Seen::default(),
        });
        self.write(&watches)?;
        Ok(Registered::New)
    }

    /// Keeps `watch` in the file in place of the watch with its key, or
    /// removes it when `end` is true. At the end, `then` takes its place,
    /// when the repository has no watch with the key of `then` yet. A
    /// watch that the file lost stays lost.
    fn settle(&self, watch: &Watch, end: bool, then: Option<Watch>) -> Result<()> {
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let key = watch.op.key();
        let Some(at) = watches
            .iter()
            .position(|w| w.repo == watch.repo && w.op.key() == key)
        else {
            return Ok(());
        };
        if !end {
            watches[at] = watch.clone();
            return self.write(&watches);
        }
        watches.remove(at);
        if let Some(then) = then {
            let key = then.op.key();
            if !watches
                .iter()
                .any(|w| w.repo == then.repo && w.op.key() == key)
            {
                watches.insert(at, then);
            }
        }
        self.write(&watches)
    }

    /// One watch of the current wave for each wave that the daemon runs
    /// (`waves.json`), and none for a repository that it does not run.
    fn keep_waves(&self, now_ms: i64) -> Result<()> {
        let waves = crate::wave::read_waves(&self.dir)?;
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let before = watches.len();
        watches.retain(|w| w.op != Op::Wave || waves.contains_key(&w.repo));
        let mut changed = watches.len() != before;
        for repo in waves.keys() {
            if !watches.iter().any(|w| w.repo == *repo && w.op == Op::Wave) {
                watches.push(Watch {
                    repo: repo.clone(),
                    op: Op::Wave,
                    since_ms: now_ms,
                    seen: Seen::default(),
                });
                changed = true;
            }
        }
        if changed {
            self.write(&watches)?;
        }
        Ok(())
    }

    /// One look at each watch (see "A look" above). It posts through
    /// `lookout`. The error is a file that riff cannot read or write. It
    /// gives `true` when no watch but the watch of a wave stands: that
    /// watch ends at no time, and it needs no quick look
    /// ([`Watches::pause`], #1549). `riffs` is what the daemon and its leads
    /// run, for a deploy that passed ([`Riffs`]). For a repository with the
    /// watch of a deploy, the look adds the riff of the daemon of each
    /// other host ([`Lookout::hosts`]).
    pub async fn look_all(
        &self,
        lookout: &Arc<dyn Lookout>,
        now_ms: i64,
        riffs: &Riffs,
    ) -> Result<bool> {
        self.keep_waves(now_ms)?;
        // No look while a limit of GitHub is out: each call would fail, and
        // the lead would hear the fault (#1557).
        if lookout.limit().is_some() {
            return Ok(false);
        }
        let mut by_repo: BTreeMap<String, Vec<Watch>> = BTreeMap::new();
        for watch in self.read()? {
            by_repo.entry(watch.repo.clone()).or_default().push(watch);
        }
        let slow = by_repo
            .values()
            .all(|watches| watches.iter().all(|w| w.op == Op::Wave));
        for (repo, watches) in by_repo {
            let wave = match wave_of(&self.dir, &repo) {
                Ok(wave) => wave,
                Err(e) => {
                    self.no_wave(watches, &format!("{e:#}"));
                    continue;
                }
            };
            let mut riffs = riffs.clone();
            if watches.iter().any(|w| matches!(w.op, Op::Deploy { .. })) {
                match lookout.hosts(&wave).await {
                    Ok(hosts) => riffs.hosts = hosts,
                    Err(e) => eprintln!("riff daemon: {repo}: cannot read the hosts: {e:#}"),
                }
            }
            let seen = {
                let (lookout, wave) = (lookout.clone(), wave.clone());
                tokio::task::spawn_blocking(move || {
                    read_all(&*lookout, &wave, watches, now_ms, &riffs)
                })
                .await?
            };
            let mut failed = Vec::new();
            for (watch, result) in seen {
                match result {
                    Ok(outcome) => {
                        self.settle_one(lookout, &wave, watch, outcome, now_ms)
                            .await
                    }
                    Err(e) => failed.push((watch, format!("{e:#}"))),
                }
            }
            self.settle_faults(lookout, &wave, failed).await;
        }
        Ok(slow)
    }

    /// How long the daemon waits after a look that gave `slow`: the time
    /// `every` between two looks, and for a look at the watch of a wave
    /// alone [`IDLE_FACTOR`] times that (#1549).
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff::watches::Watches;
    ///
    /// let every = Duration::from_secs(60);
    /// assert_eq!(Watches::wait(every, false), every);
    /// assert_eq!(Watches::wait(every, true), Duration::from_secs(300));
    /// ```
    pub fn wait(every: Duration, slow: bool) -> Duration {
        match slow {
            true => every * IDLE_FACTOR,
            false => every,
        }
    }

    /// Waits [`Watches::wait`] after a look. A slow wait looks at the
    /// file each `every`: a command that registers a watch in that time
    /// ends the wait, so the new watch gets its first look at once, and
    /// not after 5 minutes (#1549).
    pub async fn pause(&self, every: Duration, slow: bool) {
        let stamp = || {
            std::fs::metadata(self.path())
                .ok()
                .map(|m| (m.modified().ok(), m.len()))
        };
        let seen = stamp();
        let mut left = Self::wait(every, slow);
        while !left.is_zero() {
            let step = left.min(every);
            tokio::time::sleep(step).await;
            left -= step;
            if slow && stamp() != seen {
                break;
            }
        }
    }

    /// The watches of a repository whose clone the daemon does not know
    /// any more: the fault goes to stderr, one time.
    fn no_wave(&self, watches: Vec<Watch>, fault: &str) {
        for mut watch in watches {
            if watch.seen.fault.as_deref() == Some(fault) {
                continue;
            }
            eprintln!("riff daemon: {}: {fault}", watch.repo);
            watch.seen.fault = Some(fault.to_owned());
            if let Err(e) = self.settle(&watch, false, None) {
                eprintln!("riff daemon: {e:#}");
            }
        }
    }

    /// Posts what one look at `now_ms` found, then keeps or removes the
    /// watch. At its end, the watch of [`Outcome::then`] takes its place.
    async fn settle_one(
        &self,
        lookout: &Arc<dyn Lookout>,
        wave: &Wave,
        mut watch: Watch,
        outcome: Outcome,
        now_ms: i64,
    ) {
        watch.seen.fault = None;
        for post in &outcome.posts {
            if let Err(e) = post_one(&**lookout, wave, post).await {
                eprintln!("riff daemon: {}: {e:#}", watch.repo);
                // The lead has no message yet: the next look tries again.
                if matches!(post, Post::Lead(_) | Post::Once { .. }) {
                    return;
                }
            }
        }
        let then = outcome.then.map(|op| Watch {
            repo: watch.repo.clone(),
            op,
            since_ms: now_ms,
            seen: Seen::default(),
        });
        if let Err(e) = self.settle(&watch, outcome.end, then) {
            eprintln!("riff daemon: {e:#}");
        }
    }

    /// Tells the lead the faults of one look at a repository: one message
    /// for each fault, with the name of each watch that failed with it
    /// (see "A look" above). A watch that told the fault at a look before
    /// tells it no more, and no other watch of the repository tells it
    /// now. The post has a key ([`watch_fault_key`]).
    async fn settle_faults(
        &self,
        lookout: &Arc<dyn Lookout>,
        wave: &Wave,
        failed: Vec<(Watch, String)>,
    ) {
        // The wave tells the lead of a limit of GitHub (#1557).
        if lookout.limit().is_some() {
            return;
        }
        let mut by_fault: BTreeMap<String, Vec<Watch>> = BTreeMap::new();
        for (watch, fault) in failed {
            by_fault.entry(fault).or_default().push(watch);
        }
        for (fault, watches) in by_fault {
            let told = watches
                .iter()
                .any(|w| w.seen.fault.as_deref() == Some(fault.as_str()));
            if !told {
                let whats: Vec<String> = watches.iter().map(|w| w.op.what()).collect();
                let body = crate::text::watch_fault(&whats, &fault);
                let key = watch_fault_key(&fault);
                if let Err(e) = lookout.tell(wave, &body, Some(&key)).await {
                    eprintln!("riff daemon: {}: {e:#}", wave.repo);
                    continue;
                }
            }
            for mut watch in watches {
                if watch.seen.fault.as_deref() == Some(fault.as_str()) {
                    continue;
                }
                watch.seen.fault = Some(fault.clone());
                if let Err(e) = self.settle(&watch, false, None) {
                    eprintln!("riff daemon: {e:#}");
                }
            }
        }
    }
}

/// The key of the post of a fault (01M6H160E2ZK2AM0PCTQG82V2G):
/// `watch-fault-` and the start of the SHA-256 sum of the fault. The
/// watch of each host, and a watch that lost what it saw, make the same
/// key for the same fault, and riff-server drops the second post.
///
/// ```
/// use riff::watches::watch_fault_key;
///
/// let key = watch_fault_key("gh: HTTP 502");
/// assert!(key.starts_with("watch-fault-"));
/// assert_eq!(key.len(), "watch-fault-".len() + 16);
/// assert_eq!(key, watch_fault_key("gh: HTTP 502"));
/// assert_ne!(key, watch_fault_key("gh: HTTP 503"));
/// ```
pub fn watch_fault_key(fault: &str) -> String {
    let sum = crate::release::sha256(fault.as_bytes());
    format!("watch-fault-{}", &sum[..16])
}

/// The result of [`look`] for each of `watches`, with one forge.
fn read_all(
    lookout: &dyn Lookout,
    wave: &Wave,
    watches: Vec<Watch>,
    now_ms: i64,
    riffs: &Riffs,
) -> Vec<(Watch, Result<Outcome>)> {
    // The watch of a wave and the watch of a check need only `gh`, but for
    // the read of a release before its tag (01M75R6FPVZA3JA5C48PQ26A85).
    let fetch = watches.iter().any(|w| match w.op {
        Op::Wave => w
            .seen
            .release
            .as_ref()
            .is_some_and(|read| read.fetches(now_ms)),
        Op::Check { .. } => false,
        _ => true,
    });
    let forge = match lookout.forge(wave, fetch) {
        Ok(forge) => forge,
        Err(e) => {
            let fault = format!("{e:#}");
            return watches
                .into_iter()
                .map(|w| (w, Err(anyhow!(fault.clone()))))
                .collect();
        }
    };
    watches
        .into_iter()
        .map(|mut watch| {
            let result = look(&*forge, &mut watch, now_ms, riffs);
            (watch, result)
        })
        .collect()
}

async fn post_one(lookout: &dyn Lookout, wave: &Wave, post: &Post) -> Result<()> {
    match post {
        Post::Lead(body) => lookout.tell(wave, body, None).await,
        Post::Once { body, key } => lookout.tell(wave, body, Some(key)).await,
        Post::Leads(body) => lookout.leads(wave, body).await,
        Post::Admins { tag, run, reminder } => {
            let fact = BotFact::ReleaseWaits {
                tag: tag.clone(),
                run: *run,
                reminder: *reminder,
            };
            lookout.bot(wave, fact).await
        }
    }
}

/// The next step after an operation on `tag` ended: the next step of the
/// release at its state now ([`cycle::next`]), or `fallback`.
fn next_of(forge: &dyn Forge, tag: &str, fallback: &str) -> String {
    cycle::tag_version(tag)
        .and_then(|version| cycle::stage(forge, version))
        .ok()
        .flatten()
        .map_or_else(|| fallback.to_owned(), |stage| cycle::next(&stage))
}

/// The failed jobs for a line: `: A, B`, or a text for no name.
fn jobs_text(names: &[String], ended: &str) -> String {
    match names.is_empty() {
        true => format!(" (it ended with {ended})"),
        false => format!(": {}", names.join(", ")),
    }
}

/// Looks at the forge for `watch` at `now_ms`, and says what to post.
/// `riffs` is what the daemon and its leads run ([`Riffs`]). It changes
/// `watch.seen` for what it decided. It fails when the forge does not
/// answer.
pub fn look(forge: &dyn Forge, watch: &mut Watch, now_ms: i64, riffs: &Riffs) -> Result<Outcome> {
    match watch.op.clone() {
        Op::Pull { tag, number } => look_pull(forge, &mut watch.seen, &tag, number),
        Op::Test { tag, commit } => look_test(forge, &tag, &commit),
        Op::Deploy { tag, stale } => {
            let deploy = Deployed {
                repo: &watch.repo,
                tag: &tag,
                now_ms,
                riffs,
            };
            look_deploy(forge, &mut watch.seen, &deploy, stale)
        }
        Op::Wave => look_wave(forge, &watch.repo, &mut watch.seen, now_ms),
        Op::Check {
            tag,
            comments,
            newer,
            ..
        } => look_check(forge, &mut watch.seen, &tag, (comments, newer)),
    }
}

/// The facts of a look at the watch of a deploy.
struct Deployed<'a> {
    /// The repository, as `OWNER/REPO`.
    repo: &'a str,
    /// The tag of the release.
    tag: &'a str,
    /// The time of the look, in milliseconds since the epoch.
    now_ms: i64,
    /// What the daemon and its leads run.
    riffs: &'a Riffs,
}

fn say(body: String, end: bool) -> Outcome {
    Outcome {
        posts: vec![Post::Lead(body)],
        end,
        then: None,
    }
}

/// [`say`], and a note to each other lead of the riff
/// (01M5SNV75DRFHR26BT2M8243VV).
fn say_and_tell_leads(body: String, note: String, end: bool) -> Outcome {
    Outcome {
        posts: vec![Post::Lead(body), Post::Leads(note)],
        end,
        then: None,
    }
}

/// The end of the watch of the deploy of `tag` with `outcome`: the watch of
/// the check of the lead takes its place (01M6XTS7RGXB30MFM6DZ7PVZ75).
/// `deployed` is true when the job `Deploy` passed. The watch of the check
/// starts with the comments of the release issue and the releases newer
/// than `tag` that the forge has now ([`Op::Check`]). A read that fails
/// counts none: the end of the deploy waits for no read.
fn then_check(outcome: Outcome, forge: &dyn Forge, tag: &str, deployed: bool) -> Outcome {
    let (comments, newer) = cycle::tag_version(tag)
        .ok()
        .and_then(|version| {
            let newer = forge
                .releases()
                .ok()?
                .iter()
                .filter(|r| **r > version)
                .count();
            let issue = forge.issue_of(&cycle::title(version)).ok()?;
            Some((issue.map_or(0, |i| i.comments.len()), newer))
        })
        .unwrap_or_default();
    Outcome {
        then: Some(Op::Check {
            tag: tag.to_owned(),
            deployed,
            comments,
            newer,
        }),
        ..outcome
    }
}

/// The watch of the pull request `number` of the next version after the
/// release `tag` (01M76SBPGH2218K3ED569T051D). The cut passed its verify,
/// so it merges by itself when its Gate passes: a passed Gate gets no
/// message. A failed Gate gets one message, with the failed jobs, and the
/// watch stays: the lead runs the jobs again, and a new fail gets a new
/// message. The merge and the close end the watch. After the merge, the
/// next step is the step of the release at its stage now.
fn look_pull(forge: &dyn Forge, seen: &mut Seen, tag: &str, number: u64) -> Result<Outcome> {
    let pull = forge.pull(number)?;
    match pull.state.as_str() {
        "MERGED" => {
            let next = next_of(forge, tag, "riff release status");
            return Ok(say(
                format!("The pull request #{number} of the release {tag} merged. next: {next}"),
                true,
            ));
        }
        "CLOSED" => {
            let cut = cycle::tag_version(tag).map_or_else(
                |_| "riff release cut X.Y.Z".to_owned(),
                |v| format!("riff release cut {v}"),
            );
            return Ok(say(
                format!(
                    "The pull request #{number} of the release {tag} closed with no merge. \
                     next: {cut}"
                ),
                true,
            ));
        }
        _ => {}
    }
    let checks = forge.full_check()?;
    match forge.gate(&checks, &pull.head)? {
        Gate::Ended(conclusion) if !seen.gate_failed => {
            seen.gate_failed = true;
            let jobs = forge.failed_jobs(&checks, &pull.head)?;
            Ok(say(
                format!(
                    "The Gate of the pull request #{number} of the release {tag} failed{}. next: \
                     run the failed jobs again: the pull request merges when its Gate passes",
                    jobs_text(&jobs, &conclusion)
                ),
                false,
            ))
        }
        Gate::Ended(_) => Ok(Outcome::default()),
        _ => {
            seen.gate_failed = false;
            Ok(Outcome::default())
        }
    }
}

fn look_test(forge: &dyn Forge, tag: &str, commit: &str) -> Result<Outcome> {
    let Some(state @ (TestState::Passed | TestState::Failed)) = forge.test_state(commit)? else {
        return Ok(Outcome::default());
    };
    let next = next_of(forge, tag, "riff release status");
    Ok(say(
        match state {
            TestState::Passed => {
                format!("The real-session test of {tag} passed on {commit}. next: {next}")
            }
            _ => format!(
                "The real-session test of {tag} failed on {commit}. The failed step is on the \
                 release issue. next: {next}"
            ),
        },
        true,
    ))
}

fn look_deploy(
    forge: &dyn Forge,
    seen: &mut Seen,
    deployed: &Deployed,
    stale: Option<Stale>,
) -> Result<Outcome> {
    let (repo, tag, now_ms) = (deployed.repo, deployed.tag, deployed.now_ms);
    let Some(deploy) = forge.deploy(tag)? else {
        return Ok(Outcome::default());
    };
    if !seen.moved {
        let now = Stale {
            run: deploy.run,
            state: deploy.state,
        };
        if stale == Some(now) {
            return Ok(Outcome::default());
        }
        seen.moved = true;
    }
    seen.state = Some(deploy.state);
    let run = deploy.url.as_str();
    match deploy.state {
        DeployState::Before | DeployState::Running => {
            seen.waits = false;
            seen.reminded_ms = None;
            Ok(Outcome::default())
        }
        DeployState::Waiting if !seen.waits => {
            seen.waits = true;
            seen.reminded_ms = Some(now_ms);
            Ok(Outcome {
                posts: vec![
                    Post::Lead(format!(
                        "The deploy of {tag} waits for a person: {run}. riff told each admin \
                         in riff chat. next: a person runs riff release approve {tag} in a terminal"
                    )),
                    Post::Admins {
                        tag: tag.to_owned(),
                        run: deploy.run,
                        reminder: false,
                    },
                    Post::Leads(crate::text::release_note(Step::Waits, tag, &hold::by(repo))),
                ],
                end: false,
                then: None,
            })
        }
        DeployState::Waiting => {
            let due = seen
                .reminded_ms
                .is_none_or(|at| now_ms.saturating_sub(at) >= REMIND_EVERY.as_millis() as i64);
            if !due {
                return Ok(Outcome::default());
            }
            seen.reminded_ms = Some(now_ms);
            Ok(Outcome {
                posts: vec![Post::Admins {
                    tag: tag.to_owned(),
                    run: deploy.run,
                    reminder: true,
                }],
                end: false,
                then: None,
            })
        }
        DeployState::Passed => {
            // The message waits for the new riff of the daemon and of the
            // lead (01M6WBX2G1PX11D89HNBAZ6MCM), and for the job Check the
            // deploy (01M77BT0489T9R084WEFHPMCR5). A run by hand has no
            // such job.
            let since = *seen.deployed_ms.get_or_insert(now_ms);
            let waited = now_ms.saturating_sub(since);
            let settle = i64::try_from(DEPLOY_SETTLE.as_millis()).unwrap_or(i64::MAX);
            let riff_why = deployed.riffs.waits_for(repo, tag);
            if riff_why.is_some() && waited < settle {
                return Ok(Outcome::default());
            }
            let check = match deploy.by_hand {
                true => None,
                false => Some(forge.deploy_check(tag)?),
            };
            let runs = check
                .as_ref()
                .is_some_and(|c| cycle::check_waits(&deploy, c));
            if runs && waited < settle {
                return Ok(Outcome::default());
            }
            let next = next_of(forge, tag, &format!("riff release check {tag}"));
            // What the job found, the failed checks or the reason; or what
            // the message waited for; or the riff of the container.
            let found = match &check {
                Some(c) if !runs => {
                    let found = cycle::check_none(&deploy, c)
                        .map_or_else(|| cycle::log_of(tag, &deploy, c), Err);
                    crate::text::deploy_check_found(&found).map(|words| format!(" {words}"))
                }
                _ => None,
            };
            let whys: Vec<String> = riff_why
                .into_iter()
                .chain(runs.then(|| crate::text::deploy_check_runs(run)))
                .collect();
            let riff = match (whys.is_empty(), deployed.riffs.leads.contains_key(repo)) {
                (false, _) => format!(
                    " riff waited {} minutes, but {}.",
                    waited / 60_000,
                    whys.join(", and ")
                ),
                (true, true) => match deployed.riffs.daemon {
                    Some(daemon) => format!(
                        " Your container runs v{daemon}, as the riff daemon of this host does."
                    ),
                    None => " Your container runs the riff of the riff daemon of this host.".into(),
                },
                (true, false) => String::new(),
            };
            let said = say_and_tell_leads(
                format!(
                    "The deploy of {tag} passed: {run}.{}{riff} next: {next}",
                    found.unwrap_or_default()
                ),
                crate::text::release_note(Step::Deployed, tag, &hold::by(repo)),
                true,
            );
            Ok(then_check(said, forge, tag, true))
        }
        DeployState::Failed => {
            let next = next_of(
                forge,
                tag,
                &format!("riff release retry {tag}, or riff release rollback vPREVIOUS"),
            );
            let said = say_and_tell_leads(
                format!(
                    "The deploy of {tag} failed{}: {run}. The old revision still serves. next: \
                     {next}",
                    jobs_text(&deploy.failed, "a failure")
                ),
                crate::text::release_note(Step::DeployFailed, tag, &hold::by(repo)),
                true,
            );
            Ok(then_check(said, forge, tag, false))
        }
    }
}

/// One look of the watch of the check of `tag` (01M6XTS7RGXB30MFM6DZ7PVZ75).
/// It reads the releases and the release issue, and posts nothing. Only the
/// comments after the first `comments` count, and only the releases newer
/// than `tag` past the first `newer`. A failed check keeps the watch with
/// [`Seen::check_failed`]. The watch ends when a newer release is
/// published, when the release issue gets a passed check, a check of a
/// failed deploy or a rollback ([`cycle::check_mark`],
/// [`cycle::rollback_mark`]), and when the release issue is closed. The
/// issue of an older release is closed from before: after a rollback only
/// a check ends the watch. A tag with no version ends it.
fn look_check(
    forge: &dyn Forge,
    seen: &mut Seen,
    tag: &str,
    (comments, newer): (usize, usize),
) -> Result<Outcome> {
    let ended = Outcome {
        end: true,
        ..Outcome::default()
    };
    let Ok(version) = cycle::tag_version(tag) else {
        return Ok(ended);
    };
    if forge.releases()?.iter().filter(|r| **r > version).count() > newer {
        return Ok(ended);
    }
    let Some(issue) = forge.issue_of(&cycle::title(version))? else {
        return Ok(Outcome::default());
    };
    let fresh = issue.comments.get(comments..).unwrap_or_default();
    let marks = [
        cycle::check_mark(tag, "passed"),
        cycle::check_mark(tag, "failed"),
        cycle::check_mark(tag, "unknown"),
        cycle::check_mark(tag, "failed deploy"),
    ];
    let last = fresh
        .iter()
        .rev()
        .filter_map(|c| c.lines().next())
        .find(|line| marks.iter().any(|m| m.as_str() == *line));
    let rolled_back = fresh
        .iter()
        .any(|c| c.starts_with("riff release rollback v"));
    let closed = !issue.open && newer == 0;
    if closed || rolled_back || last.is_some_and(|l| *l == marks[0] || *l == marks[3]) {
        return Ok(ended);
    }
    seen.check_failed = last.is_some_and(|l| *l == marks[1]);
    Ok(Outcome::default())
}

/// One look of the watch of the current wave of `repo` at `now_ms`. When the
/// last open item of the wave closes, it tells the lead one time. While the
/// wave of a repository with the table `[release]` has no open item and no
/// release issue, the watch keeps it in [`Seen::complete`], with why the cut
/// waits ([`cut_waits`], 01M75N6YNHQQHGS0VGKTKSNJTR). The release issue of
/// the cut ends it. From then on, the watch keeps the release of the newest
/// release issue of the wave at its step in [`Seen::release`]
/// ([`read_release`], 01M75R6FPVZA3JA5C48PQ26A85). It reads it again each
/// [`read_every`], and at once for a new release issue. A read that fails
/// keeps the last read.
fn look_wave(forge: &dyn Forge, repo: &str, seen: &mut Seen, now_ms: i64) -> Result<Outcome> {
    let Some((title, open)) = forge.wave_open()? else {
        seen.complete = None;
        seen.release = None;
        return Ok(Outcome::default());
    };
    let number = hygiene::wave_number(&title).unwrap_or_default();
    let release = forge.has_release();
    let issue = match release {
        true => forge.wave_release(&title)?,
        false => None,
    };
    let cut = issue.is_some();
    let kept = |read: &ReleaseRead, issue: &str| read.issue == issue && !read.due(now_ms);
    seen.release = match issue {
        None => None,
        Some(issue) if seen.release.as_ref().is_some_and(|r| kept(r, &issue)) => {
            seen.release.take()
        }
        Some(issue) => Some(ReleaseRead {
            release: read_release(forge, repo, &issue)?,
            issue,
            at_ms: now_ms,
        }),
    };
    seen.complete = match open == 0 && release && !cut {
        true => Some(daemons::Complete {
            repo: repo.to_owned(),
            wave: number,
            waits: cut_waits(forge),
        }),
        false => None,
    };
    if open > 0 {
        seen.wave = Some(title);
        return Ok(Outcome::default());
    }
    if seen.wave.as_deref() != Some(title.as_str()) {
        return Ok(Outcome::default());
    }
    seen.wave = None;
    let next = match release {
        true => format!(
            "riff release test vX.Y.Z --host HOST, then riff release cut X.Y.Z, or riff wave end \
             {number} --no-release TEXT"
        ),
        false => format!("riff wave end {number}"),
    };
    Ok(Outcome {
        posts: vec![Post::Once {
            body: format!("Wave {number} is complete: next {next}"),
            key: wave_complete_key(number),
        }],
        end: false,
        then: None,
    })
}

/// The release of the release issue `issue` of the current wave of `repo` at
/// its step, as `riff release status` reads it ([`cycle::stage`],
/// 01M75R6FPVZA3JA5C48PQ26A85, 01M7642H27T4862WZENFAN4277). Before the tag,
/// the real-session test of the last commit of `origin/main` gives the step.
/// `None` for a title with no version, a release with no tag that is not
/// the version on `origin/main`, the state `building`, a checked release
/// and a release after a rollback.
fn read_release(forge: &dyn Forge, repo: &str, issue: &str) -> Result<Option<daemons::Release>> {
    let Some(version) = issue
        .strip_prefix("Release ")
        .and_then(|tag| cycle::tag_version(tag).ok())
    else {
        return Ok(None);
    };
    let Some(stage) = cycle::stage(forge, version)? else {
        return Ok(None);
    };
    let step = match stage.state {
        State::Built => match stage.head.and_then(|h| h.real_session) {
            None | Some(TestState::Passed) => ReleaseStep::Merged,
            Some(TestState::Running) => ReleaseStep::TestRuns,
            Some(TestState::Failed) => ReleaseStep::TestFailed,
        },
        State::Tested => ReleaseStep::Tested,
        State::Published => ReleaseStep::Published,
        State::Waiting => ReleaseStep::Waits,
        State::Deploying => ReleaseStep::Runs,
        State::Deployed => ReleaseStep::Passed,
        State::Failed if stage.checks_failed => ReleaseStep::CheckFailed,
        State::Failed => ReleaseStep::DeployFailed,
        State::Building | State::Checked | State::RolledBack => return Ok(None),
    };
    Ok(Some(daemons::Release {
        repo: repo.to_owned(),
        tag: cycle::tag(version),
        step,
    }))
}

/// Why the cut of the release of a complete wave waits, from the full check
/// of the head of `main` (01M75N6YNHQQHGS0VGKTKSNJTR): it runs, it failed, or
/// it passed and the lead cuts next. `None` when the forge does not answer,
/// when the head has no run of a check, and for a check that ended with no
/// failure, for example `cancelled`.
fn cut_waits(forge: &dyn Forge) -> Option<daemons::CutWaits> {
    let checks = forge.full_check().ok()?;
    match forge.gate(&checks, "main").ok()? {
        Gate::Passed => Some(daemons::CutWaits::Lead),
        Gate::Running => Some(daemons::CutWaits::MainRuns),
        gate if gate.broken() => Some(daemons::CutWaits::MainFailed),
        Gate::Missing | Gate::Ended(_) => None,
    }
}

/// The key of the post "Wave N is complete" (01M6FZY6NABXGXVRYF6W1509XE).
/// The watch of the wave on each host makes the same key for the same wave,
/// and riff-server drops the second post.
///
/// ```
/// assert_eq!(riff::watches::wave_complete_key(45), "wave-complete-45");
/// ```
pub fn wave_complete_key(number: u64) -> String {
    format!("wave-complete-{number}")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::task::JoinHandle;

    use super::*;
    use crate::cycle::tests::Fake;
    use crate::cycle::{CheckJob, Deploy, DeployCheck, Pull};

    const REPO: &str = "o/r";

    /// The host of a test: a fake forge that a test changes between two
    /// looks, and the posts that the watches made.
    struct Ground {
        forge: Mutex<Fake>,
        /// The fault of the forge, when it fails.
        fault: Mutex<Option<String>>,
        /// True when riff-server does not take a message.
        down: AtomicBool,
        told: Mutex<Vec<String>>,
        chats: Mutex<Vec<BotFact>>,
        /// The key of each message to the lead.
        keys: Mutex<Vec<Option<String>>>,
        /// The notes to each other lead.
        leads: Mutex<Vec<String>>,
        /// The `fetch` of each forge that the watches asked for.
        fetches: Mutex<Vec<bool>>,
        /// The limit of GitHub that waits, and whether it starts only after
        /// the first forge (a call that runs into the limit).
        limit: Mutex<Option<(crate::ghcalls::Wait, bool)>>,
        /// What the daemon and its leads run.
        riffs: Mutex<Riffs>,
        /// What the daemon of each other host runs, as `who` tells it.
        hosts: Mutex<BTreeMap<String, Semver>>,
    }

    impl Ground {
        /// The ground of `forge`. The job Check the deploy set the status
        /// `success` of the tag, unless `forge` has a status.
        fn new(mut forge: Fake) -> Arc<Ground> {
            if forge.deploy_check.status.is_none() {
                forge.deploy_check = log_checked("success", LOG_PASSED);
            }
            Arc::new(Ground {
                forge: Mutex::new(forge),
                fault: Mutex::new(None),
                down: AtomicBool::new(false),
                told: Mutex::default(),
                keys: Mutex::default(),
                chats: Mutex::default(),
                leads: Mutex::default(),
                fetches: Mutex::default(),
                limit: Mutex::default(),
                riffs: Mutex::default(),
                hosts: Mutex::default(),
            })
        }

        fn riffs(&self, change: impl FnOnce(&mut Riffs)) {
            change(&mut self.riffs.lock().unwrap());
        }

        /// The daemon of the other host `host` runs `version`.
        fn host(&self, host: &str, version: &str) {
            let version = version.parse().unwrap();
            self.hosts.lock().unwrap().insert(host.to_owned(), version);
        }

        fn forge(&self, change: impl FnOnce(&mut Fake)) {
            change(&mut self.forge.lock().unwrap());
        }

        /// The messages to the lead since the last call.
        fn told(&self) -> Vec<String> {
            std::mem::take(&mut self.told.lock().unwrap())
        }

        /// The key of each message to the lead since the last call.
        fn keys(&self) -> Vec<Option<String>> {
            std::mem::take(&mut self.keys.lock().unwrap())
        }

        /// The facts of the lines of riff-bot since the last call.
        fn chats(&self) -> Vec<BotFact> {
            std::mem::take(&mut self.chats.lock().unwrap())
        }

        /// The notes to each other lead since the last call.
        fn leads(&self) -> Vec<String> {
            std::mem::take(&mut self.leads.lock().unwrap())
        }
    }

    impl Lookout for Ground {
        fn limit(&self) -> Option<crate::ghcalls::Wait> {
            let (wait, late) = (*self.limit.lock().unwrap())?;
            (!late || !self.fetches.lock().unwrap().is_empty()).then_some(wait)
        }

        fn forge(&self, _: &Wave, fetch: bool) -> Result<Box<dyn Forge>> {
            self.fetches.lock().unwrap().push(fetch);
            if let Some(fault) = self.fault.lock().unwrap().clone() {
                return Err(anyhow!(fault));
            }
            Ok(Box::new(self.forge.lock().unwrap().clone()))
        }

        fn tell<'a>(
            &'a self,
            _: &'a Wave,
            body: &'a str,
            key: Option<&'a str>,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("riff-server does not answer"));
                }
                self.told.lock().unwrap().push(body.to_owned());
                self.keys.lock().unwrap().push(key.map(str::to_owned));
                Ok(())
            })
        }

        fn bot<'a>(&'a self, _: &'a Wave, fact: BotFact) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                self.chats.lock().unwrap().push(fact);
                Ok(())
            })
        }

        fn leads<'a>(&'a self, _: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("riff-server does not answer"));
                }
                self.leads.lock().unwrap().push(body.to_owned());
                Ok(())
            })
        }

        fn hosts<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<BTreeMap<String, Semver>>> {
            Box::pin(async move { Ok(self.hosts.lock().unwrap().clone()) })
        }
    }

    /// The folder of a daemon that runs the wave of `o/r`.
    fn daemon_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let wave = Wave {
            repo: REPO.into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(dir.path(), &BTreeMap::from([(REPO.to_owned(), wave)])).unwrap();
        dir
    }

    const MINUTE: i64 = 60_000;

    /// One look of a new `Watches` on the folder: a daemon that started
    /// again reads the file. The daemon and its leads run what
    /// `ground.riffs` holds.
    async fn look(dir: &Path, ground: &Arc<Ground>, now_ms: i64) {
        let riffs = ground.riffs.lock().unwrap().clone();
        let ground: Arc<dyn Lookout> = ground.clone();
        Watches::new(dir)
            .look_all(&ground, now_ms, &riffs)
            .await
            .unwrap();
    }

    fn watch(dir: &Path, op: Op) {
        let registered = Watches::new(dir).register(REPO, op, 0).unwrap();
        assert_eq!(registered, Registered::New);
    }

    /// The watches in the file, but the watch of the wave.
    fn kept(dir: &Path) -> Vec<Op> {
        let watches = Watches::new(dir).read().unwrap();
        watches
            .into_iter()
            .map(|w| w.op)
            .filter(|op| *op != Op::Wave)
            .collect()
    }

    fn pull() -> Op {
        Op::Pull {
            tag: "v3.3.0".into(),
            number: 1140,
        }
    }

    /// A forge with an open pull request of the next version whose Gate
    /// runs.
    fn open_pull() -> Fake {
        Fake {
            pull: Pull {
                state: "OPEN".into(),
                head: "h1".into(),
            },
            checks: vec!["Gate".into()],
            gate: Gate::Running,
            ..Fake::new()
        }
    }

    /// The pull request of the next version 3.4.0 merged after the cut of
    /// 3.3.0: the deploy of v3.3.0 waits for a person.
    fn merged(fake: &mut Fake) {
        fake.pull.state = "MERGED".into();
        fake.main = "3.4.0".into();
        fake.releases = vec!["3.0.0", "3.2.0", "3.3.0"];
        fake.tags = vec!["v3.3.0".into()];
        fake.deploy = deploy(DeployState::Waiting, &[]);
    }

    fn deploy(state: DeployState, failed: &[&str]) -> Option<Deploy> {
        Some(Deploy {
            run: 9,
            url: "https://github.com/o/r/actions/runs/9".into(),
            state,
            at: String::new(),
            ended: String::new(),
            failed: failed.iter().map(ToString::to_string).collect(),
            by_hand: false,
            check_job: match state {
                DeployState::Passed => CheckJob::Ended("success".into()),
                _ => CheckJob::Runs,
            },
        })
    }

    /// The description of the status riff/deploy-check when each check of
    /// the log passed.
    const LOG_PASSED: &str = "503 run: 0 s, 500: 0, token store: 0, old lease: no";

    /// The status riff/deploy-check of the tag that the job Check the
    /// deploy set: `state` with `description`.
    fn log_checked(state: &str, description: &str) -> DeployCheck {
        DeployCheck {
            commit: "c1".into(),
            status: Some(crate::cycle::CommitStatus {
                state: state.into(),
                description: description.into(),
                at: String::new(),
            }),
        }
    }

    /// 01M76SBPGH2218K3ED569T051D, 01M76428R8GY6J323AWG3JV8HA: the cut
    /// passed the verify of the pull request of the next version, so a
    /// Gate that passes gets no message: the pull request merges by itself.
    /// The merge names the next step of the release that it follows, here
    /// the approval of the deploy, and the watch goes.
    #[tokio::test]
    async fn the_gate_passes_with_no_message_and_the_merge_names_the_step_of_the_release() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());

        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty(), "the Gate runs");

        ground.forge(|f| f.gate = Gate::Passed);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty(), "it merges by itself");
        assert_eq!(kept(dir.path()), [pull()]);

        ground.forge(merged);
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The pull request #1140 of the release v3.3.0 merged. next: a person runs riff \
                 release approve v3.3.0 in a terminal"
            ]
        );
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert!(ground.told().is_empty(), "no second message for the merge");
        assert!(kept(dir.path()).is_empty(), "the watch goes");
    }

    /// 01M76SBPGH2218K3ED569T051D: a merge whose release riff cannot read
    /// names riff release status.
    #[tokio::test]
    async fn a_merge_with_no_release_to_read_names_the_status() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());
        ground.forge(|f| f.pull.state = "MERGED".into());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The pull request #1140 of the release v3.3.0 merged. next: riff release status"]
        );
        assert!(kept(dir.path()).is_empty(), "the watch goes");
    }

    /// 01M76SBPGH2218K3ED569T051D: a failed Gate tells the failed jobs one
    /// time, and the watch stays. After the jobs run again, a new fail
    /// gets a new message, and the merge ends the watch.
    #[tokio::test]
    async fn a_gate_that_fails_tells_the_failed_jobs_once_and_the_watch_stays() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| {
            f.gate = Gate::Ended("failure".into());
            f.failed = vec!["Clippy".into(), "Tests 1/4".into()];
        });
        watch(dir.path(), pull());
        let failed = "The Gate of the pull request #1140 of the release v3.3.0 failed: Clippy, \
                      Tests 1/4. next: run the failed jobs again: the pull request merges when \
                      its Gate passes";

        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(ground.told(), [failed]);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert_eq!(kept(dir.path()), [pull()]);

        ground.forge(|f| f.gate = Gate::Running);
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty(), "the jobs run again");
        ground.forge(|f| f.gate = Gate::Ended("failure".into()));
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert_eq!(ground.told(), [failed]);

        ground.forge(merged);
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty(), "the watch goes");
    }

    #[tokio::test]
    async fn a_pull_request_that_closes_with_no_merge_tells_the_lead() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| f.pull.state = "CLOSED".into());
        watch(dir.path(), pull());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The pull request #1140 of the release v3.3.0 closed with no merge. next: riff \
                 release cut 3.3.0"
            ]
        );
        assert!(kept(dir.path()).is_empty());
    }

    fn test_op() -> Op {
        Op::Test {
            tag: "v3.3.0".into(),
            commit: "c1".into(),
        }
    }

    /// A forge whose last commit c1 of origin/main has 3.3.0, its release
    /// build, and a real-session test that runs.
    fn testing() -> Fake {
        let mut fake = Fake::ready("3.3.0");
        fake.running = vec!["c1"];
        fake.real = vec![];
        fake
    }

    #[tokio::test]
    async fn a_test_that_passes_tells_the_lead_the_next_step_once() {
        let dir = daemon_dir();
        let ground = Ground::new(testing());
        watch(dir.path(), test_op());
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty(), "the test runs");

        ground.forge(|f| {
            f.running.clear();
            f.real = vec![("c1", true)];
        });
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The real-session test of v3.3.0 passed on c1. next: riff release cut 3.3.0"]
        );
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_test_that_fails_tells_the_lead_the_next_step_once() {
        let dir = daemon_dir();
        let ground = Ground::new(testing());
        watch(dir.path(), test_op());
        ground.forge(|f| {
            f.running.clear();
            f.real = vec![("c1", false)];
        });
        look(dir.path(), &ground, MINUTE).await;
        let told = ground.told();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(
            told[0].starts_with(
                "The real-session test of v3.3.0 failed on c1. The failed step is on the \
                 release issue. next: file the bug in the wave"
            ),
            "{told:?}"
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    fn deploy_op(stale: Option<Stale>) -> Op {
        Op::Deploy {
            tag: "v3.3.0".into(),
            stale,
        }
    }

    /// The watch of the check that the watch of the deploy of v3.3.0
    /// leaves at its end (01M6XTS7RGXB30MFM6DZ7PVZ75).
    fn check_op(deployed: bool) -> Op {
        Op::Check {
            tag: "v3.3.0".into(),
            deployed,
            comments: 0,
            newer: 0,
        }
    }

    /// The fact of the line of riff-bot to the admins while the run 9 of
    /// v3.3.0 waits: riff-server writes the text, with each hold of the
    /// version (01M5SNV7BC5J58WRZRJ5HS4NSJ).
    fn waits(reminder: bool) -> BotFact {
        BotFact::ReleaseWaits {
            tag: "v3.3.0".into(),
            run: 9,
            reminder,
        }
    }

    #[tokio::test]
    async fn a_run_that_still_builds_says_nothing_of_the_approval() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.deploy = deploy(DeployState::Before, &[]));
        watch(dir.path(), deploy_op(None));

        for at in 1..4 {
            look(dir.path(), &ground, at * MINUTE).await;
        }
        assert!(ground.told().is_empty(), "the jobs before Deploy run");
        assert!(ground.chats().is_empty(), "no line of an approval");
        assert_eq!(kept(dir.path()), [deploy_op(None)]);

        // The job Deploy waits: the lead and each admin get it, one time.
        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The deploy of v3.3.0 waits for a person: \
                 https://github.com/o/r/actions/runs/9. riff told each admin in riff chat. \
                 next: a person runs riff release approve v3.3.0 in a terminal"]
        );
        assert_eq!(ground.chats(), [waits(false)],);
        // 01M5SNV75DRFHR26BT2M8243VV: each other lead gets the wait too.
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::Waits,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(ground.chats().is_empty());
        assert!(ground.leads().is_empty(), "one note for the wait");
    }

    #[tokio::test]
    async fn a_deploy_that_waits_reminds_each_admin_each_four_hours() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, 0).await;
        assert_eq!((ground.told().len(), ground.chats().len()), (1, 1));

        let hours = |n: i64| n * 60 * MINUTE;
        look(dir.path(), &ground, hours(3)).await;
        assert!(ground.chats().is_empty(), "3 hours is too soon");
        look(dir.path(), &ground, hours(4)).await;
        let chats = ground.chats();
        assert_eq!(chats, [waits(true)]);
        assert!(
            ground.told().is_empty(),
            "a reminder is no message to the lead"
        );
        look(dir.path(), &ground, hours(5)).await;
        assert!(ground.chats().is_empty());
        look(dir.path(), &ground, hours(8)).await;
        assert_eq!(ground.chats().len(), 1);
    }

    #[tokio::test]
    async fn a_deploy_that_passes_tells_the_lead_once() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Waiting, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        ground.told();

        // A person approved: the job runs, then passes.
        ground.forge(|f| f.deploy = deploy(DeployState::Running, &[]));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.chats().len() == 1);
        ground.forge(|f| f.deploy = deploy(DeployState::Passed, &[]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. next: \
                 riff release check v3.3.0"
            ]
        );
        assert_eq!(
            ground.leads(),
            [
                crate::text::release_note(Step::Waits, "v3.3.0", "the lead of o/r"),
                crate::text::release_note(Step::Deployed, "v3.3.0", "the lead of o/r"),
            ]
        );
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// A forge of v3.3.0 whose job Deploy passed, and whose job Check the
    /// deploy runs: the tag has no status riff/deploy-check yet.
    fn check_runs(ground: &Ground) {
        ground.forge(|f| {
            f.tags = vec!["v3.3.0".into()];
            f.deploy = deploy(DeployState::Passed, &[]);
            f.deploy.as_mut().unwrap().check_job = CheckJob::Runs;
            f.deploy_check = DeployCheck {
                commit: "c1".into(),
                status: None,
            };
        });
    }

    /// 01M77BT0489T9R084WEFHPMCR5: the message of a deploy that passed, and
    /// the note to the other leads, wait for the status riff/deploy-check
    /// of the job Check the deploy. The lead gets the step of the check
    /// only then.
    #[tokio::test]
    async fn a_deploy_that_passed_waits_for_the_job_check_the_deploy() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        check_runs(&ground);
        watch(dir.path(), deploy_op(None));
        for minute in 1..=5 {
            look(dir.path(), &ground, minute * MINUTE).await;
        }
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert_eq!(kept(dir.path()), [deploy_op(None)]);
        // The deploy passed: riff who says so while the message waits.
        let watches = Watches::new(dir.path()).read().unwrap();
        assert_eq!(watches[0].release().unwrap().step, ReleaseStep::Passed);

        // The job sets the status: the message goes before the job ends.
        ground.forge(|f| f.deploy_check = log_checked("success", LOG_PASSED));
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. next: \
                 riff release check v3.3.0"
            ]
        );
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::Deployed,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// 01M77BT0489T9R084WEFHPMCR5: a job Check the deploy that failed, and a
    /// status `error`, wake the lead with the reason: the failed checks of
    /// the log, the description of the error, or the end of a job that set
    /// no status.
    #[tokio::test]
    async fn a_job_check_the_deploy_that_fails_wakes_the_lead_with_the_reason() {
        /// The end of the job, and the reason in the message of the lead.
        type End = (Box<dyn Fn(&mut Fake)>, String);
        let url = "https://github.com/o/r/actions/runs/9";
        let cases: [End; 3] = [
            (
                Box::new(|f| {
                    f.deploy.as_mut().unwrap().check_job = CheckJob::Ended("failure".into());
                    f.deploy_check = log_checked(
                        "failure",
                        "503 run: 25 s, 500: 0, token store: 3, old lease: no",
                    );
                }),
                "The job Check the deploy found a failed check of the log: a run of 503 took 25 \
                 s; 3 lines."
                    .into(),
            ),
            (
                Box::new(|f| {
                    f.deploy_check = log_checked(
                        "error",
                        "The job Check the deploy failed before it read the log.",
                    );
                }),
                "The check of the log has no result: the status riff/deploy-check of v3.3.0 \
                 says: The job Check the deploy failed before it read the log."
                    .into(),
            ),
            (
                Box::new(|f| {
                    f.deploy.as_mut().unwrap().check_job = CheckJob::Ended("cancelled".into());
                }),
                format!(
                    "The check of the log has no result: the job Check the deploy of {url} \
                     ended with cancelled, and set no status riff/deploy-check for this deploy."
                ),
            ),
        ];
        for (end, reason) in cases {
            let dir = daemon_dir();
            let ground = Ground::new(Fake::ready("3.3.0"));
            check_runs(&ground);
            watch(dir.path(), deploy_op(None));
            look(dir.path(), &ground, MINUTE).await;
            assert!(ground.told().is_empty(), "{reason}");
            ground.forge(|f| end(f));
            look(dir.path(), &ground, 7 * MINUTE).await;
            assert_eq!(
                ground.told(),
                [format!(
                    "The deploy of v3.3.0 passed: {url}. {reason} next: riff release check v3.3.0"
                )]
            );
            assert_eq!(ground.leads().len(), 1, "{reason}");
            assert_eq!(kept(dir.path()), [check_op(true)], "{reason}");
        }
    }

    /// 01M77BT0489T9R084WEFHPMCR5: after 30 minutes the message goes all
    /// the same, and names the job that did not end.
    #[tokio::test]
    async fn a_deploy_that_passed_names_after_30_minutes_the_job_check_the_deploy() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        check_runs(&ground);
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        look(dir.path(), &ground, 30 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());

        look(dir.path(), &ground, 31 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. riff \
                 waited 30 minutes, but the job Check the deploy of \
                 https://github.com/o/r/actions/runs/9 did not end. next: riff release check \
                 v3.3.0"
            ]
        );
        assert_eq!(ground.leads().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// 01M77BT0489T9R084WEFHPMCR5: a run by hand, as of a rollback, has no
    /// job Check the deploy: its message waits for none.
    #[tokio::test]
    async fn a_deploy_by_hand_waits_for_no_job_check_the_deploy() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        check_runs(&ground);
        ground.forge(|f| f.deploy.as_mut().unwrap().by_hand = true);
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. next: \
                 riff release check v3.3.0"
            ]
        );
    }

    /// #1693, 01M6WBX2G1PX11D89HNBAZ6MCM: the message of a deploy that
    /// passed, and the note to the other leads, wait until the daemon runs
    /// the release and the lead runs the riff of the daemon. Each look is
    /// a new daemon that reads the file.
    #[tokio::test]
    async fn a_deploy_that_passed_waits_for_the_new_riff_of_the_daemon_and_the_lead() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Passed, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        let version = |v: &str| Some(v.parse().unwrap());
        ground.riffs(|r| {
            r.daemon = version("3.2.0");
            r.auto = true;
            r.leads = BTreeMap::from([(REPO.to_owned(), false)]);
        });
        watch(dir.path(), deploy_op(None));

        // The daemon still runs the old release.
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());

        // `riff update` replaced its binary: it starts again soon.
        ground.riffs(|r| r.replaced = true);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());

        // The new daemon runs the release, and the lead the old riff.
        ground.riffs(|r| {
            r.replaced = false;
            r.daemon = version("3.3.0");
            r.leads = BTreeMap::from([(REPO.to_owned(), true)]);
        });
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert_eq!(kept(dir.path()), [deploy_op(None)]);

        // The end of the turn: the lead runs the riff of the daemon.
        ground.riffs(|r| r.leads = BTreeMap::from([(REPO.to_owned(), false)]));
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. Your \
                 container runs v3.3.0, as the riff daemon of this host does. next: riff \
                 release check v3.3.0"
            ]
        );
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::Deployed,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// 01M6WBX2G1PX11D89HNBAZ6MCM: after 30 minutes the message goes all
    /// the same, and says what it waited for.
    #[tokio::test]
    async fn a_deploy_that_passed_tells_the_lead_after_30_minutes_what_it_waited_for() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Passed, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        ground.riffs(|r| {
            r.daemon = Some("3.3.0".parse().unwrap());
            r.auto = true;
            r.leads = BTreeMap::from([(REPO.to_owned(), true)]);
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        look(dir.path(), &ground, 30 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());

        look(dir.path(), &ground, 31 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. riff \
                 waited 30 minutes, but your container still runs an old riff: riff starts it \
                 again on the riff of the daemon at the end of your turn. next: riff release \
                 check v3.3.0"
            ]
        );
        assert_eq!(ground.leads().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// A deploy that passed, with this host on the release and its lead on
    /// the riff of the daemon: only the other hosts make it wait.
    fn passed_on_this_host(ground: &Ground) {
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Passed, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        ground.riffs(|r| {
            r.daemon = Some("3.3.0".parse().unwrap());
            r.auto = true;
            r.leads = BTreeMap::from([(REPO.to_owned(), false)]);
        });
    }

    /// 01M6WFNDS76NESPXHMQPGHHQSS: with two hosts, the message of a deploy
    /// that passed waits until the daemon of the other host runs the
    /// release too.
    #[tokio::test]
    async fn a_deploy_that_passed_waits_until_each_host_runs_the_release() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        passed_on_this_host(&ground);
        ground.host("bob@pangolin", "3.2.0");
        watch(dir.path(), deploy_op(None));

        look(dir.path(), &ground, MINUTE).await;
        look(dir.path(), &ground, 10 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert_eq!(kept(dir.path()), [deploy_op(None)]);

        // The daemon of the other host starts again on the release.
        ground.host("bob@pangolin", "3.3.0");
        look(dir.path(), &ground, 11 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. Your \
                 container runs v3.3.0, as the riff daemon of this host does. next: riff \
                 release check v3.3.0"
            ]
        );
        assert_eq!(ground.leads().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    /// 01M6WFNDS76NESPXHMQPGHHQSS: after 30 minutes the message goes all
    /// the same, and names the host that still runs the old riff.
    #[tokio::test]
    async fn a_deploy_that_passed_names_after_30_minutes_the_host_on_the_old_riff() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        passed_on_this_host(&ground);
        ground.host("bob@pangolin", "3.2.0");
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        look(dir.path(), &ground, 30 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());

        look(dir.path(), &ground, 31 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. riff \
                 waited 30 minutes, but the riff daemon of bob@pangolin still runs v3.2.0, not \
                 v3.3.0. next: riff release check v3.3.0"
            ]
        );
        assert_eq!(ground.leads().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(true)]);
    }

    #[tokio::test]
    async fn a_deploy_that_fails_tells_the_failed_jobs_once() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Failed, &["Crate audit", "Deploy"]);
            f.tags = vec!["v3.3.0".into()];
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The deploy of v3.3.0 failed: Crate audit, Deploy: \
                 https://github.com/o/r/actions/runs/9. The old revision still serves. next: \
                 riff release retry v3.3.0, or riff release rollback v3.2.0"]
        );
        assert!(ground.chats().is_empty());
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::DeployFailed,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert_eq!(kept(dir.path()), [check_op(false)]);
    }

    #[tokio::test]
    async fn a_retry_ignores_the_failed_state_that_it_saw_until_the_run_moves() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Failed, &["Deploy"]);
            f.tags = vec!["v3.3.0".into()];
        });
        let stale = Stale {
            run: 9,
            state: DeployState::Failed,
        };
        watch(dir.path(), deploy_op(Some(stale)));

        // GitHub still shows the old failure.
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());

        ground.forge(|f| f.deploy = deploy(DeployState::Running, &[]));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());

        // The run fails again: this one is news.
        ground.forge(|f| f.deploy = deploy(DeployState::Failed, &["Deploy"]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(false)]);
    }

    #[tokio::test]
    async fn a_rollback_ignores_the_old_run_of_the_older_release() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Passed, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        let stale = Stale {
            run: 9,
            state: DeployState::Passed,
        };
        watch(dir.path(), deploy_op(Some(stale)));
        look(dir.path(), &ground, MINUTE).await;
        assert!(
            ground.told().is_empty(),
            "the run of the old release passed long ago"
        );

        // The run by hand has a new ID and waits.
        ground.forge(|f| {
            f.deploy = Some(Deploy {
                run: 10,
                state: DeployState::Waiting,
                ..deploy(DeployState::Waiting, &[]).unwrap()
            });
        });
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    /// The step of each watch in the file, as the facts of the host carry
    /// it (01M6XTS7RGXB30MFM6DZ7PVZ75).
    fn steps(dir: &Path) -> Vec<ReleaseStep> {
        let watches = Watches::new(dir).read().unwrap();
        watches
            .iter()
            .filter_map(Watch::release)
            .map(|r| r.step)
            .collect()
    }

    /// The release issue of v3.3.0, open, with a comment for each of
    /// `results` of `riff release check`.
    fn issue_with(open: bool, results: &[&str]) -> Option<crate::cycle::ReleaseIssue> {
        Some(crate::cycle::ReleaseIssue {
            number: 7,
            open,
            comments: results
                .iter()
                .map(|r| format!("{}\n\n- a: x\n", cycle::check_mark("v3.3.0", r)))
                .collect(),
        })
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: the watch of a deploy gives the step of
    /// the release at each look: it waits for a person, it runs, it passed.
    /// At its end the watch of the check takes its place: it posts nothing
    /// and fetches nothing, gives the failed check, and ends with the check
    /// that passed.
    #[tokio::test]
    async fn the_watches_give_each_step_of_a_release_up_to_the_check_that_passed() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.tags = vec!["v3.3.0".into()]);
        watch(dir.path(), deploy_op(None));
        // The CI run builds: no step yet.
        ground.forge(|f| f.deploy = deploy(DeployState::Before, &[]));
        look(dir.path(), &ground, MINUTE).await;
        assert!(steps(dir.path()).is_empty());

        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Waits]);
        ground.forge(|f| f.deploy = deploy(DeployState::Running, &[]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Runs]);

        // The job passed, and the message waits for the new riff.
        ground.forge(|f| f.deploy = deploy(DeployState::Passed, &[]));
        ground.riffs(|r| {
            r.daemon = Some("3.2.0".parse().unwrap());
            r.auto = true;
        });
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        assert_eq!(kept(dir.path()), [deploy_op(None)]);
        ground.told();

        // The message goes: the watch of the check stays at the same step.
        ground.riffs(|r| r.daemon = Some("3.3.0".parse().unwrap()));
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert_eq!(kept(dir.path()), [check_op(true)]);
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        ground.fetches.lock().unwrap().clear();
        ground.leads();
        ground.chats();
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        assert_eq!(*ground.fetches.lock().unwrap(), [false], "only gh");

        // A check could not run, then a check failed, then one passed.
        ground.forge(|f| f.issue = issue_with(true, &["unknown"]));
        look(dir.path(), &ground, 7 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        ground.forge(|f| f.issue = issue_with(true, &["unknown", "failed"]));
        look(dir.path(), &ground, 8 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::CheckFailed]);
        look(dir.path(), &ground, 9 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::CheckFailed]);
        ground.forge(|f| f.issue = issue_with(true, &["failed", "passed"]));
        look(dir.path(), &ground, 10 * MINUTE).await;
        assert!(kept(dir.path()).is_empty(), "the watch of the check goes");
        assert!(ground.told().is_empty() && ground.leads().is_empty() && ground.chats().is_empty());
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: after a failed deploy, the watch of the
    /// check gives the step until the check of the lead files the issue.
    #[tokio::test]
    async fn the_watch_of_the_check_of_a_failed_deploy_ends_with_the_check() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.tags = vec!["v3.3.0".into()];
            f.deploy = deploy(DeployState::Failed, &["Deploy"]);
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(kept(dir.path()), [check_op(false)]);
        assert_eq!(steps(dir.path()), [ReleaseStep::DeployFailed]);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::DeployFailed]);
        ground.forge(|f| f.issue = issue_with(true, &["failed deploy"]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(kept(dir.path()).is_empty());
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: the watch of a check ends at a newer
    /// release, at a rollback, at a release issue that closed, and at a new
    /// watch of a deploy of the repository: a retry, or the deploy of an
    /// older release.
    #[tokio::test]
    async fn the_watch_of_a_check_ends_when_another_deploy_follows() {
        let ends: [Ends; 3] = [
            (Box::new(|f| f.releases.push("3.3.1")), "a newer release"),
            (
                Box::new(|f| {
                    f.issue = issue_with(true, &["failed"]);
                    let rollback = format!(
                        "{}\n\nThe deploy of v3.2.0 ...",
                        cycle::rollback_mark("v3.2.0")
                    );
                    f.issue.as_mut().unwrap().comments.push(rollback);
                }),
                "a rollback",
            ),
            (
                Box::new(|f| f.issue = issue_with(false, &[])),
                "a closed issue",
            ),
        ];
        for (end, name) in ends {
            let dir = daemon_dir();
            let ground = Ground::new(Fake::ready("3.3.0"));
            ground.forge(|f| {
                f.tags = vec!["v3.3.0".into()];
                f.deploy = deploy(DeployState::Passed, &[]);
            });
            watch(dir.path(), deploy_op(None));
            look(dir.path(), &ground, MINUTE).await;
            ground.forge(|f| f.issue = issue_with(true, &["failed"]));
            look(dir.path(), &ground, 2 * MINUTE).await;
            assert_eq!(steps(dir.path()), [ReleaseStep::CheckFailed], "{name}");
            ground.forge(|f| end(f));
            look(dir.path(), &ground, 3 * MINUTE).await;
            assert!(kept(dir.path()).is_empty(), "{name}");
        }

        // A retry of a failed deploy follows its own run.
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.tags = vec!["v3.3.0".into()];
            f.deploy = deploy(DeployState::Failed, &["Deploy"]);
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(kept(dir.path()), [check_op(false)]);
        let stale = Stale {
            run: 9,
            state: DeployState::Failed,
        };
        watch(dir.path(), deploy_op(Some(stale)));
        assert_eq!(kept(dir.path()), [deploy_op(Some(stale))]);
        assert!(
            steps(dir.path()).is_empty(),
            "GitHub still shows the failure"
        );
        // A watch of another operation keeps the watch of the check.
        let dir = daemon_dir();
        watch(dir.path(), check_op(true));
        watch(dir.path(), pull());
        assert_eq!(kept(dir.path()), [pull(), check_op(true)]);
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: the watch of a check is in its own file.
    /// [`FILE`] holds only the kinds of watch that an older riff knows, so
    /// its daemon still reads its watches after a rollback of riff.
    #[tokio::test]
    async fn the_watch_of_a_check_stays_out_of_the_file_of_an_older_riff() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.tags = vec!["v3.3.0".into()];
            f.deploy = deploy(DeployState::Passed, &[]);
        });
        watch(dir.path(), deploy_op(None));
        watch(dir.path(), pull());
        look(dir.path(), &ground, MINUTE).await;
        let kinds = |file: &str| -> Vec<String> {
            let text = std::fs::read_to_string(dir.path().join(file)).unwrap();
            let all: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
            all.iter().map(|w| w["op"]["kind"].to_string()).collect()
        };
        assert_eq!(kinds(FILE), [r#""pull""#, r#""wave""#]);
        assert_eq!(kinds(CHECKS), [r#""check""#]);
        assert_eq!(kept(dir.path()), [pull(), check_op(true)]);
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: after a retry, the release issue holds
    /// the check of the failed deploy from before. Only a later check ends
    /// the watch of the check of the deploy that passed.
    #[tokio::test]
    async fn the_watch_of_a_check_after_a_retry_waits_for_a_new_check() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.tags = vec!["v3.3.0".into()];
            f.deploy = deploy(DeployState::Passed, &[]);
            f.issue = issue_with(true, &["failed deploy"]);
        });
        let stale = Stale {
            run: 9,
            state: DeployState::Failed,
        };
        watch(dir.path(), deploy_op(Some(stale)));
        look(dir.path(), &ground, MINUTE).await;
        let check = Op::Check {
            tag: "v3.3.0".into(),
            deployed: true,
            comments: 1,
            newer: 0,
        };
        assert_eq!(kept(dir.path()), [check]);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        ground.forge(|f| f.issue = issue_with(true, &["failed deploy", "passed"]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(kept(dir.path()).is_empty());
    }

    /// 01M6XTS7RGXB30MFM6DZ7PVZ75: after the deploy of an older release, the
    /// newer release and the closed issue of the older release are from
    /// before. Only the check of the older release ends the watch.
    #[tokio::test]
    async fn the_watch_of_a_check_after_a_rollback_waits_for_the_check() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.releases = vec!["3.0.0", "3.2.0", "3.3.0"];
            f.tags = vec!["v3.2.0".into(), "v3.3.0".into()];
            f.deploy = deploy(DeployState::Passed, &[]);
            f.issue = Some(crate::cycle::ReleaseIssue {
                number: 6,
                open: false,
                comments: vec![cycle::check_mark("v3.2.0", "passed")],
            });
        });
        let older = Op::Deploy {
            tag: "v3.2.0".into(),
            stale: None,
        };
        watch(dir.path(), older);
        look(dir.path(), &ground, MINUTE).await;
        look(dir.path(), &ground, 2 * MINUTE).await;
        let check = Op::Check {
            tag: "v3.2.0".into(),
            deployed: true,
            comments: 1,
            newer: 1,
        };
        assert_eq!(kept(dir.path()), [check]);
        assert_eq!(steps(dir.path()), [ReleaseStep::Passed]);
        ground.forge(|f| {
            let issue = f.issue.as_mut().unwrap();
            issue.comments.push(cycle::check_mark("v3.2.0", "passed"));
        });
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn the_wave_tells_the_lead_when_its_last_open_item_closes() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        ground.forge(|f| {
            f.wave_open = Some(("Wave 41: only the current release".into(), 2));
            f.has_release = true;
        });
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        assert_eq!(Watches::new(dir.path()).read().unwrap()[0].op, Op::Wave);

        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 1);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());

        // A new start of the daemon between the two looks keeps the watch.
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "Wave 41 is complete: next riff release test vX.Y.Z --host HOST, then riff \
                 release cut X.Y.Z, or riff wave end 41 --no-release TEXT"
            ]
        );
        // The watch of each host makes this key, and riff-server keeps one post
        // (01M6FZY6NABXGXVRYF6W1509XE).
        assert_eq!(ground.keys(), [Some("wave-complete-41".to_owned())]);
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert!(ground.told().is_empty(), "once");

        // A new item, then its close: the watch tells again, with the same key.
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 1);
        look(dir.path(), &ground, 5 * MINUTE).await;
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    /// 01M75N6YNHQQHGS0VGKTKSNJTR: while the current wave has no open item
    /// and no release issue, the watch of the wave keeps it, with why the cut
    /// waits: the full check of `main` runs, failed, or passed. The release
    /// issue of the cut ends it, and so does a new item.
    #[tokio::test]
    async fn the_watch_of_a_wave_keeps_the_complete_wave_until_the_cut() {
        use riff_client::daemons::{Complete, CutWaits};
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        let complete = || {
            let watches = Watches::new(dir.path()).read().unwrap();
            let wave = watches.iter().find(|w| w.op == Op::Wave).unwrap();
            wave.seen.complete.clone()
        };
        let of = |waits| Complete {
            repo: REPO.into(),
            wave: 41,
            waits,
        };
        ground.forge(|f| {
            f.wave_open = Some(("Wave 41: only the current release".into(), 1));
            f.has_release = true;
            f.gate = Gate::Running;
        });
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(complete(), None);

        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(complete(), Some(of(Some(CutWaits::MainRuns))));
        ground.forge(|f| f.gate = Gate::Ended("failure".into()));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(complete(), Some(of(Some(CutWaits::MainFailed))));
        ground.forge(|f| f.gate = Gate::Ended("cancelled".into()));
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert_eq!(complete(), Some(of(None)));
        ground.forge(|f| f.gate = Gate::Passed);
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert_eq!(complete(), Some(of(Some(CutWaits::Lead))));

        // The cut makes the release issue of the wave.
        ground.forge(|f| f.wave_release = Some("Release v3.3.0".into()));
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert_eq!(complete(), None);
        // A new item of the wave.
        ground.forge(|f| {
            f.wave_release = None;
            f.wave_open.as_mut().unwrap().1 = 1;
        });
        look(dir.path(), &ground, 7 * MINUTE).await;
        assert_eq!(complete(), None);
        // A repository with no table `[release]` has no release to cut.
        ground.forge(|f| {
            f.has_release = false;
            f.wave_open.as_mut().unwrap().1 = 0;
        });
        look(dir.path(), &ground, 8 * MINUTE).await;
        assert_eq!(complete(), None);
    }

    /// The step of the release of the last read of the watch of the wave.
    fn wave_step(dir: &Path) -> Option<ReleaseStep> {
        let watches = Watches::new(dir).read().unwrap();
        let wave = watches.iter().find(|w| w.op == Op::Wave).unwrap();
        wave.release().map(|r| r.step)
    }

    /// A forge with the current wave 41 of a repository with the table
    /// `[release]`, no open item, and the release issue of v3.3.0 that
    /// `riff release test` made. The last commit c1 of origin/main has
    /// 3.3.0, and its release test runs.
    fn cut_forge() -> Fake {
        Fake {
            wave_open: Some(("Wave 41: only the current release".into(), 0)),
            has_release: true,
            wave_release: Some("Release v3.3.0".into()),
            main: "3.3.0".into(),
            release_test: Some(TestState::Running),
            ..open_pull()
        }
    }

    /// 01M75R6FPVZA3JA5C48PQ26A85, 01M7642H27T4862WZENFAN4277: from the
    /// release issue to the check, the watch of the wave reads the release
    /// of the newest release issue of the wave at each step, as `riff
    /// release status` does, and gives it to the facts of its host. The
    /// state `building` and a checked release have no step.
    #[tokio::test]
    async fn the_watch_of_a_wave_reads_each_step_of_the_release_from_its_issue() {
        let dir = daemon_dir();
        let ground = Ground::new(cut_forge());
        let every = i64::try_from(read_every().as_millis()).unwrap();
        // A change of the forge, then the step of the next read.
        type Step = (fn(&mut Fake), Option<ReleaseStep>);
        let steps: [Step; 12] = [
            (|_| {}, None),
            (
                |f| f.release_test = Some(TestState::Passed),
                Some(ReleaseStep::Merged),
            ),
            (|f| f.running = vec!["c1"], Some(ReleaseStep::TestRuns)),
            (
                |f| {
                    f.running = vec![];
                    f.real = vec![("c1", false)];
                },
                Some(ReleaseStep::TestFailed),
            ),
            (|f| f.real = vec![("c1", true)], Some(ReleaseStep::Tested)),
            // The cut, then the merge of the pull request of the next
            // version: the read follows v3.3.0.
            (
                |f| {
                    f.tags = vec!["v3.3.0".into()];
                    f.releases = vec!["3.0.0", "3.2.0", "3.3.0"];
                    f.main = "3.4.0".into();
                },
                Some(ReleaseStep::Published),
            ),
            (
                |f| f.deploy = deploy(DeployState::Waiting, &[]),
                Some(ReleaseStep::Waits),
            ),
            (
                |f| f.deploy = deploy(DeployState::Running, &[]),
                Some(ReleaseStep::Runs),
            ),
            (
                |f| f.deploy = deploy(DeployState::Passed, &[]),
                Some(ReleaseStep::Passed),
            ),
            (
                |f| f.issue = issue_with(true, &["failed"]),
                Some(ReleaseStep::CheckFailed),
            ),
            (
                |f| {
                    f.issue = issue_with(true, &[]);
                    f.deploy = deploy(DeployState::Failed, &["Deploy"]);
                },
                Some(ReleaseStep::DeployFailed),
            ),
            (|f| f.issue = issue_with(false, &[]), None),
        ];
        for (n, (change, step)) in (0..).zip(steps) {
            ground.forge(change);
            let now = MINUTE + n * every;
            look(dir.path(), &ground, now).await;
            assert_eq!(wave_step(dir.path()), step, "at {now}");
        }
        let watches = Watches::new(dir.path()).read().unwrap();
        let read = watches[0].seen.release.as_ref().unwrap();
        assert_eq!(read.issue, "Release v3.3.0");
        // The wave has a release issue: it is not complete.
        assert_eq!(watches[0].seen.complete, None);
        assert!(ground.told().is_empty());
    }

    /// 01M75R6FPVZA3JA5C48PQ26A85: the watch of the wave reads the release
    /// again only each `read_every`, and at once for a new release issue,
    /// for example the test of a fix forward. The read of a release before
    /// its tag, and a read with no release, fetch the main clone first, and
    /// a read that fails keeps the last read.
    #[tokio::test]
    async fn the_watch_of_a_wave_reads_the_release_each_few_minutes() {
        let dir = daemon_dir();
        let ground = Ground::new(cut_forge());
        ground.forge(|f| f.release_test = Some(TestState::Passed));
        let every = i64::try_from(read_every().as_millis()).unwrap();
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(wave_step(dir.path()), Some(ReleaseStep::Merged));
        // The first read of a new release issue fetches nothing.
        assert_eq!(*ground.fetches.lock().unwrap(), [false]);

        ground.forge(|f| f.running = vec!["c1"]);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(
            wave_step(dir.path()),
            Some(ReleaseStep::Merged),
            "no read yet"
        );
        look(dir.path(), &ground, MINUTE + every).await;
        assert_eq!(wave_step(dir.path()), Some(ReleaseStep::TestRuns));
        assert_eq!(*ground.fetches.lock().unwrap(), [false, false, true]);

        // A fix forward: the test of v3.4.0 makes a new release issue.
        ground.forge(|f| {
            f.wave_release = Some("Release v3.4.0".into());
            f.main = "3.4.0".into();
            f.commit = "c2";
        });
        look(dir.path(), &ground, MINUTE + every + MINUTE).await;
        let watches = Watches::new(dir.path()).read().unwrap();
        let release = watches[0].release().unwrap();
        assert_eq!(
            (release.tag.as_str(), release.step),
            ("v3.4.0", ReleaseStep::Merged)
        );

        // A read that fails keeps the last read.
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());
        look(dir.path(), &ground, 3 * every).await;
        let watches = Watches::new(dir.path()).read().unwrap();
        assert_eq!(watches[0].release().unwrap().tag, "v3.4.0");

        // After the tag, the read needs no fetch.
        *ground.fault.lock().unwrap() = None;
        ground.forge(|f| f.tags = vec!["v3.4.0".into()]);
        look(dir.path(), &ground, 4 * every).await;
        assert_eq!(wave_step(dir.path()), Some(ReleaseStep::Published));
        ground.fetches.lock().unwrap().clear();
        look(dir.path(), &ground, 5 * every).await;
        assert_eq!(*ground.fetches.lock().unwrap(), [false]);
    }

    #[tokio::test]
    async fn the_wave_of_a_repository_with_no_release_table_ends_the_wave() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        ground.forge(|f| f.wave_open = Some(("Wave 3".into(), 1)));
        look(dir.path(), &ground, MINUTE).await;
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told(), ["Wave 3 is complete: next riff wave end 3"]);
    }

    /// The watch of a wave needs only `gh`: it does not fetch the main
    /// clone of the person each minute.
    #[tokio::test]
    async fn only_a_watch_of_an_operation_fetches_the_main_clone() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(*ground.fetches.lock().unwrap(), [false]);
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(*ground.fetches.lock().unwrap(), [false, true]);
    }

    /// 01M6GYN9526HAYVKJC4HS858TH (#1549): a look says it is slow while the
    /// watch of a wave is the only watch. The daemon then waits 5 times as
    /// long for the next look.
    #[tokio::test]
    async fn the_look_is_slow_while_the_watch_of_a_wave_is_the_only_watch() {
        let dir = daemon_dir();
        let ground: Arc<dyn Lookout> = Ground::new(open_pull());
        let watches = Watches::new(dir.path());
        assert!(
            watches
                .look_all(&ground, MINUTE, &Riffs::default())
                .await
                .unwrap()
        );
        watch(dir.path(), pull());
        assert!(
            !watches
                .look_all(&ground, 2 * MINUTE, &Riffs::default())
                .await
                .unwrap()
        );
    }

    /// 01M6GYN9526HAYVKJC4HS858TH (#1549): a slow wait ends when a command
    /// registers a watch, so the new watch gets its first look in one
    /// step, not after the whole wait.
    #[tokio::test(start_paused = true)]
    async fn a_new_watch_ends_the_slow_wait() {
        let dir = daemon_dir();
        let ground: Arc<dyn Lookout> = Ground::new(open_pull());
        let watches = Watches::new(dir.path());
        assert!(
            watches
                .look_all(&ground, MINUTE, &Riffs::default())
                .await
                .unwrap()
        );

        let every = Duration::from_millis(200);
        let registers = {
            let dir = dir.path().to_owned();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                watch(&dir, pull());
            })
        };
        let start = tokio::time::Instant::now();
        watches.pause(every, true).await;
        // The watch came in the second step: the wait ends at 400 ms, not
        // at 1,000 ms.
        assert_eq!(start.elapsed(), Duration::from_millis(400));
        registers.await.unwrap();

        // With no change in the file, a slow wait lasts 5 steps, and a
        // quick wait one.
        let start = tokio::time::Instant::now();
        watches.pause(every, true).await;
        assert_eq!(start.elapsed(), 5 * every);
        let start = tokio::time::Instant::now();
        watches.pause(every, false).await;
        assert_eq!(start.elapsed(), every);
    }

    #[tokio::test]
    async fn a_wave_that_the_daemon_stopped_loses_its_watch() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(Watches::new(dir.path()).read().unwrap().len(), 1);
        crate::wave::write_waves(dir.path(), &BTreeMap::new()).unwrap();
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(Watches::new(dir.path()).read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_forge_that_fails_tells_the_lead_the_fault_once() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());

        look(dir.path(), &ground, MINUTE).await;
        // Two watches of the repository, one fault: one message, with the
        // name of each watch.
        assert_eq!(
            ground.told(),
            [
                "riff cannot read the state of the pull request #1140 of the release and the \
                 current wave: gh: HTTP 502. riff tries again each minute, and tells you this \
                 fault one time."
            ]
        );
        assert_eq!(ground.keys(), [Some(watch_fault_key("gh: HTTP 502"))]);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty(), "the same fault: one message");
        assert_eq!(kept(dir.path()), [pull()], "the watch goes on");

        // The forge answers again: the watch ends as it does with no fault.
        *ground.fault.lock().unwrap() = None;
        ground.forge(|f| f.pull.state = "CLOSED".into());
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty());
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1557): while a limit of GitHub is out, a
    /// look makes no call, and a fault that comes from the limit goes to the
    /// lead of the wave and not of the watch. After the reset the look goes
    /// on.
    #[tokio::test]
    async fn a_limit_of_github_stops_the_look_and_hides_its_fault() {
        let wait =
            crate::ghcalls::Wait::until(crate::ghcalls::Resource::Graphql, 1_793_718_035_000);
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());
        *ground.fault.lock().unwrap() = Some("gh: API rate limit already exceeded".into());

        // The limit stands: the look asks for no forge.
        *ground.limit.lock().unwrap() = Some((wait, false));
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.fetches.lock().unwrap().is_empty(), "no call of gh");
        assert!(ground.told().is_empty());

        // The limit starts in the look: the call fails, and nobody tells
        // the fault.
        *ground.limit.lock().unwrap() = Some((wait, true));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(!ground.fetches.lock().unwrap().is_empty());
        assert!(ground.told().is_empty(), "the wave tells the limit");
        assert_eq!(kept(dir.path()), [pull()], "the watch goes on");

        // The limit is over: a fault is a fault again.
        *ground.limit.lock().unwrap() = None;
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1, "one message for both watches");
    }

    #[tokio::test]
    async fn a_watch_that_starts_in_a_known_fault_tells_nothing() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(ground.told().len(), 1, "the watch of the wave");

        // A command registers a second watch: it fails with the same fault.
        watch(dir.path(), pull());
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty(), "one fault, one message");
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty());
        let all = Watches::new(dir.path()).read().unwrap();
        assert!(
            all.iter()
                .all(|w| w.seen.fault.as_deref() == Some("gh: HTTP 502"))
        );

        // A new fault is a new message.
        *ground.fault.lock().unwrap() = Some("gh: HTTP 503".into());
        look(dir.path(), &ground, 4 * MINUTE).await;
        let told = ground.told();
        assert_eq!(told.len(), 1);
        assert!(told[0].contains("gh: HTTP 503"), "{told:?}");
    }

    #[tokio::test]
    async fn a_watch_that_lost_what_it_saw_sends_the_fault_with_the_same_key() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());
        look(dir.path(), &ground, MINUTE).await;
        let first = ground.keys();
        assert_eq!((first.len(), ground.told().len()), (1, 1));

        // The forge answers one time, and fails again: the watch starts
        // from nothing, and riff-server drops the second post by its key.
        *ground.fault.lock().unwrap() = None;
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert_eq!(ground.keys(), first);
    }

    #[tokio::test]
    async fn a_fault_that_ends_with_a_dot_has_one_dot() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        *ground.fault.lock().unwrap() = Some(
            "gh issue list --state open: GraphQL: API rate limit already exceeded for user ID 1."
                .into(),
        );
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "riff cannot read the state of the current wave: gh issue list --state open: \
                 GraphQL: API rate limit already exceeded for user ID 1. riff tries again each \
                 minute, and tells you this fault one time."
            ]
        );
    }

    #[tokio::test]
    async fn a_fault_that_does_not_go_out_stays_for_the_next_look() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());

        ground.down.store(true, Ordering::SeqCst);
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        let all = Watches::new(dir.path()).read().unwrap();
        assert!(all.iter().all(|w| w.seen.fault.is_none()), "{all:?}");

        ground.down.store(false, Ordering::SeqCst);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    #[tokio::test]
    async fn a_message_that_does_not_go_out_stays_for_the_next_look() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| f.pull.state = "CLOSED".into());
        watch(dir.path(), pull());

        ground.down.store(true, Ordering::SeqCst);
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        assert_eq!(kept(dir.path()), [pull()], "the end is not lost");

        ground.down.store(false, Ordering::SeqCst);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty());
    }

    #[test]
    fn a_second_registration_of_the_same_operation_changes_nothing() {
        let dir = daemon_dir();
        let watches = Watches::new(dir.path());
        assert_eq!(watches.register(REPO, pull(), 5).unwrap(), Registered::New);
        assert_eq!(watches.register(REPO, pull(), 9).unwrap(), Registered::Kept);
        let all = watches.read().unwrap();
        assert_eq!((all.len(), all[0].since_ms), (1, 5));
        let other = Op::Pull {
            tag: "v3.3.0".into(),
            number: 1141,
        };
        assert_eq!(watches.register(REPO, other, 9).unwrap(), Registered::New);
    }

    #[test]
    fn a_repository_that_the_daemon_does_not_know_gets_no_watch() {
        let dir = daemon_dir();
        let error = Watches::new(dir.path())
            .register("o/other", pull(), 0)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("does not serve o/other"),
            "{error:#}"
        );
        assert!(Watches::new(dir.path()).read().unwrap().is_empty());
    }

    #[test]
    fn a_repository_that_the_daemon_serves_only_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let wave = Wave {
            repo: REPO.into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        remote::serve_in_file(dir.path(), &wave).unwrap();
        assert_eq!(wave_of(dir.path(), REPO).unwrap(), wave);
        assert!(
            Watches::new(dir.path())
                .register(REPO, test_op(), 0)
                .is_ok()
        );
    }

    #[test]
    fn the_file_keeps_a_watch_that_an_old_riff_wrote_with_less() {
        let dir = daemon_dir();
        let text = r#"[{"repo":"o/r","op":{"kind":"deploy","tag":"v3.3.0"},"since_ms":7}]"#;
        std::fs::write(dir.path().join(FILE), text).unwrap();
        let all = Watches::new(dir.path()).read().unwrap();
        assert_eq!(all[0].op, deploy_op(None));
        assert_eq!(all[0].seen, Seen::default());
    }
    /// Waits until `done` holds: the daemon looks each 20 ms.
    async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..250 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the daemon did not do it in 5 seconds");
    }

    /// A daemon on `dir` that looks each 20 ms through `ground`.
    fn daemon(
        dir: &Path,
        ground: &Arc<Ground>,
    ) -> (
        Arc<crate::daemon::Daemon<crate::runtime::Fake>>,
        JoinHandle<()>,
    ) {
        let lookout: Arc<dyn Lookout> = ground.clone();
        let daemon = crate::daemon::Daemon::new(crate::runtime::Fake::default(), dir.to_owned())
            .unwrap()
            .with_lookout(lookout)
            .with_auto(false)
            .with_own_replaced(Arc::new(|| false));
        let daemon = Arc::new(daemon);
        let task = daemon.watch_every(Duration::from_millis(20));
        (daemon, task)
    }

    /// A change of the fake forge, and the words of the message of the lead.
    type Ends = (Box<dyn Fn(&mut Fake)>, &'static str);

    /// What a command starts, and how its operation ends.
    struct Case {
        command: &'static str,
        op: Op,
        /// The forge when the operation succeeds, and the words of the
        /// message of the lead.
        ok: Ends,
        /// The same for a failure.
        bad: Ends,
        /// True when the watch stays after the news: a deploy that waits.
        stays_on_ok: bool,
    }

    fn cases() -> Vec<Case> {
        let tag = || "v3.3.0".to_owned();
        let wait = |f: &mut Fake| f.deploy = deploy(DeployState::Waiting, &[]);
        let stale = |state| Some(Stale { run: 9, state });
        let pass = |f: &mut Fake| f.deploy = deploy(DeployState::Passed, &[]);
        let fail = |f: &mut Fake| f.deploy = deploy(DeployState::Failed, &["Deploy"]);
        vec![
            // A failed Gate keeps the watch of the pull request: see
            // a_gate_that_fails_tells_the_failed_jobs_once_and_the_watch_stays.
            Case {
                command: "riff release cut",
                op: pull(),
                ok: (Box::new(merged), "of the release v3.3.0 merged"),
                bad: (
                    Box::new(|f| f.pull.state = "CLOSED".into()),
                    "closed with no merge",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release test",
                op: test_op(),
                ok: (
                    Box::new(|f| {
                        f.running.clear();
                        f.real = vec![("c1", true)];
                    }),
                    "passed on c1",
                ),
                bad: (
                    Box::new(|f| {
                        f.running.clear();
                        f.real = vec![("c1", false)];
                    }),
                    "failed on c1",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release cut",
                op: Op::Deploy {
                    tag: tag(),
                    stale: None,
                },
                ok: (Box::new(wait), "waits for a person"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: true,
            },
            Case {
                command: "riff release approve",
                op: Op::Deploy {
                    tag: tag(),
                    stale: stale(DeployState::Waiting),
                },
                ok: (Box::new(pass), "passed"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: false,
            },
            Case {
                command: "riff release retry",
                op: Op::Deploy {
                    tag: tag(),
                    stale: stale(DeployState::Failed),
                },
                ok: (Box::new(pass), "passed"),
                bad: (
                    // The run moved and failed again.
                    Box::new(|f| {
                        f.deploy = Some(Deploy {
                            run: 10,
                            ..deploy(DeployState::Failed, &["Deploy"]).unwrap()
                        });
                    }),
                    "failed: Deploy",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release rollback",
                op: Op::Deploy {
                    tag: "v3.2.0".into(),
                    stale: None,
                },
                ok: (Box::new(pass), "passed"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: false,
            },
        ]
    }

    /// 01M5RGCW5NCKQYA61RM63914TX: for each command that starts an async
    /// operation, the daemon follows the watch that the command
    /// registered. A new daemon between the start and the end keeps the
    /// watch. The lead gets one message at the end, with a success and
    /// with a failure, and no second message.
    #[tokio::test]
    async fn the_daemon_wakes_the_lead_once_at_the_end_of_each_operation() {
        for ok in [true, false] {
            for case in cases() {
                let name = format!("{} {}", case.command, if ok { "ok" } else { "bad" });
                let dir = daemon_dir();
                let mut forge = open_pull();
                forge.main = "3.3.0".into();
                forge.running = vec!["c1"];
                forge.tags = vec!["v3.3.0".into(), "v3.2.0".into()];
                forge.deploy = match &case.op {
                    // The state that the command saw before.
                    Op::Deploy { stale: Some(s), .. } => deploy(s.state, &["Deploy"]),
                    _ => deploy(DeployState::Before, &[]),
                };
                let ground = Ground::new(forge);
                Watches::new(dir.path())
                    .register(REPO, case.op.clone(), 0)
                    .unwrap();

                // The operation runs, and the daemon starts again.
                let (first, task) = daemon(dir.path(), &ground);
                tokio::time::sleep(Duration::from_millis(100)).await;
                assert!(ground.told().is_empty(), "{name}: it runs");
                task.abort();
                drop(first);
                let (_second, _task) = daemon(dir.path(), &ground);

                let (change, words) = if ok { &case.ok } else { &case.bad };
                ground.forge(|f| change(f));
                until(|| !ground.told.lock().unwrap().is_empty()).await;
                // Some looks more: no second message.
                tokio::time::sleep(Duration::from_millis(150)).await;
                let told = ground.told();
                assert_eq!(told.len(), 1, "{name}: {told:?}");
                assert!(told[0].contains(words), "{name}: {told:?}");
                let stays = ok && case.stays_on_ok;
                // The end of a deploy leaves the watch of its check
                // (01M6XTS7RGXB30MFM6DZ7PVZ75).
                let (checks, others): (Vec<Op>, Vec<Op>) = kept(dir.path())
                    .into_iter()
                    .partition(|op| matches!(op, Op::Check { .. }));
                assert_eq!(others.len(), usize::from(stays), "{name}");
                match &case.op {
                    Op::Deploy { tag, .. } if !stays => {
                        let check = Op::Check {
                            tag: tag.clone(),
                            deployed: ok,
                            comments: 0,
                            newer: 0,
                        };
                        assert_eq!(checks, [check], "{name}");
                    }
                    _ => assert!(checks.is_empty(), "{name}"),
                }
            }
        }
    }

    /// A daemon that starts again learns what a watch told, from the file.
    #[tokio::test]
    async fn a_new_daemon_does_not_tell_again_what_the_watch_told() {
        let dir = daemon_dir();
        let mut forge = Fake::ready("3.3.0");
        forge.deploy = deploy(DeployState::Waiting, &[]);
        let ground = Ground::new(forge);
        watch(dir.path(), deploy_op(None));
        let (first, task) = daemon(dir.path(), &ground);
        until(|| !ground.told.lock().unwrap().is_empty()).await;
        task.abort();
        drop(first);
        let (_second, _task) = daemon(dir.path(), &ground);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            ground.told().len(),
            1,
            "the waiting deploy is told one time"
        );
        assert_eq!(ground.chats().len(), 1);
    }
}
