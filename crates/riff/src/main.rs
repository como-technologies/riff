//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::TimeZone;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use futures::StreamExt;
use riff::reach::Reach;
use riff::{
    audit, binary, claude_settings, cloud, dropped, help, identity, lifecycle, local, login,
    permissions, pr, settings, text, usage, view,
};
use riff_client::api::{self, Api, DEFAULT_SERVER, PauseScope, follow};
use riff_client::link::STREAM_RETRY;
use riff_client::sessions::Sessions;

use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, PullState, RiffState, Status, StepChange};

/// The local client that finds sessions and wakes yours.
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about)]
struct Cli {
    // `riff help server` shows the long text (01M3NT228WA11PGNWDJ0WP7PQD).
    // riff reads RIFF_SERVER itself, not with the `env` of clap: so a
    // usage error does not name --server (01M3Q5VE4VVXT9FH4J4MAWX68V).
    /// The riff-server (default: RIFF_SERVER, else this machine)
    #[arg(long, global = true, value_parser = api::server_url)]
    server: Option<String>,

    // One option of each command (01M3Q5VE2D244XDZRYXM8DNSRS).
    /// When to use color. `auto` uses color only when stdout is a
    /// terminal, and obeys NO_COLOR and CLICOLOR_FORCE.
    #[arg(long, global = true, value_enum, default_value_t = ColorWhen::Auto)]
    color: ColorWhen,

