//! The lease: only one instance of `riff-server` serves (R29,
//! R137-R142).
//!
//! During a deploy, Cloud Run runs the old and the new instance for a
//! short time. The lease is the object [`LEASE`] in the store. It holds
//! the ID of the instance that may serve.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant O as old instance
//!     participant S as store
//!     participant N as new instance
//!     N->>S: load the state
//!     Note over N: a load that fails: exit, no lease, no port
//!     Note over N: opens its port: 503 until it serves
//!     N->>S: write the lease (new ID)
//!     Note over N: waits 15 s
//!     O->>S: read the lease (every 2 s)
//!     S-->>O: new ID
//!     Note over O: stops for good: closes its port, saves no record
//!     O->>S: write the last checkpoint: the hand-over
//!     Note over O: exits
//!     N->>S: list the checkpoints again
//!     Note over N: a checkpoint came: load the state again from it
//!     N->>S: read the chunks that came since the load
//!     Note over N: serves from the next whole second
//! ```
//!
//! # Rules
//!
//! - At start, an instance loads the state first, then opens its port
//!   (01M52JYGHHK5X0XS39VFT3ZGGM). Then it writes a new random ID to the
//!   lease with [`Lease::take`]. It waits [`Timing::wait`], applies the
//!   chunks that came since the load, and starts to serve (R138). See
//!   [`crate::Service::load`].
//! - The lease also holds the [`Revision`] of its instance: the order of
//!   the deploys (01M53NHJXGK1PN2A9F7NSX0B4G). An instance of an older
//!   revision does not take a live lease of a newer revision. It replies
//!   503, reads the lease each [`Timing::read_every`], and takes the
//!   lease only when that lease ended. See [`Lease::take_as`] and "A
//!   second old instance" below. With no revision on one side, the
//!   newest instance takes the lease.
//! - It reads the lease each [`Timing::read_every`]. It serves only for
//!   [`Timing::valid_for`] after the start of the last read that showed
//!   its own ID. Else it replies 503 (R139).
//! - When a read shows another ID, the instance stops for good
//!   (01M52JYGPX5ETP3HKR0S0Y0YCR). A save that finds another version
//!   does the same (R141). It closes its port at once, so Cloud Run
//!   sends each call to the new instance, and it exits after the
//!   hand-over (see [`crate::listen::serve`]).
//! - When that ID is of another instance, the instance then writes a
//!   last checkpoint: the hand-over (01M51X8MG0YQT5KBZC9H7917J3). The
//!   read cursors are only in memory and in the checkpoints, so the new
//!   instance loads its state again from it after its wait
//!   (01M51X8P4M8PHSSJMWF1J1KQS1). With the ID of a tool, or after a
//!   lease that ended by its age, it writes none: a tool can cut the
//!   log then. See [`Lease::taken_by_instance`].
//! - Each [`Timing::renew_every`], the read is a renewal: the instance
//!   also writes the time to the lease with [`Lease::renew`]
//!   (01M3X34282SG0DJ6X34F90HS26). A renewal that fails counts as a read
//!   that failed, and the next read is a renewal again.
//! - On a shutdown, the instance ends its lease with [`Lease::end`]
//!   (01M3X342ARX5Y7R9ZJDT12R9A1).
//!
//! # Why 15 seconds is enough
//!
//! The new instance must not serve while the old one serves. It must
//! also refuse each proof that the old instance saw, because it does not
//! know their IDs (R142).
//!
//! - The old instance read its own ID for the last time before the new
//!   ID was in the store. So it serves for at most 5 seconds after the
//!   write.
//! - A proof is at most 10 seconds in the future (R86). So each proof
//!   that the old instance saw was issued at most 15 seconds after the
//!   write, and its `iat` is at most that second.
//! - The new instance starts to serve 15 seconds after the write, at the
//!   next whole second. It refuses each proof issued before that second.
//!
//! So no proof works on both instances, and the two never serve at the
//! same time.
//!
//! # A second old instance
//!
//! At a deploy, the old instance closes its port when it reads the new
//! lease. Cloud Run sends the calls to the old revision until the new
//! revision is ready. So Cloud Run can start a second instance of the
//! old revision, while the new instance waits. The second old instance
//! reads a lease of a newer revision, and does not take it. So the wait
//! of the new instance does not start again, and the second old instance
//! exits at its SIGTERM.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant O as old instance, revision 25
//!     participant S as store
//!     participant N as new instance, revision 26
//!     participant P as second old instance, revision 25
//!     N->>S: write the lease (revision 26)
//!     Note over N: waits 15 s
//!     O->>S: read the lease: another ID
//!     Note over O: closes its port, hands over, exits
//!     Note over P: Cloud Run starts it: no old port is open
//!     P->>S: read the lease: revision 26 is newer
//!     Note over P: does not take it: 503, reads it each 2 s
//!     Note over N: serves
//!     Note over P: SIGTERM: exits
//! ```
//!
//! # A live lease
//!
//! A tool that changes the log, for example `riff-server log cut`, must
//! not run while an instance writes the store. [`holder`] gives the
//! instance of a live lease (01M3X342DH98YEZ3X5CND43DGD).
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> live: an instance takes the lease
//!     live --> live: the instance writes the time, each 30 s
//!     live --> ended: the instance shuts down and ends the lease
//!     live --> ended: 90 s with no new time
//!     ended --> live: an instance takes the lease
//! ```
//!
//! An instance serves for at most [`Timing::renew_every`], plus one
//! [`Timing::read_every`], plus [`Timing::valid_for`] after the time in
//! the lease. A try of a chunk takes at most 3 seconds more
//! ([`riff_state::log::Timing::attempt`]). That is 40 seconds. The other 50
//! seconds of [`Timing::ends_after`] are for the difference between the
//! clock of the instance and the clock of the tool.
//!
//! The tool compares the time in the lease with its own clock. So the
//! clock of the tool must be less than 50 seconds ahead of the clock of
//! the instance.
//!
//! A lease of an older build has no time. The tool cannot know that
//! its instance stopped, so the lease is live until a person deletes
//! the object. An instance that still runs then stops for good (01M52JYGPX5ETP3HKR0S0Y0YCR).
//!
//! # A tool takes the lease
//!
//! An instance can stop for longer than the live time and then run
//! again: a process that a signal stopped, a machine that slept. It
//! holds records in memory that a cut removed. Two rules keep it from
//! the log:
//!
//! - The tool takes the lease with an ID of its own
//!   ([`ToolLease::take`], 01M3X5TP9CD4NXGPJEGP2Q4RS0). The write names
//!   the version of the lease that the tool read, so it fails when an
//!   instance wrote the lease in between. The old instance then reads
//!   another ID and stops for good (01M52JYGPX5ETP3HKR0S0Y0YCR). After a lease that ended by
//!   its age, the tool waits [`Timing::settle`] and reads the lease
//!   again, before it changes the log.
//! - An instance whose last write of the time is
//!   [`Timing::ends_after`] old stops for good, before it reads or
//!   writes the lease again (01M3X5TPBMF81TDVZ7Q4NVXBQX). It counts
//!   the time on the monotonic clock and on the wall clock.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant I as instance
//!     participant S as store
//!     participant T as log cut --yes
//!     Note over I: does not run for 95 s
//!     T->>S: read the lease: ID of I, 95 s old
//!     T->>S: write the lease: ID of T, only when not changed
//!     Note over T: waits 10 s, reads the lease again
//!     T->>S: remove the records
//!     T->>S: end the lease of T
//!     Note over I: runs again
//!     Note over I: its time is 95 s old: stops for good, exits with an error
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use riff_state::store::{LEASE, Store, StoreError, Version};

