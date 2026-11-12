//! `riff questions` (01M6QBJJ3R5CNTHT4FAPZ5DDSW, 01M6QQJHV8THJG36XFT95S3Q84,
//! 01M6QQJHXNAVYNNJ86WYY3C8EG, 01M77F889Q3D6VCEDNNNXSWGNK, #1605, #1666,
//! #1864): the open issues with the label `question` in each project of
//! the person at the terminal, from a real riff-server and a fake `gh`.
//! The sessions of another person add no repository, a repository that
//! is no project adds none, and a project with the notes of a lead and
//! no session adds one. `--web` opens one GitHub search in the
//! browser, here a fake `xdg-open`. `--answered N` removes the label of
//! the issue N, and wakes the lead of the person (#1782,
//! 01M6XZ2T9TPZC1X36Y2ZXZ83C0).

use isolated::Isolated;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::common;
use riff_core::name::SessionUri;
use riff_core::record::{Change, NotesSet};

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
#[cfg(target_os = "linux")]
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

/// A real server that loads a log with each of `projects` as a project
/// of the riff, and the notes of a lead of brett in each of `notes`, with
/// no session. The session of a lead can be gone and forgotten while its
/// notes stay.
async fn server(projects: &[&str], notes: &[&str]) -> common::Signed {
    let notes = notes.iter().map(|repo| {
        Change::NotesSet(NotesSet {
            thread: repo.parse().unwrap(),
            user: "brett".into(),
            text: "Prefer thelio for builds.".into(),
        })
    });
    let changes = projects.iter().map(|repo| common::project(repo));
    common::start_signed_log(changes.chain(notes).collect()).await
}

const RIFF: &str = r#"[{"number":12,"title":"Which design?","url":"https://github.com/como-technologies/riff/issues/12"},
{"number":9,"title":"Ship it?","url":"https://github.com/como-technologies/riff/issues/9"}]"#;
const OTHER: &str =
    r#"[{"number":3,"title":"Who owns it?","url":"https://github.com/o/other/issues/3"}]"#;
const PALETTE: &str = r#"[{"number":5,"title":"Which libvirt?","url":"https://github.com/como-technologies/palette-edge-libvirt/issues/5"},
{"number":4,"title":"Which edge?","url":"https://github.com/como-technologies/palette-edge-libvirt/issues/4"}]"#;
const MIKE: &str =
    r#"[{"number":8,"title":"Not for brett","url":"https://github.com/o/mikes/issues/8"}]"#;

/// The projects of each test, also the repository of the folder.
const PROJECTS: &[&str] = &[
    "como-technologies/riff",
    "o/other",
    "o/quiet",
    "o/mikes",
    "o/off",
    "o/gone",
    "o/lost",
];

/// The list has one line for each open issue with the label of each
/// project of the person: the repository of the folder, and the
/// repository of a session in another repository. A repository with no
/// question adds no line. A session of another person adds no
/// repository: `gh` never reads it.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_lists_the_issues_with_the_label_of_each_repository_of_the_person() {
    let server = server(PROJECTS, &[]).await;
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

/// 01M77F889Q3D6VCEDNNNXSWGNK: a repository that a session of the person
/// names, and that is no project, gives no line: `gh` never reads it.
/// The folder of a repository that is no project adds none either.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_reads_no_repository_that_is_no_project() {
    let server = server(&["o/other"], &[]).await;
    let dir = repo();
    let bin = bin(
        &[
            ("como-technologies/riff", RIFF),
            ("o/other", OTHER),
            ("local/theme", MIKE),
        ],
        &[],
        &[],
    );
    live(&server, "riff://brett@thelio/o/other?session=b2").await;
    live(&server, "riff://brett@thelio/local/theme?session=b3").await;

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "o/other  #3  Who owns it?  https://github.com/o/other/issues/3\n"
    );
    assert_eq!(stderr, "");
    assert_eq!(asked(bin.path()), ["o/other"]);
}

/// A project with the notes of a lead of the person and no session is
/// read: riff-server keeps the notes after it forgets the session.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_reads_a_repository_with_the_notes_of_a_lead_and_no_session() {
    let palette = "como-technologies/palette-edge-libvirt";
    let server = server(&[palette, "como-technologies/riff"], &[palette]).await;
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
    let server = server(PROJECTS, &[]).await;
    let dir = repo();
    let bin = bin(&[], &[], &[]);
    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "No open question in your projects.\n"
    );
}

