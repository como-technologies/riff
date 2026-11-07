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

    /// Starts `riff` at the riff-server `url` as `user` on `host`,
    /// in a new pseudo-terminal. With `signed`, the command is signed in.
    /// A fake `gh` comes first on `PATH`: each repository has the open
    /// issue #12, "Show the wave".
    fn start(url: &str, user: &str, host: &str, signed: bool) -> Tty {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        isolated::write_program(
            bin.join("gh"),
            "#!/bin/sh\necho '[{\"number\": 12, \"title\": \"Show the wave\", \"milestone\": null}]'\n",
        );
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap()
        .into_string()
        .unwrap();
        Self::start_in(dir, url, user, host, signed, &[("PATH", path)])
    }

    /// [`Tty::start`] in the folder `dir`, with the variables `vars` too.
    fn start_in(
        dir: tempfile::TempDir,
        url: &str,
        user: &str,
        host: &str,
        signed: bool,
        vars: &[(&str, String)],
    ) -> Tty {
        let size = nix::pty::Winsize {
            ws_row: Self::ROWS,
            ws_col: Self::COLUMNS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&size), None).unwrap();
        if signed {
            crate::common::sign_in(url, dir.path(), user);
        }
        let slave = std::fs::File::from(pty.slave);
        let mut command = Isolated::shared().tokio_riff();
        command
            .current_dir(dir.path())
            .env("RIFF_SERVER", url)
            .env("RIFF_USER", user)
            .env("RIFF_HOST", host)
            .env("RIFF_HOME", dir.path())
            .env("TERM", "xterm")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .kill_on_drop(true);
        for (name, value) in vars {
            command.env(name, value);
        }
        let child = command.spawn().unwrap();
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
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    let rows = tty
        .shows(|rows| rows[0].starts_with("[1 Sessions]") && !rows[22].is_empty())
        .await;
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
    // The Sessions tab: no session yet, and no riff daemon here.
    assert!(
        rows[2].starts_with(" 0 sessions") && rows[2].ends_with("by person"),
        "{rows:#?}"
    );
    assert_eq!(rows[22], format!(" {}", riff::text::NO_DAEMON_HERE));
    assert_eq!(
        rows[23],
        " ↑↓ select  ←→ fold  Enter open  l lead  t take over  p pause  / filter  ? help"
    );

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
    // No repository has a live session, and the folder is no clone: no
    // board (#1638).
    let rows = tty
        .shows(|rows| rows[0].contains("[3 Waves]") && !rows[2].is_empty())
        .await;
    assert_eq!(rows[2], format!(" {}", riff::tui::waves::NO_BOARD[0]));

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

    let mut tty = Tty::start(&server, "brett", "thelio", true);
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
        rows[4].starts_with(" >   thelio     0/3   1.5/16   24/32 GB    200/455 GB  56%   0     "),
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
    assert!(rows[2].ends_with("/host:thelio · by person"), "{rows:#?}");
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// A session `uri` on the riff-server `server`, with the claim `item`
/// in the thread of its repository. It is live until the test ends.
async fn session(server: &str, uri: &str, item: &str) {
    let uri: riff_core::name::SessionUri = uri.parse().unwrap();
    let api = crate::common::api_of(server, &uri);
    api.register(&uri).await.unwrap();
    let thread = uri.default_thread().unwrap();
    api.claim(&uri, &thread, item).await.unwrap();
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&uri).await.unwrap();
        open.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    opened.await.unwrap();
}

