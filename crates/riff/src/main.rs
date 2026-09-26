//! The local client that finds sessions and wakes yours.

use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use riff::api::{Api, DEFAULT_SERVER};
use riff::{identity, mcp, text};
use riff_core::name::{SessionName, ThreadName};

/// The time between two tries to reach the server.
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
    /// Show the session name for this directory.
    Whoami,
    /// List the sessions in the riff.
    Who,
    /// Post a message to a thread.
    Post {
        /// The thread. The default is your repository thread.
        #[arg(long, short)]
        thread: Option<String>,
        /// The message.
        #[arg(required = true)]
        body: Vec<String>,
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
    /// Print one line each time a direct message or a mention arrives.
    Watch,
    /// Serve the riff tools to an agent session over stdio.
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = Api::new(&cli.server);
    let me = identity::session_name(&std::env::current_dir()?)?;
    match cli.command {
        Command::Whoami => println!("{}  {me}", me.short()),
        Command::Who => print!("{}", text::who(&api.who().await?, &me)),
        Command::Post { thread, body } => {
            let thread = thread_or_default(thread, &me)?;
            let posted = api.post(&me, &thread, &body.join(" ")).await?;
            println!("{}", text::posted(&posted));
        }
        Command::Claim { thread, item } => {
            let thread = thread_or_default(thread, &me)?;
            let reply = api.claim(&me, &thread, &item).await?;
            println!("{}", text::claimed(&reply, &thread, &item));
            if !reply.granted {
                std::process::exit(1);
            }
        }
        Command::Release { thread, item } => {
            let thread = thread_or_default(thread, &me)?;
            api.release(&me, &thread, &item).await?;
            println!("{}", text::released(&thread, &item));
        }
        Command::Tail { thread } => tail(&api, &thread_or_default(thread, &me)?).await?,
        Command::Watch => watch(&api, &me).await,
        Command::Mcp => mcp::serve(api, me).await?,
    }
    Ok(())
}

fn thread_or_default(given: Option<String>, me: &SessionName) -> Result<ThreadName> {
    match given {
        Some(t) => Ok(t.parse()?),
        None => me
            .default_thread()
            .ok_or_else(|| anyhow::anyhow!("name a thread: this directory is not in git")),
    }
}

async fn tail(api: &Api, thread: &ThreadName) -> Result<()> {
    let mut stream = Box::pin(api.tail(thread).await?);
    eprintln!("riff: showing new messages in {thread}. Ctrl-C stops.");
    while let Some(tailed) = stream.next().await {
        println!("{}", text::message(&tailed?.message));
    }
    Ok(())
}

/// Runs until stopped. It connects again after the server goes away.
async fn watch(api: &Api, me: &SessionName) {
    let mut reported = false;
    loop {
        match api.watch(me).await {
            Ok(stream) => {
                reported = false;
                let mut stream = Box::pin(stream);
                while let Some(Ok(wake)) = stream.next().await {
                    println!("{}", text::wake_line(&wake));
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
        tokio::time::sleep(RETRY).await;
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
