//! `riff-server` refuses a `riff` of a version that it cannot talk to,
//! takes a `riff` of its line or of the line before, and names its own
//! build in each reply (01M3JEE7P46GWXR1BD4Q1TTSGN,
//! 01M3MX1E65XGWDZ062PQ9YXQ5T, 01M3MX1DYY6AVDW946NR0B9T2C,
//! 01M3MX1E1EY1M7JGNCN6FCEVQK, 01M5RG6BXBQVFABA1N4NRZBMQF). The tests
//! take the version of this build at run time.

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
    let before = this().line_before().unwrap();
    let versions = [
        Semver {
            patch: this().patch + 3,
            ..this()
        },
        before,
        Semver { patch: 9, ..before },
    ];
    for version in versions {
        for path in CALLS {
            let (status, theirs, body) = call(&url, path, Some(&at(version))).await;
            assert_ne!(status, 409, "{version} {path}: {body}");
            assert_eq!(theirs, VERSION, "{path}");
        }
    }
}

/// The version of a `riff` two lines before this server: it talks with
/// neither.
fn two_lines_before() -> Semver {
    this().line_before().unwrap().line_before().unwrap()
}

/// A `riff` two lines before the server gets 409 and the step to update
/// riff, before the call takes effect (01M5RG6BXBQVFABA1N4NRZBMQF).
#[tokio::test]
async fn a_riff_two_lines_before_is_refused_before_the_call_takes_effect() {
    let (_service, url) = common::start(false, &[]).await;
    let old = at(two_lines_before());
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=lead"
        .parse()
        .unwrap();
    let thread = me.default_thread().unwrap();
    let register = Register {
        me: me.clone(),
        worker: false,
    };
    let before = at(this().line_before().unwrap());
    let reply = send(&url, "/v1/register", &before, &register).await;
    assert_eq!(reply.status(), 200);
    let post = |body| Post::new(&me, Some(thread.clone()), Vec::new(), body);
    let reply = send(&url, "/v1/post", &old, &post("from two lines before")).await;
    assert_eq!(reply.status(), 409);
    let reply = send(&url, "/v1/post", &before, &post("from the line before")).await;
    assert_eq!(reply.status(), 200);
    let read = Read {
        me: me.clone(),
        thread: thread.clone(),
        all: true,
        after: None,
    };
    let reply: ReadReply = send(&url, "/v1/read", &before, &read)
        .await
        .json()
        .await
        .unwrap();
    let bodies: Vec<&str> = reply.messages.iter().map(|m| m.body.as_str()).collect();
    assert!(bodies.contains(&"from the line before"), "{bodies:?}");
    assert!(!bodies.contains(&"from two lines before"), "{bodies:?}");
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
    let old = at(two_lines_before());
    for path in CALLS {
        let (status, theirs, body) = call(&url, path, Some(&old)).await;
        assert_eq!(status, 409, "{path}");
        assert_eq!(theirs, VERSION);
        assert!(
            body.contains(&old.to_string()) && body.contains(VERSION),
            "{body}"
        );
        let line = this().line();
        let before = this().line_before().unwrap().line();
        let lines = format!("riff-server {line} talks only with riff {line} and {before}.");
        assert!(body.contains(&lines), "{body}");
        assert!(body.contains("Update riff on this machine"), "{body}");
        assert!(body.contains(UPDATE_URL), "{body}");
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