/// 01M6VGGW73KKG6Q0TZNBGYN7E0, 01M6VGH4JTM81PZH3F4R51VWG5,
/// 01M6VGH4Y0G25VYBBD5ZV4GG2H: the Sessions tab shows the sessions of
/// riff-server in the tree, `/` filters them, and `p` pauses the
/// repository of the row after `y`, with the code of `riff pause`.
#[tokio::test]
async fn the_sessions_tab_shows_the_tree_filters_it_and_pauses_a_repository() {
    let server = crate::common::start_signed().await;
    // A new riff starts paused: the owner brett resumes it, as `riff
    // resume --riff` does.
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    crate::common::api_of(&server, &brett)
        .set_pause(
            &brett,
            &riff_client::api::PauseScope::Riff,
            riff_core::wire::RiffState::Running,
        )
        .await
        .unwrap();
    session(
        &server,
        "riff://mike@pangolin/como-technologies/riff?session=5b1e2a90",
        "issue-12",
    )
    .await;
    session(
        &server,
        "riff://mike@pangolin/como-technologies/strata?session=9c0d1e2f",
        "issue-88",
    )
    .await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    // The groups of mike need no look: one folded line with the counts.
    let rows = tty
        .shows(|rows| rows.iter().any(|row| row.starts_with(" ▶ mike")))
        .await;
    assert!(rows[2].starts_with(" 2 sessions: 2 busy"), "{rows:#?}");
    let mike = rows.iter().find(|row| row.starts_with(" ▶ mike")).unwrap();
    assert!(mike.ends_with("2 sessions · 2 busy"), "{rows:#?}");

    // `/` and a word: only the rows that match, in open groups.
    tty.press("/").await;
    for key in ["#", "1", "2"] {
        tty.press(key).await;
    }
    tty.shows(|rows| rows[22] == " /#12█").await;
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| rows.iter().any(|row| row.contains("5b1e2a90")))
        .await;
    assert!(rows[2].starts_with(" 1 session: 1 busy"), "{rows:#?}");
    assert!(rows[2].ends_with("/#12 · by person"), "{rows:#?}");
    assert!(
        !rows.iter().any(|row| row.contains("9c0d1e2f")),
        "{rows:#?}"
    );
    let row = rows.iter().find(|row| row.contains("5b1e2a90")).unwrap();
    assert!(row.contains("5b1e2a90  #12 "), "{rows:#?}");
    // The detail of the selected session has the title from `gh`.
    tty.shows(|rows| {
        rows.iter()
            .any(|row| row.starts_with("──5b1e2a90 · #12 Show the wave · pangolin ─"))
    })
    .await;

    // `p` asks first, `n` changes nothing, and `y` pauses.
    tty.press("p").await;
    let ask = " Pause como-technologies/riff? y/N";
    tty.shows(|rows| rows[22] == ask).await;
    tty.press("n").await;
    tty.shows(|rows| rows[22] == format!(" {}", riff::tui::NOTHING_CHANGED))
        .await;
    tty.press("p").await;
    tty.shows(|rows| rows[22] == ask).await;
    tty.press("y").await;
    let rows = tty
        .shows(|rows| rows[22].starts_with(" The repository como-technologies/riff is paused now."))
        .await;
    // The next look shows the pause: the session is paused.
    tty.press("r").await;
    tty.shows(|rows| rows[2].starts_with(" 1 session: 1 paused"))
        .await;
    assert!(!rows.is_empty());

    // Esc clears the filter, and `q` ends the TUI.
    tty.press("\x1b").await;
    tty.shows(|rows| rows[2].starts_with(" 2 sessions")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6VJQPJXKHKJ8WT1CAQMN1YR, 01M6VJQPNJ4AX4RT877F1QAPPX: in the
/// Machines tab, `/` with a word shows only the hosts that match, and
/// Esc shows each host again. While the person types, `g` and `q` are
/// letters of the filter: `g` selects no host, and `q` does not end the
/// TUI.
#[tokio::test(flavor = "multi_thread")]
async fn the_machines_tab_filters_the_hosts() {
    use riff_client::daemons::Facts;
    use riff_client::machines::{Disk, Machine};

    let server = crate::common::start_signed().await;
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
    // The daemons of brett on thelio and on pangolin send their facts.
    let daemons: Vec<(riff_core::name::SessionUri, _)> = [("thelio", "d1"), ("pangolin", "d2")]
        .into_iter()
        .map(|(host, session)| {
            let uri = format!("riff://brett@{host}/o/r?session={session}");
            (uri.parse().unwrap(), server.api("brett", Some(session)))
        })
        .collect();
    let mut watches = Vec::new();
    for (daemon, api) in &daemons {
        api.register(daemon).await.unwrap();
        watches.push(api.watch(daemon).await.unwrap());
        api.machine(daemon, &facts.line()).await.unwrap();
    }

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("4").await;
    let rows = tty
        .shows(|rows| rows[0].contains("[4 Machines]") && rows[2] == " 2 hosts")
        .await;
    assert!(rows[4].starts_with(" >   pangolin "), "{rows:#?}");
    assert!(rows[5].starts_with("     thelio "), "{rows:#?}");

    // `/`, then a word: only pangolin shows.
    tty.press("/").await;
    tty.press("pang").await;
    let rows = tty
        .shows(|rows| rows[2] == " 1 of 2 hosts · /pang" && rows[22] == " /pang█")
        .await;
    assert!(rows[4].starts_with(" >   pangolin "), "{rows:#?}");
    assert_eq!(rows[5], "");
    assert_eq!(rows[23], " Enter keep the filter  Esc clear it");
    // `q` is a letter too, and Backspace takes it away.
    tty.press("q").await;
    tty.shows(|rows| rows[2] == " 0 of 2 hosts · /pangq").await;
    tty.press("\x7f").await;
    tty.shows(|rows| rows[2] == " 1 of 2 hosts · /pang").await;
    // Enter keeps the filter.
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| rows[22].is_empty() && rows[23].starts_with(" ↑↓ select"))
        .await;
    assert_eq!(rows[2], " 1 of 2 hosts · /pang");

    // Esc shows each host again.
    tty.press("\x1b").await;
    let rows = tty.shows(|rows| rows[2] == " 2 hosts").await;
    assert!(rows[4].starts_with(" >   pangolin "), "{rows:#?}");
    assert!(rows[5].starts_with("     thelio "), "{rows:#?}");
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

    let mut tty = Tty::start(server, "brett", "thelio", false);
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

    let mut tty = Tty::start(&server, "brett", "thelio", true);
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

/// A fake `gh` for the clone of acme/app: the open issues of Wave 3, as
/// the REST list gives them, and no closed item, no pull request and no
/// check. It logs each call to the file `calls` of its folder.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  *'issues?state=open'*)
    printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n'
    printf '[{"number":12,"title":"Show the board","milestone":{"number":5,"title":"Wave 3"}},'
    printf '{"number":13,"title":"Hold this one","milestone":{"number":5,"title":"Wave 3"}}]' ;;
  *'issues?state=closed'*) printf 'HTTP/2.0 200 OK\r\n\r\n[]' ;;
  'api repos/acme/app') echo '{"default_branch":"main"}' ;;
  *) echo '[]' ;;
