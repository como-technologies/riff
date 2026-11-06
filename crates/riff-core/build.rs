//! Names the build: the hash of the code, and the time of the last
//! commit that changed it (see `build.rs` in the docs of
//! `riff_core::build`). The image build has no git, so `RIFF_BUILD` and
//! `RIFF_BUILD_TIME` come first.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// The paths that hold the code.
const CODE: [&str; 3] = ["crates", "Cargo.toml", "Cargo.lock"];

fn main() {
    println!("cargo:rerun-if-env-changed=RIFF_BUILD");
    println!("cargo:rerun-if-env-changed=RIFF_BUILD_TIME");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (id, time) = match (
        std::env::var("RIFF_BUILD"),
        std::env::var("RIFF_BUILD_TIME"),
    ) {
        (Ok(id), Ok(time)) if !id.is_empty() && !time.is_empty() => (id, time),
        _ => from_git(&root).unwrap_or_else(|| ("unknown".into(), "unknown".into())),
    };
    println!("cargo:rustc-env=RIFF_BUILD_ID={id}");
    println!("cargo:rustc-env=RIFF_BUILD_TIME={time}");
}

/// The hash of the code and the UTC time of its last commit, with the
/// commands of `deploy/build-id.sh`. It also asks cargo to build again
/// after each commit or checkout.
fn from_git(root: &Path) -> Option<(String, String)> {
    let git_dir = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
        None,
    )?;
    for file in ["HEAD", "logs/HEAD"] {
        let path = Path::new(&git_dir).join(file);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let mut ls_tree = vec!["ls-tree", "HEAD", "--"];
    ls_tree.extend(CODE);
    let trees = git(root, &ls_tree, None)?;
    // `git` trims the output: the script hashes the lines with their end.
    let hash = git(
        root,
        &["hash-object", "--stdin"],
        Some(&format!("{trees}\n")),
    )?;
    let mut log = vec![
        "log",
        "-1",
        "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
        "--format=%cd",
        "--",
    ];
    log.extend(CODE);
    let time = git(root, &log, None)?;
    Some((hash.chars().take(12).collect(), time))
}

fn git(dir: &Path, args: &[&str], input: Option<&str>) -> Option<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("TZ", "UTC")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(input) = input {
        child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    }
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}
