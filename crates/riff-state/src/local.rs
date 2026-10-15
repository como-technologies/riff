//! The state of one process: a [`State`] with its log and its
//! checkpoints in one [`Store`]. The riff daemon keeps the state of
//! the sessions of its machine in it, with no server
//! (01M4Z60B8AFV68ZSAFA540G73C).
//!
//! # Design
//!
//! [`Local`] runs the same steps as the engine of riff-server, with no
//! HTTP, no sign-in and no lease. One process owns the store: the
//! daemon of the machine.
//!
//! ```mermaid
//! flowchart TD
//!     O["Local::open"] --> CP["load the newest checkpoint"] --> RP["replay the log after it"]
//!     RP --> L[Local]
//!     L --> R["Local::run: State::run_as, under the lock"]
//!     R --> W["write the records as one chunk"]
//!     W -->|written| A["State::written, then State::reply"]
//!     W -->|failed for good| B["the Local is broken: each next run fails"]
//!     A --> D{"a checkpoint is due?"}
//!     D -->|yes| C["write a checkpoint, then prune"]
//!     L --> S["Local::checkpoint, at a stop"]
//! ```
//!
//! - [`Local::open`] loads the newest checkpoint that this build reads
//!   ([`checkpoint::load`]), and replays the log after it
//!   ([`log::replay_after`]). An empty store gives an empty state: the
//!   caller then makes the riff with [`MakeRiff`].
//! - [`Local::run`] runs one command at a time. It holds the lock of the
//!   state while it writes, so each command sees the records of the
//!   command before it. A refused command can still make records: the
//!   register of a caller that the state does not know
//!   ([`State::check`]). They go to the log too.
//! - The records of a command are in the log before the command gets its
//!   reply: the reply comes from the written copy ([`State::reply`]).
//! - A write that fails for good ([`log::write`] tries again for
//!   [`log::Timing::retry_for`]) breaks the [`Local`]: the pending copy
//!   has records that the log does not have. Each next call fails with
//!   [`Error::Broken`]. A new [`Local::open`] reads the log again.
//! - After a command, [`Local::run`] writes a checkpoint when one is due
//!   ([`checkpoint::Settings::every_records`] records, or
//!   [`checkpoint::Settings::every`] with records), then prunes the old
//!   checkpoints and chunks ([`checkpoint::prune`]).
//! - [`Local::checkpoint`] writes a checkpoint now, also with no new
//!   record. So the memory of the presence stays across a restart. The
//!   daemon calls it at its stop.
//! - A checkpoint of a later version, a checkpoint that does not read,
//!   or a record that this build skipped, stops each checkpoint of this
//!   build ([`Local::blocked`]). The log still takes each record.
//!
//! # Example
//!
//! ```
//! # #[tokio::main(flavor = "current_thread")] async fn main() -> Result<(), riff_state::local::Error> {
//! use std::sync::Arc;
//! use std::time::Instant;
//! use riff_core::name::SessionUri;
//! use riff_core::wire::Join;
//! use riff_state::checkpoint::Settings;
//! use riff_state::local::Local;
//! use riff_state::state::{Caller, MakeRiff};
//! use riff_state::store::Memory;
//!
//! let store = Arc::new(Memory::default());
//! let local = Local::open(store.clone(), Settings::default()).await?;
//! let make = MakeRiff { riff_id: "r1".into() };
//! local.run(&Caller::server(), &make).await?;
//!
//! let mike: SessionUri = "riff://mike@thelio/como-technologies/riff?session=a6cf".parse().unwrap();
//! let join = Join { me: mike.clone(), thread: "design".parse().unwrap() };
//! local.run(&Caller::of(&mike), &join).await?;
//! drop(local);
//!
//! // A restart replays the log: the riff and the thread are there.
//! let again = Local::open(store, Settings::default()).await?;
//! assert_eq!(again.read(|state| state.riff_id()).await.as_deref(), Some("r1"));
//! let threads = again.read(|state| state.threads_of(mike.who(), Instant::now())).await;
//! assert!(threads.iter().any(|t| t.thread.to_string() == "design"));
//! # Ok(()) }
//! ```

use std::fmt;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use crate::checkpoint::{self, Checkpoint};
use crate::log;
use crate::state::{Caller, Command, Done, Refused, State};
use crate::store::{Store, StoreError};

#[cfg(doc)]
use crate::state::MakeRiff;

