//! `riff questions` (01M6QBJJ3R5CNTHT4FAPZ5DDSW, 01M6QQJHV8THJG36XFT95S3Q84,
//! 01M6QQJHXNAVYNNJ86WYY3C8EG, #1605, #1666): the open
//! issues with the label `question` in each repository of the person at
//! the terminal, from a real riff-server and a fake `gh`. The sessions of
//! another person add no repository, and a repository with the notes of a
//! lead and no session does. `--web` opens one GitHub search in the
//! browser, here a fake `xdg-open`.

use isolated::Isolated;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::common;
use riff_core::name::SessionUri;
use riff_core::record::{Change, Envelope, NotesSet, Record};
use riff_state::store::{Memory, Store};
use std::sync::Arc;

/// A git repository with a GitHub origin, so the folder is in
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

/// A dir for `PATH` with `git` and a fake `gh`. The fake `gh` answers `issue
/// list --label question` for each repository of `answers`, with its
/// text, fails with a 404 for each repository of `broken`, and fails for
/// each repository of `off` as `gh` does when it turned its issues off.
/// It writes each call in the file `calls` ([`asked`]).
fn bin(answers: &[(&str, &str)], broken: &[&str], off: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    let mut script = String::from("#!/bin/sh\ncase \"$*\" in\n");
    script.push_str("  *\"--label question\"*) ;;\n  *) echo 'no label' >&2; exit 1 ;;\nesac\n");
    // The fake `gh` has no `sed` and no `dirname`: the path is in the script.
    let calls = dir.path().join("calls");
    script.push_str(&format!("echo \"$*\" >> {}\n", calls.display()));
    script.push_str("case \"$*\" in\n");
    for repo in broken {
        script.push_str(&format!(
            "  *\"--repo {repo} \"*) echo 'HTTP 404: Not Found' >&2; exit 1 ;;\n"
        ));
    }
    for repo in off {
        script.push_str(&format!(
            "  *\"--repo {repo} \"*) echo \"the '{repo}' repository has disabled issues\" >&2; exit 1 ;;\n"
        ));
    }
    for (repo, issues) in answers {
        script.push_str(&format!("  *\"--repo {repo} \"*) echo '{issues}' ;;\n"));
    }
    script.push_str("  *) echo '[]' ;;\nesac\n");
    isolated::write_program(dir.path().join("gh"), script);
    dir
}

/// The repositories that the fake `gh` of `dir` got, in order.
fn asked(dir: &Path) -> Vec<String> {
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap_or_default();
    calls
        .lines()
        .filter_map(|line| {
            let mut words = line.split(' ');
            words.by_ref().find(|w| *w == "--repo")?;
            words.next().map(str::to_owned)
        })
        .collect()
}

/// A fake `xdg-open` in `dir`: it writes its argument in the file `opened`.
fn browser(dir: &Path) {
    let opened = dir.join("opened");
    let script = format!("#!/bin/sh\nprintf '%s' \"$1\" > {}\n", opened.display());
    isolated::write_program(dir.join("xdg-open"), script);
}

/// `riff questions ARGS` in `dir` through a pipe, as the person brett,
/// with `path` as `PATH`.
fn riff(server: &str, dir: &Path, path: &Path, args: &[&str]) -> Command {
    common::sign_in(server, dir, "brett");
    let mut cmd = Isolated::shared().riff();
    cmd.arg("questions")
        .args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("PATH", path)
        .env("RIFF_USER", "brett")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// The output of `cmd`, after its exit.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// Opens a watch for the session `uri`, so that the session is live
/// until the test ends.
async fn live(server: &str, uri: &str) {
    let uri: SessionUri = uri.parse().unwrap();
    let api = common::api_of(server, &uri);
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&uri).await.unwrap();
        open.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    opened.await.unwrap();
}

/// A real server that loads a log with the notes of a lead of brett in
/// each of `repos`, and no session. The session of a lead can be gone and
/// forgotten while its notes stay.
async fn server_with_notes(repos: &[&str]) -> common::Signed {
    let store = Memory::default();
    let records: Vec<Record> = repos
        .iter()
        .zip(1..)
        .map(|(repo, position)| Record {
            envelope: Envelope {
                position,
                written_at_ms: 1_790_812_800_000 + position * 1000,
                by: None,
                command: None,
                call: None,
            },
            change: Change::NotesSet(NotesSet {
                thread: repo.parse().unwrap(),
                user: "brett".into(),
                text: "Prefer thelio for builds.".into(),
            }),
        })
        .collect();
    riff_state::log::write(&store, &records, &Default::default(), || true)
        .await
        .unwrap();
    let lease = riff_server::lease::Timing {
        wait: Duration::from_millis(50),
        read_every: Duration::from_millis(50),
        valid_for: Duration::from_millis(500),
        exit_after: Duration::from_secs(1),
        ..Default::default()
    };
    let loaded = (Arc::new(store) as Arc<dyn Store>, lease);
    let (listener, url) = common::listen().await;
    common::start_signed_at(listener, &url, Some(loaded), |router| router).await
}

const RIFF: &str = r#"[{"number":12,"title":"Which design?","url":"https://github.com/como-technologies/riff/issues/12"},
{"number":9,"title":"Ship it?","url":"https://github.com/como-technologies/riff/issues/9"}]"#;
const OTHER: &str =
    r#"[{"number":3,"title":"Who owns it?","url":"https://github.com/o/other/issues/3"}]"#;
