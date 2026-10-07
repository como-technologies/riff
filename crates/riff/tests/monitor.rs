//! The monitor of a machine that runs workers (01M421QPKWPX00X24F8V6DT8Z3
//! to 01M421QQ1K7EFDV2PVPTSTE5FK). Each test runs the real `riff`
//! binary: `riff workers host` with a look each second, a fake `tmux`
//! with no worker, fake numbers of the machine in `RIFF_PROC`, and a
//! fake `journalctl`.

use crate::book;

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff::monitor::Saved;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// A tmux with no worker pane.
const FAKE_TMUX: &str = r#"#!/bin/sh
case "$1" in
  display-message) echo "@0" ;;
esac
exit 0
"#;

/// `journalctl` prints the file `journal` next to it.
const FAKE_JOURNALCTL: &str = r#"#!/bin/sh
dir=$(dirname "$0")
[ -f "$dir/journal" ] && cat "$dir/journal"
exit 0
"#;

/// The bound of each wait. It only ends a test that hangs.
const WAIT: Duration = Duration::from_secs(60);

/// A machine with 8 cores: the load limit is 1.5 times 8, 12.
const MACHINE: &str = "cpu 8x3000MHz, mem 64GB, 60GB available, load 0.50";

/// The `meminfo` of a machine with `gb` GB available.
fn meminfo(gb: u64) -> String {
    format!(
        "MemTotal:       67108864 kB\nMemAvailable:   {} kB\n",
        gb * 1024 * 1024
    )
}

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A riff that runs, the lead `l1` on the host `a`, and a clone on the
/// machine `pangolin`.
struct Riff {
    api: Api,
    lead: SessionUri,
    fake: tempfile::TempDir,
    proc: tempfile::TempDir,
    home: tempfile::TempDir,
    root: tempfile::TempDir,
    /// Each text that the lead read.
    read: std::sync::Mutex<String>,
}

