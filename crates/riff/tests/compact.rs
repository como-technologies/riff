//! riff compacts the lead at the end of a wave
//! (01M3Q88G1K7N2EMPBA07X069A7 to 01M3Q88GBSRJRP4VGVDV3EJZ4R). A fake
//! `gh` on `PATH` gives a done wave with its release out.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::wire::{Kind, RiffState};

/// A fake `gh`: Wave 13 is done, its release v0.8.0 is merged and
/// deployed, and no pull request is open.
const FAKE_GH: &str = r#"#!/bin/sh
echo "gh $*" >> "$(dirname "$0")/gh.log"
case "$*" in
  *milestones*) echo '[{"title":"Wave 13","open_issues":0,"closed_issues":9},{"title":"Wave 14","open_issues":2,"closed_issues":0}]' ;;
  "issue list"*) echo '[{"title":"Release v0.8.0"}]' ;;
  "pr list"*merged*) echo '[{"title":"Release v0.8.0"}]' ;;
  "pr list"*) echo '[]' ;;
  "run list"*) echo '[{"conclusion":"success"}]' ;;
  *) exit 1 ;;
esac
"#;

const ENDED: &str = r#"{"type":"user","message":{"content":"Wave 13 is closed."}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Wave 13 is done."}]}}
{"type":"system","subtype":"stop_hook_summary"}
{"type":"system","subtype":"turn_duration"}
"#;

struct Lead {
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    repo: tempfile::TempDir,
    transcript: std::path::PathBuf,
    server: String,
}

