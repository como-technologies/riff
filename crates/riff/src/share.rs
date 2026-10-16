//! Share a riff of peers at the CLI (#843): `riff invite USER`,
//! `riff join STRING`, `riff leave`, `riff remove USER [--host HOST]` and
//! `riff members`. Each command asks the riff daemon of this machine
//! ([`crate::daemon::peers`] does the steps). The book page "riff 3.0:
//! peers" has the how-to of each one.
//!
//! # Design
//!
//! A person runs `riff invite`, `riff remove`, `riff join` and
//! `riff leave` in a terminal: the standard input and the standard output
//! are terminals (01M4Z60B8JEX15M1Q2435FZSDM, 01M521J6QBQFRDWRPR0NGHE7TM).
//! An agent session runs a command with no terminal, so it cannot admit
//! a member, remove one, or move the daemon of the machine to another
//! riff. `riff members` only reads, and needs no terminal.
//!
//! Until Wave C, the member commands of riff-server 1.x take `--email`
//! (01M519Z4AEMDFGBF22RZ3HQR1R): `riff invite --email EMAIL`,
//! `riff remove --email EMAIL` and `riff members --email`.
//!
//! | Command | Terminal | Ask of the daemon |
//! |---|---|---|
//! | `riff invite USER` | yes | [`crate::daemon::Ask::Invite`] |
//! | `riff join STRING` | yes | [`crate::daemon::Ask::Join`] |
//! | `riff leave` | yes | [`crate::daemon::Ask::Leave`] |
//! | `riff remove USER [--host HOST]` | yes | [`crate::daemon::Ask::Remove`] |
//! | `riff members` | no | [`crate::daemon::Ask::Members`] |

use std::io::IsTerminal;

use anyhow::{Result, bail};

use crate::daemon::{self, Ask};
use crate::peer::invite::{Invite, Text};
use crate::text;

/// Refuses the command `command` with no terminal.
fn in_terminal(command: &str) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!("{}", text::share_no_terminal(command));
    }
    Ok(())
}

/// `riff invite USER`: prints the invite string of one new key of `user`,
/// and that it is a secret.
pub async fn invite(user: &str) -> Result<()> {
    in_terminal("riff invite")?;
    let (invite, ends_ms) = daemon::invite(&daemon::dir()?, user).await?;
    let user = Invite::parse(&invite.0).map_or_else(|_| user.to_owned(), |read| read.user);
    println!("{}", text::invited_to_peers(&user, &invite.0, ends_ms));
    Ok(())
}

/// `riff join STRING`: this machine joins the riff of the invite string
/// `invite`.
pub async fn join(invite: &str) -> Result<()> {
    in_terminal("riff join")?;
    // A string that is not an invite fails here, with no daemon step.
    Invite::parse(invite)?;
    let ask = Ask::Join {
        invite: Text(invite.trim().to_owned()),
    };
    daemon::ask_done(&daemon::dir()?, ask, |line| println!("{line}")).await
}

/// `riff leave`: this machine leaves the riff of another person, and has a
/// riff of its own again.
pub async fn leave() -> Result<()> {
    in_terminal("riff leave")?;
    daemon::ask_done(&daemon::dir()?, Ask::Leave, |line| println!("{line}")).await
}

/// `riff remove USER [--host HOST]`: the owner removes each key of `user`,
/// or its key on `host`.
pub async fn remove(user: &str, host: Option<&str>) -> Result<()> {
    in_terminal("riff remove")?;
    let ask = Ask::Remove {
        user: user.to_owned(),
        host: host.map(str::to_owned),
    };
    daemon::ask_done(&daemon::dir()?, ask, |line| println!("{line}")).await
}

/// `riff members`: the owner and each member key of the riff of the
/// daemon of this machine.
pub async fn members() -> Result<()> {
    let rows = daemon::members(&daemon::dir()?).await?;
    let now = chrono::Utc::now().timestamp_millis();
    print!("{}", text::members_of_peers(&rows, now));
    Ok(())
}
