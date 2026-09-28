//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use chrono::TimeZone;
use clap::{Parser, Subcommand};
use futures::{Stream, StreamExt};
use riff::api::{Api, DEFAULT_SERVER, follow};
use riff::terminal::{Program, Terminal, Tmux};
use riff::{hook, identity, local, login, mcp, plugin, settings, terminal, text};
use riff_core::build::Mismatch;
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Freed, Kind, RiffState, Status};

/// The time between two tries to connect a stream.
const RETRY: Duration = Duration::from_secs(5);

/// The longest time that `riff statusline` waits for riff-server.
const STATUSLINE_WAIT: Duration = Duration::from_secs(2);

/// The local client that finds sessions and wakes yours.
#[derive(Parser)]
#[command(version = riff_core::build::VERSION, about)]
struct Cli {
    /// The riff-server URL.
    #[arg(long, global = true, env = "RIFF_SERVER", default_value = DEFAULT_SERVER)]
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
    /// can.
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
    /// Print the status line of a Claude Code session: its short session
    /// ID, its claims, and `lead` or `blocked`. Claude Code runs it with
    /// the session on stdin. It always exits with status 0.
    Statusline,
    /// Run a Claude Code hook. The riff plugin calls it.
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Install the riff plugin in an agent tool. Run it again to update
    /// the plugin.
    Connect {
        #[command(subcommand)]
        tool: Tool,
    },
    /// Start, list and stop the worker sessions of this machine. They
    /// need tmux. With no subcommand, it lists each worker: its pane,
    /// its session ID, its claims and its status
    Workers {
        #[command(subcommand)]
        command: Option<Workers>,
    },
}

