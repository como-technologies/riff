//! `riff-server` refuses a `riff` of a version that it cannot talk to,
//! takes a `riff` of its line, of the line before or of the other line
//! of the bridge, and names its own build in each reply
//! (01M3JEE7P46GWXR1BD4Q1TTSGN, 01M3MX1E65XGWDZ062PQ9YXQ5T,
//! 01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK,
//! 01M55FCZNNZGKM4EKAXZQFTDWR). riff-server of the line 3 takes the line
//! 1 only from 1.8.2.

use crate::common;

use riff_core::build::{Build, HEADER, Semver, UPDATE_URL, VERSION};
use riff_core::name::SessionUri;
use riff_core::wire::{Post, Read, ReadReply, Register};
use riff_server::auth::RESOURCE_METADATA_PATH;

/// The version of this server.
fn this() -> Semver {
    Build::this().semver().unwrap()
}

/// A build of `version` with another commit.
fn at(version: Semver) -> Build {
    Build {
        version: version.to_string(),
        commit: "0000deadbeef".into(),
        time: "2000-01-01T00:00:00Z".into(),
    }
}

/// A client with no default header: it names `build`, or no build.
async fn call(url: &str, path: &str, build: Option<&Build>) -> (u16, String, String) {
    let client = reqwest::Client::new();
    let mut request = client
        .post(format!("{url}{path}"))
        .json(&serde_json::json!({}));
    if let Some(b) = build {
        request = request.header(HEADER, b.to_string());
    }
    let reply = request.send().await.unwrap();
    let status = reply.status().as_u16();
    let theirs = reply.headers()[HEADER].to_str().unwrap().to_owned();
    (status, theirs, reply.text().await.unwrap())
}

const CALLS: [&str; 3] = ["/v1/join", "/v1/post", "/v1/read"];

#[tokio::test]
async fn a_call_of_this_build_passes_the_check() {
    let (_service, url) = common::start(false, &[]).await;
    for path in CALLS {
        let (status, theirs, _) = call(&url, path, Some(&Build::this())).await;
        assert_ne!(status, 409, "{path}");
        assert_eq!(theirs, VERSION, "{path}");
    }
}

#[tokio::test]
async fn a_riff_of_this_line_or_the_line_before_passes_the_check() {
    let (_service, url) = common::start(false, &[]).await;
    let mut versions = vec![Semver {
        patch: this().patch + 3,
        ..this()
    }];
    // The line 1 has no line before: 0.x does not talk with it.
    if let Some(before) = this().line_before() {
        versions.extend([before, Semver { patch: 9, ..before }]);
    }
    for version in versions {
        for path in CALLS {
            let (status, theirs, body) = call(&url, path, Some(&at(version))).await;
            assert_ne!(status, 409, "{version} {path}: {body}");
            assert_eq!(theirs, VERSION, "{path}");
        }
    }
}

/// The bridge: riff-server of the line 1 takes a `riff` of the line 3,
/// and riff-server of the line 3 takes a `riff` of the line 1 from 1.8.2
/// (01M55FCZNNZGKM4EKAXZQFTDWR). This server is on one of the two lines.
#[tokio::test]
async fn a_riff_of_the_other_line_of_the_bridge_passes_the_check() {
    let (_service, url) = common::start(false, &[]).await;
    let others = match this().major {
        1 => ["3.0.0", "3.2.1"],
        3 => ["1.8.2", "1.9.0"],
        major => panic!("riff-server {major}.x is on no line of the bridge"),
    };
    for version in others {
        for path in CALLS {
            let (status, theirs, body) =
                call(&url, path, Some(&at(version.parse().unwrap()))).await;
            assert_ne!(status, 409, "{version} {path}: {body}");
            assert_eq!(theirs, VERSION, "{path}");
        }
    }
}

/// riff-server of the line 3 takes a `riff` of the line 1 only from
/// 1.8.2, the first build that knows the bridge
/// (01M55FCZNNZGKM4EKAXZQFTDWR). riff 1.8.1 refuses each reply of
/// riff-server 3, so the server refuses its call with 409 and the step
/// to update riff.
#[tokio::test]
async fn riff_server_3_refuses_a_riff_of_the_line_1_before_the_bridge() {
    let server = at("3.0.0".parse().unwrap());
    let (_service, url) = common::start_as(server.clone()).await;
    for version in ["1.8.1", "1.0.0"] {
        let old = at(version.parse().unwrap());
        for path in CALLS {
            let (status, theirs, body) = call(&url, path, Some(&old)).await;
            assert_eq!(status, 409, "{version} {path}: {body}");
            assert_eq!(theirs, server.to_string(), "{path}");
            assert!(
                body.contains(&old.to_string()) && body.contains(&server.to_string()),
                "{body}"
            );
            assert!(
                body.contains("riff-server 3 talks only with riff 3, 2 and 1 from 1.8.2."),
                "{body}"
            );
            assert!(body.contains("Update riff on this machine"), "{body}");
            assert!(body.contains(UPDATE_URL), "{body}");
        }
    }
    for version in ["1.8.2", "1.9.0"] {
        for path in CALLS {
            let (status, _, body) = call(&url, path, Some(&at(version.parse().unwrap()))).await;
            assert_ne!(status, 409, "{version} {path}: {body}");
        }
    }
}

