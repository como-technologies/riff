//! An instance change as Cloud Run makes it (#923): a second real
//! `riff-server` starts on the same directory while the first one
//! serves. Cloud Run sends each call to an instance with an open port
//! (01M52JYGM7WH565B7PCVBEJC17, 01M52JYGPX5ETP3HKR0S0Y0YCR). At a
//! deploy, Cloud Run can also start a second instance of the old
//! revision (01M53NHJXGK1PN2A9F7NSX0B4G).

use crate::common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use isolated::Span;
use riff_state::store::LEASE;
use serde_json::{Value, json};
use tempfile::NamedTempFile;
use tokio::time::sleep;

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";

/// The reply to one call of the front.
struct Got {
    /// The instance that replied: 0 for the old one, 1 for the new one.
    instance: usize,
    status: u16,
    /// True when the reply names the build of riff-server.
    riff: bool,
}

/// One call as Cloud Run sends it: to an instance whose port is open.
/// It tries the instances in turn, from `first`. A connect that fails
/// means that the port is closed. `None` when no port is open.
async fn front(listens: &[&str], first: usize) -> Option<Got> {
    for step in 0..listens.len() {
        let instance = (first + step) % listens.len();
        let sent = common::client()
            .post(format!("http://{}/v1/register", listens[instance]))
            .json(&json!({ "me": MIKE }))
            .send()
            .await;
        if let Ok(reply) = sent {
            return Some(Got {
                instance,
                status: reply.status().as_u16(),
                riff: reply.headers().contains_key(riff_core::build::HEADER),
            });
        }
    }
    None
}

async fn open(listen: &str) -> bool {
    tokio::net::TcpStream::connect(listen).await.is_ok()
}

/// Each call gets a reply from an instance that serves, or a 503 of
/// riff-server and then a reply in less than 20 seconds. The old
/// instance closes its port soon after the new one takes the lease, and
/// exits soon after: no call gets its 503 for the 60 seconds of the old
/// build. The real lease wait of 15 seconds runs two times.
#[tokio::test]
async fn at_an_instance_change_each_call_gets_a_reply_within_20_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let (old_log, new_log) = (
        tempfile::NamedTempFile::new().unwrap(),
        tempfile::NamedTempFile::new().unwrap(),
    );
    let old_listen = common::free_address();
    let mut old = common::Server(common::server_on_dir(dir.path(), &old_listen, &old_log));
    common::wait_until_it_serves(&old_listen).await;
    let lease = dir.path().join(LEASE);
    let first_lease = fs::read(&lease).unwrap();

    // Cloud Run starts a second instance while the first one serves.
    let new_listen = common::free_address();
    let _new = common::Server(common::server_on_dir(dir.path(), &new_listen, &new_log));
    let listens = [old_listen.as_str(), new_listen.as_str()];
    let span = Span::start();
    let mut waiting: Option<Span> = None;
    let mut taken: Option<Span> = None;
    let mut closed = false;
    let mut exited = false;
    let mut served = false;
    for call in 0.. {
        assert!(span.within(Duration::from_secs(90)), "no end of the change");
        if taken.is_none() && fs::read(&lease).unwrap() != first_lease {
            taken = Some(Span::start());
        }
        if let Some(taken) = &taken {
            // The old instance reads the lease each 2 seconds, and then
            // closes its port at once.
            if !closed && !open(&old_listen).await {
                closed = true;
                assert!(taken.within(Duration::from_secs(5)), "the port closed late");
            }
            if !exited && let Some(status) = old.0.try_wait().unwrap() {
                exited = true;
                assert!(status.success(), "{status}");
                // It exits after the hand-over, not after 60 seconds.
                assert!(taken.within(Duration::from_secs(20)), "it exited late");
            }
        }
        let Some(got) = front(&listens, call % 2).await else {
            waiting.get_or_insert_with(Span::start);
            sleep(Duration::from_millis(100)).await;
            continue;
        };
        if closed {
            assert_eq!(got.instance, 1, "a call reached the stopped instance");
        }
        match got.status {
            200 => {
                if let Some(waited) = waiting.take() {
                    assert!(
                        waited.within(Duration::from_secs(20)),
                        "a call waited {:?}",
                        waited.wall()
                    );
                }
                served |= got.instance == 1;
            }
            503 => {
                assert!(got.riff, "a 503 that is not of riff-server");
                waiting.get_or_insert_with(Span::start);
            }
            status => panic!("the instance {} gave {status}", got.instance),
        }
        if served && closed && exited {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    let log = fs::read_to_string(old_log.path()).unwrap();
    assert!(
        log.contains("another instance serves: the port is closed"),
        "{log}"
    );
    assert!(log.contains("exiting: another instance serves"), "{log}");
}

/// The lease object as JSON.
fn lease_of(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join(LEASE)).unwrap()).unwrap()
}

