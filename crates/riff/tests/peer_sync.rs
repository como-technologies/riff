//! The sync of the records of a riff of peers over iroh (#842): daemons
//! on this machine, with no relay and no DNS, as the test of the spike
//! of #697 (01M519Z2QE6QYPJX8HVY17GR6Z, 01M519Z2SV9XSQA4N59N10JAQC,
//! 01M519Z2WAGQEX1H4EGZXWBZT6, 01M519Z2YSXADT45A1R0N70WRQ,
//! 01M519Z33N2MW43XY62NYC6RRH, 01M51BFYQVCFEWCJZERBD4S5N5,
//! 01M519Z363W5GV4MV5RDB6ECG0, 01M519Z3MMW5RKAR1R14PPQ4T9).
//!
//! Each daemon is the records ([`Peers`]), the hub and the peer links of
//! a riff daemon, with its own folder and its own endpoint on 127.0.0.1.
//! A daemon finds another daemon only when the test gives it the address
//! ([`Daemon::knows`]). The members come from the records of the owner:
//! the test writes them (riff join comes with #843).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{Connection, ConnectionError, RecvStream, presets};
use iroh::{Endpoint, EndpointAddr, SecretKey};
use riff::daemon_key::{Key, Public};
use riff::peer::body::{Body, Member, Message, Riff};
use riff::peer::book::Book;
use riff::peer::keep::{Heads, Peers};
use riff::peer::record::{Id, Record};
use riff::peer::sync::{ALPN, LinkState, Links, Net};
use riff::peer::talk::Hub;
use riff::peer::wire::{self, Frame};
use riff_core::name::ThreadName;

/// The key of the test daemon `byte`.
fn key(byte: u8) -> Key {
    Key::from_bytes(&[byte; 32])
}

/// The repository thread of each message of the tests.
fn thread() -> ThreadName {
    "o/r".parse().unwrap()
}

/// A message of `user` in [`thread`].
fn message(user: &str, text: &str) -> Body {
    Body::Message(Box::new(Message {
        from: format!("riff://{user}@h/o/r?session={user}1")
            .parse()
            .unwrap(),
        thread: thread(),
        to: vec![],
        kind: Default::default(),
        body: text.into(),
    }))
}

/// The `member` record body of the test daemon `byte`.
fn admit(byte: u8, user: &str, host: &str) -> Body {
    Body::Member(Member {
        key: key(byte).public(),
        user: user.into(),
        host: host.into(),
    })
}

/// The bytes 1, 2 and 3 of three test keys, in the order of their public
/// keys: the lower key connects.
fn ordered() -> [u8; 3] {
    let mut bytes = [1, 2, 3];
    bytes.sort_by_key(|byte| key(*byte).public());
    bytes
}

/// Waits up to 30 seconds for `done`.
async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = tokio::time::Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(tokio::time::Instant::now() < end, "no {what} in 30 s");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// One daemon of a test.
struct Daemon {
    byte: u8,
    dir: tempfile::TempDir,
    hub: Arc<Hub>,
    links: Arc<Links>,
    lookup: MemoryLookup,
    lines: Arc<Mutex<Vec<String>>>,
}

impl Daemon {
    /// Starts the daemon of the key `byte` in the riff `riff`, on the
    /// records in `dir`.
    async fn start(byte: u8, riff: Public, dir: tempfile::TempDir) -> Daemon {
        let peers = Peers::open(dir.path(), riff, |_| {}).unwrap();
        let lookup = MemoryLookup::new();
        let endpoint = Links::bind(&key(byte), &Net::Local(lookup.clone()))
            .await
            .unwrap();
        let hub = Arc::new(Hub::new(key(byte), peers));
        let lines = Arc::new(Mutex::new(Vec::new()));
        let said = lines.clone();
        let links = Links::start(endpoint, hub.clone(), move |line| {
            said.lock().unwrap().push(line);
        });
        Daemon {
            byte,
            dir,
            hub,
            links,
            lookup,
            lines,
        }
    }

    /// Starts the owner `byte` of a new riff, with its `riff` record and
    /// a `member` record of each of `members`.
    async fn owner(byte: u8, members: &[(u8, &str, &str)]) -> Daemon {
        let owner = Daemon::start(byte, key(byte).public(), tempfile::tempdir().unwrap()).await;
        let riff = Body::Riff(Riff {
            user: "mike".into(),
        });
        owner.hub.write(&riff).unwrap();
        for (member, user, host) in members {
            owner.hub.write(&admit(*member, user, host)).unwrap();
        }
        owner
    }

    /// Starts the daemon `byte` in the riff of `owner`, with each record
    /// that the owner has now: as after a join (#843).
    async fn joined(byte: u8, owner: &Daemon) -> Daemon {
        let dir = tempfile::tempdir().unwrap();
        let (mut book, _) = Book::open(dir.path()).unwrap();
        for record in owner.hub.with_peers(|peers| peers.lacks(&Heads::new())) {
            book.append(record.bytes()).unwrap();
        }
        drop(book);
        Daemon::start(byte, owner.public(), dir).await
    }

