//! "Start a Riff", the first page of the book for a person. It asks one
//! question first (01M3MN2R92DA7QPP80G1AENX4M). Its path "Just this
//! machine" has at most three commands (R4), each one real, and no
//! sign-in. No book page names the Cloud Run URL (R5). The
//! `riff-server` command of the path runs in
//! `crates/riff-server/tests/start_a_riff.rs`. "Join a Riff" is checked
//! in `join_a_riff.rs`.

use crate::book;

use isolated::Isolated;
use std::fs;
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

/// The install, the server and `riff`: three commands (R4).
#[test]
fn a_person_starts_a_riff_with_at_most_three_commands() {
    let mut commands = commands();
    commands.extend(commands_of_part(PAGE, "Start the riff"));
    assert_eq!(commands.len(), 3, "{commands:?}");
    assert_eq!(commands[1..], ["riff-server", "riff"]);
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
    let riff = commands_of_part(PAGE, "Start the riff");
    assert_eq!(riff, ["riff"]);
    each_is_real(&riff);
    let text = part(PAGE, "Start the riff");
    assert!(!text.contains("riff login"), "{text}");
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

/// The riff of this machine needs no sign-in: `riff who` works with
/// no `riff login`. The installed `riff` of the home stays the same
/// (01M3MRDEVR5VPPV6B1BDDVYSBG).
#[tokio::test]
async fn the_riff_of_this_machine_needs_no_sign_in() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("home/.cargo/bin/riff");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "#!/bin/sh\necho riff 0.1.0 release\n").unwrap();

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
            "riff cloud create NAME --project PROJECT --region REGION",
            "riff cloud signin NAME",
            "riff cloud forge NAME APP_ID ~/Downloads/riff.private-key.pem",
            "riff cloud deploy NAME",
            "riff cloud list",
            "riff cloud status NAME",
            "riff cloud log NAME --errors",
            "riff cloud delete NAME",
            "riff cloud list",
            "riff login",
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