/// The times of the lease rules. Tests use short times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The wait after the lease write, before the instance serves (R138).
    pub wait: Duration,
    /// The time between two reads of the lease (R139).
    pub read_every: Duration,
    /// How long a read that shows the own ID lets the instance serve
    /// (R139).
    pub valid_for: Duration,
    /// How long a stopped instance waits at most for the end of the
    /// hand-over and of the calls in flight, with its port closed,
    /// before it exits (01M52JYGPX5ETP3HKR0S0Y0YCR).
    pub exit_after: Duration,
    /// The time between two writes of the time to the lease
    /// (01M3X34282SG0DJ6X34F90HS26).
    pub renew_every: Duration,
    /// How long a lease is live after the time that it holds
    /// (01M3X342DH98YEZ3X5CND43DGD).
    pub ends_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            wait: Duration::from_secs(15),
            read_every: Duration::from_secs(2),
            valid_for: Duration::from_secs(5),
            exit_after: Duration::from_secs(10),
            renew_every: Duration::from_secs(30),
            ends_after: Duration::from_secs(90),
        }
    }
}

impl Timing {
    /// How long an instance can write after another holder took the
    /// lease: one read, the serve time of the read before it, and one
    /// try of a chunk.
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff_server::lease::Timing;
    ///
    /// assert_eq!(Timing::default().settle(), Duration::from_secs(10));
    /// ```
    pub fn settle(&self) -> Duration {
        self.read_every + self.valid_for + riff_state::log::Timing::default().attempt
    }
}