esac
"#;

/// 01M6VFG882T794B1CTHK5AKPDG, 01M6VFGK9XXWCA8X7CE8EZNSJV,
/// 01M6VFGK4P2FWMFQHBZKR3JXSZ: in a clone of acme/app, the Waves tab
/// shows the board of its current wave from `gh`, and holds, frees and
/// pauses with the code of `riff plan hold`, `riff plan free` and `riff
/// pause`, each after `y`.
#[tokio::test]
async fn the_waves_tab_shows_the_board_and_holds_frees_and_pauses() {
    let server = crate::common::start_signed().await;
    let clone = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    ] {
        let git = std::process::Command::new("git")
            .args(args)
            .current_dir(clone.path())
            .status()
            .unwrap();
        assert!(git.success());
    }
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), FAKE_GH);
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut tty = Tty::start_in(clone, &server, "brett", "thelio", true, &[("PATH", path)]);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;

    // `3` opens the Waves tab: the board of acme/app, from the REST list
    // of the open issues.
    tty.press("3").await;
    let rows = tty
        .shows(|rows| rows.iter().any(|r| r.contains("#13")))
        .await;
    assert!(
        rows[2].starts_with(" ▼ acme/app · Wave 3 · paused"),
        "{rows:#?}"
    );
    assert!(
        rows[4].starts_with(" > ○   free    #12    Show the board"),
        "{rows:#?}"
    );
    assert!(
        rows[5].starts_with("   ○   free    #13    Hold this one"),
        "{rows:#?}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("api -i repos/acme/app/issues?state=open"),
        "{calls}"
    );
    assert!(!calls.contains("pr list"), "no open pull request: {calls}");

    // `h` on #13: a reason, then `y`. The hold shows at the next look.
    tty.press("j").await;
    tty.press("h").await;
    tty.press("later").await;
    tty.shows(|rows| rows[22].ends_with("The reason: later█"))
        .await;
    tty.press("\r").await;
    tty.shows(|rows| rows[22] == " Hold #13 of acme/app: later? y/N")
        .await;
    tty.press("y").await;
    let rows = tty
        .shows(|rows| rows[5].starts_with(" > ◌   held    #13    Hold this one"))
        .await;
    assert!(rows[5].contains("later"), "{rows:#?}");
    tty.shows(|rows| rows[22].contains("issue-13 in acme/app is on hold now"))
        .await;

    // `f` frees it after `y`.
    tty.press("f").await;
    tty.shows(|rows| rows[22] == " Free #13 of acme/app? y/N")
        .await;
    tty.press("y").await;
    tty.shows(|rows| rows[5].starts_with(" > ○   free    #13"))
        .await;

    // `p` pauses the repository after `y`, and `n` changes nothing.
    tty.press("p").await;
    tty.shows(|rows| rows[22] == " Pause acme/app? y/N").await;
    tty.press("n").await;
    tty.shows(|rows| rows[22] == " Nothing changed.").await;
    tty.press("p").await;
    tty.press("y").await;
    tty.shows(|rows| rows[22].contains("The repository acme/app is paused now"))
        .await;

    // `P` resumes the whole riff after `y`: the header says so.
    tty.press("P").await;
    tty.shows(|rows| rows[22] == " Resume the whole riff? y/N")
        .await;
    tty.press("y").await;
    tty.shows(|rows| rows[1].starts_with(" riff running")).await;
    // The repository stays paused.
    tty.shows(|rows| rows[2].starts_with(" ▼ acme/app · Wave 3 · paused"))
        .await;

    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6VMACJGF4A0ZXC6YK13KNCH: a post of the Chat tab that fails while
