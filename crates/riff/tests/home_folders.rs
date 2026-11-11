//! In a real container, each folder of the home above a bind belongs to
//! the session user, and `~/.local/state` exists
//! (01M748WY71EFVZR6SG5CCEQYE0). `all.rs` builds this file on Linux
//! only.
//!
//! The test uses the real podman and the session image of this
//! repository. It skips, and says why, on a host that has no rootless
//! podman or no copy of the image. The Gate has none of them. To run it
//! on a host:
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
//!     P->>C: a tmpfs of the session user at each folder above a bind
//!     C->>C: mkdir -p ~/.cache/x ~/.local/state/x
//!     C->>T: the owner of each folder
//! ```

use std::path::Path;
use std::process::{Command, Stdio};

use riff::runtime::{Bind, Env, Runtime, SessionSpec, Source};
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
    let bind = |source: &Path, target: &str| Bind {
        source: Source::Path(source.into()),
        target: format!("{HOME}/{target}").into(),
        write: true,
    };
    let script = format!(
        "mkdir -p ~/.cache/x ~/.local/state/x && cd ~ && stat -c '%u %n' {}",
        FOLDERS.join(" ")
    );
    let spec = SessionSpec {
        name: format!("riff-home-test-{}", std::process::id()),
        image: image(),
        home: HOME.into(),
        workdir: "/".into(),
        binds: vec![
            bind(clone.path(), ".local/share/riff/s/clone"),
            bind(cache.path(), ".cache/sccache"),
        ],
        env: vec![Env::Value("HOME".into(), HOME.into())],
        memory: "512m".into(),
        cpus: 1,
        command: vec!["sh".into(), "-c".into(), script],
    };
    let _clean = Clean(spec.name.clone());
    let mut running = riff::podman::Podman.start(&spec, &[]).await.unwrap();
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

    let uid = nix::unistd::getuid().as_raw();
    let want: Vec<String> = FOLDERS.iter().map(|f| format!("{uid} {f}")).collect();
    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, want, "{errors}");
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