    fn public(&self) -> Public {
        key(self.byte).public()
    }

    /// The address of the endpoint of the daemon.
    fn addr(&self) -> EndpointAddr {
        self.links.endpoint().addr()
    }

    /// This daemon can find the daemon `other` now.
    fn knows(&self, other: &Daemon) {
        self.lookup.set_endpoint_info(other.addr());
    }

    /// Stops the peer links of the daemon, and gives its folder.
    async fn stop(self) -> tempfile::TempDir {
        self.links.close().await;
        self.dir
    }

    /// The text of each message of [`thread`] that the daemon keeps.
    fn messages(&self) -> BTreeSet<String> {
        self.hub.read(|state| {
            state
                .messages(&thread())
                .iter()
                .map(|kept| kept.message.body.clone())
                .collect()
        })
    }

    /// The time of the start of each link of the daemon that runs.
    fn ups(&self) -> Vec<(String, i64)> {
        self.links
            .shown()
            .into_iter()
            .filter_map(|shown| match shown.link {
                LinkState::Up { since_ms, .. } => Some((shown.key, since_ms)),
                LinkState::Connects { .. } | LinkState::Waits => None,
            })
            .collect()
    }

    /// The log lines of the daemon that hold `part`.
    fn said(&self, part: &str) -> Vec<String> {
        let lines = self.lines.lock().unwrap();
        lines
            .iter()
            .filter(|line| line.contains(part))
            .cloned()
            .collect()
    }
}

fn texts(texts: &[&str]) -> BTreeSet<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

/// A record of the owner A reaches C through B, while A and C have no
/// connection. Then C stops. A and B write records, and C gets each
/// record that it lacks at its next connect.
#[tokio::test]
async fn a_record_goes_through_b_and_a_stopped_daemon_gets_what_it_lacks() {
    // B has the highest key: A and C connect to it.
    let [a, c, b] = ordered();
    let owner = Daemon::owner(a, &[(b, "brett", "kadomony"), (c, "mike", "pangolin")]).await;
    let riff = owner.public();
    let brett = Daemon::joined(b, &owner).await;
    let pangolin = Daemon::joined(c, &owner).await;
    owner.knows(&brett);
    pangolin.knows(&brett);

    owner.hub.write(&message("mike", "one")).unwrap();
    until("the record of A on C", || {
        pangolin.messages() == texts(&["one"])
    })
    .await;
    assert!(!pangolin.links.linked().contains(&owner.public()));
    assert!(!owner.links.linked().contains(&pangolin.public()));
    assert!(pangolin.links.linked().contains(&brett.public()));
    assert_eq!(
        pangolin.hub.read(|state| state.view()),
        owner.hub.read(|state| state.view())
    );

    let dir = pangolin.stop().await;
    owner.hub.write(&message("mike", "two")).unwrap();
    brett.hub.write(&message("brett", "three")).unwrap();
    until("the three records on B", || brett.messages().len() == 3).await;

    let pangolin = Daemon::start(c, riff, dir).await;
    pangolin.knows(&brett);
    until("the records that C lacks", || {
        pangolin.messages() == texts(&["one", "two", "three"])
    })
    .await;
    assert_eq!(
        pangolin.hub.read(|state| state.view()),
        brett.hub.read(|state| state.view())
    );
    assert!(owner.said("refused").is_empty(), "{:?}", owner.said(""));
}

/// 01M51BFYQVCFEWCJZERBD4S5N5: in a full mesh of three daemons, a record
/// comes to a daemon over two paths. The daemon keeps it one time, and
/// each link stays.
#[tokio::test]
async fn in_a_full_mesh_a_record_that_comes_twice_is_kept_once_and_each_link_stays() {
    let [low, mid, high] = ordered();
    let low = Daemon::owner(
        low,
        &[(mid, "mike", "pangolin"), (high, "brett", "kadomony")],
    )
    .await;
    let riff = low.public();
    let mid = Daemon::start(mid, riff, tempfile::tempdir().unwrap()).await;
    let high = Daemon::start(high, riff, tempfile::tempdir().unwrap()).await;
    for (one, other) in [(&low, &mid), (&low, &high), (&mid, &high)] {
        one.knows(other);
        other.knows(one);
    }
    until("two links on each daemon", || {
        [&low, &mid, &high].iter().all(|d| d.ups().len() == 2)
    })
    .await;
    let ups: Vec<_> = [&low, &mid, &high].iter().map(|d| d.ups()).collect();
    let ended: Vec<usize> = [&low, &mid, &high]
        .iter()
        .map(|d| d.said("ended").len())
        .collect();
    let before = high.links.duplicates();

    low.hub.write(&message("mike", "once")).unwrap();
    until("the record twice on the high daemon", || {
        high.links.duplicates() > before && mid.messages() == texts(&["once"])
    })
    .await;
    for daemon in [&low, &mid, &high] {
        let kept = daemon.hub.read(|state| state.messages(&thread()).len());
        assert_eq!(kept, 1, "the daemon {} keeps the record once", daemon.byte);
    }
    let now: Vec<_> = [&low, &mid, &high].iter().map(|d| d.ups()).collect();
    assert_eq!(now, ups, "each link stays");
    let now: Vec<usize> = [&low, &mid, &high]
        .iter()
        .map(|d| d.said("ended").len())
        .collect();
    assert_eq!(now, ended, "no link ended");
}

