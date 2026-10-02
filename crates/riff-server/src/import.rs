//! The import of go-live: the state of a riff-server of v0.8.0 moves
//! into the log (01M3Z8MRDZEKTXSKZTDTDSCZ3W).
//!
//! # Design
//!
//! A riff-server from before the log kept its state as objects:
//! [`SESSIONS`], [`TOKENS`] and one object for each thread below
//! [`THREADS`]. A start that finds one of them and no log is the import.
//! It needs no flag.
//!
//! ```mermaid
//! flowchart TD
//!     S[start] --> L{a log or a checkpoint?}
//!     L -- yes --> R[replay the log: no import]
//!     L -- no --> O{old objects?}
//!     O -- no --> N[a new riff: make_riff]
//!     O -- yes --> P[read the old objects: Old::read]
//!     P -->|an object does not read| X[error: no lease.<br/>The old instance serves on]
//!     P --> T[take the lease]
//!     T --> K[the sign-ins: Tokens::import, saved]
//!     K --> I[the command import: the records of Old::changes, one chunk]
//!     I --> M[the memory of each session: State::imported]
//!     M --> C[a checkpoint with the read cursors]
//!     C --> V[open the port]
//! ```
//!
//! - [`Old::read`] reads the old objects before the server takes the
//!   lease. An object that does not read stops the start, and changes
//!   nothing.
//! - [`Old::changes`] gives the changes of the command `import`
//!   ([`crate::state::Import`]), in this order:
//!   1. `riff_made`, with the riff ID of today.
//!   2. `person_joined` for each USER, with its email.
//!   3. `member_invited`, `admin_set` and `owner_set`; `owner_asked`
//!      when a request for the owner role waits.
//!   4. `pause_set` for the riff, paused, and `setting_changed`.
//!   5. The `posted` records: the last [`KEEP_MESSAGES`] messages of
//!      each thread, with their old seq. A direct thread with no session
//!      that did not end is not in the import.
//!   6. `session_forgotten` for each sender of a message that is not a
//!      session of the import. A `posted` record names its sender, and
//!      this record takes the sender out again.
//!   7. For each session: `session_started` (the reason `join`, the
//!      worker mark), then its `joined_thread` records. For a session
//!      that held at the save: its `lead_set` and `claimed` records.
//! - The session records come after the `posted` records. So the place
//!   of each session is the place of the old `sessions` object, not the
//!   place of its last message.
//! - A session is in the import when its last sign of life is less than
//!   [`SESSION_EXPIRY`] before the import. A session that ended is in
//!   the import with its threads and its read cursors, and with no
//!   claim and no lead: it can come back, and then it reads no message
//!   a second time.
//! - A session held at the save when it did not end, and its last sign
//!   of life was at most [`CLAIM_GRACE`] before the save. Only such a
//!   session keeps its claims and its lead, as a load of v0.8.0 did.
//! - [`Old::memory`] gives what the log does not hold: the last call of
//!   each session, its end, its status, and the read cursors. The
//!   server writes a checkpoint at once after the import, so the read
//!   cursors stay.
//! - The server saves the sign-ins before it writes the log. A server
//!   that stops between the two steps finds no log at its next start,
//!   and imports again.
//! - The old objects stay in the store. The import changes none of
//!   them. A second start finds the log, and does not import again.
//!
//! # Example
//!
//! ```
//! use riff_core::record::Change;
//! use riff_server::import::Old;
//!
//! let sessions = br#"{"saved_ms":1000,"sessions":[{"uri":"riff://ann@heron/acme/app?session=a1","seen_ms":900,"alive_ms":950}],"cursors":[],"claims":[{"thread":"acme/app","item":"issue-7","who":{"user":"ann","session":"a1"}}],"leads":[],"riff":"running"}"#;
//! let thread = br#"{"name":"acme/app","members":[{"user":"ann","session":"a1"}],"messages":[]}"#;
//! let old = Old::parse(Some(sessions), [("threads/acme%2Fapp", thread.as_slice())], None).unwrap();
//! let kinds: Vec<&str> = old.changes(1000).iter().map(Change::kind).collect();
//! assert_eq!(
//!     kinds,
//!     ["pause_set", "setting_changed", "session_started", "joined_thread", "claimed"]
//! );
//! ```