/// A repository with its issues turned off is no error: no line on
/// stderr, and the command exits with 0.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_skips_a_repository_with_its_issues_off() {
    let server = server(PROJECTS, &[]).await;
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
    let server = server(PROJECTS, &[]).await;
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
    let server = server(PROJECTS, &[]).await;
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
/// notes of a lead and no session, none of another person, and no
/// repository that is no project.
/// On macOS, riff opens the browser with `/usr/bin/open`, which no
/// fake on `PATH` replaces.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_web_opens_one_search_over_the_repositories() {
    use std::time::Duration;
    let server = server(&[PROJECTS, &["o/notes"]].concat(), &["o/notes"]).await;
    let dir = repo();
    let bin = bin(&[], &[], &[]);
    browser(bin.path());
    live(&server, "riff://brett@thelio/o/other?session=b2").await;
    live(&server, "riff://mike@thelio/o/mikes?session=m1").await;
    live(&server, "riff://brett@thelio/local/theme?session=b3").await;

    let out = run(riff(&server, dir.path(), bin.path(), &["--web"])).await;
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "");
    let url = "https://github.com/issues?q=is%3Aissue+is%3Aopen+label%3Aquestion\
               +repo%3Acomo-technologies%2Friff+repo%3Ao%2Fnotes+repo%3Ao%2Fother";
    assert_eq!(stderr, format!("riff: opening {url} in your browser.\n"));
    // The browser starts detached: wait for its file.
    let opened = bin.path().join("opened");
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while !opened.exists() && std::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read_to_string(opened).unwrap(), url);
    assert!(asked(bin.path()).is_empty());
}

/// A dir for `PATH` with `git` and a fake `gh` for `--answered`: `issue
/// view N --json labels` gives the label `question` for each issue of
/// `asked`, and only the label `bug` for each other. It takes each `issue
/// edit`. It writes each call in the file `calls` ([`issue_calls`]).
fn answering(asked: &[u64]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    let calls = dir.path().join("calls");
    let mut script = format!(
        "#!/bin/sh\necho \"$*\" >> {}\ncase \"$*\" in\n",
        calls.display()
    );
    for n in asked {
        script.push_str(&format!(
            "  \"issue view {n} \"*) echo '{{\"labels\":[{{\"name\":\"bug\"}},{{\"name\":\"question\"}}]}}' ;;\n"
        ));
    }
    script.push_str("  \"issue view \"*) echo '{\"labels\":[{\"name\":\"bug\"}]}' ;;\n");
    script.push_str("  \"issue edit \"*) ;;\n");
    script.push_str("  *) echo 'unknown call' >&2; exit 1 ;;\nesac\n");
    isolated::write_program(dir.path().join("gh"), script);
    dir
}

/// The calls `gh issue` that the fake `gh` of `dir` got, in order.
fn issue_calls(dir: &Path) -> Vec<String> {
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap_or_default();
    calls
        .lines()
        .filter(|line| line.starts_with("issue "))
        .map(str::to_owned)
        .collect()
}

/// The lead of brett in `repo`, named by the person, with no unread
/// message.
async fn lead_of_brett(server: &str, repo: &str) -> SessionUri {
    let lead: SessionUri = format!("riff://brett@thelio/{repo}?session=1ead")
        .parse()
        .unwrap();
    common::api_of(server, &lead).register(&lead).await.unwrap();
    common::name_lead(&common::api_of(server, &common::person_of(&lead)), &lead).await;
    common::api_of(server, &lead)
        .inbox(&lead, None, false)
        .await
        .unwrap();
    lead
}

/// Each unread message of the lead `lead` from a person: its sender and
/// its body.
async fn lead_reads(server: &str, lead: &SessionUri) -> Vec<(String, String)> {
    common::api_of(server, lead)
        .inbox(lead, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|inbox| inbox.messages)
        .map(|checked| checked.message)
        .filter(|m| m.from.who().session().is_none())
        .map(|m| (m.from.to_string(), m.body))
        .collect()
}

