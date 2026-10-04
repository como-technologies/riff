//! The image, the release check and the CI deploy (R135,
//! 01M3NJAZ6BYH7TWKDYTVEK78PG, 01M3NJAZAQ3AKMAM0EGM7R3S89,
//! 01M4262E0QJHCZXNH7EFG7FXN2). The tests of `riff cloud` are in
//! `crates/riff/tests/cloud.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn deploy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy")
}

/// The CI deploy is the `riff cloud deploy` of a person
/// (01M4262E0QJHCZXNH7EFG7FXN2).
#[test]
fn ci_deploys_after_the_gate_or_the_release_check_with_no_key() {
    let (_, job) = ci_parts();
    for part in [
        "needs: [gate, release]",
        "github.ref == 'refs/heads/main'",
        "id-token: write",
        "google-github-actions/auth@",
        "workload_identity_provider:",
        "cloud deploy shared \"$TAG\" --confirm shared",
        "RIFF_OWNER: ${{ vars.RIFF_OWNER }}",
    ] {
        assert!(job.contains(part), "{part} is not in the deploy job");
    }
    assert!(
        !job.contains("credentials_json"),
        "the job must not use a key"
    );
}

/// The deploy job of the CI workflow, and the triggers of the workflow.
fn ci_parts() -> (String, String) {
    let text = fs::read_to_string(deploy().join("../.github/workflows/ci.yml")).unwrap();
    let (on, rest) = text.split_once("\njobs:\n").unwrap();
    let job = rest
        .split_once("\n  deploy:\n")
        .unwrap()
        .1
        .split_once("\n  audit:\n")
        .unwrap()
        .0;
    (on.to_owned(), job.to_owned())
}

/// The `if:` of the deploy job, as one line.
fn deploy_if() -> String {
    let (_, job) = ci_parts();
    let when = job.split_once("\n    if: >-\n").unwrap().1;
    let when = when.split_once("\n    runs-on:").unwrap().0;
    when.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A value of a GitHub Actions expression.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str(String),
    Bool(bool),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty(),
            Value::Bool(b) => *b,
        }
    }
}

/// Evaluates the part of the GitHub Actions expression language that
/// the deploy job uses: strings, context paths, `==`, `!=`, `&&`, `||`,
/// `!`, parentheses, `startsWith` and `cancelled`. A context path that
/// `ctx` does not name is the empty string. It panics on each other
/// part, so a new part of the `if:` needs a new part here.
struct Eval<'a> {
    rest: &'a str,
    ctx: &'a [(&'a str, &'a str)],
}

impl Eval<'_> {
    fn eat(&mut self, token: &str) -> bool {
        self.rest = self.rest.trim_start();
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn or(&mut self) -> Value {
        let mut value = self.and();
        while self.eat("||") {
            let right = self.and();
            value = Value::Bool(value.truthy() || right.truthy());
        }
        value
    }

    fn and(&mut self) -> Value {
        let mut value = self.unary();
        while self.eat("&&") {
            let right = self.unary();
            value = Value::Bool(value.truthy() && right.truthy());
        }
        value
    }

    fn unary(&mut self) -> Value {
        if self.eat("!") {
            return Value::Bool(!self.unary().truthy());
        }
        let left = self.primary();
        if self.eat("==") {
            Value::Bool(left == self.primary())
        } else if self.eat("!=") {
            Value::Bool(left != self.primary())
        } else {
            left
        }
    }

    fn primary(&mut self) -> Value {
        if self.eat("(") {
            let value = self.or();
            assert!(self.eat(")"), "no ): {}", self.rest);
            return value;
        }
        if self.eat("'") {
            let (text, rest) = self.rest.split_once('\'').unwrap();
            self.rest = rest;
            return Value::Str(text.to_owned());
        }
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
            .unwrap_or(self.rest.len());
        let (name, rest) = self.rest.split_at(end);
        assert!(!name.is_empty(), "no value at: {}", self.rest);
        self.rest = rest;
        if self.eat("(") {
            let mut args = Vec::new();
            while !self.eat(")") {
                args.push(self.or());
                self.eat(",");
            }
            return match (name, args.as_slice()) {
                ("cancelled", []) => Value::Bool(false),
                ("startsWith", [Value::Str(text), Value::Str(start)]) => {
                    Value::Bool(text.starts_with(start.as_str()))
                }
                _ => panic!("no function {name}({args:?})"),
            };
        }
        let value = self.ctx.iter().find(|(key, _)| *key == name);
        Value::Str(value.map_or("", |(_, value)| value).to_owned())
    }
}