impl Riff {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let api = Api::new(&format!("http://{addr}"));
        let fake = tempfile::tempdir().unwrap();
        script(fake.path(), "tmux", FAKE_TMUX);
        script(fake.path(), "journalctl", FAKE_JOURNALCTL);
        // A host installs no real sccache (01M4923963S666V9YWTZ46ZZ50).
        script(fake.path(), "sccache", "#!/bin/sh\necho sccache 0.18.0\n");
        let root = tempfile::tempdir().unwrap();
        let main = root.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "first"]);
        let main = std::fs::canonicalize(main).unwrap();
        let place = identity::place_in(&main, "a").unwrap();
        let lead = SessionUri::new(Who::new("mike", Some("l1")).unwrap(), place);
        api.register(&lead).await.unwrap();
        api.set_riff(&lead, RiffState::Running).await.unwrap();
        let r = Riff {
            api,
            lead,
            fake,
            proc: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            root,
            read: std::sync::Mutex::default(),
        };
        r.numbers(1.0, 60);
        for args in [
            &["workers", "limit", "2"][..],
            &["workers", "monitor", "on", "--every", "1"],
        ] {
            let out = r.riff(args).output().unwrap();
            assert!(out.status.success(), "{out:?}");
        }
        r
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    /// The 5-minute load and the available memory of the machine.
    fn numbers(&self, load5: f64, avail_gb: u64) {
        let loadavg = format!("{load5:.2} {load5:.2} {load5:.2} 1/100 4242\n");
        std::fs::write(self.proc.path().join("loadavg"), loadavg).unwrap();
        std::fs::write(self.proc.path().join("meminfo"), meminfo(avail_gb)).unwrap();
    }

    /// `riff ARGS` in the main clone, as the person on `pangolin` in a
    /// tmux pane.
    fn riff(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.main())
            .env("PATH", path)
            .env("RIFF_HOME", self.home.path())
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_MACHINE", MACHINE)
            .env("RIFF_PROC", self.proc.path())
            .env("RIFF_ON", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_SESSION")
            .env_remove("RIFF_WORKER");
        cmd
    }

    /// Starts `riff workers host`, and waits until it is in `riff who`.
    async fn host(&self) -> Host {
        let out = self.fake.path().join("host.out");
        let child = self
            .riff(&["workers", "host", "--claude", "true"])
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&out).unwrap())
            .stderr(std::fs::File::create(self.fake.path().join("host.err")).unwrap())
            .spawn()
            .unwrap();
        let host = Host(child, out);
        self.until("the host in riff who", || async {
            self.host_session().await
        })
        .await;
        host
    }

    /// The session of the live workers host.
    async fn host_session(&self) -> Option<SessionUri> {
        let who = self.api.who(&self.lead, false).await.ok()?;
        who.into_iter()
            .find(|s| {
                s.live
                    && s.uri.place().host() == "pangolin"
                    && s.status
                        .as_ref()
                        .is_some_and(|st| st.status.step.starts_with("workers host"))
            })
            .map(|s| s.uri)
    }

    /// The last look of the monitor.
    fn saved(&self) -> Option<Saved> {
        Saved::read(&self.home.path().join("state"))
    }

    /// Waits until the monitor looked `n` more times: its last look is
    /// `n` seconds newer.
    async fn looks(&self, n: u64) {
        let first = self
            .until("a look of the monitor", || async { self.saved() })
            .await
            .at;
        self.until("the looks of the monitor", || async {
            self.saved().filter(|s| s.at >= first + n)
        })
        .await;
    }

    /// Waits until `check` gives `Some`.
    async fn until<T, F, Fut>(&self, what: &str, mut check: F) -> T
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        let start = Instant::now();
        loop {
            if let Some(value) = check().await {
                return value;
            }
            assert!(start.elapsed() < WAIT, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Each text that the lead read up to now.
    async fn lead_read(&self) -> String {
        let inbox = self.api.inbox(&self.lead, None, false).await.unwrap();
        let mut read = self.read.lock().unwrap();
        read.push_str(&riff::text::inbox(&inbox, &self.lead));
        read.clone()
    }

    /// Waits until the lead read `needle`. Returns each text that it read.
    async fn lead_reads(&self, needle: &str) -> String {
        self.until(needle, || async {
            let read = self.lead_read().await;
            read.contains(needle).then_some(read)
        })
        .await
    }

    /// The count of the messages of the monitor that the lead read.
    async fn monitor_messages(&self) -> usize {
        self.lead_read().await.matches("monitor: pangolin:").count()
    }
}

/// A `riff workers host` that is killed on drop, and the file of its
/// output.
struct Host(Child, PathBuf);

impl Host {
    fn output(&self) -> String {
        std::fs::read_to_string(&self.1).unwrap_or_default()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 01M421QPP5QFBB0YN25HY2MG1Z: no message with good numbers, one
/// message at the limit, none while it stays, and one when it is good
/// again. The same for the memory, and one message for a kill.
#[tokio::test(flavor = "multi_thread")]
async fn the_monitor_tells_the_lead_once_at_a_limit_and_once_when_good() {
    let r = Riff::new().await;
    let host = r.host().await;

    // Good numbers: no message.
    r.looks(3).await;
    assert_eq!(r.monitor_messages().await, 0, "{}", host.output());

    // The 5-minute load goes over 1.5 times 8 cores: one message.
    r.numbers(13.0, 60);
    let read = r
        .lead_reads(
            "monitor: pangolin: the 5-minute load is 13.00, over the limit 12.00 \
             (1.5 times 8 physical cores).",
        )
        .await;
    assert!(read.contains("riff changes nothing"), "{read}");
    r.looks(3).await;
    assert_eq!(r.monitor_messages().await, 1, "{}", r.lead_read().await);

    // Good again: one message.
    r.numbers(5.0, 60);
    r.lead_reads("the 5-minute load is good again: 5.00, under the limit 12.00.")
        .await;
    r.looks(3).await;
    assert_eq!(r.monitor_messages().await, 2, "{}", r.lead_read().await);

    // The memory under the floor of 4 GB, and good again.
    r.numbers(5.0, 2);
    r.lead_reads("monitor: pangolin: 2 GB of memory is available, under the floor 4 GB.")
        .await;
    r.numbers(5.0, 30);
    r.lead_reads("the available memory is good again: 30 GB, over the floor 4 GB.")
        .await;
    r.looks(2).await;
    assert_eq!(r.monitor_messages().await, 4, "{}", r.lead_read().await);

    // A kill of systemd-oomd: one message, and the host tells it.
    let at = riff::monitor::now_secs() + 1;
    std::fs::write(
        r.fake.path().join("journal"),
        format!(
            "{at}.5 pangolin systemd-oomd[812]: Killed /user.slice/app.slice/tmux-spawn-f089.scope \
             due to memory pressure\n"
        ),
    )
    .unwrap();
    r.lead_reads(&format!(
        "monitor: pangolin: systemd-oomd killed tmux-spawn-f089.scope at {}.",
        riff::text::clock(at)
    ))
    .await;
    r.looks(3).await;
    assert_eq!(r.monitor_messages().await, 5, "{}", r.lead_read().await);
    r.until("the kill in the status of the host", || async {
        let who = r.api.who(&r.lead, false).await.ok()?;
        who.iter()
            .find_map(|s| riff::host::HostStatus::parse(&s.status.as_ref()?.status.step))
            .and_then(|status| status.monitor)
            .and_then(|n| n.kill)
            .filter(|k| k.at == at && k.by == "systemd-oomd")
    })
    .await;

    // riff workers shows the monitor and its last numbers
    // (01M421QPX01BB15GJXHFYRETTX).
    let out = r.riff(&["workers"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = stdout(&out);
    assert!(
        text.contains("monitor on  load5 5.00 of 12.00 (8 cores)  jobs 3"),
        "{text}"
    );
    assert!(
        text.contains(&format!("last kill {} systemd-oomd", riff::text::clock(at))),
        "{text}"
    );
    drop(host);
}

/// 01M421QPRF45DQDA8S4PT1Q12V: the monitor only reads and tells. Over
/// each limit, it changes no setting of the machine.
#[tokio::test(flavor = "multi_thread")]
async fn the_monitor_changes_no_setting() {
    let r = Riff::new().await;
    let config = r.home.path().join("config.toml");
    let before = std::fs::read_to_string(&config).unwrap();
    let host = r.host().await;
    r.numbers(40.0, 1);
    r.lead_reads("over the limit 12.00").await;
    r.lead_reads("under the floor 4 GB").await;
    r.looks(2).await;
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
    drop(host);
}

/// 01M421QQ1K7EFDV2PVPTSTE5FK: a second monitor on the machine does not
/// look while the first one holds the lock.
#[tokio::test(flavor = "multi_thread")]
async fn one_monitor_runs_on_a_machine() {
    let r = Riff::new().await;
    let state = r.home.path().join("state");
    let held = riff::local::monitor_lock(&state).unwrap().unwrap();
    let host = r.host().await;
    r.numbers(13.0, 60);
    // The monitor of the host does not hold the lock: no look, no
    // message.
    r.until("the line of the other monitor", || async {
        host.output()
            .contains(riff::text::MONITOR_RUNS)
            .then_some(())
    })
    .await;
    assert!(r.saved().is_none(), "the monitor looked: {:?}", r.saved());
    assert_eq!(r.monitor_messages().await, 0);
    // The lock is free: the monitor of the host takes it, and looks.
    drop(held);
    r.lead_reads("over the limit 12.00").await;
    drop(host);
}

/// 01M421QPTQ8BQ0KMG8F7CRHNMX: the lead turns the monitor of a host off
/// and on, through the workers host. The host replies with a note.
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_turns_the_monitor_of_a_host_off_and_on() {
    let r = Riff::new().await;
    let host = r.host().await;
    let config = r.home.path().join("config.toml");
    let lead_home = tempfile::tempdir().unwrap();
    let ask = |state: &str| {
        let out = r
            .riff(&["workers", "monitor", state, "--host", "pangolin"])
            .env("RIFF_HOME", lead_home.path())
            .env("RIFF_HOST", "a")
            .env("RIFF_SESSION", "l1")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(
            stdout(&out).contains(&format!(
                "Asked the workers host on pangolin: workers monitor {state}."
            )),
            "{out:?}"
        );
    };
    ask("off");
    r.lead_reads("pangolin: the monitor is off.").await;
    let monitor = riff::settings::monitor(&config).unwrap();
    assert!(!monitor.on);
    // Off: no look, also over the limit.
    r.numbers(13.0, 60);
    let at = r.saved().map(|s| s.at);
    ask("on");
    r.lead_reads("pangolin: the monitor is on: it looks each 1 seconds.")
        .await;
    assert!(riff::settings::monitor(&config).unwrap().on);
    r.lead_reads("over the limit 12.00").await;
    assert!(r.saved().map(|s| s.at) > at);
    drop(host);
}

/// `riff workers monitor` shows and sets the settings of the monitor.
#[tokio::test(flavor = "multi_thread")]
async fn workers_monitor_shows_and_sets_the_settings() {
    let r = Riff::new().await;
    let out = r
        .riff(&["workers", "monitor", "on", "--every", "5", "--load", "2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = stdout(&out);
    for want in [
        "monitor.on  true",
        "monitor.every  5",
        "monitor.load  2",
        "each 5 seconds",
        "over 16.00 (2 times 8 physical cores)",
        "less than 4 GB of memory is available (workers.floor)",
    ] {
        assert!(text.contains(want), "no {want:?}: {text}");
    }
    let out = r.riff(&["workers", "monitor", "off"]).output().unwrap();
    assert!(stdout(&out).contains("The monitor is off."), "{out:?}");
    let out = r
        .riff(&["workers", "monitor", "--every", "0"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let out = r
        .riff(&["workers", "monitor", "--host", "pangolin"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "--host needs on or off: {out:?}");
}

/// The book has a how-to with an `sh` block for the command and for
/// each setting, and the example of `riff top` shows the line of the
/// numbers.
#[test]
fn the_book_has_a_how_to_for_the_monitor() {
    let page = book::page("how-it-works.md");
    let heading = "### Watch the health of a machine";
    let start = page
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("the book has no {heading:?}"));
    let how = &page[start + 1..];
    let how = &how[..how[4..].find("\n### ").map_or(how.len(), |n| n + 4)];
    let commands = book::commands_in(how);
    for want in [
        "riff workers monitor on",
        "riff workers monitor on --host thelio",
        "riff workers monitor --every 15",
        "riff workers monitor --load 1.5",
        "riff workers floor 4",
        "riff workers monitor",
    ] {
        assert!(commands.iter().any(|c| c == want), "no {want:?}: {how}");
    }
    book::each_is_real(&commands);
    let top = book::page("how-it-works.md");
    assert!(
        top.contains("load 13.2 9.8/8  3000MHz now 2990  20GB avail/4  workers 3/4  jobs 2"),
        "the example of riff top has no line of numbers"
    );
}
