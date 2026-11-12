//! The guard of riff with fake events (01M6WPV2HK96JH900RX9ZPYHJ3,
//! 01M6WPV81PJDTFK6DAJ6MJHRQ3). The test does what GitHub does: it
//! takes the value of the `if` of the job and of each env of the step on
//! the event, and runs the script with `bash` and a fake `gh`. The fake
//! keeps the arguments of each call.

use std::path::Path;
use std::process::Command;

use riff::guard::{ENV, SCRIPT, condition};
use riff_core::wire::GitHubApp;
use serde_json::{Value, json};

/// The bot of the session App of the test.
const BOT: &str = "riff-acme[bot]";

/// The comment of the guard after it put back the title and the body.
const BOTH: &str = "riff put back the title and the body of this issue: a worker or a verifier \
                    changed it. A session adds to an issue with a comment \
                    (como-technologies/riff#1537).";

/// The value of `path` in `context`, or null.
fn lookup<'a>(context: &'a Value, path: &str) -> &'a Value {
    path.split('.')
        .try_fold(context, |value, key| value.get(key))
        .unwrap_or(&Value::Null)
}

/// The value of the expression of GitHub `expression` in `context`: a
/// path of `github`, `PATH != null` or `PATH == 'TEXT'`.
fn evaluate(expression: &str, context: &Value) -> Value {
    let path = |p: &str| lookup(context, p).clone();
    if let Some((left, right)) = expression.split_once(" != ") {
        assert_eq!(right, "null", "{expression}");
        return Value::Bool(!path(left).is_null());
    }
    if let Some((left, right)) = expression.split_once(" == ") {
        let text = right.strip_prefix('\'').and_then(|r| r.strip_suffix('\''));
        return Value::Bool(path(left) == json!(text.expect(expression)));
    }
    path(expression)
}

/// A value in the env of a step, as GitHub writes it: a text as it is, a
/// number and a bool as their text, null as nothing.
fn env_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        other => panic!("no value of the env: {other}"),
    }
}

/// The calls of `gh` in the folder `dir`, in order, each as its
/// arguments.
fn calls(dir: &Path) -> Vec<Vec<String>> {
    (0..)
        .map(|n| dir.join(format!("call-{n}")))
        .take_while(|path| path.exists())
        .map(|path| {
            let bytes = std::fs::read(path).unwrap();
            let text = String::from_utf8(bytes).unwrap();
            let mut args: Vec<String> = text.split('\0').map(str::to_owned).collect();
            args.pop();
            args
        })
        .collect()
}

/// The calls of `gh` of the guard for the event `name` with the payload
/// `event`, in the repository `acme/app`. No call when the `if` of the
/// job is false: GitHub starts no runner.
fn guard(name: &str, event: Value) -> Vec<Vec<String>> {
    let context = json!({
        "token": "the-token-of-the-run",
        "repository": "acme/app",
        "event_name": name,
        "event": event,
    });
    let context = json!({ "github": context });
    let run = evaluate(&condition(BOT), &context);
    if run != json!(true) {
        return vec![];
    }
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    isolated::write_program(
        bin.join("gh"),
        "#!/bin/sh\ndir=$(dirname \"$0\")\nn=0\nwhile [ -e \"$dir/call-$n\" ]; do n=$((n + 1)); done\n\
         for arg; do printf '%s\\0' \"$arg\"; done > \"$dir/call-$n\"\n",
    );
    let script = dir.path().join("script.sh");
    std::fs::write(&script, SCRIPT).unwrap();
    let mut command = Command::new("bash");
    command
        .args(["--noprofile", "--norc", "-eo", "pipefail"])
        .arg(&script)
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    for (name, expression) in ENV {
        command.env(name, env_text(&evaluate(expression, &context)));
    }
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let made: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "bin" && n != "script.sh")
        .collect();
    assert!(made.is_empty(), "the script made {made:?}");
    calls(&bin)
}

