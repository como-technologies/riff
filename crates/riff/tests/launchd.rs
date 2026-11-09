//! `riff daemon install`, `riff daemon status` and `riff daemon
//! uninstall` on macOS, under launchd (01M6Z4A4X2V5H4EE3A9RJ5RA6W,
//! 01M6Z4A4ZHF2ZBBZ1RC1ZHVSEH, 01M6Z4A521PMRBK1R1E7NNADCQ). The daemon
//! runs on a Mac, and starts no session (01M6Z49R87D50VD4RS0BP641MM).
//!
//! A fake `launchctl` logs each call. Its `bootstrap` starts the `riff
//! daemon` of this build in the background, as launchd does with
//! `RunAtLoad`, and its `bootout` stops it with a SIGTERM. The tests run
//! on macOS only: on Linux, the service manager is systemd (`daemon.rs`).

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use isolated::Isolated;

/// The label of the agent of the daemon.
const LABEL: &str = "io.github.como-technologies.riff";

/// The fake `launchctl` in its folder. The agent is loaded while the
/// file `loaded` is there.
const LAUNCHCTL: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "launchctl $*" >> "$dir/launchctl.log"
runs() { [ -f "$dir/daemon.pid" ] && kill -0 "$(cat "$dir/daemon.pid")" 2>/dev/null; }
case "$1" in
  print)
    [ -f "$dir/loaded" ] || { echo "Could not find service \"$2\"" >&2; exit 113; }
    if runs; then
      printf '%s = {\n\tstate = running\n\tpid = %s\n}\n' "$2" "$(cat "$dir/daemon.pid")"
    else
      printf '%s = {\n\tstate = not running\n\tlast exit code = 0\n}\n' "$2"
    fi ;;
  bootout)
    [ -f "$dir/loaded" ] || { echo "Boot-out failed: 3: No such process" >&2; exit 3; }
    runs && kill "$(cat "$dir/daemon.pid")"
    rm -f "$dir/loaded" "$dir/daemon.pid" ;;
  bootstrap)
    touch "$dir/loaded"
    "$FAKE_RIFF" daemon >> "$dir/daemon.out" 2>&1 &
    echo $! > "$dir/daemon.pid" ;;
esac
exit 0
"#;

/// An environment with the fake `launchctl` first in `PATH`.
struct Here {
    env: Isolated,
    bin: tempfile::TempDir,
    path: String,
}

impl Here {
    fn new() -> Here {
        let env = Isolated::new();
        let bin = tempfile::tempdir().unwrap();
        isolated::write_program(bin.path().join("launchctl"), LAUNCHCTL);
        let path = format!(
            "{}:{}",
            bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        Here { env, bin, path }
    }

    /// `riff ARGS` of the machine: with no `RIFF_HOME`, in the isolated
    /// home.
    async fn run(&self, args: &[&str]) -> Output {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .env("PATH", &self.path)
            .env("FAKE_RIFF", self.env.riff_path())
            .env_remove("RIFF_HOME");
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap_or_default()
    }

    /// The calls of `launchctl` that change the agent: each call but
    /// `print`, which riff makes to read its state.
    fn changes(&self) -> String {
        self.read("launchctl.log")
            .lines()
            .filter(|l| !l.starts_with("launchctl print "))
            .map(|l| format!("{l}\n"))
            .collect()
    }

    fn plist(&self) -> PathBuf {
        self.env
            .home()
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist"))
    }

    /// The folder of the daemon of the machine.
    fn daemon_dir(&self) -> PathBuf {
        self.env.home().join(".local/share/riff/daemon")
    }
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Waits until the text of `file` holds `part`.
async fn wait_for(file: &Path, part: &str) {
    let span = isolated::Span::start();
    while !std::fs::read_to_string(file)
        .unwrap_or_default()
        .contains(part)
    {
        assert!(
            span.within(isolated::HANG),
            "{} has no {part:?}, it holds:\n{}",
            file.display(),
            std::fs::read_to_string(file).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "launchd is the service manager of macOS only"
)]
async fn riff_daemon_install_on_a_mac_writes_the_plist_and_the_daemon_starts_no_session() {
    let here = Here::new();
    let uid = nix::unistd::getuid().as_raw();
    let out = here.run(&["daemon", "install"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    let plist = here.plist();
    assert!(
        stdout.contains(&format!(
            "riff wrote the unit {}, and started it again.",
            plist.display()
        )),
        "{stdout}"
    );
    assert!(stdout.contains("The riff daemon runs: pid "), "{stdout}");
    let text_of_plist = std::fs::read_to_string(&plist).unwrap();
    let exe = std::fs::canonicalize(here.env.riff_path()).unwrap();
    for line in [
        format!("\t\t<string>{}</string>", exe.display()),
        "\t\t<string>daemon</string>".to_owned(),
        format!("\t\t<string>{}</string>", here.path),
        format!(
            "\t<string>{}</string>",
            here.env
                .home()
                .join("Library/Logs/riff/daemon.log")
                .display()
        ),
    ] {
        assert!(
            text_of_plist.lines().any(|l| l == line),
            "{line}\nnot in\n{text_of_plist}"
        );
    }
    assert!(here.env.home().join("Library/Logs/riff").is_dir());
    assert_eq!(
        here.changes(),
        format!(
            "launchctl enable gui/{uid}/{LABEL}\n\
             launchctl bootstrap gui/{uid} {}\n",
            plist.display()
        )
    );

    // The daemon answers in the data folder of riff of the machine.
    let out = here.run(&["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    assert!(status.contains("Sessions: none."), "{status}");

    // A session needs its image first: the daemon on a Mac says that
    // the sessions come later.
    let dir = here.daemon_dir();
    let ask = riff::daemon::image(&dir, "ghcr.io/o/i@sha256:0", |_| {});
    let e = ask.await.unwrap_err().to_string();
    assert_eq!(e, riff::text::SESSIONS_ON_A_MAC);

    let out = here.run(&["daemon", "uninstall"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(&format!(
            "riff stopped the daemon, and deleted the unit {}.",
            plist.display()
        )),
        "{stdout}"
    );
    assert!(!plist.exists());
    wait_for(&here.bin.path().join("daemon.out"), "The riff daemon stops").await;
    assert!(here.daemon_dir().is_dir());

    // With no daemon, the status shows the agent and its log.
    let out = here.run(&["daemon", "status"]).await;
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(&format!(
            "The unit {LABEL} is inactive. Start it: riff daemon install. See its log: tail -f {}",
            here.env
                .home()
                .join("Library/Logs/riff/daemon.log")
                .display()
        )),
        "{stdout}"
    );
}
