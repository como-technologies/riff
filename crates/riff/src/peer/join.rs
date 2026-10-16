//! A join of a new daemon to a riff of peers over iroh
//! (01M519Z3YA9WBH9TWQPAH6A64Z, 01M521J6HX5BN0FNDBSKZDVQE7).
//!
//! # Design
//!
//! `riff join STRING` on the machine of a person: its daemon connects to
//! the daemon of the owner by the riff ID of the invite, with the ALPN
//! [`ALPN`]. It opens one stream, sends one [`Ask`] in JSON (the secret of
//! the invite and its host), and ends the stream. The daemon of the owner
//! answers with one [`Answer`] in JSON, and ends its stream. On
//! [`Answer::Joined`], the connection goes on as the peer link of the two
//! daemons ([`crate::peer::sync::Links::serve_joined`]): the new daemon
//! gets each record of the riff at once.
//!
//! The daemon of the owner checks a join in this order ([`admit`]). The
//! first check that fails refuses the join, and the refusal changes
//! nothing:
//!
//! 1. The daemon is the owner of its riff.
//! 2. The key of the new daemon, that iroh checked, is not the owner, not
//!    a member and not removed. The refusal names which of the three
//!    (01M5328FS8XGVW3X6N94F65FA8). A key that left may join again
//!    (01M5328FPJT7Q5Q1509BC5F0J7).
//! 3. The host is a name that riff takes.
//! 4. The invite of the secret is known, not used and not ended
//!    ([`Invites::take`]).
//!
//! Then it writes the `member` record: the key of the new daemon, the
//! user of the invite and the host. For a key that left, the record
//! also names the sequence number of its `left` record (`after`): the
//! records of the key after it count with the user of the new invite.
//! When the write fails, it gives the invite back.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant N as the new daemon
//!     participant O as the daemon of the owner
//!     N->>O: connect to the riff ID, ALPN riff/3/join
//!     N->>O: the secret and the host, then the end of the stream
//!     O->>O: admit: the owner, the key, the host, the invite
//!     O->>O: the member record
//!     O-->>N: joined, then the end of the stream
//!     Note over N,O: the connection goes on as the peer link
//! ```

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use iroh::Endpoint;
use iroh::endpoint::Connection;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::daemon_key::Public;
use crate::peer::body::{Body, Member};
use crate::peer::invite::{Invite, Invites, Secret};
use crate::peer::state::State;
use crate::peer::talk::Hub;
use crate::text;

/// The ALPN of a join: a connection of a key that is not in the riff
/// yet (01M519Z2WAGQEX1H4EGZXWBZT6).
pub const ALPN: &[u8] = b"riff/3/join";

/// The largest ask or answer, in bytes.
pub const LARGEST: usize = 4096;

/// The limit of the connect of a join, and of each step of it: the
/// DNS of n0 can take some seconds to find the daemon of the owner.
pub const LIMIT: Duration = Duration::from_secs(30);

/// What the new daemon sends.
#[derive(Serialize, Deserialize)]
pub struct Ask {
    /// The secret of the invite, in hex.
    pub secret: String,
    /// The host of the new daemon.
    pub host: String,
}

impl fmt::Debug for Ask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ask")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// What the daemon of the owner answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    /// The new key is a member: the connection goes on as the peer link.
    Joined,
    /// The daemon of the owner refused the join, and changed nothing.
    Refused {
        /// Why.
        why: String,
    },
}

