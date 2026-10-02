//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::TimeZone;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use futures::{Stream, StreamExt};
use riff::api::{self, Api, DEFAULT_SERVER, PauseScope, Reconnect, follow};
use riff::terminal::{Program, Terminal, Tmux};
use riff::{
    auto_update, binary, dropped, enable, help, hook, identity, lifecycle, local, login, mcp, next,
    permissions, plugin, pr, settings, terminal, text, usage, view, worker,
};
use riff_core::build::{Build, Mismatch};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Freed, Kind, RiffReply, RiffState, SessionInfo, StartReason, Status};

/// The time between two tries to connect a stream.
const RETRY: Duration = Duration::from_secs(5);

/// The longest time that `riff statusline` waits for riff-server.
const STATUSLINE_WAIT: Duration = Duration::from_secs(2);

/// How often a watch looks for a leave of its session
/// (01M3MEEFETT9A0DRWBKQTG77Z2).
const LEFT_POLL: Duration = Duration::from_millis(250);

/// The hidden option in which a watch gives the end of its wait to the
/// new binary of an update (01M3Z64J08GW6N1H42AR2FZQZ4).
const UNTIL_ARG: &str = "--until";

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
    /// Set your status: your current step
    ///
    /// `riff who` shows it with its age. It replaces your old status.
    Status {
        /// You cannot go on. REASON says why.
        #[arg(long, value_name = "REASON")]
        blocked: Option<String>,
        /// Your current step, in one short line.
        #[arg(required = true)]
        step: Vec<String>,
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
        /// Read only this thread. It need not be one of your threads.
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
    /// and sends each line that you type. A line with @lead wakes your
    /// lead, and @USER wakes the lead of USER. Other lines wake no
    /// session. /me TEXT sends an action. /quit or Ctrl-C exits.
    Chat {
        /// The last line that the chat showed before an update. Only
        /// riff gives it (01M3NT6WXGCNKW3EQ7MBJDQTR4).
        #[arg(long, hide = true)]
        after: Option<u64>,
    },
    /// Show a live table of each session
    ///
    /// Each row shows the tags, the item and the status of a session. It
    /// draws the table again in place until Ctrl-C. It posts nothing and
    /// wakes no session.
    Top {
        /// Print the table once and exit.
        #[arg(long)]
        once: bool,
    },
    /// Claim a work item
    ///
    /// Then no other session does the same work. It exits with status 1
    /// when another session holds it.
    Claim {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
    },
    /// Release a work item that you claimed
    ///
    /// The lead frees the claim of another session of its user with
    /// `--session`, for example a session that is gone or that does not
    /// answer. Then the next session can claim the item.
    Release {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The session that holds the item: its session ID or the start
        /// of it, as `riff who` shows it. Only the lead can name one.
        #[arg(long)]
        session: Option<String>,
        /// The work item, for example issue-12.
        item: String,
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
        /// Each issue of this wave, for example "Wave 3".
        #[arg(long, value_name = "TITLE")]
        wave: Option<String>,
    },
    /// Make this session the lead of your user
    ///
    /// Each person has one lead in each repository. The other sessions of
    /// your user send their questions to the lead. It replaces the old
    /// lead. Run it in the agent session, for example `! riff lead` in
    /// Claude Code.
    Lead {
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
    /// Print one line for each wake of this session
    ///
    /// The plugin runs it. One watch runs for each session: a second one
    /// stops at once.
    #[command(hide = true, args_conflicts_with_subcommands = true)]
    Watch {
        /// Exit after the first wake, or after the time of `riff watch
        /// limit` with no wake. For a runner that wakes the session
        /// when the command exits.
        #[arg(long)]
        once: bool,
        /// The time at which the watch of before an update ends with no
        /// wake, in seconds since 1970. Only riff gives it.
        #[arg(long, hide = true)]
        until: Option<u64>,
        #[command(subcommand)]
        command: Option<WatchCommand>,
    },
    /// Serve the riff tools to an agent session over stdio
    ///
    /// The plugin runs it.
    #[command(hide = true)]
    Mcp {
        /// The initialize request of the client, as JSON, after an
        /// update. Only riff gives it (01M3NT6WZTKAFKGDWGCFKC8TB5).
        #[arg(long, hide = true)]
        client: Option<String>,
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
    /// It adds their verified email to the members. They need no allowed
    /// domain. Only the owner or an admin can. It prints the address of
    /// the riff and the lines that the person runs to join.
    Invite {
        /// The email that the person signs in with.
        email: String,
    },
    /// Remove a member of this riff
    ///
    /// It also ends each sign-in of that person. Only the owner or an
    /// admin can. The owner stays.
    Remove {
        /// The email of the person.
        email: String,
    },
    /// List who may join this riff
    ///
    /// It lists the owner, the admins, the members and the allowed
    /// domains.
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
    /// Print the status line of a Claude Code session
    ///
    /// It shows the short session ID, the claims, and `lead` or
    /// `blocked`. Claude Code runs it with the session on stdin. It always
    /// exits with status 0.
    #[command(hide = true)]
    Statusline,
    /// Run a Claude Code hook
    ///
    /// The riff plugin calls it.
    #[command(hide = true)]
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Install the riff plugin in an agent tool
    ///
    /// When the riff has sign-in and this machine has none, it signs you
    /// in. Run it again to update the plugin.
    Connect {
        #[command(subcommand)]
        tool: Tool,
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
    /// Turn riff on in this repository
    ///
    /// riff is off in a Claude Code session until you turn it on for
    /// the repository of the session. This command writes the entry
    /// `riff@riff` to the key `enabledPlugins` of a Claude Code settings
    /// file. It keeps each other key. A new session in the repository
    /// then has riff.
    Enable {
        #[command(flatten)]
        place: PlaceArgs,
    },
    /// Turn riff off in this repository
    ///
    /// It removes the entry that `riff enable` wrote. When another file
    /// still turns riff on, for example after `riff enable --global`,
    /// it writes a no for this repository to the local settings. No
    /// other repository changes. A session that runs keeps riff until
    /// it ends.
    Disable {
        #[command(flatten)]
        place: PlaceArgs,
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
    /// It installs riff and riff-server of a release with cargo, then
    /// updates the plugin with `riff connect claude`. It installs the
    /// release that the riff runs, or the newest release when riff uses
    /// the riff of this machine or cannot read the build of the riff.
    /// When the riff of this machine runs the old build, it tells you to
    /// start riff-server again.
    Update {
        /// Install this release, for example v0.2.0, not the release
        /// that the riff runs.
        #[arg(long, value_parser = lifecycle::parse_tag)]
        tag: Option<String>,
        /// Turn the update by itself on or off for this machine, and
        /// install nothing now. When it is on, riff installs each new
        /// release that the riff runs, in the background. With no value,
        /// show the setting.
        #[arg(long, value_enum, conflicts_with_all = ["tag", "background"])]
        auto: Option<Option<Switch>>,
        /// Run as the update by itself: take the update lock of this
        /// machine, and tell the lead the result.
        #[arg(long, hide = true, requires = "tag")]
        background: bool,
        /// The cargo command.
        #[arg(long, default_value = "cargo")]
        cargo: std::path::PathBuf,
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
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
    /// the result to the session that holds its issue.
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
    /// Start, list and stop the workers of this machine
    ///
    /// Workers are agent sessions in tmux. With no subcommand, it lists
    /// each worker: its pane, its session ID, its claims and its status.
    Workers {
        /// Show the full session ID of each worker.
        #[arg(long)]
        long: bool,
        #[command(subcommand)]
        command: Option<Workers>,
    },
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

#[derive(Subcommand)]
enum Pr {
    /// Open the pull request of this branch
    ///
    /// It is for the issue that this session claims, and it turns on
    /// auto-merge with a squash. The body has the link line and the
    /// trailers Issue: and Milestone: of the issue. Push the branch first.
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
enum Workers {
    /// Start COUNT workers
    ///
    /// It starts them in the tmux window riff-workers, one pane each. Each
    /// pane runs `claude "Join the riff."` in the main worktree. It
    /// starts at most the limit minus the workers that run. It refuses in
    /// a worker, and in an agent session that is not the lead. Outside
    /// tmux, it starts nothing. With --host, the lead asks the workers
    /// host on that machine to start them.
    Start {
        /// The number of workers.
        #[arg(value_parser = clap::value_parser!(u16).range(1..))]
        count: u16,
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
        /// Start the workers on HOST, through its `riff workers host`.
        /// `riff workers` lists the hosts.
        #[arg(long)]
        host: Option<String>,
    },
    /// Offer the workers of this machine to your lead
    ///
    /// Run it in a tmux pane in the main clone, and leave it running. It
    /// starts and stops workers only on a verified request of the lead of
    /// your user, at most the limit of this machine. Ctrl-C stops it.
    Host {
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
        /// The session of the host that ran this binary after an update
        /// (01M3Q55KJ8BKMPE9RADB63X8SP).
        #[arg(long, hide = true)]
        session: Option<String>,
    },
    /// Show or set the most workers on this machine
    ///
    /// The default is 0, so no worker starts until you set it. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.limit.
    Limit {
        /// The new limit. Leave it out to show the limit.
        limit: Option<u16>,
    },
    /// Show or set the seconds between two workers that riff starts
    ///
    /// When the riff runs, the current wave has free work and no worker
    /// is idle, the lead starts one worker each SECONDS, on the machine
    /// with the most free capacity. The default is 10. 0 turns it
    /// off. It is in $XDG_CONFIG_HOME/riff/config.toml, key
    /// workers.interval, on the machine of the lead.
    Interval {
        /// The new interval in seconds. Leave it out to show it.
        seconds: Option<u16>,
    },
    /// Show or set the compile jobs and test threads of each worker
    ///
    /// Each worker gets the number in CARGO_BUILD_JOBS and
    /// RUST_TEST_THREADS. The default is 0: the cores of this machine
    /// divided by the limit of workers, and 2 or more. The next worker
    /// that starts gets the new number. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.jobs.
    Jobs {
        /// The new number of jobs. Leave it out to show it.
        jobs: Option<u16>,
    },
    /// Show or set the nice value of each worker
    ///
    /// Each worker runs with this nice value, so its builds give way to
    /// your other work. The default is 10. 0 turns it off. The next
    /// worker that starts gets the new value. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.nice.
    Nice {
        /// The new nice value, 0 to 19. Leave it out to show it.
        #[arg(value_parser = clap::value_parser!(u8).range(..=19))]
        nice: Option<u8>,
    },
    /// Show or set the most memory of all workers of this machine
    ///
    /// On a machine with systemd, all workers run in the slice
    /// riff-workers.slice with this memory limit. The default is 0:
    /// three quarters of the memory of this machine. The next
    /// `riff workers start` sets the new limit. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.memory.
    Memory {
        /// The new limit in GB. Leave it out to show it.
        gb: Option<u32>,
    },
    /// Show or set the available memory that a new worker needs
    ///
    /// riff starts no new worker on this machine while less than GB of
    /// memory is available. The default is 4. 0 turns it off. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.floor.
    Floor {
        /// The new floor in GB. Leave it out to show it.
        gb: Option<u32>,
    },
    /// Show or change the MCP servers of each worker
    ///
    /// These are the MCP servers that each worker of this machine loads.
    /// The default is riff only. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.mcp.
    Mcp {
        #[command(subcommand)]
        command: Option<WorkersMcp>,
    },
    /// End the workers of this machine
    ///
    /// It ends each worker, or only the worker in PANE. Each worker leaves
    /// `riff who` and frees its claims at once. With --host, the lead asks
    /// the workers host on that machine to end each of its workers, or
    /// only the worker in PANE.
    Stop {
        /// The tmux pane of one worker, for example %3, or its session ID
        /// or the first 8 characters of it. `riff workers` shows them.
        pane: Option<String>,
        /// Stop the workers on HOST, through its `riff workers host`.
        #[arg(long)]
        host: Option<String>,
    },
    /// Show or set how the server stops idle workers
    ///
    /// The server keeps at most PER_HOST idle workers on each host. It
    /// stops each other worker that holds no claim and makes no call for
    /// AFTER seconds. The defaults are 1 and 60. Only the owner or an
    /// admin of the riff can set them.
    Idle {
        /// The most idle workers that stay on each host.
        #[arg(long)]
        per_host: Option<u16>,
        /// The idle time in seconds after which the server stops a
        /// worker.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        after: Option<u64>,
    },
    /// Run CLAUDE as a worker, and wait
    ///
    /// When CLAUDE exits on its own, it tells the lead the pane, the
    /// session ID and the exit code. It never starts CLAUDE again. Each
    /// worker pane runs it.
    #[command(hide = true)]
    Run {
        /// The claude command.
        claude: std::path::PathBuf,
        /// The arguments of CLAUDE.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum WatchCommand {
    /// Show or set the longest wait of riff watch --once
    ///
    /// Claude Code stops a background task after 2 hours at most. So
    /// riff watch --once ends by itself after this time with no wake,
    /// and the session starts it again. The default is 6000 seconds. It
    /// is in $XDG_CONFIG_HOME/riff/config.toml, key watch.limit.
    Limit {
        /// The seconds. 0 is no limit. Leave it out to show the
        /// setting.
        seconds: Option<u64>,
    },
}

#[derive(Subcommand)]
enum LeadCommand {
    /// Show or set how riff compacts the lead at the end of a wave
    ///
    /// When the riff is paused, the wave and its release are done, and
    /// the lead and its person are idle, riff asks the lead for a handoff
    /// note, then types /compact into its tmux pane. It is on by default.
    /// It is in $XDG_CONFIG_HOME/riff/config.toml, keys lead.compact and
    /// lead.quiet.
    Compact {
        /// Turn it on or off. Leave it out to show the setting.
        switch: Option<Switch>,
        /// The seconds with no input in the pane of the lead before riff
        /// compacts it. The default is 60.
        #[arg(long)]
        quiet: Option<u64>,
    },
}

#[derive(Subcommand)]
enum WorkersMcp {
    /// Give each new worker an MCP server
    ///
    /// NAME is an MCP server of your Claude Code config. `claude mcp list`
    /// shows the names.
    Add {
        /// The name of the MCP server.
        name: String,
    },
    /// Take an MCP server from each new worker
    ///
    /// riff stays.
    Remove {
        /// The name of the MCP server.
        name: String,
    },
}

#[derive(Subcommand)]
enum HookEvent {
    /// Run the SessionStart hook
    ///
    /// It reads the SessionStart input on stdin, and prints the context
    /// that starts the watch. It always exits with status 0.
    SessionStart,
    /// Run the SessionEnd hook
    ///
    /// It reads the SessionEnd input on stdin, and tells riff-server that
    /// the session ended, unless the reason is clear. It always exits
    /// with status 0.
    SessionEnd,
    /// Run the Stop hook
    ///
    /// It reads the Stop input on stdin. In a worker that released its
    /// last claim, riff then gives its pane `/clear` and the start
    /// prompt. It always exits with status 0.
    Stop,
    /// Check whether riff clears the context of a worker now
    ///
    /// The Stop hook starts it, detached, in each worker in tmux.
    #[command(hide = true)]
    Clear {
        /// The riff session ID of the worker.
        #[arg(long)]
        session: String,
        /// The tmux pane of the worker.
        #[arg(long)]
        pane: String,
        /// The transcript of the session.
        #[arg(long)]
        transcript: Option<std::path::PathBuf>,
    },
    /// Check whether riff compacts the lead now
    ///
    /// The Stop hook starts it, detached, in each session that is not a
    /// worker.
    #[command(hide = true)]
    Compact {
        /// The session ID of the agent tool.
        #[arg(long)]
        session: String,
        /// The transcript of the session.
        #[arg(long)]
        transcript: Option<std::path::PathBuf>,
        /// The tmux pane of the session.
        #[arg(long)]
        pane: Option<String>,
    },
    /// Report the tokens of each claim that a start or an end freed
    ///
    /// It ends each open claim in the marks of the session. The start
    /// hook starts it at a new start, and the end hook starts it,
    /// detached.
    #[command(hide = true)]
    Usage {
        /// The riff session ID.
        #[arg(long)]
        session: String,
        /// The time of the hook, in milliseconds since the Unix epoch.
        #[arg(long)]
        before: u64,
    },
}

/// The settings file of `riff enable` and `riff disable`.
#[derive(clap::Args)]
#[group(multiple = false)]
struct PlaceArgs {
    /// Only for you: .claude/settings.local.json at the top of the
    /// repository (the default).
    #[arg(long)]
    local: bool,
    /// For the team: .claude/settings.json at the top of the
    /// repository. Commit the file.
    #[arg(long)]
    shared: bool,
    /// For each repository on this machine: the user settings of
    /// Claude Code.
    #[arg(long)]
    global: bool,
}

impl PlaceArgs {
    fn place(&self) -> enable::Place {
        match (self.shared, self.global) {
            (true, _) => enable::Place::Shared,
            (_, true) => enable::Place::Global,
            _ => enable::Place::Local,
        }
    }
}

/// Where `riff connect claude` turns riff on.
#[derive(Clone, Copy, clap::ValueEnum)]
enum ScopeArg {
    /// Only in the repository of this directory.
    Repo,
    /// In each repository on this machine.
    Global,
    /// Nowhere now. Run `riff enable` later.
    None,
}

impl From<ScopeArg> for enable::Scope {
    fn from(scope: ScopeArg) -> Self {
        match scope {
            ScopeArg::Repo => enable::Scope::Repo,
            ScopeArg::Global => enable::Scope::Global,
            ScopeArg::None => enable::Scope::None,
        }
    }
}

#[derive(Subcommand)]
enum Tool {
    /// Add the riff plugin to Claude Code.
    ///
    /// riff stays off in a session until you turn it on for a
    /// repository. In a terminal, the command asks one time where you
    /// want riff on. With no terminal, it turns riff on nowhere, and
    /// keeps an earlier choice.
    Claude {
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
        /// Where to turn riff on, with no question.
        #[arg(long, value_enum)]
        scope: Option<ScopeArg>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = help::matches(help::grouped(Cli::command(), help::GROUPS));
    let cli = Cli::from_arg_matches(&matches)?;
    use_color(cli.color);
    // Each entry of the plugin does nothing where riff is off
    // (01M3XY2ST8R67SKTXJECAYJZRX). A `riff mcp` that an update started
    // again serves a session that runs: it goes on.
    match &cli.command {
        Command::Hook { .. } | Command::Statusline if !enable::State::here().on => return Ok(()),
        Command::Mcp { client: None } if !enable::State::here().on => {
            return mcp::serve_off().await;
        }
        _ => {}
    }
    let (server, source) = server_of(cli.server.as_deref())?;
    if let Command::Server = cli.command {
        let view = lifecycle::view(&server, DEFAULT_SERVER, source).await;
        anstream::println!("{}", text::server_view(&view));
        return Ok(());
    }
    if let Command::Update {
        tag,
        auto,
        background,
        cargo,
        claude,
    } = &cli.command
    {
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
            return riff::auto_update::run(
                cargo,
                claude,
                tag,
                &server,
                DEFAULT_SERVER,
                cli.place.as_ref(),
            )
            .await;
        }
        println!(
            "{}",
            lifecycle::update(cargo, claude, tag.as_deref(), &server, DEFAULT_SERVER).await?
        );
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionStart,
    } = cli.command
    {
        let output = session_start(&server).await;
        if !output.is_empty() {
            println!("{output}");
        }
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionEnd,
    } = cli.command
    {
        session_end(&server).await;
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::Stop,
    } = cli.command
    {
        stop_hook();
        return Ok(());
    }
    if let Command::Hook {
        event:
            HookEvent::Clear {
                session,
                pane,
                transcript,
            },
    } = &cli.command
    {
        if let Err(e) = clear_check(session, pane, transcript.as_deref(), &server).await {
            eprintln!("riff: cannot clear the context of the worker: {e:#}");
        }
        return Ok(());
    }
    if let Command::Hook {
        event:
            HookEvent::Compact {
                session,
                transcript,
                pane,
            },
    } = cli.command
    {
        let check = riff::compact::Check {
            session,
            transcript,
            pane,
        };
        if let Err(e) = riff::compact::run(&check, &server).await {
            eprintln!("riff: cannot check the compact of the lead: {e:#}");
        }
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::Usage { session, before },
    } = &cli.command
    {
        if let Some(meter) = usage::Meter::here() {
            meter.release_all(session, *before);
        }
        return Ok(());
    }
    if let Command::Usage { issue, wave } = &cli.command {
        print!("{}", usage_text(issue.as_deref(), wave.as_deref())?);
        return Ok(());
    }
    if let Command::Lead {
        command: Some(LeadCommand::Compact { switch, quiet }),
    } = &cli.command
    {
        let path = settings::path()?;
        if let Some(switch) = switch {
            settings::set_lead_compact(&path, matches!(switch, Switch::On))?;
        }
        if let Some(quiet) = quiet {
            settings::set_lead_quiet(&path, *quiet)?;
        }
        let (on, quiet) = (settings::lead_compact(&path)?, settings::lead_quiet(&path)?);
        anstream::println!("{}", view::lead_compact(on, quiet, &path));
        return Ok(());
    }
    if let Command::Watch {
        command: Some(WatchCommand::Limit { seconds }),
        ..
    } = &cli.command
    {
        let path = settings::path()?;
        if let Some(seconds) = seconds {
            settings::set_watch_limit(&path, *seconds)?;
        }
        let limit = settings::watch_limit(&path)?;
        anstream::println!("{}", view::watch_limit(limit, &path));
        return Ok(());
    }
    if let Command::Statusline = cli.command {
        println!("{}", statusline(&server).await);
        return Ok(());
    }
    if let Command::Connect {
        tool: Tool::Claude { claude, scope },
    } = &cli.command
    {
        let settings = plugin::user_settings();
        let connected = plugin::connect(claude, &plugin::dir()?, settings.as_deref())?;
        let scoped = connect_scope(settings.as_deref(), scope.map(Into::into))?;
        println!("{}", text::connected(&connected, &scoped));
        match login::ensure(&Api::new(&server), open_browser).await {
            Ok(Some(_)) => println!("{}", text::connect_signed_in(&server)),
            Ok(None) => {}
            Err(e) => anstream::eprintln!("riff: {}", text::connect_no_sign_in(&server, &e)),
        }
        ask_auto_update();
        return Ok(());
    }
    if let Command::Setup { check } = cli.command {
        return setup(check);
    }
    if let Command::Enable { place } | Command::Disable { place } = &cli.command {
        let on = matches!(cli.command, Command::Enable { .. });
        println!("{}", text::enabled(&set_enabled(place.place(), on)?, on));
        return Ok(());
    }
    if let Command::Workers { command, long } = &cli.command {
        return workers(command.as_ref(), *long, &server).await;
    }
    if let Command::Pr {
        command: Pr::Wait { number, every },
    } = &cli.command
    {
        eprintln!("{}", text::pr_waits(*number));
        let every = Duration::from_secs(*every);
        println!("{}", pr::wait(&pr::Gh::default(), *number, every)?);
        // The merge is done: the total never fails the wait.
        match total_after_merge(*number) {
            Ok(line) => eprintln!("{line}"),
            Err(e) => eprintln!("riff: cannot write the total of the tokens: {e:#}"),
        }
        return Ok(());
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
            let had = login::logout(api.base())?;
            println!("{}", text::signed_out(had, api.base()));
            return Ok(());
        }
        Command::Logout { all: true, user } => {
            let done = login::logout_all(&api, user.as_deref()).await?;
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
    let me = identity::me(&here, api.base())?;
    // A session that left makes no call (01M3MEEFETT9A0DRWBKQTG77Z2).
    // `riff mcp` still runs: its `join` tool brings the session back.
    if let Some(id) = me.who().session()
        && local::left_here(id)
    {
        match cli.command {
            Command::Mcp { .. } => {}
            Command::Watch { .. } => {
                println!("{}", text::WATCH_LEFT);
                return Ok(());
            }
            _ => anyhow::bail!(text::LEFT_COMMAND),
        }
    }
    let api = api.signed_in(me.who().session())?;
    match cli.command {
        Command::Whoami => {
            let pauses = api.pauses(&me).await.map_err(|e| format!("{e:#}"));
            anstream::print!("{}", view::whoami(&me, pauses));
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
        Command::Status { blocked, step } => {
            let status = Status {
                step: step.join(" "),
                blocked,
            };
            api.status(&me, &status).await?;
            println!("{}", text::status_set(&status));
        }
        Command::Tell { session, body } => {
            let posted = api.tell(&me, &session, &body.join(" ")).await?;
            println!("{}", text::posted(&posted));
        }
        Command::Read { thread, all } => {
            if let Some(repo) = here.default_thread() {
                api.join(&me, &repo).await?;
            }
            let thread = thread.map(|t| t.parse::<ThreadName>()).transpose()?;
            let inbox = api.inbox(&me, thread.as_ref(), all).await?;
            println!("{}", text::inbox(&inbox, &me).trim_end());
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
            if let Some(line) = dropped::at_claim(&identity::working_dir()?, &item).await {
                println!("{line}");
            }
        }
        Command::Release {
            thread,
            session,
            item,
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
        Command::Lead { command: None } => println!("{}", text::led(&api.lead(&me).await?)),
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
            let to: Selector = format!("claim=issue-{}", reported.issue).parse()?;
            let body = text::verify_post(verdict, number, &reported, &result);
            let posted = api
                .post(&me, Some(&thread), &[to], &body, Kind::Message)
                .await?;
            println!("{}", text::posted(&posted));
        }
        Command::Tail { thread } => {
            tail(&api, &me, &thread_or_default(thread, &here)?, &here).await;
        }
        Command::Top { once } => top(&api, &me, here.default_thread(), once).await?,
        Command::Watch { once, until, .. } => {
            let me = identity::session(&here, api.base())?;
            let Some(_lock) = lock_watch(&me) else {
                println!("{}", text::WATCH_RUNS);
                std::process::exit(1);
            };
            watch(&api, &me, once, watch_limit(once), until).await
        }
        Command::Mcp { client } => {
            let me = identity::session(&here, api.base())?;
            let _record = record_session(&me);
            let (registered, wait) = tokio::sync::oneshot::channel();
            let tail = async {
                // A session that left the riff sends nothing.
                if wait.await.is_ok() {
                    tail_beside_lead(&api, &me).await;
                }
            };
            let serve = mcp::serve(api.clone(), me.clone(), client.as_deref(), registered);
            let (_, served) = tokio::join!(tail, serve);
            served?
        }
        Command::Hook { .. }
        | Command::Usage { .. }
        | Command::Statusline
        | Command::Connect { .. }
        | Command::Setup { .. }
        | Command::Enable { .. }
        | Command::Disable { .. }
        | Command::Server
        | Command::Update { .. }
        | Command::Workers { .. }
        | Command::Lead {
            command: Some(LeadCommand::Compact { .. }),
        }
        | Command::Pr {
            command: Pr::Wait { .. },
        }
        | Command::Login
        | Command::Logout { .. }
        | Command::Invite { .. }
        | Command::Remove { .. }
        | Command::Members
        | Command::Chat { .. }
        | Command::Admin { .. }
        | Command::Owner { .. } => unreachable!("handled before the identity"),
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

/// `riff workers` and its subcommands.
async fn workers(command: Option<&Workers>, long: bool, server: &str) -> Result<()> {
    match command {
        None => list_workers(long, server).await,
        Some(Workers::Start {
            count,
            host: Some(host),
            ..
        }) => ask_host(host, riff::host::Request::Start(*count), server).await,
        Some(Workers::Start { count, claude, .. }) => start_workers(*count, claude, server).await,
        Some(Workers::Host { claude, session }) => {
            riff::host::serve(
                &identity::working_dir()?,
                claude,
                server,
                session.as_deref(),
            )
            .await
        }
        Some(Workers::Stop {
            pane,
            host: Some(host),
        }) => {
            let request = match pane {
                Some(one) => riff::host::Request::StopOne(one.clone()),
                None => riff::host::Request::Stop,
            };
            ask_host(host, request, server).await
        }
        Some(Workers::Idle { per_host, after }) => {
            let here = identity::place(&identity::working_dir()?)?;
            // Only a person changes the settings: a change goes as the
            // person, also inside an agent session
            // (01M3WRD959DYNZHDKP5ZT9Q1C7). A read goes as the caller.
            let set = per_host.is_some() || after.is_some();
            let me = if set {
                identity::person(&Place::host_only(here.host())?, server)?
            } else {
                identity::me(&here, server)?
            };
            let api = Api::new(server).signed_in(me.who().session())?;
            let idle = api.idle(&me, *per_host, *after).await?;
            println!("{}", text::idle_workers(&idle));
            Ok(())
        }
        Some(Workers::Limit { limit }) => {
            let path = settings::path()?;
            if let Some(limit) = limit {
                settings::set_workers_limit(&path, *limit)?;
            }
            anstream::println!(
                "{}",
                view::workers_limit(settings::workers_limit(&path)?, &path)
            );
            Ok(())
        }
        Some(Workers::Interval { seconds }) => {
            let path = settings::path()?;
            if let Some(seconds) = seconds {
                settings::set_workers_interval(&path, *seconds)?;
            }
            anstream::println!(
                "{}",
                view::workers_interval(settings::workers_interval(&path)?, &path)
            );
            Ok(())
        }
        Some(Workers::Jobs { jobs }) => {
            let path = settings::path()?;
            if let Some(jobs) = jobs {
                settings::set_workers_jobs(&path, *jobs)?;
            }
            let machine = riff::machine::Machine::here();
            anstream::println!(
                "{}",
                view::workers_jobs(
                    settings::workers_jobs(&path)?,
                    riff::limits::Limits::of(&path, &machine)?.jobs,
                    &path
                )
            );
            Ok(())
        }
        Some(Workers::Nice { nice }) => {
            let path = settings::path()?;
            if let Some(nice) = nice {
                settings::set_workers_nice(&path, *nice)?;
            }
            anstream::println!(
                "{}",
                view::workers_nice(settings::workers_nice(&path)?, &path)
            );
            Ok(())
        }
        Some(Workers::Memory { gb }) => {
            let path = settings::path()?;
            if let Some(gb) = gb {
                settings::set_workers_memory(&path, *gb)?;
            }
            let setting = settings::workers_memory(&path)?;
            let machine = riff::machine::Machine::here();
            anstream::println!(
                "{}",
                view::workers_memory(
                    setting,
                    riff::limits::memory(machine.mem_gb, setting),
                    &path
                )
            );
            Ok(())
        }
        Some(Workers::Floor { gb }) => {
            let path = settings::path()?;
            if let Some(gb) = gb {
                settings::set_workers_floor(&path, *gb)?;
            }
            anstream::println!(
                "{}",
                view::workers_floor(
                    settings::workers_floor(&path)?,
                    riff::machine::Machine::here().avail_gb,
                    &path
                )
            );
            Ok(())
        }
        Some(Workers::Mcp { command }) => {
            let path = settings::path()?;
            let mut names = settings::workers_mcp(&path)?;
            match command {
                None => {}
                Some(WorkersMcp::Add { name }) => {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                    settings::set_workers_mcp(&path, &names)?;
                }
                Some(WorkersMcp::Remove { name }) => {
                    if name == settings::RIFF_MCP {
                        anyhow::bail!(text::WORKERS_MCP_KEEPS_RIFF);
                    }
                    names.retain(|n| n != name);
                    settings::set_workers_mcp(&path, &names)?;
                }
            }
            anstream::println!("{}", view::workers_mcp(&names, &path));
            Ok(())
        }
        Some(Workers::Stop { pane, .. }) => stop_workers(pane.as_deref(), server).await,
        Some(Workers::Run { claude, args }) => {
            std::process::exit(worker::run(claude, args, server).await?)
        }
    }
}

/// Starts `count` workers in tmux (01M3JD392Q5ANX0FPZ51W7B0E3), at most
/// the limit of the machine minus the workers that run
/// (01M3JPQT57PJCRBQYJNDVESS04). It refuses in a worker and in an agent
/// session that is not the lead (01M3JPQT79FE47518Z8DFFQYYG). Outside
/// tmux, it starts nothing and fails (01M3JD3973J7A9BG8G9EP9TVDP).
async fn start_workers(count: u16, claude: &std::path::Path, server: &str) -> Result<()> {
    // A worker in a repository with riff off has no riff
    // (01M3XY2T542DCHBN95H9PX4AGQ). `worker::start` checks it for each
    // start; here the refusal comes before each other one.
    if let Some(why) = worker::off(&identity::working_dir()?) {
        eprintln!("{why}");
        std::process::exit(1);
    }
    if let Some(why) = start_refusal(server).await {
        eprintln!("{why}");
        std::process::exit(1);
    }
    let Some(tmux) = Tmux::from_env() else {
        eprintln!("{}", text::NO_TMUX);
        std::process::exit(1);
    };
    let started = match worker::start(&tmux, count, claude, server, &identity::working_dir()?)? {
        Ok(started) => started,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    };
    if let Some(line) = &started.fresh {
        println!("{line}");
    }
    let n = u16::try_from(started.panes.len()).unwrap_or(u16::MAX);
    println!(
        "{}",
        text::workers_started(n, &started.window, &started.main)
    );
    for line in [&started.limited, &started.no_scope].into_iter().flatten() {
        println!("{line}");
    }
    Ok(())
}

/// Why this process may not start workers, or `None` when it may
/// (01M3JPQT79FE47518Z8DFFQYYG). A person in a plain terminal may. A
/// worker may not. An agent session may only when it is the lead.
async fn start_refusal(server: &str) -> Option<String> {
    if std::env::var("RIFF_WORKER").is_ok_and(|v| v == "1") {
        return Some(text::WORKER_STARTS_NO_WORKER.into());
    }
    let id = identity::session_id()?;
    let lead = async {
        let here = identity::place(&identity::working_dir()?)?;
        let api = Api::new(server);
        let me = identity::agent(&here, &id, api.base())?;
        let sessions = api.signed_in(Some(&id))?.who(&me, false).await?;
        anyhow::Ok(
            sessions
                .iter()
                .any(|s| s.uri.who() == me.who() && s.uri.lead()),
        )
    };
    match lead.await {
        Ok(true) => None,
        Ok(false) => Some(text::NOT_THE_LEAD_STARTS_NO_WORKER.into()),
        Err(_) => Some(text::LEAD_UNKNOWN_STARTS_NO_WORKER.into()),
    }
}

/// The Stop hook (01M3JQCCZ5M9VY3RGXWJYJN9Q9). In a worker, it starts
/// the check of the clear of its context
/// (01M3XV0562D3H3P22CJDBPAZBH). In a session that is not a worker, it
/// starts the check of the compact of the lead
/// (01M3Q88G1K7N2EMPBA07X069A7). It never fails.
fn stop_hook() {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: next::StopInput = serde_json::from_str(&stdin).unwrap_or_default();
    let Some(id) = identity::agent_session(input.session_id) else {
        return;
    };
    count_usage(&id, input.transcript_path.as_deref(), false);
    if !riff::worker::is_worker() {
        if let Err(e) = start_compact_check(&id, input.transcript_path.as_deref()) {
            eprintln!("riff: cannot check the compact of the lead: {e:#}");
        }
        return;
    }
    if let Err(e) = start_clear_check(&id, input.transcript_path.as_deref()) {
        eprintln!("riff: cannot check the clear of the worker: {e:#}");
    }
}

/// Starts `riff hook clear` for the worker `id`, detached, so that the
/// Stop hook returns at once (01M3JQCCZ5M9VY3RGXWJYJN9Q9). It starts
/// nothing outside tmux, or when the session left the riff.
fn start_clear_check(id: &str, transcript: Option<&std::path::Path>) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let in_tmux = std::env::var_os("TMUX").is_some_and(|t| !t.is_empty());
    let pane = std::env::var("TMUX_PANE").ok().filter(|p| !p.is_empty());
    let Some(pane) = pane.filter(|_| in_tmux) else {
        return Ok(());
    };
    if local::left_here(id) {
        return Ok(());
    }
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.args(["hook", "clear", "--session", id, "--pane", &pane]);
    if let Some(transcript) = transcript {
        cmd.arg("--transcript").arg(transcript);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}

/// The check of the clear of the worker `id` in the pane `pane`
/// ([`next::check`], 01M3XV0562D3H3P22CJDBPAZBH).
async fn clear_check(
    id: &str,
    pane: &str,
    transcript: Option<&std::path::Path>,
    server: &str,
) -> Result<()> {
    let dir = identity::working_dir()?;
    let here = identity::place(&dir)?;
    let api = Api::new(server);
    let me = identity::agent(&here, id, api.base())?;
    let api = api.signed_in(Some(id))?;
    next::check(&api, &me, pane, &dir, transcript).await?;
    Ok(())
}

/// Records the transcript of the session `id`. With `ends`, it starts
/// `riff hook usage`, detached, when the marks of the session have an
/// open claim: a new start and the end of a session free each claim
/// (01M3Y1YP1ZA5TBRA01MKWM3VC6). A hook never waits for the report, and
/// a failure goes to stderr.
fn count_usage(id: &str, transcript: Option<&std::path::Path>, ends: bool) {
    use std::os::unix::process::CommandExt;
    let Some(meter) = usage::Meter::here() else {
        return;
    };
    if let Some(transcript) = transcript {
        meter.saw(id, transcript);
    }
    if !ends || !meter.holds(id) {
        return;
    }
    let started = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .args(["hook", "usage", "--session", id, "--before"])
            .arg(usage::now_ms().to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
    });
    if let Err(e) = started {
        eprintln!("riff: cannot report the tokens of the claims: {e}");
    }
}

/// The text of `riff usage` ([`usage`]): of `issue`, of each issue of
/// `wave`, or of the sessions of this machine.
fn usage_text(issue: Option<&str>, wave: Option<&str>) -> Result<String> {
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
        let counted = usage::counted(number, &forge.comments(number)?);
        return Ok(usage::issue_text(number, &counted));
    }
    let wave = wave.unwrap_or_default();
    let mut rows = Vec::new();
    for (number, title) in forge.wave(wave)? {
        let counted = usage::counted(number, &forge.comments(number)?);
        rows.push((number, title, usage::sum(&counted)));
    }
    Ok(usage::wave_text(wave, &rows))
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

/// Starts `riff hook compact` for the session `id`, detached, so that
/// the Stop hook returns at once (01M3Q88G1K7N2EMPBA07X069A7). It starts
/// nothing when `lead.compact` is off, or the session left the riff.
fn start_compact_check(id: &str, transcript: Option<&std::path::Path>) -> Result<()> {
    use std::os::unix::process::CommandExt;
    if !settings::lead_compact(&settings::path()?)? || local::left_here(id) {
        return Ok(());
    }
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.args(["hook", "compact", "--session", id]);
    if let Some(transcript) = transcript {
        cmd.arg("--transcript").arg(transcript);
    }
    if std::env::var_os("TMUX").is_some_and(|t| !t.is_empty())
        && let Ok(pane) = std::env::var("TMUX_PANE")
    {
        cmd.args(["--pane", &pane]);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}

/// Lists the workers of this machine, with their claims and status in
/// `riff who` (01M3JPQTBDGT54WN7FZP9CD6B5).
async fn list_workers(long: bool, server: &str) -> Result<()> {
    let panes = Tmux::machine().worker_panes()?;
    let who = async {
        let here = identity::place(&identity::working_dir()?)?;
        let api = Api::new(server);
        let me = identity::me(&here, api.base())?;
        let api = api.signed_in(me.who().session())?;
        let riff = api.riff(&me).await?;
        let mut sessions = api.who(&me, false).await?;
        riff::state::fill(&mut sessions, riff);
        Ok::<_, anyhow::Error>(sessions)
    };
    // With no worker here, a riff that does not answer only hides the
    // hosts.
    let sessions = who.await.unwrap_or_else(|e| {
        if !panes.is_empty() {
            eprintln!("riff: cannot read riff who: {e:#}");
        }
        Vec::new()
    });
    let settings = settings::path()?;
    let limit = settings::workers_limit(&settings)?;
    let floor = settings::workers_floor(&settings)?;
    let me =
        identity::place(&identity::working_dir()?).and_then(|here| identity::me(&here, server));
    let host = me
        .as_ref()
        .map_or_else(|_| identity::this_host(), |me| me.place().host().to_owned());
    let machine = riff::machine::Machine::here();
    anstream::println!(
        "{}",
        view::host_heading(&host, limit, panes.len(), Some(&machine), floor)
    );
    anstream::print!("{}", view::workers(&panes, &sessions, long));
    let Ok(me) = me else {
        return Ok(());
    };
    for (info, status) in riff::host::hosts(&sessions, me.who().user()) {
        let host = info.uri.place().host();
        if host == me.place().host() {
            continue;
        }
        let panes = riff::host::panes(&status, &sessions);
        anstream::println!(
            "\n{}",
            view::host_heading(
                host,
                status.limit,
                status.workers.len(),
                status.machine.as_ref(),
                status.floor
            )
        );
        anstream::print!("{}", view::workers(&panes, &sessions, long));
    }
    Ok(())
}

/// Asks the workers host on `host` for `request`
/// (01M3N7AKB3KXS2XYK0309C4M18). The host acts only on a request of the
/// lead of its user (01M3N7AKDE7DEA6NXS9ZMECRMH), so riff sends it only
/// from an agent session, and a start only from the lead.
async fn ask_host(host: &str, request: riff::host::Request, server: &str) -> Result<()> {
    if matches!(request, riff::host::Request::Start(_))
        && let Some(why) = start_refusal(server).await
    {
        eprintln!("{why}");
        std::process::exit(1);
    }
    let Some(id) = identity::session_id() else {
        eprintln!("{}", text::HOST_NEEDS_THE_LEAD);
        std::process::exit(1);
    };
    let here = identity::place(&identity::working_dir()?)?;
    let api = Api::new(server);
    let me = identity::agent(&here, &id, api.base())?;
    let api = api.signed_in(Some(&id))?;
    let sessions = api.who(&me, false).await?;
    let found = riff::host::hosts(&sessions, me.who().user())
        .into_iter()
        .find_map(|(s, _)| (s.uri.place().host() == host).then(|| s.uri.who().session()))
        .flatten();
    let Some(to) = found else {
        eprintln!("{}", text::no_host(host));
        std::process::exit(1);
    };
    api.tell(&me, to, &request.to_string()).await?;
    println!("{}", text::host_asked(host, &request));
    Ok(())
}

/// Ends each worker of this machine, or the one in `pane`: it kills the
/// pane, then sends the end call of the session
/// (01M3JPQTDFW3C7QBSZZ2M831MH).
async fn stop_workers(pane: Option<&str>, server: &str) -> Result<()> {
    let stopped = worker::stop(&Tmux::machine(), pane, server).await?;
    println!("{}", text::workers_stopped(stopped));
    Ok(())
}

/// In tmux, adds the `riff tail` pane beside the lead
/// (01M3JD390F49HZSKEJ3VACX0ZA). Call it after the register of
/// `riff mcp` ends: only that register makes the lead
/// (01M3XM68N5M5DKB86W5079X2G9). An error goes to stderr: the tools
/// still work.
async fn tail_beside_lead(api: &Api, me: &SessionUri) {
    let Some(tmux) = Tmux::from_env() else {
        return;
    };
    let added = async {
        let program = Program::tail(
            &riff::binary::this_on_disk()?,
            &identity::working_dir()?,
            api.base(),
        );
        terminal::tail_beside_lead(api, me, &tmux, &program).await
    };
    if let Err(e) = added.await {
        eprintln!("riff: cannot add the riff tail pane: {e:#}");
    }
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

/// The SessionStart hook output. It has no URI when riff cannot find the
/// session, and no riff state when it cannot read it in
/// [`hook::STATE_WAIT`], but it always has the context (R69). It has a
/// line when the clone is behind `origin` ([`hook::behind`]), and a line
/// at a new start in a linked worktree ([`hook::Linked`]).
async fn session_start(server: &str) -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: hook::StartInput = serde_json::from_str(&stdin).unwrap_or_default();
    let id = identity::agent_session(input.session_id);
    // A session that left gets no riff context (01M3MEEFETT9A0DRWBKQTG77Z2).
    if id.as_deref().is_some_and(local::left_here) {
        return String::new();
    }
    if let Some(id) = &id {
        let transcript = input.transcript_path.as_deref();
        count_usage(id, transcript, input.source.is_new_start());
    }
    let api = Api::new(server);
    let cwd = std::env::current_dir().ok();
    let uri = id.as_deref().zip(cwd.as_deref()).and_then(|(id, cwd)| {
        let here = identity::place(cwd).ok()?;
        identity::agent(&here, id, api.base()).ok()
    });
    let facts = async {
        match uri {
            Some(uri) => {
                let facts = start_facts(api, &uri, input.source.reason());
                match tokio::time::timeout(hook::STATE_WAIT, facts).await {
                    Ok(Ok((lead, pauses, freed, who))) => (
                        Some(uri.with_lead(lead)),
                        Some(pauses),
                        freed,
                        Some(who),
                        None,
                    ),
                    Ok(Err(e)) => (
                        Some(uri),
                        None,
                        Vec::new(),
                        None,
                        e.downcast::<Mismatch>().ok(),
                    ),
                    Err(_) => (Some(uri), None, Vec::new(), None, None),
                }
            }
            None => (None, None, Vec::new(), None, None),
        }
    };
    let behind = async {
        match &cwd {
            Some(cwd) => hook::behind(cwd, hook::FETCH_WAIT).await,
            None => None,
        }
    };
    let linked = async {
        match &cwd {
            Some(cwd) if input.source.is_new_start() => hook::linked(cwd).await,
            _ => None,
        }
    };
    let ((uri, riff, freed, who, mismatch), behind, linked) = tokio::join!(facts, behind, linked);
    let others = uri
        .as_ref()
        .zip(who.as_ref())
        .map(|(me, who)| hook::others_here(me, who));
    let watching = id
        .as_deref()
        .zip(local::dir())
        .is_some_and(|(id, dir)| local::watching(&dir, id));
    let mismatch_free = mismatch.is_none();
    let mut context = match mismatch {
        Some(mismatch) => hook::mismatch_context(uri.as_ref(), &mismatch),
        None => hook::start_context(uri.as_ref(), input.source, watching, riff.as_ref(), &freed),
    };
    if let Some(behind) = behind {
        context.push_str(&behind.line());
    }
    if let Some(linked) = linked.filter(|_| mismatch_free) {
        context.push_str(&linked.line(others.as_deref()));
    }
    // After the fetch of `behind`, so the list has the pushed branches
    // of now (01M3WFYETKXPWWE0R0EAKGCD1E).
    if let (Some(cwd), Some(me), Some(who)) = (&cwd, &uri, &who)
        && input.source.is_new_start()
        && mismatch_free
    {
        let free = dropped::without_owner(dropped::all(cwd), me, who);
        if let Some(lines) = dropped::start_lines(&free) {
            context.push_str(&lines);
        }
    }
    if worker::is_worker() {
        context.push_str(hook::WORKER_LINE);
    }
    if let Some(line) = hook::on_line(&enable::State::here()) {
        context.push_str(&line);
    }
    if mcp_off_here() {
        context.push_str(hook::MCP_OFF_LINE);
    }
    if let Some(cwd) = cwd
        .as_deref()
        .filter(|_| uri.as_ref().is_some_and(SessionUri::lead))
    {
        let project = permissions::Project::of(cwd);
        let user = plugin::settings_from(
            std::env::var_os("CLAUDE_CONFIG_DIR"),
            std::env::var_os("HOME"),
        );
        if let Some(line) = hook::rules_line(&project.missing(user.as_deref()), &project.top) {
            context.push_str(&line);
        }
    }
    hook::start_output(&context)
}

/// Whether the server names `me` as the lead, the pauses of the riff
/// (01M3JCG48QPCNNTKW34FTR0AMR), the claims that a new start freed
/// (01M3JEE1QQCFS5TMZW5N2DAD2D), and the sessions of the riff. With a
/// `reason`, it sends the start call first, with the reason and the
/// worker mark of the session (01M3X9X9M079WGFPJZHNXH9VEP). A compaction has no reason,
/// and sends no start.
async fn start_facts(
    api: Api,
    me: &SessionUri,
    reason: Option<StartReason>,
) -> Result<(bool, RiffReply, Vec<Freed>, Vec<SessionInfo>)> {
    let api = api.signed_in(me.who().session())?;
    let freed = match reason {
        Some(reason) => api.start(me, reason, worker::is_worker()).await?,
        None => Vec::new(),
    };
    let riff = api.pauses(me).await?;
    let who = api.who(me, false).await?;
    let lead = who.iter().any(|s| s.uri.who() == me.who() && s.uri.lead());
    Ok((lead, riff, freed, who))
}

/// `riff setup`: adds the missing permission rules of riff to the
/// project settings, or with `check`, names them
/// (01M3Q53RNDJBDHVDFHJ9HCX9S1).
fn setup(check: bool) -> Result<()> {
    let project = permissions::Project::of(&identity::working_dir()?);
    let user = plugin::settings_from(
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

/// `riff enable` (`on` true) or `riff disable` for the working
/// directory (01M3XY2SKQ27K3TE4NV28FHTVV). A change of the user
/// settings is a choice of the person, so riff keeps it as the answer
/// to the scope question: an update then keeps it.
fn set_enabled(place: enable::Place, on: bool) -> Result<enable::Changed> {
    let dir = identity::working_dir()?;
    let user = plugin::user_settings();
    let done = if on {
        enable::enable(&dir, place, user.as_deref())?
    } else {
        enable::disable(&dir, place, user.as_deref())?
    };
    if place == enable::Place::Global {
        let scope = if on {
            enable::Scope::Global
        } else {
            enable::Scope::None
        };
        settings::set_connect_scope(&settings::path()?, scope)?;
    }
    Ok(done)
}

/// The scope step of `riff connect claude` ([`enable::scope`]). It asks
/// the scope question only in a terminal.
fn connect_scope(
    user: Option<&std::path::Path>,
    flag: Option<enable::Scope>,
) -> Result<enable::Scoped> {
    use std::io::IsTerminal;
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let riff = settings::path()?;
    let claude = enable::claude_state();
    let files = enable::Files {
        user,
        riff: &riff,
        claude: claude.as_deref(),
    };
    enable::scope(&identity::working_dir()?, files, flag, || {
        enable::ask_scope(
            terminal,
            &mut std::io::stdin().lock(),
            &mut std::io::stdout(),
        )
    })
}

/// True when a person turned the riff server off for the project of
/// the working directory in `/mcp` (01M3XY2T0R2Q39XYX8AYV7T0RK).
fn mcp_off_here() -> bool {
    let Ok(dir) = std::env::current_dir() else {
        return false;
    };
    let repo = enable::Repo::of(&dir);
    enable::mcp_off_in(enable::claude_state().as_deref(), &dir, repo.as_ref())
}

/// The status line of the Claude Code session on stdin
/// ([`text::statusline`]). It finds the session like a hook does, and
/// asks riff-server for only that session with `GET /v1/me`, not `who`
/// (01M3T5GFVS8NMA992KHZN4VE17). It never fails, and it waits at most
/// [`STATUSLINE_WAIT`] for riff-server.
async fn statusline(server: &str) -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: hook::StartInput = serde_json::from_str(&stdin).unwrap_or_default();
    let Some(id) = identity::agent_session(input.session_id) else {
        return "riff: no session".into();
    };
    if local::left_here(&id) {
        return text::statusline_left(&id);
    }
    if mcp_off_here() {
        return text::statusline_mcp_off(&id);
    }
    let find = async {
        let here = identity::place(&identity::working_dir()?)?;
        let api = Api::new(server);
        let me = identity::agent(&here, &id, api.base())?;
        anyhow::Ok(api.signed_in(me.who().session())?.me(&me).await?.session)
    };
    let found = tokio::time::timeout(STATUSLINE_WAIT, find).await;
    // The build of the server comes with its answer: no extra call
    // (01M3NJCWDN5APKZ3Z53XQR8P0B).
    let server = match &found {
        Ok(Err(e)) => e.downcast_ref::<Mismatch>().and_then(|m| m.server.clone()),
        _ => riff::api::server_build(),
    };
    let info = found.ok().and_then(Result::ok).flatten();
    let line = text::statusline(&id, info.as_ref());
    match server.and_then(|server| update_tag(&server)) {
        Some(tag) => format!("{line} {}", text::update_tag(&tag)),
        None => line,
    }
}

/// The tag of the status line for a riff of `server`
/// (01M3NT6X22A4GNFTNKRYV8Z4N1): the build of the `riff mcp` of the
/// session, else of this riff, against the server. It reads only local
/// files.
fn update_tag(server: &Build) -> Option<auto_update::Tag> {
    let installed = Build::this();
    let dir = local::dir();
    let session = match dir.as_deref().and_then(local::recorded_build_above) {
        Some(recorded) => recorded,
        None => Some(installed.clone()),
    };
    let tried = dir.as_deref().and_then(local::tried);
    let machine = auto_update::Machine {
        auto: settings::path().is_ok_and(|p| settings::update_auto(&p).unwrap_or(false)),
        updating: dir.as_deref().is_some_and(local::updating),
        tried: tried.as_deref(),
    };
    auto_update::tag(session.as_ref(), &installed, server, machine)
}

/// The SessionEnd hook: the end call for the session (R205). It never
/// fails: an error goes to stderr.
async fn session_end(server: &str) {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: hook::EndInput = serde_json::from_str(&stdin).unwrap_or_default();
    if !input.ends_the_session() {
        return;
    }
    let Some(id) = identity::agent_session(input.session_id) else {
        return;
    };
    if local::left_here(&id) {
        return;
    }
    count_usage(&id, input.transcript_path.as_deref(), true);
    let ended = async {
        let here = identity::place(&identity::working_dir()?)?;
        let api = Api::new(server);
        let me = identity::agent(&here, &id, api.base())?;
        api.signed_in(me.who().session())?.end(&me).await
    };
    match tokio::time::timeout(mcp::END_WAIT, ended).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("riff: {e:#}"),
        Err(_) => eprintln!("riff: the end call took too long"),
    }
}

/// Takes the watch lock of the session `me` (R169). `None` when another
/// watch holds it. Without the lock file, the watch runs with no lock.
fn lock_watch(me: &SessionUri) -> Option<Option<local::Held>> {
    let (Some(dir), Some(id)) = (local::dir(), me.who().session()) else {
        return Some(None);
    };
    match local::watch(&dir, id) {
        Ok(Some(lock)) => Some(Some(lock)),
        Ok(None) => None,
        Err(e) => {
            eprintln!("riff: the watch runs with no lock: {e}");
            Some(None)
        }
    }
}

/// Records the session ID of `riff mcp` for its agent process, the
/// parent (R167). The record lasts while the result lives. Without it,
/// the tools still work.
fn record_session(me: &SessionUri) -> Option<local::Held> {
    let (dir, id) = (local::dir()?, me.who().session()?);
    local::record(&dir, std::os::unix::process::parent_id(), id)
        .inspect_err(|e| eprintln!("riff: cannot record the session ID: {e}"))
        .ok()
        .flatten()
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
/// (01M3MNVTC248YYJJQKFD9H1WY9).
async fn tail(api: &Api, me: &SessionUri, thread: &ThreadName, here: &Place) {
    tokio::select! {
        () = tail_each(api, me, thread) => {}
        () = binary::follow_update(here) => {}
    }
}

/// Prints each message of `thread`. It connects again at once when the
/// stream ends, and shows only a short dim line while a connect fails,
/// or the error in red for a version that it cannot talk to
/// (01M3MNVTC248YYJJQKFD9H1WY9, 01M3NK7VHXB0PAR8VH8GQQA06K).
async fn tail_each(api: &Api, me: &SessionUri, thread: &ThreadName) {
    let error = riff::style::ERROR;
    anstream::eprintln!("riff: showing new messages in {thread}. Ctrl-C stops.");
    let mut stream = Box::pin(follow(|| api.tail(me, thread), RETRY));
    let mut link = Reconnect::default();
    let mut last_day = None;
    while let Some(item) = stream.next().await {
        if let Some(line) = link.line(&item) {
            anstream::eprintln!("{line}");
        }
        let Ok(checked) = item else { continue };
        let at = i64::try_from(checked.message.at_ms)
            .ok()
            .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
            .unwrap_or_else(chrono::Local::now);
        let block = text::block(&checked, &at, last_day, textwrap::termwidth());
        last_day = Some(at.date_naive());
        anstream::println!("{block}");
    }
    anstream::eprintln!("{error}riff: the stream of {thread} ended.{error:#}");
}

/// Draws [`riff::top::Top`] once, or again every
/// [`riff::top::REFRESH`] and after each message of `thread` until
/// stopped (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
/// (01M3NB589WMPRSAR43BSG9SP41). Until stopped, it runs a new binary
/// (01M3NT6WXGCNKW3EQ7MBJDQTR4).
async fn top(api: &Api, me: &SessionUri, thread: Option<ThreadName>, once: bool) -> Result<()> {
    if once {
        return draw_top(api, me, thread, once).await;
    }
    tokio::select! {
        drawn = draw_top(api, me, thread, once) => drawn,
        () = binary::follow_update(me.place()) => Ok(()),
    }
}

/// The loop of [`top`].
async fn draw_top(
    api: &Api,
    me: &SessionUri,
    thread: Option<ThreadName>,
    once: bool,
) -> Result<()> {
    use std::io::{IsTerminal, Write};
    use std::time::Instant;
    let fetch = |thread: Option<ThreadName>| {
        tokio::task::spawn_blocking(move || {
            thread.and_then(|t| riff::top::Issues::from_gh(&t.to_string()))
        })
    };
    let mut issues = fetch(thread.clone()).await?;
    let mut fetched = Instant::now();
    let mut messages = thread
        .as_ref()
        .map(|t| Box::pin(follow(|| api.tail(me, t), RETRY)));
    let clear = !once && std::io::stdout().is_terminal();
    let repo = thread.as_ref().map(ToString::to_string);
    loop {
        let pauses = api.pauses(me).await?;
        let mut who = api.roster(me, false).await?;
        riff::state::fill(&mut who.sessions, pauses.state);
        let server = riff::api::server_build();
        let top = riff::top::Top {
            pauses: &pauses,
            owner: &who.owner,
            server: server.as_ref(),
            sessions: &who.sessions,
            people: &who.people,
            issues: issues.as_ref(),
            repo: repo.as_deref(),
            width: textwrap::termwidth(),
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
        if fetched.elapsed() >= riff::top::ISSUES_TTL {
            issues = fetch(thread.clone()).await?;
            fetched = Instant::now();
        }
    }
}

/// Runs until stopped, or with `once` until the first wake (R170). It
/// connects again when the stream ends (R131). It stops when the session
/// leaves the riff (01M3MEEFETT9A0DRWBKQTG77Z2). It sends a keep-alive
/// each minute while it runs ([`api::keep_alive`],
/// 01M3WG240PNMQYZ7TX6Z7ZF6M9).
///
/// With a `limit`, it ends with one line and status 0 when no wake came
/// in that time (01M3Z64J08GW6N1H42AR2FZQZ4). On a new binary, it runs
/// it (01M3MNVTC248YYJJQKFD9H1WY9), and gives it the end of the wait in
/// `--until`: an update does not start the time again. `until` is that
/// end, from the watch of before an update.
async fn watch(
    api: &Api,
    me: &riff_core::name::SessionUri,
    once: bool,
    limit: Option<Duration>,
    until: Option<u64>,
) {
    let stream = follow(|| api.watch(me), RETRY);
    let left = async {
        let Some(id) = me.who().session() else {
            return std::future::pending().await;
        };
        while !local::left_here(id) {
            tokio::time::sleep(LEFT_POLL).await;
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let until = limit.map(|limit| {
        let until = until.map_or(now + limit, Duration::from_secs);
        (limit, until.min(now + limit))
    });
    let no_wake = async {
        match until {
            Some((limit, until)) => {
                tokio::time::sleep(until.saturating_sub(now)).await;
                println!("{}", text::watch_no_wake(limit));
            }
            None => std::future::pending().await,
        }
    };
    let update = async {
        let follow = binary::Follow::this();
        follow.new_one().await;
        let mut args = binary::with_place(std::env::args_os().skip(1), me.place());
        if let Some((_, until)) = until {
            args = binary::with_last(args, UNTIL_ARG, until.as_secs().to_string());
        }
        follow.run(args);
        std::future::pending().await
    };
    tokio::select! {
        () = print_each(stream, text::wake_line, once) => {}
        () = api::keep_alive(api, me, riff_core::wire::ALIVE_EVERY) => {}
        () = left => println!("{}", text::WATCH_LEFT),
        () = update => {}
        () = no_wake => {}
    }
}

/// The longest wait of a `riff watch --once`: `watch.limit`
/// (01M3Z64J08GW6N1H42AR2FZQZ4). `None` is no limit: a watch with no
/// `--once`, or the setting 0. A settings file that riff cannot read
/// gives the default, so that the watch still ends before the limit of
/// the harness.
fn watch_limit(once: bool) -> Option<Duration> {
    if !once {
        return None;
    }
    let secs = settings::path()
        .and_then(|path| settings::watch_limit(&path))
        .unwrap_or_else(|e| {
            eprintln!(
                "riff: the watch uses the limit of {} seconds: {e:#}",
                settings::WATCH_LIMIT
            );
            settings::WATCH_LIMIT
        });
    (secs > 0).then(|| Duration::from_secs(secs))
}

/// Prints one line for each item, or with `once` only the first line.
/// It reports a failed connect on stderr once, until the next item
/// comes. A version that it cannot talk to is a failed connect too: the
/// watch waits for an update (01M3MNVTC248YYJJQKFD9H1WY9).
async fn print_each<T>(
    stream: impl Stream<Item = Result<T>>,
    line: impl Fn(&T) -> String,
    once: bool,
) {
    let mut stream = Box::pin(stream);
    let mut reported = false;
    while let Some(item) = stream.next().await {
        match item {
            Ok(item) => {
                reported = false;
                println!("{}", line(&item));
                if once {
                    return;
                }
            }
            Err(e) if !reported => {
                eprintln!(
                    "riff: {e:#}. Trying again every {} seconds.",
                    RETRY.as_secs()
                );
                reported = true;
            }
            Err(_) => {}
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
