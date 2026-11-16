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
    for (name, other, most) in [
        (".claude.json", "settings.json", riff::run::MAX_CONFIG),
        ("settings.json", ".claude.json", riff::run::MAX_SETTINGS),
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
            riff::text::config_too_large(&big, most),
            "{name}"
        );
        assert!(format!("{error:#}").ends_with("Remove it, and start again."));
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
/// book (01M7FNTSHXPQXXKZF761V0NN9Z, 01M7FX6SWN3DGZMH5HS9BWEBY7,
/// 01M7FX6SZ88S06N1AT6WJHKNMX, 01M7FX6YQKTN9EVPTZJ9XB8PM4,
/// 01M7GF9G7ZNJD2SXZZ62XKJYBG, 01M7GF9QGV86BENYWT1TDJPM64): the row of
/// the files of the machine points to it, it names each limit of riff
/// and the escape of a name in the log, and its command finds each line
/// of a limit. Its two parts show the error and the line of riff, and
/// their commands remove the file and attach to the lead for a question
/// of `claude`, or move the entries of the folder. Only the error of a
/// start gives a step: the line of the lead gives none,
/// also for a link.
#[test]
fn the_book_says_what_to_do_with_a_large_file_in_the_folder_of_the_lead() {
    let page = crate::book::page("how-it-works.md");
    let how_to = &crate::book::how_to(
        "how-it-works.md",
        "#### When a session makes a large file in its folder",
    );
    let start = &crate::book::how_to(
        "how-it-works.md",
        "##### When the start of the lead stops at a large file",
    );
    let unseen = &crate::book::how_to(
        "how-it-works.md",
        "##### When riff sees no turn of the lead",
    );
    let lead = Path::new("/home/mike/.local/share/riff/leads/acme-app");
    let error =
        riff::text::config_too_large(&lead.join("claude/.claude.json"), riff::run::MAX_CONFIG);
    assert!(start.contains(&error), "{start}");
    let projects = lead.join("claude").join(riff::usage::PROJECTS);
    let cause = riff::text::host_file_too_many(&projects, riff::usage::MAX_ENTRIES);
    let line = format!(
        "riff daemon: {}: {}",
        riff::lead::name_of("acme/app"),
        riff::text::lead_turns_unseen(&cause)
    );
    assert!(unseen.contains(&line), "{unseen}");
    assert!(!line.contains("Remove"), "{line}");
    // A line of the lead and a line of a count of tokens give no step,
    // also for a link.
    let link = riff::text::host_file_link(&projects);
    let unseen_link = riff::text::lead_turns_unseen(&link);
    assert!(!unseen_link.contains("Remove"), "{unseen_link}");
    assert!(
        unseen.contains("When the line says that `projects` is a link"),
        "{unseen}"
    );
    assert!(
        unseen.contains("(#when-a-session-puts-a-link-in-its-folder)"),
        "{unseen}"
    );
    let skipped = riff::text::transcript_skipped(&link);
    assert!(skipped.starts_with("riff counts no tokens"), "{skipped}");
    assert!(!skipped.contains("Remove"), "{skipped}");
    assert!(
        how_to.contains("A line that says \"riff counts no tokens\" needs no step"),
        "{how_to}"
    );
    for (limit, words) in [
        (riff::usage::MAX_TRANSCRIPT, "| 256 MB |"),
        (riff::lead::MAX_ALTERNATES, "| 1 MB |"),
    ] {
        let mb = words.trim_matches(['|', ' ']).trim_end_matches(" MB");
        assert_eq!(limit, mb.parse::<u64>().unwrap() << 20, "{words}");
        assert!(how_to.contains(words), "{words}");
    }
    assert_eq!(
        (riff::run::MAX_CONFIG, riff::run::MAX_SETTINGS),
        (16 << 20, 1 << 20)
    );
    assert!(how_to.contains("| 16 MB, and 1 MB |"), "{how_to}");
    assert_eq!(riff::usage::MAX_ENTRIES, 10_000);
    assert!(how_to.contains("| 10,000 entries |"), "{how_to}");
    assert_eq!(
        (
            riff::usage::MAX_COUNT_BYTES,
            riff::usage::MAX_COUNT_ENTRIES,
            riff::usage::MAX_COUNT_REPLIES,
            riff::usage::MAX_COUNT_MODELS
        ),
        (1 << 30, 100_000, 1_000_000, 32)
    );
    assert!(
        how_to.contains("| 1 GB, 100,000 entries, 1,000,000 replies and 32 models in all |"),
        "{how_to}"
    );
    assert!(
        how_to.contains("It reads the\ntranscripts of one count at a time"),
        "{how_to}"
    );
    assert!(
        how_to.contains("for example `\\n` for a line break"),
        "{how_to}"
    );
    let too_many = riff::text::host_file_too_many(Path::new("/p"), riff::usage::MAX_ENTRIES);
    assert!(too_many.starts_with("riff lists at most"), "{too_many}");
    use riff::text::CountLimit;
    for limit in [
        CountLimit::Bytes(riff::usage::MAX_COUNT_BYTES),
        CountLimit::Entries(riff::usage::MAX_COUNT_ENTRIES),
        CountLimit::Replies(riff::usage::MAX_COUNT_REPLIES),
        CountLimit::Models(riff::usage::MAX_COUNT_MODELS),
    ] {
        let stopped = riff::text::transcripts_stopped(Path::new("/p/a.jsonl"), limit);
        assert!(stopped.starts_with("riff reads at most"), "{stopped}");
    }
    assert!(error.starts_with("riff reads at most"), "{error}");
    let settings = lead.join("claude/settings.json");
    let grows = riff::text::config_grows_too_large(&settings, riff::run::MAX_SETTINGS);
    assert!(grows.starts_with("riff writes at most"), "{grows}");
    assert!(start.contains("\"riff writes at most\""), "{start}");
    assert_eq!(
        crate::book::commands_in(how_to),
        ["riff daemon log --grep 'reads at most' --grep 'writes at most' --grep 'lists at most'"]
    );
    assert_eq!(
        crate::book::commands_in(start),
        [
            "rm ~/.local/share/riff/leads/OWNER-REPO/claude/.claude.json",
            "riff lead start",
        ]
    );
    assert!(
        start.contains("after you remove\n`settings.json`"),
        "{start}"
    );
    assert_eq!(crate::book::commands_in(unseen), ["riff lead clean"]);
    assert!(unseen.contains("`projects-left/TIME`"), "{unseen}");
    for anchor in [
        "(#when-a-session-makes-a-large-file-in-its-folder)",
        "(#when-the-start-of-the-lead-stops-at-a-large-file)",
        "(#when-riff-sees-no-turn-of-the-lead)",
    ] {
        assert!(page.contains(anchor), "the book points to {anchor}");
    }
}