/// Why a call of [`Local`] failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The state refused the command. Its records, if any, are in the
    /// log.
    Refused(Refused),
    /// The store did not do a call: the open, or the write of a chunk.
    /// After a failed write, each next call fails with [`Error::Broken`].
    Store(StoreError),
    /// An earlier write failed for good. The text says why. Open the
    /// store again.
    Broken(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Refused(refused) => write!(f, "{}", refused.reason),
            Error::Store(error) => write!(f, "{error}"),
            Error::Broken(why) => write!(
                f,
                "the state stopped after a failed write of the log: {why}. Start it again"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<StoreError> for Error {
    fn from(error: StoreError) -> Self {
        Error::Store(error)
    }
}

/// The state of one process, with its log and its checkpoints in one
/// store. See the module docs.
pub struct Local {
    store: Arc<dyn Store>,
    settings: checkpoint::Settings,
    timing: log::Timing,
    inner: Mutex<Inner>,
}

/// The parts of a [`Local`] under its lock.
struct Inner {
    state: State,
    /// The position of the last checkpoint, and when this process wrote
    /// it or loaded it.
    checkpoint: u64,
    at: Instant,
    /// Why this build writes no checkpoint.
    blocked: Option<String>,
    /// Why the state stopped: a write that failed for good.
    broken: Option<String>,
}

/// The time now in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Local {
    /// Loads the state of `store`: the newest checkpoint that reads, and
    /// the log after it. `settings` say when [`Local`] writes a
    /// checkpoint, which it keeps, and the build that writes them.
    pub async fn open(store: Arc<dyn Store>, settings: checkpoint::Settings) -> Result<Local, Error> {
        let found = checkpoint::load(&*store, &settings.build).await?;
        let snapshot = found.checkpoint.map(|c| c.state);
        let from = snapshot.as_ref().map_or(0, |s| s.position);
        let replayed = log::replay_after(&*store, from).await?;
        let mut state = State::load(snapshot, replayed.records, Instant::now(), now_ms());
        state.continue_after(replayed.last);
        let blocked = found.blocked.or_else(|| {
            replayed
                .skipped
                .map(|position| format!("this build skipped the record at position {position}"))
        });
        Ok(Local {
            store,
            settings,
            timing: log::Timing::default(),
            inner: Mutex::new(Inner {
                state,
                checkpoint: from,
                at: Instant::now(),
                blocked,
                broken: None,
            }),
        })
    }

    /// The same, with the times `timing` of the write of a chunk.
    pub fn with_timing(self, timing: log::Timing) -> Local {
        Local { timing, ..self }
    }

    /// Runs `command` of `caller`: it checks the command, writes its
    /// records to the log, and gives the reply from the written copy.
    /// Then it writes a checkpoint when one is due.
    pub async fn run<C: Command>(&self, caller: &Caller, command: &C) -> Result<C::Reply, Error> {
        let mut inner = self.inner.lock().await;
        if let Some(why) = &inner.broken {
            return Err(Error::Broken(why.clone()));
        }
        let now = Instant::now();
        let ran = inner.state.run_as(caller, command, now);
        let records = inner.state.take_queue();
        if let Err(error) = log::write(&*self.store, &records, &self.timing, || true).await {
            inner.broken = Some(error.to_string());
            return Err(Error::Store(error));
        }
        inner.state.written(&records);
        let (caller, made, note) = ran.map_err(Error::Refused)?;
        let reply = inner
            .state
            .reply(&caller, command, &Done::of(made), note, now);
        let came = inner.state.written_position().saturating_sub(inner.checkpoint);
        let due = came >= self.settings.every_records || inner.at.elapsed() >= self.settings.every;
        if came > 0 && due {
            self.write_checkpoint(&mut inner, true).await;
        }
        Ok(reply)
    }

    /// Reads the written copy of the state.
    pub async fn read<T>(&self, with: impl FnOnce(&State) -> T) -> T {
        with(&self.inner.lock().await.state)
    }

    /// Writes a checkpoint now, also when no record came since the last
    /// one. It does not prune. Gives the name of the checkpoint, or
    /// `None` when this build writes no checkpoint ([`Local::blocked`]).
    pub async fn checkpoint(&self) -> Result<Option<String>, Error> {
        let mut inner = self.inner.lock().await;
        if let Some(why) = &inner.broken {
            return Err(Error::Broken(why.clone()));
        }
        Ok(self.write_checkpoint(&mut inner, false).await)
    }

    /// Why this build writes no checkpoint, when it writes none.
    pub async fn blocked(&self) -> Option<String> {
        self.inner.lock().await.blocked.clone()
    }

    /// Writes the checkpoint of the written copy, then prunes when
    /// `prune`. A failed write or prune does not stop the state: the log
    /// has each record. Gives the name of the checkpoint.
    async fn write_checkpoint(&self, inner: &mut Inner, prune: bool) -> Option<String> {
        if inner.blocked.is_some() {
            return None;
        }
        let at_ms = now_ms();
        let snapshot = inner.state.snapshot(Instant::now(), at_ms);
        let position = snapshot.position;
        let checkpoint = Checkpoint::new(&self.settings.build, at_ms, snapshot);
        let name = match checkpoint::write(&*self.store, &checkpoint).await {
            Ok(name) => name,
            Err(error) => {
                tracing::warn!("the checkpoint at position {position} was not written: {error}");
                return None;
            }
        };
        inner.checkpoint = position;
        inner.at = Instant::now();
        if prune && let Err(error) = checkpoint::prune(&*self.store, &self.settings, at_ms).await {
            tracing::warn!("the delete of old checkpoints failed: {error}");
        }
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Code, MakeRiff, Role};
    use crate::store::{Loaded, Memory, Version};
    use futures::future::BoxFuture;
    use riff_core::name::SessionUri;
    use riff_core::wire::{Claim, Resume};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn mike() -> SessionUri {
        "riff://mike@thelio/como-technologies/riff?session=a6cf"
            .parse()
            .unwrap()
    }

    fn settings() -> checkpoint::Settings {
        checkpoint::Settings {
            build: "3.0.0".into(),
            ..checkpoint::Settings::default()
        }
    }

    /// A new riff with its ID, running.
    async fn running(local: &Local) {
        let make = MakeRiff {
            riff_id: "r1".into(),
        };
        local.run(&Caller::server(), &make).await.unwrap();
        // A person resumes the riff: an admin.
        let person: SessionUri = "riff://mike@thelio".parse().unwrap();
        let admin = Caller::of(&person).with_role(Role::Admin);
        local.run(&admin, &Resume::whole(person)).await.unwrap();
    }

    async fn claim(local: &Local, item: &str) -> Result<riff_core::wire::ClaimReply, Error> {
        let claim = Claim {
            me: mike(),
            thread: mike().default_thread().unwrap(),
            item: item.into(),
        };
        local.run(&Caller::of(&mike()), &claim).await
    }

    async fn claims(local: &Local) -> Vec<String> {
        local
            .read(|state| state.uri(mike().who(), Instant::now()).claims().to_vec())
            .await
    }

    #[tokio::test]
    async fn an_empty_store_gives_an_empty_state() {
        let local = Local::open(Arc::new(Memory::default()), settings())
            .await
            .unwrap();
        assert_eq!(local.read(State::riff_id).await, None);
        assert_eq!(local.read(State::written_position).await, 0);
        assert_eq!(local.blocked().await, None);
    }

    /// The records of each command are in the log, and a new open
    /// replays them.
    #[tokio::test]
    async fn a_new_open_replays_the_log() {
        let store = Arc::new(Memory::default());
        let local = Local::open(store.clone(), settings()).await.unwrap();
        running(&local).await;
        claim(&local, "issue-12").await.unwrap();
        assert_eq!(claims(&local).await, ["issue-12"]);
        let position = local.read(State::written_position).await;
        drop(local);

        assert!(checkpoint::names(&*store).await.unwrap().is_empty());
        let again = Local::open(store, settings()).await.unwrap();
        assert_eq!(again.read(State::written_position).await, position);
        assert_eq!(again.read(State::riff_id).await.as_deref(), Some("r1"));
        assert_eq!(claims(&again).await, ["issue-12"]);
        // The next record comes after the last one of the log.
        claim(&again, "issue-13").await.unwrap();
        assert_eq!(again.read(State::written_position).await, position + 1);
    }

    /// A refused command is refused after the write of the register of
    /// its caller: a new open knows the session.
    #[tokio::test]
    async fn a_refused_command_keeps_the_register_of_its_caller() {
        let store = Arc::new(Memory::default());
        let local = Local::open(store.clone(), settings()).await.unwrap();
        let make = MakeRiff {
            riff_id: "r1".into(),
        };
        local.run(&Caller::server(), &make).await.unwrap();
        // A new riff is paused.
        let Err(Error::Refused(refused)) = claim(&local, "issue-12").await else {
            panic!("a claim in a paused riff is refused");
        };
        assert_eq!(refused.code, Code::Paused);
        drop(local);

        let again = Local::open(store, settings()).await.unwrap();
        let known = again
            .read(|state| state.who(Instant::now(), now_ms(), true))
            .await;
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].uri.who(), mike().who());
    }

    /// A checkpoint at a stop: a new open starts from it, and replays
    /// only the log after it.
    #[tokio::test]
    async fn a_new_open_starts_from_the_checkpoint() {
        let store = Arc::new(Memory::default());
        let local = Local::open(store.clone(), settings()).await.unwrap();
        running(&local).await;
        claim(&local, "issue-12").await.unwrap();
        let name = local.checkpoint().await.unwrap().unwrap();
        let position = local.read(State::written_position).await;
        drop(local);

        let found = checkpoint::load(&*store, "3.0.0").await.unwrap();
        assert_eq!(found.newest.as_deref(), Some(name.as_str()));
        assert_eq!(found.checkpoint.unwrap().state.position, position);
        // The checkpoint holds the state: the log can go.
        for (_, chunk) in log::chunks(&*store).await.unwrap() {
            store.delete(&chunk).await.unwrap();
        }
        let again = Local::open(store, settings()).await.unwrap();
        assert_eq!(again.read(State::written_position).await, position);
        assert_eq!(claims(&again).await, ["issue-12"]);
    }

    #[tokio::test]
    async fn a_run_writes_a_checkpoint_when_one_is_due() {
        let store = Arc::new(Memory::default());
        let every = checkpoint::Settings {
            every_records: 4,
            ..settings()
        };
        let local = Local::open(store.clone(), every).await.unwrap();
        running(&local).await;
        assert_eq!(local.read(State::written_position).await, 3);
        assert!(checkpoint::names(&*store).await.unwrap().is_empty());
        claim(&local, "issue-12").await.unwrap();
        let names = checkpoint::names(&*store).await.unwrap();
        assert!(!names.is_empty());
        let newest = names.last().unwrap().0;
        assert_eq!(newest, local.read(State::written_position).await);
    }

    #[tokio::test]
    async fn a_checkpoint_of_a_later_version_stops_each_checkpoint() {
        let store = Arc::new(Memory::default());
        let later = checkpoint::Settings {
            build: "3.1.0".into(),
            ..settings()
        };
        let local = Local::open(store.clone(), later).await.unwrap();
        running(&local).await;
        local.checkpoint().await.unwrap().unwrap();
        drop(local);

        let local = Local::open(store.clone(), settings()).await.unwrap();
        let why = local.blocked().await.unwrap();
        assert!(why.contains("the later version 3.1.0"), "{why}");
        claim(&local, "issue-12").await.unwrap();
        assert_eq!(local.checkpoint().await.unwrap(), None);
        assert_eq!(checkpoint::names(&*store).await.unwrap().len(), 1);
    }

    /// A store whose saves fail while `fail` is set.
    #[derive(Clone, Default)]
    struct Failing {
        memory: Memory,
        fail: Arc<AtomicBool>,
    }

    impl Store for Failing {
        fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
            self.memory.load(name)
        }

        fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
            self.memory.list(prefix)
        }

        fn save<'a>(
            &'a self,
            name: &'a str,
            bytes: Vec<u8>,
            version: Option<Version>,
        ) -> BoxFuture<'a, Result<Version, StoreError>> {
            if self.fail.load(Ordering::SeqCst) {
                return Box::pin(async { Err(StoreError::Failed("the disk is full".into())) });
            }
            self.memory.save(name, bytes, version)
        }

        fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
            self.memory.delete(name)
        }
    }

    #[tokio::test]
    async fn a_failed_write_stops_the_state_until_a_new_open() {
        let store = Failing::default();
        let quick = log::Timing {
            retry_for: Duration::from_millis(1),
            ..log::Timing::default()
        };
        let local = Local::open(Arc::new(store.clone()), settings())
            .await
            .unwrap()
            .with_timing(quick);
        running(&local).await;
        store.fail.store(true, Ordering::SeqCst);
        let Err(Error::Store(_)) = claim(&local, "issue-12").await else {
            panic!("the write fails");
        };
        store.fail.store(false, Ordering::SeqCst);
        let Err(Error::Broken(why)) = claim(&local, "issue-13").await else {
            panic!("the state stopped");
        };
        assert!(why.contains("the disk is full"), "{why}");
        assert!(matches!(local.checkpoint().await, Err(Error::Broken(_))));
        drop(local);

        // The log has no record of the failed claim.
        let again = Local::open(Arc::new(store), settings()).await.unwrap();
        assert!(claims(&again).await.is_empty());
        claim(&again, "issue-13").await.unwrap();
        assert_eq!(claims(&again).await, ["issue-13"]);
    }
}
