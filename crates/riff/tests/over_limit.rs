//! A lower limit of workers takes effect: when more workers run than the
//! limit of the machine, a worker that ends its item ends, in place of
//! the clear (01M402VFGAJQM1QW8B42NKMJM4, 01M402VFKXEJARG7CM60TDCMKW).
//! A fake `tmux` on `PATH` lists the worker panes from a file, and
//! `kill-pane` takes a pane out of it. Each test runs the real `riff`
//! binary.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::Duration;

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::{RiffState, StartReason};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::Memory;

/// Logs each call. `list-panes` prints the file `panes`, `kill-pane -t
/// P` takes the line of `P` out of it.
const FAKE_TMUX: &str = r#"#!/bin/sh
# Outside tmux, riff names its own server: -L riff (see start.rs).
[ "$1" = -L ] && shift 2
dir="$(dirname "$0")"
printf '%s\n' "$*" >> "$dir/log"
case "$1" in
list-panes) cat "$dir/panes" ;;
kill-pane) grep -v "^$3 " "$dir/panes" > "$dir/panes.new"; mv "$dir/panes.new" "$dir/panes" ;;
esac
"#;

struct Machine {
    /// The start of each session ID of the test. A worker that ends
    /// stops each process with its session ID, so the tests that run at
    /// one time do not share an ID.
    tag: String,
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    repo: tempfile::TempDir,
    api: Api,
    _service: Service,
}

impl Machine {
    /// A running riff with the lead `l1`, and `workers` workers on
    /// pangolin with the `limit`. The worker `wN` (its ID is
    /// [`Machine::id`]) runs in the pane `%N`, from 3 on, and holds
    /// `issue-N`.
    async fn new(limit: u16, workers: u16) -> Self {
        let store = Arc::new(Memory::default());
        let mut config = Config::default();
        config.lease.wait = Duration::from_millis(10);
        let service = Service::load(config, store).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = service.router();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let fake = tempfile::tempdir().unwrap();
        let tmux = fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = tempfile::tempdir().unwrap();
        let out = Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        let run = tempfile::tempdir().unwrap();
        std::fs::write(
            run.path().join("config.toml"),
            format!("[workers]\nlimit = {limit}\n"),
        )
        .unwrap();
        let tag = fake
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let m = Machine {
            tag,
            fake,
            run,
            repo,
            api: Api::new(&format!("http://{addr}")),
            _service: service,
        };
        let lead = m.uri("l1");
        m.api.register(&lead).await.unwrap();
        m.api.set_riff(&lead, RiffState::Running).await.unwrap();
        let mut panes = String::new();
        for n in 3..3 + workers {
            let w = m.uri(&m.id(n));
            m.api.start(&w, StartReason::Process, true).await.unwrap();
            let thread = w.default_thread().unwrap();
            m.api
                .claim(&w, &thread, &format!("issue-{n}"))
                .await
                .unwrap();
            panes.push_str(&format!("%{n} {}\n", m.id(n)));
        }
        std::fs::write(m.fake.path().join("panes"), panes).unwrap();
        m
    }

    /// The session ID of the worker `wN`.
    fn id(&self, n: u16) -> String {
        format!("{}-w{n}", self.tag)
    }

    fn uri(&self, id: &str) -> SessionUri {
        let place = identity::place_in(self.repo.path(), "pangolin").unwrap();
        SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default()
    }

    /// The panes of the workers that run.
    fn panes(&self) -> Vec<String> {
        let panes = std::fs::read_to_string(self.fake.path().join("panes")).unwrap();
        panes
            .lines()
            .map(|l| l[..l.find(' ').unwrap()].to_owned())
            .collect()
    }

    /// A riff command of the session `id` in the pane `pane`.
    fn riff(&self, id: &str, pane: &str, worker: bool, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SESSION", id)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", pane)
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("CLAUDE_CODE_SESSION_ID");
        if worker {
            cmd.env("RIFF_WORKER", "1");
        } else {
            cmd.env_remove("RIFF_WORKER");
        }
        cmd
    }

    /// The worker `wN` releases its item.
    async fn release(&self, n: u16) {
        let w = self.uri(&self.id(n));
        let thread = w.default_thread().unwrap();
        self.api
            .release(&w, &thread, &format!("issue-{n}"))
            .await
            .unwrap();
    }

    /// The turn of the worker `wN` ends: its Stop hook.
    fn stop_hook(&self, n: u16) {
        let id = self.id(n);
        let mut hook = self
            .riff(&id, &format!("%{n}"), true, &["hook", "stop"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
        std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
        let out = hook.wait_with_output().unwrap();
        assert!(out.status.success(), "{out:?}");
    }

    /// Waits until the pane `%N` ended or got the start prompt, and
    /// gives true when it ended.
    async fn ended(&self, n: u16) -> bool {
        let (kill, keys) = (
            format!("kill-pane -t %{n}"),
            format!("send-keys -t %{n} -l Join the riff."),
        );
        let span = isolated::Span::start();
        loop {
            let log = self.log();
            if log.lines().any(|l| l == kill) {
                return true;
            }
            if log.lines().any(|l| l == keys) {
                return false;
            }
            assert!(
                span.within(Duration::from_secs(20)),
                "no end and no clear of %{n}: {log}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The history of the repository thread, as the lead reads it.
    fn history(&self) -> String {
        let out: Output = self
            .riff("l1", "%1", false, &["read", "--all"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// Limit 2, 4 workers: each ends its item. The first 2 end, the last 2
/// get the clear. 2 workers stay, and the lead gets a note for each end.
#[tokio::test(flavor = "multi_thread")]
async fn with_limit_2_and_4_workers_2_workers_stay() {
    let m = Machine::new(2, 4).await;
    let mut ended = Vec::new();
    for n in 3..7 {
        m.release(n).await;
        m.stop_hook(n);
        ended.push(m.ended(n).await);
    }
    assert_eq!(ended, [true, true, false, false], "{}", m.log());
    assert_eq!(m.panes(), ["%5", "%6"]);
    let history = m.history();
    for (pane, runs) in [("%3", 4), ("%4", 3)] {
        let note = format!(
            "workers: limit 2, runs {runs} on pangolin: the worker in the pane {pane} ends"
        );
        assert!(history.contains(&note), "{history}");
    }
}

/// A worker in the middle of an item is not stopped: a turn that ends
/// while it holds its claim gives no end.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_a_claim_does_not_end() {
    let m = Machine::new(1, 2).await;
    m.stop_hook(3);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(!m.log().contains("kill-pane"), "{}", m.log());
    assert_eq!(m.panes(), ["%3", "%4"]);
}

/// Two workers end their items at one time, and only one is over the
/// limit: only one ends. The lock makes the count and the end one step.
#[tokio::test(flavor = "multi_thread")]
async fn two_workers_at_one_time_end_only_the_one_over_the_limit() {
    let m = Machine::new(3, 4).await;
    m.release(3).await;
    m.release(4).await;
    m.stop_hook(3);
    m.stop_hook(4);
    let ended = [m.ended(3).await, m.ended(4).await];
    assert_eq!(
        ended.iter().filter(|e| **e).count(),
        1,
        "{ended:?}: {}",
        m.log()
    );
    assert_eq!(m.panes().len(), 3);
}
