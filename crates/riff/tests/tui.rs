//! `riff` with no command in a pseudo-terminal: the TUI
//! (01M6VAZ7WH3MFAYF4PAZW6EDVY). It shows the six tabs and the frame,
//! the keys and a click change the tab, `?` shows the help, and `q`
//! ends it. The first look that fails ends it with the error of the
//! CLI, before it draws a screen (01M6VAZKAH6B53YKR8W4YKNSH6).

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use isolated::{Isolated, Span, in_time};
use tokio::process::Child;

/// The bound of each wait. It is generous: it only ends a test that
/// hangs.
const WAIT: Duration = Duration::from_secs(30);

/// A running `riff` in a pseudo-terminal of 80 x 24, and its screen.
struct Tty {
    child: Child,
    master: std::fs::File,
    screen: Arc<Mutex<vt100::Parser>>,
    dir: tempfile::TempDir,
}

impl Tty {
    const ROWS: u16 = 24;
    const COLUMNS: u16 = 80;

    /// Starts `riff ARGS` at the riff-server `url` as `user` on `host`,
    /// in a new pseudo-terminal. With `signed`, the command is signed in.
    fn start(url: &str, user: &str, host: &str, signed: bool, args: &[&str]) -> Tty {
        let size = nix::pty::Winsize {
            ws_row: Self::ROWS,
            ws_col: Self::COLUMNS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&size), None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        if signed {
            crate::common::sign_in(url, dir.path(), user);
        }
        let slave = std::fs::File::from(pty.slave);
        let child = Isolated::shared()
            .tokio_riff()
            .args(args)
            .current_dir(dir.path())
            .env("RIFF_SERVER", url)
            .env("RIFF_USER", user)
            .env("RIFF_HOST", host)
            .env("RIFF_HOME", dir.path())
            .env("TERM", "xterm")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let master = std::fs::File::from(pty.master);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(Self::ROWS, Self::COLUMNS, 0)));
        let mut reader = master.try_clone().unwrap();
        let parser = Arc::clone(&screen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            // The read fails when riff exits.
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                parser.lock().unwrap().process(&buf[..n]);
            }
        });
        Tty {
            child,
            master,
            screen,
            dir,
        }
    }

    /// Sends `bytes` to the terminal in one write: a key, or a mouse
    /// event of the terminal.
    async fn press(&mut self, bytes: &str) {
        self.master.write_all(bytes.as_bytes()).unwrap();
        self.master.flush().unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    /// The rows of the screen, with no spaces at their ends, once `test`
    /// holds for them.
    async fn shows(&self, test: impl Fn(&[String]) -> bool) -> Vec<String> {
        let span = Span::start();
        loop {
            let rows = self.rows();
            if test(&rows) {
                return rows;
            }
            assert!(span.within(WAIT), "the screen in time: {rows:#?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn rows(&self) -> Vec<String> {
        let parser = self.screen.lock().unwrap();
        parser
            .screen()
            .rows(0, Self::COLUMNS)
            .map(|row| row.trim_end().to_owned())
            .collect()
    }

    /// The `RIFF_HOME` of riff: its data folder too.
    fn home(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// The exit code of riff, once it exits.
    async fn exits(&mut self) -> Option<i32> {
        in_time(WAIT, self.child.wait())
            .await
            .expect("riff exits in time")
            .unwrap()
            .code()
    }
}

#[tokio::test]
async fn riff_with_no_command_in_a_terminal_shows_the_six_tabs_and_the_frame() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true, &[]);
    let rows = tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    assert_eq!(
        rows[0],
        "[1 Sessions]  2 Chat  3 Waves  4 Machines  5 Devices  6 Reports"
    );
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    assert!(
        rows[1].starts_with(&format!(
            " riff paused · owner brett · build {build} · link ok "
        )),
        "{rows:#?}"
    );
    assert_eq!(
        rows[2],
        " The Sessions tab is not built yet. Use these commands:"
    );
    assert_eq!(rows[23], " 1-6 tab  r read again  ? help  q quit");

    // The keys open a tab: a digit, Tab and Shift-Tab.
    tty.press("4").await;
    tty.shows(|rows| rows[0].contains("[4 Machines]")).await;
    tty.press("\t").await;
    tty.shows(|rows| rows[0].contains("[5 Devices]")).await;
    tty.press("\x1b[Z").await;
    tty.shows(|rows| rows[0].contains("[4 Machines]")).await;

    // A click of the mouse on `3 Waves` opens it: the SGR mouse event
    // of a press and a release at column 26, row 1.
    tty.press("\x1b[<0;26;1M\x1b[<0;26;1m").await;
    let rows = tty.shows(|rows| rows[0].contains("[3 Waves]")).await;
    assert_eq!(
        rows[2],
        " The Waves tab is not built yet. Use these commands:"
    );

    // `?` shows the keys of the tab, and Esc closes them.
    tty.press("?").await;
    tty.shows(|rows| rows.iter().any(|row| row.contains("Keys of the Waves tab")))
        .await;
    tty.press("\x1b").await;
    tty.shows(|rows| !rows.iter().any(|row| row.contains("Keys of the")))
        .await;

    // `q` ends the TUI, and gives back the screen of the terminal.
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
    let rows = tty.rows();
    assert!(
        !rows.iter().any(|row| row.contains("Sessions")),
        "{rows:#?}"
    );
}

/// 01M6VFCPK2PSDTHHE5TSY3XP5N, 01M6VFD09QN71SWMPP8YXCRAEK,
/// 01M6VFD0HGCVMYNNC3H4788W9X, 01M6VFD0M3RHT488D0RQ84RVMC,
/// 01M6VFD0PPXDNH3NKD3YE4W9SB, 01M6VFD0S90SB8M8WV952NT4Y1: the Machines
/// tab shows the facts that the daemon of thelio sent to riff-server.
/// This host has no daemon socket, so its row comes from riff-server.
/// `l` sets the limit of this host in the folder of its daemon, after
/// `y`, and `Enter` opens the Sessions tab with the filter of the host.
#[tokio::test(flavor = "multi_thread")]
async fn the_machines_tab_shows_the_hosts_and_sets_the_limit() {
    use riff_client::daemons::Facts;
    use riff_client::machines::{Disk, Machine};

    let server = crate::common::start_signed().await;
    // The session of the daemon of brett on thelio sends its facts.
    let daemon: riff_core::name::SessionUri = "riff://brett@thelio/o/r?session=d1".parse().unwrap();
    let api = server.api("brett", Some("d1"));
    api.register(&daemon).await.unwrap();
    let _watch = api.watch(&daemon).await.unwrap();
    let facts = Facts {
        build: riff_core::build::Build::this().to_string(),
        limit: 3,
        floor_gb: 8,
        machine: Some(Machine {
            threads: 16,
            mhz: 4500,
            now_mhz: 4400,
            mem_gb: 32,
            avail_gb: 24,
            load: 1.5,
        }),
        disk: Some(Disk {
            free_gb: 200,
            total_gb: 455,
        }),
        ..Facts::default()
    };
    api.machine(&daemon, &facts.line()).await.unwrap();

    let mut tty = Tty::start(&server, "brett", "thelio", true, &[]);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("4").await;
    let rows = tty
        .shows(|rows| rows[0].contains("[4 Machines]") && rows[2] == " 1 host")
        .await;
    assert_eq!(
        rows[3],
        "     HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN"
    );
    assert!(
        rows[4].starts_with(" >   thelio     0/3   1.5/16   24/32 GB    200/455 GB  57%   0     "),
        "{rows:#?}"
    );
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    assert!(
        rows.iter()
            .any(|row| row.starts_with(&format!("──thelio · riff daemon build {build} ─"))),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.starts_with(" link ok: ") && row.contains(" ms · ")),
        "{rows:#?}"
    );
    assert!(
        rows.contains(&" GitHub: no riff daemon here counts the points".to_owned()),
        "{rows:#?}"
    );
    assert_eq!(
        rows[23],
        " ↑↓ select  Enter sessions of the host  l limit  r read again  ? help  q quit"
    );

    // `l`, the digits, Enter, then `y`: the limit of this host.
    tty.press("l").await;
    tty.shows(|rows| rows[22] == " The most live sessions of thelio (now 3): █")
        .await;
    tty.press("5").await;
    tty.press("\r").await;
    tty.shows(|rows| rows[22] == " Set the limit of thelio to 5 sessions? y/N")
        .await;
    tty.press("y").await;
    tty.shows(|rows| {
        rows[22]
            == " thelio: The limit of this host: 5 live sessions, and 8 GB of free memory or more for a start."
                .chars()
                .take(80)
                .collect::<String>()
                .trim_end()
    })
    .await;
    let limit = std::fs::read_to_string(tty.home().join("daemon/limit.json")).unwrap();
    let limit: riff::room::Limit = serde_json::from_str(&limit).unwrap();
    assert_eq!((limit.sessions, limit.floor_gb), (5, 8));

    // Enter: the Sessions tab, with the filter of the host.
    tty.press("\r").await;
    let rows = tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    assert_eq!(rows[22], " The filter of the Sessions tab: host:thelio.");
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6VAZKAH6B53YKR8W4YKNSH6: when the first look fails, the TUI ends
/// with the error of the CLI, before it draws a screen.
#[tokio::test]
async fn a_first_look_that_fails_ends_the_tui_with_the_error_of_the_cli() {
    // Nothing listens at this riff-server.
    let server = "http://127.0.0.1:9";
    let dir = tempfile::tempdir().unwrap();
    let top = Isolated::shared()
        .riff()
        .args(["top", "--once"])
        .current_dir(dir.path())
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "brett")
        .env("RIFF_HOST", "thelio")
        .env("RIFF_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(top.status.code(), Some(1), "{top:?}");
    let error = String::from_utf8(top.stderr).unwrap();
    let first = error.lines().next().unwrap().to_owned();
    assert_eq!(
        first,
        format!("Error: cannot reach riff-server at {server}")
    );

    let mut tty = Tty::start(server, "brett", "thelio", false, &[]);
    assert_eq!(tty.exits().await, Some(1));
    let rows = tty
        .shows(|rows| rows.iter().any(|row| !row.is_empty()))
        .await;
    let shown = rows.join("\n");
    assert!(shown.contains(&first), "{first}: {shown}");
    assert!(!shown.contains("Sessions"), "{shown}");
}
