//! `hygiene pr N` checks pull request N, the cut of each requirement
//! that it supersedes in part, and the name of each requirement that it
//! adds in a file under `crates/`, with `gh`. `hygiene commit [REV]`
//! checks the message of a commit with `git`. `hygiene book [DIR]` builds
//! the book in DIR with `mdbook` and checks it. `hygiene wrap [DIR]`
//! checks the wrap of each Markdown file in DIR. `hygiene blank PATH...`
//! finds two blank lines in a row in each Markdown file in each PATH and
//! its subfolders. `hygiene versions [ROOT]` finds the name of a version
//! of riff in the code and the book. `hygiene ci [BASE]`
//! prints the recipe that `just ci` runs. `hygiene crates [BASE]` prints
//! the arguments of `cargo test` that `just check` runs. `hygiene
//! test-part I/N` runs part I of N of the tests. `hygiene test-build DIR`
//! builds the test binaries once and packs them into DIR, and `hygiene
//! test-part I/N --archive DIR` runs part I of N from DIR with no build.
//! `hygiene test-name NAME --archive DIR` runs the tests whose name has
//! NAME from DIR with no build. See the library docs.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use hygiene::{Issue, PullRequest, Rules};
use serde::de::DeserializeOwned;

const USAGE: &str = "usage: hygiene pr NUMBER | hygiene commit [REV] | hygiene book [DIR] \
                     | hygiene wrap [DIR] | hygiene blank PATH... \
                     | hygiene versions [ROOT] | hygiene ci [BASE] | hygiene crates [BASE] \
                     | hygiene test-part I/N [--archive DIR] | hygiene test-name NAME --archive DIR \
                     | hygiene test-build DIR";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["pr", number] => pr(number),
        ["commit"] => commit("HEAD"),
        ["commit", rev] => commit(rev),
        ["book"] => book("docs"),
        ["book", dir] => book(dir),
        ["wrap"] => wrap("docs/src"),
        ["wrap", dir] => wrap(dir),
        ["blank", ref paths @ ..] if !paths.is_empty() => blank(paths),
        ["versions"] => versions("."),
        ["versions", root] => versions(root),
        ["ci"] => ci("origin/main"),
        ["ci", base] => ci(base),
        ["crates"] => crates("origin/main"),
        ["crates", base] => crates(base),
        ["test-part", part] => test_part(part),
        ["test-part", part, "--archive", dir] => test_archive(part, Path::new(dir)),
        ["test-name", name, "--archive", dir] => test_named(name, Path::new(dir)),
        ["test-build", dir] => test_build(Path::new(dir)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Checks pull request `number` and its issue.
fn pr(number: &str) -> ExitCode {
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        eprintln!("error: {number:?} is not the number of a pull request\n{USAGE}");
        return ExitCode::from(2);
    }
    let json: serde_json::Value = match gh(&[
        "pr",
        "view",
        number,
        "--json",
        "title,body,milestone,author,commits,baseRefOid,headRefOid",
    ]) {
        Ok(json) => json,
        Err(e) => return tool_error(&e),
    };
    let (pr, refs): (PullRequest, Refs) = match serde_json::from_value(json.clone())
        .and_then(|pr| Ok((pr, serde_json::from_value(json)?)))
    {
        Ok(read) => read,
        Err(e) => return tool_error(&format!("gh pr view {number}: {e}")),
    };
    let issue: Option<Issue> = match hygiene::issue_number(&pr) {
        Some(n) => {
            let n = n.to_string();
            match gh(&[
                "issue",
                "view",
                &n,
                "--json",
                "number,state,milestone,labels",
            ]) {
                Ok(issue) => Some(issue),
                Err(e) => return tool_error(&e),
            }
        }
        None => None,
    };
    let mut errors = hygiene::check_pr(&pr, issue.as_ref());
    let pages = match requirements(&refs) {
        Ok(pages) => pages,
        Err(e) => return tool_error(&e),
    };
    errors.extend(hygiene::check_requirements(&pages.old, &pages.new));
    let ids = hygiene::unnamed_ids(Rules::All, issue.as_ref(), &pages);
    if !ids.is_empty() {
        let files = match crates_at(&refs.head_ref_oid) {
            Ok(files) => files,
            Err(e) => return tool_error(&e),
        };
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        errors.extend(hygiene::check_unnamed(&ids, &files));
    }
    report(&format!("pull request #{number}"), &errors, HYGIENE)
}