/// A call that riff-server 3 refuses takes no effect: a post of riff
/// 1.8.1 is not in the thread. The same post of riff 1.8.2 is
/// (01M55FCZNNZGKM4EKAXZQFTDWR).
#[tokio::test]
async fn a_call_that_riff_server_3_refuses_takes_no_effect() {
    let (_service, url) = common::start_as(at("3.0.0".parse().unwrap())).await;
    let old = at("1.8.1".parse().unwrap());
    let bridge = at("1.8.2".parse().unwrap());
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=lead"
        .parse()
        .unwrap();
    let thread = me.default_thread().unwrap();
    let register = Register {
        me: me.clone(),
        worker: false,
    };
    let reply = send(&url, "/v1/register", &bridge, &register).await;
    assert_eq!(reply.status(), 200);
    let post = |body| Post::new(&me, Some(thread.clone()), Vec::new(), body);
    let reply = send(&url, "/v1/post", &old, &post("from 1.8.1")).await;
    assert_eq!(reply.status(), 409);
    let reply = send(&url, "/v1/post", &bridge, &post("from 1.8.2")).await;
    assert_eq!(reply.status(), 200);
    let read = Read {
        me: me.clone(),
        thread: thread.clone(),
        all: true,
        after: None,
    };
    let reply: ReadReply = send(&url, "/v1/read", &bridge, &read)
        .await
        .json()
        .await
        .unwrap();
    let bodies: Vec<&str> = reply.messages.iter().map(|m| m.body.as_str()).collect();
    assert!(bodies.contains(&"from 1.8.2"), "{bodies:?}");
    assert!(!bodies.contains(&"from 1.8.1"), "{bodies:?}");
}

/// A call of a `riff` of `build`, with `body` as JSON.
async fn send(
    url: &str,
    path: &str,
    build: &Build,
    body: &impl serde::Serialize,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{url}{path}"))
        .header(HEADER, build.to_string())
        .json(body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn an_older_riff_is_refused_and_told_to_update_riff() {
    let (_service, url) = common::start(false, &[]).await;
    // No line of riff-server from the line 1 takes a riff of 0.x.
    let old = at("0.8.0".parse().unwrap());
    for path in CALLS {
        let (status, theirs, body) = call(&url, path, Some(&old)).await;
        assert_eq!(status, 409, "{path}");
        assert_eq!(theirs, VERSION);
        assert!(
            body.contains(&old.to_string()) && body.contains(VERSION),
            "{body}"
        );
        assert!(body.contains("Update riff on this machine"), "{body}");
        assert!(body.contains(riff_core::build::UPDATE_URL), "{body}");
    }
}

#[tokio::test]
async fn a_newer_riff_is_refused_and_told_to_update_the_server() {
    let (_service, url) = common::start(false, &[]).await;
    let new = at(this().line_after());
    for path in CALLS {
        let (status, _, body) = call(&url, path, Some(&new)).await;
        assert_eq!(status, 409, "{path}");
        assert!(
            body.contains(&new.to_string()) && body.contains(VERSION),
            "{body}"
        );
        assert!(body.contains("Update riff-server"), "{body}");
    }
}

#[tokio::test]
async fn a_riff_with_no_build_is_refused() {
    let (_service, url) = common::start(false, &[]).await;
    for path in CALLS {
        let (status, _, body) = call(&url, path, None).await;
        assert_eq!(status, 409, "{path}");
        assert!(body.contains("riff (an older build)"), "{body}");
    }
}

/// The image build gets its build from `deploy/build-id.sh`. It names
/// the same commit and time as `build.rs` (01M3JEE7YXQPWS65FBVTASAEBX).
#[test]
fn the_script_of_the_image_build_names_this_build() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new(root.join("deploy/build-id.sh"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let this = Build::this();
    let expected = format!(
        "RIFF_COMMIT={}\nRIFF_COMMIT_TIME={}\n",
        this.commit, this.time
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[tokio::test]
async fn the_oauth_metadata_stays_open_and_names_the_build() {
    let (_service, url) = common::start(true, &[]).await;
    let reply = reqwest::get(format!("{url}{RESOURCE_METADATA_PATH}"))
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.headers()[HEADER], VERSION);
}
