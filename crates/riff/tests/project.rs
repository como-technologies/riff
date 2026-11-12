//! `riff project init`, `riff project list` and `riff project remove`
//! (01M76SG2E9RPV4B7CYSZPGGZ3P, 01M76SHAX0KR0DM9GPXAPZJRDZ,
//! 01M76SHB27S4QDV9HMRR4B6APJ, 01M76SHB4SAWFW3S5Y7X9KC6E4).
//!
//! The `gh` of the machine is the fake forge `tests/fake/forge`: it keeps
//! the repository `acme/app` in the files of `bin`, logs each call, and
//! `gh auth token` prints the file `bin/token`. The riff-server has no
//! sign-in, and its GitHub is a fake ([`FakeGitHub`]) where [`ADMIN_TOKEN`]
//! is an admin of `acme/app` and [`WRITER_TOKEN`] is not. The clone is a
//! real git repository with a `riff.toml` and a remote `origin` on GitHub.

use std::path::{Path, PathBuf};
use std::process::Output;

use isolated::Isolated;

use crate::book::{commands_of, each_is_real};
use crate::github_apps::fake_github::FakeGitHub;

/// The image that `riff.toml` of the clone pins.
const IMAGE: &str = "ghcr.io/como-technologies/riff-session@sha256:\
                     0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The token of `gh` of a person who is an admin of `acme/app`.
const ADMIN_TOKEN: &str = "gho_admin_of_acme_app";