fn text(log: &NamedTempFile) -> String {
    fs::read_to_string(log.path()).unwrap()
}

/// A deploy as Cloud Run made it at the release 1.7.0 (#923). The old
/// instance closes its port when it reads the new lease. The new revision
/// is not ready yet, so Cloud Run starts a second instance of the old
/// revision while the new one waits. The second old instance does not
/// take the lease of the newer revision (01M53NHJXGK1PN2A9F7NSX0B4G), and
/// exits at once at its SIGTERM, before it serves
/// (01M53NHK07KNXJ7FXS26NB1675). So the new instance serves after one
/// wait of 15 seconds, not after two or three.
#[tokio::test]
async fn at_a_deploy_a_second_old_instance_does_not_take_the_lease_of_the_new_revision() {
    let dir = tempfile::tempdir().unwrap();
    let logs: [NamedTempFile; 3] = std::array::from_fn(|_| NamedTempFile::new().unwrap());
    let old_listen = common::free_address();
    let _old = common::Server(common::server_of_revision(
        dir.path(),
        &old_listen,
        &logs[0],
        "riff-server-00025-2s5",
    ));
    common::wait_until_it_serves(&old_listen).await;
    let first = lease_of(dir.path());

    // The deploy: an instance of the new revision starts.
    let new_listen = common::free_address();
    let mut new = common::Server(common::server_of_revision(
        dir.path(),
        &new_listen,
        &logs[1],
        "riff-server-00026-899",
    ));
    let span = Span::start();
    let id = loop {
        let lease = lease_of(dir.path());
        if lease["id"] != first["id"] {
            break lease["id"].clone();
        }
        assert!(span.within(Duration::from_secs(30)), "no new lease");
        sleep(Duration::from_millis(50)).await;
    };
    let taken = Span::start();
    while open(&old_listen).await {
        assert!(taken.within(Duration::from_secs(5)), "the port closed late");
        sleep(Duration::from_millis(50)).await;
    }

    // No instance of the old revision has an open port: Cloud Run starts
    // a second one.
    let second_listen = common::free_address();
    let mut second = common::Server(common::server_of_revision(
        dir.path(),
        &second_listen,
        &logs[2],
        "riff-server-00025-c7d",
    ));
    while !text(&logs[2]).contains("does not take it") {
        assert_eq!(
            lease_of(dir.path())["id"],
            id,
            "the second old instance took the lease"
        );
        assert!(taken.within(Duration::from_secs(12)), "{}", text(&logs[2]));
        sleep(Duration::from_millis(50)).await;
    }
    // It replies 503 of riff-server.
    let got = front(&[second_listen.as_str()], 0).await.unwrap();
    assert_eq!((got.status, got.riff), (503, true));

    // The new revision is ready: Cloud Run stops the second old instance.
    let stop = Span::start();
    let kill = std::process::Command::new("kill")
        .args(["-TERM", &second.0.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let status = loop {
        if let Some(status) = second.0.try_wait().unwrap() {
            break status;
        }
        assert!(stop.within(Duration::from_secs(3)), "it exited late");
        sleep(Duration::from_millis(20)).await;
    };
    assert!(status.success(), "{status}");
    assert!(
        text(&logs[2]).contains("stopping before it serves"),
        "{}",
        text(&logs[2])
    );

    // The new instance serves after its one wait.
    common::wait_until_it_serves(&new_listen).await;
    assert!(
        taken.within(Duration::from_secs(20)),
        "it served {:?} after its take",
        taken.wall()
    );
    assert!(new.0.try_wait().unwrap().is_none(), "{}", text(&logs[1]));
    let got = front(&[new_listen.as_str()], 0).await.unwrap();
    assert_eq!(got.status, 200);
    // Each lease names the revision of its instance.
    assert_eq!(first["revision"], 25);
    let lease = lease_of(dir.path());
    assert_eq!((&lease["id"], &lease["revision"]), (&id, &json!(26)));
}