/// The answer of the daemon of `hub` to a join of the key `key` with
/// `ask`, at the time `now_ms`. See the design of the module. It writes
/// a record, so it syncs a file: call it on a thread for blocking work.
pub fn admit(hub: &Hub, invites: &Invites, key: Public, ask: &Ask, now_ms: u64) -> Result<Answer> {
    let refused = |why: String| Ok(Answer::Refused { why });
    let me = hub.public();
    let (riff, known) = hub.read(|state| (state.riff(), known(state, &key)));
    if riff != me {
        return refused(text::JOIN_NOT_THE_OWNER.into());
    }
    let after = match known {
        Ok(after) => after,
        Err(why) => return refused(why),
    };
    if riff_core::name::check("host", &ask.host).is_err() {
        return refused(text::JOIN_BAD_HOST.into());
    }
    let Some(secret) = Secret::from_hex(&ask.secret) else {
        return refused(crate::peer::invite::Refused::Unknown.to_string());
    };
    let user = match invites.take(&secret, now_ms)? {
        Ok(user) => user,
        Err(why) => return refused(why.to_string()),
    };
    let member = Body::Member(Member {
        key,
        user,
        host: ask.host.clone(),
        after,
    });
    if let Err(e) = hub.write(&member) {
        invites.give_back(&secret)?;
        return Err(e);
    }
    Ok(Answer::Joined)
}

/// What the riff of `state` knows of the key `key` of a join: the
/// `after` of its new `member` record (`None` for a key that the riff
/// does not know, the number of its `left` record for a key that left),
/// or why the daemon of the owner refuses it (01M5328FS8XGVW3X6N94F65FA8).
fn known(state: &State, key: &Public) -> Result<Option<u64>, String> {
    let hex = key.to_string();
    if *key == state.riff() {
        return Err(text::join_owner_key(&hex));
    }
    let Some(member) = state.members().get(key) else {
        return Ok(None);
    };
    if member.removed {
        return Err(text::join_removed_key(&hex));
    }
    member
        .left_at()
        .map(Some)
        .ok_or_else(|| text::join_member_key(&hex))
}

/// The time now, in milliseconds since the epoch.
fn now_ms() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or_default()
}

/// The daemon of the owner answers the join of the connection `conn`
/// with the hub `hub` and the invites `invites`. It gives the answer that
/// it sent. On [`Answer::Joined`], the caller runs the peer link on
/// `conn`.
pub async fn answer(conn: &Connection, hub: &Arc<Hub>, invites: &Arc<Invites>) -> Result<Answer> {
    let (mut send, mut recv) = tokio::time::timeout(LIMIT, conn.accept_bi())
        .await
        .map_err(|_| anyhow!(text::peer_no_stream_in(LIMIT)))??;
    let bytes = tokio::time::timeout(LIMIT, recv.read_to_end(LARGEST))
        .await
        .map_err(|_| anyhow!(text::peer_no_stream_in(LIMIT)))??;
    let bytes = Zeroizing::new(bytes);
    let answer = match serde_json::from_slice::<Ask>(&bytes) {
        Ok(ask) => {
            let key = Public::from_bytes(*conn.remote_id().as_bytes());
            let (hub, invites) = (hub.clone(), invites.clone());
            tokio::task::spawn_blocking(move || admit(&hub, &invites, key, &ask, now_ms()))
                .await??
        }
        Err(_) => Answer::Refused {
            why: text::JOIN_BAD_ASK.into(),
        },
    };
    send.write_all(&serde_json::to_vec(&answer)?).await?;
    send.finish()?;
    // The answer reaches the new daemon before the link starts.
    let _ = tokio::time::timeout(LIMIT, send.stopped()).await;
    Ok(answer)
}

