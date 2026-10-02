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
//!     Note over N: a load that fails: exit, no lease
//!     N->>S: write the lease (new ID)
//!     Note over N: waits 15 s
//!     O->>S: read the lease (every 2 s)
//!     S-->>O: new ID
//!     Note over O: stops for good: 503, saves nothing, exits after 60 s
//!     N->>S: read the chunks that came since the load
//!     Note over N: opens its port, serves from the next whole second
//! ```
//!
//! # Rules
//!
//! - At start, an instance loads the state first
//!   (01M3THEE08ZKV8WGHDSVWV69ZE). Then it writes a new random ID to the
//!   lease with [`Lease::take`]. It waits [`Timing::wait`], applies the
//!   chunks that came since the load, and starts to serve (R138). See
//!   [`crate::Service::load`].
//! - It reads the lease each [`Timing::read_every`]. It serves only for
//!   [`Timing::valid_for`] after the start of the last read that showed
//!   its own ID. Else it replies 503 (R139).
//! - When a read shows another ID, the instance stops for good (R140).
//!   A save that finds another version does the same (R141).
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
//! ([`crate::log::Timing::attempt`]). That is 40 seconds. The other 50
//! seconds of [`Timing::ends_after`] are for the difference between the
//! clock of the instance and the clock of the tool.
//!
//! The tool compares the time in the lease with its own clock. So the
//! clock of the tool must be less than 50 seconds ahead of the clock of
//! the instance.
//!
//! A lease of an older build has no time. The tool cannot know that
//! its instance stopped, so the lease is live until a person deletes
//! the object. An instance that still runs then stops for good (R140).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::store::{LEASE, Store, StoreError};

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
    /// How long a stopped instance still replies 503 before it exits
    /// (R140).
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
            exit_after: Duration::from_secs(60),
            renew_every: Duration::from_secs(30),
            ends_after: Duration::from_secs(90),
        }
    }
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
}

/// The instance that holds the lease of `store` at the time `now_ms`.
/// `None` when the store has no lease, or when the lease ended: its
/// holder ended it, or its time is [`Timing::ends_after`] old
/// (01M3X342DH98YEZ3X5CND43DGD). A lease with no time is live. See "A
/// live lease" in the module docs.
///
/// ```
/// # #[tokio::main] async fn main() -> Result<(), riff_server::store::StoreError> {
/// use std::sync::Arc;
/// use riff_server::lease::{Lease, Timing, holder, now_ms};
/// use riff_server::store::Memory;
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
    let Some(lease) = store.load(LEASE).await? else {
        return Ok(None);
    };
    let holder = parse(&lease.bytes)?;
    let age = u128::from(now_ms.saturating_sub(holder.renewed_at_ms));
    // A lease with no time never ends by its age: an instance of an
    // older build can still write the store.
    let fresh = holder.renewed_at_ms == 0 || age < timing.ends_after.as_millis();
    let live = !holder.ended && fresh;
    Ok(live.then_some(Held {
        id: holder.id,
        renewed_at_ms: holder.renewed_at_ms,
    }))
}

/// The lease of one instance.
///
/// ```
/// use std::sync::Arc;
/// use futures::executor::block_on;
/// use riff_server::lease::Lease;
/// use riff_server::store::Memory;
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
///     Ok::<(), riff_server::store::StoreError>(())
/// })?;
/// # Ok::<(), riff_server::store::StoreError>(())
/// ```
pub struct Lease {
    store: Arc<dyn Store>,
    id: String,
    /// True once this instance ended the lease.
    ended: AtomicBool,
}

impl Lease {
    /// Writes a new random ID to the lease, over the ID of any other
    /// instance (R138).
    pub async fn take(store: Arc<dyn Store>) -> Result<Lease, StoreError> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| StoreError::Failed(e.to_string()))?;
        let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let lease = Lease {
            store,
            id,
            ended: AtomicBool::new(false),
        };
        loop {
            let known = lease.store.load(LEASE).await?.map(|lease| lease.version);
            match lease.store.save(LEASE, lease.json(false)?, known).await {
                Ok(_) => return Ok(lease),
                // Another instance wrote the lease since the load. Write
                // over it: the newest instance serves.
                Err(StoreError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// The bytes of the lease of this instance, with the time now.
    fn json(&self, ended: bool) -> Result<Vec<u8>, StoreError> {
        let holder = Holder {
            id: self.id.clone(),
            renewed_at_ms: now_ms(),
            ended,
        };
        serde_json::to_vec(&holder).map_err(|e| StoreError::Failed(e.to_string()))
    }

    /// True when the lease holds the ID of this instance.
    pub async fn held(&self) -> Result<bool, StoreError> {
        let Some(lease) = self.store.load(LEASE).await? else {
            return Ok(false);
        };
        Ok(parse(&lease.bytes)?.id == self.id)
    }

    /// Writes the lease of this instance again, when the lease holds its
    /// ID. False when the lease holds another ID.
    async fn write(&self, ended: bool) -> Result<bool, StoreError> {
        loop {
            let Some(lease) = self.store.load(LEASE).await? else {
                return Ok(false);
            };
            if parse(&lease.bytes)?.id != self.id {
                return Ok(false);
            }
            let known = Some(lease.version);
            match self.store.save(LEASE, self.json(ended)?, known).await {
                Ok(_) => return Ok(true),
                // Another instance wrote the lease since the load. Read
                // it again.
                Err(StoreError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// Writes the time now to the lease, when the lease holds the ID of
    /// this instance (01M3X34282SG0DJ6X34F90HS26). True when it holds
    /// that ID, as [`Lease::held`]. After [`Lease::end`], it only reads.
    pub async fn renew(&self) -> Result<bool, StoreError> {
        if self.ended.load(Ordering::SeqCst) {
            return self.held().await;
        }
        self.write(false).await
    }

    /// Ends the lease, when it holds the ID of this instance
    /// (01M3X342ARX5Y7R9ZJDT12R9A1). Call it only when the instance
    /// writes nothing more.
    pub async fn end(&self) -> Result<(), StoreError> {
        self.ended.store(true, Ordering::SeqCst);
        self.write(true).await.map(|_| ())
    }

    /// The ID of this instance.
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Loaded, Memory, Version};
    use futures::executor::block_on;
    use futures::future::{BoxFuture, FutureExt};

    fn lease(store: &Memory, id: &str) -> Lease {
        Lease {
            store: Arc::new(store.clone()),
            id: id.into(),
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
    /// renewal.
    struct Taken {
        store: Memory,
        armed: AtomicBool,
    }

    impl Store for Taken {
        fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
            async move {
                let loaded = self.store.load(name).await?;
                if name == LEASE && self.armed.swap(false, Ordering::SeqCst) {
                    let known = loaded.as_ref().map(|lease| lease.version);
                    let other = br#"{"id":"other","renewed_at_ms":1}"#.to_vec();
                    self.store.save(LEASE, other, known).await?;
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
        let store = Arc::new(Taken {
            store: memory.clone(),
            armed: AtomicBool::new(false),
        });
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
        assert_eq!(t.exit_after, Duration::from_secs(60));
        // The argument in the module docs.
        let skew = Duration::from_secs(riff_core::dpop::MAX_SKEW);
        assert!(t.valid_for + skew <= t.wait);
        assert_eq!(t.renew_every, Duration::from_secs(30));
        assert_eq!(t.ends_after, Duration::from_secs(90));
        // The argument in "A live lease" of the module docs.
        let attempt = crate::log::Timing::default().attempt;
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
