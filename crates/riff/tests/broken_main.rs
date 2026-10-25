//! riff finds a broken `main` at once (#874): the look of the lead reads
//! the full check of `main` from a fake `gh`, against a real server
//! (01M51QEYKTD7ZEB0YA0C3MQTPN to 01M51QEYVQN74Q1VDTVHB0445Q). A failed
//! full check gives one message to the lead with the run and the failed jobs,
//! and riff files one fix item. The rollout gives the fix item first.
//! When the full check passes again, riff closes the fix item.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use riff::look::{self, MainWatch, PullWatch};
use riff::pr::Gh;
use riff_client::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, RiffState};

const REPO: &str = "como-technologies/riff";

const RUN: &str = "https://github.com/como-technologies/riff/actions/runs/38861280871";

/// A fake `gh` that logs each call to `gh.log` on one line. The file
/// `gate` holds the check runs of the head of the default branch, which
/// is `master`, and `jobs` the jobs of the run. The file `pulls` holds the
/// open pull requests.
/// `issue list` gives the file `issues`, and the file `with-fix` after
/// `issue create` and before `issue close`.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s' "$*" | tr '\n' ' ' >> "$dir/gh.log"
printf '\n' >> "$dir/gh.log"
case "$1 $2" in
  api*)
    case "$2" in
      repos/como-technologies/riff)
        case "$*" in
          *--jq*) echo master ;;
          *) echo '{"default_branch":"master"}' ;;
        esac ;;
      */rules/branches/master*) echo '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]' ;;
      */commits/master/check-runs*) cat "$dir/gate" ;;
      */actions/runs/38861280871/jobs*) cat "$dir/jobs" ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list")
    if [ -e "$dir/created" ] && [ ! -e "$dir/closed" ]; then
      cat "$dir/with-fix"
    else
      cat "$dir/issues"
    fi ;;
  "label create") ;;
  "issue create")
    while [ $# -gt 0 ]; do
      [ "$1" = "--body-file" ] && cp "$2" "$dir/body"
      shift
    done
    touch "$dir/created"
    echo "https://github.com/como-technologies/riff/issues/901" ;;
  "issue comment") ;;
  "issue close") touch "$dir/closed" ;;
  *) exit 1 ;;
esac
"#;

/// Issue 1 is free, and the pull request of issue 2 waits for a verify.
const ISSUES: &str = r#"[{"number":1,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// [`ISSUES`] and the fix item 901.
const WITH_FIX: &str = r#"[{"number":1,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":901,"body":"Done when: x.\nNeeds: nothing","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"broken-main"}]}]"#;

const PULLS: &str = r#"[{"number":40,"headRefName":"worktree-issue-2","headRefOid":"1a2b3c4d","isDraft":false,"statusCheckRollup":[]}]"#;

const JOBS: &str = r#"[{"name":"Clippy","conclusion":"failure"},
{"name":"Tests 1/4","conclusion":"failure"},
{"name":"Checks","conclusion":"success"},
{"name":"Gate","conclusion":"failure"}]"#;

// Unsigned on purpose: `look::once` posts as the person with the api of the lead session, so no one token can make its calls.
async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

fn thread() -> ThreadName {
    REPO.parse().unwrap()
}

/// The lead of mike in a running riff, with no unread message.
async fn lead(server: &str) -> (Api, SessionUri) {
    let api = Api::new(server);
    let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=l1"
        .parse()
        .unwrap();
    api.register(&lead).await.unwrap();
    crate::common::name_lead(&api, &lead).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.inbox(&lead, None, false).await.unwrap();
    (api, lead)
}

/// The fake `gh` in `dir`, with `main` broken at 5103311.
fn forge(dir: &Path) -> Arc<Gh> {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    for (name, text) in [
        ("issues", ISSUES),
        ("with-fix", WITH_FIX),
        ("pulls", PULLS),
        ("jobs", JOBS),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    gate(dir, "5103311abcdef", "failure");
    Arc::new(Gh::at(path))
}

/// The full check of the head `commit` of `main` ended with `conclusion`.
fn gate(dir: &Path, commit: &str, conclusion: &str) {
    let json = format!(
        r#"{{"check_runs":[{{"status":"completed","conclusion":"{conclusion}","head_sha":"{commit}","html_url":"{RUN}/job/1"}}]}}"#
    );
    std::fs::write(dir.join("gate"), json).unwrap();
}

/// The lines of the log of the fake `gh` that start with `call`.
fn calls(dir: &Path, call: &str) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with(call))
        .map(str::to_owned)
        .collect()
}

/// One look of the lead.
async fn look_at(
    api: &Api,
    lead: &SessionUri,
    gh: &Arc<Gh>,
    pulls: &mut PullWatch,
    main: &mut MainWatch,
) {
    look::once(api, lead, gh, Duration::from_secs(600), None, pulls, main)
        .await
        .unwrap();
}

/// Each unread post of the person to the lead, with its kind.
async fn posts(api: &Api, lead: &SessionUri) -> Vec<(Kind, String)> {
    api.inbox(lead, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message)
        .filter(|m| m.from.who().session().is_none())
        .map(|m| (m.kind, m.body))
        .collect()
}

/// The free work of the rollout, with no claim and no hold.
fn free_work(gh: &Gh) -> Vec<String> {
    riff::rollout::free_work(gh, REPO, &HashSet::new(), &HashSet::new()).unwrap()
}

