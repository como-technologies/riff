//! `riff members`, `riff who`, `riff whoami`, `riff session why` and
//! `riff run` in the container of the lead (#1338): with `RIFF_SOCKET`
//! and no `RIFF_SERVER`, each reads or asks through the socket of the
//! session, over a fake daemon socket.
//! Nothing listens at the riff-server of the machine: a command that
//! called it would fail. `riff audit` has its test in `audit.rs`.

use crate::common;

use std::process::{Command, Output};

use isolated::Isolated;
use serde_json::{Value, json};

/// A clone of a GitHub repository, in the environment of one test.
struct Container {
    env: Isolated,
    repo: tempfile::TempDir,
}

impl Container {
    fn new() -> Container {
        let repo = tempfile::tempdir().unwrap();
        for args in [
            &["init", "-q"][..],
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/como-technologies/riff.git",
            ],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .status();
            assert!(status.unwrap().success());
        }
        Container {
            env: Isolated::new(),
            repo,
        }
    }

    /// `riff ARGS` in the container: the socket of the session, the
    /// session ID, and no `RIFF_SERVER`.
    async fn riff(&self, socket: &str, args: &[&str]) -> Output {
        assert!(
            std::path::Path::new(socket).starts_with(std::env::temp_dir()),
            "RIFF_SOCKET of a test is not in a temp folder: {socket}"
        );
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SOCKET", socket)
            .env("RIFF_SESSION", "c1")
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env_remove("RIFF_SERVER");
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn members_reads_the_members_through_the_socket() {
    let container = Container::new();
    let reply = json!({
        "owner": "ada@example.com",
        "admins": ["grace@example.com"],
        "members": ["alan@example.com"],
        "allowed_domains": ["example.com"],
    });
    let (_dir, socket, asked) = common::session_socket(move |_| Ok(reply.clone()));
    let out = container.riff(&socket, &["members"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    let shown = text(&out.stdout);
    assert!(shown.contains("ada@example.com"), "{shown}");
    assert!(shown.contains("grace@example.com"), "{shown}");
    assert!(shown.contains("alan@example.com"), "{shown}");
    assert_eq!(common::call_names(&asked), ["members"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn members_shows_the_error_of_the_daemon() {
    let container = Container::new();
    let (_dir, socket, _) = common::session_socket(|_| Err("riff-server is away".into()));
    let out = container.riff(&socket, &["members"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("riff-server is away"),
        "{}",
        text(&out.stderr)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn session_why_reads_the_records_through_the_socket() {
    let container = Container::new();
    let body = |session: &str, code: i32| {
        format!(
            "ended item=issue-12 session={session} code={code} failed=yes denied=0: exit code \
             {code}\nthe last line"
        )
    };
    let records: Vec<_> = [("1a2b3c4d", 1), ("5e6f7a8b", 2)]
        .iter()
        .map(|(session, code)| riff::ends::Record::parse(&body(session, *code), 9).unwrap())
        .collect();
    let reply = serde_json::to_value(&records).unwrap();
    let (_dir, socket, asked) = common::session_socket(move |_| Ok(reply.clone()));

    let out = container
        .riff(&socket, &["session", "why", "issue-12"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    assert_eq!(
        text(&out.stdout),
        "issue-12: 2 sessions ended with no pull request. The last one, 5e6f7a8b, ended: exit \
         code 2.\nIts last 1 lines of output:\nthe last line\n"
    );
    let out = container
        .riff(&socket, &["session", "why", "issue-13"])
        .await;
    assert!(text(&out.stdout).starts_with("No session of issue-13 ended"));
    assert_eq!(common::call_names(&asked), ["ends", "ends"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_asks_the_daemon_through_the_socket() {
    let container = Container::new();
    let (_dir, socket, asked) = common::session_socket(|call| {
        assert_eq!(call["item"], "issue-12");
        Ok(Value::String(riff::text::run_asked("issue-12")))
    });
    let out = container.riff(&socket, &["run", "issue-12"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    assert_eq!(
        text(&out.stdout),
        format!("{}\n", riff::text::run_asked("issue-12"))
    );
    assert_eq!(common::call_names(&asked), ["run"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_in_a_container_takes_no_image_and_shows_a_refusal_of_the_daemon() {
    let container = Container::new();
    let (_dir, socket, asked) =
        common::session_socket(|_| Err(riff::text::DAEMON_RUN_ONLY_LEAD.into()));
    let out = container
        .riff(&socket, &["run", "issue-12", "--image", "localhost/x:y"])
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(riff::text::RUN_IMAGE_ON_HOST),
        "{}",
        text(&out.stderr)
    );
    assert!(asked.lock().unwrap().is_empty());

    let out = container.riff(&socket, &["run", "issue-12"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(riff::text::DAEMON_RUN_ONLY_LEAD),
        "{}",
        text(&out.stderr)
    );
}

const LEAD: &str = "riff://mike@thelio/como-technologies/riff?session=c1";

#[tokio::test(flavor = "multi_thread")]
async fn whoami_reads_the_session_through_the_socket() {
    let container = Container::new();
    let (_dir, socket, asked) = common::session_socket(|_| {
        Ok(json!({
            "state": {"Ok": {"state": "running"}},
            "uri": format!("{LEAD}&lead=1"),
        }))
    });
    let out = container.riff(&socket, &["whoami"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    let shown = text(&out.stdout);
    // The URI that the server holds, not the one that riff makes.
    assert!(shown.contains(&format!("{LEAD}&lead=1")), "{shown}");
    assert!(shown.contains("running"), "{shown}");
    assert_eq!(common::call_names(&asked), ["whoami"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn whoami_names_the_fault_of_a_read_and_keeps_the_uri_of_the_session() {
    let container = Container::new();
    let (_dir, socket, _) =
        common::session_socket(|_| Ok(json!({"state": {"Err": "refused"}, "uri": null})));
    let out = container.riff(&socket, &["whoami"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(shown.contains("unknown: refused"), "{shown}");
    assert!(shown.contains("?session=c1"), "{shown}");
}

#[tokio::test(flavor = "multi_thread")]
async fn who_reads_the_sessions_and_the_owner_through_the_socket() {
    let container = Container::new();
    let (_dir, socket, asked) = common::session_socket(|call| {
        assert_eq!(call["all"], true, "{call}");
        Ok(json!({
            "pauses": {"state": "paused", "riff": {"by": null, "at_ms": 0}},
            "who": {
                "owner": {"kind": "owner", "user": "ada", "email": "ada@example.com"},
                "sessions": [
                    {"uri": LEAD, "live": true, "container": true},
                    {
                        "uri": "riff://mike@thelio/como-technologies/riff?session=w1",
                        "live": false,
                    },
                ],
            },
        }))
    });
    let out = container.riff(&socket, &["who", "--all"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    let shown = text(&out.stdout);
    assert!(shown.contains("ada (ada@example.com)"), "{shown}");
    assert!(shown.contains("paused"), "{shown}");
    assert!(shown.contains("(c1)") && shown.contains("(w1)"), "{shown}");
    assert_eq!(common::call_names(&asked), ["who_facts"]);
}

/// #1430, 01M5SN566ZH5ZQ52NYT24WYA4P: `riff wave run` in the container of
/// the lead starts no wave:
/// the container has no daemon of its own. It names the hosts of the user
/// with a daemon that does not serve the repository, and `riff wave run
/// --host` as the way to add one.
#[tokio::test(flavor = "multi_thread")]
async fn wave_run_names_the_hosts_that_do_not_run_the_wave() {
    let container = Container::new();
    let daemon = |uri: &str| {
        json!({
            "uri": uri,
            "live": true,
            "status": {
                "step": "daemon host: pid 1, build 4.3.0, 0 sessions",
                "age_secs": 1,
                "stale": false,
            },
        })
    };
    let sessions = json!([
        daemon("riff://mike@thelio/como-technologies/riff?session=d1"),
        daemon("riff://mike@pangolin/como-technologies/other?session=d2"),
        daemon("riff://ada@minty/como-technologies/other?session=d3"),
        {"uri": LEAD, "live": true, "container": true},
    ]);
    let (_dir, socket, asked) = common::session_socket(move |call| match call["call"].as_str() {
        Some("roster") => {
            Ok(json!({"user": "mike", "owner": {"kind": "no_sign_in"}, "people": []}))
        }
        Some("who") => Ok(sessions.clone()),
        _ => Err(format!("unexpected call {call}")),
    });
    let out = container.riff(&socket, &["wave", "run"]).await;
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let hosts = ["pangolin".to_owned()];
    assert_eq!(
        text(&out.stderr).trim(),
        riff::text::wave_run_in_container("como-technologies/riff", &hosts)
    );
    assert_eq!(common::call_names(&asked), ["roster", "who"]);
}
