//! The host follows no link in the Claude folder of a session or a lead
//! (#2172, 01M7FE5FYFAR92PM3EJJ211VM2): the container writes that
//! folder, and the paths are the same in the container and on the host.
//! A link there to a file of the person gives an error that names the
//! link. The file of the person does not change, and its text does not
//! come into the folder.

use std::path::{Path, PathBuf};

use riff::permissions::{Role, Trust};

/// The Claude folder of a lead in `root`, with each name of `links` a
/// link to the file of the person: a sign-in file of `claude` in JSON,
/// with a fake token. A host that follows the link of `.claude.json`
/// would keep its keys in the folder.
struct Hostile {
    claude: PathBuf,
    person: PathBuf,
    token: String,
    text: String,
}

fn hostile(root: &Path, links: &[&str]) -> Hostile {
    let claude = root.join("leads/o-r/claude");
    std::fs::create_dir_all(&claude).unwrap();
    let person = root.join("home/.claude/.credentials.json");
    std::fs::create_dir_all(person.parent().unwrap()).unwrap();
    let token = isolated::fake_secret();
    let text = serde_json::json!({"claudeAiOauth": {"accessToken": token}}).to_string();
    std::fs::write(&person, &text).unwrap();
    for name in links {
        std::os::unix::fs::symlink(&person, claude.join(name)).unwrap();
    }
    Hostile {
        claude,
        person,
        token,
        text,
    }
}

/// Runs [`riff::run::claude_folder`] on `claude` for the lead.
fn claude_folder(claude: &Path) -> anyhow::Result<()> {
    riff::run::claude_folder(
        claude,
        &claude.with_file_name("clone"),
        Path::new("/nowhere/.claude.json"),
        &Trust::default(),
        Role::Lead,
    )
}

/// Each plain file in `dir` and its folders, with its text.
fn texts(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            all.extend(texts(&path));
        } else if kind.is_file() {
            all.push((path.clone(), std::fs::read_to_string(&path).unwrap()));
        }
    }
    all
}

/// The file of the person does not change, and no file of the Claude
/// folder has its token.
fn kept(hostile: &Hostile) {
    assert_eq!(
        std::fs::read_to_string(&hostile.person).unwrap(),
        hostile.text
    );
    for (path, text) in texts(&hostile.claude) {
        assert!(!text.contains(&hostile.token), "{}: {text}", path.display());
    }
}

/// The three files that `claude_folder` reads and writes are links to a
/// file of the person: the host refuses the first one that it meets.
#[test]
fn the_host_follows_no_link_of_the_three_files_of_a_claude_folder() {
    let root = tempfile::tempdir().unwrap();
    let links = [".claude.json.riff", ".claude.json", "settings.json"];
    let hostile = hostile(root.path(), &links);
    let error = claude_folder(&hostile.claude).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        riff::text::host_file_link(&hostile.claude.join(".claude.json"))
    );
    kept(&hostile);
}

/// Each one of the three files alone as a link: an error that names it.
#[test]
fn each_link_alone_gives_an_error_that_names_it() {
    for name in [".claude.json.riff", ".claude.json", "settings.json"] {
        let root = tempfile::tempdir().unwrap();
        let hostile = hostile(root.path(), &[name]);
        let claude = &hostile.claude;
        let error = claude_folder(claude).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            riff::text::host_file_link(&claude.join(name)),
            "{name}"
        );
        kept(&hostile);
        assert!(
            std::fs::symlink_metadata(claude.join(name))
                .unwrap()
                .file_type()
                .is_symlink(),
            "{name}"
        );
    }
}

/// With the link gone, the next start writes the folder.
#[test]
fn the_start_after_the_remove_of_the_link_writes_the_folder() {
    let root = tempfile::tempdir().unwrap();
    let hostile = hostile(root.path(), &["settings.json"]);
    let claude = &hostile.claude;
    assert!(claude_folder(claude).is_err());
    std::fs::remove_file(claude.join("settings.json")).unwrap();
    claude_folder(claude).unwrap();
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(claude.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings, riff::run::settings(&Trust::default(), Role::Lead));
    kept(&hostile);
}

/// A FIFO in the place of `.claude.json`: the start does not wait for a
/// writer, and gives an error that names it.
#[test]
fn a_fifo_in_the_claude_folder_gives_an_error_at_once() {
    let root = tempfile::tempdir().unwrap();
    let hostile = hostile(root.path(), &[]);
    let fifo = hostile.claude.join(".claude.json");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let error = claude_folder(&hostile.claude).unwrap_err();
    assert_eq!(format!("{error:#}"), riff::text::host_file_not_plain(&fifo));
}

/// The inode of the file `path`: [`riff::host_file::write`] makes a new
/// file at each write.
fn inode(path: &Path) -> u64 {
    std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(path).unwrap())
}