/// A test daemon of its own: an endpoint with the key `byte`, with no
/// links of riff. It connects to a daemon, and sends and reads frames.
struct Fake {
    byte: u8,
    conn: Connection,
    send: iroh::endpoint::SendStream,
    recv: RecvStream,
    _endpoint: Endpoint,
}

impl Fake {
    /// Connects to `to` with the key `byte`, and sends empty heads.
    async fn connect(byte: u8, to: EndpointAddr) -> Fake {
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&[byte; 32]))
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap();
        let conn = endpoint.connect(to, ALPN).await.unwrap();
        let (mut send, recv) = conn.open_bi().await.unwrap();
        wire::write(&mut send, &Frame::Heads(Heads::new()))
            .await
            .unwrap();
        Fake {
            byte,
            conn,
            send,
            recv,
            _endpoint: endpoint,
        }
    }

    /// The next record of the fake daemon: `seq` after the record
    /// `before`.
    fn record(&self, riff: Public, seq: u64, before: Id, text: &str) -> Record {
        let body = message("ann", text);
        Record::sign(&key(self.byte), riff, seq, before, seq, &body).unwrap()
    }

    async fn send(&mut self, record: &Record) {
        let frame = Frame::Record(record.bytes().to_vec());
        wire::write(&mut self.send, &frame).await.unwrap();
    }

    /// Reads frames up to the next heads of the daemon.
    async fn next_heads(&mut self) -> Heads {
        let read = async {
            loop {
                match wire::read(&mut self.recv).await.unwrap() {
                    Some(Frame::Heads(heads)) => return heads,
                    Some(Frame::Record(_)) => {}
                    None => panic!("the stream ended"),
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(30), read)
            .await
            .expect("heads in 30 s")
    }

    /// The application code with which the daemon closed the connection.
    async fn closed_with(&self) -> u64 {
        let closed = tokio::time::timeout(Duration::from_secs(30), self.conn.closed())
            .await
            .expect("a close in 30 s");
        match closed {
            ConnectionError::ApplicationClosed(close) => close.error_code.into_inner(),
            other => panic!("not closed by the daemon: {other}"),
        }
    }
}

/// 01M519Z2WAGQEX1H4EGZXWBZT6: a daemon closes a connection of a key
/// that is not the owner or a member, at once.
#[tokio::test]
async fn a_key_that_is_not_a_member_is_closed_at_connect() {
    let owner = Daemon::owner(1, &[(2, "brett", "kadomony")]).await;
    let stranger = Fake::connect(9, owner.addr()).await;
    assert_eq!(stranger.closed_with().await, 1);
    let stranger = key(9).public().to_string();
    assert_eq!(owner.said(&stranger).len(), 1, "{:?}", owner.said(""));
    assert!(owner.links.linked().is_empty());
}

/// 01M519Z33N2MW43XY62NYC6RRH, 01M519Z363W5GV4MV5RDB6ECG0: a record
/// after a gap is not kept. The daemon sends its heads at once, and the
/// records come in order. A bad record closes the connection, and the
/// daemon does not keep it. Its log names the author, the sequence
/// number and the check, with no body.
#[tokio::test]
async fn a_gap_waits_for_the_next_heads_and_a_bad_record_closes_the_link() {
    let owner = Daemon::owner(1, &[(5, "ann", "anvil")]).await;
    let riff = owner.public();
    let mut ann = Fake::connect(5, owner.addr()).await;
    let first_heads = ann.next_heads().await;
    assert!(!first_heads.contains_key(&key(5).public()));

    let one = ann.record(riff, 1, Id::ZERO, "one");
    let two = ann.record(riff, 2, one.id(), "two");
    ann.send(&two).await;
    let heads = ann.next_heads().await;
    assert!(
        !heads.contains_key(&key(5).public()),
        "a record after a gap"
    );
    assert!(owner.messages().is_empty());
    ann.send(&one).await;
    ann.send(&two).await;
    until("the records in order", || {
        owner.messages() == texts(&["one", "two"])
    })
    .await;

    let three = ann.record(riff, 3, two.id(), "secret body");
    let mut bad = three.bytes().to_vec();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    wire::write(&mut ann.send, &Frame::Record(bad))
        .await
        .unwrap();
    assert_eq!(ann.closed_with().await, 2);
    assert_eq!(owner.messages(), texts(&["one", "two"]));
    let refused = owner.said("refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    let author = &key(5).public().to_string()[..8];
    assert!(
        refused[0].contains(&format!("the record 3 of the author {author}"))
            && refused[0].contains("the check of the signature")
            && !refused[0].contains("secret body"),
        "{}",
        refused[0]
    );
}