/// A clone of the repository `acme/app` in the environment `env`, as
/// the person has it.
fn acme_clone(env: &isolated::Isolated) -> PathBuf {
    let clone = env.path().join("src/app");
    std::fs::create_dir_all(&clone).unwrap();
    for args in [
        &["init", "-q"][..],
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    ] {
        let git = env.command("git").args(args).current_dir(&clone).output();
        assert!(git.unwrap().status.success(), "git {args:?}");
    }
    clone
}

/// The command of the how-to "When riff sees no turn of the lead", in
/// the clone `clone` of the environment `env`: its exit code, stdout and
/// stderr.
fn run_the_book(env: &isolated::Isolated, clone: &Path) -> (i32, String, String) {
    let unseen = &crate::book::how_to(
        "how-it-works.md",
        "##### When riff sees no turn of the lead",
    );
    let commands = crate::book::commands_in(unseen);
    let [command] = &commands[..] else {
        panic!("{commands:?}")
    };
    let args: Vec<&str> = command.split_whitespace().collect();
    assert_eq!(args[0], "riff");
    let out = env
        .riff()
        .args(&args[1..])
        .current_dir(clone)
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap().trim().to_owned();
    (
        out.status.code().unwrap(),
        text(out.stdout),
        text(out.stderr),
    )
}

