//! The members of a riff of peers, as `riff members` shows them
//! (01M519Z4823WC7D3VVTFSDYBS4), and the `removed` records of
//! `riff remove` (01M519Z45J9RC734KBY1G8D49V).
//!
//! # Design
//!
//! [`shown`] gives one [`Shown`] for the owner, then one for each member
//! key, in the order of the user and the host. Each row has the user,
//! the host, the key and the state of the key:
//!
//! | State | When |
//! |---|---|
//! | this machine | the key of this daemon |
//! | online | a peer link to the key runs |
//! | last seen | no link runs: the later of the end of its last link and the time of its last record (01M521J72EC8Z5HAKA1Q9P5Z24) |
//! | forked, removed, left | the key forked, the owner removed it, or it left |
//!
//! The time of a record is the time of its author: riff shows it, and
//! orders nothing by it (01M519Z28S7FB4T1H52642JXZ1). A daemon that
//! started again has no link time yet, so the time of the last record
//! counts.
//!
//! [`removals`] gives the `removed` record of each key that
//! `riff remove USER` removes: each member key of USER that is not
//! removed and did not leave, or with `--host HOST` the one key of USER
//! on HOST. Each record names the last sequence number of that key that
//! the daemon of the owner has. The owner key is no member, so the owner
//! cannot remove itself.
//!
//! # Example
//!
//! ```
//! use riff::daemon_key::Key;
//! use riff::peer::body::{Body, Member, Riff};
//! use riff::peer::keep::Peers;
//! use riff::peer::members::{removals, shown};
//!
//! let dir = tempfile::tempdir()?;
//! let owner = Key::from_bytes(&[1; 32]);
//! let brett = Key::from_bytes(&[2; 32]).public();
//! let mut peers = Peers::open(dir.path(), owner.public(), |_| {})?;
//! let riff = Riff { user: "mike".into(), host: Some("thelio".into()) };
//! peers.write(&owner, &Body::Riff(riff), 1)?;
//! let member = Member { key: brett, user: "brett".into(), host: "kadomony".into() };
//! peers.write(&owner, &Body::Member(member), 2)?;
//!
//! let rows = shown(peers.state(), owner.public(), &Default::default(), |_| false, |_| None);
//! assert_eq!(rows[0].host.as_deref(), Some("thelio"));
//! assert!(rows[0].owner && rows[0].this);
//! assert_eq!(rows[1].user.as_deref(), Some("brett"));
//! let removed = removals(peers.state(), "brett", None).unwrap();
//! assert_eq!(removed[0].key, brett);
//! assert!(removals(peers.state(), "mike", None).is_err());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::daemon_key::Public;
use crate::peer::body::Removed;
use crate::peer::state::State;
use crate::text;

/// One key of the riff, as `riff members` shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// The key, in hex.
    pub key: String,
    /// Its user, when the records name it.
    pub user: Option<String>,
    /// Its host, when the records name it.
    pub host: Option<String>,
    /// True for the owner.
    pub owner: bool,
    /// True for the key of this daemon.
    pub this: bool,
    /// True while a peer link to the key runs.
    pub online: bool,
    /// When it was last seen, in milliseconds since the epoch.
    pub seen_ms: Option<i64>,
    /// True when the key forked (01M519Z38G3RAZPKGFA3R2CB3P).
    pub forked: bool,
    /// True after a `removed` record of the key.
    pub removed: bool,
    /// True after the `left` record of the key.
    pub left: bool,
}

/// The owner and each member key of the riff of `state`, for the daemon
/// of the key `me`. `forked` holds each key that forked. `online` says
/// whether a link to a key runs, and `seen` gives the time of the end of
/// its last link. See the design of the module.
pub fn shown(
    state: &State,
    me: Public,
    forked: &BTreeSet<Public>,
    online: impl Fn(&Public) -> bool,
    seen: impl Fn(&Public) -> Option<i64>,
) -> Vec<Shown> {
    let row = |key: Public, user: Option<&str>, host: Option<&str>| {
        let last = state.last_ms(&key).and_then(|ms| i64::try_from(ms).ok());
        Shown {
            key: key.to_string(),
            user: user.map(str::to_owned),
            host: host.map(str::to_owned),
            owner: key == state.riff(),
            this: key == me,
            online: key != me && online(&key),
            seen_ms: seen(&key).max(last),
            forked: forked.contains(&key),
            removed: false,
            left: false,
        }
    };
    let riff = state.riff();
    let mut rows = vec![row(riff, state.owner(), state.owner_host())];
    let mut members: Vec<Shown> = state
        .members()
        .iter()
        .map(|(key, member)| Shown {
            removed: member.removed,
            left: member.left,
            ..row(*key, Some(&member.user), Some(&member.host))
        })
        .collect();
    members.sort_by(|a, b| (&a.user, &a.host, &a.key).cmp(&(&b.user, &b.host, &b.key)));
    rows.extend(members);
    rows
}