/// Does the deploy job run in the context `ctx`?
fn deploys(ctx: &[(&str, &str)]) -> bool {
    let when = deploy_if();
    let mut eval = Eval { rest: &when, ctx };
    let value = eval.or();
    assert!(eval.rest.trim().is_empty(), "left: {}", eval.rest);
    value.truthy()
}

type Ctx = Vec<(&'static str, &'static str)>;

/// The context of a push of `git_ref`, with the results of the gate
/// and the Release check.
fn push(git_ref: &'static str, gate: &'static str, release: &'static str) -> Ctx {
    vec![
        ("github.event_name", "push"),
        ("github.ref", git_ref),
        ("vars.CLOUD_DEPLOY", "true"),
        ("needs.gate.result", gate),
        ("needs.release.result", release),
    ]
}

/// The context of a run by hand on `git_ref` with the input `tag`.
fn dispatch(git_ref: &'static str, tag: &'static str, gate: &'static str) -> Ctx {
    vec![
        ("github.event_name", "workflow_dispatch"),
        ("github.ref", git_ref),
        ("inputs.tag", tag),
        ("vars.CLOUD_DEPLOY", "true"),
        ("needs.gate.result", gate),
        ("needs.release.result", "skipped"),
    ]
}

/// The same context with no `CLOUD_DEPLOY`.
fn off(mut ctx: Ctx) -> Ctx {
    ctx.retain(|(key, _)| *key != "vars.CLOUD_DEPLOY");
    ctx
}

/// 01M3NJAZ6BYH7TWKDYTVEK78PG, 01M3NJAZ8HSE87H0GZ8SNGWN31: a release
/// tag deploys after its Release check, and a run by hand with the
/// input `tag` deploys after the gate. A push to main never deploys.
#[test]
fn a_release_tag_or_a_run_by_hand_deploys() {
    let (on, job) = ci_parts();
    assert!(on.contains("\n  workflow_dispatch:\n"), "{on}");
    assert!(on.contains("\n      tag:\n"), "{on}");
    assert!(on.contains("type: string"), "{on}");
    assert!(on.contains("tags: [\"v*\"]"), "{on}");
    assert!(!job.contains("github.event.before"), "{job}");

    assert!(deploys(&push("refs/tags/v0.6.0", "skipped", "success")));
    assert!(deploys(&dispatch("refs/heads/main", "v0.5.0", "success")));

    for gate in ["success", "skipped", "failure"] {
        for release in ["success", "skipped", "failure"] {
            let ctx = push("refs/heads/main", gate, release);
            assert!(!deploys(&ctx), "a push to main deploys: {ctx:?}");
        }
    }
    // A tag that is not vX.Y.Z fails its Release check.
    assert!(!deploys(&push("refs/tags/v0.6", "skipped", "failure")));
    assert!(!deploys(&push("refs/tags/nightly", "skipped", "success")));
    assert!(!deploys(&push("refs/tags/v0.6.0", "skipped", "cancelled")));
    assert!(!deploys(&dispatch("refs/heads/main", "v0.5.0", "failure")));
    assert!(!deploys(&dispatch("refs/heads/main", "", "success")));
    assert!(!deploys(&dispatch("refs/heads/other", "v0.5.0", "success")));
    assert!(!deploys(&off(push(
        "refs/tags/v0.6.0",
        "skipped",
        "success"
    ))));
    assert!(!deploys(&off(dispatch(
        "refs/heads/main",
        "v0.5.0",
        "success"
    ))));
}

/// The part of the Development page under `heading`, up to the next
/// heading of its level.
fn book_part(heading: &str) -> String {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page.find(heading).unwrap()..];
    part[..part[4..].find("\n### ").unwrap()].to_owned()
}

/// The book says that the release tag deploys, and that each machine
/// updates after it (01M3NJAZ8HSE87H0GZ8SNGWN31).
#[test]
fn the_book_deploys_at_the_end_of_a_wave() {
    let part = book_part("### Deploy the shared server at the end of a wave\n");
    assert!(part.contains("The release tag deploys itself."), "{part}");
    assert!(part.contains("`riff update`"), "{part}");
    assert!(part.contains("`riff update --auto on`"), "{part}");
    assert!(part.contains("riff workers start"), "{part}");
    assert!(!part.contains("gh workflow run"), "{part}");
}

/// The book how-to deploys again or rolls back with the real input
/// name of the workflow.
#[test]
fn the_book_deploys_a_release_again() {
    let part = book_part("### Deploy a release again, or roll back\n");
    assert!(part.contains("```sh\n"), "{part}");
    assert!(
        part.contains("gh workflow run CI --ref main -f tag=v0.2.0"),
        "{part}"
    );
}