/// The commits of a pull request, as `gh pr view --json
/// baseRefOid,headRefOid` gives them.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Refs {
    /// The tip of the base branch.
    base_ref_oid: String,
    /// The head of the pull request.
    head_ref_oid: String,
}

/// The pages of the requirements of pull request `refs`: the two pages at
/// the merge base of its base and its head, and the requirements at its
/// head. It reads them with `gh api`, so it needs no clone of the pull
/// request.
fn requirements(refs: &Refs) -> Result<hygiene::Pages, String> {
    let Refs {
        base_ref_oid: base,
        head_ref_oid: head,
    } = refs;
    let compare = format!("repos/{{owner}}/{{repo}}/compare/{base}...{head}");
    let fork = gh_text(&["api", &compare, "--jq", ".merge_base_commit.sha"])?;
    let fork = fork.trim();
    let file = |page: &str, sha: &str| {
        let path = format!("repos/{{owner}}/{{repo}}/contents/{page}?ref={sha}");
        gh_text(&[
            "api",
            "-H",
            "Accept: application/vnd.github.raw+json",
            &path,
        ])
    };
    Ok(hygiene::Pages {
        old: file(reqs::REQUIREMENTS, fork)?,
        history: file(reqs::HISTORY, fork)?,
        new: file(reqs::REQUIREMENTS, head)?,
    })
}

/// Each file under [`hygiene::CRATES`] at the commit `head`: its path
/// from the top of the repository, and its text. It reads the tarball of
/// the commit with `gh api`, and unpacks it with `tar` in a temporary
/// folder, so it needs no clone of the pull request.
fn crates_at(head: &str) -> Result<Vec<(String, String)>, String> {
    let api = format!("repos/{{owner}}/{{repo}}/tarball/{head}");
    let tarball = gh_bytes(&["api", &api])?;
    let dir = tempfile::tempdir().map_err(|e| format!("a temporary folder: {e}"))?;
    let file = dir.path().join("head.tar.gz");
    std::fs::write(&file, tarball).map_err(|e| format!("{}: {e}", file.display()))?;
    let top = dir.path().join("head");
    std::fs::create_dir(&top).map_err(|e| format!("{}: {e}", top.display()))?;
    let out = Command::new("tar")
        .arg("-xzf")
        .arg(&file)
        .arg("-C")
        .arg(&top)
        .output()
        .map_err(|e| format!("tar: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tar -xzf the tarball of {api}: {}", stderr.trim()));
    }
    // The tarball holds one folder, OWNER-REPO-SHA: the top of the commit.
    let mut files = Vec::new();
    let folders = std::fs::read_dir(&top).map_err(|e| format!("{}: {e}", top.display()))?;
    for folder in folders {
        let root = folder
            .map_err(|e| format!("{}: {e}", top.display()))?
            .path();
        let crates = root.join(hygiene::CRATES);
        if crates.is_dir() {
            tree(&root, &crates, &mut files).map_err(|e| format!("{}: {e}", crates.display()))?;
        }
    }
    files.sort();
    Ok(files)
}

/// Adds each file in `dir` and its subfolders to `files`: its path from
/// `root`, and its text. It follows no link.
fn tree(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            tree(root, &path, files)?;
        } else if kind.is_file() {
            let text = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
            let shown = path.strip_prefix(root).unwrap_or(&path);
            files.push((shown.to_string_lossy().into_owned(), text));
        }
    }
    Ok(())
}