const PALETTE: &str = r#"[{"number":5,"title":"Which libvirt?","url":"https://github.com/como-technologies/palette-edge-libvirt/issues/5"},
{"number":4,"title":"Which edge?","url":"https://github.com/como-technologies/palette-edge-libvirt/issues/4"}]"#;
const MIKE: &str =
    r#"[{"number":8,"title":"Not for brett","url":"https://github.com/o/mikes/issues/8"}]"#;

/// The list has one line for each open issue with the label of each
/// repository of the person: the repository of the folder, and the
/// repository of a session in another repository. A repository with no
/// question adds no line. A session of another person adds no
/// repository: `gh` never reads it.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_lists_the_issues_with_the_label_of_each_repository_of_the_person() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(
        &[
            ("como-technologies/riff", RIFF),
            ("o/other", OTHER),
            ("o/mikes", MIKE),
        ],
        &[],
        &[],
    );
    live(&server, "riff://brett@thelio/o/other?session=b2").await;
    live(&server, "riff://brett@thelio/o/quiet?session=c3").await;
    live(&server, "riff://mike@thelio/o/mikes?session=m1").await;
    live(&server, "riff://mike@thelio/o/other?session=m2").await;

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "como-technologies/riff  #9  Ship it?  https://github.com/como-technologies/riff/issues/9\n\
         como-technologies/riff  #12  Which design?  https://github.com/como-technologies/riff/issues/12\n\
         o/other  #3  Who owns it?  https://github.com/o/other/issues/3\n"
    );
    assert_eq!(stderr, "");
    assert_eq!(
        asked(bin.path()),
        ["como-technologies/riff", "o/other", "o/quiet"]
    );
}

/// A repository with the notes of a lead of the person and no session
/// is read: riff-server keeps the notes after it forgets the session.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_reads_a_repository_with_the_notes_of_a_lead_and_no_session() {
    let server = server_with_notes(&["como-technologies/palette-edge-libvirt"]).await;
    let dir = repo();
    let bin = bin(
        &[
            ("como-technologies/riff", RIFF),
            ("como-technologies/palette-edge-libvirt", PALETTE),
        ],
        &[],
        &[],
    );

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains(
            "como-technologies/palette-edge-libvirt  #4  Which edge?  \
             https://github.com/como-technologies/palette-edge-libvirt/issues/4\n\
             como-technologies/palette-edge-libvirt  #5  Which libvirt?"
        ),
        "{stdout}"
    );
    assert_eq!(
        asked(bin.path()),
        [
            "como-technologies/palette-edge-libvirt",
            "como-technologies/riff"
        ]
    );
}

/// No open question: one line that says so.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_says_when_no_issue_has_the_label() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[], &[], &[]);
    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "No open question in your repositories.\n"
    );
}

/// A repository with its issues turned off is no error: no line on
/// stderr, and the command exits with 0.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_skips_a_repository_with_its_issues_off() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[("como-technologies/riff", RIFF)], &[], &["o/off"]);
    live(&server, "riff://brett@thelio/o/off?session=b2").await;

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(stderr, "");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("como-technologies/riff  #12  Which design?"),
        "{stdout}"
    );
    assert!(asked(bin.path()).contains(&"o/off".to_owned()));
}

/// Two repositories that `gh` cannot read give one line on stderr, at the
/// end, with both names. The list shows the others, and the command exits
/// with 0.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_names_the_repositories_that_gh_cannot_read_in_one_line() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(
        &[("como-technologies/riff", RIFF)],
        &["o/gone", "o/lost"],
        &["o/off"],
    );
    live(&server, "riff://brett@thelio/o/gone?session=b2").await;
    live(&server, "riff://brett@thelio/o/lost?session=b3").await;
    live(&server, "riff://brett@thelio/o/off?session=b4").await;

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{stderr}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("como-technologies/riff  #12  Which design?"),
        "{stdout}"
    );
    assert_eq!(
        stderr,
        "riff could not read 2 repositories: o/gone, o/lost\n"
    );
}

/// When `gh` fails for each repository, there is nothing to list: the
/// command names them and exits with 1.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_exits_with_1_when_gh_cannot_read_any_repository() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[], &["como-technologies/riff"], &[]);

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "riff could not read 1 repository: como-technologies/riff\n"
    );
}

/// `--web` opens one search for all the repositories of the person, and
/// lists nothing. The test shows the address that reaches the browser:
/// the repository of a session of the person, the repository with the
/// notes of a lead and no session, and none of another person.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_web_opens_one_search_over_the_repositories() {
    let server = server_with_notes(&["o/notes"]).await;
    let dir = repo();
    let bin = bin(&[], &[], &[]);
    browser(bin.path());
    live(&server, "riff://brett@thelio/o/other?session=b2").await;
    live(&server, "riff://mike@thelio/o/mikes?session=m1").await;

    let out = run(riff(&server, dir.path(), bin.path(), &["--web"])).await;
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "");
    let url = "https://github.com/issues?q=is%3Aissue+is%3Aopen+label%3Aquestion\
               +repo%3Acomo-technologies%2Friff+repo%3Ao%2Fnotes+repo%3Ao%2Fother";
    assert_eq!(stderr, format!("riff: opening {url} in your browser.\n"));
    // The browser starts detached: wait for its file.
    let opened = bin.path().join("opened");
    let until = Instant::now() + Duration::from_secs(10);
    while !opened.exists() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read_to_string(opened).unwrap(), url);
    assert!(asked(bin.path()).is_empty());
}
