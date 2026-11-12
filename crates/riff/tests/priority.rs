//! In a real container, each process of a session runs at nice 19, the
//! cgroup of the container has the lowest CPU weight, and the container
//! runs in the slice of riff, with the lowest CPU weight of its level
//! (#1990, #2001, 01M7755V17SGY2NW231WJ12WXN, 01M778VSP6Z29FCPGX5S76ME1T).
//! `all.rs` builds this file on Linux only.
//!
//! The test uses the real podman, the real manager of systemd for the
//! user and the session image of this repository. It skips, and says
//! why, on a host that has no rootless podman, no manager of systemd for
//! the user or no copy of the image. The Gate has none of them. To run
//! it on a host:
//!
//! ```sh
//! cargo test -p riff --test all priority:: -- --nocapture
//! ```
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as the test
//!     participant S as systemd of the user
//!     participant P as podman
//!     participant C as the container
//!     T->>S: Podman.start: systemctl --user set-property riff.slice CPUWeight=1
//!     T->>P: Podman.start: nice -n 19 podman run --cgroup-parent riff.slice, a worker
//!     C->>T: nice, and /sys/fs/cgroup/cpu.weight
//!     T->>P: Podman.start_terminal: nice -n 19 podman run -d, the lead
//!     T->>P: podman top NAME nice args
//!     P->>T: the nice value of each process, PID 1 too
//!     T->>P: podman exec NAME cat /sys/fs/cgroup/cpu.weight
//!     T->>T: /proc/PID/cgroup: riff.slice in user@UID.service
//!     T->>T: cpu.weight of riff.slice and of each cgroup beside it
//! ```

use std::os::unix::fs::MetadataExt;
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

/// The container and its conmon run in `riff.slice` of the manager of
/// the user, next to `app.slice`. No cgroup beside the slice has a lower
/// CPU weight. With `--nocapture`, the test shows each of them.
#[tokio::test]
async fn the_slice_of_riff_has_the_lowest_cpu_weight_of_its_level() {
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }
    let spec = spec(
        &format!("riff-priority-slice-test-{}", std::process::id()),
        &["sleep", "600"],
    );
    let name = spec.name.clone();
    let _clean = Clean(name.clone());
    let _end = riff::podman::Podman
        .start_terminal(&spec, &[])
        .await
        .unwrap();

    // The cgroup of PID 1 of the container, as the host sees it:
    // /user.slice/user-UID.slice/user@UID.service/riff.slice/libpod-ID.scope/...
    let pid = podman(&["container", "inspect", "--format", "{{.State.Pid}}", &name]).unwrap();
    let cgroups = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
    let path = cgroups
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .unwrap();
    let parts: Vec<&str> = path.split('/').collect();
    let at = parts
        .iter()
        .position(|part| *part == riff::podman::SLICE)
        .unwrap_or_else(|| panic!("the container is not in riff.slice: {path}"));
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    assert_eq!(parts[at - 1], format!("user@{uid}.service"), "{path}");
    let scope = parts[at + 1];
    assert!(scope.starts_with("libpod-"), "{path}");

    let slice = Path::new("/sys/fs/cgroup").join(parts[1..=at].join("/"));
    let id = scope.trim_start_matches("libpod-");
    assert!(
        slice.join(format!("libpod-conmon-{id}")).is_dir(),
        "no conmon of the container in {}",
        slice.display()
    );
    let ours = weight(&slice).unwrap();
    assert_eq!(ours.to_string(), WEIGHT);

    let level = slice.parent().unwrap();
    eprintln!("the CPU weight of each cgroup in {}:", level.display());
    let mut lower = Vec::new();
    for entry in std::fs::read_dir(level).unwrap() {
        let dir = entry.unwrap().path();
        let Some(other) = weight(&dir) else { continue };
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        eprintln!("{other:>6}  {name}");
        if other < ours {
            lower.push(name);
        }
    }
    assert!(
        lower.is_empty(),
        "a lower CPU weight than riff.slice: {lower:?}"
    );
}

/// The CPU weight of the cgroup `dir`, or `None` when it has none.
fn weight(dir: &Path) -> Option<u64> {
    std::fs::read_to_string(dir.join("cpu.weight"))
        .ok()?
        .trim()
        .parse()
        .ok()
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
    let manager = Command::new("systemctl")
        .args(["--user", "show", "--property=Version"])
        .stdin(Stdio::null())
        .output();
    if !manager.is_ok_and(|out| out.status.success()) {
        return Err("systemctl --user does not reach the manager of the user".into());
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