/// Checks the message of commit `rev`.
fn commit(rev: &str) -> ExitCode {
    let out = match Command::new("git")
        .args(["log", "-1", "--format=%B", rev])
        .output()
    {
        Ok(out) => out,
        Err(e) => return tool_error(&format!("git: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return tool_error(&format!("git log {rev}: {}", stderr.trim()));
    }
    let message = String::from_utf8_lossy(&out.stdout);
    report(
        &format!("commit {rev}"),
        &hygiene::check_commit(&message),
        HYGIENE,
    )
}

/// Builds the book in `dir` with `mdbook build`, and checks its log and
/// each page in `dir/book`, except `print.html`: it repeats each page.
/// It installs the theme first when it is missing, and fails when the
/// install or the build changed a tracked file.
fn book(dir: &str) -> ExitCode {
    let before = tracked(dir);
    if let Err(e) = theme(dir) {
        return tool_error(&e);
    }
    let out = match Command::new("mdbook")
        .args(["build", dir])
        .env("NO_COLOR", "1")
        .output()
    {
        Ok(out) => out,
        Err(e) => return tool_error(&format!("mdbook build {dir}: {e}")),
    };
    let log = String::from_utf8_lossy(&out.stderr);
    eprint!("{log}");
    if !out.status.success() {
        return tool_error(&format!("mdbook build {dir}: {}", out.status));
    }
    let mut errors = hygiene::book::check_log(&log);
    let pages = Path::new(dir).join("book");
    let mut names = match std::fs::read_dir(&pages) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "html"))
            .filter(|p| p.file_name().is_some_and(|n| n != "print.html"))
            .collect::<Vec<_>>(),
        Err(e) => return tool_error(&format!("{}: {e}", pages.display())),
    };
    names.sort();
    for page in names {
        match std::fs::read_to_string(&page) {
            Ok(html) => {
                errors.extend(hygiene::book::check_page(
                    &page.display().to_string(),
                    &html,
                ));
            }
            Err(e) => return tool_error(&format!("{}: {e}", page.display())),
        }
    }
    errors.extend(hygiene::book::check_tracked(&before, &tracked(dir)));
    report(&format!("the book in {dir}"), &errors, BOOK)
}

/// Checks the wrap of each `.md` file in `dir`.
fn wrap(dir: &str) -> ExitCode {
    let mut pages = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect::<Vec<_>>(),
        Err(e) => return tool_error(&format!("{dir}: {e}")),
    };
    pages.sort();
    let mut errors = Vec::new();
    for page in pages {
        match std::fs::read_to_string(&page) {
            Ok(text) => errors.extend(hygiene::wrap::check(&page.display().to_string(), &text)),
            Err(e) => return tool_error(&format!("{}: {e}", page.display())),
        }
    }
    report(&format!("the wrap of the pages in {dir}"), &errors, WRAP)
}

/// Finds two blank lines in a row in each `.md` file in each of
/// `paths` and its subfolders.
fn blank(paths: &[&str]) -> ExitCode {
    let mut pages = Vec::new();
    for path in paths {
        if let Err(e) = markdown(Path::new(path), &mut pages) {
            return tool_error(&format!("{path}: {e}"));
        }
    }
    pages.sort();
    let mut errors = Vec::new();
    for page in pages {
        match std::fs::read_to_string(&page) {
            Ok(text) => errors.extend(hygiene::blank::check(&page.display().to_string(), &text)),
            Err(e) => return tool_error(&format!("{}: {e}", page.display())),
        }
    }
    report(
        &format!("the blank lines of the pages in {}", paths.join(", ")),
        &errors,
        BLANK,
    )
}

/// Adds each `.md` file in `dir` and its subfolders to `pages`.
fn markdown(dir: &Path, pages: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            markdown(&path, pages)?;
        } else if path.extension().is_some_and(|x| x == "md") {
            pages.push(path);
        }
    }
    Ok(())
}

/// Finds the names of versions of riff under `root` that the allow list
/// does not allow, and the lines of the list that allow nothing.
fn versions(root: &str) -> ExitCode {
    let root = Path::new(root);
    let allow = root.join(hygiene::versions::ALLOW_FILE);
    let source = match std::fs::read_to_string(&allow) {
        Ok(source) => source,
        Err(e) => return tool_error(&format!("{}: {e}", allow.display())),
    };
    let files = match hygiene::versions::scan(root) {
        Ok(files) => files,
        Err(e) => return tool_error(&format!("{}: {e}", root.display())),
    };
    let (allow, mut errors) = hygiene::versions::parse_allow(&source);
    errors.extend(hygiene::versions::check(&files, &allow));
    report(
        &format!("the version names in {}", root.display()),
        &errors,
        VERSIONS,
    )
}

/// Prints the recipe that `just ci` runs for the diff from the merge
/// base with `base` to stdout, and the set and the reason to stderr.
fn ci(base: &str) -> ExitCode {
    let changed = changed(base);
    let choice = hygiene::ci::choose(base, changed.as_deref());
    eprintln!("{choice}");
    println!("{}", choice.checks.recipe());
    ExitCode::SUCCESS
}