    /// The place of the process that ran this binary after an update
    /// (01M3NJGD45GF7Y4CZWQ7GRDHZN). Only riff gives it.
    #[arg(long, global = true, hide = true, value_parser = identity::place_from_text)]
    place: Option<Place>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in to the riff
    ///
    /// It opens the browser at the provider of riff-server.
    Login,
    /// Show your URI and the state of the riff
    ///
    /// Inside Claude Code, it is the URI of the session.
    Whoami,
    /// List the sessions of the riff
    ///
    /// It shows the state of the riff too. A session that ended, or
    /// stopped for 3 minutes, is gone and not listed.
    Who {
        /// List gone sessions too.
        #[arg(long)]
        all: bool,
        /// Show the URI of each session in place of its name and claims.
        #[arg(long)]
        long: bool,
    },
    /// Post a message to a thread.
    Post {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// Wake the sessions that match: FIELD=VALUE pairs with commas
        /// between them, for example user=mike,claim=issue-6. The fields
        /// are user, session, host, repo, worktree, claim and lead. Give
        /// --to again to wake more sessions.
        #[arg(long)]
        to: Vec<Selector>,
        /// The kind of post: message, status or note. A status request
        /// asks each session that it wakes to set its status. `riff who`
        /// then shows each status. A note wakes no session: the sessions
        /// that --to selects see it at their next read.
        #[arg(long, default_value = "message")]
        kind: Kind,
        /// The message. A status request needs none.
        body: Vec<String>,
    },
    /// Set your status: your current step, in your own words
    ///
    /// `riff who` shows it with its age, after the state that riff makes
    /// from facts. It replaces your old status.
    Status {
        /// Your current step, in one short line.
        #[arg(required = true)]
        step: Vec<String>,
    },
    /// Say that you cannot go on, and wake your lead
    ///
    /// riff shows you as blocked, and tells the lead of your user the
    /// reason. The block of a worker ends at its next work after an
    /// answer. The lead itself shows as waiting for its user, and tells
    /// nobody. The next prompt of the person ends the block of the lead.
    Blocked {
        /// Why you cannot go on, in one short line.
        #[arg(required = true)]
        reason: Vec<String>,
    },
    /// Show a long step in riff who and riff top
    ///
    /// A long step runs longer than one tool call, for example a live
    /// window of two hours. riff who and riff top show it with its age
    /// until it is done. A failed step shows its reason, and wakes the
    /// lead of your user.
    Step {
        #[command(subcommand)]
        change: StepCommand,
    },
    /// Send a direct message to one session
    ///
    /// It wakes that session.
    Tell {
        /// The session: its session ID or the start of it, as `riff read`
        /// shows it, its full riff:// URI from `riff who`, or `lead` for
        /// the lead of your user in this repository.
        session: String,
        /// The message.
        #[arg(required = true)]
        body: Vec<String>,
    },
    /// Show the unread messages of your threads
    ///
    /// You join the thread of your repository first.
    Read {
        /// Read only this thread. It need not be one of your threads. A
        /// direct thread of yours is dm:PEER: the session ID of the other
        /// side or its start, or the user of a person, as riff read shows
        /// it.
        #[arg(long, short)]
        thread: Option<String>,
        /// Show the full history, not only the unread messages.
        #[arg(long)]
        all: bool,
    },
    /// Follow a thread, for people
    ///
    /// It shows each new message of the thread in a block, with color in
    /// a terminal.
    Tail {
        /// The thread. The default is your repository thread.
        thread: Option<String>,
    },
    /// Chat with the people of the riff
    ///
    /// It works in the style of IRC. It shows the chat and each new line,
    /// and sends each line that you type. A line wakes your lead. A line
    /// with @USER wakes the lead of USER, and not yours, unless it has
    /// @lead too. /me TEXT sends an action. /quit or Ctrl-C exits.
    Chat {
        /// The last line that the chat showed before an update. Only
        /// riff gives it (01M3NT6WXGCNKW3EQ7MBJDQTR4).
        #[arg(long, hide = true)]
        after: Option<u64>,
    },
    /// Show a live table of each session
    ///
    /// The tree has the levels person, host, repository and session. Each
    /// row shows the tags, the item and the status of a session. It
    /// draws the table again in place until Ctrl-C. It posts nothing and
    /// wakes no session.
    Top {
        /// Print the table once and exit.
        #[arg(long)]
        once: bool,
        /// Show only the sessions of this user.
        #[arg(long, value_name = "USER")]
        user: Option<String>,
        /// Show only the sessions on this host.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        /// Show only the sessions in this repository.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
        /// The top level of the tree: person (person, host, repository,
        /// session) or repo (repository, person, host, session).
        #[arg(long, value_enum, default_value = "person")]
        by: ByArg,
    },
    /// Claim a work item
    ///
    /// Then no other session does the same work. It exits with status 1
    /// when another session holds it, or when a lead holds it and this
    /// session is a worker.
    Claim {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
    },
    /// Hold or free a work item of the plan of your repository
    ///
    /// A held item is no free work: no worker can claim it. A hold is
    /// not a claim. Only a lead, the owner or an admin can hold and free
    /// an item.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Release a claimed item, or make a release of riff
    ///
    /// With ITEM, it frees your claim. The lead frees the claim of
    /// another session of its user with `--session`, for example a
    /// session that is gone or that does not answer. Then the next
    /// session can claim the item. The subcommands make a release of
    /// riff: cut, publish, status, approve, check, retry and rollback.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Release {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The session that holds the item: its session ID or the start
        /// of it, as `riff who` shows it. Only the lead can name one.
        #[arg(long)]
        session: Option<String>,
        /// The work item, for example issue-12.
        #[arg(required = true)]
        item: Option<String>,
        #[command(subcommand)]
        cycle: Option<ReleaseCommand>,
    },
    /// Show the tokens and the models of an issue
    ///
    /// With ISSUE, it sums the comments that riff put on the issue: the
    /// total, then each work claim and each verify claim with its models.
    /// With --wave, it lists each issue of the wave with its total. With
    /// no issue and no wave, it shows each session of this machine: the
    /// tokens of each item, and the tokens for no issue.
    Usage {
        /// The issue: 12, #12 or issue-12.
        #[arg(conflicts_with = "wave")]
        issue: Option<String>,
        /// Each issue of this wave: its number, for example 3 or "Wave
        /// 3". The name of the wave is not necessary.
        #[arg(long, value_name = "WAVE")]
        wave: Option<String>,
        /// Write the total comment of ISSUE again, for example when
        /// `riff pr wait` could not write it.
        #[arg(long, requires = "issue")]
        total: bool,
    },
    /// Count the refusals of auto mode
    ///
    /// Each refusal of the auto mode checker of Claude Code goes to the
    /// thread denials as one record with tags. It counts the records by
    /// reason, by tool and head, by role and by repo, and then shows the
    /// last 20 records.
    Denials {
        /// Only the records of this time, for example 7d, 12h or 30m.
        #[arg(long, value_name = "TIME")]
        since: Option<String>,
        /// text for the counts, json for the records.
        #[arg(long, value_enum, default_value = "text")]
        format: DenialsFormat,
    },
    /// Find out about the sessions of a wave
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    /// Name the lead of your user: riff lead --session ID
    ///
    /// Each person has one lead in each repository. The other sessions of
    /// your user send their questions to the lead. `riff lead --session
    /// ID` makes the session ID the lead of your user in the repository
    /// of this directory. It replaces the old lead, and riff tells the old
    /// lead. Run it in a terminal with no session, not in an agent
    /// session: a session cannot name the lead, also not itself. `riff
    /// lead start` names its new lead too.
    #[command(args_conflicts_with_subcommands = true)]
    Lead {
        /// The session ID of the new lead, or its start: at least 4
        /// characters. `riff who` shows it.
        #[arg(long)]
        session: Option<String>,
        #[command(subcommand)]
        command: Option<LeadCommand>,
    },
    /// Pause your repository, or the whole riff
    ///
    /// Each session of the repository of this directory stops at its
    /// next step and waits. Nobody claims work there. The other
    /// repositories go on. You in a shell, or the lead, can pause a
    /// repository.
    Pause {
        /// Pause the whole riff: each repository. Only the owner or an
        /// admin can
        #[arg(long, conflicts_with = "repo")]
        riff: bool,
        /// Pause this repository, not the repository of this directory.
        /// Only the owner or an admin can
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Resume your repository, or the whole riff
    ///
    /// Each session of the repository of this directory goes on from
    /// where it stopped. A new riff starts paused: the owner or an
    /// admin resumes it with --riff to start the work.
    Resume {
        /// Resume the whole riff. Only the owner or an admin can
        #[arg(long, conflicts_with = "repo")]
        riff: bool,
        /// Resume this repository, not the repository of this
        /// directory. Only the owner or an admin can
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Sign out of the riff on this device
    ///
    /// It removes the sign-in at riff-server from this device. With
    /// --all, it ends each sign-in of a person on each device.
    Logout {
        /// End each sign-in, on each device.
        #[arg(long)]
        all: bool,
        /// With --all: the person. The default is you. Only an admin
        /// names another person.
        #[arg(long, requires = "all")]
        user: Option<String>,
    },
    /// Let a person join this riff
    ///
    /// It adds the verified email of a person to the members of the riff
    /// of riff-server, and prints the address of the riff and the lines
    /// that the person runs to join. Only the owner or an admin can.
    Invite {
        /// The email that the person signs in with.
        email: String,
    },
    /// Remove a member of this riff
    ///
    /// It removes a person from the riff of riff-server, and ends each
    /// sign-in of that person. Only the owner or an admin can. The owner
    /// stays.
    Remove {
        /// The email of the person.
        email: String,
    },
    /// List who may join this riff
    ///
    /// It lists the owner, the admins, the members and the allowed domains
    /// of the riff of riff-server.
    Members,
    /// Make a person an admin, or a member again
    ///
    /// An admin can invite and remove members. Only the owner can.
    Admin {
        #[command(subcommand)]
        command: Admin,
    },
    /// Pass the owner role to another person
    ///
    /// The person is a member or an admin. You stay an admin. Only the
    /// owner can. An admin asks for the role with --take, and the owner
    /// keeps it with --deny.
    #[command(group = clap::ArgGroup::new("step").required(true).args(["email", "take", "deny"]))]
    Owner {
        /// The email of the new owner. The person must be a member or an
        /// admin.
        email: Option<String>,
        /// Ask for the owner role. Only an admin can. The owner has 10
        /// minutes to answer (a setting of riff-server). With no answer,
        /// you are the owner. On a riff with no owner, you are the owner
        /// at once.
        #[arg(long)]
        take: bool,
        /// Keep the owner role when an admin asks for it. Only the owner
        /// can.
        #[arg(long)]
        deny: bool,
    },
    /// Add the Claude Code permission rules of riff to this project
    ///
    /// It adds each missing rule to .claude/settings.json at the top of
    /// the repository: allow each riff tool, each riff command and the
    /// pull request steps; deny a push to the default branch and
    /// `gh pr merge --admin`. It keeps each rule and key that is there.
    /// A rule in the user or the local settings counts as there. Commit
    /// the file, so that each clone and each worktree has the rules.
    Setup {
        /// Change nothing. Name each missing rule, and exit with status 1
        /// when a rule is missing.
        #[arg(long)]
        check: bool,
    },
    /// Show the riff that riff uses
    ///
    /// It shows one fact on a line: the release of riff, the riff that
    /// riff uses and where that choice comes from (--server, RIFF_SERVER
    /// or the default), its release and your sign-in. The riff of this
    /// machine shows only when it answers. When you must act, the last
    /// line says what to run.
    ///
    /// --server and RIFF_SERVER take a URL, HOST or HOST:PORT. With no
    /// scheme, riff uses http, and port 7878 when there is no port. With
    /// neither, riff uses the riff of this machine, http://127.0.0.1:7878.
    Server,
    /// Update riff on this machine
    ///
    /// It installs riff and riff-server of a release. On Linux x86-64 it downloads
    /// the static binaries of the GitHub release, checks them, and puts
    /// them in the bin folder of cargo. On another machine, or for a
    /// release with no static binaries, cargo builds them. It installs
    /// the release that the riff runs, or the release that GitHub marks
    /// Latest when riff uses the riff of this machine or cannot read the
    /// build of the riff.
    /// When the unit of the riff daemon runs another riff than the riff on
    /// PATH, it writes the unit again with that riff. It starts the daemon
    /// again on the new riff when the daemon runs no session; else the
    /// daemon starts again after its last session.
    /// When the riff of this machine runs the old build, it tells you to
    /// start riff-server again.
    Update {
        /// Install this release, for example v0.2.0, not the release
        /// that the riff runs.
        #[arg(long, value_parser = lifecycle::parse_tag)]
        tag: Option<String>,
        /// Turn the update by itself on or off for this machine, and
        /// install nothing now. When it is on, riff installs each new
        /// release that the riff runs, in the background, and the riff
        /// daemon of each release does each step of riff init that needs
        /// no person: the permission rules of Claude Code, the unit of the
        /// daemon and the session images. With no value, show the setting.
        #[arg(long, value_enum, conflicts_with_all = ["tag", "background"])]
        auto: Option<Option<Switch>>,
        /// Show each step of an update on this machine, and change
        /// nothing: the riff binary, the rules of Claude Code, the unit of
        /// the riff daemon, the session images and the checks of the
        /// machine, each in step or not, with the fix. It exits with 1
        /// when a step is not in step.
        #[arg(long, conflicts_with_all = ["tag", "auto", "background"])]
        check: bool,
        /// Run as the update by itself: take the update lock of this
        /// machine, and tell the lead the result.
        #[arg(long, hide = true, requires = "tag")]
        background: bool,
        /// The cargo command.
        #[arg(long, default_value = "cargo")]
        cargo: std::path::PathBuf,
    },
    /// Open a pull request, and wait for its merge
    ///
    /// These are the steps of a pull request on GitHub, with the gh of
    /// this machine.
    Pr {
        #[command(subcommand)]
        command: Pr,
    },
    /// Report the verify of a pull request
    ///
    /// It uses the gh of this machine. It comments the result on the pull
    /// request, sets the status riff/verify of its head commit, and posts
    /// the result to the session that holds its issue. A pass needs a
    /// success of the check Gate of the head commit.
    Verify {
        /// pass sets the status success, fail sets failure.
        verdict: VerdictArg,
        /// The number of the pull request.
        number: u64,
        /// The result: each criterion, and what you did to check it.
        #[arg(long)]
        file: std::path::PathBuf,
        /// The commit that you tested. The default is HEAD of this
        /// directory. riff reports nothing when it is not the head of the
        /// pull request.
        #[arg(long, value_name = "SHA")]
        commit: Option<String>,
    },
    /// Build riff of the default branch in the check clone
    ///
    /// A check after a merge can need a new build of riff. riff builds it
    /// in the one check clone of the machine, ~/.cache/riff/check. When
    /// the folder is not there, riff clones the origin of the repository
    /// of this folder into it. Then it fetches origin, puts the clone on
    /// the default branch, and runs just static. It prints the path of
    /// the riff of the build. A session in a worktree runs this command
    /// in place of git steps on the check clone.
    Build {
        /// The just command.
        #[arg(long, default_value = "just")]
        just: std::path::PathBuf,
        /// Build on the host HOST, through riff-server. Only you in a
        /// terminal and your lead in this repository can use --host. The
        /// daemon of HOST refuses each other session, for example a
        /// worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Get this machine ready for riff run: check, install, pull
    ///
    /// The first run of riff 3.0 on a machine. It checks rootless podman:
    /// podman runs and riff does not run as root, podman has its init
    /// (catatonit), /etc/subuid and /etc/subgid give you 65536 IDs or more,
    /// and cgroup v2 gives cpu and memory to your user. It checks
    /// that riff.toml of the repository of this folder pins the session
    /// image. It writes no rule into your user settings of Claude Code. It
    /// names each allow rule of your user, project and local settings of
    /// Claude Code that is wider than the steps of riff, and asks one
    /// question before it removes them. It prints the fix of each check that fails, and stops. When
    /// each check passes, it installs and starts the riff daemon, as riff
    /// daemon install does, and the daemon pulls the session image. A
    /// daemon that answers stays: riff init does not start it again.
    Init {
        /// Change nothing. Print the fix of each check that fails, and
        /// each wide allow rule. Exit with status 1 when a check fails.
        #[arg(long, conflicts_with = "yes")]
        check: bool,
        /// Ask no question. It removes no wide rule.
        #[arg(long)]
        yes: bool,
    },
    /// Run one work item in a rootless podman container
    ///
    /// It needs the riff daemon of this machine: see riff daemon. It
    /// claims ITEM as a new worker session, makes a clone of its own in
    /// the folder of riff, and asks the daemon to run claude -p in a
    /// container of the session image. The daemon pulls the image when
    /// this machine does not have it. riff run shows the events of the
    /// session. At the end, it sends a verify request for the pull
    /// request of the branch, releases ITEM, and keeps the clone until
    /// the merge. Ctrl-C stops the container.
    Run {
        /// The work item: an issue, for example issue-12.
        item: String,
        /// The session image, for example a local build. The default is
        /// the merged pin: the image that riff.toml on the default branch
        /// of origin pins.
        #[arg(long)]
        image: Option<String>,
    },
    /// Plan the waves of this repository, or run one
    ///
    /// The lead plans the waves with riff wave list, riff wave make, riff
    /// wave rename and riff wave end. They use the gh of this machine, on
    /// the repository of this folder.
    ///
    /// With riff wave run, the daemon starts one session for each free
    /// item of the current wave, and one for each pull request that waits
    /// for a verify, when the host has room: see riff daemon limit. Each
    /// session does one item in its own container, and ends. The daemon
    /// claims each item, sends the verify request, reports the result of
    /// each verify, and removes the clone of each item after its merge.
    Wave {
        #[command(subcommand)]
        command: WaveCommand,
    },
    /// Show the sessions of the riff daemon of this machine
    ///
    /// It shows each session with its item, its state, its age and its
    /// last step, the limit of this host, each start that waits for room
    /// and the waves of the daemon. It draws them again each second. Up
    /// and Down select a session. Enter attaches to the lead: Ctrl-P
    /// Ctrl-Q detaches and comes back here. t takes over the selected
    /// session of this machine, as riff takeover does: it stops the turn
    /// at once. q ends the view; the sessions go on.
    View {
        /// Show the riff daemon of HOST, through riff-server. riff asks it
        /// each 3 seconds. Enter and t work only on HOST. Only you in a
        /// terminal and your lead in this repository can use --host. The
        /// daemon of HOST refuses each other session, for example a
        /// worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Take over a session of the riff daemon in your terminal
    ///
    /// riff stops the turn of the session at once, also in the middle of
    /// a tool call. Then claude opens here with the history of the
    /// session, in its container. When you end it with /exit, the session
    /// goes on with its item. riff view and riff daemon status show the
    /// names.
    Takeover {
        /// Wait for the end of the turn of the session. Do not stop it.
        #[arg(long)]
        at_end: bool,
        /// The name of the container, for example riff-issue-12-1a2b3c4d.
        name: String,
    },
    /// Run the riff daemon, or install, remove or show it
    ///
    /// The riff daemon owns the containers of riff run on this machine.
    /// With no subcommand, it runs the daemon in this terminal, as the
    /// systemd user unit riff.service does, until Ctrl-C or SIGTERM. A
    /// stop of the daemon stops no container.
    Daemon {
        #[command(subcommand)]
        command: Option<DaemonCommand>,
    },
    /// The riff in a session of the riff daemon
    ///
    /// The riff daemon runs it in each container: riff relay -- claude -p
    /// .... It starts the command with the riff tools, gives it its
    /// input, and sends its events to the daemon on the socket of the
    /// session. While the daemon is away, it keeps the events.
    #[command(hide = true, args_conflicts_with_subcommands = true)]
    Relay {
        /// Run the command with the terminal, in the place of the relay:
        /// the lead, and the claude of a takeover.
        #[arg(long)]
        terminal: bool,
        /// The command of the session and its arguments.
        #[arg(last = true)]
        session: Vec<String>,
        #[command(subcommand)]
        command: Option<RelayCommand>,
    },
    /// Check that a wave followed the rules
    ///
    /// It reads the log of the repository of this directory from the
    /// riff, and the issues and pull requests of the wave with gh. It
    /// prints each rule with pass, fail or not checked, and the records
    /// that show it. It exits with 1 when a rule fails. Only the owner or
    /// an admin can.
    Audit {
        /// The wave: its number, for example 3 or "Wave 3". The name of
        /// the wave is not necessary.
        #[arg(long, value_name = "WAVE")]
        wave: String,
    },
    /// Make and run riff-server instances on Cloud Run
    ///
    /// It runs the gcloud of this machine. Sign in first with gcloud auth
    /// login. Each instance has a settings file NAME.env: in deploy/cloud
    /// of this repository when it has that folder, else in the folder
    /// cloud beside the riff settings of this machine.
    Cloud {
        #[command(subcommand)]
        command: CloudCommand,
    },
    /// Keep the token of claude setup-token for the containers
    ///
    /// Run claude setup-token on this machine first, and sign in at the
    /// browser. Then run riff claude-token in a terminal, and paste the
    /// token. riff reads it from stdin with no echo, and keeps it in the
    /// OS keyring, in no file. It removes the old file of riff 3.3.0.
    /// The daemon gives each new
    /// container session the token in CLAUDE_CODE_OAUTH_TOKEN, and binds
    /// no sign-in file of claude. So a session starts also when the
    /// sign-in of claude on this machine ends soon. Run it again for a
    /// new token. It refuses when stdin is not a terminal: an agent
    /// session cannot run it.
    ClaudeToken,
    /// Print the session image of this repository
    ///
    /// It reads the key image in the table [session] of riff.toml, at the
    /// top of the git work tree of the current folder. The image has a
    /// digest and no tag, so podman pull takes it as it is. It fails when
    /// riff.toml names a tag. riff run, riff lead start and the waves use
    /// only the merged pin: riff.toml on the default branch of origin.
    Image,
}

/// The top level of the tree of `riff top`.
#[derive(Clone, Copy, clap::ValueEnum)]
enum ByArg {
    Person,
    Repo,
}

impl From<ByArg> for riff_client::sessions::By {
    fn from(by: ByArg) -> Self {
        match by {
            ByArg::Person => Self::Person,
            ByArg::Repo => Self::Repo,
        }
    }
}

/// The subcommands of `riff cloud`.
#[derive(Subcommand)]
enum CloudCommand {
    /// Make the resources of an instance
    ///
    /// A new instance gets a settings file with its own service, bucket,
    /// accounts and secret. Then riff makes each resource that is
    /// missing: it checks each one first, so you can run it again. With
    /// RIFF_OWNER set and an alert in the settings, it makes an alert
    /// that sends each error of the service to that email.
    Create {
        /// The name of the instance, for example stage.
        name: String,
        /// The Google Cloud project. Only for a new instance.
        #[arg(long)]
        project: Option<String>,
        /// The region of the service, for example us-central1. Only for
        /// a new instance.
        #[arg(long)]
        region: Option<String>,
    },
    /// Store the sign-in client of an instance
    ///
    /// It shows the steps to make the client in the console. Then it
    /// asks for the client ID and the secret. The secret goes to Secret
    /// Manager, and the ID to the settings file.
    Signin {
        /// The name of the instance.
        name: String,
    },
    /// Deploy riff-server to an instance
    ///
    /// With a release tag, it deploys the image of that release from the
    /// image repository of the project. With the full ID of a commit, it
    /// deploys the image that CI built for a merge to main; only an
    /// instance with CLOUD_CONFIRM=false takes it. With no tag, Cloud
    /// Build builds the tree of this directory. An instance with
    /// CLOUD_CONFIRM=true, for example the shared riff, asks for its name
    /// first.
    Deploy {
        /// The name of the instance.
        name: String,
        /// The release tag vX.Y.Z, or the full ID of a commit of main.
        tag: Option<String>,
        /// The name of the instance again, when there is no terminal to
        /// type it, for example in CI.
        #[arg(long, value_name = "NAME")]
        confirm: Option<String>,
    },
    /// Run the smoke test against an instance
    ///
    /// It signs in as the test account, posts a message, reads it back,
    /// and checks that the server runs the build of this riff. The
    /// refresh token of the test account comes from RIFF_SMOKE_TOKEN.
    /// Set RIFF_HOME to a new directory, so that the sign-in stays out of
    /// the keyring.
    Smoke {
        /// The name of the instance.
        name: String,
    },
    /// List each instance: URL, release, ready, paused
    List,
    /// Show the state of one instance
    Status {
        /// The name of the instance.
        name: String,
    },
    /// Show the log of the service of an instance
    Log {
        /// The name of the instance.
        name: String,
        /// Show only the lines with the severity ERROR or more.
        #[arg(long)]
        errors: bool,
        /// The number of lines.
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// A filter of Cloud Logging, for example
        /// 'jsonPayload.result="refused"'.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Delete the service of an instance
    ///
    /// It asks for the name first. The bucket with the state stays,
    /// unless you add --with-state. The settings file stays.
    Delete {
        /// The name of the instance.
        name: String,
        /// Delete the bucket with the state too.
        #[arg(long)]
        with_state: bool,
        /// The name of the instance again, when there is no terminal to
        /// type it.
        #[arg(long, value_name = "NAME")]
        confirm: Option<String>,
    },
}

impl CloudCommand {
    /// True for a write: a subcommand of [`cloud::WRITES`]
    /// (01M558C4VZ2PSMEYPAZBRYW1VV).
    fn writes(&self) -> bool {
        matches!(
            self,
            Self::Create { .. }
                | Self::Signin { .. }
                | Self::Deploy { .. }
                | Self::Smoke { .. }
                | Self::Delete { .. }
        )
    }
}

/// On or off.
#[derive(Clone, Copy, clap::ValueEnum)]
enum Switch {
    On,
    Off,
}

/// The result of a verify.
#[derive(Clone, Copy, clap::ValueEnum)]
enum VerdictArg {
    Pass,
    Fail,
}

impl From<VerdictArg> for riff::pr::Verdict {
    fn from(verdict: VerdictArg) -> Self {
        match verdict {
            VerdictArg::Pass => Self::Pass,
            VerdictArg::Fail => Self::Fail,
        }
    }
}

/// The release cycle, the subcommands of `riff release`
/// (01M58B0HZYMAK1B2AJMWKHCXG0).
#[derive(Subcommand)]
enum ReleaseCommand {
    /// Make the release pull request from origin/main
    ///
    /// It sets the version in Cargo.toml and Cargo.lock,
    /// runs deploy/release-check.sh, and pushes the branch
    /// release-vX.Y.Z. The pull request has the label release, the
    /// current wave, auto-merge on, and the items of each wave since the
    /// last release. It says Part of #N for the release issue. It
    /// refuses a version that is not newer than the last release, while
    /// the version on origin/main has no published release, and while a
    /// deploy waits for the approval of production.
    Cut {
        /// The new version, for example 3.3.0.
        version: String,
        /// The version on origin/main that gets no release. Only with
        /// --reason. Only the owner or an admin, in a plain terminal:
        /// it refuses in each agent session.
        #[arg(long, requires = "reason", value_name = "VERSION")]
        skip: Option<String>,
        /// Why the version of --skip gets no release. It goes into the
        /// pull request and into the GitHub release.
        #[arg(long, requires = "skip", value_name = "TEXT")]
        reason: Option<String>,
        /// The reason of the level, in one line for a person. It follows
        /// the level line.
        #[arg(long, value_name = "TEXT")]
        why: Option<String>,
    },
    /// Verify a release pull request with no model session
    ///
    /// For a pull request with the label release. It checks that the
    /// diff is only Cargo.toml and Cargo.lock, that the body has the
    /// format of riff release cut, and that the Gate of the head commit
    /// passed. Then it sets the status riff/verify. Else it refuses and
    /// sets nothing.
    Verify {
        /// The number of the release pull request.
        number: u64,
    },
    /// Tag the release, make the GitHub release, and tell the owner
    ///
    /// Only the owner or an admin of the riff in a terminal, or the lead
    /// of one, with a gh of an admin of the repository. A riff with no
    /// sign-in has no roles. It refuses in a worker and in a container
    /// session.
    /// It checks first: the pull request "Release vX.Y.Z" with the label
    /// release merged with riff/verify passed, its merge commit is on
    /// origin/main, a commit from it to origin/main passed the
    /// real-session test (the status riff/real-session), Cargo.toml has
    /// X.Y.Z, and the tag does not exist. Then it pushes only the
    /// annotated tag on that commit, makes the release, marked as the
    /// latest, and posts a chat line to the owner.
    Publish {
        /// The release tag, for example v3.3.0.
        tag: String,
    },
    /// Show the release cycle
    ///
    /// The version on origin/main, the last published release, a merged
    /// release that is not published, each CI run that waits for the
    /// environment production, the state of the current release, and
    /// the next step.
    Status,
    /// Approve the deploy of a release to production
    ///
    /// A person in a plain terminal may approve. The lead of the owner or
    /// of an admin may approve too, when riff chat has a verified message
    /// of the owner or of an admin, after the publish, that says approve
    /// or deploy and names the version. Else it refuses. A worker and
    /// each other agent session may not. It finds the CI run of the tag
    /// that waits for the environment production, shows the tag, the
    /// commit and the run, and a person answers yes or no. On yes, it
    /// pauses the whole riff, which tells each other lead, then approves.
    /// The lead asks no yes or no.
    Approve {
        /// The release tag, for example v3.3.0.
        tag: String,
    },
    /// Do the checks after the deploy of a release
    ///
    /// The lead runs it after the job Deploy. It checks that riff server
    /// shows the build, that the Cloud Run log has no run of 503 longer
    /// than 20 s, no 500 of riff-server, no 401 or 429 of the token
    /// store, and no old instance that takes the lease, and that each
    /// host has the build. It writes the result on the release issue.
    /// When each check passes, it closes the release issue and resumes
    /// the riff. After a failed deploy job, it files an issue with the
    /// log and resumes the riff: the old revision still serves.
    Check {
        /// The release tag, for example v3.3.0.
        tag: String,
    },
    /// Run the failed jobs of the CI run of a release again
    ///
    /// For a failed deploy. The job Deploy then waits for the approval
    /// of a person again: riff release approve.
    Retry {
        /// The release tag, for example v3.3.0.
        tag: String,
    },
    /// Run the real-session test of a merged release on a host
    ///
    /// A container session cannot run just real-session: podman does not
    /// run inside a container. The lead asks the riff daemon of HOST. It
    /// clones the newest commit of origin/main, runs the test there, and
    /// sets the commit status riff/real-session: pending, then success
    /// or failure. riff release status shows the state. Only you in a
    /// terminal and your lead can use it. The daemon refuses a worker.
    Test {
        /// The release tag, for example v3.3.0.
        tag: String,
        /// The host with podman. riff daemon status --host lists them.
        #[arg(long)]
        host: String,
    },
    /// Deploy an older release again
    ///
    /// It starts the CI workflow on main with the input tag, and writes
    /// the rollback on the release issue of the newest release. The job
    /// Deploy then waits for the approval of a person: riff release
    /// approve with the older tag.
    Rollback {
        /// The older release tag, for example v3.2.0.
        tag: String,
    },
}

#[derive(Subcommand)]
enum Pr {
    /// Open the pull request of this branch
    ///
    /// It is for the issue that this session claims, and it turns on
    /// auto-merge with a squash. The body has the link line and the
    /// trailers Issue: and Milestone: of the issue. The trailer
    /// Milestone: names a wave by its number, for example Wave 3. Push
    /// the branch first.
    Open {
        /// The title. Do not end it with (#N).
        #[arg(long)]
        title: String,
        /// A file with the summary of the change, for the body. It can
        /// hold the link line and the trailers: riff adds only the ones
        /// that are missing.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// Link with Refs #N, not Closes #N: the merge leaves the issue
        /// open. Use it for each pull request before the last one of the
        /// issue, and when a check after the release is left.
        #[arg(long)]
        refs: bool,
        /// The issue. The default is the claim issue-N of this session.
        #[arg(long)]
        issue: Option<u64>,
    },
    /// Wait for the merge of a pull request
    ///
    /// When pull request NUMBER is merged, it prints its merge commit. It
    /// exits with status 1 when the pull request closes unmerged or a
    /// required check fails.
    Wait {
        /// The number of the pull request.
        number: u64,
        /// The seconds between two looks at the pull request.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
        every: u64,
    },
}

#[derive(Subcommand)]
enum Admin {
    /// Make a person an admin. The person is also a member.
    Add {
        /// The email that the person signs in with.
        email: String,
    },
    /// Make an admin a member again. The owner stays an admin.
    Remove {
        /// The email of the admin.
        email: String,
    },
}

#[derive(Subcommand)]
enum WaveCommand {
    /// Show the open waves of this repository, with their items
    ///
    /// It shows the waves in number order, then each other open group of
    /// items, for example the backlog. Each line has the number of the
    /// open and the closed items. The current wave, the open wave with
    /// the lowest number, has the mark current, and "runs on HOST" for
    /// each host whose riff daemon runs its loop. Under each wave come
    /// its open issues in the groups of riff top: free, held (with the
    /// reason of the lead), claimed, verify and tracker. A tracker has
    /// the label tracker, or a title that starts with "Tracker:". No
    /// session does it. When riff cannot read riff-server, it shows only
    /// the waves and their numbers.
    List,
    /// Make a wave
    ///
    /// The title is Wave N, or Wave N: NAME. Give N the last number plus
    /// one. riff refuses a number that an open wave has. riff also makes
    /// the label tracker when it is missing.
    Make {
        /// The title, for example "Wave 6" or "Wave 6: Cloud".
        #[arg(required = true)]
        title: Vec<String>,
    },
    /// Rename a wave, and keep its number
    ///
    /// The open wave N gets the title Wave N: NAME. Each pull request
    /// names its wave by the number (Milestone: Wave N), so no pull
    /// request changes.
    Rename {
        /// The number of the wave, for example 6 for Wave 6.
        number: u64,
        /// The new name, for example "Cloud" for Wave 6: Cloud.
        #[arg(required = true)]
        name: Vec<String>,
    },
    /// Move items to another wave, or to the Backlog
    ///
    /// riff runs gh issue edit for each item. It refuses an item that a
    /// session claims, and a target that no open milestone has. Then it
    /// moves nothing.
    Move {
        /// The numbers of the issues, for example 71 72.
        #[arg(required = true)]
        items: Vec<u64>,
        /// The target: the number of an open wave, for example 7 for
        /// Wave 7, or Backlog.
        #[arg(long, value_name = "WAVE|Backlog")]
        to: String,
    },
    /// End a wave when each of its items is closed
    ///
    /// riff refuses a wave that has an open item. In a repository with
    /// deploy/release-check.sh, it also refuses while a merged pull
    /// request of the wave is in no checked release: the release cycle
    /// ends each wave with a release.
    ///
    /// With --move-open-to-backlog, riff first moves each open item of the
    /// wave to the Backlog, and refuses an item that a session claims.
    End {
        /// The number of the wave, for example 6 for Wave 6.
        number: u64,
        /// End the wave with no release, when no change of it is one that
        /// a person sees. The reason goes into the description of the
        /// wave.
        #[arg(long, value_name = "TEXT")]
        no_release: Option<String>,
        /// Move each open item of the wave to the Backlog, then end the wave.
        #[arg(long)]
        move_open_to_backlog: bool,
    },
    /// Run the current wave of the repository of this folder
    ///
    /// The daemon keeps the wave, also over its own restart, until riff
    /// wave stop. It needs the sign-in of riff, of gh and of claude on
    /// this machine. With --host, the daemon of HOST runs the wave in its
    /// own main clone of the repository.
    Run {
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Start no new session for the wave of this repository
    ///
    /// The sessions that run go on to their end. With --host, the daemon of
    /// HOST stops its wave.
    Stop {
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Install and start the systemd user unit of the riff daemon
    ///
    /// It writes riff.service in ~/.config/systemd/user, with the path
    /// of this riff and the PATH of this shell. It enables the unit and
    /// starts it again, so a new riff takes effect. It waits until the
    /// daemon answers. With RIFF_HOME or RIFF_DATA set, it installs
    /// nothing.
    Install,
    /// Stop the riff daemon, and remove its systemd user unit
    ///
    /// It disables and stops riff.service, deletes the unit file that
    /// riff daemon install wrote, and reloads systemd. Each container
    /// runs on, and the folder of the daemon stays. With RIFF_HOME or
    /// RIFF_DATA set, it removes nothing.
    Uninstall,
    /// Show the riff daemon and its sessions
    ///
    /// It shows the process, the build and the folder of the daemon, the
    /// limit of the host, each session, each start that waits for room,
    /// and each wave. It exits with 1 when no daemon answers, and shows
    /// the state of the unit riff.service.
    Status {
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Show the events of a session of the riff daemon
    ///
    /// It shows each event of the session in the container NAME from its
    /// start, as riff run does, and each new event until the session
    /// ends. It also shows the lines of the daemon about the session.
    /// The daemon keeps the end of a session for 5 minutes. riff daemon
    /// status shows the names. Ctrl-C ends the view: the session goes
    /// on.
    Events {
        /// The name of the container, for example riff-issue-12-1a2b3c4d.
        name: String,
        /// Ask the riff daemon of HOST, through riff-server. It shows the
        /// same events, and follows them up to the end of the session.
        /// Only you in a terminal and your lead in this repository can
        /// use --host. The daemon of HOST refuses each other session, for
        /// example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Show or set the limit of this host
    ///
    /// The daemon starts a session only when fewer sessions than the
    /// limit live, and the free memory is at or above the floor. Each
    /// other start waits for room, in order. With no flag, it shows the
    /// limit. The default is 3 sessions and a floor of 8 GB.
    ///
    /// The limit also holds the caps of each new container: its CPUs,
    /// which are also its build jobs, and its memory. The default is 8
    /// CPUs and 8 GB. A session that runs keeps its caps.
    Limit {
        /// The most live sessions.
        #[arg(long)]
        sessions: Option<u32>,
        /// The floor of free memory for a start, in GB.
        #[arg(long, value_name = "GB")]
        floor: Option<u64>,
        /// The CPUs and the build jobs of each new container, 1 or more.
        #[arg(long, value_name = "N")]
        cpus: Option<u32>,
        /// The memory cap of each new container, in GB, 1 or more.
        #[arg(long, value_name = "GB")]
        memory: Option<u64>,
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Show, set or empty the compile cache of the sessions
    ///
    /// The container sessions of this host share one compile cache of
    /// sccache: the podman volume riff-sccache. With no argument, it
    /// shows the size of the volume now and the most size. The default
    /// most size is 40G. A new session gets the size at its start; a
    /// session that runs keeps its size. --clear deletes each file of
    /// the cache, also while sessions run: their builds compile again.
    Cache {
        /// The new most size: a number with K, M, G or T, for example 20G.
        #[arg(conflicts_with = "clear")]
        size: Option<String>,
        /// Delete each file of the cache.
        #[arg(long)]
        clear: bool,
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Remove the folders of the sessions whose work is over, now
    ///
    /// The daemon does it by itself at its start and each 10 minutes.
    /// It removes the folder of each session that does not run, when the
    /// pull request of its branch merged or closed, or when its issue is
    /// closed and its branch has no pull request. A folder whose clone has
    /// a file that is not committed, or a commit that is not pushed,
    /// stays. It prints each folder that it removed, and each folder that
    /// stays with work in its clone, with the reason.
    Sweep {
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// End one session of the riff daemon, and free its item
    ///
    /// The daemon frees the claim of the session on riff-server, then
    /// stops its container. riff daemon status shows the names. It
    /// refuses the lead: end it in its terminal with /exit.
    End {
        /// The name of the container, for example riff-issue-12-1a2b3c4d.
        name: String,
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Start the riff daemon again on the new riff
    ///
    /// When the daemon runs an old riff and no session but the lead, start
    /// that waits or job of a wave, it starts again on the riff on disk.
    /// The lead stays. Else it says why not. riff daemon status shows the
    /// new pid and build.
    Restart {
        /// Ask the riff daemon of HOST, through riff-server. riff waits
        /// for its reply. Only you in a terminal and your lead in this
        /// repository can use --host. The daemon of HOST refuses each
        /// other session, for example a worker.
        #[arg(long)]
        host: Option<String>,
    },
    /// Let your lead on another host reach this riff daemon
    ///
    /// Run it once in the main clone of a repository. The daemon gets a
    /// session of its own on riff-server for the repository, and keeps
    /// it over its restarts. Then the lead runs the daemon commands
    /// with --host and the name of this host. riff wave run does this
    /// step too.
    Host,
    /// Show or change the MCP servers of the sessions of this host
    ///
    /// These are the MCP servers that each new container session of this
    /// host loads. The default is riff only. A session that runs keeps
    /// its servers. They are in mcp.json in the folder of the daemon.
    Mcp {
        #[command(subcommand)]
        command: Option<DaemonMcp>,
    },
}

#[derive(Subcommand)]
enum DaemonMcp {
    /// Give each new session of this host an MCP server
    ///
    /// NAME is an MCP server of your Claude Code config. `claude mcp list`
    /// shows the names. riff copies its config now: after a change of
    /// it, add it again. A stdio server runs in the container, so riff
    /// refuses it when the session image of the repository of this folder
    /// does not have its program.
    Add {
        /// The name of the MCP server.
        name: String,
    },
    /// Take an MCP server from each new session of this host
    ///
    /// riff stays.
    Remove {
        /// The name of the MCP server.
        name: String,
    },
}

#[derive(Subcommand)]
enum RelayCommand {
    /// Serve the riff tools of the session to claude over stdio
    ///
    /// claude runs it through the MCP config that riff relay gives it.
    Mcp,
    /// Wait for the next wake of the session, and print its line
    ///
    /// Without a flag, it ends at the next wake. With --follow, it never ends: it
    /// prints one line for each wake, so the Monitor tool of Claude Code wakes
    /// the session at each line. The lead runs that once.
    Watch {
        /// Print one line for each wake, and keep running.
        #[arg(long)]
        follow: bool,
    },
    /// Run the PermissionDenied hook of the session
    ///
    /// It reads the hook input on stdin, keeps one record with no secret
    /// in the Claude folder of the session, and starts the send through
    /// the riff daemon, detached. It always exits with status 0.
    Denied {
        /// Send the kept records now: the hook starts it, detached.
        #[arg(long)]
        send: bool,
    },
    /// Run the UserPromptSubmit hook of the session
    ///
    /// It tells the riff daemon that you gave a prompt. In the lead, the
    /// prompt ends the block of the lead. It prints nothing, and always
    /// exits with status 0.
    Prompt,
}

#[derive(Subcommand)]
enum PlanCommand {
    /// Hold a work item, so that no worker claims it
    ///
    /// The reason says why, for example "waits for the word of Mike". A
    /// worker that claims the item gets the reason. Each other session
    /// gets the claim, with the reason as a warning. A hold of a held
    /// item replaces its reason. It does not end a claim.
    Hold {
        /// The repository thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
        /// Why the item is held: 1 to 200 characters.
        #[arg(required = true)]
        reason: Vec<String>,
    },
    /// Free a held work item, so that a worker can claim it again
    Free {
        /// The repository thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
    },
}

#[derive(Subcommand)]
enum SessionCommand {
    /// Show why a session ended with no pull request
    ///
    /// The riff daemon leaves one record in the thread ends when a
    /// session of a wave ends with no open pull request: its exit code or
    /// signal, the tool calls that auto mode denied, and the last 50
    /// lines of its output. The lead reads it here, also in a container.
    Why {
        /// The item, for example issue-1203.
        item: String,
    },
}

#[derive(Subcommand)]
enum LeadCommand {
    /// Start the lead of this repository in a container
    ///
    /// The riff daemon starts claude with a terminal in a container of
    /// the session image, with a clone and a Claude folder of its own
    /// that stay between starts. When the lead runs already, riff
    /// attaches to it. Ctrl-P Ctrl-Q detaches: the lead runs on. riff
    /// view shows it and attaches to it too.
    ///
    /// When no wave of this repository runs, riff first asks the riff
    /// daemon to run the wave: the lead in a container has no rollout.
    Start,
    /// Move the lead of this repository to another host
    ///
    /// riff asks the old lead to write its handover to the notes of the
    /// lead, and waits for it for 5 minutes. Then riff asks the riff
    /// daemon of HOST to start the lead there, in a container, with the
    /// handover in its first prompt. The new lead becomes the lead, and
    /// riff tells the old lead that it is not the lead any more. The old
    /// session stays as a plain session: you end it. The lead can run
    /// this command on your word. Only the lead and you can run it.
    ///
    /// On HOST, the daemon must serve this repository: run `riff daemon
    /// host` in its main clone there. Then `riff lead start` or `riff
    /// view` on HOST attaches to the new lead.
    Move {
        /// The host of the new lead, for example thelio.
        host: String,
    },
    /// Show or edit the notes of the lead
    ///
    /// The notes are text that riff-server keeps for your user and the
    /// repository: the running state of the lead, what waits for you, and
    /// your choices. A lead on any host reads them at its start. The lead
    /// writes them with its notes tool. riff refuses a text over 64 KiB and
    /// a text that looks like a token. Only you and the owner can read them.
    Notes {
        #[command(subcommand)]
        command: Option<NotesCommand>,
        /// The repository thread. The default is your repository thread.
        #[arg(long, short, global = true)]
        thread: Option<String>,
        /// Show also each old text, the newest first.
        #[arg(long)]
        history: bool,
        /// Show the notes of this user. Only the owner can show the notes
        /// of another user.
        #[arg(long)]
        user: Option<String>,
    },
}

#[derive(Subcommand)]
enum NotesCommand {
    /// Change the notes of the lead in your editor
    ///
    /// riff opens $VISUAL, $EDITOR or vi on the notes. When you save a new
    /// text, it replaces the old text. The old text stays in the history.
    Edit {
        /// Take the new text from this file, and open no editor. - is
        /// the standard input.
        #[arg(long, value_name = "FILE")]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum StepCommand {
    /// Start a long step. It replaces your old step
    Start {
        /// The name of the step, in one short line.
        #[arg(required = true)]
        name: Vec<String>,
    },
    /// The step ended well. riff shows it no more
    Done,
    /// The step failed. riff shows the reason, and wakes your lead
    Fail {
        /// Why the step failed, in one short line.
        #[arg(required = true)]
        reason: Vec<String>,
    },
}

/// The output of `riff denials`.
#[derive(Clone, Copy, clap::ValueEnum)]
enum DenialsFormat {
    /// The counts and the last records.
    Text,
    /// Each record as JSON.
    Json,
}

impl Command {
    /// True for a command that serves and does not end by itself:
    /// `riff daemon` and `riff relay`. They keep the default panic hook
    /// (01M5MRG35BYRGZYD4VQVV8ZTYX).
    fn serves(&self) -> bool {
        matches!(
            self,
            Command::Daemon { command: None } | Command::Relay { .. }
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    riff::platform::install();
    let matches = help::matches(help::grouped(Cli::command(), help::GROUPS));
    let cli = Cli::from_arg_matches(&matches)?;
    // A closed pipe on stdout ends a command quietly
    // (01M5MRG35BYRGZYD4VQVV8ZTYX).
    if !cli.command.serves() {
        riff_core::pipe::end_quietly();
    }
    use_color(cli.color);
    // The local folder is private (01M56RB1FZ575KQYHW9FF716CZ).
    if let Err(line) = local::secure_here() {
        eprintln!("{line}");
    }
    if let Command::ClaudeToken = cli.command {
        println!("{}", riff::claude_token::run(&riff::daemon::dir()?)?);
        return Ok(());
    }
    if let Command::Image = cli.command {
        let dir = std::env::current_dir().context("cannot read the current folder")?;
        println!("{}", riff::session_image::of_folder(&dir)?);
        return Ok(());
    }
    if let Command::Daemon { command } = &cli.command {
        return daemon_command(command.as_ref(), cli.server.as_deref()).await;
    }
    if let Command::Init { check, yes } = cli.command {
        use riff::first_run::Mode;
        let mode = match (check, yes) {
            (true, _) => Mode::Check,
            (false, true) => Mode::Yes,
            (false, false) => Mode::Ask,
        };
        let code = riff::first_run::init(mode).await?;
        if code != 0 {
            std::process::exit(code);
        }
        return Ok(());
    }
    if let Command::Relay {
        terminal,
        session,
        command,
    } = cli.command
    {
        return relay_command(terminal, session, command).await;
    }
    if let Command::View { host: None } = cli.command {
        return view_command().await;
    }
    if let Command::View { host: Some(host) } = &cli.command {
        let (server, _) = server_of(cli.server.as_deref())?;
        let here = identity::place(&identity::working_dir()?)?;
        let (api, me) = daemon_asker(&here, &server)?;
        return riff::screen::view_host(&api, &me, host).await;
    }
    if let Command::Takeover { name, at_end } = &cli.command {
        let code = riff::takeover::here(name, *at_end).await?;
        if code != 0 {
            std::process::exit(code);
        }
        return Ok(());
    }
    let (server, source) = server_of(cli.server.as_deref())?;
    // In a container, the socket of the session reaches riff-server
    // (01M5GVPNAKAPA596K9361HSAAV).
    let reach = Reach::of(&server, source, riff::session_relay::socket_here().ok());
    if let Command::Server = cli.command {
        let view = lifecycle::view(&server, DEFAULT_SERVER, source).await;
        anstream::println!("{}", text::server_view(&view));
        return Ok(());
    }
    if let Command::Update {
        tag,
        auto,
        check,
        background,
        cargo,
    } = &cli.command
    {
        if *check {
            std::process::exit(update_check().await?);
        }
        if let Some(auto) = auto {
            let path = settings::path()?;
            if let Some(switch) = auto {
                settings::set_update_auto(&path, matches!(switch, Switch::On))?;
            }
            anstream::println!(
                "{}",
                view::auto_update(settings::update_auto(&path)?, &path)
            );
            return Ok(());
        }
        if let (true, Some(tag)) = (background, tag) {
            return riff::auto_update::run(cargo, tag, &server, DEFAULT_SERVER, cli.place.as_ref())
                .await;
        }
        let words = lifecycle::update(cargo, tag.as_deref(), &server, DEFAULT_SERVER).await?;
        for line in lifecycle::daemon_after_update().await {
            println!("{line}");
        }
        println!("{words}");
        return Ok(());
    }
    if let Command::Usage { issue, wave, total } = &cli.command {
        print!("{}", usage_text(issue.as_deref(), wave.as_deref(), *total)?);
        return Ok(());
    }
    if let Command::Setup { check } = cli.command {
        return setup(check);
    }
    if let Command::Build {
        host: Some(host), ..
    } = &cli.command
    {
        let request = riff::daemon::remote::Request::Build;
        let here = identity::place(&identity::working_dir()?)?;
        return ask_daemon(host, &request, &here, &reach).await;
    }
    if let Command::Build { just, .. } = &cli.command {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .context("riff cannot find your home: HOME is not set")?;
        let check = riff::check::dir_from(
            std::env::var_os(riff::check::VAR),
            std::path::Path::new(&home),
        );
        let riff = riff::check::build(&identity::working_dir()?, &check, just)?;
        println!("{}", riff.display());
        return Ok(());
    }
    if let Command::Cloud { command } = &cli.command {
        return cloud(command, &server).await;
    }
    if let Command::Release {
        cycle: Some(command),
        ..
    } = &cli.command
    {
        return release_cycle(command, &server, &reach).await;
    }
    if let Command::Pr {
        command: Pr::Wait { number, every },
    } = &cli.command
    {
        eprintln!("{}", text::pr_waits(*number));
        let every = Duration::from_secs(*every);
        println!("{}", pr::wait(&pr::Gh::default(), *number, every)?);
        // The merge is a fact of the item (01M41FZP2C4Z4J6WKRXZ5B31EH).
        // It never fails the wait.
        let api = Api::new(&server);
        let me =
            identity::here(cli.place.as_ref()).and_then(|here| identity::me(&here, api.base()));
        if let (Ok(me), Ok(issue)) = (me, pr::issue_of(&pr::Gh::default(), *number)) {
            riff::look::tell_fact(&api, &me, issue, *number, PullState::Merged).await;
        }
        // The merge is done: the total never fails the wait.
        match total_after_merge(*number) {
            Ok(line) => eprintln!("{line}"),
            Err(e) => eprintln!("riff: cannot write the total of the tokens: {e:#}"),
        }
        return Ok(());
    }
    if let Command::Wave { command } = &cli.command {
        let gh = pr::Gh::default();
        let line = match command {
            WaveCommand::Make { title } => {
                let made = riff::waves::make(&gh, &title.join(" "))?;
                Some(text::waves_made(&made.title))
            }
            WaveCommand::Rename { number, name } => {
                let (old, new) = riff::waves::rename(&gh, *number, &name.join(" "))?;
                Some(text::waves_renamed(&old.title, &new.title))
            }
            WaveCommand::End {
                number,
                no_release,
                move_open_to_backlog: false,
            } => {
                wave_release_gate(*number, no_release.as_deref())?;
                let ended = riff::waves::end(&gh, *number)?;
                Some(text::waves_ended(&ended.title, ended.closed_issues))
            }
            WaveCommand::List
            | WaveCommand::Run { .. }
            | WaveCommand::Stop { .. }
            | WaveCommand::Move { .. }
            | WaveCommand::End { .. } => None,
        };
        if let Some(line) = line {
            println!("{line}");
            return Ok(());
        }
    }
    let api = Api::new(&server);
    match &cli.command {
        Command::Login => {
            let sign_in = login::login(&api, open_browser).await?;
            println!("{}", text::signed_in(&sign_in.user, api.base()));
            ask_auto_update();
            return Ok(());
        }
        Command::Logout { all: false, .. } => {
            let had = riff_client::login::logout(api.base())?;
            println!("{}", text::signed_out(had, api.base()));
            return Ok(());
        }
        Command::Logout { all: true, user } => {
            let done = riff_client::login::logout_all(&api, user.as_deref()).await?;
            println!("{}", text::revoked(&done));
            return Ok(());
        }
        Command::Invite { email } => {
            let me = person(&api)?;
            let changed = api.signed_in(None)?.invite(&me, email).await?;
            println!("{}", text::invited(&changed.done));
            print_members_news(&changed.news);
            return Ok(());
        }
        Command::Remove { email } => {
            let me = person(&api)?;
            let changed = api.signed_in(None)?.remove(&me, email).await?;
            println!("{}", text::removed(&changed.done));
            print_members_news(&changed.news);
            return Ok(());
        }
        Command::Chat { after } => {
            let me = person(&api)?;
            return riff::chat::run(&api.signed_in(None)?, &me, *after).await;
        }
        Command::Members => {
            anstream::print!("{}", view::members(&api.signed_in(None)?.members().await?));
            return Ok(());
        }
        Command::Audit { wave } => {
            let here = identity::place(&identity::working_dir()?)?;
            let repo = audit::repo_of(&here)?;
            let gh = pr::Gh::default();
            let wave = riff::waves::title(&gh, &here.repo_text(), wave)?;
            let log = api.signed_in(None)?.log(&repo).await?;
            let forge = audit::Forge::read(&gh, &here.repo_text(), &wave)?;
            let report = audit::audit(&wave, &log.records, log.pruned, &forge, audit::now_ms())?;
            print!("{}", audit::render(&report, &here.repo_text()));
            if report.failed() {
                std::process::exit(1);
            }
            return Ok(());
        }
        Command::Admin { command } => {
            let (email, admin) = match command {
                Admin::Add { email } => (email, true),
                Admin::Remove { email } => (email, false),
            };
            let me = person(&api)?;
            let changed = api.signed_in(None)?.set_admin(&me, email, admin).await?;
            println!("{}", text::admin_set(&changed.done));
            print_members_news(&changed.news);
            return Ok(());
        }
        Command::Owner {
            email: Some(email), ..
        } => {
            let me = person(&api)?;
            let changed = api.signed_in(None)?.pass_owner(&me, email).await?;
            println!("{}", text::owner_passed(&changed.done));
            print_members_news(&changed.news);
            return Ok(());
        }
        Command::Owner { take: true, .. } => {
            let asked = api.signed_in(None)?.take_owner().await?;
            println!("{}", text::owner_asked(&asked));
            return Ok(());
        }
        Command::Owner { .. } => {
            let denied = api.signed_in(None)?.deny_owner().await?;
            println!("{}", text::owner_denied(&denied));
            return Ok(());
        }
        _ => {}
    }
    let here = identity::here(cli.place.as_ref())?;
    riff::auto_update::remember(&here);
    // `riff run` is a session of its own, not the session that runs it
    // (01M4Z81APKC14AVNJWCGRQKNVW).
    if let Command::Run { item, image } = &cli.command {
        let server = api.base().to_owned();
        let code = riff::run::run(api, &here, &server, item, image.as_deref()).await?;
        std::process::exit(code);
    }
    if let Command::Lead {
        command: Some(LeadCommand::Start),
        ..
    } = &cli.command
    {
        let server = api.base().to_owned();
        return riff::lead::start(api, &here, &server).await;
    }
    if let Command::Lead {
        command: Some(LeadCommand::Move { host }),
        ..
    } = &cli.command
    {
        let (api, me) = daemon_asker(&here, api.base())?;
        match riff::lead::move_to(&api, &me, host, riff::lead::HANDOVER_WAIT).await {
            Ok(session) => {
                println!("{}", text::lead_move_done(host, &session));
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
        }
    }
    if let Command::Lead {
        command:
            Some(LeadCommand::Notes {
                command,
                thread,
                history,
                user,
            }),
        ..
    } = &cli.command
    {
        let thread = thread_or_default(thread.clone(), &here)?;
        let me = person(&api)?;
        let file = command
            .as_ref()
            .map(|NotesCommand::Edit { file }| file.as_deref());
        return riff::lead::notes(&api, &me, &thread, user.as_deref(), *history, file).await;
    }
    if let Command::Wave { command } = &cli.command {
        let dir = riff::daemon::dir()?;
        match command {
            WaveCommand::Run { host: Some(host) } => {
                let request = riff::daemon::remote::Request::WaveRun;
                ask_daemon(host, &request, &here, &reach).await?;
            }
            WaveCommand::Stop { host: Some(host) } => {
                let request = riff::daemon::remote::Request::WaveStop;
                ask_daemon(host, &request, &here, &reach).await?;
            }
            WaveCommand::Run { host: None } => riff::wave::ask_run(&dir, &here, api.base()).await?,
            WaveCommand::Stop { host: None } => riff::wave::ask_stop(&dir, &here).await?,
            WaveCommand::List => wave_list(&reach, &here).await?,
            WaveCommand::Move { items, to } => wave_move(&reach, &here, items, to).await?,
            WaveCommand::End {
                number,
                no_release,
                move_open_to_backlog: true,
            } => wave_end_moving(&reach, &here, *number, no_release.as_deref()).await?,
            WaveCommand::Make { .. } | WaveCommand::Rename { .. } | WaveCommand::End { .. } => {
                unreachable!("handled before the identity")
            }
        }
        return Ok(());
    }
    let me = identity::me(&here, api.base())?;
    // A session that left makes no call (01M3MEEFETT9A0DRWBKQTG77Z2).
    if let Some(id) = me.who().session()
        && local::left_here(id)
    {
        anyhow::bail!(riff_client::text::LEFT_COMMAND);
    }
    let api = api.signed_in(me.who().session())?;
    match cli.command {
        Command::Whoami => {
            let pauses = api.pauses(&me).await.map_err(|e| format!("{e:#}"));
            // The URI of the server holds the claims and the lead
            // (01M517N7ANC06S4GTEEZHAB255).
            let shown = match api.me(&me).await {
                Ok(reply) => reply.session.map_or(me, |s| s.uri),
                Err(_) => me,
            };
            anstream::print!("{}", view::whoami(&shown, pauses));
        }
        Command::Who { all, long } => {
            let pauses = api.pauses(&me).await?;
            let mut who = api.roster(&me, all).await?;
            riff::state::fill(&mut who.sessions, pauses.state);
            anstream::print!(
                "{}",
                view::who(&pauses, &who.owner, &who.sessions, &me, long)
            );
        }
        Command::Pause { riff, repo } => {
            pause(
                &api,
                &me,
                &here,
                pause_scope(riff, repo)?,
                RiffState::Paused,
            )
            .await?;
        }
        Command::Resume { riff, repo } => {
            pause(
                &api,
                &me,
                &here,
                pause_scope(riff, repo)?,
                RiffState::Running,
            )
            .await?;
        }
        Command::Post {
            thread,
            to,
            kind,
            body,
        } => {
            if kind.needs_body() && body.is_empty() {
                anyhow::bail!("give the message. Only a post with --kind status needs none.");
            }
            let thread = thread_or_default(thread, &here)?;
            let posted = api
                .post(&me, Some(&thread), &to, &body.join(" "), kind)
                .await?;
            println!("{}", text::posted(&posted));
        }
        Command::Status { step } => {
            let status = Status {
                step: step.join(" "),
            };
            api.status(&me, &status).await?;
            println!("{}", text::status_set(&status));
        }
        Command::Blocked { reason } => {
            let reason = reason.join(" ");
            let told = api.blocked(&me, &reason).await?;
            println!("{}", text::blocked_set(&reason, told));
        }
        Command::Step { change } => {
            let change = match change {
                StepCommand::Start { name } => StepChange::Start {
                    name: name.join(" "),
                },
                StepCommand::Done => StepChange::Done,
                StepCommand::Fail { reason } => StepChange::Fail {
                    reason: reason.join(" "),
                },
            };
            let told = api.step(&me, change.clone()).await?;
            println!("{}", text::step_set(&change, told));
        }
        Command::Tell { session, body } => {
            let posted = api.tell(&me, &session, &body.join(" ")).await?;
            println!("{}", text::posted(&posted));
        }
        Command::Read { thread, all } => {
            if let Some(repo) = here.default_thread() {
                api.join(&me, &repo).await?;
            }
            let thread = match thread {
                Some(t) => Some(api.thread_to_read(&me, &t).await?),
                None => None,
            };
            let inbox = api.inbox(&me, thread.as_ref(), all).await?;
            println!("{}", text::inbox(&inbox, &me).trim_end());
        }
        Command::Session {
            command: SessionCommand::Why { item },
        } => {
            let records = riff::ends::read(&api, &me).await?;
            println!("{}", riff::ends::why(&item, &records));
        }
        Command::Denials { since, format } => {
            let span = since.as_deref().map(riff::denied::since).transpose()?;
            let from_ms = span.map_or(0, |span| {
                usage::now_ms().saturating_sub(span.as_millis() as u64)
            });
            let records = riff::denied::read(&api, &me, from_ms).await?;
            match format {
                DenialsFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&records)?);
                }
                DenialsFormat::Text => {
                    let span = match &since {
                        Some(since) => format!("in the last {since}"),
                        None => format!("in the thread {}", riff::denied::THREAD),
                    };
                    print!("{}", riff::denied::report(&records, &span));
                }
            }
        }
        Command::Claim { thread, item } => {
            let thread = thread_or_default(thread, &here)?;
            let reply = api.claim(&me, &thread, &item).await?;
            println!("{}", text::claimed(&reply, &thread, &item));
            if !reply.granted {
                std::process::exit(1);
            }
            if let Some((meter, id)) = usage::Meter::here().zip(me.who().session()) {
                meter.started(id, &thread.to_string(), &item);
            }
            let dir = identity::working_dir()?;
            if let Some(line) = dropped::at_claim(&dir, &item).await {
                println!("{line}");
            }
            let repo = me.place().repo_text();
            if let Some(line) = riff::rollout::at_claim(&dir, &repo, &item).await {
                println!("{line}");
            }
        }
        Command::Plan {
            command:
                PlanCommand::Hold {
                    thread,
                    item,
                    reason,
                },
        } => {
            let thread = thread_or_default(thread, &here)?;
            let reply = api.hold(&me, &thread, &item, &reason.join(" ")).await?;
            println!("{}", text::held(&reply, &thread, &item));
        }
        Command::Plan {
            command: PlanCommand::Free { thread, item },
        } => {
            let thread = thread_or_default(thread, &here)?;
            let reply = api.free(&me, &thread, &item).await?;
            println!("{}", text::freed(reply, &thread, &item));
        }
        Command::Release {
            thread,
            session,
            item: Some(item),
            cycle: None,
        } => {
            let thread = thread_or_default(thread, &here)?;
            match session {
                Some(holder) => {
                    api.release_for(&me, &thread, &item, &holder).await?;
                    println!("{}", text::released_for(&thread, &item, &holder));
                }
                None => {
                    let reply = api.release(&me, &thread, &item).await?;
                    println!("{}", text::released(&thread, &item, reply));
                    let line = usage::Meter::here()
                        .zip(me.who().session())
                        .and_then(|(meter, id)| meter.release(id, &thread.to_string(), &item));
                    if let Some(line) = line {
                        println!("{line}");
                    }
                }
            }
        }
        Command::Lead {
            command: None,
            session,
        } => {
            let Some(session) = session else {
                anyhow::bail!(text::LEAD_NAME_SESSION);
            };
            // A session sends its own URI: riff-server refuses it
            // (01M5NJAXTQ0YKN7Q7XN0A7ZMPF). A person sends the repository.
            let me = match me.who().session() {
                Some(_) => me,
                None => identity::person(&here, api.base())?,
            };
            println!("{}", text::led(&api.lead(&me, &session).await?));
        }
        Command::Pr {
            command:
                Pr::Open {
                    title,
                    file,
                    refs,
                    issue,
                },
        } => {
            let issue = match issue {
                Some(n) => n,
                None => {
                    let sessions = api.who(&me, false).await?;
                    let mine = sessions.iter().find(|s| s.uri.who() == me.who());
                    pr::claimed_issue(mine.map_or(&[][..], |s| s.uri.claims()))?
                }
            };
            let summary = match file {
                Some(file) => std::fs::read_to_string(&file)
                    .with_context(|| format!("cannot read {}", file.display()))?,
                None => String::new(),
            };
            let (number, url) = pr::open(&pr::Gh::default(), &title, &summary, issue, refs)?;
            println!("{}", text::pr_opened(number, &url));
            riff::look::tell_fact(&api, &me, issue, number, PullState::Asked).await;
        }
        Command::Verify {
            verdict,
            number,
            file,
            commit,
        } => {
            let result = std::fs::read_to_string(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            let tested = match commit {
                Some(commit) => commit,
                None => pr::head_here(&identity::working_dir()?)?,
            };
            let thread = thread_or_default(None, &here)?;
            let verdict = verdict.into();
            let reported = pr::report(
                &pr::Gh::default(),
                &thread.to_string(),
                number,
                &tested,
                verdict,
                &result,
            )?;
            println!("{}", text::verify_reported(verdict, number, &reported));
            let state = match verdict {
                pr::Verdict::Pass => PullState::Passed,
                pr::Verdict::Fail => PullState::Failed,
            };
            riff::look::tell_fact(&api, &me, reported.issue, number, state).await;
            let to = pr::result_to(reported.issue, &me, &api.who(&me, false).await?);
            let body = text::verify_post(verdict, number, &reported, &result);
            let posted = api
                .post(&me, Some(&thread), &to, &body, Kind::Message)
                .await?;
            println!("{}", text::posted(&posted));
        }
        Command::Tail { thread } => {
            tail(&api, &me, &thread_or_default(thread, &here)?, &here).await;
        }
        Command::Top {
            once,
            user,
            host,
            repo,
            by,
        } => {
            let show = riff_client::sessions::Show {
                user,
                host,
                repo,
                by: by.into(),
            };
            top(&api, &me, here.default_thread(), &show, once).await?;
        }
        Command::Usage { .. }
        | Command::Setup { .. }
        | Command::Build { .. }
        | Command::Server
        | Command::Update { .. }
        | Command::Cloud { .. }
        | Command::ClaudeToken
        | Command::Release { .. }
        | Command::Image
        | Command::Daemon { .. }
        | Command::Init { .. }
        | Command::Relay { .. }
        | Command::Lead {
            command: Some(LeadCommand::Start | LeadCommand::Notes { .. } | LeadCommand::Move { .. }),
            ..
        }
        | Command::View { .. }
        | Command::Takeover { .. }
        | Command::Pr {
            command: Pr::Wait { .. },
        }
        | Command::Login
        | Command::Logout { .. }
        | Command::Invite { .. }
        | Command::Remove { .. }
        | Command::Members
        | Command::Audit { .. }
        | Command::Chat { .. }
        | Command::Admin { .. }
        | Command::Owner { .. }
        | Command::Run { .. }
        | Command::Wave { .. } => unreachable!("handled before the identity"),
    }
    Ok(())
}

/// The person on this host, with no session and no repository. It posts
/// the note of a change of the members (01M3MN14ZCTRVD3T455P6TFK1B).
fn person(api: &Api) -> Result<SessionUri> {
    let here = identity::place(&identity::working_dir()?)?;
    identity::person(&Place::host_only(here.host())?, api.base())
}

/// Prints where the note of a change of the members went, or the error
/// of its post on stderr (01M3MN1537Z0K3BRK6H2BZKZT0).
fn print_members_news(news: &Result<Vec<riff_core::wire::Posted>>) {
    match news {
        Ok(_) => println!("{}", text::members_news(news)),
        Err(_) => anstream::eprintln!("{}", text::members_news(news)),
    }
}

/// Shows the authorize URL of a sign-in, and opens it in the browser.
fn open_browser(url: &str) {
    eprintln!("riff: sign in with your browser. If it does not open, go to:\n{url}");
    let _ = open::that_detached(url);
}

/// `riff daemon` and its subcommands. With `--host`, riff asks the
/// daemon of that host through the riff-server of `server`
/// ([`riff::daemon::remote`]).
async fn daemon_command(command: Option<&DaemonCommand>, server: Option<&str>) -> Result<()> {
    if let Some(DaemonCommand::Events {
        name,
        host: Some(host),
    }) = command
    {
        let (server, _) = server_of(server)?;
        let here = identity::place(&identity::working_dir()?)?;
        return daemon_events(host, name, &here, &server).await;
    }
    if let Some((host, request)) = command.and_then(remote_request) {
        let (server, source) = server_of(server)?;
        let here = identity::place(&identity::working_dir()?)?;
        let reach = Reach::of(&server, source, riff::session_relay::socket_here().ok());
        return ask_daemon(host, &request, &here, &reach).await;
    }
    let Some(command) = command else {
        return riff::daemon::serve_here().await;
    };
    let dir = riff::daemon::dir()?;
    match command {
        DaemonCommand::Install => {
            let manager = riff::service::Systemd::here()?;
            let unit = riff::service::Unit::of_this()?;
            let wait = Duration::from_secs(10);
            riff::daemon::install(&manager, &unit, &dir, wait).await
        }
        DaemonCommand::Uninstall => {
            let manager = riff::service::Systemd::here()?;
            println!(
                "{}",
                riff::daemon::uninstall(&manager, &manager.unit_path())?
            );
            Ok(())
        }
        DaemonCommand::Status { .. } => {
            if let Ok(socket) = riff::session_relay::socket_here() {
                return status_in_container(&socket).await;
            }
            let manager = riff::service::Systemd::here()?;
            let code = riff::daemon::show_status(&dir, &manager).await;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        DaemonCommand::Events { name, .. } => {
            let code = riff::daemon::show_events(&dir, name).await?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        DaemonCommand::Limit { .. } => {
            let change = limit_change(command).unwrap_or_default();
            let limit = riff::room::set(&dir, change)?;
            println!("{}", riff::text::daemon_limit(&limit));
            Ok(())
        }
        DaemonCommand::Cache { size, clear, .. } => {
            println!(
                "{}",
                riff::compile_cache::change(&dir, size.as_deref(), *clear)?
            );
            Ok(())
        }
        DaemonCommand::Sweep { .. } => riff::daemon::ask_sweep(&dir).await,
        DaemonCommand::End { name, .. } => riff::daemon::ask_end(&dir, name).await,
        DaemonCommand::Restart { .. } => {
            let ask = riff::daemon::Ask::Restart;
            riff::daemon::ask_done(&dir, ask, |line| println!("{line}")).await
        }
        DaemonCommand::Mcp { command } => {
            use riff::session_mcp;
            let names = match command {
                None => session_mcp::names(&session_mcp::read(&dir)?),
                Some(DaemonMcp::Add { name }) => {
                    let here = identity::working_dir()?;
                    let person = riff::session_mcp::person_servers(
                        riff::session_mcp::claude_json().as_deref(),
                        &here,
                    );
                    session_mcp::add(&dir, name, &person, |program| {
                        let image = riff::session_image::of_folder(&here)?;
                        session_mcp::has_program(&image.to_string(), program)
                    })?
                }
                Some(DaemonMcp::Remove { name }) => session_mcp::remove(&dir, name)?,
            };
            println!("{}", riff::text::session_mcp(&names));
            Ok(())
        }
        DaemonCommand::Host => {
            let (server, _) = server_of(server)?;
            let work = identity::working_dir()?;
            let here = identity::place(&work)?;
            riff::daemon::ask_serve(&dir, &work, &here, &server).await
        }
    }
}

/// `riff daemon status` in a container: asks the daemon on the socket of
/// the session (#1239). It looks for no socket and no unit of the host.
async fn status_in_container(socket: &std::path::Path) -> Result<()> {
    match Reach::daemon_status(socket).await {
        Ok(status) => {
            print!("{}", text::daemon_status(&status, riff::daemon::now_ms()));
            Ok(())
        }
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}

/// The change of `riff daemon limit` from its flags.
fn limit_change(command: &DaemonCommand) -> Option<riff::room::Change> {
    let DaemonCommand::Limit {
        sessions,
        floor,
        cpus,
        memory,
        ..
    } = command
    else {
        return None;
    };
    Some(riff::room::Change {
        sessions: *sessions,
        floor_gb: *floor,
        cpus: *cpus,
        memory_gb: *memory,
    })
}

/// The host and the request of a daemon command with `--host`
/// (01M579Z9MPS86MX7890T6HV18S).
fn remote_request(command: &DaemonCommand) -> Option<(&str, riff::daemon::remote::Request)> {
    use riff::daemon::remote::Request;
    let (host, request) = match command {
        DaemonCommand::Status { host } => (host, Request::Status),
        // `daemon_command` follows the events of another host.
        DaemonCommand::Events { .. } => return None,
        DaemonCommand::Limit { host, .. } => (host, Request::Limit(limit_change(command)?)),
        DaemonCommand::Cache { size, clear, host } => (
            host,
            Request::Cache {
                size: size.clone(),
                clear: *clear,
            },
        ),
        DaemonCommand::Sweep { host } => (host, Request::Sweep),
        DaemonCommand::End { name, host } => (host, Request::End(name.clone())),
        DaemonCommand::Restart { host } => (host, Request::Restart),
        DaemonCommand::Install
        | DaemonCommand::Uninstall
        | DaemonCommand::Host
        | DaemonCommand::Mcp { .. } => return None,
    };
    Some((host.as_deref()?, request))
}

/// The client and the caller of a request to the daemon of another
/// host, in the repository of the place `here`: the agent session of
/// this process, or else the person.
fn daemon_asker(here: &Place, server: &str) -> Result<(Api, SessionUri)> {
    let api = Api::new(server);
    let me = match identity::session_id() {
        Some(id) => identity::agent(here, &id, api.base())?,
        None => identity::person(here, api.base())?,
    };
    let api = api.signed_in(me.who().session())?;
    Ok((api, me))
}

/// Asks the daemon of `host` for `request` through riff-server, in the
/// repository of the place `here`, and prints its reply
/// (01M579Z9MPS86MX7890T6HV18S). It exits with 1 when the daemon
/// refused or failed.
async fn ask_daemon(
    host: &str,
    request: &riff::daemon::remote::Request,
    here: &Place,
    reach: &Reach,
) -> Result<()> {
    let answer = match reach {
        Reach::Direct(server) => {
            let (api, me) = daemon_asker(here, server)?;
            let wait = riff::daemon::remote::REPLY_WAIT;
            riff::daemon::remote::ask(&api, &me, host, request, wait).await
        }
        Reach::Daemon(_) => reach.host(host, request).await,
    };
    match answer {
        Ok(reply) => {
            println!("{reply}");
            Ok(())
        }
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}

/// `riff daemon events NAME --host HOST`: follows the session NAME of
/// the daemon of `host` up to its end, as on that host
/// (01M57D4FX5VHQR150QTDS6BA1C). It exits with 1 when the session
/// failed, or the daemon refused.
async fn daemon_events(host: &str, name: &str, here: &Place, server: &str) -> Result<()> {
    let (api, me) = daemon_asker(here, server)?;
    let show = |line: &str| println!("{line}");
    match riff::daemon::remote::follow_events(&api, &me, host, name, show).await {
        Ok(false) => Ok(()),
        Ok(true) => std::process::exit(1),
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}

/// `riff update --check` on this machine, in the working directory
/// (01M51QFPYHK0916C5ZN2B55BR0). It gives the exit code.
async fn update_check() -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        let (daemon, here) = (riff::daemon::dir()?, identity::working_dir()?);
        Ok(riff::update_steps::check(&riff::podman::Podman, &daemon, &here).await)
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(text::RUN_LINUX_ONLY)
    }
}

/// `riff view` on this machine: the daemon of [`riff::daemon::dir`] and
/// rootless podman.
async fn view_command() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        riff::screen::view(&riff::daemon::dir()?, &riff::podman::Podman).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(text::RUN_LINUX_ONLY)
    }
}

/// The longest wait of `riff relay prompt` for the daemon: the prompt
/// of the person waits for the hook.
const PROMPT_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// `riff relay`: the riff in a session of the riff daemon
/// ([`riff::session_relay`]). It ends with the exit code of the command
/// of the session.
async fn relay_command(
    terminal: bool,
    session: Vec<String>,
    command: Option<RelayCommand>,
) -> Result<()> {
    match command {
        Some(RelayCommand::Mcp) => {
            let socket = riff::session_relay::socket_here()?;
            riff::session_relay::mcp(&socket, tokio::io::stdin(), tokio::io::stdout()).await
        }
        Some(RelayCommand::Watch { follow }) => {
            let socket = riff::session_relay::socket_here()?;
            if follow {
                return riff::session_relay::follow(&socket, |line| println!("{line}")).await;
            }
            println!("{}", riff::session_relay::watch(&socket).await?);
            Ok(())
        }
        Some(RelayCommand::Denied { send }) => {
            let Some(queue) = riff::denied::Queue::here() else {
                return Ok(());
            };
            if send {
                let socket = riff::session_relay::socket_here()?;
                if let Err(e) = riff::session_relay::send_denials(&socket, &queue).await {
                    eprintln!("riff: cannot send the refusals of auto mode: {e:#}");
                }
                return Ok(());
            }
            let mut stdin = String::new();
            let _ = std::io::stdin().read_to_string(&mut stdin);
            if let Err(e) = riff::denied::keep_input(&stdin, &queue, false) {
                eprintln!("riff: cannot keep the refusal of auto mode: {e:#}");
                return Ok(());
            }
            start_detached(&["relay", "denied", "--send"]);
            Ok(())
        }
        Some(RelayCommand::Prompt) => {
            // The hook input says nothing that riff needs. Each line on
            // stdout would go into the prompt.
            let mut stdin = String::new();
            let _ = std::io::stdin().read_to_string(&mut stdin);
            let told = async {
                let socket = riff::session_relay::socket_here()?;
                riff::session_relay::prompt(&socket).await
            };
            match tokio::time::timeout(PROMPT_WAIT, told).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("riff: cannot tell the daemon of the prompt: {e:#}"),
                Err(_) => eprintln!("riff: the daemon did not take the prompt in time"),
            }
            Ok(())
        }
        None if terminal => Err(riff::session_relay::terminal(&session)),
        None => {
            let code = riff::session_relay::main(session).await?;
            std::process::exit(code)
        }
    }
}

/// `riff cloud` (01M4262DQ9RNFNJ07CRTSGEAM1). A write refuses before
/// each call of `gcloud` in a worker and in an agent session that is not
/// the lead (01M558C4VZ2PSMEYPAZBRYW1VV).
async fn cloud(command: &CloudCommand, server: &str) -> Result<()> {
    if command.writes()
        && let Some(why) = lead_only(server, &text::CLOUD).await
    {
        eprintln!("{why}");
        std::process::exit(1);
    }
    let gcloud = cloud::Gcloud::default();
    let here = identity::working_dir()?;
    let dir = cloud::dir(&here)?;
    let owner = std::env::var("RIFF_OWNER").ok().filter(|o| !o.is_empty());
    match command {
        CloudCommand::Create {
            name,
            project,
            region,
        } => {
            let s = cloud::settings_for_create(
                &gcloud,
                &dir,
                name,
                project.as_deref(),
                region.as_deref(),
            )?;
            cloud::create(&gcloud, &s, owner.as_deref())
        }
        CloudCommand::Signin { name } => {
            let s = cloud::load(&dir, name)?;
            println!("{}", text::cloud_signin_steps(&s.project));
            let id = cloud::read_line("Client ID: ")?;
            let secret = cloud::read_hidden("Client secret (hidden): ")?;
            cloud::signin(&gcloud, &s, &cloud::file(&dir, name), &id, &secret)
        }
        CloudCommand::Deploy { name, tag, confirm } => {
            let s = cloud::load(&dir, name)?;
            let source = match tag {
                Some(tag) if cloud::release_tag(tag) => cloud::Source::Tag(tag.clone()),
                Some(tag) if cloud::commit_tag(tag) && s.confirm => {
                    anyhow::bail!(text::cloud_commit_needs_stage(name))
                }
                Some(tag) if cloud::commit_tag(tag) => cloud::Source::Tag(tag.clone()),
                Some(tag) => anyhow::bail!(text::cloud_bad_tag(tag)),
                None => {
                    let top = cloud::top(&here).filter(|t| t.join("Dockerfile").is_file());
                    cloud::Source::Tree(top.context(text::CLOUD_NO_TREE)?)
                }
            };
            let owner = owner.context(text::CLOUD_NO_OWNER)?;
            if s.confirm {
                cloud::confirm(name, confirm.as_deref(), "deploys to")?;
            }
            cloud::deploy(&gcloud, &s, &source, &owner)
        }
        CloudCommand::Smoke { name } => {
            let s = cloud::load(&dir, name)?;
            let token = std::env::var(riff::smoke::TOKEN_VAR)
                .ok()
                .filter(|t| !t.is_empty())
                .context(text::SMOKE_NO_TOKEN)?;
            riff::smoke::run(&Api::new(&s.url), &token, |line| println!("{line}")).await?;
            println!("{}", text::smoke_passed(name));
            Ok(())
        }
        CloudCommand::List => {
            let all = cloud::all(&dir)?;
            if all.is_empty() {
                println!("{}", text::cloud_none(&dir));
            }
            for s in all {
                let facts = cloud::facts(&gcloud, &s)?;
                let paused = riff_paused(&s.url, &here).await;
                println!("{}", text::cloud_row(&s.name, &s.url, &facts, paused));
            }
            Ok(())
        }
        CloudCommand::Status { name } => {
            let s = cloud::load(&dir, name)?;
            let facts = cloud::facts(&gcloud, &s)?;
            let paused = riff_paused(&s.url, &here).await;
            println!("{}", text::cloud_row(&s.name, &s.url, &facts, paused));
            print!("{}", text::cloud_status(&s, &facts));
            Ok(())
        }
        CloudCommand::Log {
            name,
            errors,
            limit,
            filter,
        } => {
            let s = cloud::load(&dir, name)?;
            gcloud.show(&cloud::log_args(&s, *errors, *limit, filter.as_deref()))
        }
        CloudCommand::Delete {
            name,
            with_state,
            confirm,
        } => {
            let s = cloud::load(&dir, name)?;
            cloud::confirm(name, confirm.as_deref(), "deletes the service of")?;
            cloud::delete(&gcloud, &s, *with_state)
        }
    }
}

/// Whether the riff at `url` is paused, as this machine sees it with
/// its sign-in. `None` when riff cannot tell, for example with no
/// sign-in to that riff.
async fn riff_paused(url: &str, here: &std::path::Path) -> Option<bool> {
    let api = Api::new(url);
    let place = identity::place(here).ok()?;
    let me = identity::person(&place, api.base()).ok()?;
    let state = api.signed_in(None).ok()?.riff(&me).await.ok()?;
    Some(state == RiffState::Paused)
}

/// Why this process may not run the command of `refusals`, or `None`
/// when it may (01M558C4VZ2PSMEYPAZBRYW1VV). `riff workers start` and
/// the writes of `riff cloud` use it.
/// A person in a plain terminal may. A worker may not. An agent session
/// may only when it is the lead.
async fn lead_only(server: &str, refusals: &text::LeadOnly) -> Option<String> {
    if std::env::var("RIFF_WORKER").is_ok_and(|v| v == "1") {
        return Some(refusals.worker.into());
    }
    let id = identity::session_id()?;
    match is_lead(server, &id).await {
        Ok(true) => None,
        Ok(false) => Some(refusals.not_the_lead.into()),
        Err(_) => Some(refusals.lead_unknown.into()),
    }
}

/// True when the agent session `id` is the lead of its user, as
/// riff-server sees it.
async fn is_lead(server: &str, id: &str) -> Result<bool> {
    Ok(lead_and_role(server, Some(id)).await?.0)
}

/// Whether the agent session `id` is the lead of its user, and the role
/// of its user in the riff. With no `id`: the role of the person on this
/// machine. The role is `None` in a riff with no sign-in, which has no
/// people (01M58B0JSQMWNCMSNX19536XT8). A user with no row is a member.
async fn lead_and_role(
    server: &str,
    id: Option<&str>,
) -> Result<(bool, Option<riff_core::wire::PersonRole>)> {
    use riff_core::wire::PersonRole;
    let here = identity::place(&identity::working_dir()?)?;
    let api = Api::new(server);
    let me = match id {
        Some(id) => identity::agent(&here, id, api.base())?,
        None => identity::person(&here, api.base())?,
    };
    let who = api.signed_in(id)?.roster(&me, false).await?;
    let lead = id.is_some()
        && who
            .sessions
            .iter()
            .any(|s| s.uri.who() == me.who() && s.uri.lead());
    let role = (!who.people.is_empty()).then(|| {
        who.people
            .iter()
            .find(|p| p.user == me.who().user())
            .map_or(PersonRole::Member, |p| p.role)
    });
    Ok((lead, role))
}

/// The caller of a command of the release cycle: a worker, a container
/// session, an agent session, whether it is the lead, and the role of
/// its user (01M58B0JSQMWNCMSNX19536XT8). riff asks riff-server for the
/// lead and the role only when `ask_role` is true: for an agent session
/// with a session ID, and for a person in a terminal.
async fn release_caller(server: &str, ask_role: bool) -> riff::cycle::Caller {
    let worker = std::env::var(riff::identity::WORKER).is_ok_and(|v| v == "1");
    let container = riff::cycle::in_container();
    let agent = riff::cycle::in_agent();
    let mut caller = riff::cycle::Caller {
        worker,
        container,
        agent,
        ..riff::cycle::Caller::default()
    };
    let id = identity::session_id();
    if container && !worker {
        // The daemon knows by the socket of the session whether it is
        // the lead (01M59269HHZ5C82DR3RG3QHAM3).
        if let Ok(socket) = riff::session_relay::socket_here()
            && let Ok(riff::daemon::Say::Caller {
                lead: true,
                role,
                role_unknown,
            }) = riff::session_relay::ask_caller(&socket).await
        {
            caller.lead_container = true;
            caller.lead = Some(true);
            caller.role = role;
            caller.role_unknown = role_unknown;
        }
        return caller;
    }
    if !ask_role || worker || (agent && id.is_none()) {
        return caller;
    }
    match lead_and_role(server, id.as_deref()).await {
        Ok((lead, role)) => {
            caller.lead = agent.then_some(lead);
            caller.role = role;
        }
        Err(_) => caller.role_unknown = true,
    }
    caller
}

/// The USER of the owner of the riff, or an empty text when it has none.
async fn release_owner(reach: &Reach) -> Result<String> {
    Ok(match reach.roster().await?.owner {
        riff_core::wire::RiffOwner::Owner { user, .. } => user,
        _ => String::new(),
    })
}

/// The log of the Cloud Run service of the shared riff, from the cloud
/// settings `shared` of this repository.
struct NoLog(String);

impl riff::cycle::CloudLog for NoLog {
    fn lines(&self, _filter: &str) -> Result<Vec<riff::cycle::LogLine>> {
        anyhow::bail!("{}", self.0)
    }
}

/// The log of the shared riff, or a log that names why riff cannot read
/// it.
fn shared_log(here: &std::path::Path, reach: &Reach) -> Box<dyn riff::cycle::CloudLog> {
    match cloud::dir(here).and_then(|dir| cloud::load(&dir, "shared")) {
        // The container has no gcloud: the daemon of the host reads the log.
        Ok(settings) => match reach {
            Reach::Daemon(socket) => Box::new(riff::reach::DaemonLog {
                socket: socket.clone(),
                settings,
            }),
            Reach::Direct(_) => Box::new(riff::cycle::RunLog {
                gcloud: cloud::Gcloud::default(),
                settings,
            }),
        },
        Err(e) => Box::new(NoLog(format!("{e:#}"))),
    }
}

/// Resumes the whole riff after `riff release check`. A failure is a
/// line: the owner or an admin resumes it by hand.
async fn resume_after_check(reach: &Reach) {
    match reach.resume_riff().await {
        Ok(line) => println!("{line}"),
        Err(e) => eprintln!("riff: cannot resume the riff: {e:#}. Run: riff resume --riff"),
    }
}

/// The release gate of `riff wave end N` (01M58KMMCZCE7HDNVT9PVHVZX2): only
/// in a repository with the check of a release tag.
fn wave_release_gate(number: u64, no_release: Option<&str>) -> Result<()> {
    let here = identity::working_dir()?;
    let Some(top) = cloud::top(&here) else {
        return Ok(());
    };
    if !top.join(riff::cycle::CHECK).is_file() {
        return Ok(());
    }
    let forge = riff::cycle::GitHub::new(pr::Gh::default(), &top)?;
    if riff::cycle::wave_gate(&forge, number, no_release)? == riff::cycle::WaveGate::NoRelease {
        println!("Wave {number} ends with no release. The reason is in the wave.");
    }
    Ok(())
}

/// `riff release cut`, `publish`, `status`, `approve`, `check`, `retry`
/// and `rollback` (01M58B0HZYMAK1B2AJMWKHCXG0 to
/// 01M58B0JSQMWNCMSNX19536XT8, 01M58KMKR33P1FSCFDQRZ56A8P to
/// 01M58KMMP9FKRFYX58ESVRSZQX).
async fn release_cycle(command: &ReleaseCommand, server: &str, reach: &Reach) -> Result<()> {
    use riff::cycle::{self, Approved, Checked};
    let here = identity::working_dir()?;
    let refuse = |command: &str, why: cycle::Refusal| -> ! {
        eprintln!("{}", text::release_refused(command, why));
        std::process::exit(1)
    };
    let forge = || cycle::GitHub::new(pr::Gh::default(), &here);
    let next = |forge: &cycle::GitHub, tag: &str| match cycle::tag_version(tag)
        .and_then(|v| cycle::stage(forge, v))
    {
        Ok(stage) => print!("{}", cycle::stage_lines(stage.as_ref())),
        Err(e) => eprintln!("riff: cannot read the state of {tag}: {e:#}"),
    };
    match command {
        ReleaseCommand::Cut {
            version,
            skip,
            reason,
            why,
        } => {
            let version = cycle::version(version)?;
            let skip = match (skip, reason) {
                (Some(skip), Some(reason)) => {
                    if let Some(why) = release_caller(server, true).await.person_step() {
                        refuse("cut --skip", why);
                    }
                    Some(cycle::Skip {
                        version: cycle::version(skip)?,
                        reason: reason.clone(),
                    })
                }
                _ => None,
            };
            let forge = forge()?;
            let cut = cycle::cut(&forge, version, skip.as_ref(), why.as_deref())?;
            println!(
                "The release pull request #{} for issue #{}, with auto-merge on: {}",
                cut.number, cut.issue, cut.url
            );
            let told = async {
                let owner = release_owner(reach).await?;
                let line = cycle::cut_line(&owner, &cycle::tag(version));
                reach.chat_post(&line).await?;
                anyhow::Ok(())
            };
            if let Err(e) = told.await {
                eprintln!("riff: cannot post the chat line of the cut: {e:#}");
            }
            println!(
                "next: wait for the verify and the merge of pull request #{}",
                cut.number
            );
        }
        ReleaseCommand::Verify { number } => {
            let forge = forge()?;
            let head = cycle::verify(&forge, *number)?;
            println!(
                "Pull request #{number} passed: the status riff/verify of {head} is a success."
            );
            println!("next: the auto-merge merges pull request #{number}");
        }
        ReleaseCommand::Publish { tag } => {
            if let Some(why) = release_caller(server, true).await.lead_step() {
                refuse("publish", why);
            }
            let forge = forge()?;
            let done = cycle::publish(&forge, tag)?;
            println!("Published {tag}: {}", done.release);
            match &done.run {
                Some(run) => println!("The CI run of the tag: {run}"),
                None => println!("GitHub shows no CI run of the tag yet."),
            }
            let told = async {
                let owner = release_owner(reach).await?;
                let line = cycle::publish_line(&owner, tag, done.run.as_deref());
                reach.chat_post(&line).await?;
                anyhow::Ok(owner)
            };
            match told.await {
                Ok(owner) if !owner.is_empty() => {
                    println!("riff told the owner {owner} in riff chat.")
                }
                Ok(_) => println!("riff told the riff in riff chat."),
                Err(e) => eprintln!("riff: cannot post the chat line to the owner: {e:#}"),
            }
            println!(
                "next: wait for the jobs of the CI run; then a person runs riff release approve \
                 {tag} in a terminal"
            );
        }
        ReleaseCommand::Status => {
            let forge = forge()?;
            print!("{}", cycle::render_status(&cycle::status(&forge)?));
        }
        ReleaseCommand::Approve { tag } => {
            let first = release_caller(server, false).await;
            let caller = if first.person_step().is_some() && !first.worker {
                release_caller(server, true).await
            } else {
                first
            };
            if caller.person_step().is_some() {
                // Only the lead of the owner goes on, and only with the
                // word of the owner in riff chat (01M594XQJMXAXY3JXP66Y8EVQ7).
                match caller.lead_step() {
                    None => {}
                    Some(why @ (cycle::Refusal::Member | cycle::Refusal::RoleUnknown)) => {
                        refuse("approve", why)
                    }
                    Some(_) => refuse("approve", cycle::Refusal::Agent),
                }
                let roster = reach.roster().await?;
                let me = roster.user.clone();
                let owner_or_admin = |user: &str| match &roster.owner {
                    riff_core::wire::RiffOwner::NoSignIn => user == me,
                    owner => {
                        owner.is(user)
                            || roster.people.iter().any(|p| {
                                p.user == user && p.role == riff_core::wire::PersonRole::Admin
                            })
                    }
                };
                let lines = reach.chat_read().await?;
                let tag = cycle::tag(cycle::tag_version(tag)?);
                let Some(seq) = cycle::owner_said(&lines, &tag, &owner_or_admin) else {
                    refuse("approve", cycle::Refusal::OwnerSilent);
                };
                let forge = forge()?;
                let Some(run) = cycle::waiting_run(&forge, &tag)? else {
                    println!(
                        "No CI run of {tag} waits for the environment production. riff changed nothing."
                    );
                    return Ok(());
                };
                println!("{}", reach.pause_riff().await?);
                cycle::approve_run_said(&forge, &run, seq).with_context(|| {
                    format!(
                        "riff release approve: the approval of {} failed, and the riff stays \
                         paused. next: riff release approve {} again, or riff resume --riff",
                        run.tag, run.tag
                    )
                })?;
                println!(
                    "Approved with message {seq} of the owner: the deploy of {} runs: {}",
                    run.tag, run.url
                );
                println!(
                    "next: the lead runs riff release check {} after the deploy",
                    run.tag
                );
                return Ok(());
            }
            let forge = forge()?;
            let answer = &mut std::io::stdin().lock();
            match cycle::approve(&forge, tag, answer, &mut std::io::stdout())? {
                Approved::NoRun => println!(
                    "No CI run of {tag} waits for the environment production. riff changed nothing."
                ),
                Approved::No => println!("riff changed nothing."),
                Approved::Yes(run) => {
                    // The pause is only for the deploy, and it tells each
                    // other lead (01M58KMM3NFGJRW4CCV50KJQB2).
                    println!("{}", reach.pause_riff().await?);
                    cycle::approve_run(&forge, &run).with_context(|| {
                        format!(
                            "riff release approve: the approval of {} failed, and the riff stays \
                             paused. next: riff release approve {} again, or riff resume --riff",
                            run.tag, run.tag
                        )
                    })?;
                    println!("Approved: the deploy of {} runs: {}", run.tag, run.url);
                    println!(
                        "next: the lead runs riff release check {} after the deploy",
                        run.tag
                    );
                }
            }
        }
        ReleaseCommand::Check { tag } => {
            if let Some(why) = release_caller(server, true).await.lead_step() {
                refuse("check", why);
            }
            let forge = forge()?;
            let served = reach.served().await;
            let log = shared_log(&here, reach);
            match cycle::check(&forge, tag, &served, log.as_ref())? {
                Checked::DeployFailed { deploy, issue } => {
                    println!(
                        "The deploy of {tag} failed: {}. The old revision still serves. The issue: #{issue}",
                        deploy.url
                    );
                    resume_after_check(reach).await;
                }
                Checked::Ran {
                    lines,
                    passed,
                    issue,
                } => {
                    print!("{}", cycle::check_text(tag, &lines));
                    println!("The result is on the release issue #{issue}.");
                    if passed {
                        println!("riff closed the release issue #{issue}.");
                        resume_after_check(reach).await;
                    }
                }
            }
            next(&forge, tag);
        }
        ReleaseCommand::Retry { tag } => {
            if let Some(why) = release_caller(server, true).await.lead_step() {
                refuse("retry", why);
            }
            let forge = forge()?;
            let deploy = cycle::retry(&forge, tag)?;
            println!("The failed jobs of {} run again.", deploy.url);
            println!("next: a person runs riff release approve {tag} in a terminal");
        }
        ReleaseCommand::Test { tag, host } => {
            if let Some(why) = release_caller(server, false).await.test_step() {
                refuse("test", why);
            }
            let forge = forge()?;
            let commit = cycle::test_target(&forge, tag)?;
            let request = riff::daemon::remote::Request::ReleaseTest {
                commit,
                tag: cycle::tag(cycle::tag_version(tag)?),
            };
            ask_daemon(host, &request, &identity::place(&here)?, reach).await?;
            next(&forge, tag);
        }
        ReleaseCommand::Rollback { tag } => {
            if let Some(why) = release_caller(server, true).await.lead_step() {
                refuse("rollback", why);
            }
            let forge = forge()?;
            let done = cycle::rollback(&forge, tag)?;
            println!(
                "The deploy of {} to replace {} started.",
                cycle::tag(done.to),
                cycle::tag(done.from)
            );
            println!(
                "next: a person runs riff release approve {} in a terminal",
                cycle::tag(done.to)
            );
        }
    }
    Ok(())
}

/// Starts this riff with `args`, detached, so that a hook returns at
/// once. A failure goes to stderr.
fn start_detached(args: &[&str]) {
    use std::os::unix::process::CommandExt;
    let started = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
    });
    if let Err(e) = started {
        eprintln!("riff: cannot start riff {}: {e}", args.join(" "));
    }
}

/// The text of `riff usage` ([`usage`]): of `issue`, of each issue of
/// `wave`, or of the sessions of this machine. With `total`, it first
/// writes the total comment of the issue again
/// (01M49HF0ADEQ83XTJCQT0PK3RS).
fn usage_text(issue: Option<&str>, wave: Option<&str>, total: bool) -> Result<String> {
    if issue.is_none() && wave.is_none() {
        let dir = local::marks().context("this machine has no directory for the marks of riff")?;
        return Ok(usage::machine_text(&dir, usage::now_ms()));
    }
    let thread = identity::place(&identity::working_dir()?)?
        .default_thread()
        .context("run riff usage in a git repository")?
        .to_string();
    let gh = pr::Gh::default();
    let forge =
        usage::Forge::of(&gh, &thread).with_context(|| format!("{thread} is no repository"))?;
    if let Some(issue) = issue {
        let number = issue.trim_start_matches('#');
        let number = number.strip_prefix("issue-").unwrap_or(number);
        let number: u64 = number
            .parse()
            .with_context(|| format!("{issue} is no issue: name it as 12, #12 or issue-12"))?;
        let mut text = String::new();
        if total {
            text = usage::total_written(number, &forge.total(number)?) + "\n";
        }
        let comments = forge.comments(number)?;
        text.push_str(&usage::issue_text(
            number,
            &usage::counted(number, &comments),
        ));
        if let Some(line) = usage::uncounted(number, &comments) {
            text.push_str(&line);
            text.push('\n');
        }
        return Ok(text);
    }
    let wave = riff::waves::title(&gh, &thread, wave.unwrap_or_default())?;
    let mut rows = Vec::new();
    for (number, title) in forge.wave(&wave)? {
        let counted = usage::counted(number, &forge.comments(number)?);
        rows.push((number, title, usage::sum(&counted)));
    }
    Ok(usage::wave_text(&wave, &rows))
}

/// After the merge of pull request `number`: the total of the tokens of
/// its issue, as a comment on the issue (01M3Y1YP514MPX8DTKMTWDHE8Q).
fn total_after_merge(number: u64) -> Result<String> {
    let meter = usage::Meter::here().context("this machine has no directory for the marks")?;
    let issue = pr::issue_of(&meter.gh, number)?;
    let thread = identity::place(&identity::working_dir()?)?
        .default_thread()
        .context("this directory is in no git repository")?;
    meter.merged(
        identity::session_id().as_deref(),
        &thread.to_string(),
        issue,
    )
}

/// The scope of `riff pause` and `riff resume` from the flags `--riff`
/// and `--repo` (01M3XAHZBGSSJB3YX23K88W01K).
fn pause_scope(riff: bool, repo: Option<String>) -> Result<PauseScope> {
    let repo = repo.map(|repo| repo.parse::<ThreadName>()).transpose()?;
    Ok(PauseScope::of(riff, repo))
}

/// Pauses or resumes `scope`, and wakes each session that the change
/// stops or starts (01M3XAHZDSQR263QZVB41CK0MX,
/// 01M3JCG3YD7C2Y3V0QJPF082YH).
///
/// The URI of a person names only the host. For the pause of the
/// repository of the directory `here`, the person calls from `here`, so
/// the server knows the repository.
async fn pause(
    api: &Api,
    me: &SessionUri,
    here: &Place,
    scope: PauseScope,
    state: RiffState,
) -> Result<()> {
    let at_here;
    let me = if matches!(scope, PauseScope::Here) && me.who().session().is_none() {
        at_here = SessionUri::new(me.who().clone(), here.clone());
        &at_here
    } else {
        me
    };
    let (reply, posted) = api.set_pause(me, &scope, state).await?;
    let repository = scope.repository(me);
    println!(
        "{}",
        text::riff_set(repository.as_ref(), state, &reply, &posted)
    );
    Ok(())
}

fn thread_or_default(given: Option<String>, here: &Place) -> Result<ThreadName> {
    match given {
        Some(t) => Ok(t.parse()?),
        None => here
            .default_thread()
            .ok_or_else(|| anyhow::anyhow!("name a thread: this directory is not in git")),
    }
}

/// `riff setup`: adds the missing permission rules of riff to the
/// project settings, or with `check`, names them
/// (01M3Q53RNDJBDHVDFHJ9HCX9S1).
fn setup(check: bool) -> Result<()> {
    let project = permissions::Project::of(&identity::working_dir()?);
    let user = claude_settings::settings_from(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("HOME"),
    );
    let left = project.missing(user.as_deref());
    if check {
        println!("{}", text::setup_check(&project.settings(), &left));
        if !left.is_empty() {
            std::process::exit(1);
        }
        return Ok(());
    }
    let added = permissions::add(&project.settings(), &left)?;
    println!("{}", text::setup_added(&project.settings(), &added));
    Ok(())
}

/// Asks the person once about the update by itself on a machine with no
/// `update.auto` key, in a terminal only, and shows the new setting
/// (01M3NT6WV8Q8EFZBK8DHYKW5CC).
fn ask_auto_update() {
    use std::io::IsTerminal;
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let asked = settings::path().and_then(|path| {
        settings::ask_update_auto(
            &path,
            terminal,
            &mut std::io::stdin().lock(),
            &mut std::io::stdout(),
        )
    });
    match asked {
        Ok(Some(on)) => {
            if let Ok(path) = settings::path() {
                anstream::println!("{}", view::auto_update(on, &path));
            }
        }
        Ok(None) => {}
        Err(e) => eprintln!("riff: {e:#}"),
    }
}

/// The riff-server that riff uses, and where it comes from: `--server`,
/// else a `RIFF_SERVER` that is not empty, else the riff of this
/// machine (01M3Q5VE4VVXT9FH4J4MAWX68V).
fn server_of(flag: Option<&str>) -> Result<(String, lifecycle::Source)> {
    if let Some(flag) = flag {
        return Ok((flag.to_owned(), lifecycle::Source::Flag));
    }
    match std::env::var("RIFF_SERVER") {
        Ok(env) if !env.is_empty() => {
            let url = api::server_url(&env).map_err(|e| anyhow::anyhow!("RIFF_SERVER: {e}"))?;
            Ok((url, lifecycle::Source::Env))
        }
        _ => Ok((DEFAULT_SERVER.to_owned(), lifecycle::Source::Default)),
    }
}

/// When each riff command uses color (01M3Q5VE2D244XDZRYXM8DNSRS,
/// 01M3JDCA9070MY30AYHK3Y67EF).
#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

/// Sets the color of each later `anstream` print.
fn use_color(color: ColorWhen) {
    anstream::ColorChoice::write_global(match color {
        ColorWhen::Auto => anstream::ColorChoice::Auto,
        ColorWhen::Always => anstream::ColorChoice::Always,
        ColorWhen::Never => anstream::ColorChoice::Never,
    });
}

/// Runs until stopped. It connects again when the stream ends (R131).
/// Each message is a [`text::block`]. The status lines go to stderr
/// (01M3JDCA6R894JG6SDJ2R7AFMN). On a new binary, it runs it
/// (01M3MNVTC248YYJJQKFD9H1WY9). The first poll reads the binary of
/// this process before the first line goes out, so a test that waits
/// for that line and then puts a new binary in place sees the update.
async fn tail(api: &Api, me: &SessionUri, thread: &ThreadName, here: &Place) {
    tokio::select! {
        biased;
        () = binary::follow_update(here) => {}
        () = tail_each(api, me, thread) => {}
    }
}

/// Prints each message of `thread`. It connects again at once when the
/// stream ends, and shows only a short dim line while a connect fails,
/// or the error in red for a version that it cannot talk to
/// (01M3MNVTC248YYJJQKFD9H1WY9, 01M3NK7VHXB0PAR8VH8GQQA06K). After a
/// break, it shows each message of the break one time, or a line with
/// the number of the lost messages (see [`riff::catch_up`]).
async fn tail_each(api: &Api, me: &SessionUri, thread: &ThreadName) {
    use riff::catch_up::{Seen, Shown, Start, follow_thread};
    let error = riff::style::ERROR;
    anstream::eprintln!("riff: showing new messages in {thread}. Ctrl-C stops.");
    let shown = std::sync::Arc::new(std::sync::Mutex::new(Shown::new(Start::New)));
    let mut stream = Box::pin(follow_thread(api, me, thread, shown, STREAM_RETRY));
    let mut link = riff::view::Reconnect::default();
    let mut last_day = None;
    while let Some(item) = stream.next().await {
        if let Some(line) = link.line(&item) {
            anstream::eprintln!("{line}");
        }
        let checked = match item {
            Ok(Seen::Message(checked)) => checked,
            Ok(Seen::Lost(n)) => {
                let warning = riff::style::WARNING;
                anstream::eprintln!("{warning}{}{warning:#}", text::lost_messages(n));
                continue;
            }
            Err(_) => continue,
        };
        let line = riff_client::threads::Line::of(&checked, thread);
        let at = i64::try_from(line.at_ms)
            .ok()
            .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
            .unwrap_or_else(chrono::Local::now);
        let block = text::block(&line, &at, last_day, textwrap::termwidth());
        last_day = Some(at.date_naive());
        anstream::println!("{block}");
    }
    anstream::eprintln!("{error}riff: the stream of {thread} ended.{error:#}");
}

/// The items of this repository that a session claims, from
/// riff-server (01M5924014WN63RCHJWGZS4AFZ), straight or through the
/// riff daemon ([`Reach`], 01M5GVPNAKAPA596K9361HSAAV).
async fn claimed_here(reach: &Reach, here: &riff_core::name::Place) -> Result<Vec<u64>> {
    let Some(thread) = here.default_thread() else {
        anyhow::bail!(text::waves_no_repo());
    };
    let who = reach.who(false).await?;
    Ok(riff::waves::claimed(&who, &thread.to_string()))
}

/// `riff wave move ITEM... --to WAVE` (01M5924014WN63RCHJWGZS4AFZ).
async fn wave_move(
    reach: &Reach,
    here: &riff_core::name::Place,
    items: &[u64],
    to: &str,
) -> Result<()> {
    let claimed = claimed_here(reach, here).await?;
    let title = riff::waves::move_items(&pr::Gh::default(), items, to, &claimed)?;
    println!("{}", text::waves_moved(items, &title));
    Ok(())
}

/// `riff wave end N --move-open-to-backlog` (01M592403Q4TNZEDNR76C7NK9M).
async fn wave_end_moving(
    reach: &Reach,
    here: &riff_core::name::Place,
    number: u64,
    no_release: Option<&str>,
) -> Result<()> {
    let claimed = claimed_here(reach, here).await?;
    wave_release_gate(number, no_release)?;
    let (ended, moved) = riff::waves::end_moving_open(&pr::Gh::default(), number, &claimed)?;
    println!("{}", text::waves_ended_moving(&ended.title, &moved));
    Ok(())
}

/// `riff wave list`: the open milestones from `gh`, with the items and
/// the hosts of riff-server (01M58ATZ408RBCX0PM0D0ZCY4B). As in
/// [`top`], the repository comes from the folder `here`, also for a
/// person with no session, and each call carries the token of the
/// caller, straight or through the riff daemon ([`Reach`]). When riff
/// cannot read them, one line on stderr says why
/// (01M58ATZ6FA5PJMD3D848CEWBJ).
async fn wave_list(reach: &Reach, here: &riff_core::name::Place) -> Result<()> {
    let gh = pr::Gh::default();
    let milestones = riff::waves::open(&gh)?;
    let items = riff::waves::items(reach, here.default_thread(), &gh).await;
    let items = items
        .inspect_err(|e| eprintln!("{}", text::waves_no_items(&format!("{e:#}"))))
        .ok();
    println!("{}", text::waves_list(&milestones, items.as_ref()));
    Ok(())
}

/// Draws [`riff::top::Top`] once, or again every
/// [`riff::top::REFRESH`] and after each message of `thread` until
/// stopped (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
/// (01M3NB589WMPRSAR43BSG9SP41). Until stopped, it runs a new binary
/// (01M3NT6WXGCNKW3EQ7MBJDQTR4), and it goes on after a look that
/// fails: see "A look that fails" in [`riff::top`]
/// (01M3Z8FXE2DY34ZP75WJE1S8HR).
async fn top(
    api: &Api,
    me: &SessionUri,
    thread: Option<ThreadName>,
    show: &riff_client::sessions::Show,
    once: bool,
) -> Result<()> {
    if once {
        return draw_top(api, me, thread, show, once).await;
    }
    tokio::select! {
        drawn = draw_top(api, me, thread, show, once) => drawn,
        () = binary::follow_update(me.place()) => Ok(()),
    }
}

async fn draw_top(
    api: &Api,
    me: &SessionUri,
    thread: Option<ThreadName>,
    show: &riff_client::sessions::Show,
    once: bool,
) -> Result<()> {
    use std::io::{IsTerminal, Write};
    use std::time::Instant;
    // One read of `gh` for each repository with a live session
    // (01M42KHN80V49HDDZF953HXDT0). The calls share one token
    // (01M54510SFK1Z9VPFYBYYSV10F).
    let gh = pr::Gh::default().with_one_token();
    let fetch = |repos: std::collections::BTreeSet<String>| {
        let gh = gh.clone();
        tokio::task::spawn_blocking(move || {
            repos
                .into_iter()
                .map(|repo| {
                    let issues = riff::top::issues_from_gh(&gh, &repo);
                    (repo, issues)
                })
                .collect::<Vec<_>>()
        })
    };
    let mut issues = std::collections::BTreeMap::new();
    let mut fetched: Option<Instant> = None;
    let mut holds = std::collections::BTreeMap::new();
    let mut messages = thread
        .as_ref()
        .map(|t| Box::pin(follow(|| api.tail(me, t), STREAM_RETRY)));
    let clear = !once && std::io::stdout().is_terminal();
    let repo = thread.as_ref().map(ToString::to_string);
    // The last good look, and its time.
    let mut last = None;
    loop {
        let look = async {
            let pauses = api.pauses(me).await?;
            let mut who = api.roster(me, false).await?;
            riff::state::fill(&mut who.sessions, pauses.state);
            // `riff top` asks no riff daemon: the model has no container
            // session (01M53NGW6F5HP1DZ9QRGP50MGT, 01M56Y8KD05JH2J7AMK636WGT7).
            let sessions = Sessions::new(std::mem::take(&mut who.sessions), Vec::new());
            anyhow::Ok((pauses, who, sessions))
        };
        // Each call of a look has the budget of a short command
        // (01M4A803Z4Q0KX6NT1KC6QR43H).
        let looked = look.await;
        // After one good look, a fault that a new try can repair keeps
        // the last table (01M3Z8FXE2DY34ZP75WJE1S8HR).
        let failed = match looked {
            Ok(look) => {
                last = Some((look, chrono::Local::now()));
                None
            }
            Err(e) if last.is_some() && api::passes(&e) => Some(e),
            Err(e) => return Err(e),
        };
        // A fault with no good look ended the loop above.
        let Some(((pauses, who, sessions), at)) = &last else {
            continue;
        };
        if fetched.is_none_or(|t| t.elapsed() >= riff::top::ISSUES_TTL) {
            // A `gh` that fails keeps the last issues of its repository
            // (01M3ZC09FA9DZPTHK31XECZ566).
            let repos = riff_client::waves::board_repos(sessions, show, repo.as_deref());
            let read = fetch(repos).await?;
            issues = riff_client::waves::Issues::newest_each(std::mem::take(&mut issues), read);
            fetched = Some(Instant::now());
        }
        // The holds of each board at each good look. A read that fails
        // keeps the last holds of its repository
        // (01M58ATZ1F42DWNNB24C2HJPEY).
        if failed.is_none() {
            holds.retain(|repo, _| issues.contains_key(repo));
            for repo in issues.keys() {
                let Ok(thread) = repo.parse::<ThreadName>() else {
                    continue;
                };
                if let Ok(plan) = api.plan_show(me, &thread).await {
                    holds.insert(repo.clone(), plan.holds);
                }
            }
        }
        let now = chrono::Local::now();
        let fault = failed.map(|e| text::top_fault(&e.to_string(), api.base(), at, &now));
        let server = riff_client::link::server_build();
        let machines = riff_client::machines::machines(sessions, None);
        let top = riff::top::Top {
            pauses,
            owner: &who.owner,
            server: server.as_ref(),
            sessions,
            people: &who.people,
            issues: &issues,
            holds: &holds,
            repo: repo.as_deref(),
            show,
            width: textwrap::termwidth(),
            fault: fault.as_deref(),
            machines: &machines,
        };
        if clear {
            // Home and erase: the table draws again in place.
            print!("\x1b[H\x1b[2J");
            std::io::stdout().flush()?;
        }
        anstream::print!("{}", top.view());
        if once {
            return Ok(());
        }
        let message = async {
            match messages.as_mut() {
                Some(stream) => {
                    stream.next().await;
                }
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = tokio::time::sleep(riff::top::REFRESH) => {}
            () = message => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// Only `riff daemon` and `riff relay` serve. Each other command
    /// ends quietly at a closed pipe (01M5MRG35BYRGZYD4VQVV8ZTYX).
    #[test]
    fn only_the_daemon_and_the_relay_serve() {
        use clap::Parser;
        let serves = |args: &[&str]| {
            Cli::try_parse_from(args.iter().copied())
                .unwrap()
                .command
                .serves()
        };
        assert!(serves(&["riff", "daemon"]));
        assert!(serves(&["riff", "relay", "--", "claude"]));
        assert!(serves(&["riff", "relay", "prompt"]));
        assert!(!serves(&["riff", "daemon", "status"]));
        assert!(!serves(&["riff", "usage"]));
        assert!(!serves(&["riff", "who"]));
        assert!(!serves(&["riff", "run", "12"]));
    }

    /// The writes of `riff cloud` are the subcommands of
    /// `riff::cloud::WRITES`, which the flag settings of a worker deny.
    /// Each other subcommand is a read (01M558C4VZ2PSMEYPAZBRYW1VV).
    #[test]
    fn the_writes_of_riff_cloud_are_the_subcommands_that_a_worker_denies() {
        use clap::Parser;
        let cli = Cli::command();
        let cloud = cli.find_subcommand("cloud").unwrap();
        let mut writes = Vec::new();
        for sub in cloud.get_subcommands() {
            let name = sub.get_name();
            let mut args = vec!["riff", "cloud", name];
            if sub.get_positionals().any(clap::Arg::is_required_set) {
                args.push("stage");
            }
            let parsed = Cli::try_parse_from(&args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
            let super::Command::Cloud { command } = parsed.command else {
                panic!("{args:?}: not riff cloud");
            };
            if command.writes() {
                writes.push(name);
            }
        }
        assert_eq!(writes, riff::cloud::WRITES);
    }

    /// Each subcommand that a person uses is in one group of `riff
    /// --help`, and each name of a group is such a subcommand
    /// (01M3NJDSQ23FFRMH8ZD4GC57WY).
    #[test]
    fn each_shown_command_is_in_one_group() {
        let cli = Cli::command();
        let mut shown: Vec<_> = cli
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(|sub| sub.get_name().to_string())
            .collect();
        let mut grouped: Vec<_> = riff::help::GROUPS
            .iter()
            .flat_map(|group| group.commands.iter().map(ToString::to_string))
            .collect();
        shown.sort();
        grouped.sort();
        assert_eq!(shown, grouped);
    }

    /// The short help of each command is one phrase of at most 60
    /// characters, with no period (01M3NJDSQ23FFRMH8ZD4GC57WY).
    #[test]
    fn each_short_help_is_one_short_phrase() {
        fn check(cmd: &clap::Command) {
            for sub in cmd.get_subcommands() {
                let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
                assert!(
                    !about.is_empty() && about.len() <= 60 && !about.ends_with('.'),
                    "{}: {about:?}",
                    sub.get_name()
                );
                check(sub);
            }
        }
        check(&Cli::command());
    }

    /// The help of `--server` is one short line on each command
    /// (01M3NT228WA11PGNWDJ0WP7PQD).
    #[test]
    fn the_server_help_is_short_on_each_command() {
        fn check(cmd: &clap::Command, seen: &mut usize) {
            let server = cmd
                .get_arguments()
                .find(|arg| arg.get_id() == "server")
                .unwrap_or_else(|| panic!("{}: no --server", cmd.get_name()));
            let help = server
                .get_help()
                .map(ToString::to_string)
                .unwrap_or_default();
            assert!(
                !help.is_empty() && help.len() <= 60 && server.get_long_help().is_none(),
                "{}: {help:?}",
                cmd.get_name()
            );
            *seen += 1;
            // clap makes a `help` subcommand with no options.
            for sub in cmd.get_subcommands().filter(|sub| sub.get_name() != "help") {
                check(sub, seen);
            }
        }
        let mut cli = Cli::command();
        cli.build();
        let mut seen = 0;
        check(&cli, &mut seen);
        assert!(seen > 30, "{seen}");
    }
}
