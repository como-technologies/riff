//! 01M550XYBAW5QYXEMQ49Z34M4N: `riff retire LINE --issue N` retires a
//! release line on GitHub in one step (#795). A fake GitHub API, a fake
//! `gh` on `PATH`, keeps the state of each release, so a second run sees
//! the work of the first.

use crate::book;

use isolated::Isolated;
use std::fs;
use std::path::Path;
use std::process::Output;

/// A fake `gh` in `bin`. Its list has the Latest release and two other
/// releases of the line 1.x, and a release of each of the lines 2.x and
/// 0.x. It keeps each release in `bin/ID.json`, logs each call to
/// `bin/gh.log`, and writes the new state of a release at a PATCH.
fn fake_gh(bin: &Path) {
    let list = r#"[{"tagName":"v1.8.1","isLatest":true,"isDraft":false},{"tagName":"v1.8.0","isLatest":false,"isDraft":false},{"tagName":"v2.0.1","isLatest":false,"isDraft":false},{"tagName":"v1.7.0","isLatest":false,"isDraft":false},{"tagName":"v0.8.0","isLatest":false,"isDraft":false}]"#;
    fs::write(bin.join("list.json"), list).unwrap();
    for (tag, id, prerelease, body) in [
        ("v1.8.1", 81, false, "Level: patch."),
        ("v1.8.0", 80, false, "Level: minor."),
        (
            "v2.0.1",
            201,
            true,
            "**Retired.** The line 2.x is retired (#706).",
        ),
        ("v1.7.0", 70, false, "Level: minor."),
        ("v0.8.0", 8, false, "Level: minor."),
    ] {
        let release = serde_json::json!({"id": id, "prerelease": prerelease, "body": body});
        fs::write(bin.join(format!("{id}.json")), release.to_string()).unwrap();
        fs::write(bin.join(format!("{tag}.id")), id.to_string()).unwrap();
    }
    isolated::write_program(
        bin.join("gh"),
        "#!/bin/sh\nd=\"$(dirname \"$0\")\"\necho \"gh $*\" >> \"$d/gh.log\"\ncase \"$*\" in\n\
         'release list --json tagName,isLatest,isDraft --limit 1000') cat \"$d/list.json\" ;;\n\
         'api repos/{owner}/{repo}/releases/tags/'*) cat \"$d/$(cat \"$d/${2##*/}.id\").json\" ;;\n\
         'api -X PATCH repos/{owner}/{repo}/releases/'*' --include --input '*)\n\
         id=\"${4##*/}\"; { printf '{\"id\":%s,' \"$id\"; tail -c +2 \"$7\"; } > \"$d/$id.new\"\n\
         mv \"$d/$id.new\" \"$d/$id.json\"; printf 'HTTP/2.0 200 OK\\r\\n\\r\\n{}' ;;\n\
         *) exit 1 ;;\nesac\n",
    );
}

/// `riff retire ARGS` with the fake `gh` of `bin` first in `PATH`.
fn riff_retire(env: &Isolated, bin: &Path, args: &[&str]) -> Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    env.riff()
        .arg("retire")
        .args(args)
        .current_dir(bin)
        .env("PATH", path)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

/// The state of the release `id` in the fake.
fn release(bin: &Path, id: u64) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(bin.join(format!("{id}.json"))).unwrap()).unwrap()
}

/// The calls of the fake `gh` that change a release.
fn writes(bin: &Path) -> Vec<String> {
    fs::read_to_string(bin.join("gh.log"))
        .unwrap()
        .lines()
        .filter(|l| l.contains(" -X "))
        .map(|l| l.split(" --include").next().unwrap().to_owned())
        .collect()
}

#[test]
fn riff_retire_marks_each_release_of_the_line_and_a_second_run_changes_nothing() {
    let env = Isolated::new();
    let bin = tempfile::tempdir().unwrap();
    fake_gh(bin.path());
    let (two, zero) = (release(bin.path(), 201), release(bin.path(), 8));
    let latest = release(bin.path(), 81);

    let first = riff_retire(&env, bin.path(), &["1.x", "--issue", "961"]);
    assert!(first.status.success(), "{}", text(&first));
    assert_eq!(
        text(&first),
        "v1.8.1: GitHub marks it Latest, so riff kept it. Make a release of another line the \
         Latest, then run riff retire again.\n\
         v1.8.0: retired. It is a prerelease now, and its notes start with **Retired.**\n\
         v1.7.0: retired. It is a prerelease now, and its notes start with **Retired.**\n\
         riff retired 2 releases of the line 1.x.\n"
    );
    let line = "**Retired.** The line 1.x is retired (#961). Do not install this release.";
    for id in [80, 70] {
        assert_eq!(
            release(bin.path(), id),
            serde_json::json!({"id": id, "prerelease": true, "body": format!("{line}\n\nLevel: minor.")})
        );
    }
    assert_eq!(release(bin.path(), 81), latest);
    assert_eq!(release(bin.path(), 201), two);
    assert_eq!(release(bin.path(), 8), zero);
    assert_eq!(
        writes(bin.path()),
        [
            "gh api -X PATCH repos/{owner}/{repo}/releases/80",
            "gh api -X PATCH repos/{owner}/{repo}/releases/70",
        ]
    );

    let again = riff_retire(&env, bin.path(), &["1", "--issue", "961"]);
    assert!(again.status.success(), "{}", text(&again));
    assert!(
        text(&again).ends_with(
            "v1.8.0: retired already. riff changed nothing.\n\
             v1.7.0: retired already. riff changed nothing.\n\
             riff retired no release of the line 1.x.\n"
        ),
        "{}",
        text(&again)
    );
    assert_eq!(
        writes(bin.path()).len(),
        2,
        "the second run changed a release"
    );
}

#[test]
fn riff_retire_refuses_a_line_with_no_release_or_a_bad_line() {
    let env = Isolated::new();
    let bin = tempfile::tempdir().unwrap();
    fake_gh(bin.path());

    let none = riff_retire(&env, bin.path(), &["7.x", "--issue", "961"]);
    assert_eq!(none.status.code(), Some(1), "{}", text(&none));
    assert!(
        text(&none).contains("No GitHub release has a tag of the line 7.x"),
        "{}",
        text(&none)
    );

    let bad = riff_retire(&env, bin.path(), &["1.8", "--issue", "961"]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    assert!(
        text(&bad).contains("\"1.8\" is not a release line"),
        "{}",
        text(&bad)
    );
    assert!(writes(bin.path()).is_empty());
}

/// The how-to "Retire a release line" of `development.md` retires a line
/// with one `riff` command, and has no raw `gh` step that edits a
/// release.
#[test]
fn the_book_retires_a_line_with_one_riff_command() {
    let page = book::page("development.md");
    let start = page
        .find("\n### Retire a release line\n")
        .expect("development.md has \"### Retire a release line\"");
    let rest = &page[start + 1..];
    let part = rest[4..].find("\n##").map_or(rest, |end| &rest[..end + 4]);
    let commands = book::commands_in(part);
    assert!(
        commands.contains(&"riff retire 1.x --issue 961".to_owned()),
        "{commands:#?}"
    );
    assert!(
        !commands.iter().any(|c| c.contains("gh release edit")),
        "{commands:#?}"
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    book::each_is_real(&riff);
}