/// 01M7GF9QGV86BENYWT1TDJPM64, 01M7FX6YQKTN9EVPTZJ9XB8PM4,
/// 01M7FNTSHXPQXXKZF761V0NN9Z: the command of the how-to "When riff sees
/// no turn of the lead", as the person runs it in a clone of the
/// repository. The lead made more entries than the limit in its folder
/// `projects`, also a hidden one, two with the name of its folder of
/// conversations and more at the end, and a link to a folder of the
/// person. The first run reads the limit of entries, moves them into a
/// new folder of the lead, and says to run it again: riff sees the turns
/// of the lead again. The second run moves the rest. riff keeps the
/// folder of the conversations, which `claude` names from the path of the
/// clone of the lead, and follows no link.
#[test]
fn the_command_of_the_book_keeps_the_conversation_of_the_lead() {
    let env = isolated::Isolated::new();
    let clone = acme_clone(&env);
    let lead = riff::lead::folder_of(&env.riff_home(), "acme/app");
    let claude = lead.join("claude");
    let projects = claude.join(riff::usage::PROJECTS);
    let name = riff::lead::claude_name(&lead.join("clone"));
    let own = projects.join(&name);
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("s1.jsonl"), "").unwrap();
    let made = riff::usage::MAX_ENTRIES;
    for n in 4..made {
        std::fs::write(projects.join(format!("{n}.jsonl")), "").unwrap();
    }
    std::fs::write(projects.join(".hidden"), "").unwrap();
    for other in [format!("{name}-2"), format!("x{name}")] {
        std::fs::create_dir(projects.join(other)).unwrap();
    }
    let person = env.home().join("person");
    std::fs::create_dir_all(&person).unwrap();
    std::fs::write(person.join("id_ed25519"), "SECRET").unwrap();
    std::os::unix::fs::symlink(&person, projects.join("home")).unwrap();
    assert!(riff::usage::written_in(&claude, "s1").is_err());

    let runs = || {
        let mut runs = std::fs::read_dir(lead.join(riff::lead::PROJECTS_LEFT))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        runs.sort();
        runs
    };
    let (code, out, err) = run_the_book(&env, &clone);
    assert_eq!(code, 1, "{out}");
    let [first] = &runs()[..] else {
        panic!("{:?}", runs())
    };
    let moved = std::fs::read_dir(first).unwrap().count();
    assert!(moved >= made - 1, "{moved}");
    assert_eq!(
        err,
        riff::text::lead_clean_more(&projects, moved, first, riff::usage::MAX_ENTRIES)
    );
    assert!(riff::usage::written_in(&claude, "s1").unwrap().is_some());

    let (code, out, err) = run_the_book(&env, &clone);
    assert_eq!(code, 0, "{err}");
    let [_, second] = &runs()[..] else {
        panic!("{:?}", runs())
    };
    assert_eq!(
        out,
        riff::text::lead_cleaned(&projects, made - moved, second)
    );
    let kept: Vec<_> = std::fs::read_dir(&projects)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(kept, [own]);
    let left = |name: &str| [first, second].map(|run| run.join(name));
    assert!(
        left("home")
            .iter()
            .any(|home| home.symlink_metadata().is_ok_and(|m| m.is_symlink()))
    );
    assert_eq!(
        std::fs::read_to_string(person.join("id_ed25519")).unwrap(),
        "SECRET"
    );
    assert_eq!(std::fs::read_dir(&person).unwrap().count(), 1);

    // Nothing more to move: riff says so, and makes no new folder.
    let (code, out, _) = run_the_book(&env, &clone);
    assert_eq!(code, 0);
    assert!(out.starts_with("riff moved no entry of "), "{out}");
    assert_eq!(runs().len(), 2);
}

/// 01M7GF9QGV86BENYWT1TDJPM64: the lead put a link at its folder
/// `projects`, to a folder of the person. The command of the book
/// refuses it with the error that names the link, and moves nothing:
/// each file of the person stays, and riff makes no folder for the
/// entries. Out of a clone, the command names the step.
#[test]
fn the_command_of_the_book_moves_nothing_through_a_link_at_projects() {
    let env = isolated::Isolated::new();
    let clone = acme_clone(&env);
    let lead = riff::lead::folder_of(&env.riff_home(), "acme/app");
    let claude = lead.join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    let person = env.home().join(".ssh");
    std::fs::create_dir_all(&person).unwrap();
    std::fs::write(person.join("id_ed25519"), "SECRET").unwrap();
    std::fs::write(person.join("config"), "Host *").unwrap();
    let projects = claude.join(riff::usage::PROJECTS);
    std::os::unix::fs::symlink(&person, &projects).unwrap();

    let (code, out, err) = run_the_book(&env, &clone);
    assert_eq!(code, 1, "{out}");
    assert_eq!(err, riff::text::host_file_link(&projects));
    assert_eq!(std::fs::read_dir(&person).unwrap().count(), 2);
    assert_eq!(
        std::fs::read_to_string(person.join("id_ed25519")).unwrap(),
        "SECRET"
    );
    assert!(!lead.join(riff::lead::PROJECTS_LEFT).exists());

    // No folder `projects`: riff names the host of the lead.
    std::fs::remove_file(&projects).unwrap();
    let (code, _, err) = run_the_book(&env, &clone);
    assert_eq!(code, 1);
    assert_eq!(err, riff::text::lead_no_projects("acme/app", &projects));

    // Out of a clone of a repository.
    let (code, _, err) = run_the_book(&env, &env.home());
    assert_eq!(code, 1);
    assert_eq!(err, riff::text::LEAD_CLEAN_NO_REPO);
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