/// the prompt holds a line shows `cannot send the line` in the note line
/// at once. The next key brings back the note of the wake. The server
/// holds the post of the line `this line fails` until the test types the
/// next line, then it refuses the post.
#[tokio::test]
async fn a_post_that_fails_while_the_prompt_holds_a_line_shows_until_the_next_key() {
    use axum::body::{Body, to_bytes};
    use axum::extract::Request;
    use axum::http::StatusCode;
    use axum::middleware::{Next, from_fn};
    use axum::response::Response;

    let go = Arc::new(tokio::sync::Notify::new());
    let gate = Arc::clone(&go);
    let server = crate::common::start_signed_with(move |router| {
        router.layer(from_fn(move |request: Request, next: Next| {
            let gate = Arc::clone(&gate);
            async move {
                let (head, body) = request.into_parts();
                let bytes = to_bytes(body, usize::MAX).await.unwrap();
                let fails = head.uri.path() == "/v1/post"
                    && serde_json::from_slice::<serde_json::Value>(&bytes)
                        .is_ok_and(|post| post["body"] == "this line fails");
                if fails {
                    gate.notified().await;
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header(
                            riff_core::build::HEADER,
                            riff_core::build::Build::this().to_string(),
                        )
                        .header(riff_core::wire::REFUSED_HEADER, "bad_request")
                        .body(Body::from("no post now"))
                        .unwrap();
                }
                next.run(Request::from_parts(head, Body::from(bytes))).await
            }
        }))
    })
    .await;

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press("this line fails\r").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press("hi all").await;
    tty.shows(|rows| rows[22] == " Enter wakes each lead of yours.")
        .await;

    // The post fails while the prompt holds a line: the failure shows.
    go.notify_one();
    let rows = tty
        .shows(|rows| rows[22].starts_with(" riff: cannot send the line:"))
        .await;
    assert_eq!(
        rows[22],
        " riff: cannot send the line: post failed (400 Bad Request): no post now"
    );
    assert_eq!(rows[21], "> hi all", "{rows:#?}");

    // The next key brings back the note of the wake.
    tty.press("!").await;
    let rows = tty
        .shows(|rows| rows[22] == " Enter wakes each lead of yours.")
        .await;
    assert_eq!(rows[21], "> hi all!", "{rows:#?}");
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}