/// A failed full check of a commit of `main` gives one message to the lead with
/// the run and the failed jobs, and riff files one fix item. The rollout
/// gives the fix item first, before a verify. When the full check passes
/// again, riff closes the free fix item and tells the lead in a note
/// (01M51QEYKTD7ZEB0YA0C3MQTPN, 01M51QEYPGD6W345MZ4F85C38F,
/// 01M51QEYS538XA58KDRY844T52, 01M51QEYVQN74Q1VDTVHB0445Q).
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_gate_of_main_tells_the_lead_once_and_the_rollout_gives_the_fix_item_first() {
    let server = start_server().await;
    let (api, lead) = lead(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let (mut pulls, mut main) = (PullWatch::default(), MainWatch::default());
    assert_eq!(free_work(&gh), ["verify-issue-2", "issue-1"]);

    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert_eq!(
        posts(&api, &lead).await,
        [(
            Kind::Message,
            format!(
                "main is broken: the full check failed at commit 5103311 ({RUN}). Failed jobs: Clippy, \
                 Tests 1/4. The fix item is #901: the rollout gives it to a worker before each \
                 other item. When a pull request already has the fix, hold #901."
            )
        )]
    );
    assert_eq!(calls(dir.path(), "label create broken-main").len(), 1);
    let created = calls(dir.path(), "issue create");
    assert_eq!(created.len(), 1, "{created:?}");
    assert!(
        created[0].contains("--title main is broken at 5103311: the full check failed")
            && created[0].contains("--label broken-main")
            && created[0].contains("--milestone Wave 1"),
        "{created:?}"
    );
    let body = std::fs::read_to_string(dir.path().join("body")).unwrap();
    assert!(
        body.contains(&format!("- Run: {RUN}\n- Failed jobs: Clippy, Tests 1/4\n"))
            && body.contains("\nNeeds: nothing\n")
            && body.contains("\nDone when: "),
        "{body}"
    );

    // The same failure at the next look: no message, and no second fix item.
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert!(posts(&api, &lead).await.is_empty());
    assert_eq!(calls(dir.path(), "issue create").len(), 1);
    assert!(calls(dir.path(), "issue comment").is_empty());

    // The rollout gives the fix item first.
    assert_eq!(free_work(&gh), ["issue-901", "verify-issue-2", "issue-1"]);

    // The fix merges, and the full check of main passes.
    gate(dir.path(), "c0ffee1234", "success");
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert_eq!(
        posts(&api, &lead).await,
        [(
            Kind::Note,
            format!(
                "main passes again: the full check passed at commit c0ffee1 ({RUN}). riff closed the \
                 fix item #901."
            )
        )]
    );
    let closed = calls(dir.path(), "issue close 901");
    assert_eq!(closed.len(), 1, "{closed:?}");
    assert_eq!(free_work(&gh), ["verify-issue-2", "issue-1"]);

    // main passes: no more post.
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert!(posts(&api, &lead).await.is_empty());
}

/// While main stays broken, a failed full check at a newer commit gives a note
/// and a comment on the open fix item, not a second item. A fix item that
/// a session claims stays open when the full check passes again
/// (01M51QEYKTD7ZEB0YA0C3MQTPN, 01M51QEYPGD6W345MZ4F85C38F,
/// 01M51QEYVQN74Q1VDTVHB0445Q).
#[tokio::test(flavor = "multi_thread")]
async fn a_newer_failed_gate_comments_on_the_fix_item_and_a_claimed_fix_item_stays_open() {
    let server = start_server().await;
    let (api, lead) = lead(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let (mut pulls, mut main) = (PullWatch::default(), MainWatch::default());
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    let first = posts(&api, &lead).await;
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].0, Kind::Message);

    // A full check that runs at the new head: nothing.
    std::fs::write(
        dir.path().join("gate"),
        r#"{"check_runs":[{"status":"in_progress","conclusion":null,"head_sha":"c0ffee1234"}]}"#,
    )
    .unwrap();
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert!(posts(&api, &lead).await.is_empty());

    gate(dir.path(), "c0ffee1234", "failure");
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert_eq!(
        posts(&api, &lead).await,
        [(
            Kind::Note,
            format!(
                "main is still broken: the full check failed at commit c0ffee1 ({RUN}) too. Failed \
                 jobs: Clippy, Tests 1/4. The fix item #901 has a comment with this run."
            )
        )]
    );
    let comments = calls(dir.path(), "issue comment 901");
    assert_eq!(comments.len(), 1, "{comments:?}");
    assert!(comments[0].contains("at commit c0ffee1"), "{comments:?}");
    assert_eq!(calls(dir.path(), "issue create").len(), 1);

    // A worker claims the fix item. Then the full check passes: the item stays.
    let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1#issue-901"
        .parse()
        .unwrap();
    api.register(&worker).await.unwrap();
    api.claim(&worker, &thread(), "issue-901").await.unwrap();
    gate(dir.path(), "d00d1234", "success");
    look_at(&api, &lead, &gh, &mut pulls, &mut main).await;
    assert_eq!(
        posts(&api, &lead).await,
        [(
            Kind::Note,
            format!(
                "main passes again: the full check passed at commit d00d123 ({RUN}). The fix item #901 \
                 stays open: a session claims it, or a pull request names it."
            )
        )]
    );
    assert!(calls(dir.path(), "issue close").is_empty());
}