/// The order of the deploy of an instance: the number of its Cloud Run
/// revision (01M53NHJXGK1PN2A9F7NSX0B4G). Cloud Run names each revision
/// `SERVICE-NUMBER-SUFFIX`, and the number grows with each deploy, also
/// with the deploy of an older release. Cloud Run gives the name to the
/// instance in `K_REVISION`.
///
/// ```
/// use riff_server::lease::Revision;
///
/// let old = Revision::parse("riff-server-00025-2s5").unwrap();
/// let new = Revision::parse("riff-server-00026-899").unwrap();
/// assert!(old < new);
/// assert_eq!(new.to_string(), "26");
/// // A name with no number gives no revision.
/// for name in ["riff-server", "riff-server-v1", "00026-899", "riff-+26-899", ""] {
///     assert_eq!(Revision::parse(name), None, "{name}");
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// The revision of the Cloud Run revision `name`, or `None` when the
    /// name has no number.
    pub fn parse(name: &str) -> Option<Revision> {
        let mut parts = name.rsplit('-');
        parts.next().filter(|suffix| !suffix.is_empty())?;
        let number = parts.next().filter(|n| n.bytes().all(|b| b.is_ascii_digit()))?;
        // The name of the service comes before the number.
        parts.next().filter(|service| !service.is_empty())?;
        number.parse().ok().map(Revision)
    }
}

impl std::fmt::Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// True when the revision `holder` comes after the revision `mine`. With
/// no revision on one side, neither comes first.
fn newer(holder: Option<Revision>, mine: Option<Revision>) -> bool {
    matches!((holder, mine), (Some(holder), Some(mine)) if holder > mine)
}

/// The JSON of the lease object.
#[derive(Serialize, Deserialize)]
struct Holder {
    id: String,
    /// The time of the last write of the holder, in milliseconds since
    /// the Unix epoch. A lease of an older build has none.
    #[serde(default)]
    renewed_at_ms: u64,
    /// True when the holder ended the lease at its shutdown.
    #[serde(default)]
    ended: bool,
    /// The revision of the holder. A tool, an instance outside Cloud Run
    /// and an older build write none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<Revision>,
}

fn parse(bytes: &[u8]) -> Result<Holder, StoreError> {
    serde_json::from_slice(bytes).map_err(|e| StoreError::Failed(format!("{LEASE}: {e}")))
}

/// The time now, in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |since| since.as_millis() as u64)
}

/// The instance of a live lease. See [`holder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// The ID of the instance. The instance logs it when it takes the
    /// lease.
    pub id: String,
    /// The time of its last write of the lease, in milliseconds since
    /// the Unix epoch. 0 for a lease of an older build, with no time.
    pub renewed_at_ms: u64,
    /// The revision of the instance, when it has one.
    pub revision: Option<Revision>,
}