/// A sparse file of 64 GiB in the place of `.claude.json` or of
/// `settings.json` (01M7FNTSHXPQXXKZF761V0NN9Z): the start reads none of
/// it, gives an error that names it at once, and writes no file. With the
/// file gone, the next start writes the folder.
#[test]
fn a_sparse_config_over_the_limit_gives_an_error_that_names_it() {
    for (name, other) in [
        (".claude.json", "settings.json"),
        ("settings.json", ".claude.json"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let hostile = hostile(root.path(), &[]);
        let claude = &hostile.claude;
        claude_folder(claude).unwrap();
        let (big, other) = (claude.join(name), claude.join(other));
        std::fs::File::create(&big)
            .unwrap()
            .set_len(64 << 30)
            .unwrap();
        let before = inode(&other);
        let error = claude_folder(claude).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            riff::text::host_file_too_large(&big, riff::run::MAX_CONFIG),
            "{name}"
        );
        assert_eq!(std::fs::metadata(&big).unwrap().len(), 64 << 30, "{name}");
        assert_eq!(inode(&other), before, "riff wrote {}", other.display());
        std::fs::remove_file(&big).unwrap();
        claude_folder(claude).unwrap();
        assert!(std::fs::metadata(&big).unwrap().len() < 1 << 20, "{name}");
        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(claude.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(config["hasCompletedOnboarding"], true, "{name}");
    }
}

/// The how-to "When a session makes a large file in its folder" in the
/// book (01M7FNTSHXPQXXKZF761V0NN9Z): the row of the files of the
/// machine points to it, it names each limit of riff, its error is the
/// error of riff, and its commands find the error and the file.
#[test]
fn the_book_says_what_to_do_with_a_large_file_in_the_folder_of_the_lead() {
    let page = crate::book::page("how-it-works.md");
    let how_to = &crate::book::how_to(
        "how-it-works.md",
        "#### When a session makes a large file in its folder",
    );
    let error = riff::text::host_file_too_large(
        Path::new("/home/mike/.local/share/riff/leads/acme-app/claude/.claude.json"),
        riff::run::MAX_CONFIG,
    );
    assert!(how_to.contains(&error), "{how_to}");
    for (limit, words) in [
        (riff::usage::MAX_TRANSCRIPT, "| 256 MB |"),
        (riff::run::MAX_CONFIG, "| 16 MB |"),
        (riff::lead::MAX_ALTERNATES, "| 1 MB |"),
    ] {
        let mb = words.trim_matches(['|', ' ']).trim_end_matches(" MB");
        assert_eq!(limit, mb.parse::<u64>().unwrap() << 20, "{words}");
        assert!(how_to.contains(words), "{words}");
    }
    assert_eq!(riff::usage::MAX_ENTRIES, 10_000);
    assert!(how_to.contains("| 10,000 entries |"), "{how_to}");
    let too_many = riff::text::host_file_too_many(Path::new("/p"), riff::usage::MAX_ENTRIES);
    assert!(too_many.starts_with("riff lists at most"), "{too_many}");
    assert!(error.starts_with("riff reads at most"), "{error}");
    assert_eq!(
        crate::book::commands_in(how_to),
        [
            "journalctl --user -u riff | grep -E 'reads at most|lists at most'",
            "find ~/.local/share/riff/leads/OWNER-REPO/claude -size +16M",
            "rm ~/.local/share/riff/leads/OWNER-REPO/claude/.claude.json",
        ]
    );
    assert!(
        page.contains("(#when-a-session-makes-a-large-file-in-its-folder)"),
        "the row of the files of the machine points to the how-to"
    );
}

/// The how-to "When a session puts a link in its folder" in the book: the
/// row of the files of the machine points to it, its error is the error
/// of riff, it finds the error in the log of the daemon, and its commands
/// are real. Only the daemon starts a lead: `riff lead start` starts
/// none, so the how-to points to the start of the lead by the daemon.
/// There, a new daemon starts the lead at once: `riff daemon restart`
/// starts no new daemon when the daemon runs the riff on disk.
#[test]
fn the_book_says_what_to_do_with_a_link_in_the_folder_of_the_lead() {
    let page = crate::book::page("how-it-works.md");
    let how_to = &crate::book::how_to(
        "how-it-works.md",
        "#### When a session puts a link in its folder",
    );
    let error = riff::text::host_file_link(Path::new(
        "/home/mike/.local/share/riff/leads/acme-app/claude/settings.json",
    ));
    assert!(how_to.contains(&error), "{how_to}");
    assert!(error.contains("follows no link"), "{error}");
    assert!(
        how_to.contains("(#when-the-lead-fails-to-start)"),
        "the how-to points to the wait of the daemon"
    );
    assert_eq!(
        crate::book::commands_in(how_to),
        [
            "riff daemon log --grep 'follows no link'",
            "find ~/.local/share/riff/leads/OWNER-REPO/claude -type l",
            "rm ~/.local/share/riff/leads/OWNER-REPO/claude/settings.json",
        ]
    );
    let wait = &crate::book::how_to("how-it-works.md", "#### When the lead fails to start");
    assert_eq!(
        crate::book::commands_in(wait).last().map(String::as_str),
        Some("systemctl --user restart riff"),
        "{wait}"
    );
    assert_eq!(
        riff::text::daemon_restart(riff::daemon::remote::Restart::Newest, "1"),
        "The riff daemon runs the riff on disk, build 1. It does not start again."
    );
    assert!(
        page.contains("(#when-a-session-puts-a-link-in-its-folder)"),
        "the row of the files of the machine points to the how-to"
    );
}
