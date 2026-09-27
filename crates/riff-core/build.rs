//! Names the build: the last commit that changed the code, and its time
//! (see `build.rs` in the docs of `riff_core::build`). The image build
//! has no git, so `RIFF_COMMIT` and `RIFF_COMMIT_TIME` come first.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=RIFF_COMMIT");
    println!("cargo:rerun-if-env-changed=RIFF_COMMIT_TIME");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (commit, time) = match (
        std::env::var("RIFF_COMMIT"),
        std::env::var("RIFF_COMMIT_TIME"),
    ) {
        (Ok(commit), Ok(time)) if !commit.is_empty() && !time.is_empty() => (commit, time),
        _ => from_git(&root).unwrap_or_else(|| ("unknown".into(), "unknown".into())),
    };
    println!("cargo:rustc-env=RIFF_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RIFF_BUILD_TIME={time}");
}

/// The commit and its UTC time, with the command of
/// `deploy/build-id.sh`. It also asks cargo to build again after each
/// commit or checkout.
fn from_git(root: &Path) -> Option<(String, String)> {
    let git_dir = git(root, &["rev-parse", "--path-format=absolute", "--git-dir"])?;
    for file in ["HEAD", "logs/HEAD"] {
        let path = Path::new(&git_dir).join(file);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let line = git(
        root,
        &[
            "log",
            "-1",
            "--abbrev=12",
            "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
            "--format=%h %cd",
            "--",
            "crates",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    let (commit, time) = line.split_once(' ')?;
    Some((commit.to_owned(), time.to_owned()))
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("TZ", "UTC")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}