/// The book how-to makes a release: bump, merge, tag, with the real
/// commands (01M3N73AW2J3TVSZWFJ88A91PG). The pull request states the
/// level and the reason, and the notes list what each person runs
/// (01M3N73EGY4NQDQQP8Y185E9VZ). GitHub generates the rest of the
/// notes (01M3NB3EWE2V9PCTMNTZAKEXMA).
#[test]
fn the_book_makes_a_release() {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page.find("### Make a release\n").unwrap()..];
    let part = &part[..part[4..].find("\n### ").unwrap()];
    for text in [
        "An admin makes the release",
        "(how-it-works.md#the-test-for-each-release)",
        "states the level and the\nreason in one line",
        "```text\nLevel: minor.",
        "the commands that each person\nruns",
        "`riff update --tag v0.2.0`",
        "gh pr create --title \"Release v0.2.0\" --label release --body-file pr.md\n",
        "the level line\nof the pull request",
        "gh release create v0.2.0 --verify-tag --title v0.2.0 --notes-file notes.md --generate-notes\n",
        "`--notes-start-tag v0.1.0`",
        "riff workers stop\n",
        "sed -i 's/^version = \".*\"/version = \"0.2.0\"/' Cargo.toml\n",
        "sed -i 's/\"version\": \".*\"/\"version\": \"0.2.0\"/' crates/riff/claude-plugin/riff/.claude-plugin/plugin.json\n",
        "cargo update --workspace\n",
        "git tag v0.2.0 origin/main\n",
        "git push origin v0.2.0\n",
        "`Release check`",
    ] {
        assert!(part.contains(text), "{text:?} is not in: {part}");
    }
    let root = tempfile::tempdir().unwrap();
    fs::copy(
        deploy().join("../Cargo.toml"),
        root.path().join("Cargo.toml"),
    )
    .unwrap();
    let bump = Command::new("sh")
        .arg("-c")
        .arg("sed -i 's/^version = \".*\"/version = \"0.2.0\"/' Cargo.toml")
        .current_dir(root.path())
        .status()
        .unwrap();
    assert!(bump.success());
    let bumped = fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
    assert!(bumped.contains("\nversion = \"0.2.0\"\n"), "{bumped}");
    assert!(bumped.contains("tokio = { version = \"1."), "{bumped}");
    let plugin = root.path().join("plugin.json");
    fs::copy(
        deploy().join("../crates/riff/claude-plugin/riff/.claude-plugin/plugin.json"),
        &plugin,
    )
    .unwrap();
    let bump = Command::new("sh")
        .arg("-c")
        .arg("sed -i 's/\"version\": \".*\"/\"version\": \"0.2.0\"/' plugin.json")
        .current_dir(root.path())
        .status()
        .unwrap();
    assert!(bump.success());
    let bumped = fs::read_to_string(&plugin).unwrap();
    assert!(bumped.contains("\"version\": \"0.2.0\""), "{bumped}");
}

/// Runs `deploy/release-check.sh` with `args`.
fn release_check(args: &[&str]) -> std::process::Output {
    Command::new(deploy().join("release-check.sh"))
        .args(args)
        .output()
        .unwrap()
}

/// A checkout whose crates have the version `crates` in Cargo.toml and
/// `locked` in Cargo.lock.
fn checkout(crates: &str, locked: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        format!("[workspace.package]\nedition = \"2024\"\nversion = \"{crates}\"\n"),
    )
    .unwrap();
    let lock: String = ["riff", "riff-core", "riff-server"]
        .iter()
        .map(|name| format!("[[package]]\nname = \"{name}\"\nversion = \"{locked}\"\n\n"))
        .collect();
    fs::write(root.path().join("Cargo.lock"), lock).unwrap();
    root
}

/// 01M3MRMAY3P1K151RGAP9K6GSH: the deploy checks out the release tag,
/// the pushed tag or its input, and checks it before it builds. So it
/// refuses a tag that is not vX.Y.Z.
#[test]
fn the_deploy_takes_only_a_release_tag() {
    let (_, job) = ci_parts();
    let checkout = job.find("ref: refs/tags/${{ env.TAG }}").unwrap();
    let check = job.find("run: deploy/release-check.sh \"$TAG\"").unwrap();
    let build = job.find("docker/build-push-action@").unwrap();
    assert!(checkout < check && check < build, "{job}");
    assert!(
        job.contains("TAG: ${{ github.event_name == 'push' && github.ref_name || inputs.tag }}"),
        "{job}"
    );
    assert!(job.contains("riff-server:$TAG"), "{job}");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.to_str().unwrap();
    for bad in ["", "main", "v0.2", "0.2.0", "v0.2.0-rc1", "refs/heads/main"] {
        let out = release_check(&[bad, root]);
        assert!(!out.status.success(), "{bad} passed");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("\"{bad}\" is not a release tag. Give vX.Y.Z")),
            "{stderr}"
        );
    }
    let this = format!("v{}", env!("CARGO_PKG_VERSION"));
    let out = release_check(&[&this, root]);
    assert!(out.status.success(), "{out:?}");
}