/// The token of `gh` of a person who can push to `acme/app`, and is no
/// admin of it.
const WRITER_TOKEN: &str = "gho_writer_of_acme_app";

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The output of `cmd`, away from the runtime of the test.
async fn output(mut cmd: std::process::Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// A riff-server with no sign-in, whose GitHub is `fake`.
async fn serve(fake: &FakeGitHub) -> String {
    let (listener, url) = crate::common::listen().await;
    let config = riff_server::auth::Config {
        github: riff_server::github::Settings {
            api: fake.url.clone(),
            web: fake.url.clone(),
            ..riff_server::github::Settings::default()
        },
        ..riff_server::auth::Config::new(&url)
    };
    let router = riff_server::Service::new(config).router();
    tokio::spawn(async move {
        let service = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
        axum::serve(listener, service).await.unwrap()
    });
    url
}

/// A person in a clone of `acme/app`, with the fake forge as `gh` and a
/// riff-server.
struct Clone {
    env: Isolated,
    bin: PathBuf,
    repo: PathBuf,
    /// The riff-server, or `None` for one that does not answer.
    server: Option<String>,
    github: FakeGitHub,
}

impl Clone {
    /// A clone with a remote `origin` on GitHub, and a riff-server.
    async fn new() -> Clone {
        let clone = Clone::with_no_remote().await;
        clone.git(
            &clone.repo,
            &["remote", "add", "origin", "https://github.com/acme/app.git"],
        );
        clone
    }

    /// A clone with no remote, and a riff-server.
    async fn with_no_remote() -> Clone {
        let env = Isolated::new();
        let root = env.path().to_owned();
        let (bin, repo) = (root.join("bin"), root.join("repo"));
        for dir in [&bin, &repo] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let github = FakeGitHub::start().await;
        github.seen().person(ADMIN_TOKEN, "acme/app", true);
        github.seen().person(WRITER_TOKEN, "acme/app", false);
        let server = Some(serve(&github).await);
        let clone = Clone {
            env,
            bin,
            repo,
            server,
            github,
        };
        clone.git(clone.env.path(), &["init", "-q", "-b", "main", "repo"]);
        std::fs::write(
            clone.repo.join("riff.toml"),
            format!("[session]\nimage = \"{IMAGE}\"\n"),
        )
        .unwrap();
        clone
    }

    /// `riff ARGS` in the folder `dir`, with the fakes, as a person in a
    /// plain terminal.
    fn command(&self, dir: &Path, args: &[&str]) -> std::process::Command {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(dir)
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            // A plain terminal: the test can run under Claude Code.
            .env_remove("CLAUDECODE");
        if let Some(server) = &self.server {
            cmd.env("RIFF_SERVER", server);
        }
        cmd
    }

    /// `riff project ARGS` in the folder `dir`.
    async fn project_in(&self, dir: &Path, args: &[&str]) -> Output {
        let words: Vec<&str> = std::iter::once("project")
            .chain(args.iter().copied())
            .collect();
        output(self.command(dir, &words)).await
    }

    /// `riff project ARGS` in the clone.
    async fn project(&self, args: &[&str]) -> Output {
        self.project_in(&self.repo, args).await
    }

    /// `git ARGS` in `dir`, as a person with a name.
    fn git(&self, dir: &Path, args: &[&str]) {
        let status = self
            .env
            .command("git")
            .args([
                "-c",
                "user.name=riff",
                "-c",
                "user.email=riff@example.com",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.join(name)).unwrap_or_default()
    }

    /// The calls of the fake forge that change it.
    fn forge_changes(&self) -> Vec<String> {
        self.read("calls")
            .lines()
            .filter(|l| l.contains(" -X ") || l.starts_with("label "))
            .map(str::to_owned)
            .collect()
    }

    /// The tokens of each read of the repository on the fake GitHub of
    /// riff-server.
    fn repo_reads(&self) -> Vec<String> {
        self.github.seen().repo_reads.clone()
    }

    /// A clone of acme/app that lacks only the last commit of `origin`:
    /// the commit with `riff.toml` that the merge of the step makes. The
    /// remote `origin` is a folder of this machine whose path names
    /// `github.com/acme/app`, so riff takes it for the repository on GitHub.
    fn clone_behind_origin(&self) -> PathBuf {
        let root = self.env.path();
        let origin = format!("{}//github.com/acme/app.git", root.join("remote").display());
        let (work, clone) = (root.join("work"), root.join("clone"));
        self.git(root, &["init", "-q", "--bare", "-b", "main", &origin]);
        self.git(root, &["init", "-q", "-b", "main", "work"]);
        self.git(&work, &["remote", "add", "origin", &origin]);
        std::fs::write(work.join("CLAUDE.md"), "rules\n").unwrap();
        self.git(&work, &["add", "."]);
        self.git(&work, &["commit", "-q", "-m", "first"]);
        self.git(&work, &["push", "-q", "origin", "main"]);
        self.git(root, &["clone", "-q", &origin, "clone"]);
        std::fs::write(
            work.join("riff.toml"),
            format!("[session]\nimage = \"{IMAGE}\"\n"),
        )
        .unwrap();
        self.git(&work, &["add", "."]);
        self.git(&work, &["commit", "-q", "-m", "Set up riff"]);
        self.git(&work, &["push", "-q", "origin", "main"]);
        clone
    }
}

/// The fake forge `tests/fake/forge` as the `gh` of the machine: it
/// keeps its state in the files of `bin`, and logs each call to
/// `bin/calls`. The `state` files give the repository; the files
/// `bin/not_admin` and `bin/no_login` change the person. `gh auth token`
/// gives [`ADMIN_TOKEN`].
fn fake_forge(bin: &Path, state: &[(&str, &str)]) {
    isolated::write_program(bin.join("gh"), include_str!("fake/forge"));
    std::fs::write(bin.join("token"), ADMIN_TOKEN).unwrap();
    for (name, text) in state {
        std::fs::write(bin.join(name), text).unwrap();
    }
}

/// A pull request that ran the checks of `names`, as the files of the
/// fake forge give it.
fn ci(names: &str) -> [(&'static str, String); 2] {
    [
        (
            "prior_pulls",
            r#"[{"number":3,"html_url":"https://github.com/acme/app/pull/3","head":{"ref":"feature","sha":"abc1234"},"base":{"ref":"main"}}]"#.to_owned(),
        ),
        (
            "checks_abc1234",
            format!(r#"{{"check_runs":[{names}]}}"#),
        ),
    ]
}

/// A repository with no `riff.toml`, no auto-merge, no ruleset and no
/// milestone, whose latest pull request ran two checks.
fn bare(bin: &Path) {
    let [pulls, checks] = ci(r#"{"name":"checks / lint"},{"name":"checks / test"}"#);
    fake_forge(
        bin,
        &[
            ("files", ""),
            ("milestones", ""),
            (pulls.0, &pulls.1),
            (checks.0, &checks.1),
        ],
    );
}

/// A repository that is set up for riff.
fn set_up(bin: &Path) {
    let [pulls, checks] = ci(r#"{"name":"Gate"}"#);
    fake_forge(
        bin,
        &[
            ("files", "riff.toml\n"),
            ("milestones", "Wave 1\nBacklog\n"),
            ("auto_merge", ""),
            (
                "ruleset",
                r#"{"name":"riff","rules":[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]}"#,
            ),
            (pulls.0, &pulls.1),
            (checks.0, &checks.1),
        ],
    );
}

/// A repository that is set up but for its CI: `riff.toml` is on `main`, and
/// no pull request ran a check.
fn no_ci(bin: &Path) {
    fake_forge(
        bin,
        &[
            ("files", "riff.toml\n"),
            ("milestones", "Wave 1\nBacklog\n"),
            ("auto_merge", ""),
        ],
    );
}

/// A repository that is set up, whose ruleset requires the verify and no
/// check of the CI, and whose latest pull request ran the check
/// `checks / lint`.
fn ci_not_required(bin: &Path) {
    let [pulls, checks] = ci(r#"{"name":"checks / lint"}"#);
    fake_forge(
        bin,
        &[
            ("files", "riff.toml\n"),
            ("milestones", "Wave 1\nBacklog\n"),
            ("auto_merge", ""),
            (
                "ruleset",
                r#"{"rules":[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"riff/verify"}]}}]}"#,
            ),
            (pulls.0, &pulls.1),
            (checks.0, &checks.1),
        ],
    );
}

/// The lines of `stdout` that start with `fail  `.
fn failed_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| l.starts_with("  fail  "))
        .collect()
}

/// The lines of `stdout` that start with `step  `: the checks that a step
/// of the setup fixes (01M5RGG9KJXCKH8C94145WS8SM).
fn step_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| l.starts_with("  step  "))
        .collect()
}

/// The lines at the end of `stdout` after "N checks failed": `  NAME: FIX`
/// for each failed check (01M5RGGD6YR36EXZEG61XCJVRY).
fn named_at_the_end(stdout: &str) -> Vec<&str> {
    let mut lines = stdout
        .lines()
        .skip_while(|l| !l.contains(" failed. riff enrolled no project."));
    lines.next();
    lines.collect()
}

/// 01M5QVF5535YRN1EF3DZAB4W8G, 01M5R1672NF56P15VD0JEX51K4,
/// 01M76SHAX0KR0DM9GPXAPZJRDZ: `riff project init --check` in a clone of a
/// repository with no `riff.toml`, no auto-merge, no ruleset that requires
/// `riff/verify`, no `Backlog` and no wave names the fix of each fact,
/// changes nothing, and enrolls nothing. It runs no check of the machine.
#[tokio::test]
async fn riff_project_init_check_names_the_github_fix_of_each_fact_that_fails() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    assert!(
        stdout.starts_with("riff checks the repository of this clone on GitHub:\n"),
        "{stdout}"
    );
    assert_eq!(
        failed_lines(&stdout),
        [
            "  fail  riff.toml on main: main of acme/app has no riff.toml, so riff lead start \
             stops. Fix: put riff.toml with the session image on main. See \"Add another \
             repository to the riff\" in the book",
            "  fail  auto-merge and squash merge: acme/app allows no auto-merge. Fix: allow \
             them: gh api -X PATCH repos/acme/app -F allow_auto_merge=true -F \
             allow_squash_merge=true",
            "  fail  the ruleset of main: no ruleset of main requires the check riff/verify, \
             so a merge does not wait for the verify. Fix: make a ruleset of main that \
             requires a pull request, the check riff/verify and each check that your CI \
             runs on a pull request. See \"Add another \
             repository to the riff\" in the book",
            "  fail  the milestone Backlog: acme/app has no open milestone Backlog. Fix: make \
             it: gh api -X POST repos/acme/app/milestones -f title=Backlog. riff wave make \
             makes it too, with the next wave",
            "  fail  the first wave: acme/app has no open wave. Fix: make it: riff wave make \
             \"Wave 1\"",
        ],
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "5 checks failed. riff enrolled no project. Do each fix, then run riff project init \
             again:"
        ),
        "{stdout}"
    );
    for word in ["podman", "subuid", "cgroup", "the riff binary", "daemon"] {
        assert!(!stdout.contains(word), "{word}\nin\n{stdout}");
    }
    // --check only reads: it plans no step, so the lines say `fail`.
    assert!(step_lines(&stdout).is_empty(), "{stdout}");
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(clone.forge_changes().is_empty(), "{}", clone.read("calls"));
    // No enrollment: no token, and no read on GitHub by riff-server.
    assert!(!clone.read("calls").contains("auth token"));
    assert!(clone.repo_reads().is_empty());
}

/// 01M5R1672NF56P15VD0JEX51K4, 01M76SHAX0KR0DM9GPXAPZJRDZ,
/// 01M76SG8GBXFYXKWMQHPCNEXBN, 01M76SHB4SAWFW3S5Y7X9KC6E4: `riff project
/// init --yes` in a clone of a bare repository makes the pull request with
/// `riff.toml` and merges it, allows auto-merge and squash merge, makes the
/// ruleset with `riff/verify` and the checks of the CI, makes the two
/// milestones, and enrolls the repository with the token of `gh`. The
/// repository has no `CLAUDE.md`: riff prints no line for it
/// (01M5RN89Z749VYS1CRYJQTKAWY). A second run changes nothing on GitHub,
/// asks nothing, and with `--host` sets the hosts. `riff project list`
/// shows the project, and `riff project remove` forgets it.
#[tokio::test]
async fn riff_project_init_sets_up_and_enrolls_a_repository_and_list_and_remove_show_it() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    let list = || clone.project(&["list"]);
    let none = text(&list().await.stdout);
    assert!(none.starts_with("The riff has no project."), "{none}");

    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for line in [
        "riff can set up acme/app for riff. These steps change GitHub:",
        "  allow auto-merge and squash merge",
        "  make riff.toml on main with a pull request",
        "  make the milestone Backlog",
        "  make the milestone Wave 1",
        "  make the ruleset riff on main",
        "  done  allow auto-merge and squash merge: allowed both",
        "  done  make riff.toml on main with a pull request: opened \
         https://github.com/acme/app/pull/7 and merged it",
        "  done  make the milestone Backlog: made Backlog",
        "  done  make the milestone Wave 1: made Wave 1",
        "  done  make the ruleset riff on main: requires checks / lint and checks / test and \
         riff/verify",
        "riff checks the repository again:",
        "riff enrolled acme/app as a project of the riff. Hosts that may run its wave: each \
         host.",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    // The clone has its own riff.toml, so riff does not ask for a pull.
    assert!(!stdout.contains("Run git pull"), "{stdout}");
    assert!(!stdout.contains("CLAUDE.md"), "{stdout}");
    assert!(!stdout.contains("AGENTS.md"), "{stdout}");
    // riff-server read the repository one time, with the token of gh.
    assert_eq!(clone.repo_reads(), [ADMIN_TOKEN]);

    let json =
        |name: &str| -> serde_json::Value { serde_json::from_str(&clone.read(name)).unwrap() };
    assert_eq!(json("pull.json")["title"], "Set up riff");
    assert_eq!(json("pull.json")["head"], "riff-setup");
    assert_eq!(json("merge.json")["merge_method"], "squash");
    assert_eq!(json("config.json")["branch"], "riff-setup");
    assert!(clone.bin.join("auto_merge").exists());
    assert_eq!(clone.read("milestones"), "Backlog\nWave 1\n");
    let rules = clone.read("ruleset");
    for context in ["checks / lint", "checks / test", "riff/verify"] {
        assert!(
            rules.contains(&format!(r#""context":"{context}""#)),
            "{rules}"
        );
    }
    let changes = clone.forge_changes();
    assert!(changes.len() >= 8, "{changes:?}");
    // #1605: the first wave comes with the label question, and riff never
    // forces it: a label that is there keeps its text.
    let question: Vec<&String> = changes
        .iter()
        .filter(|c| c.starts_with("label create question"))
        .collect();
    assert_eq!(question.len(), 1, "{changes:?}");
    assert!(!question[0].contains("--force"), "{}", question[0]);

    let listed = text(&list().await.stdout);
    assert!(
        listed.starts_with("acme/app\n  enrolled by the person "),
        "{listed}"
    );
    assert!(
        listed.ends_with("\n  hosts: each host\n  leads: no lead\n"),
        "{listed}"
    );

    // A second run: the checks pass, nothing changes on GitHub, nothing is
    // asked, and the hosts are new.
    let out = clone
        .project(&["init", "--host", "thelio", "--host", "heron"])
        .await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(!stdout.contains("fail  "), "{stdout}");
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(!stdout.contains("Do them?"), "{stdout}");
    assert!(
        stdout.ends_with(
            "acme/app is a project of the riff already. riff set its hosts. Hosts that may run \
             its wave: heron, thelio.\n"
        ),
        "{stdout}"
    );
    assert_eq!(clone.forge_changes(), changes);
    let out = clone
        .project(&["init", "--host", "heron", "--host", "thelio"])
        .await;
    assert!(
        text(&out.stdout).ends_with(
            "acme/app is a project of the riff already, with the same hosts. riff changed \
             nothing. Hosts that may run its wave: heron, thelio.\n"
        ),
        "{}",
        text(&out.stdout)
    );
    let listed = text(&list().await.stdout);
    assert!(listed.contains("\n  hosts: heron, thelio\n"), "{listed}");

    // The riff forgets the project, and GitHub keeps the repository.
    let out = clone.project(&["remove", "acme/app"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "riff forgot the project acme/app. GitHub keeps the repository as it is.\n"
    );
    assert_eq!(clone.forge_changes(), changes);
    let none = text(&list().await.stdout);
    assert!(none.starts_with("The riff has no project."), "{none}");
}

/// 01M76SG8GBXFYXKWMQHPCNEXBN: a person whose `gh` is no admin of the
/// repository on GitHub enrolls nothing and removes nothing: riff-server
/// refuses with the reason of GitHub, and the command exits with 1.
#[tokio::test]
async fn a_person_who_is_no_admin_of_the_repository_on_github_enrolls_nothing() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    std::fs::write(clone.bin.join("token"), WRITER_TOKEN).unwrap();
    let out = clone.project(&["init"]).await;
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}\n{stderr}",
        text(&out.stdout)
    );
    assert!(
        stderr.contains("GitHub says that the account of your gh is no admin of acme/app"),
        "{stderr}"
    );
    assert_eq!(clone.repo_reads(), [WRITER_TOKEN]);
    let listed = text(&clone.project(&["list"]).await.stdout);
    assert!(listed.starts_with("The riff has no project."), "{listed}");

    // An admin enrolls it; the writer cannot remove it.
    std::fs::write(clone.bin.join("token"), ADMIN_TOKEN).unwrap();
    assert!(clone.project(&["init"]).await.status.success());
    std::fs::write(clone.bin.join("token"), WRITER_TOKEN).unwrap();
    let out = clone.project(&["remove", "acme/app"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("no admin of acme/app"),
        "{}",
        text(&out.stderr)
    );
    let listed = text(&clone.project(&["list"]).await.stdout);
    assert!(listed.starts_with("acme/app\n"), "{listed}");

    // With no token of gh, riff names the fix and enrolls nothing.
    std::fs::remove_file(clone.bin.join("token")).unwrap();
    let out = clone.project(&["remove", "acme/app"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("Fix: gh auth login"),
        "{}",
        text(&out.stderr)
    );
}

/// 01M76SHB27S4QDV9HMRR4B6APJ: `riff project init`, also with `--check`,
/// and `riff project remove` refuse in an agent session and in a
/// container, before each call: no call of `gh`, and no call of
/// riff-server. `riff project list` runs in an agent session.
#[tokio::test]
async fn riff_project_init_and_remove_refuse_in_an_agent_session_and_in_a_container() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    let place = [("CLAUDECODE", "1"), ("RIFF_SOCKET", "/run/riff/riff.sock")];
    for (name, value) in place {
        for args in [&["init"][..], &["init", "--check"], &["remove", "acme/app"]] {
            let mut cmd = clone.command(&clone.repo, &[&["project"][..], args].concat());
            cmd.env(name, value);
            let out = output(cmd).await;
            let stderr = text(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{name} {args:?}: {stderr}");
            let who = if name == "RIFF_SOCKET" {
                "a container"
            } else {
                "a session"
            };
            assert!(
                stderr.starts_with(&format!("riff: {who} never runs riff project {}.", args[0])),
                "{name} {args:?}: {stderr}"
            );
            assert!(text(&out.stdout).is_empty(), "{name} {args:?}");
        }
    }
    assert_eq!(clone.read("calls"), "");
    assert!(clone.repo_reads().is_empty());

    let mut cmd = clone.command(&clone.repo, &["project", "list"]);
    cmd.env("CLAUDECODE", "1");
    let out = output(cmd).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("The riff has no project."));
}

/// 01M76SHAX0KR0DM9GPXAPZJRDZ: in a folder with no remote `origin` on
/// GitHub, `riff project init` stops and says so. It calls no `gh`, and
/// enrolls nothing. A remote on another host is no remote on GitHub.
#[tokio::test]
async fn riff_project_init_stops_in_a_folder_with_no_remote_on_github() {
    let clone = Clone::with_no_remote().await;
    bare(&clone.bin);
    for remote in [None, Some("https://gitlab.com/acme/app.git")] {
        if let Some(remote) = remote {
            clone.git(&clone.repo, &["remote", "add", "origin", remote]);
        }
        let out = clone.project(&["init", "--yes"]).await;
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{remote:?}: {stderr}");
        assert!(
            stderr.contains(
                "riff project init runs in a clone of a repository on GitHub, and this folder \
                 has no remote origin on GitHub. riff changed nothing."
            ),
            "{stderr}"
        );
        assert_eq!(clone.read("calls"), "", "{remote:?}");
    }
    assert!(clone.repo_reads().is_empty());
}

/// 01M75NAPNH1SP8AC90NYFQP8SK: `riff project init --yes` in a clone of a
/// repository with the release cycle of riff and no merge queue puts the
/// rule `merge_queue` in the ruleset that requires `riff/verify`, and
/// keeps its other rules. It changes nothing else. A second run finds the
/// queue, changes nothing and asks nothing.
#[tokio::test]
async fn riff_project_init_yes_sets_the_merge_queue_on_main_and_a_second_run_changes_nothing() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    std::fs::write(clone.bin.join("config"), "[release]\n").unwrap();
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for line in [
        "  step  the merge queue: acme/app has the release cycle of riff and no merge queue on \
         main, so a merge can put a tree on main that no Gate tested. Step: add the merge queue \
         to the ruleset of main",
        "riff can set up acme/app for riff. These steps change GitHub:",
        "  add the merge queue to the ruleset of main",
        "  done  add the merge queue to the ruleset of main: the ruleset riff has the merge queue \
         with the method squash",
        "  pass  the merge queue: main of acme/app merges through a merge queue with the method \
         squash",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    let changes = clone.forge_changes();
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert!(
        changes[0].starts_with("api -X PUT repos/{owner}/{repo}/rulesets/77 --include --input "),
        "{changes:?}"
    );
    let ruleset: serde_json::Value = serde_json::from_str(&clone.read("ruleset")).unwrap();
    assert_eq!(ruleset["name"], "riff");
    let rules = ruleset["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2, "{ruleset}");
    assert_eq!(
        rules[0]["parameters"]["required_status_checks"],
        serde_json::json!([{ "context": "Gate" }, { "context": "riff/verify" }])
    );
    assert_eq!(rules[1], riff::repo_setup::merge_queue());

    // A second run: the check passes, nothing changes, and nothing is asked.
    let out = clone.project(&["init"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(
            "\n  pass  the merge queue: main of acme/app merges through a merge queue with the \
             method squash\n"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("fail  "), "{stdout}");
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(!stdout.contains("Do them?"), "{stdout}");
    assert_eq!(clone.forge_changes(), changes);
}

/// The `riff.toml` that the fake forge got in the pull request of the
/// setup.
fn made_config(clone: &Clone) -> String {
    use base64::Engine as _;
    let config: serde_json::Value = serde_json::from_str(&clone.read("config.json")).unwrap();
    let content = config["content"].as_str().expect("the step made riff.toml");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content)
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

/// 01M5SNTPEP41NMSP3SEKR7KVB0: the `riff.toml` that `riff project init
/// --yes` makes in a new repository pins `riff-base`, the image of the
/// release with no language, and one line says that the project adds its
/// toolchain in an image of its own.
#[tokio::test]
async fn riff_project_init_yes_pins_the_base_image_and_says_to_add_the_toolchain() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    let base = riff::session_image::base();
    assert_eq!(base.name(), "ghcr.io/como-technologies/riff-base");
    assert_eq!(made_config(&clone), riff::session_image::config_of(&base));
    let line = format!("  {}", riff::text::init_toolchain(&base));
    assert_eq!(stdout.lines().filter(|l| *l == line).count(), 1, "{stdout}");
    assert!(stdout.contains("an image with claude, git and gh and no language"));
}

/// 01M5SNTPEP41NMSP3SEKR7KVB0: `riff project init --yes --image REF` pins
/// REF in place of `riff-base`, and says nothing of a toolchain. A tag is
/// refused before riff changes anything.
#[tokio::test]
async fn riff_project_init_image_pins_the_image_that_it_names_and_refuses_a_tag() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    let mine = format!("ghcr.io/acme/app-session@sha256:{}", "ab".repeat(32));
    let out = clone.project(&["init", "--yes", "--image", &mine]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert_eq!(
        made_config(&clone),
        format!("[session]\nimage = \"{mine}\"\n")
    );
    assert!(!stdout.contains("no language"), "{stdout}");

    let clone = Clone::new().await;
    bare(&clone.bin);
    let out = clone
        .project(&["init", "--yes", "--image", "ghcr.io/acme/app:latest"])
        .await;
    assert!(!out.status.success(), "{out:?}");
    let error = text(&out.stderr);
    assert!(error.contains("names a tag"), "{error}");
    assert!(clone.forge_changes().is_empty());
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY: with no `y` (no
/// terminal), the first list has `step` lines for the five facts that a step
/// fixes, and no `fail`. A no does no step, so the end names each check with
/// its fix, and riff enrolls nothing.
#[tokio::test]
async fn riff_project_init_without_a_terminal_names_the_steps_and_does_none() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    let out = clone.project(&["init"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(failed_lines(&stdout).is_empty(), "{stdout}");
    let steps = step_lines(&stdout);
    assert_eq!(steps.len(), 5, "{stdout}");
    assert_eq!(
        steps[0],
        "  step  riff.toml on main: main of acme/app has no riff.toml, so riff lead start \
         stops. Step: make riff.toml on main with a pull request"
    );
    assert_eq!(
        steps[2],
        "  step  the ruleset of main: no ruleset of main requires the check riff/verify, so a \
         merge does not wait for the verify. Step: make the ruleset riff on main"
    );
    assert!(
        stdout.contains("These steps change GitHub:\n  allow auto-merge and squash merge\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "riff did no step. To do them, run riff project init in a terminal and answer y, or \
             run riff project init --yes."
        ),
        "{stdout}"
    );
    // No step ran, so no second list. The end names each check, with its fix.
    assert!(
        !stdout.contains("riff checks the repository again:"),
        "{stdout}"
    );
    assert!(stdout.contains("5 checks failed."), "{stdout}");
    let named = named_at_the_end(&stdout);
    assert_eq!(named.len(), 5, "{stdout}");
    assert!(
        named[1].starts_with("  auto-merge and squash merge: allow them: gh api -X PATCH"),
        "{}",
        named[1]
    );
    assert!(clone.forge_changes().is_empty());
    assert!(clone.repo_reads().is_empty());
}

/// 01M5R1672NF56P15VD0JEX51K4: a person who is not an admin gets the
/// steps named, and riff changes nothing, also with `--yes`.
#[tokio::test]
async fn a_person_who_is_not_an_admin_gets_the_steps_named_and_riff_changes_nothing() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    std::fs::write(clone.bin.join("not_admin"), "").unwrap();
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert_eq!(failed_lines(&stdout).len(), 5, "{stdout}");
    assert!(
        stdout.contains(
            "You are not an admin of acme/app, so riff changes nothing there. Ask an admin to \
             run riff project init, or do the fix of each failed check."
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(clone.forge_changes().is_empty());
}

/// A clone that is not safe to pull gets no `git pull`: riff says why it
/// ran no `git pull`, and to pull (01M5RGGD9AYNC12Z2CG5CDZWNA).
#[tokio::test]
async fn a_clone_that_riff_does_not_pull_gets_a_line_to_pull() {
    let clone = Clone::new().await;
    bare(&clone.bin);
    std::fs::remove_file(clone.repo.join("riff.toml")).unwrap();
    // On another branch than the default branch of the repository.
    clone.git(&clone.repo, &["symbolic-ref", "HEAD", "refs/heads/dev"]);
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(
            "riff.toml is on main now. riff ran no git pull in this clone: the clone is on dev, \
             not on main. Run git pull in this clone to get it."
        ),
        "{stdout}"
    );
    assert!(!clone.repo.join("riff.toml").exists());
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY,
/// 01M5RGGD9AYNC12Z2CG5CDZWNA: `riff project init` in a clone of a
/// repository with no setup, that lacks only the commits of `origin`. The
/// first list has `step` lines, and no failed check. After the steps the clone
/// has `riff.toml` with no `git pull` by hand, the second list shows each
/// check, and the end names the one whose step failed.
#[tokio::test]
async fn riff_project_init_shows_the_steps_pulls_riff_toml_and_names_the_check_that_still_fails() {
    let clone = Clone::new().await;
    let behind = clone.clone_behind_origin();
    bare(&clone.bin);
    // The step of the ruleset fails: GitHub refuses it.
    std::fs::write(clone.bin.join("fail"), "rulesets").unwrap();
    assert!(!behind.join("riff.toml").exists());
    let out = clone.project_in(&behind, &["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    let (first, rest) = stdout
        .split_once("riff checks the repository again:\n")
        .expect("a second list");

    // The first list: a `step` for each fact that a step fixes, and no
    // `fail`.
    let steps: Vec<&str> = step_lines(first)
        .iter()
        .map(|l| l.split_once(": ").unwrap().0)
        .collect();
    assert_eq!(
        steps,
        [
            "  step  riff.toml on main",
            "  step  auto-merge and squash merge",
            "  step  the ruleset of main",
            "  step  the milestone Backlog",
            "  step  the first wave",
        ],
        "{first}"
    );
    // No check fails before the steps: the one `fail` is the step that
    // GitHub refused.
    let failed = failed_lines(first);
    assert_eq!(failed.len(), 1, "{first}");
    assert!(
        failed[0].starts_with("  fail  make the ruleset riff on main: "),
        "{}",
        failed[0]
    );

    // The steps ran, and riff ran `git pull --ff-only`.
    assert!(
        first.contains(
            "riff.toml is on main now. riff ran git pull --ff-only in this clone, so the clone \
             has it."
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("Run git pull"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(behind.join("riff.toml")).unwrap(),
        format!("[session]\nimage = \"{IMAGE}\"\n")
    );

    // The second list shows each check; only the ruleset fails; the end
    // names it.
    let (second, end) = rest.split_once("1 check failed.").expect("an end");
    assert!(step_lines(second).is_empty(), "{second}");
    let failed = failed_lines(second);
    assert_eq!(failed.len(), 1, "{second}");
    assert!(
        failed[0].starts_with("  fail  the ruleset of main: "),
        "{}",
        failed[0]
    );
    for line in [
        "  pass  riff.toml on main: main of acme/app has it",
        "  pass  auto-merge and squash merge: acme/app allows both",
        "  pass  the milestone Backlog: acme/app has it open",
    ] {
        assert!(
            second.lines().any(|l| l == line),
            "{line}\nnot in\n{second}"
        );
    }
    assert!(
        end.starts_with(
            " riff enrolled no project. Do each fix, then run riff project init again:\n  the \
             ruleset of main: "
        ),
        "{end}"
    );
    assert_eq!(named_at_the_end(&stdout).len(), 1, "{stdout}");
    assert!(clone.repo_reads().is_empty());
}

/// 01M5SMNKDWBD1J6Y18ENQG87CK: a repository with no CI has no check but
/// `riff/verify`. `riff project init --yes` makes the ruleset with that
/// check alone, names no job `Gate`, says so in a note, and exits 0.
#[tokio::test]
async fn riff_project_init_yes_makes_a_ruleset_with_the_verify_alone_for_a_repository_with_no_ci() {
    let clone = Clone::new().await;
    no_ci(&clone.bin);
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    let (first, second) = stdout
        .split_once("riff checks the repository again:\n")
        .expect("a second list");
    // The ruleset is the one step. The check of the CI passes, with its note.
    assert_eq!(step_lines(first).len(), 1, "{first}");
    assert!(step_lines(first)[0].starts_with("  step  the ruleset of main: "));
    assert!(failed_lines(first).is_empty(), "{first}");
    assert!(
        first.contains("  done  make the ruleset riff on main: requires riff/verify\n"),
        "{first}"
    );
    let note = "  pass  the checks of a pull request: acme/app has no CI: no pull request ran a \
                check, so riff/verify is the only check";
    assert!(first.lines().any(|l| l == note), "{first}");
    assert!(second.lines().any(|l| l == note), "{second}");
    assert!(failed_lines(&stdout).is_empty(), "{stdout}");
    assert!(!stdout.contains("Gate"), "{stdout}");
    // The ruleset requires the verify and no other check.
    let posts: Vec<String> = clone
        .read("calls")
        .lines()
        .filter(|l| l.contains("rulesets"))
        .map(str::to_owned)
        .collect();
    assert!(posts.iter().any(|l| l.contains("-X POST")), "{posts:?}");
    let body = clone.read("ruleset");
    assert!(body.contains(r#""context":"riff/verify""#), "{body}");
    assert!(!body.contains("Gate"), "{body}");
    assert_eq!(body.matches("\"context\"").count(), 1, "{body}");
}

/// 01M76SHAX0KR0DM9GPXAPZJRDZ: `riff project init --check` passes a
/// repository that is set up, and enrolls nothing.
#[tokio::test]
async fn riff_project_init_check_passes_a_repository_that_is_set_up_and_enrolls_nothing() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for line in [
        "  pass  riff.toml on main: main of acme/app has it",
        "  pass  auto-merge and squash merge: acme/app allows both",
        "  pass  the ruleset of main: it requires the check riff/verify",
        "  pass  the milestone Backlog: acme/app has it open",
        "  pass  the first wave: acme/app has the open wave Wave 1",
        "  pass  the checks of a pull request: the full check is Gate",
        "  pass  the guard: the riff has no session App, so riff writes no guard",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    assert!(
        stdout.ends_with(
            "Each check passed. riff changed nothing and enrolled no project: --check.\n"
        ),
        "{stdout}"
    );
    assert!(clone.repo_reads().is_empty());
    let listed = text(&clone.project(&["list"]).await.stdout);
    assert!(listed.starts_with("The riff has no project."), "{listed}");
}

/// 01M6WPV86SW4AAVA2NCBQNYXK3: when riff cannot ask riff-server for the
/// bot of the session App, `riff project init --check` names a repository
/// with no guard, with the fix and no step. A guard that is there passes.
#[tokio::test]
async fn riff_project_init_check_names_a_repository_with_no_guard_when_riff_server_does_not_answer()
{
    let mut clone = Clone::new().await;
    clone.server = None;
    set_up(&clone.bin);
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let failed = failed_lines(&stdout);
    assert_eq!(failed.len(), 1, "{stdout}");
    assert!(
        failed[0].starts_with(
            "  fail  the guard: main of acme/app has no .github/workflows/riff-guard.yml, and \
             riff cannot ask riff-server at http://127.0.0.1:9 for the bot of the session App: "
        ),
        "{}",
        failed[0]
    );
    assert!(
        failed[0].ends_with(
            "Fix: start riff-server, or sign in with riff login. Then run riff project init again"
        ),
        "{}",
        failed[0]
    );
    assert!(step_lines(&stdout).is_empty(), "{stdout}");

    std::fs::write(
        clone.bin.join("guard"),
        riff::guard::workflow("riff-acme[bot]"),
    )
    .unwrap();
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("\n  pass  the guard: main of acme/app has it\n"),
        "{stdout}"
    );
    assert!(clone.forge_changes().is_empty());
}

/// The open issues of the fake forge: `(number, title, label, body)`.
fn issues(bin: &Path, open: &[(u64, &str, &str, &str)]) {
    let list: Vec<serde_json::Value> = open
        .iter()
        .map(|(number, title, label, body)| {
            let labels = if label.is_empty() {
                serde_json::json!([])
            } else {
                serde_json::json!([{ "name": label }])
            };
            serde_json::json!({ "number": number, "title": title, "body": body, "labels": labels })
        })
        .collect();
    std::fs::write(bin.join("issues"), serde_json::to_string(&list).unwrap()).unwrap();
}

/// 01M5SMMP7ZW2PZNGK63WWZAC8P, 01M6QCXGC42AG1RY5QNGT47GVV: a repository with
/// 4 open issues, one of them with the line `Done when:`, one with the
/// heading `## Done when`, and a tracker with no line. The note names the 2
/// others, `--yes` and `--check` print it, and no exit code changes.
#[tokio::test]
async fn riff_project_init_names_the_open_issues_with_no_done_when_line() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    issues(
        &clone.bin,
        &[
            (11, "Has the line", "", "Text.\n\nDone when: it works."),
            (
                15,
                "Has the heading",
                "",
                "Text.\n\n## Done when\n- it works",
            ),
            (12, "No line", "", "Text only."),
            (13, "Also no line", "bug", ""),
            (14, "Tracker: the parts", "tracker", "- #11"),
        ],
    );
    let note = "2 open issues of acme/app have no \"Done when:\" line and no \"## Done when\" \
                heading, so the wave starts no session for them: #12, #13. Add the line to each \
                issue. See \"Add a Done when line to an issue\" in the book.";
    for args in [&["init", "--check"][..], &["init", "--yes"]] {
        let out = clone.project(args).await;
        let stdout = text(&out.stdout);
        assert!(out.status.success(), "{args:?}: {stdout}");
        assert_eq!(
            stdout.lines().filter(|l| *l == note).count(),
            1,
            "{args:?}\n{note}\nnot once in\n{stdout}"
        );
        assert!(
            !stdout.contains("#11") && !stdout.contains("#14") && !stdout.contains("#15"),
            "{stdout}"
        );
        assert!(
            clone.read("calls").contains("issue list --state open"),
            "{args:?}"
        );
        // The note comes after the list of the checks.
        assert!(
            stdout.find("  pass  the checks of a pull request").unwrap()
                < stdout.find(note).unwrap(),
            "{stdout}"
        );
    }
    // The book shows the note.
    assert!(
        crate::book::page("development.md").contains(note),
        "the book does not show the note:\n{note}"
    );
    assert!(clone.forge_changes().is_empty());
}

/// 01M5SMMP7ZW2PZNGK63WWZAC8P: no note when each open issue has the line or
/// is a tracker, and a note about the read when `gh` cannot list the issues.
#[tokio::test]
async fn riff_project_init_prints_no_note_when_each_open_issue_has_the_line() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    issues(
        &clone.bin,
        &[
            (11, "Has the line", "", "Done when: it works."),
            (14, "Tracker: the parts", "", "- #11"),
        ],
    );
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(!stdout.contains("Done when"), "{stdout}");
    assert!(clone.read("calls").contains("issue list --state open"));

    std::fs::write(clone.bin.join("fail"), "issue list").unwrap();
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    let unread = stdout
        .lines()
        .find(|l| l.starts_with("riff cannot list the open issues of acme/app"))
        .unwrap_or_else(|| panic!("no note in\n{stdout}"));
    assert!(unread.contains("Must have admin rights"), "{unread}");
}

/// A ruleset that requires the verify and no check of the CI, with pull
/// requests that ran checks: the check of the CI has no step. riff names the
/// fix, and does nothing for it. A repository with no `CLAUDE.md` has no line
/// for it (01M5RN89Z749VYS1CRYJQTKAWY).
#[tokio::test]
async fn riff_project_init_names_the_fix_of_a_ci_check_that_no_rule_requires_and_has_no_line_for_claude_md()
 {
    let clone = Clone::new().await;
    ci_not_required(&clone.bin);
    let out = clone.project(&["init", "--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let failed = failed_lines(&stdout);
    assert_eq!(failed.len(), 1, "{stdout}");
    assert!(
        failed[0].starts_with(
            "  fail  the checks of a pull request: pull requests of acme/app ran checks / lint, \
             and no rule requires them"
        ),
        "{}",
        failed[0]
    );
    assert!(
        failed[0].contains("Fix: add them to the ruleset of main."),
        "{}",
        failed[0]
    );
    assert!(!stdout.contains("Gate"), "{stdout}");
    assert!(!stdout.contains("CLAUDE.md"), "{stdout}");
    assert!(clone.forge_changes().is_empty());
}

#[tokio::test]
async fn riff_project_init_check_fails_one_check_when_gh_cannot_read_the_repository() {
    let clone = Clone::new().await;
    set_up(&clone.bin);
    std::fs::write(clone.bin.join("no_login"), "").unwrap();
    let out = clone.project(&["init", "--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let failed = failed_lines(&stdout);
    assert_eq!(failed.len(), 1, "{stdout}");
    assert!(
        failed[0]
            .starts_with("  fail  the repository on GitHub: riff cannot read the repository: "),
        "{}",
        failed[0]
    );
    assert!(failed[0].contains("gh auth login"), "{}", failed[0]);
    assert!(stdout.contains("1 check failed."), "{stdout}");
}

/// The how-tos of `riff project init` in "Start a Riff" and "How It
/// Works" run real commands of riff.
#[test]
fn the_book_shows_riff_project_with_real_commands() {
    for page in ["start-a-riff.md", "how-it-works.md"] {
        let commands: Vec<String> = commands_of(page)
            .into_iter()
            .filter(|c| c.starts_with("riff project"))
            .collect();
        assert!(!commands.is_empty(), "{page} shows no riff project");
        each_is_real(&commands);
    }
    let part = crate::book::part("how-it-works.md", "The projects of the riff");
    for command in [
        "riff project init",
        "riff project list",
        "riff project remove OWNER/REPO",
    ] {
        assert!(part.contains(command), "{command} is not in the part");
    }
}

/// The `text` blocks of the part `heading` of the page `name`, each as its lines.
fn text_blocks(name: &str, heading: &str) -> Vec<Vec<String>> {
    let (mut blocks, mut open) = (Vec::new(), None::<Vec<String>>);
    for line in crate::book::part(name, heading).lines() {
        if line.trim() == "```text" {
            open = Some(Vec::new());
        } else if line.trim_start().starts_with("```") {
            blocks.extend(open.take());
        } else if let Some(block) = &mut open {
            block.push(line.to_owned());
        }
    }
    blocks
}

/// Each line of the block `book` is a line of `real`, in the same order. A
/// line `  ...` stands for lines that the book leaves out, and the answer
/// to the question is not in the output of `--yes`.
fn assert_is_output(book: &[String], real: &str) {
    let mut lines = real.lines();
    for line in book {
        if line == "  ..." || line == "Do them? [y/N] y" {
            continue;
        }
        assert!(
            lines.by_ref().any(|l| l == line),
            "{line}\nis not a line of the output, or it comes too early:\n{real}"
        );
    }
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY,
/// 01M5RGGD9AYNC12Z2CG5CDZWNA, 01M5SMNKDWBD1J6Y18ENQG87CK: "Add another
/// repository to the riff" shows the output of a first run, of a run that
/// ends with a failed check, and of a run in a repository with no CI. Each
/// line is a line of the real output.
#[tokio::test]
async fn the_book_shows_the_output_of_a_first_run_a_failed_run_and_a_run_with_no_ci() {
    let blocks = text_blocks("start-a-riff.md", "Add another repository to the riff");
    // The fourth block is the line of `riff image --check` (start_a_riff.rs).
    assert_eq!(blocks.len(), 4, "{blocks:#?}");

    let clone = Clone::new().await;
    let behind = clone.clone_behind_origin();
    bare(&clone.bin);
    let out = clone.project_in(&behind, &["init", "--yes"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_is_output(&blocks[0], &text(&out.stdout));

    let clone = Clone::new().await;
    ci_not_required(&clone.bin);
    let out = clone.project(&["init", "--yes"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert_is_output(&blocks[1], &text(&out.stdout));

    let clone = Clone::new().await;
    no_ci(&clone.bin);
    let out = clone.project(&["init", "--yes"]).await;
    assert_eq!(out.status.code(), Some(0));
    assert_is_output(&blocks[2], &text(&out.stdout));
}