impl Lead {
    fn new(server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        let gh = fake.path().join("gh");
        std::fs::write(&gh, FAKE_GH).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        let transcript = fake.path().join("transcript.jsonl");
        std::fs::write(&transcript, ENDED).unwrap();
        let lead = Lead {
            fake,
            run: tempfile::tempdir().unwrap(),
            repo,
            transcript,
            server: server.into(),
        };
        let out = lead
            .riff(&["lead", "compact", "--quiet", "0"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        lead
    }

    /// A riff command of the session `l1`, outside tmux.
    fn riff(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("RIFF_SESSION", "l1")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env_remove("RIFF_WORKER")
            .env_remove("CLAUDE_CODE_SESSION_ID");
        cmd
    }

    /// One check, as the Stop hook starts it.
    fn check(&self) -> Output {
        let transcript = self.transcript.to_str().unwrap();
        let out = self
            .riff(&[
                "hook",
                "compact",
                "--session",
                "l1",
                "--transcript",
                transcript,
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(out.stderr.is_empty(), "{out:?}");
        out
    }

    fn uri(&self, id: &str) -> SessionUri {
        let place = identity::place_in(self.repo.path(), "thelio").unwrap();
        SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
    }

    fn record(&self) -> String {
        let path = self.run.path().join("state/compact-como-technologies-riff");
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A paused riff with the lead `l1`.
async fn riff_with_a_lead() -> (Api, Lead) {
    let api = start_server().await;
    let lead = Lead::new(api.base());
    api.register(&lead.uri("l1")).await.unwrap();
    api.set_riff(&lead.uri("l1"), RiffState::Paused)
        .await
        .unwrap();
    (api, lead)
}

/// The bodies of the unread messages of `me`. It reads them.
async fn unread(api: &Api, me: &SessionUri) -> Vec<String> {
    api.inbox(me, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message.body)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lead_posts_a_handoff_note_before_riff_compacts_it() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");

    lead.check();
    let asked = unread(&api, &me).await;
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].contains("handoff: Wave 13"), "{asked:?}");
    assert_eq!(lead.record(), "Wave 13\tasked\n");

    // No note yet: riff waits, and asks nothing again.
    lead.check();
    assert!(unread(&api, &me).await.is_empty());
    assert_eq!(lead.record(), "Wave 13\tasked\n");

    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let posted = api
        .post(
            &me,
            Some(&thread),
            &[],
            "handoff: Wave 13. Next: Wave 14.",
            Kind::Note,
        )
        .await
        .unwrap();

    // With no pane, riff tells the lead to ask its user.
    lead.check();
    let told = unread(&api, &me).await;
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("/compact"), "{told:?}");
    assert!(
        told[0].contains(&format!("message {}", posted.seq)),
        "{told:?}"
    );
    assert_eq!(lead.record(), "Wave 13\tdone\n");

    // Only once for each wave.
    lead.check();
    assert!(unread(&api, &me).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_riff_asks_for_nothing() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");
    api.set_riff(&me, RiffState::Running).await.unwrap();
    unread(&api, &me).await;
    lead.check();
    assert!(unread(&api, &me).await.is_empty());
    assert_eq!(lead.record(), "");
    let log = std::fs::read_to_string(lead.fake.path().join("gh.log")).unwrap_or_default();
    assert_eq!(log, "", "a running riff asks gh nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_is_not_the_lead_asks_for_nothing() {
    let (api, lead) = riff_with_a_lead().await;
    let other = lead.uri("s2");
    api.register(&other).await.unwrap();
    let transcript = lead.transcript.to_str().unwrap();
    let out = lead
        .riff(&[
            "hook",
            "compact",
            "--session",
            "s2",
            "--transcript",
            transcript,
        ])
        .env("RIFF_SESSION", "s2")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(unread(&api, &lead.uri("l1")).await.is_empty());
    assert_eq!(lead.record(), "");
}

/// The Stop hook of the lead returns at once, and the check that it
/// starts asks for the note.
#[tokio::test(flavor = "multi_thread")]
async fn the_stop_hook_of_the_lead_starts_the_check() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");
    let start = Instant::now();
    let mut hook = lead
        .riff(&["hook", "stop"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = serde_json::json!({
        "session_id": "l1",
        "hook_event_name": "Stop",
        "transcript_path": lead.transcript,
    });
    std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.to_string().as_bytes()).unwrap();
    let out = hook.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );

    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let asked = unread(&api, &me).await;
        if !asked.is_empty() {
            assert!(asked[0].contains("handoff: Wave 13"), "{asked:?}");
            break;
        }
        assert!(Instant::now() < end, "no ask came");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[test]
fn lead_compact_shows_and_sets_the_setting() {
    let lead = Lead::new("http://127.0.0.1:9");
    let show = |args: &[&str]| {
        let out = lead.riff(args).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let shown = show(&["lead", "compact"]);
    assert!(shown.starts_with("lead.compact  true  ("), "{shown}");
    assert!(shown.contains("\nlead.quiet  0  ("), "{shown}");
    let off = show(&["lead", "compact", "off"]);
    assert!(off.starts_with("lead.compact  false  ("), "{off}");
    let on = show(&["lead", "compact", "on", "--quiet", "90"]);
    assert!(on.starts_with("lead.compact  true  ("), "{on}");
    assert!(on.contains("\nlead.quiet  90  ("), "{on}");
}

/// A fake `tmux`: it logs each call, and `capture-pane` prints the file
/// `screen`.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1" in
  capture-pane) cat "$dir/screen" ;;
  *) printf '%s\n' "$*" >> "$dir/tmux.log" ;;
esac
"#;

impl Lead {
    /// Puts a fake `tmux` on `PATH` whose pane shows `input` in the
    /// input line of Claude Code.
    fn in_tmux(&self, input: &str) {
        let tmux = self.fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let screen = format!("Wave 13 is done.\n\n────────\n❯ {input}\n────────\n  riff l1\n");
        std::fs::write(self.fake.path().join("screen"), screen).unwrap();
    }

    fn typed(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("tmux.log")).unwrap_or_default()
    }

    /// One check in the pane `%3`.
    fn check_in_pane(&self) {
        let transcript = self.transcript.to_str().unwrap();
        let out = self
            .riff(&[
                "hook",
                "compact",
                "--session",
                "l1",
                "--transcript",
                transcript,
                "--pane",
                "%3",
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(out.stderr.is_empty(), "{out:?}");
    }
}

/// With all conditions true, riff types `/compact` with the
/// instructions into the pane of the lead, after the handoff note, and
/// once for the wave.
#[tokio::test(flavor = "multi_thread")]
async fn riff_types_the_compact_into_the_pane_of_the_lead_once() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");
    lead.in_tmux("");

    lead.check_in_pane();
    assert!(unread(&api, &me).await[0].contains("handoff: Wave 13"));
    assert_eq!(lead.typed(), "", "nothing types before the note");

    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let posted = api
        .post(
            &me,
            Some(&thread),
            &[],
            "handoff: Wave 13. Next: Wave 14.",
            Kind::Note,
        )
        .await
        .unwrap();
    lead.check_in_pane();
    let typed = lead.typed();
    let lines: Vec<&str> = typed.lines().collect();
    assert_eq!(lines.len(), 2, "{typed}");
    assert!(
        lines[0].starts_with(
            "send-keys -t %3 -l /compact You are the riff lead of mike in como-technologies/riff."
        ),
        "{typed}"
    );
    assert!(
        lines[0].contains(&format!("(message {})", posted.seq)),
        "{typed}"
    );
    assert_eq!(lines[1], "send-keys -t %3 Enter");
    assert!(unread(&api, &me).await.is_empty(), "no message with a pane");

    lead.check_in_pane();
    assert_eq!(lead.typed().lines().count(), 2, "only once for each wave");
}

/// riff never types into a half-written prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_half_written_prompt_stops_the_compact() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");
    lead.in_tmux("");
    lead.check_in_pane();
    unread(&api, &me).await;
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    api.post(&me, Some(&thread), &[], "handoff: Wave 13.", Kind::Note)
        .await
        .unwrap();

    lead.in_tmux("fix the te");
    lead.check_in_pane();
    assert_eq!(lead.typed(), "");
    assert_eq!(lead.record(), "Wave 13\tasked\n");

    lead.in_tmux("");
    lead.check_in_pane();
    assert_eq!(lead.typed().lines().count(), 2);
}

/// Two turns of the lead end in the quiet time, so two checks run at
/// the same time. Only one acts: one ask, and one `/compact`.
#[tokio::test(flavor = "multi_thread")]
async fn two_checks_at_the_same_time_act_once() {
    let (api, lead) = riff_with_a_lead().await;
    let me = lead.uri("l1");
    lead.in_tmux("");
    let transcript = lead.transcript.to_str().unwrap().to_owned();
    let both = |lead: &Lead| {
        let spawn = || {
            lead.riff(&[
                "hook",
                "compact",
                "--session",
                "l1",
                "--transcript",
                &transcript,
                "--pane",
                "%3",
            ])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
        };
        let (a, b) = (spawn(), spawn());
        for child in [a, b] {
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success(), "{out:?}");
            assert!(out.stderr.is_empty(), "{out:?}");
        }
    };

    both(&lead);
    assert_eq!(unread(&api, &me).await.len(), 1, "one ask");

    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    api.post(&me, Some(&thread), &[], "handoff: Wave 13.", Kind::Note)
        .await
        .unwrap();
    both(&lead);
    let typed = lead.typed();
    assert_eq!(typed.matches("/compact").count(), 1, "{typed}");
    assert_eq!(typed.lines().count(), 2, "{typed}");
}