/// 01M3N73AW2J3TVSZWFJ88A91PG: CI checks each pushed tag v*. The check
/// fails a tag whose version is not the version of the crates.
#[test]
fn the_tag_check_fails_a_tag_of_another_version() {
    let text = fs::read_to_string(deploy().join("../.github/workflows/ci.yml")).unwrap();
    let (on, jobs) = text.split_once("\njobs:\n").unwrap();
    assert!(on.contains("tags: [\"v*\"]"), "{on}");
    let job = jobs
        .split_once("\n  release:\n")
        .unwrap()
        .1
        .split_once("\n  deploy:\n")
        .unwrap()
        .0;
    assert!(job.contains("if: github.ref_type == 'tag'"), "{job}");
    assert!(
        job.contains("run: deploy/release-check.sh \"$GITHUB_REF_NAME\""),
        "{job}"
    );

    let root = checkout("0.2.0", "0.2.0");
    let path = root.path().to_str().unwrap();
    assert!(release_check(&["v0.2.0", path]).status.success());
    let out = release_check(&["v0.3.0", path]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("the tag v0.3.0 is not the version of the crates in Cargo.toml: 0.2.0."),
        "{out:?}"
    );
    let root = checkout("0.2.0", "0.1.0");
    let out = release_check(&["v0.2.0", root.path().to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("the tag v0.2.0 is not the version of riff in Cargo.lock: 0.1.0."),
        "{out:?}"
    );
}

#[test]
fn the_image_holds_only_the_binary_and_the_certificates() {
    let text = fs::read_to_string(deploy().join("../Dockerfile")).unwrap();
    let last = text.rsplit_once("\nFROM ").unwrap().1;
    assert!(last.starts_with("scratch\n"), "{last}");
    let copies: Vec<&str> = last.lines().filter(|l| l.starts_with("COPY")).collect();
    assert_eq!(copies.len(), 2, "{copies:?}");
    assert!(copies.iter().any(|c| c.ends_with(" /riff-server")));
    assert!(copies.iter().any(|c| c.contains("ca-certificates.crt")));
    let user = last.lines().find(|l| l.starts_with("USER ")).unwrap();
    assert!(!user.contains(" 0") && !user.contains("root"), "{user}");
}

/// The generated notes of the GitHub releases are the changelog: they
/// leave out the release pull requests, no changelog file exists, and
/// the book says where to read them (01M3NB3EWE2V9PCTMNTZAKEXMA).
#[test]
fn the_release_notes_are_the_changelog() {
    let root = deploy().join("..");
    let config = fs::read_to_string(root.join(".github/release.yml")).unwrap();
    assert!(
        config.contains("changelog:\n  exclude:\n    labels:\n      - release\n"),
        "{config}"
    );
    for name in ["CHANGELOG.md", "CHANGELOG", "docs/src/changelog.md"] {
        assert!(!root.join(name).exists(), "{name} exists");
    }
    let page = fs::read_to_string(root.join("docs/src/how-it-works.md")).unwrap();
    let part = &page[page
        .find("### See what changed in a release\n")
        .expect("no how-to")..];
    let part = &part[..part[4..].find("\n### ").unwrap() + 4];
    assert!(
        part.contains(
            "```sh\ngh release list --repo como-technologies/riff\n\
             gh release view v0.4.0 --repo como-technologies/riff\n```"
        ),
        "{part}"
    );
}

/// The notes put the pull requests in three groups: the changes for
/// people first, then `design`, then `internal`, and the book says so
/// (01M40AB1D5KQ4SERC6M709Z3JF).
#[test]
fn the_release_notes_group_the_pull_requests() {
    let root = deploy().join("..");
    let config = fs::read_to_string(root.join(".github/release.yml")).unwrap();
    let groups = &config[config.find("  categories:\n").expect("no groups")..];
    assert!(
        groups.contains(
            "    - title: Changes\n      labels:\n        - \"*\"\n      \
             exclude:\n        labels:\n          - design\n          - internal\n\
             \x20   - title: Design\n      labels:\n        - design\n\
             \x20   - title: Internal\n      labels:\n        - internal\n"
        ),
        "{groups}"
    );
    let page = fs::read_to_string(root.join("docs/src/how-it-works.md")).unwrap();
    let part = &page[page
        .find("### See what changed in a release\n")
        .expect("no how-to")..];
    let part = part.replace('\n', " ");
    assert!(
        part.contains("`Changes` for people first, then `Design`"),
        "{part}"
    );
    assert!(part.contains("then `Internal`"), "{part}");
}
