//! The member steps of the daemon in a riff of peers (#843): `riff
//! invite`, `riff join`, `riff leave`, `riff remove` and `riff members`.
//! [`crate::share`] is the side of the CLI.
//!
//! # The riff that the daemon is in
//!
//! A daemon is in one riff at a time (01M519Z40SHPJ1BB2SZN9AW43F). The
//! file [`JOINED`] in its folder `state` holds the riff ID of the riff of
//! another owner that it joined. With no such file, the daemon is in its
//! riff of its own (01M521J6WZT2KR5ZQE9DZ9A5AC). The records of its own
//! riff are in `state/records`, and the records of a joined riff are in
//! `state/riffs/RIFF/records` ([`peers_dir`]). A start of the daemon opens
//! the riff of [`JOINED`]. A leave goes back to the riff of its own, and
//! the records of the old riff stay in their folder.
//!
//! | Step | Who | What the daemon does |
//! |---|---|---|
//! | `riff invite USER` | the owner | an invite in [`crate::peer::invite::FILE`]: the string goes to the CLI |
//! | `riff join STRING` | a daemon with no other member | the join of [`crate::peer::join`], then the records of the new riff, and the link to the owner |
//! | `riff leave` | a member | the `left` record to each link, then the riff of its own |
//! | `riff remove USER` | the owner | a `removed` record for each key of USER ([`crate::peer::members::removals`]) |
//! | `riff members` | each daemon | the owner and each key with its state ([`crate::peer::members::shown`]) |
//!
//! At a join and at a leave, each session of the daemon on the riff of
//! peers leaves the old riff with an end record, and starts in the new
//! riff (01M521J6ZRNWBSPK7WB23TVY59). At a leave, the end records come
//! before the `left` record: the `left` record is the last record of the
//! key.
//!
//! ```mermaid
//! flowchart TD
//!     J["riff join riff1-..."] --> C{"one riff at a time:<br/>the riff of its own, with no other member"}
//!     C -->|no| R["refused: nothing changes"]
//!     C -->|yes| A["join::ask: the daemon of the owner writes the member record"]
//!     A --> S["Hub::switch to state/riffs/RIFF; the file joined"]
//!     S --> L["the join connection goes on as the peer link: each record of the riff"]
//!     V["riff leave"] --> E["the end record of each session, then the left record"]
//!     E --> W["each link sends the left record, and ends"]
//!     W --> O["Hub::switch to state: the riff of its own; no file joined"]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};

use super::{Daemon, now_ms, state_dir};
use crate::daemon_key::Public;
use crate::peer::body::{Body, Left, Riff as RiffBody};
use crate::peer::invite::Invite;
use crate::peer::members::{self, Shown};
use crate::peer::sync::LEAVE_LIMIT;
use crate::peer::talk::Hub;
use crate::runtime::Runtime;
use crate::text;

/// The file in the folder `state` that holds the riff ID of the riff
/// that the daemon joined.
pub const JOINED: &str = "joined";

/// The folder in the folder `state` that holds a folder of records for
/// each riff that the daemon joined.
pub const RIFFS: &str = "riffs";

/// The folder of the records of the riff `riff` for the daemon of the key
/// `me`, in the folder `state_dir`: `state_dir` for the riff of its own,
/// else `state_dir/riffs/RIFF`.
///
/// ```
/// use std::path::Path;
///
/// use riff::daemon::peers::peers_dir;
/// use riff::daemon_key::Key;
///
/// let (me, other) = (Key::from_bytes(&[1; 32]).public(), Key::from_bytes(&[2; 32]).public());
/// let state = Path::new("/d/state");
/// assert_eq!(peers_dir(state, &me, &me), state);
/// assert_eq!(peers_dir(state, &other, &me), state.join("riffs").join(other.to_string()));
/// ```
pub fn peers_dir(state_dir: &Path, riff: &Public, me: &Public) -> PathBuf {
    if riff == me {
        state_dir.to_path_buf()
    } else {
        state_dir.join(RIFFS).join(riff.to_string())
    }
}

