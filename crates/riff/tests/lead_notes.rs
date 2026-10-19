//! `riff lead notes` (01M59258NTXPMWV18TKRKWNTYN,
//! 01M59258K3TYR09HC4AWHWZPEX, 01M59258RJ3CG498Z8AVQ5TV5Q): the person
//! shows and edits the notes of the lead, through the `riff` binary
//! against a real server.

use std::path::Path;
use std::process::{Command as Git, Output};

use isolated::Isolated;

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so `riff` runs in
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
        let git = Git::new("git").args(args).current_dir(dir.path()).status();
        assert!(git.unwrap().success());
    }
    dir
}

/// Runs `riff` as the person `user`, with `editor` as `$EDITOR`.
async fn riff(server: &str, dir: &Path, user: &str, editor: &str, args: &[&str]) -> Output {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", user)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("EDITOR", editor)
        .env_remove("VISUAL")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The person shows the notes, edits them with an editor and from a
/// file, and sees the history. A secret and a text over the limit are
/// refused. Another user cannot show the notes.
#[tokio::test]
async fn the_person_shows_and_edits_the_notes_of_the_lead() {
    let server = start_server().await;
    let repo = repo();
    let dir = repo.path();

    let none = riff(&server, dir, "mike", "true", &["lead", "notes"]).await;
    assert!(none.status.success(), "{}", text(&none.stderr));
    assert!(text(&none.stdout).contains("mike has no notes for como-technologies/riff yet"));

    // The editor is a script that writes the text.
    let script = "#!/bin/sh\necho 'Wave 36 runs.' > \"$1\"\n";
    let editor = isolated::write_program(dir.join("editor.sh"), script);
    let edit = riff(
        &server,
        dir,
        "mike",
        editor.to_str().unwrap(),
        &["lead", "notes", "edit"],
    )
    .await;
    assert!(edit.status.success(), "{}", text(&edit.stderr));
    assert!(
        text(&edit.stdout).contains("are saved"),
        "{}",
        text(&edit.stdout)
    );

    let file = dir.join("notes.md");
    std::fs::write(&file, "Wave 37 runs.\nPrefer thelio.\n").unwrap();
    let path = file.to_str().unwrap();
    let from_file = riff(
        &server,
        dir,
        "mike",
        "true",
        &["lead", "notes", "edit", "--file", path],
    )
    .await;
    assert!(from_file.status.success(), "{}", text(&from_file.stderr));

    let shown = riff(&server, dir, "mike", "true", &["lead", "notes"]).await;
    let shown = text(&shown.stdout);
    assert!(shown.contains("Wave 37 runs.\nPrefer thelio."), "{shown}");
    assert!(!shown.contains("Wave 36"), "{shown}");
    let history = riff(
        &server,
        dir,
        "mike",
        "true",
        &["lead", "notes", "--history"],
    )
    .await;
    let history = text(&history.stdout);
    assert!(
        history.contains("Before:") && history.contains("Wave 36 runs."),
        "{history}"
    );

    // The same text again.
    let same = riff(
        &server,
        dir,
        "mike",
        "true",
        &["lead", "notes", "edit", "--file", path],
    )
    .await;
    assert!(text(&same.stdout).contains("have this text already"));

    // A token and a text over the limit.
    std::fs::write(&file, "token: sk-ant-oat01-abcdefghijklmnop\n").unwrap();
    let secret = riff(
        &server,
        dir,
        "mike",
        "true",
        &["lead", "notes", "edit", "--file", path],
    )
    .await;
    assert!(!secret.status.success());
    let error = text(&secret.stderr);
    assert!(error.contains("looks like it has a secret"), "{error}");
    assert!(!error.contains("sk-ant"), "{error}");
    std::fs::write(&file, "a".repeat(64 * 1024 + 1)).unwrap();
    let long = riff(
        &server,
        dir,
        "mike",
        "true",
        &["lead", "notes", "edit", "--file", path],
    )
    .await;
    assert!(
        text(&long.stderr).contains("64 KiB"),
        "{}",
        text(&long.stderr)
    );

    // Another user reads no notes of mike.
    let other = riff(
        &server,
        dir,
        "brett",
        "true",
        &["lead", "notes", "--user", "mike"],
    )
    .await;
    assert!(!other.status.success());
    assert!(
        text(&other.stderr).contains("only mike and the owner"),
        "{}",
        text(&other.stderr)
    );
    let own = riff(&server, dir, "brett", "true", &["lead", "notes"]).await;
    assert!(text(&own.stdout).contains("brett has no notes"));
}