/// The `removed` records of `riff remove user`, or with `host` of the
/// one key of `user` on that host. See the design of the module. It
/// gives why not when no member key matches.
pub fn removals(state: &State, user: &str, host: Option<&str>) -> Result<Vec<Removed>, String> {
    let removed: Vec<Removed> = state
        .members()
        .iter()
        .filter(|(_, member)| member.user == user && !member.removed && !member.left)
        .filter(|(_, member)| host.is_none_or(|host| member.host == host))
        .map(|(key, _)| Removed {
            key: *key,
            last: state.heads().get(key).map_or(0, |(seq, _)| *seq),
        })
        .collect();
    if !removed.is_empty() {
        return Ok(removed);
    }
    let owner = state.owner() == Some(user)
        && host.is_none_or(|host| state.owner_host().is_none_or(|own| own == host));
    Err(if owner {
        text::REMOVE_NOT_THE_OWNER.into()
    } else {
        text::remove_no_key(user, host)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::{Body, Left, Member, Riff};
    use crate::peer::keep::Peers;

    fn key(byte: u8) -> Key {
        Key::from_bytes(&[byte; 32])
    }

    /// The riff of the owner [1] mike on thelio, with mike on pangolin
    /// [2], brett on kadomony [3], brett on lynx [4], and ann [5], who
    /// left. Each member wrote one record at the time 10 * its byte.
    fn riff(dir: &std::path::Path) -> Peers {
        let owner = key(1);
        let mut peers = Peers::open(dir, owner.public(), |_| {}).unwrap();
        let riff = Riff {
            user: "mike".into(),
            host: Some("thelio".into()),
        };
        peers.write(&owner, &Body::Riff(riff), 1).unwrap();
        let members = [
            (2, "mike", "pangolin"),
            (3, "brett", "kadomony"),
            (4, "brett", "lynx"),
            (5, "ann", "h"),
        ];
        for (byte, user, host) in members {
            let member = Member {
                key: key(byte).public(),
                user: user.into(),
                host: host.into(),
            };
            peers.write(&owner, &Body::Member(member), 2).unwrap();
        }
        for byte in [3, 4] {
            let body = Body::Message(Box::new(crate::peer::body::Message {
                from: "riff://brett@h/o/r?session=s1".parse().unwrap(),
                thread: "o/r".parse().unwrap(),
                to: vec![],
                kind: Default::default(),
                body: "hi".into(),
            }));
            peers.write(&key(byte), &body, 10 * u64::from(byte)).unwrap();
        }
        peers.write(&key(5), &Body::Left(Left {}), 50).unwrap();
        peers
    }

    /// 01M519Z4823WC7D3VVTFSDYBS4, 01M521J72EC8Z5HAKA1Q9P5Z24: the owner
    /// first, then each member key by user and host, each with its state.
    #[test]
    fn riff_members_shows_the_owner_and_each_key_with_its_state() {
        let dir = tempfile::tempdir().unwrap();
        let peers = riff(dir.path());
        let me = key(2).public();
        let forked = BTreeSet::from([key(4).public()]);
        let online = |k: &Public| *k == key(1).public();
        // A link to kadomony ended at 100; lynx has no link time.
        let seen = |k: &Public| (*k == key(3).public()).then_some(100);
        let rows = shown(peers.state(), me, &forked, online, seen);
        let names: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.user.as_deref().unwrap(), r.host.as_deref().unwrap()))
            .collect();
        assert_eq!(
            names,
            [
                ("mike", "thelio"),
                ("ann", "h"),
                ("brett", "kadomony"),
                ("brett", "lynx"),
                ("mike", "pangolin"),
            ]
        );
        assert!(rows[0].owner && rows[0].online && !rows[0].this);
        assert!(rows[1].left && !rows[1].removed);
        assert_eq!(rows[1].seen_ms, Some(50));
        assert_eq!(rows[2].seen_ms, Some(100));
        assert!(!rows[2].online);
        assert!(rows[3].forked);
        assert_eq!(rows[3].seen_ms, Some(40));
        assert!(rows[4].this && !rows[4].online);
        assert_eq!(rows[2].key, key(3).public().to_string());
    }

    /// 01M519Z45J9RC734KBY1G8D49V: riff remove of a user removes each of
    /// its keys, with --host one key. A key that left, the owner and a
    /// user with no key give why not.
    #[test]
    fn riff_remove_removes_each_key_of_the_user_or_the_key_on_the_host() {
        let dir = tempfile::tempdir().unwrap();
        let peers = riff(dir.path());
        let state = peers.state();
        let brett = removals(state, "brett", None).unwrap();
        let keys: BTreeSet<Public> = brett.iter().map(|r| r.key).collect();
        assert_eq!(keys, BTreeSet::from([key(3).public(), key(4).public()]));
        assert!(brett.iter().all(|r| r.last == 1));
        let lynx = removals(state, "brett", Some("lynx")).unwrap();
        assert_eq!(lynx.len(), 1);
        assert_eq!(lynx[0].key, key(4).public());
        let mike = removals(state, "mike", None).unwrap();
        assert_eq!(mike[0].key, key(2).public());
        assert_eq!(mike[0].last, 0);

        assert_eq!(
            removals(state, "mike", Some("thelio")),
            Err(text::REMOVE_NOT_THE_OWNER.into())
        );
        assert_eq!(
            removals(state, "ann", None),
            Err(text::remove_no_key("ann", None))
        );
        assert_eq!(
            removals(state, "brett", Some("h")),
            Err(text::remove_no_key("brett", Some("h")))
        );
        assert_eq!(
            removals(state, "eve", None),
            Err(text::remove_no_key("eve", None))
        );
    }
}