/// Prints the arguments of `cargo test` that `just check` runs for the
/// diff from the merge base with `base` to stdout, and the tests and
/// the reason to stderr. When cargo cannot read the workspace, the
/// tests of each crate.
fn crates(base: &str) -> ExitCode {
    let changed = changed(base);
    let choice = match members() {
        Ok(members) => hygiene::ci::tests(base, changed.as_deref(), &members),
        Err(e) => hygiene::ci::TestChoice {
            tests: hygiene::ci::Tests::Workspace,
            why: format!("cargo metadata: {e}"),
        },
    };
    eprintln!("{choice}");
    println!("{}", choice.tests.args());
    ExitCode::SUCCESS
}

/// Lists each test of the workspace, and runs the tests of part `part`
/// (01M513W2NSE303E6XAM18BW7T6). Part 1 also runs the doc tests. The
/// exit status is 1 when a `cargo test` fails, and 2 when the list holds
/// no test.
fn test_part(part: &str) -> ExitCode {
    let part: hygiene::part::Part = match part.parse() {
        Ok(part) => part,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let list = ["test", "--workspace", "--", "--list", "--format", "terse"];
    let out = match Command::new(&cargo)
        .args(list)
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(out) => out,
        Err(e) => return tool_error(&format!("cargo {}: {e}", list.join(" "))),
    };
    if !out.status.success() {
        eprintln!("error: cargo {}: {}", list.join(" "), out.status);
        return ExitCode::FAILURE;
    }
    let names = hygiene::part::names(&String::from_utf8_lossy(&out.stdout));
    if names.is_empty() {
        return tool_error(&format!("cargo {} listed no test", list.join(" ")));
    }
    let plan = part.plan(&names);
    eprintln!("{}", hygiene::part::summary(part, &plan, &names));
    let mut runs: Vec<Vec<&str>> = Vec::new();
    // With no name, cargo test would run each test.
    if !plan.tests.is_empty() {
        let mut run = vec![
            "test",
            "--workspace",
            "--tests",
            "--no-fail-fast",
            "--",
            "--exact",
        ];
        run.extend(&plan.tests);
        runs.push(run);
    }
    if plan.doc > 0 {
        runs.push(vec!["test", "--workspace", "--doc", "--no-fail-fast"]);
    }
    let mut failed = false;
    for run in runs {
        match Command::new(&cargo).args(&run).status() {
            Ok(status) => failed |= !status.success(),
            Err(e) => return tool_error(&format!("cargo {}: {e}", run[..3].join(" "))),
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The `TMPDIR` of each test run: `.cargo/config.toml` forces it for
/// each cargo run (01M49NP2907J4SH4S6MAY09VXE), and a test binary that
/// runs with no cargo gets it here.
const TMPDIR: &str = "/var/tmp";

/// The test build of the Gate (01M6QQ1NXY0CXMZEEEP413W4PK): builds each
/// binary and each test binary of the workspace, runs the doc tests,
/// and writes `tests.tar` and `manifest.json` in `dir`. The exit status
/// is 1 when a cargo command fails, and 2 when the archive cannot be
/// written.
fn test_build(dir: &Path) -> ExitCode {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let top = match std::env::current_dir() {
        Ok(top) => top,
        Err(e) => return tool_error(&format!("the current folder: {e}")),
    };
    let json = "--message-format=json-render-diagnostics";
    let mut messages = String::new();
    for args in [
        ["build", "--workspace", "--bins", json],
        ["test", "--workspace", "--no-run", json],
    ] {
        let out = match Command::new(&cargo)
            .args(args)
            .stderr(Stdio::inherit())
            .output()
        {
            Ok(out) => out,
            Err(e) => return tool_error(&format!("cargo {}: {e}", args[..3].join(" "))),
        };
        if !out.status.success() {
            eprintln!("error: cargo {}: {}", args[..3].join(" "), out.status);
            return ExitCode::FAILURE;
        }
        messages.push_str(&String::from_utf8_lossy(&out.stdout));
    }
    let manifest =
        hygiene::archive::Manifest::new(&top, hygiene::archive::artifacts(&messages, &top));
    if manifest.tests.is_empty() {
        return tool_error("cargo test --no-run built no test binary");
    }
    if let Some(file) = manifest
        .files()
        .into_iter()
        .find(|f| Path::new(f).is_absolute())
    {
        return tool_error(&format!(
            "{file} is outside {}. Build in a target folder of the checkout",
            top.display()
        ));
    }
    // rustdoc compiles each doc test when it runs: no part can run one
    // from the archive, so the build runs them.
    let doc = ["test", "--workspace", "--doc", "--no-fail-fast"];
    match Command::new(&cargo).args(doc).status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("error: cargo {}: {status}", doc[..3].join(" "));
            return ExitCode::FAILURE;
        }
        Err(e) => return tool_error(&format!("cargo {}: {e}", doc[..3].join(" "))),
    }
    if let Err(e) = write_archive(&top, dir, &manifest) {
        return tool_error(&e);
    }
    eprintln!(
        "hygiene test-build: {} test binaries and {} binaries in {}",
        manifest.tests.len(),
        manifest.bins.len(),
        dir.join("tests.tar").display()
    );
    ExitCode::SUCCESS
}

/// Writes `manifest.json` and `tests.tar` of `manifest` in `dir`. The
/// tar holds the files from the top `top` of the checkout.
fn write_archive(
    top: &Path,
    dir: &Path,
    manifest: &hygiene::archive::Manifest,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let path = dir.join("manifest.json");
    std::fs::write(&path, json + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    let tar = std::fs::canonicalize(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .join("tests.tar");
    let out = Command::new("tar")
        .arg("-cf")
        .arg(&tar)
        .arg("-C")
        .arg(top)
        .arg("--")
        .args(manifest.files())
        .output()
        .map_err(|e| format!("tar: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tar {}: {}", tar.display(), stderr.trim()));
    }
    Ok(())
}

/// Runs part `part` of the tests from the archive in `dir`, with no
/// build (01M6QQ1P5YCT0JX90X5FCKM4W1). It lists the tests of each test
/// binary, and runs the tests whose name falls in the part. The exit
/// status is 1 when a test binary fails, and 2 when the archive is
/// not there, is for another folder, or lists no test.
fn test_archive(part: &str, dir: &Path) -> ExitCode {
    let part: hygiene::part::Part = match part.parse() {
        Ok(part) => part,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let (manifest, top) = match open_archive(dir) {
        Ok(open) => open,
        Err(e) => return tool_error(&e),
    };
    let lists = match list_archive(&manifest, &top) {
        Ok(lists) => lists,
        Err(e) => return tool_error(&e),
    };
    let all: usize = lists.iter().map(|(_, names)| names.len()).sum();
    if all == 0 {
        return tool_error(&format!("the archive in {} lists no test", dir.display()));
    }
    let plans: Vec<(&hygiene::archive::Test, Vec<&str>)> = lists
        .iter()
        .map(|(test, names)| (*test, part.plan(names).tests))
        .collect();
    let run = plans.iter().map(|(_, names)| names.len()).sum();
    eprintln!("{}", hygiene::archive::summary(part, run, all));
    run_archive(&top, &plans)
}

/// Runs the tests whose name has `name` from the archive in `dir`, with
/// no build (01M75NDQY6BHZBV9PF735E3FGY): `just ci-test NAME`. It lists
/// the tests of each test binary, and runs the names that
/// [`hygiene::archive::named`] keeps. The exit status is 1 when a test
/// binary fails, and 2 when the archive is not there, is for another
/// folder, or has no test with `name` in its name.
fn test_named(name: &str, dir: &Path) -> ExitCode {
    if name.is_empty() {
        eprintln!("error: give the name of a test\n{USAGE}");
        return ExitCode::from(2);
    }
    let (manifest, top) = match open_archive(dir) {
        Ok(open) => open,
        Err(e) => return tool_error(&e),
    };
    let lists = match list_archive(&manifest, &top) {
        Ok(lists) => lists,
        Err(e) => return tool_error(&e),
    };
    let all: usize = lists.iter().map(|(_, names)| names.len()).sum();
    let plans: Vec<(&hygiene::archive::Test, Vec<&str>)> = lists
        .iter()
        .map(|(test, names)| (*test, hygiene::archive::named(names, name)))
        .collect();
    let run: usize = plans.iter().map(|(_, names)| names.len()).sum();
    if run == 0 {
        return tool_error(&hygiene::archive::none_named(name));
    }
    eprintln!("{}", hygiene::archive::named_summary(name, run, all));
    run_archive(&top, &plans)
}

/// The manifest of the archive in `dir`, and the top of the checkout:
/// the current folder. It fails when the archive is not there, is for
/// another folder, or is not unpacked.
fn open_archive(dir: &Path) -> Result<(hygiene::archive::Manifest, PathBuf), String> {
    let path = dir.join("manifest.json");
    let manifest: hygiene::archive::Manifest = std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let top = std::env::current_dir().map_err(|e| format!("the current folder: {e}"))?;
    manifest
        .check_root(&top)
        .and_then(|()| manifest.check_files(&top))?;
    Ok((manifest, top))
}

/// Each test binary of `manifest` in the checkout `top`, with the names
/// of the tests that it lists.
fn list_archive<'m>(
    manifest: &'m hygiene::archive::Manifest,
    top: &Path,
) -> Result<Vec<(&'m hygiene::archive::Test, Vec<String>)>, String> {
    let mut lists = Vec::new();
    for test in &manifest.tests {
        let crate_dir = top.join(&test.dir);
        let list = match Command::new(top.join(&test.exe))
            .args(["--list", "--format", "terse"])
            .current_dir(&crate_dir)
            .env("CARGO_MANIFEST_DIR", &crate_dir)
            .env("TMPDIR", TMPDIR)
            .stderr(Stdio::inherit())
            .output()
        {
            Ok(out) if out.status.success() => out,
            Ok(out) => return Err(format!("{} --list: {}", test.exe, out.status)),
            Err(e) => return Err(format!("{} --list: {e}", test.exe)),
        };
        let names = hygiene::part::names(&String::from_utf8_lossy(&list.stdout));
        lists.push((test, names));
    }
    Ok(lists)
}

/// Runs each test binary of `plans` with `--exact` and its names, in the
/// folder of its crate of the checkout `top`, as cargo runs it. A binary
/// with no name does not run. The exit status is 1 when a test binary
/// fails.
fn run_archive(top: &Path, plans: &[(&hygiene::archive::Test, Vec<&str>)]) -> ExitCode {
    let mut failed = false;
    for (test, names) in plans {
        // With no name, the binary would run each test.
        if names.is_empty() {
            continue;
        }
        let crate_dir = top.join(&test.dir);
        eprintln!("     Running {} ({})", test.name, test.exe);
        let status = Command::new(top.join(&test.exe))
            .arg("--exact")
            .args(names)
            .current_dir(&crate_dir)
            .env("CARGO_MANIFEST_DIR", &crate_dir)
            .env("TMPDIR", TMPDIR)
            .status();
        match status {
            Ok(status) => failed |= !status.success(),
            Err(e) => return tool_error(&format!("{}: {e}", test.exe)),
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The crates of the workspace, from `cargo metadata --no-deps`.
fn members() -> Result<Vec<hygiene::ci::Member>, String> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        workspace_root: PathBuf,
        packages: Vec<Package>,
    }
    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        manifest_path: PathBuf,
        dependencies: Vec<Dependency>,
    }
    #[derive(serde::Deserialize)]
    struct Dependency {
        name: String,
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let metadata: Metadata = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let members = metadata.packages.into_iter().map(|package| {
        let dir = package
            .manifest_path
            .parent()
            .and_then(|dir| dir.strip_prefix(&metadata.workspace_root).ok())
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default();
        hygiene::ci::Member {
            name: package.name,
            dir,
            deps: package.dependencies.into_iter().map(|d| d.name).collect(),
        }
    });
    Ok(members.collect())
}

/// The files of this repository that differ from the merge base of
/// `HEAD` and `base`: committed, not committed and not tracked. Each
/// path is from the top of the repository. A rename gives its two
/// paths. `None` when git cannot compare.
fn changed(base: &str) -> Option<Vec<String>> {
    let git = |at: &Path, args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(at).args(args).output();
        out.ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let top = git(Path::new("."), &["rev-parse", "--show-toplevel"])?;
    let top = PathBuf::from(top.trim_end_matches('\n'));
    let fork = git(&top, &["merge-base", "HEAD", base])?;
    let diff = git(
        &top,
        &["diff", "--name-only", "--no-renames", "-z", fork.trim()],
    )?;
    let new = git(&top, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut names: Vec<String> = diff
        .split('\0')
        .chain(new.split('\0'))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    Some(names)
}

/// Installs the theme in `dir/gruvbox` with `mdbook-gruvbox install`,
/// when `dir/book.toml` names the theme and the directory is missing.
/// The install can write `book.toml`, so this puts its bytes back
/// (01M3W5YW0172EVF2JA8T7WW392). mdbook reports a missing `book.toml`.
fn theme(dir: &str) -> Result<(), String> {
    let toml = Path::new(dir).join("book.toml");
    let Ok(saved) = std::fs::read(&toml) else {
        return Ok(());
    };
    if !hygiene::book::uses_theme(&String::from_utf8_lossy(&saved))
        || Path::new(dir).join("gruvbox").is_dir()
    {
        return Ok(());
    }
    let shown = format!("mdbook-gruvbox install {dir}");
    let out = Command::new("mdbook-gruvbox")
        .args(["install", dir])
        .output()
        .map_err(|e| format!("{shown}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{shown}: {}", stderr.trim()));
    }
    if std::fs::read(&toml).ok().as_ref() != Some(&saved) {
        std::fs::write(&toml, &saved).map_err(|e| format!("{}: {e}", toml.display()))?;
    }
    Ok(())
}

/// The tracked files of the repository of `dir` that differ from `HEAD`,
/// each with a mark of its content. Empty when `dir` is in no git
/// repository, or when the repository has no commit.
fn tracked(dir: &str) -> hygiene::book::Tracked {
    let git = |at: &Path, args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(at).args(args).output();
        out.ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let Some(top) = git(Path::new(dir), &["rev-parse", "--show-toplevel"]) else {
        return hygiene::book::Tracked::new();
    };
    let top = PathBuf::from(top.trim_end_matches('\n'));
    git(&top, &["diff", "HEAD", "--name-only", "-z"])
        .unwrap_or_default()
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(|name| (name.to_owned(), mark(&top.join(name))))
        .collect()
}

/// A mark of the content of the file `path`, or `gone` when it cannot be
/// read, for example a deleted file.
fn mark(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
        Err(_) => "gone".to_owned(),
    }
}

/// Runs `gh` and reads its JSON output.
fn gh<T: DeserializeOwned>(args: &[&str]) -> Result<T, String> {
    let out = gh_text(args)?;
    serde_json::from_str(&out).map_err(|e| format!("gh {}: {e}", args.join(" ")))
}

/// Runs `gh` and reads its output as text.
fn gh_text(args: &[&str]) -> Result<String, String> {
    String::from_utf8(gh_bytes(args)?).map_err(|e| format!("gh {}: {e}", args.join(" ")))
}

/// Runs `gh` and reads its output.
fn gh_bytes(args: &[&str]) -> Result<Vec<u8>, String> {
    let shown = format!("gh {}", args.join(" "));
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| format!("{shown}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{shown}: {}", stderr.trim()));
    }
    Ok(out.stdout)
}