use std::collections::{BTreeMap, BTreeSet};

use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::record::{
    AdminSet, Change, Claimed, Email, Forgotten, Member, OwnerAsked, OwnerSet, PauseSet,
    PersonJoined, Posted, RiffMade, Scope, SessionStarted, SettingChanged,
};
use riff_core::wire::{Idle, Message, RiffState, StartReason, Status};
use serde::Deserialize;

use crate::state::{CLAIM_GRACE, Imported, ImportedSession, KEEP_MESSAGES, SESSION_EXPIRY};
use crate::store::{Store, StoreError};
use crate::{checkpoint, log};

/// The old object with the sessions, their read cursors, the claims, the
/// leads, the riff state and the settings.
pub const SESSIONS: &str = "sessions";

/// The old object with the people, the sign-ins and the tokens.
pub const TOKENS: &str = "tokens";

/// The prefix of the old thread objects.
pub const THREADS: &str = "threads/";

/// An old object that does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    /// The name of the object.
    pub name: String,
    pub why: String,
}

/// The old objects of a store, as the import reads them. See the module
/// docs.
#[derive(Clone, Debug)]
pub struct Old {
    saved: SavedSessions,
    threads: Vec<SavedThread>,
    people: People,
    /// The bytes of the [`TOKENS`] object, for
    /// [`Tokens::import`](crate::token::Tokens::import).
    tokens: Option<Vec<u8>>,
}

/// The JSON of the sessions object of v0.8.0.
#[derive(Clone, Debug, Default, Deserialize)]
struct SavedSessions {
    /// The time of the save, in milliseconds since the Unix epoch.
    #[serde(default)]
    saved_ms: u64,
    sessions: Vec<SavedSession>,
    cursors: Vec<SavedCursor>,
    claims: Vec<SavedClaim>,
    #[serde(default)]
    leads: Vec<SavedLead>,
    #[serde(default)]
    idle: Idle,
}

#[derive(Clone, Debug, Deserialize)]
struct SavedSession {
    uri: SessionUri,
    seen_ms: u64,
    #[serde(default)]
    alive_ms: u64,
    #[serde(default)]
    ended: bool,
    #[serde(default)]
    status: Option<SavedStatus>,
    #[serde(default)]
    worker: bool,
}