/// The arguments of one call of `gh`.
fn call(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// An event `issues: edited` of the issue 12 by `sender`, with `changes`.
fn edited(sender: &str, changes: Value) -> Value {
    json!({
        "action": "edited",
        "issue": { "number": 12, "title": "A new title", "body": "A new body", "user": { "login": "mike" } },
        "changes": changes,
        "sender": { "login": sender, "type": "Bot" },
    })
}

/// An event `issue_comment` with `action` of the comment 99 of
/// `author` on the issue 12, by `sender`.
fn comment(action: &str, sender: &str, author: &str, changes: Value) -> Value {
    json!({
        "action": action,
        "issue": { "number": 12, "title": "A title", "body": "A body" },
        "comment": {
            "id": 99,
            "body": "The text of the comment\nin two lines.",
            "created_at": "2026-11-08T10:00:00Z",
            "user": { "login": author },
        },
        "changes": changes,
        "sender": { "login": sender },
    })
}

/// A change of the body and the title by the bot is put back, with one
/// comment.
#[test]
fn a_change_of_the_body_and_the_title_by_the_bot_is_put_back_with_one_comment() {
    let changes = json!({
        "title": { "from": "The old title" },
        "body": { "from": "The old body\nwith `code`, $(touch made) and \"quotes\"." },
    });
    assert_eq!(
        guard("issues", edited(BOT, changes)),
        [
            call(&[
                "api",
                "--method",
                "PATCH",
                "repos/acme/app/issues/12",
                "-f",
                "title=The old title",
                "-f",
                "body=The old body\nwith `code`, $(touch made) and \"quotes\".",
            ]),
            call(&[
                "api",
                "--method",
                "POST",
                "repos/acme/app/issues/12/comments",
                "-f",
                &format!("body={BOTH}"),
            ]),
        ]
    );
}

/// The guard puts back only what the bot changed, also an empty body.
#[test]
fn the_guard_puts_back_only_what_changed() {
    let calls = guard("issues", edited(BOT, json!({ "body": { "from": "" } })));
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(
        calls[0],
        call(&[
            "api",
            "--method",
            "PATCH",
            "repos/acme/app/issues/12",
            "-f",
            "body="
        ])
    );
    assert!(calls[1][5].starts_with("body=riff put back the body of this issue: "));

    let calls = guard("issues", edited(BOT, json!({ "title": { "from": "Old" } })));
    assert_eq!(
        calls[0],
        call(&[
            "api",
            "--method",
            "PATCH",
            "repos/acme/app/issues/12",
            "-f",
            "title=Old"
        ])
    );
    assert!(calls[1][5].starts_with("body=riff put back the title of this issue: "));
}

/// A change by a person or by the lead App stays: GitHub starts no
/// runner for it.
#[test]
fn a_change_by_a_person_or_by_the_lead_app_stays() {
    let changes = json!({ "title": { "from": "Old" }, "body": { "from": "Old body" } });
    for sender in ["mike", "riff-acme-lead[bot]", "dependabot[bot]"] {
        assert!(
            guard("issues", edited(sender, changes.clone())).is_empty(),
            "{sender}"
        );
        let edit = comment(
            "edited",
            sender,
            "brett",
            json!({ "body": { "from": "Old" } }),
        );
        assert!(guard("issue_comment", edit).is_empty(), "{sender}");
        let delete = comment("deleted", sender, "brett", json!(null));
        assert!(guard("issue_comment", delete).is_empty(), "{sender}");
    }
}

/// A change and a delete of a comment of another account are put back.
#[test]
fn a_change_and_a_delete_of_a_comment_of_another_account_are_put_back() {
    let changes = json!({ "body": { "from": "The old text of mike." } });
    assert_eq!(
        guard("issue_comment", comment("edited", BOT, "mike", changes)),
        [call(&[
            "api",
            "--method",
            "PATCH",
            "repos/acme/app/issues/comments/99",
            "-f",
            "body=The old text of mike.",
        ])]
    );
    // A comment of the lead App is a comment of another account.
    let changes = json!({ "body": { "from": "The note of the lead." } });
    let calls = guard(
        "issue_comment",
        comment("edited", BOT, "riff-acme-lead[bot]", changes),
    );
    assert_eq!(calls.len(), 1, "{calls:?}");

    assert_eq!(
        guard(
            "issue_comment",
            comment("deleted", BOT, "mike", json!(null))
        ),
        [call(&[
            "api",
            "--method",
            "POST",
            "repos/acme/app/issues/12/comments",
            "-f",
            "body=riff put back this comment: a worker or a verifier deleted it. mike wrote it \
             at 2026-11-08T10:00:00Z.\n\nThe text of the comment\nin two lines.",
        ])]
    );
}

/// A change and a delete of a comment of the bot itself stay.
#[test]
fn a_change_of_a_comment_of_the_bot_itself_stays() {
    let changes = json!({ "body": { "from": "My old text." } });
    assert!(guard("issue_comment", comment("edited", BOT, BOT, changes)).is_empty());
    assert!(guard("issue_comment", comment("deleted", BOT, BOT, json!(null))).is_empty());
}

/// The workflow of this repository is the guard of the session App of
/// como-technologies, as `riff project init` writes it.
#[test]
fn this_repository_has_the_guard_of_its_session_app() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(riff::guard::PATH);
    let text = std::fs::read_to_string(&path).unwrap();
    let bot = format!("{}[bot]", GitHubApp::Session.name("como-technologies"));
    assert_eq!(bot, "riff-como-technologies[bot]");
    assert_eq!(text, riff::guard::workflow(&bot), "{}", path.display());
}
