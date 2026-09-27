//! The local client that finds sessions and wakes yours.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::{Stream, StreamExt};
use riff::api::{Api, DEFAULT_SERVER, follow};
use riff::{hook, identity, local, login, mcp, plugin, text};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;

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
    /// List the sessions in the riff. A session that made no call for 24
    /// hours is gone and not listed.
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
        /// are user, session, host, repo, worktree and claim. Give --to
        /// again to wake more sessions.
        #[arg(long)]
        to: Vec<Selector>,
        /// The message.
        #[arg(required = true)]
        body: Vec<String>,
    },
    /// Send a direct message to one session. It wakes that session.
    Tell {
        /// The session: its session ID, or its full riff:// URI from
        /// `riff who`.
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
    /// Show each new message in a thread.
    Tail {
        /// The thread. The default is your repository thread.
        thread: Option<String>,
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
    /// Print one line each time a post wakes this session. One watch
    /// runs for each session: a second one stops at once.
    Watch,
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
}

#[derive(Subcommand)]
enum HookEvent {
    /// Read the SessionStart input on stdin. Print the context that
    /// starts the watch. It always exits with status 0.
    SessionStart,
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
    if let Command::Connect {
        tool: Tool::Claude { claude },
    } = &cli.command
    {
        let connected = plugin::connect(claude, &plugin::dir()?)?;
        println!("{}", text::connected(&connected));
        return Ok(());
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
        Command::Post { thread, to, body } => {
            let thread = thread_or_default(thread, &here)?;
            let posted = api.post(&me, Some(&thread), &to, &body.join(" ")).await?;
            println!("{}", text::posted(&posted));
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
        Command::Tail { thread } => tail(&api, &thread_or_default(thread, &here)?).await,
        Command::Watch => {
            let me = identity::session(&here, api.base())?;
            let Some(_lock) = lock_watch(&me) else {
                println!("{}", text::WATCH_RUNS);
                std::process::exit(1);
            };
            watch(&api, &me).await
        }
        Command::Mcp => {
            let me = identity::session(&here, api.base())?;
            let _record = record_session(&me);
            mcp::serve(api, me).await?
        }
        Command::Hook { .. }
        | Command::Connect { .. }
        | Command::Login
        | Command::Logout { .. } => unreachable!("handled before the identity"),
    }
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

/// Runs until stopped. It connects again when the stream ends (R131).
async fn tail(api: &Api, thread: &ThreadName) {
    eprintln!("riff: showing new messages in {thread}. Ctrl-C stops.");
    let stream = follow(|| api.tail(thread), RETRY);
    print_each(stream, |tailed| text::message(&tailed.message)).await;
}

/// Runs until stopped. It connects again when the stream ends (R131).
async fn watch(api: &Api, me: &riff_core::name::SessionUri) {
    let stream = follow(|| api.watch(me), RETRY);
    print_each(stream, text::wake_line).await;
}

/// Prints one line for each item. It reports a failed connect on
/// stderr once, until the next item comes.
async fn print_each<T>(stream: impl Stream<Item = Result<T>>, line: impl Fn(&T) -> String) {
    let mut stream = Box::pin(stream);
    let mut reported = false;
    while let Some(item) = stream.next().await {
        match item {
            Ok(item) => {
                reported = false;
                println!("{}", line(&item));
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
