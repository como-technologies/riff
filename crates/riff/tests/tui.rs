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

/// The Chat tab (#1640): it shows the history of the thread `chat` as
/// `riff chat` does, a new line while another tab shows adds to
/// `Chat•N`, the note line says whom a line will wake, and Enter sends
/// the line as the person. The lead wakes, and the line shows once.
#[tokio::test]
async fn the_chat_tab_shows_the_chat_and_sends_a_line_that_wakes_a_lead() {
    use futures::StreamExt;
    use riff_core::name::SessionUri;
    use riff_core::wire::Kind;

    let server = crate::common::start_signed().await;
    let chat = riff::chat::thread();
    // A line of mike before the TUI starts: the history.
    let mike: SessionUri = "riff://mike@heron".parse().unwrap();
    let mike_api = server.api("mike", None);
    mike_api.join(&mike, &chat).await.unwrap();
    mike_api
        .post(&mike, Some(&chat), &[], "hello brett", Kind::Message)
        .await
        .unwrap();
    // The lead of brett in como-technologies/riff. A client of this
    // process has one sign-in per server: brett is the last one.
    let lead: SessionUri = "riff://brett@thelio/como-technologies/riff?session=brett-lead"
        .parse()
        .unwrap();
    crate::common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    crate::common::name_lead(&server.api("brett", None), &lead).await;
    let lead_api = crate::common::api_of(&server, &lead);
    let mut wakes = Box::pin(lead_api.watch(&lead).await.unwrap());

    let mut tty = Tty::start(&server, "brett", "thelio", true, &[]);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    // A new line while the Sessions tab shows: the news of the chat.
    lead_api
        .post(&lead, Some(&chat), &[], "are you there?", Kind::Message)
        .await
        .unwrap();
    tty.shows(|rows| rows[0].contains("  2 Chat•1  ")).await;

    // `2` opens the Chat tab: the history and the new line, as in
    // `riff chat`, and the prompt.
    tty.press("2").await;
    let shown = |rows: &[String], text: &str| rows.iter().filter(|r| r.ends_with(text)).count();
    let rows = tty
        .shows(|rows| shown(rows, "[brett's lead, riff] are you there?") == 1)
        .await;
    assert_eq!(
        rows[0],
        " 1 Sessions  [2 Chat]  3 Waves  4 Machines  5 Devices  6 Reports"
    );
    assert!(rows[2].starts_with(" chat · online: brett"), "{rows:#?}");
    assert_eq!(shown(&rows, "<mike@heron> hello brett"), 1, "{rows:#?}");
    assert_eq!(rows[21], ">");
    assert_eq!(
        rows[23],
        " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit"
    );

    // The note line says whom the line will wake, before Enter.
    tty.press("@lead:riff is #12 done?").await;
    tty.shows(|rows| rows[22] == " Enter wakes your lead in como-technologies/riff.")
        .await;
    tty.press("\r").await;
    let wake = in_time(WAIT, wakes.next())
        .await
        .expect("the lead of brett wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.thread, chat);
    assert_eq!(wake.from.to_string(), "riff://brett@thelio");
    let rows = tty
        .shows(|rows| shown(rows, "<brett@thelio> @lead:riff is #12 done?") == 1)
        .await;
    assert_eq!(rows[21], ">", "{rows:#?}");

    // Esc leaves the prompt, and `q` ends the TUI.
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}
