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
        let forked: BTreeSet<Public> =
            hub.with_peers(|peers| peers.forks().keys().copied().collect());
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
    use std::time::Duration;

    use iroh::address_lookup::memory::MemoryLookup;
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;

    use super::super::{Ask, dir_in, listen};
    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::Message;
    use crate::peer::invite::{Secret, Text};
    use crate::peer::record::{Context as _, Standing};
    use crate::peer::sync::Net;
    use crate::runtime::Fake;

    /// One machine of a test: a riff daemon on a fake runtime, with the
    /// mock keyring, its own folder and its control socket, and an iroh
    /// endpoint on 127.0.0.1 that finds the daemons of [`Machine::knows`].
    struct Machine {
        root: tempfile::TempDir,
        dir: PathBuf,
        lookup: MemoryLookup,
        daemon: Arc<Daemon<Fake>>,
        stop: Option<oneshot::Sender<()>>,
        served: JoinHandle<Result<()>>,
    }

    impl Machine {
        async fn new() -> Machine {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            Machine::start(root, dir, MemoryLookup::new()).await
        }

        async fn start(root: tempfile::TempDir, dir: PathBuf, lookup: MemoryLookup) -> Machine {
            crate::secrets::mock_store();
            let listener = listen(&dir).await.unwrap();
            let daemon = Daemon::new(Fake::default(), dir.clone())
                .unwrap()
                .with_net(Net::Local(lookup.clone()));
            daemon
                .open_riff(Duration::from_millis(10), |_| {})
                .await
                .unwrap();
            let daemon = Arc::new(daemon);
            let (stop, stopped) = oneshot::channel::<()>();
            let served = tokio::spawn(daemon.clone().serve(listener, async {
                let _ = stopped.await;
            }));
            Machine {
                root,
                dir,
                lookup,
                daemon,
                stop: Some(stop),
                served,
            }
        }

        /// Stops the daemon, and starts a new one in the same folder.
        async fn restart(mut self) -> Machine {
            let _ = self.stop.take().unwrap().send(());
            self.served.await.unwrap().unwrap();
            self.daemon.close_links().await;
            Machine::start(self.root, self.dir, self.lookup).await
        }

        /// This daemon finds the daemon of `other`.
        fn knows(&self, other: &Machine) {
            let links = other.daemon.links().expect("peer links");
            self.lookup.set_endpoint_info(links.endpoint().addr());
        }

        fn key(&self) -> Public {
            self.hub().public()
        }

        fn hub(&self) -> Arc<Hub> {
            self.daemon.hub().unwrap()
        }

        /// The user of the daemon: the owner of its riff of its own.
        fn user(&self) -> String {
            self.daemon.riff.get().unwrap().owner.clone()
        }

        /// `riff invite USER` through the control socket.
        async fn invite(&self, user: &str) -> Result<Text> {
            Ok(crate::daemon::invite(&self.dir, user).await?.0)
        }

        /// `riff join STRING` through the control socket.
        async fn join(&self, invite: &str) -> Result<Vec<String>> {
            self.ask(Ask::Join {
                invite: Text(invite.into()),
            })
            .await
        }

        /// The lines of the daemon for `ask`, or why it failed.
        async fn ask(&self, ask: Ask) -> Result<Vec<String>> {
            let mut lines = Vec::new();
            crate::daemon::ask_done(&self.dir, ask, |line| lines.push(line.to_owned())).await?;
            Ok(lines)
        }

        /// The riff of `riff daemon status`.
        async fn riff(&self) -> String {
            crate::daemon::status(&self.dir)
                .await
                .unwrap()
                .riff
                .unwrap()
        }

        /// The heads of the records of the daemon.
        fn heads(&self) -> crate::peer::keep::Heads {
            self.hub().with_peers(crate::peer::keep::Peers::heads)
        }

        /// Each file of the folder of the daemon, with its bytes.
        fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
            fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
                for entry in std::fs::read_dir(dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, out);
                    } else if let Ok(bytes) = std::fs::read(&path) {
                        out.push((path, bytes));
                    }
                }
            }
            let mut out = Vec::new();
            walk(&self.dir, &mut out);
            out
        }
    }

    /// Waits up to 30 seconds for `done`.
    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        let end = tokio::time::Instant::now() + Duration::from_secs(30);
        while !done() {
            assert!(tokio::time::Instant::now() < end, "no {what} in 30 s");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The owner, and two other machines. Each daemon finds each other
    /// daemon.
    async fn three() -> (Machine, Machine, Machine) {
        let (o, n, t) = (
            Machine::new().await,
            Machine::new().await,
            Machine::new().await,
        );
        for (a, b) in [(&o, &n), (&o, &t), (&n, &t)] {
            a.knows(b);
            b.knows(a);
        }
        (o, n, t)
    }

    /// 01M519Z3YA9WBH9TWQPAH6A64Z, 01M519Z3SGEHVMS85HZD169CAP,
    /// 01M519Z3VXY4R3WDSSBQCVYTS4: the owner invites, a second daemon joins
    /// with the string and syncs. A wrong secret and a string after its end
    /// fail and change nothing. A second join with the same string fails.
    /// No invite secret is in a file, a record or a message. After a new
    /// start, the daemon is in the riff that it joined.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_owner_invites_and_a_second_daemon_joins_with_the_string_and_syncs() {
        let (o, n, t) = three().await;
        let invite = o.invite("Brett").await.unwrap();
        let read = Invite::parse(&invite.0).unwrap();
        assert_eq!((read.riff, read.user.as_str()), (o.key(), "brett"));
        let owner_heads = o.heads();

        // A wrong secret: refused, and nothing changes.
        let wrong = Invite::new(o.key(), "brett", Secret::random().unwrap(), read.ends_ms);
        let error = n.join(&wrong.text()).await.unwrap_err().to_string();
        assert_eq!(error, text::join_refused(text::INVITE_UNKNOWN));
        assert_eq!(o.heads(), owner_heads);
        assert_eq!(n.riff().await, n.key().to_string());

        // An invite that ended: refused by the new daemon, and by the
        // owner also when the string says another end.
        let ended = o.daemon.invite("ann", now() - 25 * 3600 * 1000).unwrap();
        let error = n.join(&ended.text()).await.unwrap_err().to_string();
        assert!(error.starts_with("That invite ended at "), "{error}");
        let forged = Invite::new(o.key(), "ann", ended.secret().clone(), read.ends_ms);
        let error = n.join(&forged.text()).await.unwrap_err().to_string();
        assert_eq!(error, text::join_refused(text::INVITE_ENDED));
        assert_eq!(o.heads(), owner_heads);

        // The join: the member record, and the records of the riff.
        let lines = n.join(&invite.0).await.unwrap();
        let host = crate::identity::this_host();
        assert_eq!(
            lines.last().unwrap(),
            &text::joined_peers(&o.key().to_string(), Some(&o.user()), "brett", &host)
        );
        assert_eq!(n.riff().await, o.key().to_string());
        let me = n.key();
        let (user, member_host) = o.hub().read(|state| {
            let member = &state.members()[&me];
            (member.user.clone(), member.host.clone())
        });
        assert_eq!(
            (user.as_str(), member_host.as_str()),
            ("brett", host.as_str())
        );
        assert_eq!(
            n.hub().read(|state| state.owner().map(str::to_owned)),
            Some(o.user())
        );

        // A new record of the owner reaches the new member.
        let hello = Body::Message(Box::new(Message {
            from: format!("riff://{}@thelio/o/r?session=s1", o.user())
                .parse()
                .unwrap(),
            thread: "o/r".parse().unwrap(),
            to: vec![],
            kind: Default::default(),
            body: "hello brett".into(),
        }));
        o.hub().write(&hello).unwrap();
        let thread = "o/r".parse().unwrap();
        until("the message of the owner", || {
            n.hub().read(|state| state.messages(&thread).len()) == 1
        })
        .await;

        // A second join with the same string fails, and changes nothing.
        let owner_heads = o.heads();
        let error = t.join(&invite.0).await.unwrap_err().to_string();
        assert_eq!(error, text::join_refused(text::INVITE_USED));
        assert_eq!(o.heads(), owner_heads);
        assert_eq!(t.riff().await, t.key().to_string());

        // riff members on the new member: the owner and this machine.
        let rows = crate::daemon::members(&n.dir).await.unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows[0].owner && !rows[0].this);
        assert!(rows[1].this && rows[1].user.as_deref() == Some("brett"));

        // No invite secret in a file of a daemon, in a record, or in a
        // message.
        for invite in [&read, &ended] {
            let secret = invite.secret();
            let hex = secret.hex();
            let string = invite.text();
            let part = &string.as_bytes()[10..40];
            for machine in [&o, &n, &t] {
                for (path, bytes) in machine.files() {
                    let shown = path.display();
                    assert!(!bytes.windows(32).any(|w| w == secret.bytes()), "{shown}");
                    assert!(!bytes.windows(64).any(|w| w == hex.as_bytes()), "{shown}");
                    assert!(!bytes.windows(30).any(|w| w == part), "{shown}");
                }
                let records = machine.hub().with_peers(|p| p.lacks(&Default::default()));
                for record in records {
                    let body = String::from_utf8_lossy(record.body()).into_owned();
                    assert!(!body.contains(hex.as_str()), "{body}");
                }
            }
        }

        // After a new start, the daemon is in the riff that it joined.
        let n = n.restart().await;
        assert_eq!(n.riff().await, o.key().to_string());
        assert_eq!(n.hub().read(|state| state.messages(&thread).len()), 1);
    }

    /// 01M519Z40SHPJ1BB2SZN9AW43F: riff join refuses while the riff of the
    /// daemon has another member, and while the daemon is in the riff of
    /// another owner. Only the owner invites and removes. The owner cannot
    /// leave.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_daemon_is_in_one_riff_at_a_time_and_only_the_owner_invites() {
        let (o, n, t) = three().await;
        let invite = o.invite("brett").await.unwrap();
        n.join(&invite.0).await.unwrap();
        let other = t.invite("ann").await.unwrap();

        let error = o.join(&other.0).await.unwrap_err().to_string();
        assert_eq!(error, text::join_has_members(1));
        let error = n.join(&other.0).await.unwrap_err().to_string();
        assert_eq!(error, text::join_in_a_riff(Some(&o.user())));
        let own = o.invite("x").await.unwrap();
        let error = o.join(&own.0).await.unwrap_err().to_string();
        assert_eq!(error, text::JOIN_OWN_RIFF);

        let not_owner = |command| text::share_not_the_owner(command, Some(&o.user()));
        let error = n.invite("ann").await.unwrap_err().to_string();
        assert_eq!(error, not_owner("riff invite"));
        let remove = Ask::Remove {
            user: "brett".into(),
            host: None,
        };
        let error = n.ask(remove).await.unwrap_err().to_string();
        assert_eq!(error, not_owner("riff remove"));
        let error = o.ask(Ask::Leave).await.unwrap_err().to_string();
        assert_eq!(error, text::LEAVE_OWNER);
        let error = o.invite("a b").await.unwrap_err().to_string();
        assert_eq!(error, text::invite_bad_user("a b"));
    }

    /// 01M519Z435DTR693NAK5GSB6XA, 01M521J6ZRNWBSPK7WB23TVY59: after riff
    /// leave, the owner kept the left record before the link ended, and
    /// refuses the key: no link, and no later record. The daemon that left
    /// has a riff of its own with the same key.
    #[tokio::test(flavor = "multi_thread")]
    async fn after_riff_leave_the_others_refuse_the_key_and_the_daemon_has_a_riff_of_its_own() {
        let (o, n, _t) = three().await;
        n.join(&o.invite("brett").await.unwrap().0).await.unwrap();
        let me = n.key();
        let links = o.daemon.links().unwrap().clone();
        until("the link of the new member", || links.online(&me)).await;

        let lines = n.ask(Ask::Leave).await.unwrap();
        let left = text::left_riff(&o.key().to_string(), Some(&o.user()), &me.to_string());
        assert_eq!(lines, [left]);
        let (seq, _) = o.hub().read(|state| state.heads()[&me]);
        assert!(o.hub().read(|state| state.members()[&me].left));
        assert_eq!(
            o.hub().read(|state| state.standing(&me)),
            Standing::Member {
                user: "brett".into(),
                until: Some(seq)
            }
        );
        until("the end of the link", || !links.online(&me)).await;
        assert!(
            !links
                .shown()
                .iter()
                .any(|shown| shown.key == me.to_string())
        );

        assert_eq!(n.riff().await, me.to_string());
        assert_eq!(joined(&state_dir(&n.dir)).unwrap(), None);
        assert_eq!(
            n.hub().read(|state| state.owner().map(str::to_owned)),
            Some(n.user())
        );
        assert!(n.invite("ann").await.is_ok());
    }

    /// 01M519Z45J9RC734KBY1G8D49V: riff remove brett removes each key of
    /// brett, and riff members shows them as removed.
    #[tokio::test(flavor = "multi_thread")]
    async fn riff_remove_removes_each_key_of_the_user() {
        let (o, n, t) = three().await;
        n.join(&o.invite("brett").await.unwrap().0).await.unwrap();
        t.join(&o.invite("brett").await.unwrap().0).await.unwrap();
        let remove = || Ask::Remove {
            user: "brett".into(),
            host: None,
        };
        let lines = o.ask(remove()).await.unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        let host = crate::identity::this_host();
        for key in [n.key(), t.key()] {
            let who = text::peer_who(&key.to_string(), Some("brett"), Some(&host));
            assert!(lines.contains(&text::removed_key(&who)), "{lines:?}");
            assert!(o.hub().read(|state| state.members()[&key].removed));
        }
        let rows = crate::daemon::members(&o.dir).await.unwrap();
        assert_eq!(rows.iter().filter(|row| row.removed).count(), 2);
        let error = o.ask(remove()).await.unwrap_err().to_string();
        assert_eq!(error, text::remove_no_key("brett", None));
    }

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
