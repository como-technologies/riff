//! "Start a Riff", the first page of the book for a person. It asks one
//! question first (01M3MN2R92DA7QPP80G1AENX4M). Its path "Just this
//! machine" has at most three commands (R4), each one real, and no
//! sign-in. No book page names the Cloud Run URL (R5). The
//! `riff-server` command of the path runs in
//! `crates/riff-server/tests/start_a_riff.rs`. "Join a Riff" is checked
//! in `join_a_riff.rs`.

mod book;

use isolated::Isolated;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;

use book::{commands_of, commands_of_part, each_is_real, page, part};

const PAGE: &str = "start-a-riff.md";

/// The commands of "Just this machine".
fn commands() -> Vec<String> {
    commands_of_part(PAGE, "Just this machine")
}

/// The text of the page before its first `##` part.
fn introduction() -> String {
    let page = page(PAGE);
    page[..page.find("\n## ").unwrap()].to_owned()
}

#[test]
fn the_introduction_says_what_a_riff_is_and_that_riff_runs_on_linux_only() {
    let introduction = introduction().replace('\n', " ");
    assert!(
        introduction.contains(
            "A riff is the place where your sessions and the sessions of your team meet."
        ),
        "{introduction}"
    );
    assert!(introduction.contains("Linux only"), "{introduction}");
}

/// The question comes before the paths. Each answer links its page.
#[test]
fn the_page_asks_one_question_first() {
    let introduction = introduction();
    let question = introduction
        .find("Did a person give you a riff address?")
        .expect("the question");
    let answers = &introduction[question..];
    for link in [
        "(join-a-riff.md)",
        "(#just-this-machine)",
        "(start-a-team-riff.md)",
    ] {
        assert!(answers.contains(link), "{link}: {answers}");
    }
}

#[test]
fn the_page_names_no_insecure_no_systemd_and_no_install_subcommand() {
    let text = page(PAGE);
    for word in ["--insecure", "systemd", "riff-server install"] {
        assert!(!text.contains(word), "{PAGE} names {word}");
    }
}

#[test]
fn a_person_starts_a_riff_with_at_most_three_commands() {
    let commands = commands();
    assert!(!commands.is_empty());
    assert!(commands.len() <= 3, "{commands:?}");
}

#[test]
fn the_install_command_installs_riff_and_its_server_from_the_repository() {
    let install = &commands()[0];
    let words: Vec<&str> = install.split_whitespace().collect();
    assert_eq!(words[..3], ["cargo", "install", "--locked"], "{install}");
    let git = words.iter().position(|w| *w == "--git").unwrap();
    assert_eq!(words[git + 1], env!("CARGO_PKG_REPOSITORY"));
    assert_eq!(words[git + 2..], [env!("CARGO_PKG_NAME"), "riff-server"]);
}

#[test]
fn each_riff_command_of_the_page_is_real_and_none_signs_in() {
    let riff: Vec<String> = commands()
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(riff, ["riff connect claude"]);
    each_is_real(&riff);
}

/// A fake `claude` command in `dir`. It fails `mcp remove`, as `claude`
/// does when there is no old entry.
fn fake_claude(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("claude");
    fs::write(&path, "#!/bin/sh\n[ \"$1\" = mcp ] && exit 1\nexit 0\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Runs `riff ARGS` for the riff at `server`, away from the runtime of
/// the riff. The Claude Code settings go to `dir`, never to the real
/// home.
async fn riff(server: &str, dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "ada")
        .env("XDG_DATA_HOME", dir)
        .env("RIFF_HOME", dir)
        .env("HOME", dir.join("home"))
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// Step 3 of "Just this machine": at the riff of this machine, with no
/// sign-in, `riff connect claude` installs the plugin and signs in to
/// nothing. Then `riff who` works with no sign-in. The installed
/// `riff` of the home stays the same (01M3MRDEVR5VPPV6B1BDDVYSBG).
#[tokio::test]
async fn step_3_connects_to_the_riff_of_this_machine_with_no_sign_in() {
    assert_eq!(commands()[2], "riff connect claude");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let claude = fake_claude(dir.path());
    let installed = dir.path().join("home/.cargo/bin/riff");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "#!/bin/sh\necho riff 0.1.0 release\n").unwrap();

    let connect = riff(
        &server,
        dir.path(),
        &["connect", "claude", "--claude", claude.to_str().unwrap()],
    )
    .await;
    let stdout = String::from_utf8_lossy(&connect.stdout);
    let stderr = String::from_utf8_lossy(&connect.stderr);
    assert!(connect.status.success(), "{stdout}{stderr}");
    assert!(stdout.starts_with("Added the riff plugin"), "{stdout}");
    assert!(!stdout.contains("sign"), "{stdout}");
    assert_eq!(stderr, "");

    let who = riff(&server, dir.path(), &["who"]).await;
    assert!(
        who.status.success(),
        "{}",
        String::from_utf8_lossy(&who.stderr)
    );
    assert_eq!(
        fs::read_to_string(&installed).unwrap(),
        "#!/bin/sh\necho riff 0.1.0 release\n"
    );
    assert!(!installed.is_symlink());
}

/// "Update riff" of "Start a Riff" is one command.
#[test]
fn a_person_updates_riff_with_riff_update() {
    let update = commands_of_part(PAGE, "Update riff");
    assert_eq!(update, ["riff update"]);
    each_is_real(&update);
}

/// "Update riff" of "Start a Riff" says which release `riff update`
/// installs (01M3MRMAVVKJ5WS8GWCJHWH0R4): the newest release only for
/// the riff of this machine, and the release of the shared server for
/// a shared riff.
#[test]
fn update_riff_says_that_it_installs_the_release_of_the_riff() {
    let text = part(PAGE, "Update riff")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for words in [
        "It installs the release that your riff runs.",
        "That is not always the newest release (see [Releases](how-it-works.md#releases))",
        "The riff of this machine: it installs the newest release.",
        "A shared riff: it installs the release of the shared server.",
        "After a new release, update when the shared riff runs it.",
    ] {
        assert!(text.contains(words), "{words}");
    }
    assert!(
        !text.contains("installs the newest release of riff"),
        "{text}"
    );
    assert!(!text.contains("same build"), "{text}");
}

/// "Start a Team Riff" (01M3MEFG6F102T1H8DFJ38EJ4A): the owner uses the
/// riff, signs in first, then invites each person, and sees each change
/// of the members with `riff tail` (01M3MN14ZCTRVD3T455P6TFK1B). Each
/// `riff` command is real. The server part of the page runs in
/// `crates/riff-server/tests/start_a_team_riff.rs`.
#[test]
fn the_owner_of_a_team_riff_signs_in_then_invites() {
    let commands = commands_of("start-a-team-riff.md");
    assert!(
        commands.contains(&"echo 'export RIFF_SERVER=URL' >> ~/.bashrc".to_owned()),
        "{commands:?}"
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(
        riff,
        [
            "riff login",
            "riff enable --shared",
            "riff invite EMAIL",
            "riff tail",
            "riff owner --take",
            "riff owner EMAIL",
            "riff owner --deny",
            "riff who",
        ]
    );
    each_is_real(&riff);
    // The example note of the page is the note that riff posts.
    let invited = riff_core::wire::Invited {
        email: "bob@gmail.com".into(),
        address: "URL".into(),
    };
    let note = riff::text::invited_news("ada", &invited);
    assert!(page("start-a-team-riff.md").contains(&note), "{note}");
}

#[test]
fn no_book_page_names_a_cloud_run_url() {
    for entry in fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(".run.app"), "{}", path.display());
    }
}
