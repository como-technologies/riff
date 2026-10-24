//! `riff audit --wave` against a real server and a fake `gh`
//! (01M3ZWRC6H0P7EF6CMECCYZ2RC): a good wave passes each rule, and a
//! log that breaks a rule gives a fail that names the records. Only the
//! owner and the admins read the log (01M3ZWRC11R5M9V1KTF05P240W), and
//! a post in the reply has only its mark (01M3ZWRC3XBFN8FJDGE8XWZ5EA).
//!
//! The server loads a test log: each record has a time in seconds after
//! [`BASE`]. The fake `gh` gives the waves, the issues and the pull
//! requests from files.

use crate::book;
use crate::common;

use std::process::{Command, Output};
use std::sync::Arc;
use std::time::Duration;

use isolated::Isolated;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::record::{
    By, Change, Claimed, Envelope, Member, PauseSet, Posted, Record, Released, Scope,
    SessionStarted,
};
use riff_core::wire::{Message, RiffState, StartReason};
use riff_state::log::{self, chunk_name};
use riff_state::store::{Memory, Store};
use serde_json::json;

/// The time of second 0 of the test log: 2026-10-01 00:00 UTC.
const BASE: u64 = 1_790_812_800_000;

/// A fake `gh`: it logs each call to `gh.log`, and answers from the
/// files of its directory.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/gh.log"
case "$1 $2" in
'issue list') cat "$dir/issues.json" ;;
'pr list') cat "$dir/pulls.json" ;;
'api repos/como-technologies/riff/milestones?state=all&per_page=100') cat "$dir/milestones.json" ;;
api\ repos/como-technologies/riff/commits/*)
    sha=${2#repos/como-technologies/riff/commits/}; sha=${sha%%/*}
    cat "$dir/statuses-$sha.json" ;;
*) echo "the fake gh does not know: $*" >&2; exit 1 ;;
esac
"#;

/// The time `seconds` after [`BASE`], as GitHub writes it.
fn iso(seconds: u64) -> String {
    let ms = i64::try_from(BASE + seconds * 1000).unwrap();
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn uri(session: &str) -> SessionUri {
    format!("riff://mike@thelio/como-technologies/riff?session={session}")
        .parse()
        .unwrap()
}

fn thread() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// A record at `second`, by `session`. The test log gives each record
/// its position when it writes it.
fn record(second: u64, session: &str, change: Change) -> Record {
    Record {
        envelope: Envelope {
            position: 0,
            written_at_ms: BASE + second * 1000,
            by: Some(By::Session(uri(session).who().clone())),
            command: None,
            call: None,
        },
        change,
    }
}

fn claimed(session: &str, item: &str) -> Claimed {
    Claimed {
        session: uri(session),
        thread: thread(),
        item: item.into(),
    }
}

fn claim(second: u64, session: &str, item: &str) -> Record {
    record(second, session, Change::Claimed(claimed(session, item)))
}

fn release(second: u64, session: &str, item: &str) -> Record {
    let released = Released {
        must_clear: true,
        ..Released::of(claimed(session, item))
    };
    record(second, session, Change::Released(released))
}

fn started(second: u64, session: &str, reason: StartReason) -> Record {
    let started = SessionStarted {
        session: uri(session),
        reason,
        worker: true,
    };
    record(second, session, Change::SessionStarted(started))
}

fn post(second: u64, session: &str, to: &str, body: &str) -> Record {
    let from = uri(session);
    let posted = Posted {
        thread: ThreadName::direct(from.who(), uri(to).who()),
        message: Message {
            seq: second,
            from,
            to: Vec::new(),
            body: body.into(),
            at_ms: BASE + second * 1000,
            kind: Default::default(),
            sig: None,
            payload: None,
        },
        woken: Default::default(),
    };
    record(second, session, Change::Posted(Box::new(posted)))
}

fn pause(second: u64, state: RiffState) -> Record {
    let set = PauseSet {
        scope: Scope::Riff,
        state,
    };
    record(second, "l1", Change::PauseSet(set))
}

/// A good wave: the lead l1 asks w1 for issue 7; w1 asks for a verify
/// and releases; w2 verifies; w1 clears and takes issue 8 after the
/// merge of 7; w2 clears and verifies it too.
fn good() -> Vec<Record> {
    let lead = Member {
        session: uri("l1"),
        thread: thread(),
    };
    vec![
        record(10, "l1", Change::LeadSet(lead)),
        started(11, "w1", StartReason::Process),
        started(12, "w2", StartReason::Process),
        post(13, "l1", "w1", "request: claim issue-7"),
        claim(14, "w1", "issue-7"),
        post(15, "w1", "l1", "verify request: issue-7, PR #40"),
        release(16, "w1", "issue-7"),
        claim(17, "w2", "verify-issue-7"),
        release(18, "w2", "verify-issue-7"),
        started(31, "w1", StartReason::Clear),
        claim(32, "w1", "issue-8"),
        post(33, "w1", "l1", "verify request: issue-8, PR #41"),
        release(34, "w1", "issue-8"),
        started(35, "w2", StartReason::Clear),
        claim(36, "w2", "verify-issue-8"),
        release(37, "w2", "verify-issue-8"),
    ]
}

/// The good wave with `records` in their place by time.
fn with(records: Vec<Record>) -> Vec<Record> {
    let mut all = good();
    all.extend(records);
    all.sort_by_key(|r| r.envelope.written_at_ms);
    all
}

/// A real server of this build that loads `records` as its log, with
/// sign-in ([`common::start_signed_at`]). Its owner is brett. The
/// commands of mike are signed in by [`Machine`].
async fn server(records: Vec<Record>) -> common::Signed {
    let (listener, url) = common::listen().await;
    serve_split(records, 0, listener, &url).await.0
}

/// As [`server`], for a server whose public address is `public`, for
/// example a proxy in front of it, and that listens on `listener`.
async fn server_at(
    records: Vec<Record>,
    listener: tokio::net::TcpListener,
    public: &str,
) -> common::Signed {
    serve_split(records, 0, listener, public).await.0
}

/// As [`server`], with the log in two chunks when `split` is not 0: the
/// first `split` records, and the rest. It gives the store too.
async fn serve_split(
    records: Vec<Record>,
    split: usize,
    listener: tokio::net::TcpListener,
    url: &str,
) -> (common::Signed, Memory) {
    let store = Memory::default();
    let records: Vec<Record> = records
        .into_iter()
        .zip(1..)
        .map(|(mut r, position)| {
            r.envelope.position = position;
            r
        })
        .collect();
    let (first, rest) = records.split_at(split);
    for chunk in [first, rest].into_iter().filter(|c| !c.is_empty()) {
        riff_state::log::write(&store, chunk, &Default::default(), || true)
            .await
            .unwrap();
    }
    let lease = riff_server::lease::Timing {
        wait: Duration::from_millis(50),
        read_every: Duration::from_millis(50),
        valid_for: Duration::from_millis(500),
        exit_after: Duration::from_secs(1),
        ..Default::default()
    };
    let loaded = (Arc::new(store.clone()) as Arc<dyn Store>, lease);
    let server = common::start_signed_at(listener, url, Some(loaded)).await;
    (server, store)
}

/// A clone of `como-technologies/riff`, and the fake `gh` with the
/// facts of the forge: Wave 1 closed at 5 s; Wave 2 is open, with the
/// issues 7 and 8; 8 needs 7; 9 is in Wave 3. The pull requests 40 and
/// 41 are merged at 30 s and 60 s, each with a pass.
struct Machine {
    env: Isolated,
    /// Who the commands sign in as: a person of [`common::PEOPLE`].
    login: &'static str,
    bin: tempfile::TempDir,
    repo: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let bin = tempfile::tempdir().unwrap();
        let gh = bin.path().join("gh");
        isolated::write_program(&gh, FAKE_GH);
        let write = |name: &str, value: serde_json::Value| {
            std::fs::write(bin.path().join(name), value.to_string()).unwrap();
        };
        write(
            "milestones.json",
            json!([
                {"title": "Wave 1", "created_at": iso(0), "closed_at": iso(5)},
                {"title": "Wave 2", "created_at": iso(1), "closed_at": null},
                {"title": "Wave 3", "created_at": iso(1), "closed_at": null},
                {"title": "Backlog", "created_at": iso(0), "closed_at": null},
            ]),
        );
        write(
            "issues.json",
            json!([
                {"number": 7, "milestone": {"title": "Wave 2"}, "body": "Done when: x.",
                 "closedAt": iso(30), "comments": []},
                {"number": 8, "milestone": {"title": "Wave 2"}, "body": "Needs: #7\n\nDone when: x.",
                 "closedAt": null, "comments": [
                     {"body": "Merged in #41 (bbb)", "createdAt": iso(60)}
                 ]},
                {"number": 9, "milestone": {"title": "Wave 3"}, "body": "", "closedAt": null,
                 "comments": []},
            ]),
        );
        let body = |n: u64| format!("Closes #{n}\n\nText.\n\nIssue: #{n}\nMilestone: Wave 2\n");
        write(
            "pulls.json",
            json!([
                {"number": 40, "body": body(7), "headRefOid": "aaa", "mergedAt": iso(30)},
                {"number": 41, "body": body(8), "headRefOid": "bbb", "mergedAt": iso(60)},
            ]),
        );
        let pass = json!([{"context": "riff/verify", "state": "success"}]);
        write("statuses-aaa.json", pass.clone());
        write("statuses-bbb.json", pass);
        Machine {
            env: Isolated::new(),
            login: "mike",
            bin,
            repo: repo(),
        }
    }

    /// `riff ARGS` against `server`, with the fake `gh` first in `PATH`.
    fn riff(&self, server: &str, args: &[&str]) -> Command {
        common::sign_in(server, &self.env.riff_home(), self.login);
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SERVER", server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("PATH", path);
        cmd
    }

    async fn run(&self, server: &str, args: &[&str]) -> Output {
        let mut cmd = self.riff(server, args);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    /// `riff audit --wave "Wave 2"` on a server that loads `records`:
    /// whether it exits with 0, and its stdout.
    async fn audit(&self, records: Vec<Record>) -> (bool, String) {
        let url = server(records).await;
        let out = self.run(&url, &["audit", "--wave", "Wave 2"]).await;
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.is_empty(), "{stderr}");
        (out.status.success(), String::from_utf8(out.stdout).unwrap())
    }
}

/// A git repository with a GitHub origin, so the place is
/// `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
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
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// The lines of `text` that name a failure.
fn fails(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("fail: "))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_good_wave_passes_each_rule() {
    let machine = Machine::new();
    let (ok, text) = machine.audit(good()).await;
    assert!(ok, "{text}");
    assert!(
        text.starts_with(
            "Audit of Wave 2 in como-technologies/riff, from 2026-10-01 00:00 UTC to now.\n"
        ),
        "{text}"
    );
    for rule in 1..=7 {
        assert!(text.contains(&format!("\nRule {rule}: pass\n")), "{text}");
    }
    assert!(text.contains(
        "  pass: issue-7: claim at record 5 by mike/w1, PR #40, verify claim at record 8 by \
         mike/w2, merged, release at record 7.\n"
    ));
    assert!(text.ends_with("\nResult: each rule passes.\n"), "{text}");
    let calls = std::fs::read_to_string(machine.bin.path().join("gh.log")).unwrap();
    assert!(calls.contains("gh pr list --repo como-technologies/riff --state all"));
    assert!(calls.contains("--search milestone:\"Wave 2\""), "{calls}");
}

/// The wave has a name. `--wave` with its number finds it, and a wave
/// that matches no milestone is an error that names the open waves
/// (01M588K2M7G0CV3RNJ4N074FTG).
#[tokio::test(flavor = "multi_thread")]
async fn the_number_finds_a_wave_with_a_name() {
    let machine = Machine::new();
    for file in ["milestones.json", "issues.json"] {
        let path = machine.bin.path().join(file);
        let named = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"Wave 2\"", "\"Wave 2: Hosts\"");
        std::fs::write(&path, named).unwrap();
    }
    let url = server(good()).await;
    for wave in ["Wave 2", "2", "Wave 2: Hosts"] {
        let out = machine.run(&url, &["audit", "--wave", wave]).await;
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(out.status.success(), "{wave}: {text}");
        assert!(
            text.starts_with("Audit of Wave 2: Hosts in como-technologies/riff, from "),
            "{text}"
        );
        assert!(text.ends_with("\nResult: each rule passes.\n"), "{text}");
    }
    let calls = std::fs::read_to_string(machine.bin.path().join("gh.log")).unwrap();
    assert!(
        calls.contains("--search milestone:\"Wave 2: Hosts\""),
        "{calls}"
    );

    let out = machine.run(&url, &["audit", "--wave", "Wave 9"]).await;
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "The repository has no wave Wave 9. The open waves: Wave 2: Hosts, Wave 3. Give the \
             number of a wave, for example 2 or \"Wave 2\"."
        ),
        "{stderr}"
    );
}

/// The log of a later server has a kind that this build does not know.
/// `riff audit` skips that record, and the good wave still passes each
/// rule (01M43GSMZKCMET3DG07K538EDD).
#[tokio::test(flavor = "multi_thread")]
async fn a_record_of_a_later_kind_in_the_log_reply_is_skipped() {
    let machine = Machine::new();
    // A proxy in front of a real server. It forwards each call, and
    // puts a record of a later kind in the reply of the log.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let behind = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let real = format!("http://{}", behind.local_addr().unwrap());
    let _server = server_at(good(), behind, &proxy).await;
    let later = json!({"position": 2, "written_at_ms": BASE + 10_500, "command": "plan",
        "change": {"plan_set": {"thread": "como-technologies/riff", "items": []}}});
    let forward = move |request: axum::extract::Request| {
        let (real, later) = (real.clone(), later.clone());
        async move {
            let (parts, body) = request.into_parts();
            let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
            let url = format!("{real}{}", parts.uri);
            let mut out = reqwest::Client::new().request(parts.method, url).body(body);
            for (name, value) in &parts.headers {
                if name != "host" {
                    out = out.header(name, value);
                }
            }
            let reply = out.send().await.unwrap();
            let status = reply.status();
            let headers = reply.headers().clone();
            let mut bytes = reply.bytes().await.unwrap().to_vec();
            if parts.uri.path() == "/v1/log" && status.is_success() {
                let mut log: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                log["records"].as_array_mut().unwrap().insert(1, later);
                bytes = serde_json::to_vec(&log).unwrap();
            }
            let mut answer = axum::response::Response::new(axum::body::Body::from(bytes));
            *answer.status_mut() = status;
            for (name, value) in &headers {
                if name != "content-length" && name != "transfer-encoding" {
                    answer.headers_mut().insert(name, value.clone());
                }
            }
            answer
        }
    };
    let app = axum::Router::new().fallback(forward);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let out = machine.run(&proxy, &["audit", "--wave", "Wave 2"]).await;
    let text = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}{stderr}");
    for rule in 1..=7 {
        assert!(text.contains(&format!("\nRule {rule}: pass\n")), "{text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_1_fails_a_release_before_the_merge() {
    // w1 is no worker, and it asks for no verify before its release.
    let records = good()
        .into_iter()
        .filter(|r| {
            r.envelope.written_at_ms != BASE + 11_000 && r.envelope.written_at_ms != BASE + 15_000
        })
        .collect();
    let (ok, text) = Machine::new().audit(records).await;
    assert!(!ok, "{text}");
    assert!(text.contains("\nRule 1: fail\n"), "{text}");
    assert_eq!(
        fails(&text),
        ["fail: issue-7: release at record 5 before the merge and before a verify result."]
    );
    assert!(text.ends_with("\nResult: rule 1 fails.\n"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_2_fails_a_verifier_that_is_the_author() {
    let (ok, text) = Machine::new()
        .audit(with(vec![
            started(38, "w1", StartReason::Clear),
            claim(39, "w1", "verify-issue-8"),
        ]))
        .await;
    assert!(!ok, "{text}");
    assert_eq!(
        fails(&text),
        ["fail: verify-issue-8 at record 18 by mike/w1: it is an author of issue-8."]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_3_fails_a_claim_with_no_clear() {
    let records = good()
        .into_iter()
        .filter(|r| r.envelope.written_at_ms != BASE + 31_000)
        .collect();
    let (ok, text) = Machine::new().audit(records).await;
    assert!(!ok, "{text}");
    assert_eq!(
        fails(&text),
        ["fail: issue-8 at record 10 by mike/w1: no clear after its last release at record 7."]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_4_fails_a_claim_while_paused() {
    let records = with(vec![
        pause(13, RiffState::Paused),
        pause(16, RiffState::Running),
    ]);
    let (ok, text) = Machine::new().audit(records).await;
    assert!(!ok, "{text}");
    assert_eq!(
        fails(&text),
        ["fail: issue-7 at record 6 by mike/w1: the riff was paused."]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_5_fails_a_later_wave_and_an_open_need() {
    let records = with(vec![claim(20, "w4", "issue-8"), claim(38, "w3", "issue-9")]);
    let (ok, text) = Machine::new().audit(records).await;
    assert!(!ok, "{text}");
    assert!(text.contains("\nRule 5: fail\n"), "{text}");
    let lines = fails(&text);
    assert!(lines.contains(&"fail: issue-8 at record 10 by mike/w4: its need #7 was open."));
    assert!(
        lines.contains(
            &"fail: issue-9 at record 18 by mike/w3: it is in Wave 3, and Wave 2 was open."
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_6_fails_a_claim_of_the_lead() {
    let (ok, text) = Machine::new()
        .audit(with(vec![claim(38, "l1", "verify-issue-8")]))
        .await;
    assert!(!ok, "{text}");
    assert!(text.contains("\nRule 6: fail\n"), "{text}");
    assert!(
        fails(&text)
            .contains(&"fail: verify-issue-8 at record 17 by mike/l1: it was the lead of mike."),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_7_fails_a_request_that_is_not_from_the_lead() {
    let (ok, text) = Machine::new()
        .audit(with(vec![post(38, "w1", "w2", "request: verify issue-8")]))
        .await;
    assert!(!ok, "{text}");
    assert_eq!(
        fails(&text),
        ["fail: request at record 17 from mike/w1: the lead of mike was mike/l1."]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_owner_and_the_admins_read_the_log() {
    let url = server(good()).await;
    let mut member = Machine::new();
    member.login = "bob";
    let out = member.run(&url, &["audit", "--wave", "Wave 2"]).await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("only the owner and the admins can read the log"),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_log_has_only_the_records_of_the_repository_and_no_body() {
    common::mock_keyring();
    let mut records = good();
    // A claim in another repository.
    records.push(record(
        40,
        "x1",
        Change::Claimed(Claimed {
            session: "riff://brett@heron/acme/app?session=x1".parse().unwrap(),
            thread: "acme/app".parse().unwrap(),
            item: "issue-1".into(),
        }),
    ));
    let url = server(records).await;
    let api = url.api("mike", None);
    let log = api.log(&thread()).await.unwrap();
    assert_eq!(log.records.len(), good().len());
    let bodies: Vec<&str> = log
        .records
        .iter()
        .filter_map(|r| match &r.change {
            Change::Posted(p) => Some(p.message.body.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(bodies, ["request", "verify request", "verify request"]);
}

/// A server whose log is in two chunks: the first `split` records of
/// `records`, and the rest. With `prune`, the checkpoint rule removed the
/// first chunk after the start, so the oldest chunk starts after 1.
async fn split_server(records: Vec<Record>, split: usize) -> (common::Signed, Memory) {
    let (listener, url) = common::listen().await;
    serve_split(records, split, listener, &url).await
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_prune_the_audit_reads_the_kept_log_and_says_what_it_does_not_see() {
    let machine = Machine::new();
    // The records 1 to 4 are gone: the oldest kept record is the claim
    // of issue-7 at 14 s, after the start of the span at 5 s.
    let (url, store) = split_server(good(), 4).await;
    store.delete(&chunk_name(1)).await.unwrap();
    assert_eq!(log::chunks(&store).await.unwrap()[0].0, 5);

    let out = machine.run(&url, &["audit", "--wave", "Wave 2"]).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.is_empty(), "{stderr}");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(
            "\nThe log keeps the records from record 5 on, from 2026-10-01 00:00 UTC. The \
             checkpoint rule removed the records before it: the audit does not see them.\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "  not checked: issue-7: release at record 7 after a verify request, and the start \
             of its session is not in the records that the log keeps.\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "Rule 4: not checked\n  No claim while the riff or the repository was paused.\n  \
             Not checked: the span starts before record 5, the oldest record that the log keeps.\n"
        ),
        "{text}"
    );
    assert!(
        text.ends_with("Result: no rule fails. 6 rules are not checked.\n"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_log_that_does_not_read_gives_the_client_no_fix_that_deletes_the_store() {
    let machine = Machine::new();
    let (url, store) = split_server(good(), 0).await;
    let broken = chunk_name(1000);
    store.save(&broken, b"[]".to_vec(), None).await.unwrap();

    let out = machine.run(&url, &["audit", "--wave", "Wave 2"]).await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("500 Internal Server Error"), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "the log did not read: cannot read the saved object {broken}"
        )),
        "{stderr}"
    );
    assert!(!stderr.contains("remove the old state"), "{stderr}");
    assert!(!stderr.contains("old format"), "{stderr}");
}

#[test]
fn the_book_has_the_how_to() {
    let page = book::page("development.md");
    assert!(page.contains("\n## Check that a wave followed the rules\n"));
    assert!(book::commands_in(&page).contains(&"riff audit --wave 18".to_owned()));
}