/// #1782: `riff questions --answered 12` in the clone removes the label
/// `question` of #12 with `gh`, and the message wakes the lead of the
/// person in the repository.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_answered_removes_the_label_and_wakes_the_lead() {
    let server = common::start_signed().await;
    let lead = lead_of_brett(&server, "como-technologies/riff").await;
    let dir = repo();
    let bin = answering(&[12]);

    let out = run(riff(&server, dir.path(), bin.path(), &["--answered", "12"])).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.starts_with(
            "Removed the label question of #12 of como-technologies/riff. \
             Posted message 1 to como-technologies/riff. Woke brett@thelio:riff (1ead)."
        ),
        "{stdout}"
    );
    assert_eq!(
        issue_calls(bin.path()),
        [
            "issue view 12 --repo como-technologies/riff --json labels",
            "issue edit 12 --repo como-technologies/riff --remove-label question",
        ]
    );
    assert_eq!(
        lead_reads(&server, &lead).await,
        [(
            "riff://brett@pangolin".to_owned(),
            "questions answered: brett answered the questions of #12 of como-technologies/riff, \
             and riff removed the label question. Record each answer in the body of #12."
                .to_owned()
        )]
    );
}

/// #1782: an issue with no label `question` gives a note. riff removes no
/// label and posts nothing.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_answered_on_an_issue_with_no_label_changes_nothing() {
    let server = common::start_signed().await;
    let lead = lead_of_brett(&server, "como-technologies/riff").await;
    let dir = repo();
    let bin = answering(&[12]);

    let out = run(riff(&server, dir.path(), bin.path(), &["--answered", "13"])).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "#13 of como-technologies/riff has no label question. Nothing changed.\n"
    );
    assert_eq!(
        issue_calls(bin.path()),
        ["issue view 13 --repo como-technologies/riff --json labels"]
    );
    assert!(lead_reads(&server, &lead).await.is_empty());
}

/// #1782: `--repo` names the repository in a folder that is no clone. With
/// no `--repo` there, the command says what to give, and calls no `gh`.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_answered_takes_the_repository_of_repo() {
    let server = common::start_signed().await;
    let lead = lead_of_brett(&server, "o/other").await;
    let dir = tempfile::tempdir().unwrap();
    let bin = answering(&[3]);

    let out = run(riff(&server, dir.path(), bin.path(), &["--answered", "3"])).await;
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("Give the repository with --repo OWNER/REPO."),
        "{stderr}"
    );
    assert!(issue_calls(bin.path()).is_empty());

    let args = ["--answered", "3", "--repo", "o/other"];
    let out = run(riff(&server, dir.path(), bin.path(), &args)).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        issue_calls(bin.path()),
        [
            "issue view 3 --repo o/other --json labels",
            "issue edit 3 --repo o/other --remove-label question",
        ]
    );
    let read = lead_reads(&server, &lead).await;
    assert_eq!(read.len(), 1, "{read:?}");
    assert!(
        read[0].1.contains("the questions of #3 of o/other"),
        "{read:?}"
    );
}

/// #1782: `y` on the question of `a` in the Waves tab runs the effect
/// `Answered`, and it does the same as `riff questions --answered`: the
/// label goes, and the lead of the person gets the message.
#[tokio::test(flavor = "multi_thread")]
async fn the_key_of_the_waves_tab_does_the_same_as_answered() {
    let server = common::start_signed().await;
    let lead = lead_of_brett(&server, "como-technologies/riff").await;
    let bin = answering(&[12]);
    let gh = riff::pr::Gh::at(bin.path().join("gh")).by("tui");
    let me: SessionUri = "riff://brett@pangolin".parse().unwrap();
    let api = common::api(&server, "brett", None);
    let reach = riff::reach::Reach::Direct(server.to_string());
    let effect = riff::tui::Effect::Answered {
        repo: "como-technologies/riff".into(),
        item: 12,
    };

    let asker = riff::proof::Asker::quiet();
    let line = effect.run(&api, &me, &reach, &gh, &asker).await.unwrap();
    assert!(
        line.starts_with("Removed the label question of #12 of como-technologies/riff."),
        "{line}"
    );
    assert_eq!(
        issue_calls(bin.path()),
        [
            "issue view 12 --repo como-technologies/riff --json labels",
            "issue edit 12 --repo como-technologies/riff --remove-label question",
        ]
    );
    let read = lead_reads(&server, &lead).await;
    assert_eq!(read.len(), 1, "{read:?}");
    assert!(
        read[0].1.starts_with("questions answered: brett"),
        "{read:?}"
    );
}
