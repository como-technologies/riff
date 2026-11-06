//! `riff questions` (01M6QBJJ3R5CNTHT4FAPZ5DDSW, #1605): the open issues
//! with the label `question` in each repository of the riff, from a
//! fake `gh`. `--web` opens one GitHub search in the browser, here a fake
//! `xdg-open`.

use isolated::Isolated;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::common;
use riff_core::name::SessionUri;

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
/// text, and fails for the repository `broken`.
fn bin(answers: &[(&str, &str)], broken: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    let mut script = String::from("#!/bin/sh\ncase \"$*\" in\n");
    script.push_str("  *\"--label question\"*) ;;\n  *) echo 'no label' >&2; exit 1 ;;\nesac\n");
    script.push_str("case \"$*\" in\n");
    if let Some(repo) = broken {
        script.push_str(&format!(
            "  *\"--repo {repo} \"*) echo 'HTTP 404: Not Found' >&2; exit 1 ;;\n"
        ));
    }
    for (repo, issues) in answers {
        script.push_str(&format!("  *\"--repo {repo} \"*) echo '{issues}' ;;\n"));
    }
    script.push_str("  *) echo '[]' ;;\nesac\n");
    isolated::write_program(dir.path().join("gh"), script);
    dir
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

/// Opens a watch for the session `uri` of mike, so that the session is
/// live until the test ends.
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

const RIFF: &str = r#"[{"number":12,"title":"Which design?","url":"https://github.com/como-technologies/riff/issues/12"},
{"number":9,"title":"Ship it?","url":"https://github.com/como-technologies/riff/issues/9"}]"#;
const OTHER: &str =
    r#"[{"number":3,"title":"Who owns it?","url":"https://github.com/o/other/issues/3"}]"#;

/// The list has one line for each open issue with the label of each
/// repository of the riff: the repository of the folder, and the
/// repository of a session in another repository. A repository with no
/// question adds no line.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_lists_the_issues_with_the_label_of_each_repository_of_the_riff() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(
        &[("como-technologies/riff", RIFF), ("o/other", OTHER)],
        None,
    );
    live(&server, "riff://mike@thelio/o/other?session=b2").await;
    live(&server, "riff://mike@thelio/o/quiet?session=c3").await;

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
}

/// No open question: one line that says so.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_says_when_no_issue_has_the_label() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[], None);
    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "No open question in the repositories of the riff.\n"
    );
}

/// A repository that `gh` cannot read gets one line on stderr. The list
/// shows the others, and the command exits with 1.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_names_a_repository_that_gh_cannot_read() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[("como-technologies/riff", RIFF)], Some("o/gone"));
    live(&server, "riff://mike@thelio/o/gone?session=b2").await;

    let out = run(riff(&server, dir.path(), bin.path(), &[])).await;
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("como-technologies/riff  #12  Which design?"),
        "{stdout}"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("riff: cannot read the questions of o/gone: ") && stderr.contains("404"),
        "{stderr}"
    );
}

/// `--web` opens one search for all the repositories of the riff, and
/// lists nothing. The test shows the address that reaches the browser.
#[tokio::test(flavor = "multi_thread")]
async fn riff_questions_web_opens_one_search_over_the_repositories() {
    let server = common::start_signed().await;
    let dir = repo();
    let bin = bin(&[], None);
    browser(bin.path());
    live(&server, "riff://mike@thelio/o/other?session=b2").await;

    let out = run(riff(&server, dir.path(), bin.path(), &["--web"])).await;
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "");
    let url = "https://github.com/issues?q=is%3Aissue+is%3Aopen+label%3Aquestion\
               +repo%3Acomo-technologies%2Friff+repo%3Ao%2Fother";
    assert_eq!(stderr, format!("riff: opening {url} in your browser.\n"));
    // The browser starts detached: wait for its file.
    let opened = bin.path().join("opened");
    let until = Instant::now() + Duration::from_secs(10);
    while !opened.exists() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read_to_string(opened).unwrap(), url);
}