#[derive(Subcommand)]
enum Workers {
    /// Start COUNT worker sessions in the tmux window riff-workers, one
    /// pane each. Each pane runs `claude "Join the riff."` in the main
    /// worktree. It starts at most the limit minus the workers that run.
    /// It refuses in a worker, and in an agent session that is not the
    /// lead. Outside tmux, it starts nothing.
    Start {
        /// The number of workers.
        #[arg(value_parser = clap::value_parser!(u16).range(1..))]
        count: u16,
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
    /// End each worker of this machine, or only the worker in PANE. Each
    /// worker leaves `riff who` and frees its claims at once
    Stop {
        /// The tmux pane of one worker, for example %3. `riff workers`
        /// shows it.
        pane: Option<String>,
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
    let cli = Cli::parse();
    if let Command::Hook {
        event: HookEvent::SessionStart,
    } = cli.command
    {
        println!("{}", session_start(&cli.server).await);
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionEnd,
    } = cli.command
    {
        session_end(&cli.server).await;
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
        return Ok(());
    }
    if let Command::Workers { command } = &cli.command {
        return workers(command.as_ref(), &cli.server).await;
    }
    let api = Api::new(&cli.server);
    match &cli.command {
        Command::Login => {
            let sign_in = login::login(&api, |url| {
                eprintln!("riff: sign in with your browser. If it does not open, go to:\n{url}");
                let _ = open::that_detached(url);
            })
            .await?;
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
            let done = api.signed_in(None)?.invite(email).await?;
            println!("{}", text::invited(&done));
            return Ok(());
        }
        Command::Remove { email } => {
            let done = api.signed_in(None)?.remove(email).await?;
            println!("{}", text::removed(&done));
            return Ok(());
        }
        Command::Members => {
            println!("{}", text::members(&api.signed_in(None)?.members().await?));
            return Ok(());
        }
        _ => {}
    }
    let here = identity::place(&std::env::current_dir()?)?;
    let me = identity::me(&here, api.base())?;
    let api = api.signed_in(me.who().session())?;
    match cli.command {
        Command::Whoami => {
            println!("{}  {me}", text::name(&me));
            match api.riff(&me).await {
                Ok(state) => println!("{}\n{}", text::riff_state(state), text::build_line()),
                Err(e) => eprintln!("riff: cannot read the state of the riff: {e:#}"),
            }
        }
        Command::Who { all } => {
            println!("{}", text::riff_state(api.riff(&me).await?));
            println!("{}", text::build_line());
            print!("{}", text::who(&api.who(&me, all).await?, &me));
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
            let tail = tail_beside_lead(&api, &me);
            let (_, served) = tokio::join!(tail, mcp::serve(api.clone(), me.clone()));
            served?
        }
        Command::Hook { .. }
        | Command::Statusline
        | Command::Connect { .. }
        | Command::Workers { .. }
        | Command::Login
        | Command::Logout { .. }
        | Command::Invite { .. }
        | Command::Remove { .. }
        | Command::Members => unreachable!("handled before the identity"),
    }
    Ok(())
}

/// `riff workers` and its subcommands.
async fn workers(command: Option<&Workers>, server: &str) -> Result<()> {
    match command {
        None => list_workers(server).await,
        Some(Workers::Start { count, claude }) => start_workers(*count, claude, server).await,
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
        Some(Workers::Stop { pane }) => stop_workers(pane.as_deref(), server).await,
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
    let limit = settings::workers_limit(&settings::path()?)?;
    if limit == 0 {
        eprintln!("{}", text::NO_WORKER_LIMIT);
        std::process::exit(1);
    }
    let Some(tmux) = Tmux::from_env() else {
        eprintln!("{}", text::NO_TMUX);
        std::process::exit(1);
    };
    let run = tmux.worker_panes()?.len();
    let start = terminal::room(count, limit, run);
    if start == 0 {
        eprintln!("{}", text::workers_full(limit, run));
        std::process::exit(1);
    }
    let dir = std::env::current_dir()?;
    let main = identity::main_worktree(&dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    let base = Api::new(server).base().to_owned();
    let programs: Vec<Program> = (0..start)
        .map(|_| Program::worker(claude, &main, &base, &terminal::new_session_id()))
        .collect();
    let window = tmux.workers(&programs)?;
    println!("{}", text::workers_started(start, &window, &main));
    if start < count {
        println!("{}", text::workers_limited(count - start, limit, run));
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
    let sessions = if panes.is_empty() {
        Vec::new()
    } else {
        who.await.unwrap_or_else(|e| {
            eprintln!("riff: cannot read riff who: {e:#}");
            Vec::new()
        })
    };
    print!("{}", text::workers(&panes, &sessions));
    Ok(())
}

/// Ends each worker of this machine, or the one in `pane`: it kills the
/// pane, then sends the end call of the session
/// (01M3JPQTDFW3C7QBSZZ2M831MH).
async fn stop_workers(pane: Option<&str>, server: &str) -> Result<()> {
    let tmux = Tmux::machine();
    let mut panes = tmux.worker_panes()?;
    if let Some(pane) = pane {
        panes.retain(|w| w.pane == pane);
        if panes.is_empty() {
            anyhow::bail!("no worker runs in the pane {pane}. `riff workers` lists them");
        }
    }
    let here = identity::place(&std::env::current_dir()?)?;
    let api = Api::new(server);
    for worker in &panes {
        tmux.kill(&worker.pane)?;
        let ended = async {
            let me = identity::agent(&here, &worker.session, api.base())?;
            api.clone().signed_in(Some(&worker.session))?.end(&me).await
        };
        if let Err(e) = ended.await {
            eprintln!(
                "riff: stopped the pane {}, but the end call of its session failed: {e:#}",
                worker.pane
            );
        }
    }
    println!("{}", text::workers_stopped(panes.len()));
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
/// line when the clone is behind `origin` ([`hook::behind`]).
async fn session_start(server: &str) -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: hook::StartInput = serde_json::from_str(&stdin).unwrap_or_default();
    let id = identity::agent_session(input.session_id);
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
                    Ok(Ok((lead, riff, freed))) => {
                        (Some(uri.with_lead(lead)), Some(riff), freed, None)
                    }
                    Ok(Err(e)) => (Some(uri), None, Vec::new(), e.downcast::<Mismatch>().ok()),
                    Err(_) => (Some(uri), None, Vec::new(), None),
                }
            }
            None => (None, None, Vec::new(), None),
        }
    };
    let behind = async {
        match &cwd {
            Some(cwd) => hook::behind(cwd, hook::FETCH_WAIT).await,
            None => None,
        }
    };
    let ((uri, riff, freed, mismatch), behind) = tokio::join!(facts, behind);
    let watching = id
        .as_deref()
        .zip(local::dir())
        .is_some_and(|(id, dir)| local::watching(&dir, id));
    let mut context = match mismatch {
        Some(mismatch) => hook::mismatch_context(uri.as_ref(), &mismatch),
        None => hook::start_context(uri.as_ref(), input.source, watching, riff, &freed),
    };
    if let Some(behind) = behind {
        context.push_str(&behind.line());
    }
    hook::start_output(&context)
}

/// Whether the server names `me` as the lead, the state of the riff
/// (01M3JCG48QPCNNTKW34FTR0AMR), and the claims that a new start freed
/// (01M3JEE1QQCFS5TMZW5N2DAD2D).
async fn start_facts(
    api: Api,
    me: &SessionUri,
    new_start: bool,
) -> Result<(bool, RiffState, Vec<Freed>)> {
    let api = api.signed_in(me.who().session())?;
    let freed = if new_start {
        api.start(me).await?
    } else {
        Vec::new()
    };
    let riff = api.riff(me).await?;
    let lead = api
        .who(me, false)
        .await?
        .iter()
        .any(|s| s.uri.who() == me.who() && s.uri.lead());
    Ok((lead, riff, freed))
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

/// When `riff tail` uses color (01M3JDCA9070MY30AYHK3Y67EF).
#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

/// Runs until stopped. It connects again when the stream ends (R131).
/// Each message is a [`text::block`]. The status lines go to stderr
/// (01M3JDCA6R894JG6SDJ2R7AFMN).
async fn tail(api: &Api, thread: &ThreadName, color: ColorWhen) {
    anstream::ColorChoice::write_global(match color {
        ColorWhen::Auto => anstream::ColorChoice::Auto,
        ColorWhen::Always => anstream::ColorChoice::Always,
        ColorWhen::Never => anstream::ColorChoice::Never,
    });
    let (warning, error) = (text::WARNING, text::ERROR);
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
            Err(e) if e.downcast_ref::<Mismatch>().is_some() => {
                anstream::eprintln!("{error}riff: {e}{error:#}");
                std::process::exit(1);
            }
            Err(e) if !lost => {
                anstream::eprintln!(
                    "{warning}riff: {e:#}. Trying again every {} seconds.{warning:#}",
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
/// connects again when the stream ends (R131).
async fn watch(api: &Api, me: &riff_core::name::SessionUri, once: bool) {
    let stream = follow(|| api.watch(me), RETRY);
    print_each(stream, text::wake_line, once).await;
}

/// Prints one line for each item, or with `once` only the first line.
/// It reports a failed connect on stderr once, until the next item
/// comes.
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
            // A mismatch stays until a person updates riff: stop, so that
            // the line wakes the session (01M3JEE7TPZMNK7X6JXJ7GWFPP).
            Err(e) if e.downcast_ref::<Mismatch>().is_some() => {
                println!("riff: {e}");
                std::process::exit(1);
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