/// The instance that holds the lease of `store` at the time `now_ms`.
/// `None` when the store has no lease, or when the lease ended: its
/// holder ended it, or its time is [`Timing::ends_after`] old
/// (01M3X342DH98YEZ3X5CND43DGD). A lease with no time is live. See "A
/// live lease" in the module docs.
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_state::store::StoreError> {
/// use std::sync::Arc;
/// use riff_server::lease::{Lease, Timing, holder, now_ms};
/// use riff_state::store::Memory;
///
/// let store = Arc::new(Memory::default());
/// let timing = Timing::default();
/// assert_eq!(holder(&*store, now_ms(), &timing).await?, None);
///
/// let lease = Lease::take(store.clone()).await?;
/// let held = holder(&*store, now_ms(), &timing).await?.unwrap();
/// assert_eq!(held.id, lease.id());
/// // 90 seconds with no new time: the lease ended.
/// let later = held.renewed_at_ms + 90_000;
/// assert_eq!(holder(&*store, later, &timing).await?, None);
///
/// // A shutdown ends the lease at once.
/// lease.end().await?;
/// assert_eq!(holder(&*store, now_ms(), &timing).await?, None);
/// # Ok(()) }
/// ```
pub async fn holder(
    store: &dyn Store,
    now_ms: u64,
    timing: &Timing,
) -> Result<Option<Held>, StoreError> {
    Ok(Found::read(store, now_ms, timing).await?.held)
}

/// The lease as a tool reads it.
struct Found {
    /// The version of the lease object, or `None` with no object.
    version: Option<Version>,
    /// The instance of a live lease.
    held: Option<Held>,
    /// True when the lease ended only by its age: its holder did not
    /// end it.
    by_age: bool,
}

impl Found {
    async fn read(store: &dyn Store, now_ms: u64, timing: &Timing) -> Result<Found, StoreError> {
        let Some(lease) = store.load(LEASE).await? else {
            return Ok(Found {
                version: None,
                held: None,
                by_age: false,
            });
        };
        let holder = parse(&lease.bytes)?;
        let by_age = !holder.ended && !holder.fresh(now_ms, timing);
        Ok(Found {
            version: Some(lease.version),
            held: holder.live(now_ms, timing),
            by_age,
        })
    }
}

impl Holder {
    /// True while the time of the lease is less than
    /// [`Timing::ends_after`] old. A lease with no time never ends by
    /// its age: an instance of an older build can still write the store.
    fn fresh(&self, now_ms: u64, timing: &Timing) -> bool {
        let age = u128::from(now_ms.saturating_sub(self.renewed_at_ms));
        self.renewed_at_ms == 0 || age < timing.ends_after.as_millis()
    }

    /// The holder of a live lease: not ended, and fresh.
    fn live(self, now_ms: u64, timing: &Timing) -> Option<Held> {
        (!self.ended && self.fresh(now_ms, timing)).then_some(Held {
            id: self.id,
            renewed_at_ms: self.renewed_at_ms,
            revision: self.revision,
        })
    }
}

/// A new random ID of an instance: 32 hex digits. The ID of a tool has
/// the name of the tool and a `-` in front ([`ToolLease::take`]).
fn new_id() -> Result<String, StoreError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| StoreError::Failed(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The bytes of the lease of the holder `id` of the revision
/// `revision`, with the time now.
fn json(id: &str, revision: Option<Revision>, ended: bool) -> Result<Vec<u8>, StoreError> {
    let holder = Holder {
        id: id.to_owned(),
        renewed_at_ms: now_ms(),
        ended,
        revision,
    };
    serde_json::to_vec(&holder).map_err(|e| StoreError::Failed(e.to_string()))
}

/// True when `id` is the ID of an instance ([`new_id`]), not of a tool.
fn of_instance(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// True when the lease of `store` holds `id`.
async fn holds(store: &dyn Store, id: &str) -> Result<bool, StoreError> {
    let Some(lease) = store.load(LEASE).await? else {
        return Ok(false);
    };
    Ok(parse(&lease.bytes)?.id == id)
}

/// Writes the lease of the holder `id` of the revision `revision` again,
/// when the lease holds `id`. Each write names the version that it read,
/// so it never writes over the lease of another holder. False when the
/// lease holds another ID.
async fn write(
    store: &dyn Store,
    id: &str,
    revision: Option<Revision>,
    ended: bool,
) -> Result<bool, StoreError> {
    loop {
        let Some(lease) = store.load(LEASE).await? else {
            return Ok(false);
        };
        if parse(&lease.bytes)?.id != id {
            return Ok(false);
        }
        let known = Some(lease.version);
        match store.save(LEASE, json(id, revision, ended)?, known).await {
            Ok(_) => return Ok(true),
            // Another holder wrote the lease since the load. Read it
            // again.
            Err(StoreError::Conflict(_)) => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Why a tool does not get the lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Busy {
    /// The lease is live: this instance holds it.
    Held(Held),
    /// An instance wrote the lease while the tool took it.
    Taken,
}

/// The lease of a tool that changes the log, for example
/// `riff-server log cut --yes` (01M3X5TP9CD4NXGPJEGP2Q4RS0). See "A tool
/// takes the lease" in the module docs.
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_state::store::StoreError> {
/// use std::sync::Arc;
/// use riff_server::lease::{Busy, Lease, Timing, ToolLease, holder, now_ms};
/// use riff_state::store::Memory;
///
/// let store = Arc::new(Memory::default());
/// let timing = Timing::default();
/// // A live lease: the tool does not get it.
/// let server = Lease::take(store.clone()).await?;
/// let busy = ToolLease::take(&*store, "cut", &timing).await?.err();
/// assert!(matches!(busy, Some(Busy::Held(held)) if held.id == server.id()));
///
/// // The server shuts down. The tool takes the lease: it is live, with
/// // the ID of the tool.
/// server.end().await?;
/// let tool = ToolLease::take(&*store, "cut", &timing).await?.unwrap();
/// let held = holder(&*store, now_ms(), &timing).await?.unwrap();
/// assert!(held.id.starts_with("cut-"));
/// // The server finds another ID.
/// assert!(!server.held().await?);
///
/// assert!(tool.end().await?);
/// assert_eq!(holder(&*store, now_ms(), &timing).await?, None);
/// # Ok(()) }
/// ```
pub struct ToolLease<'a> {
    store: &'a dyn Store,
    id: String,
}

impl<'a> ToolLease<'a> {
    /// Takes the lease for the tool `name`, when no instance holds it.
    /// The ID of the tool starts with `name`.
    ///
    /// - A live lease gives [`Busy::Held`].
    /// - The write names the version of the lease that the tool read.
    ///   When an instance wrote the lease in between, the tool gets
    ///   [`Busy::Taken`] and changes nothing.
    /// - After a lease that ended by its age, its instance can still
    ///   run. The tool waits [`Timing::settle`], then reads the lease
    ///   again: an instance that read the ID of the tool stopped, and
    ///   an instance that took the lease gives [`Busy::Taken`].
    pub async fn take(
        store: &'a dyn Store,
        name: &str,
        timing: &Timing,
    ) -> Result<Result<ToolLease<'a>, Busy>, StoreError> {
        let found = Found::read(store, now_ms(), timing).await?;
        if let Some(held) = found.held {
            return Ok(Err(Busy::Held(held)));
        }
        let id = format!("{name}-{}", new_id()?);
        match store.save(LEASE, json(&id, None, false)?, found.version).await {
            Ok(_) => {}
            Err(StoreError::Conflict(_)) => return Ok(Err(Busy::Taken)),
            Err(error) => return Err(error),
        }
        if found.by_age {
            tokio::time::sleep(timing.settle()).await;
            if !holds(store, &id).await? {
                return Ok(Err(Busy::Taken));
            }
        }
        Ok(Ok(ToolLease { store, id }))
    }

    /// Ends the lease of the tool. False when the lease holds another
    /// ID: an instance took it while the tool ran.
    pub async fn end(self) -> Result<bool, StoreError> {
        write(self.store, &self.id, None, true).await
    }

    /// The ID of the tool.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// The lease of one instance.
///
/// ```
/// use std::sync::Arc;
/// use futures::executor::block_on;
/// use riff_server::lease::Lease;
/// use riff_state::store::Memory;
///
/// let store = Arc::new(Memory::default());
/// block_on(async {
///     let old = Lease::take(store.clone()).await?;
///     assert!(old.held().await?);
///     let new = Lease::take(store).await?;
///     assert!(new.held().await?);
///     assert!(!old.held().await?);
///     // Only the holder renews the lease.
///     assert!(new.renew().await?);
///     assert!(!old.renew().await?);
///     Ok::<(), riff_state::store::StoreError>(())
/// })?;
/// # Ok::<(), riff_state::store::StoreError>(())
/// ```
pub struct Lease {
    store: Arc<dyn Store>,
    id: String,
    /// The revision of this instance.
    revision: Option<Revision>,
    /// True once this instance ended the lease.
    ended: AtomicBool,
}

impl Lease {
    /// Writes a new random ID to the lease, over the ID of any other
    /// instance (R138). The lease holds no revision: see
    /// [`Lease::take_as`].
    pub async fn take(store: Arc<dyn Store>) -> Result<Lease, StoreError> {
        let lease = Lease::new(store, None)?;
        // With no revision, no holder comes first.
        lease.write_unless_newer(&Timing::default()).await?;
        Ok(lease)
    }

    /// Writes a new random ID of an instance of the revision `revision`
    /// to the lease (R138), unless a live lease holds a newer revision
    /// (01M53NHJXGK1PN2A9F7NSX0B4G). Then it writes nothing, and gives
    /// that holder. A lease that ended, by its holder or by its age, and
    /// a lease with no revision, are no holder that comes first.
    ///
    /// ```
    /// # #[tokio::main] async fn main() -> Result<(), riff_state::store::StoreError> {
    /// use std::sync::Arc;
    /// use riff_server::lease::{Lease, Revision, Timing};
    /// use riff_state::store::Memory;
    ///
    /// let store = Arc::new(Memory::default());
    /// let timing = Timing::default();
    /// let old = Revision::parse("riff-server-00025-2s5");
    /// let new = Revision::parse("riff-server-00026-899");
    ///
    /// let first = Lease::take_as(store.clone(), old, &timing).await?.unwrap();
    /// // A deploy: the new revision takes the lease.
    /// let deploy = Lease::take_as(store.clone(), new, &timing).await?.unwrap();
    /// assert!(!first.held().await?);
    /// // A second instance of the old revision does not take it.
    /// let Err(held) = Lease::take_as(store.clone(), old, &timing).await? else {
    ///     panic!("the old revision took the lease");
    /// };
    /// assert_eq!((held.id.as_str(), held.revision), (deploy.id(), new));
    /// assert!(deploy.held().await?);
    /// // A second instance of the same revision takes it: the newest serves.
    /// let again = Lease::take_as(store.clone(), new, &timing).await?.unwrap();
    /// assert!(!deploy.held().await?);
    ///
    /// // After the end of the lease, the old revision takes it.
    /// again.end().await?;
    /// let back = Lease::take_as(store.clone(), old, &timing).await?.unwrap();
    /// assert!(back.held().await?);
    /// # Ok(()) }
    /// ```
    pub async fn take_as(
        store: Arc<dyn Store>,
        revision: Option<Revision>,
        timing: &Timing,
    ) -> Result<Result<Lease, Held>, StoreError> {
        let lease = Lease::new(store, revision)?;
        Ok(match lease.write_unless_newer(timing).await? {
            None => Ok(lease),
            Some(newer) => Err(newer),
        })
    }

    fn new(store: Arc<dyn Store>, revision: Option<Revision>) -> Result<Lease, StoreError> {
        Ok(Lease {
            store,
            id: new_id()?,
            revision,
            ended: AtomicBool::new(false),
        })
    }

    /// Writes the ID of this instance to the lease, over the ID of each
    /// other holder, unless a live lease holds a newer revision. Gives
    /// that holder, and then writes nothing.
    async fn write_unless_newer(&self, timing: &Timing) -> Result<Option<Held>, StoreError> {
        loop {
            let found = self.store.load(LEASE).await?;
            let known = found.as_ref().map(|lease| lease.version);
            // A lease that does not read has no holder that comes first:
            // write over it.
            let newer = found
                .and_then(|lease| parse(&lease.bytes).ok())
                .and_then(|holder| holder.live(now_ms(), timing))
                .filter(|held| newer(held.revision, self.revision));
            if newer.is_some() {
                return Ok(newer);
            }
            let bytes = json(&self.id, self.revision, false)?;
            match self.store.save(LEASE, bytes, known).await {
                Ok(_) => return Ok(None),
                // Another instance wrote the lease since the load. Read it
                // again: write over it, unless it comes first.
                Err(StoreError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// True when the lease holds the ID of this instance.
    pub async fn held(&self) -> Result<bool, StoreError> {
        holds(&*self.store, &self.id).await
    }

    /// True when the lease holds the ID of another instance, and that
    /// instance did not end it: a deploy took the lease. False for the
    /// ID of a tool, which can cut the log (01M51X8MG0YQT5KBZC9H7917J3).
    ///
    /// ```
    /// # #[tokio::main] async fn main() -> Result<(), riff_state::store::StoreError> {
    /// use std::sync::Arc;
    /// use riff_server::lease::{Lease, Timing, ToolLease};
    /// use riff_state::store::Memory;
    ///
    /// let store = Arc::new(Memory::default());
    /// let old = Lease::take(store.clone()).await?;
    /// assert!(!old.taken_by_instance().await?, "the lease holds its own ID");
    /// let new = Lease::take(store.clone()).await?;
    /// assert!(old.taken_by_instance().await?);
    ///
    /// // The new instance shuts down, and a tool takes the lease.
    /// new.end().await?;
    /// assert!(!old.taken_by_instance().await?);
    /// let _cut = ToolLease::take(&*store, "cut", &Timing::default()).await?.unwrap();
    /// assert!(!old.taken_by_instance().await?);
    /// # Ok(()) }
    /// ```
    pub async fn taken_by_instance(&self) -> Result<bool, StoreError> {
        let Some(lease) = self.store.load(LEASE).await? else {
            return Ok(false);
        };
        let holder = parse(&lease.bytes)?;
        Ok(holder.id != self.id && !holder.ended && of_instance(&holder.id))
    }

    /// Writes the time now to the lease, when the lease holds the ID of
    /// this instance (01M3X34282SG0DJ6X34F90HS26). True when it holds
    /// that ID, as [`Lease::held`]. After [`Lease::end`], it only reads.
    pub async fn renew(&self) -> Result<bool, StoreError> {
        if self.ended.load(Ordering::SeqCst) {
            return self.held().await;
        }
        write(&*self.store, &self.id, self.revision, false).await
    }

    /// Ends the lease, when it holds the ID of this instance
    /// (01M3X342ARX5Y7R9ZJDT12R9A1). Call it only when the instance
    /// writes nothing more.
    pub async fn end(&self) -> Result<(), StoreError> {
        self.ended.store(true, Ordering::SeqCst);
        write(&*self.store, &self.id, self.revision, true)
            .await
            .map(|_| ())
    }

    /// The ID of this instance.
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::future::{BoxFuture, FutureExt};
    use riff_state::store::{Loaded, Memory, Version};

    fn lease(store: &Memory, id: &str) -> Lease {
        Lease {
            store: Arc::new(store.clone()),
            id: id.into(),
            revision: None,
            ended: AtomicBool::new(false),
        }
    }

    #[test]
    fn each_lease_has_a_new_id() {
        let store = Arc::new(Memory::default());
        block_on(async {
            let a = Lease::take(store.clone()).await.unwrap();
            let b = Lease::take(store.clone()).await.unwrap();
            assert_ne!(a.id(), b.id());
            assert_eq!(a.id().len(), 32);
        });
    }

    /// A memory store in which another holder takes the lease after the
    /// next load of the lease: between the load and the save of a
    /// renewal. It writes the lease `other`.
    struct Taken {
        store: Memory,
        armed: AtomicBool,
        other: Vec<u8>,
    }

    impl Taken {
        fn new(store: &Memory, other: &[u8]) -> Taken {
            Taken {
                store: store.clone(),
                armed: AtomicBool::new(false),
                other: other.to_vec(),
            }
        }
    }

    impl Store for Taken {
        fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
            async move {
                let loaded = self.store.load(name).await?;
                if name == LEASE && self.armed.swap(false, Ordering::SeqCst) {
                    let known = loaded.as_ref().map(|lease| lease.version);
                    self.store.save(LEASE, self.other.clone(), known).await?;
                }
                Ok(loaded)
            }
            .boxed()
        }

        fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
            self.store.list(prefix)
        }

        fn save<'a>(
            &'a self,
            name: &'a str,
            bytes: Vec<u8>,
            known: Option<Version>,
        ) -> BoxFuture<'a, Result<Version, StoreError>> {
            self.store.save(name, bytes, known)
        }

        fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
            self.store.delete(name)
        }
    }

    /// 01M3X34282SG0DJ6X34F90HS26: the write of the time names the
    /// version of the lease that the instance read. So an instance never
    /// writes over the lease of another holder: the write fails, and
    /// the instance finds the other ID.
    #[test]
    fn a_renewal_and_an_end_never_write_over_the_lease_of_another_holder() {
        let memory = Memory::default();
        let store = Arc::new(Taken::new(&memory, br#"{"id":"other","renewed_at_ms":1}"#));
        block_on(async {
            for end in [false, true] {
                let lease = Lease::take(store.clone()).await.unwrap();
                assert!(lease.renew().await.unwrap());
                // Another holder takes the lease after the load of the
                // next write.
                store.armed.store(true, Ordering::SeqCst);
                if end {
                    lease.end().await.unwrap();
                } else {
                    assert!(!lease.renew().await.unwrap(), "the lease has another ID");
                }
                let now = memory.load(LEASE).await.unwrap().unwrap();
                assert_eq!(now.bytes, br#"{"id":"other","renewed_at_ms":1}"#);
                assert!(!lease.held().await.unwrap());
            }
        });
    }

    fn revision(number: u64) -> Option<Revision> {
        Some(Revision(number))
    }

    /// The bytes of a live lease of the instance `id` of the revision
    /// `number`.
    fn live(id: &str, number: u64) -> Vec<u8> {
        format!(r#"{{"id":"{id}","renewed_at_ms":{},"revision":{number}}}"#, now_ms()).into_bytes()
    }

    /// 01M53NHJXGK1PN2A9F7NSX0B4G: a take names the version of the lease
    /// that it read. When an instance of a newer revision writes the
    /// lease between the read and the write, the take reads it again,
    /// and does not write over it.
    #[test]
    fn a_take_reads_again_when_a_newer_revision_wrote_the_lease_in_between() {
        let memory = Memory::default();
        let newer = live("0123456789abcdef0123456789abcdef", 26);
        let store = Arc::new(Taken::new(&memory, &newer));
        let timing = Timing::default();
        block_on(async {
            store.armed.store(true, Ordering::SeqCst);
            let taken = Lease::take_as(store.clone(), revision(25), &timing).await;
            let Err(held) = taken.unwrap() else {
                panic!("it wrote over the newer revision");
            };
            assert_eq!(held.revision, revision(26));
            assert_eq!(memory.load(LEASE).await.unwrap().unwrap().bytes, newer);

            // The same with no revision: the take writes over it.
            store.armed.store(true, Ordering::SeqCst);
            let lease = Lease::take(store.clone()).await.unwrap();
            assert!(lease.held().await.unwrap());
        });
    }

    /// 01M53NHJXGK1PN2A9F7NSX0B4G: only a live lease of a newer revision
    /// comes first. With no revision on one side, and after a lease of a
    /// newer revision ended by its age, the take writes over it. So is a
    /// lease that does not read, as before the revision.
    #[test]
    fn only_a_live_lease_of_a_newer_revision_comes_first() {
        let store = Memory::default();
        let shared: Arc<dyn Store> = Arc::new(store.clone());
        let timing = Timing::default();
        let take = |mine| Lease::take_as(shared.clone(), mine, &timing);
        block_on(async {
            let newer = live("n", 26);
            store.save(LEASE, newer, None).await.unwrap();
            assert!(take(revision(25)).await.unwrap().is_err());
            assert!(take(None).await.unwrap().is_ok(), "no revision of mine");

            for lease in [
                format!(r#"{{"id":"cut-a","renewed_at_ms":{}}}"#, now_ms()).into_bytes(),
                format!(r#"{{"id":"n","renewed_at_ms":{},"revision":26}}"#, now_ms() - 95_000)
                    .into_bytes(),
                br#"{"id":"n","renewed_at_ms":1,"ended":true,"revision":26}"#.to_vec(),
                b"x".to_vec(),
            ] {
                let version = store.load(LEASE).await.unwrap().map(|l| l.version);
                store.save(LEASE, lease, version).await.unwrap();
                let mine = take(revision(25)).await.unwrap().unwrap();
                assert!(mine.held().await.unwrap());
            }
        });
    }

    /// The lease keeps the revision in each renewal and at its end. A
    /// field that this build does not know reads: so an older build reads
    /// the lease of this one at a rollback.
    #[test]
    fn a_renewal_and_an_end_keep_the_revision() {
        let store = Arc::new(Memory::default());
        let timing = Timing::default();
        let revision_of = |bytes: &[u8]| parse(bytes).unwrap().revision;
        block_on(async {
            let lease = Lease::take_as(store.clone(), revision(26), &timing)
                .await
                .unwrap()
                .unwrap();
            let bytes = store.load(LEASE).await.unwrap().unwrap().bytes;
            assert_eq!(revision_of(&bytes), revision(26));
            assert!(lease.renew().await.unwrap());
            let bytes = store.load(LEASE).await.unwrap().unwrap().bytes;
            assert_eq!(revision_of(&bytes), revision(26));
            lease.end().await.unwrap();
            let bytes = store.load(LEASE).await.unwrap().unwrap().bytes;
            assert_eq!(revision_of(&bytes), revision(26));
            // No revision: no field.
            let none = Lease::take(store.clone()).await.unwrap();
            let bytes = store.load(LEASE).await.unwrap().unwrap().bytes;
            assert!(!String::from_utf8(bytes).unwrap().contains("revision"));
            assert!(none.held().await.unwrap());
        });
        let later = br#"{"id":"a","renewed_at_ms":1,"revision":27,"zone":"b"}"#;
        assert_eq!(revision_of(later), revision(27));
    }

    #[test]
    fn no_lease_object_is_not_held() {
        let lease = lease(&Memory::default(), "x");
        assert!(!block_on(lease.held()).unwrap());
        assert!(!block_on(lease.renew()).unwrap());
    }

    #[test]
    fn a_lease_that_is_not_json_is_an_error() {
        let store = Memory::default();
        block_on(store.save(LEASE, b"x".to_vec(), None)).unwrap();
        assert!(block_on(lease(&store, "x").held()).is_err());
        assert!(block_on(holder(&store, 0, &Timing::default())).is_err());
    }

    #[test]
    fn the_default_times_follow_the_requirements() {
        let t = Timing::default();
        assert_eq!(t.wait, Duration::from_secs(15));
        assert_eq!(t.read_every, Duration::from_secs(2));
        assert_eq!(t.valid_for, Duration::from_secs(5));
        assert_eq!(t.exit_after, Duration::from_secs(10));
        // The argument in the module docs.
        let skew = Duration::from_secs(riff_core::dpop::MAX_SKEW);
        assert!(t.valid_for + skew <= t.wait);
        assert_eq!(t.renew_every, Duration::from_secs(30));
        assert_eq!(t.ends_after, Duration::from_secs(90));
        // The argument in "A live lease" of the module docs.
        let attempt = riff_state::log::Timing::default().attempt;
        let writes = t.renew_every + t.read_every + t.valid_for + attempt;
        assert_eq!(writes, Duration::from_secs(40));
        // The difference of the two clocks that the rule allows.
        assert_eq!(t.ends_after - writes, Duration::from_secs(50));
    }

    #[test]
    fn a_renewal_writes_a_new_time_and_an_end_stays() {
        let store = Arc::new(Memory::default());
        let timing = Timing::default();
        block_on(async {
            let lease = Lease::take(store.clone()).await.unwrap();
            let first = store.load(LEASE).await.unwrap().unwrap().version;
            assert!(lease.renew().await.unwrap());
            let second = store.load(LEASE).await.unwrap().unwrap().version;
            assert_ne!(first, second, "a renewal writes the lease");
            assert!(holder(&*store, now_ms(), &timing).await.unwrap().is_some());

            lease.end().await.unwrap();
            assert_eq!(holder(&*store, now_ms(), &timing).await.unwrap(), None);
            // A renewal after the end only reads: the lease stays ended.
            assert!(lease.renew().await.unwrap());
            assert_eq!(holder(&*store, now_ms(), &timing).await.unwrap(), None);
        });
    }

    #[test]
    fn an_instance_ends_only_its_own_lease() {
        let store = Arc::new(Memory::default());
        block_on(async {
            let old = Lease::take(store.clone()).await.unwrap();
            let new = Lease::take(store.clone()).await.unwrap();
            old.end().await.unwrap();
            let held = holder(&*store, now_ms(), &Timing::default()).await;
            assert_eq!(held.unwrap().unwrap().id, new.id());
        });
    }

    #[test]
    fn a_lease_with_no_time_and_a_time_in_the_future_are_live() {
        let store = Memory::default();
        let timing = Timing::default();
        block_on(async {
            // The lease of an older build: its instance can still run.
            let old = br#"{"id":"old"}"#.to_vec();
            let version = store.save(LEASE, old, None).await.unwrap();
            let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
            assert_eq!((held.id.as_str(), held.renewed_at_ms), ("old", 0));
            // The old lease still reads for an instance.
            assert!(lease(&store, "old").held().await.unwrap());

            let ahead = format!(r#"{{"id":"a","renewed_at_ms":{}}}"#, now_ms() + 5_000);
            let ahead = ahead.into_bytes();
            store.save(LEASE, ahead, Some(version)).await.unwrap();
            let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
            assert_eq!(held.id, "a");
        });
    }
}
