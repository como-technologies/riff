//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use chrono::TimeZone;
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use futures::{Stream, StreamExt};
use riff::api::{self, Api, DEFAULT_SERVER, follow};
use riff::terminal::{Program, Terminal, Tmux};
use riff::{
    binary, hook, hygiene, identity, lifecycle, local, login, mcp, next, plugin, settings,
    terminal, text, worker,
};
use riff_core::build::Mismatch;
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Freed, Kind, RiffState, Status};

/// The time between two tries to connect a stream.
const RETRY: Duration = Duration::from_secs(5);

/// The longest time that `riff statusline` waits for riff-server.
const STATUSLINE_WAIT: Duration = Duration::from_secs(2);

/// How often a watch looks for a leave of its session
/// (01M3MEEFETT9A0DRWBKQTG77Z2).
const LEFT_POLL: Duration = Duration::from_millis(250);

/// The local client that finds sessions and wakes yours.
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about)]
struct Cli {
    /// The riff-server: a URL, HOST or HOST:PORT. With no scheme, riff
    /// uses http, and port 7878 when there is no port. The default is
    /// the riff of this machine.
    #[arg(long, global = true, env = "RIFF_SERVER", default_value = DEFAULT_SERVER,
          value_parser = api::server_url)]
    server: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in with the provider of riff-server. It opens the browser.
    Login,
    /// Show your URI and the state of the riff. Inside Claude Code, it
    /// is the URI of the session.
    Whoami,
    /// Show the state of the riff and list its sessions. A session that
    /// ended, or stopped for 3 minutes, is gone and not listed.
    Who {
        /// List gone sessions too.
        #[arg(long)]
        all: bool,
        /// When to use color. `auto` uses color only when stdout is a
        /// terminal, and obeys NO_COLOR and CLICOLOR_FORCE.
        #[arg(long, value_enum, default_value_t = ColorWhen::Auto)]
        color: ColorWhen,
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
    /// Set your status: your current step. `riff who` shows it with its
    /// age. It replaces your old status.
    Status {
        /// You cannot go on. REASON says why.
        #[arg(long, value_name = "REASON")]
        blocked: Option<String>,
        /// Your current step, in one short line.
        #[arg(required = true)]
        step: Vec<String>,
    },
    /// Send a direct message to one session. It wakes that session.
    Tell {
        /// The session: its session ID or the start of it, as `riff read`
        /// shows it, its full riff:// URI from `riff who`, or `lead` for
        /// the lead of your user in this repository.
        session: String,
        /// The message.
        #[arg(required = true)]
        body: Vec<String>,
    },
    /// Show the unread messages of your threads. You join the thread of
    /// your repository first.
    Read {
        /// Read only this thread. It need not be one of your threads.
        #[arg(long, short)]
        thread: Option<String>,
        /// Show the full history, not only the unread messages.
        #[arg(long)]
        all: bool,
    },
    /// Show each new message in a thread, for people: a block for each
    /// message, with color in a terminal.
    Tail {
        /// The thread. The default is your repository thread.
        thread: Option<String>,
        /// When to use color. `auto` uses color only when stdout is a
        /// terminal, and obeys NO_COLOR and CLICOLOR_FORCE.
        #[arg(long, value_enum, default_value_t = ColorWhen::Auto)]
        color: ColorWhen,
    },
    /// Claim a work item so that no other session does the same work.
    /// Exits with status 1 when another session holds it.
    Claim {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
    },
    /// Release a work item that you claimed.
    Release {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The work item, for example issue-12.
        item: String,
    },
    /// Make this session the lead of your user in this repository. The
    /// other sessions of your user send their questions to the lead. It
    /// replaces the old lead. Run it in the agent session, for example
    /// `! riff lead` in Claude Code.
    Lead,
    /// Pause the riff. Each session stops at its next step and waits.
    /// Nobody claims work. Only you in a shell, or the lead, can pause.
    Pause,
    /// Resume the riff. Each session goes on from where it stopped.
    /// A new riff starts paused, so resume it to start the work.
    Resume,
    /// Print one line each time a post wakes this session. One watch
    /// runs for each session: a second one stops at once.
    Watch {
        /// Exit after the first wake. For a runner that wakes the
        /// session when the command exits.
        #[arg(long)]
        once: bool,
    },
    /// Serve the riff tools to an agent session over stdio.
    Mcp,
    /// Remove the sign-in at riff-server from this device. With --all,
    /// end each sign-in of a person on each device.
    Logout {
        /// End each sign-in, on each device.
        #[arg(long)]
        all: bool,
        /// With --all: the person. The default is you. Only an admin
        /// names another person.
        #[arg(long, requires = "all")]
        user: Option<String>,
    },
    /// Let a person join this riff: add their verified email to the
    /// members. They need no allowed domain. Only the owner or an admin
    /// can. It prints the address of the riff and the lines that the
    /// person runs to join.
    Invite {
        /// The email that the person signs in with.
        email: String,
    },
    /// Remove a member of this riff, and end each sign-in of that
    /// person. Only the owner or an admin can. The owner stays.
    Remove {
        /// The email of the person.
        email: String,
    },
    /// List who may join this riff: the owner, the admins, the members
    /// and the allowed domains.
    Members,
    /// Make a person an admin, or an admin a member again. An admin can
    /// invite and remove members. Only the owner can.
    Admin {
        #[command(subcommand)]
        command: Admin,
    },
    /// Pass the owner role to a member or an admin. You stay an admin.
    /// Only the owner can. An admin asks for the role with --take, and
    /// the owner keeps it with --deny.
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
    /// Print the status line of a Claude Code session: its short session
    /// ID, its claims, and `lead` or `blocked`. Claude Code runs it with
    /// the session on stdin. It always exits with status 0.
    Statusline,
    /// Run a Claude Code hook. The riff plugin calls it.
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Install the riff plugin in an agent tool. When the riff has
    /// sign-in and this machine has none, sign in. Run it again to update
    /// the plugin.
    Connect {
        #[command(subcommand)]
        tool: Tool,
    },
    /// Show the riff that riff uses, and where that choice comes from:
    /// --server, RIFF_SERVER, or the riff of this machine. For that riff
    /// and the riff of this machine: whether it answers, its build, and
    /// sign-in.
    Server,
    /// Update riff on this machine: install riff and riff-server of a
    /// release with cargo, then update the plugin with `riff connect
    /// claude`. It installs the release that the riff runs, or the newest
    /// release when riff uses the riff of this machine or cannot read the
    /// build of the riff. When the riff of
    /// this machine runs the old build, it tells you to start riff-server
    /// again.
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
    /// Start, list and stop the worker sessions of this machine. They
    /// need tmux. With no subcommand, it lists each worker: its pane,
    /// its session ID, its claims and its status
    Workers {
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
    /// Start COUNT worker sessions in the tmux window riff-workers, one
    /// pane each. Each pane runs `claude "Join the riff."` in the main
    /// worktree. It starts at most the limit minus the workers that run.
    /// It refuses in a worker, and in an agent session that is not the
    /// lead. Outside tmux, it starts nothing. With --host, the lead asks
    /// the workers host on that machine to start them.
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
    /// Offer the workers of this machine to the lead of your user. Run
    /// it in a tmux pane in the main clone, and leave it running. It
    /// starts and stops workers only on a verified request of that lead,
    /// at most the limit of this machine. Ctrl-C stops it
    Host {
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
    },
    /// Show or set the most workers on this machine. The default is 0,
    /// so no worker starts until you set it. It is in
    /// $XDG_CONFIG_HOME/riff/config.toml, key workers.limit
    Limit {
        /// The new limit. Leave it out to show the limit.
        limit: Option<u16>,
    },
    /// In a worker whose item is merged and released: ask for a fresh
    /// context. When the turn ends, riff gives the pane `/clear` and the
    /// start prompt, and the worker claims its next item
    Next,
    /// End each worker of this machine, or only the worker in PANE. Each
    /// worker leaves `riff who` and frees its claims at once. With
    /// --host, the lead asks the workers host on that machine to end
    /// each of its workers
    Stop {
        /// The tmux pane of one worker, for example %3. `riff workers`
        /// shows it.
        #[arg(conflicts_with = "host")]
        pane: Option<String>,
        /// Stop the workers on HOST, through its `riff workers host`.
        #[arg(long)]
        host: Option<String>,
    },
    /// Run CLAUDE as a worker, and wait. When it exits on its own, tell
    /// the lead the pane, the session ID and the exit code. It never
    /// starts CLAUDE again. Each worker pane runs it
    Run {
        /// The claude command.
        claude: std::path::PathBuf,
        /// The arguments of CLAUDE.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum HookEvent {
    /// Read the SessionStart input on stdin. Print the context that
    /// starts the watch. It always exits with status 0.
    SessionStart,
    /// Read the SessionEnd input on stdin. Tell riff-server that the
    /// session ended, unless the reason is clear. It always exits with
    /// status 0.
    SessionEnd,
    /// Read the Stop input on stdin. When the worker asked for its next
    /// item with `riff workers next`, give its pane `/clear` and the start
    /// prompt. It always exits with status 0.
    Stop,
}

#[derive(Subcommand)]
enum Tool {
    /// Install the riff plugin in Claude Code, in user scope.
    Claude {
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    if let Command::Server = cli.command {
        let source = match matches.value_source("server") {
            Some(ValueSource::CommandLine) => lifecycle::Source::Flag,
            Some(ValueSource::EnvVariable) => lifecycle::Source::Env,
            _ => lifecycle::Source::Default,
        };
        let view = lifecycle::view(&cli.server, DEFAULT_SERVER, source).await;
        println!("{}", text::server_view(&view));
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
            println!("{}", text::auto_update(settings::update_auto(&path)?));
            return Ok(());
        }
        if let (true, Some(tag)) = (background, tag) {
            return riff::auto_update::run(cargo, claude, tag, &cli.server, DEFAULT_SERVER).await;
        }
        println!(
            "{}",
            lifecycle::update(cargo, claude, tag.as_deref(), &cli.server, DEFAULT_SERVER).await?
        );
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionStart,
    } = cli.command
    {
        let output = session_start(&cli.server).await;
        if !output.is_empty() {
            println!("{output}");
        }
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionEnd,
    } = cli.command
    {
        session_end(&cli.server).await;
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::Stop,
    } = cli.command
    {
        stop_hook();
        return Ok(());
    }
    if let Command::Statusline = cli.command {
        println!("{}", statusline(&cli.server).await);
        return Ok(());
    }
    if let Command::Connect {
        tool: Tool::Claude { claude },
    } = &cli.command
    {
        let settings = plugin::settings_from(
            std::env::var_os("CLAUDE_CONFIG_DIR"),
            std::env::var_os("HOME"),
        );
        let connected = plugin::connect(claude, &plugin::dir()?, settings.as_deref())?;
        println!("{}", text::connected(&connected));
        match login::ensure(&Api::new(&cli.server), open_browser).await {
            Ok(Some(_)) => println!("{}", text::connect_signed_in(&cli.server)),
            Ok(None) => {}
            Err(e) => anstream::eprintln!("riff: {}", text::connect_no_sign_in(&cli.server, &e)),
        }
        return Ok(());
    }
    if let Command::Workers { command } = &cli.command {
        return workers(command.as_ref(), &cli.server).await;
    }
    let api = Api::new(&cli.server);
    match &cli.command {
        Command::Login => {
            let sign_in = login::login(&api, open_browser).await?;
            println!("{}", text::signed_in(&sign_in.user, api.base()));
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
        Command::Members => {
            println!("{}", text::members(&api.signed_in(None)?.members().await?));
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
    let here = identity::place(&std::env::current_dir()?)?;
    let me = identity::me(&here, api.base())?;
    // A session that left makes no call (01M3MEEFETT9A0DRWBKQTG77Z2).
    // `riff mcp` still runs: its `join` tool brings the session back.
    if let Some(id) = me.who().session()
        && local::left_here(id)
    {
        match cli.command {
            Command::Mcp => {}
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
            println!("{}  {me}", text::name(&me));
            match api.riff(&me).await {
                Ok(state) => println!(
                    "{}\n{}",
                    text::riff_state(state),
                    text::build_line(riff::api::server_build().as_ref())
                ),
                Err(e) => eprintln!("riff: cannot read the state of the riff: {e:#}"),
            }
        }
        Command::Who { all, color } => {
            use_color(color);
            let state = api.riff(&me).await?;
            let who = api.roster(&me, all).await?;
            anstream::print!("{}", text::who_view(state, &who.owner, &who.sessions, &me));
        }
        Command::Pause => pause(&api, &me, RiffState::Paused).await?,
        Command::Resume => pause(&api, &me, RiffState::Running).await?,
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
        }
        Command::Release { thread, item } => {
            let thread = thread_or_default(thread, &here)?;
            api.release(&me, &thread, &item).await?;
            println!("{}", text::released(&thread, &item));
        }
        Command::Lead => println!("{}", text::led(&api.lead(&me).await?)),
        Command::Tail { thread, color } => {
            tail(&api, &thread_or_default(thread, &here)?, color).await
        }
        Command::Watch { once } => {
            let me = identity::session(&here, api.base())?;
            let Some(_lock) = lock_watch(&me) else {
                println!("{}", text::WATCH_RUNS);
                std::process::exit(1);
            };
            watch(&api, &me, once).await
        }
        Command::Mcp => {
            let me = identity::session(&here, api.base())?;
            let _record = record_session(&me);
            let tail = async {
                if !me.who().session().is_some_and(local::left_here) {
                    tail_beside_lead(&api, &me).await;
                }
            };
            let (_, served) = tokio::join!(tail, mcp::serve(api.clone(), me.clone()));
            served?
        }
        Command::Hook { .. }
        | Command::Statusline
        | Command::Connect { .. }
        | Command::Server
        | Command::Update { .. }
        | Command::Workers { .. }
        | Command::Login
        | Command::Logout { .. }
        | Command::Invite { .. }
        | Command::Remove { .. }
        | Command::Members
        | Command::Admin { .. }
        | Command::Owner { .. } => unreachable!("handled before the identity"),
    }
    Ok(())
}

/// The person on this host, with no session and no repository. It posts
/// the note of a change of the members (01M3MN14ZCTRVD3T455P6TFK1B).
fn person(api: &Api) -> Result<SessionUri> {
    let here = identity::place(&std::env::current_dir()?)?;
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
async fn workers(command: Option<&Workers>, server: &str) -> Result<()> {
    match command {
        None => list_workers(server).await,
        Some(Workers::Start {
            count,
            host: Some(host),
            ..
        }) => ask_host(host, riff::host::Request::Start(*count), server).await,
        Some(Workers::Start { count, claude, .. }) => start_workers(*count, claude, server).await,
        Some(Workers::Host { claude }) => {
            riff::host::serve(&std::env::current_dir()?, claude, server).await
        }
        Some(Workers::Stop {
            host: Some(host), ..
        }) => ask_host(host, riff::host::Request::Stop, server).await,
        Some(Workers::Limit { limit }) => {
            let path = settings::path()?;
            if let Some(limit) = limit {
                settings::set_workers_limit(&path, *limit)?;
            }
            println!(
                "{}",
                text::workers_limit(settings::workers_limit(&path)?, &path)
            );
            Ok(())
        }
        Some(Workers::Stop { pane, .. }) => stop_workers(pane.as_deref(), server).await,
        Some(Workers::Next) => next_item(server).await,
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
    if let Some(why) = start_refusal(server).await {
        eprintln!("{why}");
        std::process::exit(1);
    }
    let Some(tmux) = Tmux::from_env() else {
        eprintln!("{}", text::NO_TMUX);
        std::process::exit(1);
    };
    let started = match worker::start(&tmux, count, claude, server, &std::env::current_dir()?)? {
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
    if let Some(line) = &started.limited {
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
        let here = identity::place(&std::env::current_dir()?)?;
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

/// Asks for a fresh context after the turn (01M3JQCCX22R4R4MN7XZPTS391):
/// only in a worker that is not the lead and holds no claims.
async fn next_item(server: &str) -> Result<()> {
    if !std::env::var("RIFF_WORKER").is_ok_and(|v| v == "1") {
        eprintln!("{}", text::ONLY_A_WORKER_NEXT);
        std::process::exit(1);
    }
    let pane = std::env::var("TMUX_PANE").ok().filter(|p| !p.is_empty());
    let Some(pane) = pane else {
        anyhow::bail!("riff workers next needs the tmux pane of the worker (TMUX_PANE)");
    };
    let id = identity::session_id()
        .ok_or_else(|| anyhow::anyhow!("riff workers next needs the session ID of the worker"))?;
    let here = identity::place(&std::env::current_dir()?)?;
    let api = Api::new(server);
    let me = identity::agent(&here, &id, api.base())?;
    let signed = api.signed_in(Some(&id))?;
    let sessions = signed.who(&me, false).await?;
    let Some(info) = sessions.iter().find(|s| s.uri.who() == me.who()) else {
        anyhow::bail!("this worker is not in riff who");
    };
    if info.uri.lead() {
        eprintln!("{}", text::THE_LEAD_KEEPS_ITS_CONTEXT);
        std::process::exit(1);
    }
    if !info.uri.claims().is_empty() {
        eprintln!("{}", text::next_holds_claims(info.uri.claims()));
        std::process::exit(1);
    }
    let fresh = hygiene::fast_forward(&std::env::current_dir()?);
    if let Some(line) = fresh.line() {
        println!("{line}");
        if fresh.tells_the_lead()
            && let Err(e) = signed.tell(&me, riff::api::LEAD, &line).await
        {
            eprintln!("riff: cannot tell the lead: {e:#}");
        }
    }
    let dir = local::dir().ok_or_else(|| anyhow::anyhow!("no local directory for riff"))?;
    next::mark(&dir, &id, &pane)?;
    println!("{}", text::NEXT_ASKED);
    Ok(())
}

/// The Stop hook (01M3JQCCZ5M9VY3RGXWJYJN9Q9). It never fails.
fn stop_hook() {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: next::StopInput = serde_json::from_str(&stdin).unwrap_or_default();
    let Some(id) = identity::agent_session(input.session_id) else {
        return;
    };
    let Some(pane) = local::dir().and_then(|dir| next::take(&dir, &id)) else {
        return;
    };
    if let Err(e) = next::spawn(&next::ClaudeCode, &pane) {
        eprintln!("riff: cannot give the worker a fresh context: {e:#}");
    }
}

/// Lists the workers of this machine, with their claims and status in
/// `riff who` (01M3JPQTBDGT54WN7FZP9CD6B5).
async fn list_workers(server: &str) -> Result<()> {
    let panes = Tmux::machine().worker_panes()?;
    let who = async {
        let here = identity::place(&std::env::current_dir()?)?;
        let api = Api::new(server);
        let me = identity::me(&here, api.base())?;
        api.signed_in(me.who().session())?.who(&me, false).await
    };
    // With no worker here, a riff that does not answer only hides the
    // hosts.
    let sessions = who.await.unwrap_or_else(|e| {
        if !panes.is_empty() {
            eprintln!("riff: cannot read riff who: {e:#}");
        }
        Vec::new()
    });
    print!("{}", text::workers(&panes, &sessions));
    let me =
        identity::place(&std::env::current_dir()?).and_then(|here| identity::me(&here, server));
    let Ok(me) = me else {
        return Ok(());
    };
    for (info, status) in riff::host::hosts(&sessions, me.who().user()) {
        let host = info.uri.place().host();
        if host == me.place().host() {
            continue;
        }
        println!("{}", text::host_heading(host, &status));
        print!(
            "{}",
            text::workers(&riff::host::panes(&status, &sessions), &sessions)
        );
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
    let here = identity::place(&std::env::current_dir()?)?;
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
/// (01M3JD390F49HZSKEJ3VACX0ZA). It first waits until the server lists
/// the session. An error goes to stderr: the tools still work.
async fn tail_beside_lead(api: &Api, me: &SessionUri) {
    let Some(tmux) = Tmux::from_env() else {
        return;
    };
    let added = async {
        let program = Program::tail(
            &std::env::current_exe()?,
            &std::env::current_dir()?,
            api.base(),
        );
        for _ in 0..REGISTER_TRIES {
            if api
                .who(me, false)
                .await?
                .iter()
                .any(|s| s.uri.who() == me.who())
            {
                break;
            }
            tokio::time::sleep(REGISTER_WAIT).await;
        }
        terminal::tail_beside_lead(api, me, &tmux, &program).await
    };
    if let Err(e) = added.await {
        eprintln!("riff: cannot add the riff tail pane: {e:#}");
    }
}

/// How often and how long `riff mcp` waits for its session to register.
const REGISTER_TRIES: u32 = 10;
const REGISTER_WAIT: Duration = Duration::from_millis(200);

/// Pauses or resumes the riff, and wakes each session
/// (01M3JCG3T8AJZN31SZQQTP3FAF, 01M3JCG3YD7C2Y3V0QJPF082YH).
async fn pause(api: &Api, me: &SessionUri, state: RiffState) -> Result<()> {
    let (reply, posted) = api.set_riff(me, state).await?;
    println!("{}", text::riff_set(&reply, &posted));
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
    let api = Api::new(server);
    let cwd = std::env::current_dir().ok();
    let uri = id.as_deref().zip(cwd.as_deref()).and_then(|(id, cwd)| {
        let here = identity::place(cwd).ok()?;
        identity::agent(&here, id, api.base()).ok()
    });
    let facts = async {
        match uri {
            Some(uri) => {
                let facts = start_facts(api, &uri, input.source.is_new_start());
                match tokio::time::timeout(hook::STATE_WAIT, facts).await {
                    Ok(Ok((lead, riff, freed, others))) => (
                        Some(uri.with_lead(lead)),
                        Some(riff),
                        freed,
                        Some(others),
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
    let ((uri, riff, freed, others, mismatch), behind, linked) =
        tokio::join!(facts, behind, linked);
    let watching = id
        .as_deref()
        .zip(local::dir())
        .is_some_and(|(id, dir)| local::watching(&dir, id));
    let mismatch_free = mismatch.is_none();
    let mut context = match mismatch {
        Some(mismatch) => hook::mismatch_context(uri.as_ref(), &mismatch),
        None => hook::start_context(uri.as_ref(), input.source, watching, riff, &freed),
    };
    if let Some(behind) = behind {
        context.push_str(&behind.line());
    }
    if let Some(linked) = linked.filter(|_| mismatch_free) {
        context.push_str(&linked.line(others.as_deref()));
    }
    if worker::is_worker() {
        context.push_str(hook::WORKER_LINE);
    }
    hook::start_output(&context)
}

/// Whether the server names `me` as the lead, the state of the riff
/// (01M3JCG48QPCNNTKW34FTR0AMR), the claims that a new start freed
/// (01M3JEE1QQCFS5TMZW5N2DAD2D), and the other live sessions in the
/// place of `me` ([`hook::others_here`]).
async fn start_facts(
    api: Api,
    me: &SessionUri,
    new_start: bool,
) -> Result<(bool, RiffState, Vec<Freed>, Vec<SessionUri>)> {
    let api = api.signed_in(me.who().session())?;
    let freed = if new_start {
        api.start(me).await?
    } else {
        Vec::new()
    };
    let riff = api.riff(me).await?;
    let who = api.who(me, false).await?;
    let lead = who.iter().any(|s| s.uri.who() == me.who() && s.uri.lead());
    Ok((lead, riff, freed, hook::others_here(me, &who)))
}

/// The status line of the Claude Code session on stdin
/// ([`text::statusline`]). It finds the session like a hook does, and
/// looks for it in `riff who`. It never fails, and it waits at most
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
    let find = async {
        let here = identity::place(&std::env::current_dir()?)?;
        let api = Api::new(server);
        let me = identity::agent(&here, &id, api.base())?;
        let who = api.signed_in(me.who().session())?.who(&me, false).await?;
        anyhow::Ok(
            who.into_iter()
                .find(|s| s.uri.who().session() == Some(id.as_str())),
        )
    };
    let info = tokio::time::timeout(STATUSLINE_WAIT, find)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
    text::statusline(&id, info.as_ref())
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
    let ended = async {
        let here = identity::place(&std::env::current_dir()?)?;
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

/// When `riff tail` and `riff who` use color
/// (01M3JDCA9070MY30AYHK3Y67EF, 01M3MEW75WC7Y4M1BKQ7SXRPNR).
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
async fn tail(api: &Api, thread: &ThreadName, color: ColorWhen) {
    use_color(color);
    tokio::select! {
        () = tail_each(api, thread) => {}
        () = binary::follow_update() => {}
    }
}

/// Prints each message of `thread`. It reports an error once, in red
/// for a version that it cannot talk to, and tries again until the
/// stream comes back (01M3MNVTC248YYJJQKFD9H1WY9).
async fn tail_each(api: &Api, thread: &ThreadName) {
    let (warning, error) = (riff::style::WARNING, riff::style::ERROR);
    anstream::eprintln!("riff: showing new messages in {thread}. Ctrl-C stops.");
    let mut stream = Box::pin(follow(|| api.tail(thread), RETRY));
    let mut lost = false;
    let mut last_day = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(checked) => {
                if lost {
                    anstream::eprintln!("riff: connected again.");
                    lost = false;
                }
                let at = i64::try_from(checked.message.at_ms)
                    .ok()
                    .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
                    .unwrap_or_else(chrono::Local::now);
                let block = text::block(&checked, &at, last_day, textwrap::termwidth());
                last_day = Some(at.date_naive());
                anstream::println!("{block}");
            }
            Err(e) if !lost => {
                let style = if e.downcast_ref::<Mismatch>().is_some() {
                    error
                } else {
                    warning
                };
                anstream::eprintln!(
                    "{style}riff: {e:#}. Trying again every {} seconds.{style:#}",
                    RETRY.as_secs()
                );
                lost = true;
            }
            Err(_) => {}
        }
    }
    anstream::eprintln!("{error}riff: the stream of {thread} ended.{error:#}");
}

/// Runs until stopped, or with `once` until the first wake (R170). It
/// connects again when the stream ends (R131). It stops when the session
/// leaves the riff (01M3MEEFETT9A0DRWBKQTG77Z2). On a new binary, it runs
/// it (01M3MNVTC248YYJJQKFD9H1WY9).
async fn watch(api: &Api, me: &riff_core::name::SessionUri, once: bool) {
    let stream = follow(|| api.watch(me), RETRY);
    let left = async {
        let Some(id) = me.who().session() else {
            return std::future::pending().await;
        };
        while !local::left_here(id) {
            tokio::time::sleep(LEFT_POLL).await;
        }
    };
    tokio::select! {
        () = print_each(stream, text::wake_line, once) => {}
        () = left => println!("{}", text::WATCH_LEFT),
        () = binary::follow_update() => {}
    }
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
}
