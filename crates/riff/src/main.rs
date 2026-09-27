//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use chrono::TimeZone;
use clap::{Parser, Subcommand};
use futures::{Stream, StreamExt};
use riff::api::{Api, DEFAULT_SERVER, follow};
use riff::terminal::{Program, Terminal, Tmux};
use riff::{hook, identity, local, login, mcp, plugin, terminal, text};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, Status};

/// The time between two tries to connect a stream.
const RETRY: Duration = Duration::from_secs(5);

/// The local client that finds sessions and wakes yours.
#[derive(Parser)]
#[command(version, about)]
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
    /// Show your URI. Inside Claude Code, it is the URI of the session.
    Whoami,
    /// List the sessions in the riff. A session that ended, or stopped
    /// for 3 minutes, is gone and not listed.
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
        /// The kind of post: message, or status. A status request asks
        /// each session that it wakes to set its status. `riff who` then
        /// shows each status.
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
        /// The session: its session ID, its full riff:// URI from `riff
        /// who`, or `lead` for the lead of your user in this repository.
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
    /// Start the worker sessions of this machine. They need tmux.
    Workers {
        #[command(subcommand)]
        command: Workers,
    },
}

#[derive(Subcommand)]
enum Workers {
    /// Start COUNT worker sessions in the tmux window riff-workers, one
    /// pane each. Each pane runs `claude "Join the riff."` in the main
    /// worktree. Outside tmux, it starts nothing.
    Start {
        /// The number of workers.
        #[arg(value_parser = clap::value_parser!(u16).range(1..))]
        count: u16,
        /// The claude command.
        #[arg(long, default_value = "claude")]
        claude: std::path::PathBuf,
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
        println!("{}", session_start());
        return Ok(());
    }
    if let Command::Hook {
        event: HookEvent::SessionEnd,
    } = cli.command
    {
        session_end(&cli.server).await;
        return Ok(());
    }
    if let Command::Connect {
        tool: Tool::Claude { claude },
    } = &cli.command
    {
        let connected = plugin::connect(claude, &plugin::dir()?)?;
        println!("{}", text::connected(&connected));
        return Ok(());
    }
    if let Command::Workers {
        command: Workers::Start { count, claude },
    } = &cli.command
    {
        return start_workers(*count, claude, &cli.server);
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
        _ => {}
    }
    let here = identity::place(&std::env::current_dir()?)?;
    let me = identity::me(&here, api.base())?;
    let api = api.signed_in(me.who().session())?;
    match cli.command {
        Command::Whoami => println!("{}  {me}", text::name(&me)),
        Command::Who { all } => print!("{}", text::who(&api.who(&me, all).await?, &me)),
        Command::Post {
            thread,
            to,
            kind,
            body,
        } => {
            if kind.is_message() && body.is_empty() {
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
        | Command::Connect { .. }
        | Command::Workers { .. }
        | Command::Login
        | Command::Logout { .. } => unreachable!("handled before the identity"),
    }
    Ok(())
}

/// Starts `count` workers in tmux (01M3JD392Q5ANX0FPZ51W7B0E3). Outside
/// tmux, it starts nothing and fails (01M3JD3973J7A9BG8G9EP9TVDP).
fn start_workers(count: u16, claude: &std::path::Path, server: &str) -> Result<()> {
    let Some(tmux) = Tmux::from_env() else {
        eprintln!("{}", text::NO_TMUX);
        std::process::exit(1);
    };
    let dir = std::env::current_dir()?;
    let main = identity::main_worktree(&dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    let worker = Program::worker(claude, &main, Api::new(server).base());
    let window = tmux.workers(&vec![worker; usize::from(count)])?;
    println!("{}", text::workers_started(count, &window, &main));
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

fn thread_or_default(given: Option<String>, here: &Place) -> Result<ThreadName> {
    match given {
        Some(t) => Ok(t.parse()?),
        None => here
            .default_thread()
            .ok_or_else(|| anyhow::anyhow!("name a thread: this directory is not in git")),
    }
}

/// The SessionStart hook output. It has no URI when riff cannot find the
/// session, but it always has the context (R69).
fn session_start() -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: hook::StartInput = serde_json::from_str(&stdin).unwrap_or_default();
    let id = identity::agent_session(input.session_id);
    let uri = id.as_deref().and_then(|id| {
        let here = identity::place(&std::env::current_dir().ok()?).ok()?;
        let server = std::env::var("RIFF_SERVER").unwrap_or_else(|_| DEFAULT_SERVER.into());
        identity::agent(&here, id, Api::new(&server).base()).ok()
    });
    let watching = id
        .as_deref()
        .zip(local::dir())
        .is_some_and(|(id, dir)| local::watching(&dir, id));
    hook::start_output(&hook::start_context(uri.as_ref(), input.source, watching))
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