impl SavedSession {
    /// The last sign of life, in milliseconds since the Unix epoch.
    fn alive_ms(&self) -> u64 {
        self.alive_ms.max(self.seen_ms)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SavedStatus {
    #[serde(flatten)]
    status: Status,
    set_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct SavedCursor {
    who: Who,
    thread: ThreadName,
    seq: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct SavedClaim {
    thread: ThreadName,
    item: String,
    who: Who,
}

#[derive(Clone, Debug, Deserialize)]
struct SavedLead {
    thread: ThreadName,
    who: Who,
}

/// The JSON of a thread object of v0.8.0.
#[derive(Clone, Debug, Deserialize)]
struct SavedThread {
    name: ThreadName,
    members: BTreeSet<Who>,
    messages: Vec<Stored>,
}

#[derive(Clone, Debug, Deserialize)]
struct Stored {
    message: Message,
    #[serde(default)]
    woken: BTreeSet<Who>,
}

/// The people in the tokens object of v0.8.0. The import reads the
/// sign-ins of the object in [`crate::token`].
#[derive(Clone, Debug, Default, Deserialize)]
struct People {
    #[serde(default)]
    users: BTreeMap<String, String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    members: BTreeSet<String>,
    #[serde(default)]
    admins: BTreeSet<String>,
    #[serde(default)]
    riff_id: Option<String>,
    #[serde(default)]
    no_owner: bool,
    #[serde(default)]
    take: Option<Take>,
}

#[derive(Clone, Debug, Deserialize)]
struct Take {
    admin: String,
    /// In milliseconds since the Unix epoch.
    until: u64,
}

fn millis(time: std::time::Duration) -> u64 {
    u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
}

impl Old {
    /// Reads the old objects: the sessions object, each thread object
    /// with its name, and the tokens object.
    pub fn parse<'a>(
        sessions: Option<&[u8]>,
        threads: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        tokens: Option<&[u8]>,
    ) -> Result<Old, Unreadable> {
        let unreadable = |name: &str, e: serde_json::Error| Unreadable {
            name: name.to_owned(),
            why: e.to_string(),
        };
        let saved = match sessions {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|e| unreadable(SESSIONS, e))?,
            None => SavedSessions::default(),
        };
        let threads = threads
            .into_iter()
            .map(|(name, bytes)| serde_json::from_slice(bytes).map_err(|e| unreadable(name, e)))
            .collect::<Result<_, _>>()?;
        let people = match tokens {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|e| unreadable(TOKENS, e))?,
            None => People::default(),
        };
        Ok(Old {
            saved,
            threads,
            people,
            tokens: tokens.map(<[u8]>::to_vec),
        })
    }

    /// Reads the old objects of `store`. `None` when the store has a
    /// log or a checkpoint, or no old object: then the start is no
    /// import.
    pub async fn read(store: &dyn Store) -> Result<Option<Old>, StoreError> {
        let has_log = !store.list(log::LOG).await?.is_empty()
            || !store.list(checkpoint::CHECKPOINT).await?.is_empty();
        if has_log {
            return Ok(None);
        }
        let sessions = store.load(SESSIONS).await?;
        let tokens = store.load(TOKENS).await?;
        let mut threads = Vec::new();
        for name in store.list(THREADS).await? {
            if let Some(loaded) = store.load(&name).await? {
                threads.push((name, loaded.bytes));
            }
        }
        if sessions.is_none() && tokens.is_none() && threads.is_empty() {
            return Ok(None);
        }
        let listed = threads.iter().map(|(n, b)| (n.as_str(), b.as_slice()));
        Old::parse(
            sessions.as_ref().map(|l| l.bytes.as_slice()),
            listed,
            tokens.as_ref().map(|l| l.bytes.as_slice()),
        )
        .map(Some)
        .map_err(|e| StoreError::not_valid(store, &e.name, e.why))
    }

    /// The bytes of the old tokens object, if the store has one.
    pub fn tokens(&self) -> Option<&[u8]> {
        self.tokens.as_deref()
    }

    /// Each session of the import: its last sign of life is less than
    /// [`SESSION_EXPIRY`] before `now_ms`.
    fn sessions(&self, now_ms: u64) -> impl Iterator<Item = &SavedSession> {
        let expiry = millis(SESSION_EXPIRY);
        self.saved
            .sessions
            .iter()
            .filter(move |s| now_ms.saturating_sub(s.alive_ms()) <= expiry)
    }

    /// True when the session held its claims and its lead at the save.
    fn held(&self, session: &SavedSession) -> bool {
        let stopped = self.saved.saved_ms.saturating_sub(session.alive_ms());
        !session.ended && stopped <= millis(CLAIM_GRACE)
    }

    /// Each thread of the import. A direct thread is in it only when one
    /// or more of its sessions are in the import and did not end.
    fn threads(&self, now_ms: u64) -> impl Iterator<Item = &SavedThread> {
        let open: BTreeSet<&Who> = self
            .sessions(now_ms)
            .filter(|s| !s.ended)
            .map(|s| s.uri.who())
            .collect();
        self.threads
            .iter()
            .filter(move |t| !t.name.is_direct() || t.members.iter().any(|m| open.contains(m)))
    }

    /// The changes of the command `import`, at `now_ms`. See the module
    /// docs for the order.
    pub fn changes(&self, now_ms: u64) -> Vec<Change> {
        let mut changes = Vec::new();
        let people = &self.people;
        if let Some(riff_id) = &people.riff_id {
            changes.push(Change::RiffMade(RiffMade {
                riff_id: riff_id.clone(),
            }));
        }
        for (user, email) in &people.users {
            changes.push(Change::PersonJoined(PersonJoined {
                user: user.clone(),
                email: email.clone(),
            }));
        }
        for email in &people.members {
            changes.push(Change::MemberInvited(Email {
                email: email.clone(),
            }));
        }
        for email in &people.admins {
            changes.push(Change::AdminSet(AdminSet {
                email: email.clone(),
                admin: true,
            }));
        }
        if people.owner.is_some() || people.no_owner {
            changes.push(Change::OwnerSet(OwnerSet {
                email: people.owner.clone(),
            }));
        }
        if let Some(take) = &people.take {
            changes.push(Change::OwnerAsked(OwnerAsked {
                email: take.admin.clone(),
                due_ms: take.until,
            }));
        }
        changes.push(Change::PauseSet(PauseSet {
            scope: Scope::Riff,
            state: RiffState::Paused,
        }));
        changes.push(Change::SettingChanged(SettingChanged {
            idle: self.saved.idle,
        }));

        let sessions: BTreeMap<&Who, &SavedSession> =
            self.sessions(now_ms).map(|s| (s.uri.who(), s)).collect();
        let server = crate::owner::server_uri();
        let mut senders = BTreeMap::new();
        for thread in self.threads(now_ms) {
            let skip = thread.messages.len().saturating_sub(KEEP_MESSAGES);
            for stored in thread.messages.iter().skip(skip) {
                let from = &stored.message.from;
                if !sessions.contains_key(from.who()) && from.who() != server.who() {
                    senders.insert(from.who().clone(), from.clone());
                }
                changes.push(Change::Posted(Box::new(Posted {
                    thread: thread.name.clone(),
                    message: stored.message.clone(),
                    woken: stored.woken.clone(),
                })));
            }
        }
        for (who, from) in senders {
            changes.push(Change::SessionForgotten(Forgotten {
                session: SessionUri::new(who, from.place().clone()),
            }));
        }

        for (who, session) in &sessions {
            let uri = SessionUri::new((*who).clone(), session.uri.place().clone());
            changes.push(Change::SessionStarted(SessionStarted {
                session: uri.clone(),
                reason: StartReason::Join,
                worker: session.worker,
            }));
            let member = |thread: &ThreadName| Member {
                session: uri.clone(),
                thread: thread.clone(),
            };
            for thread in self.threads(now_ms) {
                if thread.members.contains(*who) {
                    changes.push(Change::JoinedThread(member(&thread.name)));
                }
            }
            if !self.held(session) {
                continue;
            }
            for lead in self.saved.leads.iter().filter(|l| &l.who == *who) {
                changes.push(Change::LeadSet(member(&lead.thread)));
            }
            for claim in self.saved.claims.iter().filter(|c| &c.who == *who) {
                changes.push(Change::Claimed(Claimed {
                    session: uri.clone(),
                    thread: claim.thread.clone(),
                    item: claim.item.clone(),
                }));
            }
        }
        changes
    }

    /// What the log does not hold, for
    /// [`State::imported`](crate::state::State::imported): the last
    /// call, the end and the status of each session of the import, and
    /// the read cursors.
    pub fn memory(&self, now_ms: u64) -> Imported {
        let sessions: Vec<ImportedSession> = self
            .sessions(now_ms)
            .map(|s| ImportedSession {
                who: s.uri.who().clone(),
                seen_ms: s.seen_ms,
                ended: s.ended,
                status: s.status.clone().map(|s| (s.status, s.set_ms)),
            })
            .collect();
        let known: BTreeSet<&Who> = sessions.iter().map(|s| &s.who).collect();
        let cursors = self
            .saved
            .cursors
            .iter()
            .filter(|c| known.contains(&c.who))
            .map(|c| (c.who.clone(), c.thread.clone(), c.seq))
            .collect();
        Imported { sessions, cursors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSIONS_JSON: &[u8] = include_bytes!("../tests/fixtures/0.8.0/sessions");
    const TOKENS_JSON: &[u8] = include_bytes!("../tests/fixtures/0.8.0/tokens");
    const SAVED_MS: u64 = 1_790_000_000_000;

    fn old() -> Old {
        let threads = [
            (
                "threads/como-technologies%2Friff",
                include_bytes!("../tests/fixtures/0.8.0/threads/como-technologies%2Friff")
                    .as_slice(),
            ),
            (
                "threads/como-technologies%2Fstrata",
                include_bytes!("../tests/fixtures/0.8.0/threads/como-technologies%2Fstrata")
                    .as_slice(),
            ),
            (
                "threads/design",
                include_bytes!("../tests/fixtures/0.8.0/threads/design").as_slice(),
            ),
            (
                "threads/dm%3Abrett%2Fb1%7Cbrett%2Fb2",
                include_bytes!("../tests/fixtures/0.8.0/threads/dm%3Abrett%2Fb1%7Cbrett%2Fb2")
                    .as_slice(),
            ),
            (
                "threads/dm%3Amike%2Fm3%7Cmike%2Fm4",
                include_bytes!("../tests/fixtures/0.8.0/threads/dm%3Amike%2Fm3%7Cmike%2Fm4")
                    .as_slice(),
            ),
        ];
        Old::parse(Some(SESSIONS_JSON), threads, Some(TOKENS_JSON)).unwrap()
    }

    fn kinds(changes: &[Change]) -> Vec<&'static str> {
        let mut kinds: Vec<&str> = changes.iter().map(Change::kind).collect();
        kinds.dedup();
        kinds
    }

    #[test]
    fn the_changes_come_in_the_order_of_the_design() {
        let changes = old().changes(SAVED_MS);
        assert_eq!(
            kinds(&changes)[..9],
            [
                "riff_made",
                "person_joined",
                "member_invited",
                "admin_set",
                "owner_set",
                "owner_asked",
                "pause_set",
                "setting_changed",
                "posted",
            ]
        );
        // Each session record comes after the last message.
        let last_post = changes
            .iter()
            .rposition(|c| matches!(c, Change::Posted(_)))
            .unwrap();
        let first_session = changes
            .iter()
            .position(|c| matches!(c, Change::SessionStarted(_)))
            .unwrap();
        assert!(last_post < first_session);
    }

    #[test]
    fn the_riff_is_paused_and_keeps_its_id_and_its_people() {
        let changes = old().changes(SAVED_MS);
        let Change::RiffMade(made) = &changes[0] else {
            panic!("the first record is riff_made");
        };
        assert_eq!(made.riff_id, "rPj_u8SJv17e8VPwciGGxitcBSKepm_xlwgMssv5YB0");
        let paused = Change::PauseSet(PauseSet {
            scope: Scope::Riff,
            state: RiffState::Paused,
        });
        assert!(changes.contains(&paused));
        let owner = Change::OwnerSet(OwnerSet {
            email: Some("mike@comotechnologies.io".into()),
        });
        assert!(changes.contains(&owner));
        // A removed person keeps its USER, and is no member.
        let joined: Vec<&str> = changes
            .iter()
            .filter_map(|c| match c {
                Change::PersonJoined(p) => Some(p.user.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(joined, ["brett", "gone", "mike"]);
        let gone = Change::MemberInvited(Email {
            email: "gone@example.com".into(),
        });
        assert!(!changes.contains(&gone));
    }

    #[test]
    fn a_thread_keeps_its_last_messages_with_their_seq() {
        let changes = old().changes(SAVED_MS);
        let seqs: Vec<u64> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Posted(p) if p.thread.to_string() == "como-technologies/riff" => {
                    Some(p.message.seq)
                }
                _ => None,
            })
            .collect();
        assert_eq!(seqs.len(), KEEP_MESSAGES);
        assert_eq!((seqs[0], seqs[199]), (8, 207));
    }

    #[test]
    fn a_direct_thread_of_two_ended_sessions_is_not_in_the_import() {
        let changes = old().changes(SAVED_MS);
        let threads: BTreeSet<String> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Posted(p) => Some(p.thread.to_string()),
                Change::JoinedThread(m) => Some(m.thread.to_string()),
                _ => None,
            })
            .collect();
        assert!(threads.contains("dm:brett/b1|brett/b2"));
        assert!(!threads.contains("dm:mike/m3|mike/m4"));
    }

    #[test]
    fn only_a_session_that_held_at_the_save_keeps_its_claims_and_its_lead() {
        let changes = old().changes(SAVED_MS);
        let claims: Vec<(String, &str)> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Claimed(c) => Some((c.session.who().to_string(), c.item.as_str())),
                _ => None,
            })
            .collect();
        // `old` is forgotten, and the last sign of life of `m5` was 2
        // hours before the save.
        assert_eq!(
            claims,
            [
                ("brett/b2".to_owned(), "issue-7"),
                ("mike/m2".to_owned(), "issue-341")
            ]
        );
        let started: Vec<String> = changes
            .iter()
            .filter_map(|c| match c {
                Change::SessionStarted(s) => Some(s.session.who().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            [
                "brett/b1", "brett/b2", "mike/m1", "mike/m2", "mike/m3", "mike/m4", "mike/m5"
            ]
        );
    }

    #[test]
    fn a_sender_that_is_no_session_of_the_import_is_forgotten_again() {
        // 31 days later, only the sessions of that time are gone: each
        // sender of a message is forgotten.
        let later = SAVED_MS + millis(SESSION_EXPIRY) + 1;
        let changes = old().changes(later);
        let forgotten: Vec<String> = changes
            .iter()
            .filter_map(|c| match c {
                Change::SessionForgotten(f) => Some(f.session.who().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(forgotten, ["brett/b1", "mike/m1", "mike/m2"]);
        assert!(
            !changes
                .iter()
                .any(|c| matches!(c, Change::SessionStarted(_)))
        );
        // No direct thread is left.
        assert!(
            !changes
                .iter()
                .any(|c| matches!(c, Change::Posted(p) if p.thread.is_direct()))
        );
    }

    /// The log of the import alone gives the state: a replay is the
    /// same. Each session is in the place of the old `sessions` object,
    /// with its claims and its lead mark, and a sender that is no
    /// session of the import is not known.
    #[test]
    fn a_replay_of_the_import_gives_the_same_state_with_each_session_in_its_place() {
        use std::time::Instant;

        use crate::state::{Caller, Import, State};

        let now = Instant::now();
        let mut state = State::default();
        let import = Import {
            changes: old().changes(SAVED_MS),
        };
        let made = state.run(&Caller::server(), &import, now).unwrap();
        assert_eq!(made.0.len(), import.changes.len());
        state.imported(old().memory(SAVED_MS), now);

        let replayed = State::replay(state.take_queue(), now, SAVED_MS);
        assert!(replayed.same_log_state(&state));
        for state in [&state, &replayed] {
            let uris: Vec<String> = state
                .who(now, SAVED_MS, true)
                .iter()
                .map(|s| s.uri.to_string())
                .collect();
            assert_eq!(
                uris,
                [
                    "riff://brett@kadomony/como-technologies/strata?session=b1&lead=true",
                    "riff://brett@kadomony/como-technologies/strata?session=b2&claim=issue-7#issue-7",
                    "riff://mike@pangolin/como-technologies/riff?session=m1&lead=true",
                    "riff://mike@pangolin/como-technologies/riff?session=m2&claim=issue-341#issue-341",
                    "riff://mike@thelio/como-technologies/riff?session=m3",
                    "riff://mike@thelio/como-technologies/riff?session=m4",
                    "riff://mike@thelio/como-technologies/riff?session=m5#issue-9",
                ]
            );
        }
        // The memory of the old server: the worker read up to 205.
        let m2 = "riff://mike@pangolin/como-technologies/riff?session=m2"
            .parse::<SessionUri>()
            .unwrap();
        let unread: Vec<usize> = state
            .threads_of(m2.who(), now)
            .iter()
            .map(|t| t.unread)
            .collect();
        assert_eq!(unread, [1]);
    }

    #[test]
    fn the_memory_has_the_cursors_the_ends_and_the_statuses() {
        let memory = old().memory(SAVED_MS);
        assert_eq!(memory.cursors.len(), 1);
        assert_eq!(memory.cursors[0].2, 205);
        let ended: Vec<String> = memory
            .sessions
            .iter()
            .filter(|s| s.ended)
            .map(|s| s.who.to_string())
            .collect();
        assert_eq!(ended, ["mike/m3", "mike/m4"]);
        let status = memory
            .sessions
            .iter()
            .find_map(|s| s.status.as_ref())
            .unwrap();
        assert_eq!(status.0.step, "issue-341: writes the import");
    }

    #[test]
    fn an_object_that_does_not_read_names_itself() {
        let error = Old::parse(Some(b"{"), [], None).unwrap_err();
        assert_eq!(error.name, SESSIONS);
        let error = Old::parse(None, [("threads/x", b"[]".as_slice())], None).unwrap_err();
        assert_eq!(error.name, "threads/x");
        let error = Old::parse(None, [], Some(b"7")).unwrap_err();
        assert_eq!(error.name, TOKENS);
    }
}
