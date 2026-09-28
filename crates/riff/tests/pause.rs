//! Pause and resume (01M3JCFTWCR72HQB8CBTQKXJNF to
//! 01M3JCG4AV80MHFP73CWDY5E3M), over real HTTP: a new riff is paused, a
//! pause wakes the sessions and keeps their claims, and a session with
//! work runs the WIP steps of the skill. They push its branch and
//! nothing to the default branch.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::RiffState;

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The lead of mike, a session of mike with work, and the person mike.
fn sessions() -> (SessionUri, SessionUri, SessionUri) {
    (
        uri("riff://mike@pangolin/como-technologies/riff?session=l1"),
        uri("riff://mike@pangolin/como-technologies/riff?session=w1#issue-12"),
        uri("riff://mike@pangolin/como-technologies/riff"),
    )
}

#[tokio::test]
async fn a_new_riff_is_paused_until_the_lead_resumes_it() {
    let api = start_server().await;
    let (lead, worker, _) = sessions();
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();

    assert_eq!(api.riff(&worker).await.unwrap(), RiffState::Paused);
    let error = api.claim(&worker, &repo(), "issue-12").await.unwrap_err();
    assert!(format!("{error:#}").contains("the riff is paused"));
    let error = api.set_riff(&worker, RiffState::Running).await.unwrap_err();
    assert!(format!("{error:#}").contains("403"), "{error:#}");

    let (reply, posted) = api.set_riff(&lead, RiffState::Running).await.unwrap();
    assert!(reply.changed);
    let woken: Vec<_> = posted.iter().flat_map(|p| &p.woken).collect();
    assert_eq!(woken.len(), 1);
    assert_eq!(woken[0].who(), worker.who());
    // The waiting session reads the news, then claims a free item.
    let news = api.read(&worker, &repo(), false).await.unwrap();
    assert!(news[0].message.body.contains("running again"), "{news:?}");
    assert!(
        api.claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );
}

#[tokio::test]
async fn a_pause_wakes_each_session_and_keeps_its_claims() {
    let api = start_server().await;
    let (lead, worker, mike) = sessions();
    let other = uri("riff://brett@heron/other/repo?session=b1");
    for me in [&lead, &worker, &other] {
        api.register(me).await.unwrap();
    }
    api.set_riff(&mike, RiffState::Running).await.unwrap();
    assert!(
        api.claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );

    let (reply, posted) = api.set_riff(&mike, RiffState::Paused).await.unwrap();
    assert!(reply.changed);
    let mut woken: Vec<_> = posted
        .iter()
        .flat_map(|p| &p.woken)
        .map(|uri| uri.who().session().unwrap().to_owned())
        .collect();
    woken.sort();
    assert_eq!(woken, ["b1", "l1", "w1"], "each repository gets the news");

    let who = api.who(&lead, false).await.unwrap();
    let held = who.iter().find(|s| s.uri.who() == worker.who()).unwrap();
    assert_eq!(held.uri.claims(), ["issue-12"], "the claim stays");
    assert!(api.claim(&lead, &repo(), "issue-13").await.is_err());
    // Messages still flow while the riff is paused.
    api.tell(&worker, "lead", "paused at: tests of issue-12")
        .await
        .unwrap();

    let (again, posted) = api.set_riff(&mike, RiffState::Paused).await.unwrap();
    assert!(!again.changed);
    assert!(posted.is_empty(), "no change wakes nobody");
    api.release(&worker, &repo(), "issue-12").await.unwrap();
}

/// `cmd` with no git settings of the user or the machine, for example
/// a signature for each commit.
fn alone(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "riff test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "riff test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = alone(Command::new("git").args(args).current_dir(dir))
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// The `sh` block of "A session with work" in the skill.
fn wip_steps() -> String {
    let skill = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    let part = &skill[skill.find("### A session with work").unwrap()..];
    let block = &part[part.find("```sh").unwrap() + "```sh".len()..];
    let block = &block[..block.find("```").unwrap()];
    block.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

fn run_wip_steps(dir: &Path) {
    let steps = wip_steps();
    let out = alone(
        Command::new("sh")
            .args(["-e", "-c", &steps])
            .current_dir(dir),
    )
    .output()
    .unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[tokio::test]
async fn a_session_with_work_pushes_a_wip_branch_and_keeps_its_claim() {
    let api = start_server().await;
    let (lead, worker, _) = sessions();
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    assert!(
        api.claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );

    // An origin with a default branch, and the worktree of the item.
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin.git");
    let work = tmp.path().join("work");
    fs::create_dir(&work).unwrap();
    git(
        tmp.path(),
        &["init", "-q", "--bare", "-b", "main", "origin.git"],
    );
    git(&work, &["init", "-q", "-b", "main"]);
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    fs::write(work.join("a.txt"), "one\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "start"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    let main = git(&origin, &["rev-parse", "main"]);
    git(&work, &["switch", "-q", "-c", "worktree-issue-12"]);
    fs::write(work.join("a.txt"), "one\ntwo\n").unwrap();
    fs::write(work.join("b.txt"), "new\n").unwrap();

    // The pause wakes the session. It runs the steps of the skill.
    let (_, posted) = api.set_riff(&lead, RiffState::Paused).await.unwrap();
    assert!(
        posted
            .iter()
            .flat_map(|p| &p.woken)
            .any(|u| u.who() == worker.who())
    );
    run_wip_steps(&work);

    let pushed = git(&origin, &["log", "-1", "--format=%s", "worktree-issue-12"]);
    assert_eq!(pushed, "WIP: the riff is paused");
    let files = git(
        &origin,
        &["show", "--name-only", "--format=", "worktree-issue-12"],
    );
    assert_eq!(files, "a.txt\nb.txt");
    assert_eq!(
        git(&origin, &["rev-parse", "main"]),
        main,
        "main is the same"
    );
    // With no new change, the steps commit nothing and still pass.
    run_wip_steps(&work);
    assert_eq!(
        git(&origin, &["rev-list", "--count", "worktree-issue-12"]),
        "2"
    );

    let who = api.who(&lead, false).await.unwrap();
    let held = who.iter().find(|s| s.uri.who() == worker.who()).unwrap();
    assert_eq!(held.uri.claims(), ["issue-12"]);
}

/// mike has live sessions on hosts `a` and `b`. A resume on `b` gives
/// the person of mike the host `b`. A pause on `a` then posts a note
/// from `mike@a` (01M3MWW8KYJ3ZV91X22RBSAF33).
#[tokio::test]
async fn a_pause_names_the_host_where_it_ran() {
    let api = start_server().await;
    let on_a = uri("riff://mike@a/como-technologies/riff?session=s1");
    let on_b = uri("riff://mike@b/como-technologies/riff?session=s2");
    for me in [&on_a, &on_b] {
        api.register(me).await.unwrap();
    }
    let person_on_b = uri("riff://mike@b/como-technologies/riff");
    api.set_riff(&person_on_b, RiffState::Running)
        .await
        .unwrap();

    let person_on_a = uri("riff://mike@a/como-technologies/riff");
    let (reply, _) = api.set_riff(&person_on_a, RiffState::Paused).await.unwrap();
    assert!(reply.changed);

    let news = api.read(&on_b, &repo(), false).await.unwrap();
    let senders: Vec<String> = news.iter().map(|m| m.message.from.short()).collect();
    assert_eq!(senders, ["mike@b:riff", "mike@a:riff"], "{news:?}");
    assert!(news[1].message.body.contains("paused"), "{news:?}");
}
