//! `riff-server` refuses a `riff` of another wire version, takes a
//! `riff` of another build with the same wire version, and names its own
//! build in each reply (01M3JEE7P46GWXR1BD4Q1TTSGN,
//! 01M3JEE7RDTDD3KQMKH41E8D57, 01M3MNVT7G701SDP1Z1THMRDQ2).

mod common;

use riff_core::build::{Build, HEADER, VERSION, WIRE};
use riff_server::auth::RESOURCE_METADATA_PATH;

/// A build with another commit and another wire version, at `time`.
fn other(time: &str) -> Build {
    Build {
        commit: "0000deadbeef".into(),
        time: time.into(),
        wire: WIRE + 1,
        ..Build::this()
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
async fn another_build_with_the_same_wire_passes_the_check() {
    let (_service, url) = common::start(false, &[]).await;
    let same_wire = Build {
        wire: WIRE,
        ..other("2000-01-01T00:00:00Z")
    };
    for path in CALLS {
        let (status, theirs, _) = call(&url, path, Some(&same_wire)).await;
        assert_ne!(status, 409, "{path}");
        assert_eq!(theirs, VERSION, "{path}");
    }
}

#[tokio::test]
async fn an_older_riff_is_refused_and_told_to_update_riff() {
    let (_service, url) = common::start(false, &[]).await;
    let old = other("2000-01-01T00:00:00Z");
    for path in CALLS {
        let (status, theirs, body) = call(&url, path, Some(&old)).await;
        assert_eq!(status, 409, "{path}");
        assert_eq!(theirs, VERSION);
        assert!(
            body.contains(&old.to_string()) && body.contains(VERSION),
            "{body}"
        );
        assert!(body.contains("Update riff on this machine"), "{body}");
    }
}

#[tokio::test]
async fn a_newer_riff_is_refused_and_told_to_update_the_server() {
    let (_service, url) = common::start(false, &[]).await;
    let new = other("2999-01-01T00:00:00Z");
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
        assert!(
            body.contains("a build from before the wire version"),
            "{body}"
        );
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
