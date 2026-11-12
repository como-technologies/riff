//! In a real container, each process of a session runs at nice 19, and
//! the cgroup of the container has the lowest CPU weight (#1990,
//! 01M7755V17SGY2NW231WJ12WXN). `all.rs` builds this file on Linux only.
//!
//! The test uses the real podman and the session image of this
//! repository. It skips, and says why, on a host that has no rootless
//! podman or no copy of the image. The Gate has none of them. To run it
//! on a host:
//!
//! ```sh
//! cargo test -p riff --test all priority:: -- --nocapture
//! ```
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as the test
//!     participant P as podman
//!     participant C as the container
//!     T->>P: Podman.start: nice -n 19 podman run, a worker
//!     C->>T: nice, and /sys/fs/cgroup/cpu.weight
//!     T->>P: Podman.start_terminal: nice -n 19 podman run -d, the lead
//!     T->>P: podman top NAME nice args
//!     P->>T: the nice value of each process, PID 1 too
//!     T->>P: podman exec NAME cat /sys/fs/cgroup/cpu.weight
//! ```

use std::path::Path;
use std::process::{Command, Stdio};

use riff::runtime::{Env, Runtime, SessionSpec};
use tokio::io::AsyncReadExt;

/// The home in the container. No folder of the host is at that path.
const HOME: &str = "/home/riff-test";

/// The nice value of each process of a session.
const NICE: &str = "19";

/// The CPU weight of the cgroup of a session container.
const WEIGHT: &str = "1";

#[tokio::test]
async fn a_process_in_a_new_worker_container_has_nice_19_and_its_cgroup_the_lowest_cpu_weight() {
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }
    let spec = spec(
        &format!("riff-priority-test-{}", std::process::id()),
        &["sh", "-c", "nice && cat /sys/fs/cgroup/cpu.weight"],
    );
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

    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, [NICE, WEIGHT], "{errors}");
}

#[tokio::test]
async fn each_process_of_a_new_lead_container_has_nice_19_and_its_cgroup_the_lowest_cpu_weight() {
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }
    let spec = spec(
        &format!("riff-priority-lead-test-{}", std::process::id()),
        &["sleep", "600"],
    );
    let name = spec.name.clone();
    let _clean = Clean(name.clone());
    let _end = riff::podman::Podman
        .start_terminal(&spec, &[])
        .await
        .unwrap();

    // The first line is the header. PID 1, the init of podman, and the
    // command of the session.
    let top = podman(&["top", &name, "nice", "args"]).unwrap();
    let rows: Vec<&str> = top.lines().skip(1).collect();
    assert!(rows.len() >= 2, "{top}");
    for row in &rows {
        assert_eq!(row.split_whitespace().next(), Some(NICE), "{top}");
    }
    assert_eq!(
        podman(&["exec", &name, "cat", "/sys/fs/cgroup/cpu.weight"]).as_deref(),
        Ok(WEIGHT)
    );
}

/// A spec of a session with the caps of a test, and `command`.
fn spec(name: &str, command: &[&str]) -> SessionSpec {
    SessionSpec {
        name: name.into(),
        image: image(),
        home: HOME.into(),
        workdir: "/".into(),
        binds: vec![],
        env: vec![Env::Value("HOME".into(), HOME.into())],
        memory: "512m".into(),
        cpus: 1,
        command: command.iter().map(|word| (*word).to_owned()).collect(),
    }
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