/// The new daemon on `endpoint` asks the daemon of the owner of the
/// riff of `invite` to join, from the host `host`. It gives the
/// connection, to run as the peer link, or why the join failed.
pub async fn ask(endpoint: &Endpoint, invite: &Invite, host: &str) -> Result<Connection> {
    let owner = iroh::PublicKey::from_bytes(invite.riff.as_bytes())
        .map_err(|_| anyhow!(text::PEER_NO_ENDPOINT_ID))?;
    let conn = tokio::time::timeout(LIMIT, endpoint.connect(owner, ALPN))
        .await
        .map_err(|_| anyhow!(text::join_no_owner(&text::peer_no_connect_in(LIMIT))))?
        .map_err(|e| anyhow!(text::join_no_owner(&e.to_string())))?;
    let (mut send, mut recv) = conn.open_bi().await?;
    let ask = Ask {
        secret: invite.secret().hex().to_string(),
        host: host.to_owned(),
    };
    let ask = Zeroizing::new(serde_json::to_vec(&ask)?);
    send.write_all(&ask).await?;
    send.finish()?;
    let bytes = tokio::time::timeout(LIMIT, recv.read_to_end(LARGEST))
        .await
        .map_err(|_| anyhow!(text::peer_no_stream_in(LIMIT)))?
        .context(text::JOIN_NO_ANSWER)?;
    match serde_json::from_slice(&bytes).context(text::JOIN_NO_ANSWER)? {
        Answer::Joined => Ok(conn),
        Answer::Refused { why } => bail!("{}", text::join_refused(&why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::Riff;
    use crate::peer::keep::Peers;

    /// The hub of the owner [1; 32], with its `riff` record and the
    /// member [2; 32].
    fn owner(dir: &std::path::Path) -> Hub {
        let key = Key::from_bytes(&[1; 32]);
        let peers = Peers::open(dir, key.public(), |_| {}).unwrap();
        let hub = Hub::new(key, peers);
        hub.write(&Body::Riff(Riff {
            user: "mike".into(),
            host: None,
        }))
        .unwrap();
        hub.write(&Body::Member(Member {
            key: Key::from_bytes(&[2; 32]).public(),
            user: "mike".into(),
            host: "pangolin".into(),
            after: None,
        }))
        .unwrap();
        hub
    }

    fn ask(secret: &Secret, host: &str) -> Ask {
        Ask {
            secret: secret.hex().to_string(),
            host: host.into(),
        }
    }

    /// 01M519Z3YA9WBH9TWQPAH6A64Z, 01M5328FS8XGVW3X6N94F65FA8: the checks
    /// of a join, in order. A refusal writes no record, keeps the invite,
    /// and names its reason: the owner or a member.
    #[test]
    fn a_join_writes_the_member_record_only_after_each_check() {
        let dir = tempfile::tempdir().unwrap();
        let hub = owner(dir.path());
        let invites = Invites::in_dir(dir.path());
        let invite = invites.make(hub.public(), "brett", 1_000).unwrap();
        let new = Key::from_bytes(&[3; 32]).public();
        let heads = || hub.with_peers(crate::peer::keep::Peers::heads);
        let before = heads();
        let refused = |key: Public, ask: &Ask| match admit(&hub, &invites, key, ask, 2_000).unwrap()
        {
            Answer::Refused { why } => why,
            Answer::Joined => panic!("joined"),
        };
        let good = ask(invite.secret(), "kadomony");
        assert_eq!(
            refused(hub.public(), &good),
            text::join_owner_key(&hub.public().to_string())
        );
        let member = Key::from_bytes(&[2; 32]).public();
        assert_eq!(
            refused(member, &good),
            text::join_member_key(&member.to_string())
        );
        assert_eq!(
            refused(new, &ask(invite.secret(), "a b")),
            text::JOIN_BAD_HOST
        );
        let wrong = Secret::from_bytes([8; 32]);
        assert_eq!(refused(new, &ask(&wrong, "kadomony")), text::INVITE_UNKNOWN);
        let not_hex = Ask {
            secret: "zz".into(),
            host: "kadomony".into(),
        };
        assert_eq!(refused(new, &not_hex), text::INVITE_UNKNOWN);
        assert_eq!(heads(), before);

        assert_eq!(
            admit(&hub, &invites, new, &good, 2_000).unwrap(),
            Answer::Joined
        );
        let (user, host) = hub.read(|state| {
            let member = &state.members()[&new];
            (member.user.clone(), member.host.clone())
        });
        assert_eq!((user.as_str(), host.as_str()), ("brett", "kadomony"));
        let other = Key::from_bytes(&[4; 32]).public();
        assert_eq!(refused(other, &good), text::INVITE_USED);
    }

    /// 01M5328FPJT7Q5Q1509BC5F0J7, 01M5328FS8XGVW3X6N94F65FA8: a key that
    /// left joins again with a new invite: the new `member` record names
    /// its `left` record, and the key has the user of the new invite. A
    /// removed key stays refused, also with a new invite, and keeps the
    /// invite.
    #[test]
    fn a_key_that_left_joins_again_and_a_removed_key_is_refused() {
        use crate::peer::body::{Left, Removed};
        use crate::peer::record::{Id, Record};

        let dir = tempfile::tempdir().unwrap();
        let hub = owner(dir.path());
        let invites = Invites::in_dir(dir.path());
        let pangolin = Key::from_bytes(&[2; 32]);
        let left = Record::sign(
            &pangolin,
            hub.public(),
            1,
            Id::ZERO,
            5,
            &Body::Left(Left {}),
        )
        .unwrap();
        hub.keep(left.bytes().to_vec(), None).unwrap();
        assert!(hub.read(|state| state.members()[&pangolin.public()].left));

        let invite = invites.make(hub.public(), "ann", 1_000).unwrap();
        let answer = admit(
            &hub,
            &invites,
            pangolin.public(),
            &ask(invite.secret(), "h"),
            2_000,
        );
        assert_eq!(answer.unwrap(), Answer::Joined);
        let member = hub.read(|state| state.members()[&pangolin.public()].clone());
        assert_eq!((member.user.as_str(), member.left), ("ann", false));
        assert_eq!(member.joins.len(), 2);
        assert_eq!(member.joins[1].after, 1);
        // The key is a member again: a second join is refused.
        let invite = invites.make(hub.public(), "ann", 1_000).unwrap();
        let answer = admit(
            &hub,
            &invites,
            pangolin.public(),
            &ask(invite.secret(), "h"),
            2_000,
        );
        assert_eq!(
            answer.unwrap(),
            Answer::Refused {
                why: text::join_member_key(&pangolin.public().to_string())
            }
        );

        let removed = Body::Removed(Removed {
            key: pangolin.public(),
            last: 1,
        });
        hub.write(&removed).unwrap();
        let heads = hub.with_peers(crate::peer::keep::Peers::heads);
        let answer = admit(
            &hub,
            &invites,
            pangolin.public(),
            &ask(invite.secret(), "h"),
            2_000,
        );
        assert_eq!(
            answer.unwrap(),
            Answer::Refused {
                why: text::join_removed_key(&pangolin.public().to_string())
            }
        );
        assert_eq!(hub.with_peers(crate::peer::keep::Peers::heads), heads);
        assert!(invites.take(invite.secret(), 2_000).unwrap().is_ok());
    }

    /// A daemon that is not the owner of its riff refuses each join.
    #[test]
    fn only_the_daemon_of_the_owner_takes_a_join() {
        let dir = tempfile::tempdir().unwrap();
        let riff = Key::from_bytes(&[1; 32]).public();
        let member = Key::from_bytes(&[2; 32]);
        let hub = Hub::new(member, Peers::open(dir.path(), riff, |_| {}).unwrap());
        let invites = Invites::in_dir(dir.path());
        let invite = invites.make(riff, "brett", 1_000).unwrap();
        let new = Key::from_bytes(&[3; 32]).public();
        let answer = admit(&hub, &invites, new, &ask(invite.secret(), "h"), 2_000).unwrap();
        assert_eq!(
            answer,
            Answer::Refused {
                why: text::JOIN_NOT_THE_OWNER.into()
            }
        );
        // The invite is still there.
        assert!(invites.take(invite.secret(), 2_000).unwrap().is_ok());
    }

    #[test]
    fn the_debug_of_an_ask_shows_no_secret() {
        let secret = Secret::from_bytes([5; 32]);
        let shown = format!("{:?}", ask(&secret, "h"));
        assert!(!shown.contains(&*secret.hex()), "{shown}");
        assert!(shown.contains("\"h\""), "{shown}");
    }
}
