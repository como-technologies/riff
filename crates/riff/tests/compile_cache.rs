//! The compile cache in a volume of rootless podman, with a folder of
//! a user of the container: the person cannot read it, and riff still
//! shows its size and empties it (#1115, 01M588JW2KS6EVX1SE49686FQ7).
//! The test needs rootless podman; with no podman it skips.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The output of `podman ARGS`, or `None` when it fails.
fn podman(args: &[&str]) -> Option<String> {
    let out = Command::new("podman").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A volume of podman that the drop removes.
struct Volume(String);

impl Volume {
    fn folder(&self) -> PathBuf {
        let out = podman(&["volume", "inspect", "--format", "{{.Mountpoint}}", &self.0]).unwrap();
        riff::compile_cache::folder_of(&out).unwrap()
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        podman(&["volume", "rm", "--force", &self.0]);
    }
}

/// Fills `dir` as a user of the container, uid 1000 in the user
/// namespace: the person cannot read the folder `d`, as with
/// `riff-sccache` on a rootless host.
fn fill_as_container_user(dir: &Path) {
    let dir = dir.display();
    let script = format!(
        "mkdir {dir}/d && head -c 1000000 /dev/zero > {dir}/d/f && \
         chown -R 1000:1000 {dir}/d && chmod 700 {dir}/d"
    );
    podman(&["unshare", "sh", "-c", &script]).expect("podman unshare fills the volume");
}

#[test]
fn riff_measures_and_empties_a_volume_that_the_person_cannot_read() {
    if podman(&["info", "--format", "{{.Host.Security.Rootless}}"]).as_deref() != Some("true\n") {
        eprintln!("skip: no rootless podman");
        return;
    }
    let volume = Volume(format!("riff-test-cache-{}", std::process::id()));
    podman(&["volume", "create", &volume.0]).unwrap();
    let dir = volume.folder();
    fill_as_container_user(&dir);
    assert!(std::fs::read_dir(dir.join("d")).is_err());

    assert!(riff::compile_cache::used(&dir).is_err());
    let bytes = riff::compile_cache::measure(&dir).unwrap();
    assert!(bytes >= 1_000_000, "{bytes}");

    riff::compile_cache::clear(&dir).unwrap();
    assert!(dir.is_dir());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    assert!(riff::compile_cache::measure(&dir).unwrap() < 1_000_000);
}
