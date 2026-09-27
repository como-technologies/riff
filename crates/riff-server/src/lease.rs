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
//!     N->>S: write the lease (new ID)
//!     Note over N: waits 15 s
//!     O->>S: read the lease (every 2 s)
//!     S-->>O: new ID
//!     Note over O: stops for good: 503, saves nothing, exits after 60 s
//!     N->>S: load the state
//!     Note over N: serves from the next whole second
//! ```
//!
//! # Rules
//!
//! - At start, an instance writes a new random ID to the lease with
//!   [`Lease::take`]. It waits [`Timing::wait`], loads the state, and
//!   starts to serve (R138).
//! - It reads the lease each [`Timing::read_every`]. It serves only for
//!   [`Timing::valid_for`] after the start of the last read that showed
//!   its own ID. Else it replies 503 (R139).
//! - When a read shows another ID, the instance stops for good (R140).
//!   A save that finds another version does the same (R141).
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

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::store::{LEASE, Store, StoreError};

/// The times of the lease rules. Tests use short times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The wait after the lease write, before the load (R138).
    pub wait: Duration,
    /// The time between two reads of the lease (R139).
    pub read_every: Duration,
    /// How long a read that shows the own ID lets the instance serve
    /// (R139).
    pub valid_for: Duration,
    /// How long a stopped instance still replies 503 before it exits
    /// (R140).
    pub exit_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            wait: Duration::from_secs(15),
            read_every: Duration::from_secs(2),
            valid_for: Duration::from_secs(5),
            exit_after: Duration::from_secs(60),
        }
    }
}

/// The JSON of the lease object.
#[derive(Serialize, Deserialize)]
struct Holder {
    id: String,
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
///     Ok::<(), riff_server::store::StoreError>(())
/// })?;
/// # Ok::<(), riff_server::store::StoreError>(())
/// ```
pub struct Lease {
    store: Arc<dyn Store>,
    id: String,
}

impl Lease {
    /// Writes a new random ID to the lease, over the ID of any other
    /// instance (R138).
    pub async fn take(store: Arc<dyn Store>) -> Result<Lease, StoreError> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| StoreError::Failed(e.to_string()))?;
        let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let json = serde_json::to_vec(&Holder { id: id.clone() })
            .map_err(|e| StoreError::Failed(e.to_string()))?;
        loop {
            let known = store.load(LEASE).await?.map(|lease| lease.version);
            match store.save(LEASE, json.clone(), known).await {
                Ok(_) => return Ok(Lease { store, id }),
                // Another instance wrote the lease since the load. Write
                // over it: the newest instance serves.
                Err(StoreError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// True when the lease holds the ID of this instance.
    pub async fn held(&self) -> Result<bool, StoreError> {
        let Some(lease) = self.store.load(LEASE).await? else {
            return Ok(false);
        };
        let holder: Holder = serde_json::from_slice(&lease.bytes)
            .map_err(|e| StoreError::Failed(format!("{LEASE}: {e}")))?;
        Ok(holder.id == self.id)
    }

    /// The ID of this instance.
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Memory;
    use futures::executor::block_on;

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

    #[test]
    fn no_lease_object_is_not_held() {
        let store = Memory::default();
        let lease = Lease {
            store: Arc::new(store),
            id: "x".into(),
        };
        assert!(!block_on(lease.held()).unwrap());
    }

    #[test]
    fn a_lease_that_is_not_json_is_an_error() {
        let store = Memory::default();
        block_on(store.save(LEASE, b"x".to_vec(), None)).unwrap();
        let lease = Lease {
            store: Arc::new(store),
            id: "x".into(),
        };
        assert!(block_on(lease.held()).is_err());
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
    }
}
