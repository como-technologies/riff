//! In a real container, each folder of the home above a bind belongs to
//! the session user, and `~/.local/state` exists
//! (01M748WY71EFVZR6SG5CCEQYE0). A write over the size of a tmpfs of the
//! home fails with no space left (01M7DYR99QK609NF61GGCN75QQ). `all.rs`
//! builds this file on Linux only.
//!
//! Each test of a container uses the real podman and the session image
//! of this repository. It skips, and says why, on a host that has no
//! rootless podman or no copy of the image. The Gate has none of them.
//! The book says it in "The home of a session is in memory", in
//! `how-it-works.md`, and a test checks its words. To run the tests on a
//! host:
//!
//! ```sh
//! cargo test -p riff --test all home_folders:: -- --nocapture
//! ```
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as the test
//!     participant P as podman
//!     participant C as the container
//!     T->>P: Podman.start: binds at ~/.cache/sccache and in ~/.local/share
//!     P->>C: a tmpfs of 1 GiB of the session user at each folder above a bind
//!     C->>C: mkdir -p ~/.cache/x ~/.local/state/x
//!     C->>T: the owner of each folder
//!     C->>C: dd 1 GiB and more into the folder of the session
//!     C->>T: dd: No space left on device
//! ```

use std::path::Path;
use std::process::{Command, Stdio};

use riff::runtime::{Bind, Env, HOME_TMPFS_GIB, Runtime, SessionSpec, Source};
use tokio::io::AsyncReadExt;

/// The home in the container. No folder of the host is at that path.
const HOME: &str = "/home/riff-test";

/// The folders that the test checks: each folder above a bind, and the
/// folders of each session.
const FOLDERS: [&str; 7] = [
    ".cache",
    ".local",
    ".local/share",
    ".local/share/riff",
    ".local/share/riff/s",
    ".local/state",
    ".cache/x",
];

#[tokio::test]
async fn each_folder_above_a_bind_in_the_home_belongs_to_the_session_user() {
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }
    let clone = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let script = format!(
        "mkdir -p ~/.cache/x ~/.local/state/x && cd ~ && stat -c '%u %n' {}",
        FOLDERS.join(" ")
    );
    let spec = SessionSpec {
        name: format!("riff-home-test-{}", std::process::id()),
        binds: vec![
            bind(clone.path(), ".local/share/riff/s/clone"),
            bind(cache.path(), ".cache/sccache"),
        ],
        memory: "512m".into(),
        ..spec(script)
    };
    let (out, errors) = output(&spec).await;

    let uid = nix::unistd::getuid().as_raw();
    let want: Vec<String> = FOLDERS.iter().map(|f| format!("{uid} {f}")).collect();
    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, want, "{errors}");
}

/// #2113 (01M7DYR99QK609NF61GGCN75QQ): a write over the size of a tmpfs
/// of the home fails with no space left, and names its file. The kernel
/// kills nothing for lack of memory. The folder of the session, above
/// `clone/`, is such a tmpfs.
#[tokio::test]
async fn a_write_over_the_size_of_a_tmpfs_of_the_home_fails_with_no_space() {
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }
    let clone = tempfile::tempdir().unwrap();
    let big = format!("{HOME}/.local/share/riff/s/big");
    let mib = HOME_TMPFS_GIB * 1024 + 64;
    let script = format!("dd if=/dev/zero of={big} bs=1M count={mib} 2>&1; echo \"exit $?\"");
    let spec = SessionSpec {
        name: format!("riff-home-size-test-{}", std::process::id()),
        binds: vec![bind(clone.path(), ".local/share/riff/s/clone")],
        memory: "2g".into(),
        ..spec(script)
    };
    let (out, errors) = output(&spec).await;

    let full = format!("dd: error writing '{big}': No space left on device");
    assert!(out.contains(&full), "{out}{errors}");
    assert!(out.ends_with("exit 1\n"), "{out}{errors}");
}

/// A spec of the test that runs `script` in the session image.
fn spec(script: String) -> SessionSpec {
    SessionSpec {
        name: String::new(),
        image: image(),
        home: HOME.into(),
        workdir: "/".into(),
        binds: vec![],
        env: vec![Env::Value("HOME".into(), HOME.into())],
        memory: "512m".into(),
        cpus: 1,
        command: vec!["sh".into(), "-c".into(), script],
    }
}

/// A bind with write of `source` at `target` in the home.
fn bind(source: &Path, target: &str) -> Bind {
    Bind {
        source: Source::Path(source.into()),
        target: format!("{HOME}/{target}").into(),
        write: true,
    }
}

/// Runs `spec` with the real podman: its stdout and its stderr. The
/// command must end with 0.
async fn output(spec: &SessionSpec) -> (String, String) {
    let _clean = Clean(spec.name.clone());
    let mut running = riff::podman::Podman.start(spec, &[]).await.unwrap();
    drop(running.input);
    let (mut out, mut errors) = (String::new(), String::new());
    let (read, read_errors) = tokio::join!(
        running.events.read_to_string(&mut out),
        running.errors.read_to_string(&mut errors)
    );
    read.unwrap();
    read_errors.unwrap();
    let exit = running.end.wait().await.unwrap();
    assert!(exit.success(), "{exit:?}: {out}{errors}");
    (out, errors)
}

/// The session image of this repository.
fn image() -> String {
    riff::session_image::Image::of_repository(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .unwrap()
        .to_string()
}

/// Why this host cannot run the test, or `Ok`.
fn ready() -> Result<(), String> {
    if podman(&["info", "--format", "{{.Host.Security.Rootless}}"]).as_deref() != Ok("true") {
        return Err("podman does not run rootless here".into());
    }
    let image = image();
    if podman(&["image", "exists", &image]).is_err() {
        return Err(format!("podman has no copy of {image}: run riff init"));
    }
    Ok(())
}

/// The stdout of `podman ARGS` with no end of line, or its stderr.
fn podman(args: &[&str]) -> Result<String, String> {
    let out = Command::new("podman")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// At its drop, removes the container `name`.
struct Clean(String);

impl Drop for Clean {
    fn drop(&mut self) {
        let _ = podman(&["rm", "--force", "--time", "0", &self.0]);
    }
}

/// #2113: the book says that the home of a session is in memory, the
/// size of each of its tmpfs, and where a large build tree goes. Its
/// how-to makes the second worktree in `/tmp`, as the start text of a
/// session says ([`riff::text::build_trees`]).
#[test]
fn the_book_says_where_a_large_build_tree_goes() {
    let part = crate::book::part("how-it-works.md", "Sessions in containers");
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    let size = format!("Each of these tmpfs holds {HOME_TMPFS_GIB} GiB at most.");
    for words in [
        "### The home of a session is in memory",
        size.as_str(),
        "\"No space left on device\", and the error names the file",
        "| `/tmp` | A second worktree, or another large build tree. |",
        "### Make a second worktree in a session",
        &format!("show `tmpfs` with a size of {HOME_TMPFS_GIB}.0G"),
    ] {
        assert!(flat.contains(words), "{words}");
    }
    let commands = crate::book::commands_in(&part);
    let worktree = "git worktree add /tmp/second origin/main";
    assert!(commands.iter().any(|c| c == worktree), "{commands:?}");
    assert!(
        commands.iter().any(|c| c == "df -h ~ .. . /tmp"),
        "{commands:?}"
    );
    assert!(riff::text::build_trees().contains(" in /tmp, on the disk."));
}