/// The riff that the daemon with the folder `state_dir` joined: the riff
/// ID in [`JOINED`], or `None` for the riff of its own.
pub fn joined(state_dir: &Path) -> Result<Option<Public>> {
    let path = state_dir.join(JOINED);
    match std::fs::read_to_string(&path) {
        Ok(riff) => Public::parse(riff.trim())
            .map(Some)
            .with_context(|| format!("cannot read the riff in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Keeps the riff `riff` that the daemon joined in [`JOINED`], or
/// removes the file for the riff of its own: a temp file, then a rename.
fn keep_joined(state_dir: &Path, riff: Option<Public>) -> Result<()> {
    let path = state_dir.join(JOINED);
    match riff {
        Some(riff) => {
            let temp = state_dir.join(format!("{JOINED}.new"));
            std::fs::write(&temp, format!("{riff}\n"))
                .with_context(|| format!("cannot write {}", temp.display()))?;
            std::fs::rename(&temp, &path)
                .with_context(|| format!("cannot write {}", path.display()))
        }
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("cannot remove {}", path.display()))
            }
            _ => Ok(()),
        },
    }
}

/// The time now, in milliseconds since the epoch.
fn now() -> u64 {
    u64::try_from(now_ms()).unwrap_or_default()
}

impl<R: Runtime + 'static> Daemon<R> {
    /// The hub of the riff of peers, when the daemon has its riff.
    fn hub(&self) -> Result<Arc<Hub>> {
        Ok(self
            .riff
            .get()
            .context(text::DAEMON_NO_RIFF_YET)?
            .hub
            .clone())
    }

    /// Refuses the owner step `command` on a daemon that is not the
    /// owner of its riff.
    fn must_own(hub: &Hub, command: &str) -> Result<()> {
        let (riff, owner) = hub.read(|state| (state.riff(), state.owner().map(str::to_owned)));
        if riff != hub.public() {
            bail!("{}", text::share_not_the_owner(command, owner.as_deref()));
        }
        Ok(())
    }

    /// `riff invite USER` at the time `now_ms`: an invite of one new key of
    /// the user `user` (01M519Z3SGEHVMS85HZD169CAP). Only the daemon of
    /// the owner makes one.
    pub fn invite(&self, user: &str, now_ms: u64) -> Result<Invite> {
        let hub = self.hub()?;
        Self::must_own(&hub, "riff invite")?;
        let user = user.to_lowercase();
        if riff_core::name::check("user", &user).is_err() {
            bail!("{}", text::invite_bad_user(&user));
        }
        self.invites.make(hub.public(), &user, now_ms)
    }

    /// `riff join STRING`: the daemon joins the riff of the invite string
    /// `invite` (01M519Z3YA9WBH9TWQPAH6A64Z). It gives `show` a line while
    /// it connects, and gives the line of the join.
    pub async fn join(&self, invite: &str, show: impl Fn(String)) -> Result<String> {
        let invite = Invite::parse(invite)?;
        let hub = self.hub()?;
        let me = hub.public();
        if invite.riff == me {
            bail!("{}", text::JOIN_OWN_RIFF);
        }
        let (riff, owner, members) = hub.read(|state| {
            let members = state
                .members()
                .values()
                .filter(|member| !member.removed && !member.left)
                .count();
            (state.riff(), state.owner().map(str::to_owned), members)
        });
        if riff != me {
            bail!("{}", text::join_in_a_riff(owner.as_deref()));
        }
        if members > 0 {
            bail!("{}", text::join_has_members(members));
        }
        if invite.ends_ms <= now() {
            bail!(
                "{}",
                text::join_ended(i64::try_from(invite.ends_ms).unwrap_or(i64::MAX))
            );
        }
        let links = self.links.get().context(text::SHARE_NO_LINKS)?.clone();
        show(text::join_connects(&invite.riff.to_string()));
        let host = crate::identity::this_host();
        let conn = crate::peer::join::ask(links.endpoint(), &invite, &host).await?;
        // The daemon of the owner wrote the member record: this daemon is
        // a member of the riff of the invite now.
        self.switch(invite.riff)?;
        links.serve_joined(conn);
        let _ = tokio::time::timeout(crate::peer::join::LIMIT, async {
            while hub.read(|state| state.user_of(&me).is_none()) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        let owner = hub.read(|state| state.owner().map(str::to_owned));
        let user = hub
            .read(|state| state.user_of(&me).map(str::to_owned))
            .unwrap_or(invite.user);
        Ok(text::joined_peers(
            &invite.riff.to_string(),
            owner.as_deref(),
            &user,
            &host,
        ))
    }

    /// `riff leave` on a member (01M519Z435DTR693NAK5GSB6XA): the end
    /// record of each session of the daemon on the riff of peers, the
    /// `left` record, the end of each link after it sent that record, and
    /// then the riff of its own with the same key. The owner cannot
    /// leave. It gives the line of the leave.
    pub async fn leave(&self) -> Result<String> {
        let hub = self.hub()?;
        let me = hub.public();
        let (riff, owner, member) = hub.read(|state| {
            (
                state.riff(),
                state.owner().map(str::to_owned),
                state.members().contains_key(&me),
            )
        });
        if riff == me {
            bail!("{}", text::LEAVE_OWNER);
        }
        for record in self.records().iter().filter(|record| record.on_peers()) {
            hub.end(&record.live.session)?;
        }
        // A daemon whose member record did not come yet writes no record.
        if member {
            hub.write(&Body::Left(Left {}))?;
            if let Some(links) = self.links.get() {
                links.wait_no_links(LEAVE_LIMIT).await;
            }
        }
        self.switch(me)?;
        Ok(text::left_riff(
            &riff.to_string(),
            owner.as_deref(),
            &me.to_string(),
        ))
    }

    /// `riff remove USER [--host HOST]` (01M519Z45J9RC734KBY1G8D49V): a
    /// `removed` record for each key that it removes. Only the daemon of
    /// the owner writes one. It gives one line for each key.
    pub fn remove(&self, user: &str, host: Option<&str>) -> Result<Vec<String>> {
        let hub = self.hub()?;
        Self::must_own(&hub, "riff remove")?;
        let removals = hub
            .read(|state| members::removals(state, user, host))
            .map_err(|why| anyhow!(why))?;
        let mut lines = Vec::new();
        for removed in removals {
            let host = hub.read(|state| {
                state
                    .members()
                    .get(&removed.key)
                    .map(|member| member.host.clone())
            });
            let who = text::peer_who(&removed.key.to_string(), Some(user), host.as_deref());
            hub.write(&Body::Removed(removed))?;
            lines.push(text::removed_key(&who));
        }
        Ok(lines)
    }

    /// `riff members` (01M519Z4823WC7D3VVTFSDYBS4): the owner and each
    /// member key of the riff, with its state.
    pub fn members(&self) -> Result<Vec<Shown>> {
        let hub = self.hub()?;
        let me = hub.public();
        let forked: BTreeSet<Public> = hub.with_peers(|peers| peers.forks().keys().copied().collect());
        let keys: Vec<Public> = hub.read(|state| {
            std::iter::once(state.riff())
                .chain(state.members().keys().copied())
                .collect()
        });
        let links = self.links.get();
        let online: BTreeSet<Public> = keys
            .iter()
            .filter(|key| links.is_some_and(|links| links.online(key)))
            .copied()
            .collect();
        let seen: BTreeMap<Public, i64> = keys
            .iter()
            .filter_map(|key| Some((*key, links?.seen(key)?)))
            .collect();
        Ok(hub.read(|state| {
            members::shown(
                state,
                me,
                &forked,
                |key| online.contains(key),
                |key| seen.get(key).copied(),
            )
        }))
    }

    /// The daemon is in the riff `riff` now: after a join, or with its own
    /// key after a leave (01M521J6WZT2KR5ZQE9DZ9A5AC,
    /// 01M521J6ZRNWBSPK7WB23TVY59). Each session of the daemon on the riff
    /// of peers ends in the old riff and starts in the new one. The riff
    /// of its own gets its `riff` record when it has none.
    fn switch(&self, riff: Public) -> Result<()> {
        let hub = self.hub()?;
        let me = hub.public();
        let on_peers: Vec<_> = self
            .records()
            .into_iter()
            .filter(|record| record.on_peers())
            .collect();
        for record in &on_peers {
            // After a `left` record, the key writes no record: the end
            // records came before it.
            let _ = hub.end(&record.live.session);
        }
        let state_dir = state_dir(&self.dir);
        hub.switch(&peers_dir(&state_dir, &riff, &me), riff, |line| {
            println!("{line}");
        })?;
        if riff == me && hub.read(|state| state.owner().is_none()) {
            let owner = &self.riff.get().context(text::DAEMON_NO_RIFF_YET)?.owner;
            hub.write(&Body::Riff(RiffBody {
                user: owner.clone(),
                host: Some(crate::identity::this_host()),
            }))?;
        }
        keep_joined(&state_dir, (riff != me).then_some(riff))?;
        for record in &on_peers {
            let _ = self.peer_start(record);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_key::Key;

    /// 01M521J6WZT2KR5ZQE9DZ9A5AC: the file of the joined riff holds the
    /// riff ID; with no file, the daemon is in its riff of its own.
    #[test]
    fn the_joined_riff_is_in_its_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(joined(dir.path()).unwrap(), None);
        let riff = Key::from_bytes(&[2; 32]).public();
        keep_joined(dir.path(), Some(riff)).unwrap();
        assert_eq!(joined(dir.path()).unwrap(), Some(riff));
        keep_joined(dir.path(), None).unwrap();
        assert_eq!(joined(dir.path()).unwrap(), None);
        keep_joined(dir.path(), None).unwrap();
        std::fs::write(dir.path().join(JOINED), "not a key").unwrap();
        assert!(joined(dir.path()).is_err());
    }
}
