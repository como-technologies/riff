//! The refusals of auto mode of a session on the host
//! (01M57XY3Q6HEYQG27FE0CEQ7HB, 01M57XY44K2N3PS9F8NQ6SZQP9): `riff hook
//! denied` makes one record in the thread `denials` with its tags, and
//! `riff denials` counts them. A fake riff-server and a fake hook input:
//! no real config, no real keyring.

use isolated::Isolated;
use std::path::Path;
use std::process::{Command as Git, Output};
use std::time::Duration;

use riff_client::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::RiffState;

const LEAD: &str = "25456ff6-0000-4000-8000-000000000001";
const WORKER: &str = "7b7b7b7b-0000-4000-8000-000000000002";
const SECRET: &str = "gho_KnownSecret42";

/// A riff-server with no sign-in, and its URL.
async fn server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    url
}

/// A git repository with a GitHub origin.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ][..],
    ] {
        assert!(
            Git::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

fn me(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// `riff ARGS` in `dir` with `stdin`, as a process of Claude Code on the
/// host `pangolin`, with the home `home`. `worker` sets `RIFF_WORKER`.
async fn riff(
    server: &str,
    dir: &Path,
    home: &Path,
    worker: bool,
    stdin: &str,
    args: &[&str],
) -> Output {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", home)
        .env("GH_TOKEN", SECRET)
        .env_remove("RIFF_SESSION")
        .env_remove("RIFF_SOCKET")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .write_stdin(stdin);
    if worker {
        cmd.env("RIFF_WORKER", "1");
    } else {
        cmd.env_remove("RIFF_WORKER");
    }
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// The hook input of a refusal of `command` in the session `id`.
fn input(id: &str, command: &str) -> String {
    serde_json::json!({
        "session_id": id,
        "hook_event_name": "PermissionDenied",
        "permission_mode": "auto",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_use_id": "toolu_1",
        "reason": "[Production Deploy] pushes a tag",
    })
    .to_string()
}

/// The bodies of the thread `denials`, once it has `count` of them.
async fn bodies(api: &Api, reader: &SessionUri, count: usize) -> Vec<String> {
    let start = std::time::Instant::now();
    loop {
        let read: Vec<String> = riff::denied::read(api, reader, 0)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.body)
            .collect();
        if read.len() >= count {
            return read;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "{read:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_on_the_host_makes_one_record_and_riff_denials_counts_it() {
    let url = server().await;
    let api = Api::new(&url);
    let dir = repo();
    let home = tempfile::tempdir().unwrap();
    let reader: SessionUri = "riff://ann@heron/acme/app?session=r1".parse().unwrap();
    api.register(&reader).await.unwrap();
    api.register(&me(LEAD)).await.unwrap();
    api.lead(&me(LEAD)).await.unwrap();
    api.set_riff(&me(LEAD), RiffState::Running).await.unwrap();
    api.register_as(&me(WORKER), true).await.unwrap();
    let thread = me(WORKER).place().default_thread().unwrap();
    assert!(
        api.claim(&me(WORKER), &thread, "issue-7")
            .await
            .unwrap()
            .granted
    );

    // The lead: the hook returns at once, and the send posts the record.
    let command = format!("GH_TOKEN={SECRET} git push origin v3.2.0");
    let out = riff(
        &url,
        dir.path(),
        home.path(),
        false,
        &input(LEAD, &command),
        &["hook", "denied"],
    )
    .await;
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let read = bodies(&api, &reader, 1).await;
    assert_eq!(
        read,
        [format!(
            "denied reason=production-deploy tool=Bash head=\"git push\" role=lead user=mike \
             host=pangolin repo=como-technologies/riff session=25456ff6 claim=-: \
             GH_TOKEN={} git push origin v3.2.0",
            riff::run::HIDDEN
        )]
    );
    assert!(!read[0].contains(SECRET), "no secret goes to the riff");

    // A worker in tmux, with its claim.
    let out = riff(
        &url,
        dir.path(),
        home.path(),
        true,
        &input(WORKER, "nice -n 5 cargo publish"),
        &["hook", "denied"],
    )
    .await;
    assert!(out.status.success(), "{out:?}");
    let read = bodies(&api, &reader, 2).await;
    let record = riff::denied::Record::parse(&read[1], 0).unwrap();
    assert_eq!(record.tag("role"), "worker");
    assert_eq!(record.tag("claim"), "issue-7");
    assert_eq!(record.tag("head"), "cargo publish");

    // No session stays a member of the thread: its read shows no record.
    for id in [LEAD, WORKER] {
        let threads = api.threads(&me(id)).await.unwrap();
        assert!(
            threads
                .iter()
                .all(|t| t.thread.to_string() != riff::denied::THREAD
                    || t.members.iter().all(|m| m.who() != me(id).who())),
            "{threads:?}"
        );
    }

    // riff denials counts them.
    let out = riff(&url, dir.path(), home.path(), false, "", &["denials"]).await;
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("2 refusals of auto mode in the thread denials\n"),
        "{text}"
    );
    assert!(
        text.contains("By reason:\n  2  production-deploy\n"),
        "{text}"
    );
    assert!(
        text.contains("By role:\n  1  lead\n  1  worker\n"),
        "{text}"
    );
    assert!(text.contains("Last 2:\n"), "{text}");
    let out = riff(
        &url,
        dir.path(),
        home.path(),
        false,
        "",
        &["denials", "--since", "7d"],
    )
    .await;
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("2 refusals of auto mode in the last 7d\n"),
        "{text}"
    );
    let out = riff(
        &url,
        dir.path(),
        home.path(),
        false,
        "",
        &["denials", "--format", "json"],
    )
    .await;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["tags"]["role"], "lead");
    assert_eq!(json[1]["tags"]["claim"], "issue-7");
}

/// 01M57XY42CNR16M5G024PWKANV: when riff-server does not answer, the
/// local file keeps the record, and the next send posts it first.
#[tokio::test(flavor = "multi_thread")]
async fn a_record_waits_on_the_machine_while_riff_server_does_not_answer() {
    let url = server().await;
    let api = Api::new(&url);
    let dir = repo();
    let home = tempfile::tempdir().unwrap();
    let reader: SessionUri = "riff://ann@heron/acme/app?session=r1".parse().unwrap();
    api.register(&reader).await.unwrap();
    api.register(&me(LEAD)).await.unwrap();

    let dead = "http://127.0.0.1:9";
    let out = riff(
        dead,
        dir.path(),
        home.path(),
        false,
        &input(LEAD, "git push first"),
        &["hook", "denied"],
    )
    .await;
    assert!(out.status.success(), "{out:?}");
    let file = home.path().join("state").join(riff::denied::QUEUE);
    let start = std::time::Instant::now();
    while std::fs::read_to_string(&file).map_or(true, |t| !t.contains("git push first")) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the file keeps no record"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let out = riff(
        &url,
        dir.path(),
        home.path(),
        false,
        &input(LEAD, "git push second"),
        &["hook", "denied"],
    )
    .await;
    assert!(out.status.success(), "{out:?}");
    let read = bodies(&api, &reader, 2).await;
    assert!(read[0].ends_with(": git push first"), "{read:?}");
    assert!(read[1].ends_with(": git push second"), "{read:?}");
}
