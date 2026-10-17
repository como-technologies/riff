//! `riff-server` refuses a `riff` of a version that it cannot talk to,
//! takes a `riff` of its line, of the line before or of the other line
//! of the bridge, and names its own build in each reply
//! (01M3JEE7P46GWXR1BD4Q1TTSGN, 01M3MX1E65XGWDZ062PQ9YXQ5T,
//! 01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK,
//! 01M55FCZNNZGKM4EKAXZQFTDWR).

use crate::common;

use riff_core::build::{Build, HEADER, Semver, VERSION};
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
/// and riff-server of the line 3 takes a `riff` of the line 1
/// (01M55FCZNNZGKM4EKAXZQFTDWR). This server is on one of the two lines.
#[tokio::test]
async fn a_riff_of_the_other_line_of_the_bridge_passes_the_check() {
    let (_service, url) = common::start(false, &[]).await;
    let others = match this().major {
        1 => ["3.0.0", "3.2.1"],
        3 => ["1.8.2", "1.0.0"],
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