/// A tool failed: exit status 2.
fn tool_error(text: &str) -> ExitCode {
    eprintln!("error: {text}");
    ExitCode::from(2)
}

/// The end of the report of a broken rule of issue hygiene.
const HYGIENE: &str =
    "rule(s) of issue hygiene. See \"Check a pull request on GitHub\" in the book.";

/// The end of the report of a broken rule of the book check.
const BOOK: &str = "rule(s) of the book check. See \"Check the book\" in the book.";

/// The end of the report of a broken rule of the wrap check.
const WRAP: &str = "rule(s) of the wrap check. See \"Check the wrap of the book\" in the book.";

/// The end of the report of a broken rule of the blank check.
const BLANK: &str = "rule(s) of the blank check. See \"Check the wrap of the book\" in the book.";

/// The end of the report of a broken rule of the versions check.
const VERSIONS: &str =
    "rule(s) of the versions check. See \"Check the version names\" in the book.";

/// Prints each error, and then how many rules `what` breaks and where to
/// read about them (`rules`). Exit status 1 when there is one.
fn report(what: &str, errors: &[hygiene::Error], rules: &str) -> ExitCode {
    if errors.is_empty() {
        println!("ok: {what}");
        return ExitCode::SUCCESS;
    }
    for e in errors {
        eprintln!("error: {e}");
    }
    eprintln!("{what} breaks {} {rules}", errors.len());
    ExitCode::FAILURE
}
