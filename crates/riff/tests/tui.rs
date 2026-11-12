//! `riff` with no command in a pseudo-terminal: the TUI
//! (01M6VAZ7WH3MFAYF4PAZW6EDVY). It shows the six tabs and the frame,
//! the keys and a click change the tab, `?` shows the help, and `q`
//! ends it. The first look that fails ends it with the error of the
//! CLI, before it draws a screen (01M6VAZKAH6B53YKR8W4YKNSH6).

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use isolated::{Isolate, Isolated, Span, in_time};
use tokio::process::Child;

/// The bound of each wait. It is generous: it only ends a test that
/// hangs.
const WAIT: Duration = Duration::from_secs(30);

/// The keys line of the Machines tab, in row 23.
const MACHINES_KEYS: &str =
    " ↑↓ select  Enter sessions of the host  l limit  r read again  ? help  q quit";

/// A running `riff` in a pseudo-terminal of 80 x 24, its screen, and
/// each byte that it wrote.
struct Tty {
    child: Child,
    master: std::fs::File,
    screen: Arc<Mutex<vt100::Parser>>,
    written: Arc<Mutex<Vec<u8>>>,
    dir: tempfile::TempDir,
}

impl Tty {
    const ROWS: u16 = 24;
    const COLUMNS: u16 = 80;

    /// Starts `riff` at the riff-server `url` as `user` on `host`,
    /// in a new pseudo-terminal. With `signed`, the command is signed in.
    /// A fake `gh` comes first on `PATH`: each repository has the open
    /// issue #12, "Show the wave", in the REST list of the open issues.
    fn start(url: &str, user: &str, host: &str, signed: bool) -> Tty {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        isolated::write_program(
            bin.join("gh"),
            r#"#!/bin/sh
case "$*" in
  *'issues?state=open'*) printf 'HTTP/2.0 200 OK\r\n\r\n' ;;
esac
echo '[{"number": 12, "title": "Show the wave", "milestone": null}]'
"#,
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
        let riff = Isolated::shared().riff_path();
        Self::start_program(&riff, dir, url, user, host, signed, vars)
    }

    /// [`Tty::start_in`] with the riff file `riff`.
    fn start_program(
        riff: &std::path::Path,
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
        let mut command = tokio::process::Command::new(riff);
        command
            .isolate(Isolated::shared())
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
        let written = Arc::new(Mutex::new(Vec::new()));
        let mut reader = master.try_clone().unwrap();
        let (parser, all) = (Arc::clone(&screen), Arc::clone(&written));
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            // The read fails when riff exits.
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                parser.lock().unwrap().process(&buf[..n]);
                all.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        Tty {
            child,
            master,
            screen,
            written,
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

    /// True while the terminal has its bracketed paste on.
    fn bracketed_paste(&self) -> bool {
        self.screen.lock().unwrap().screen().bracketed_paste()
    }

    /// Waits until riff wrote `bytes`, for example a mode of the
    /// terminal, which the screen does not show.
    async fn wrote(&self, bytes: &str) {
        let span = Span::start();
        loop {
            let found = self
                .written
                .lock()
                .unwrap()
                .windows(bytes.len())
                .any(|w| w == bytes.as_bytes());
            if found {
                return;
            }
            assert!(span.within(WAIT), "riff writes {bytes:?} in time");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The row and the column of the cursor, and true while it shows.
    fn cursor(&self) -> (u16, u16, bool) {
        let parser = self.screen.lock().unwrap();
        let (row, column) = parser.screen().cursor_position();
        (row, column, !parser.screen().hide_cursor())
    }

    /// The color of the text at the start of the row `row`, after its
    /// first column.
    fn color_at(&self, row: u16) -> vt100::Color {
        let parser = self.screen.lock().unwrap();
        parser
            .screen()
            .cell(row, 1)
            .map_or(vt100::Color::Default, |cell| cell.fgcolor())
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
        rows[1].starts_with(&format!(" riff paused · build {build} · link ok ")),
        "{rows:#?}"
    );
    // The Sessions tab: no session yet, and no riff daemon here.
    assert!(
        rows[2].starts_with(" 0 sessions") && rows[2].ends_with("by repo"),
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

/// 01M748V7Y8DQPCQJ9D2Q7HKB76: a TUI keeps its build. With its own build
/// in its riff file, no row comes under the header. After a newer riff
/// came in the place of that file, as `riff update` puts it, the next
/// look puts a row under the header that names the new build and the
/// step. The body of the tab starts under the row.
#[tokio::test]
async fn a_newer_riff_on_disk_gets_a_row_under_the_header_that_names_it_and_the_step() {
    use riff_core::build::{Build, Semver};

    let server = crate::common::start_signed().await;
    let bin = tempfile::tempdir().unwrap();
    let riff = bin.path().join("riff");
    let built = Isolated::shared().riff_path();
    if std::fs::hard_link(&built, &riff).is_err() {
        isolated::copy_program(&built, &riff);
    }
    let dir = tempfile::tempdir().unwrap();
    let mut tty = Tty::start_program(&riff, dir, &server, "brett", "thelio", true, &[]);
    let rows = tty
        .shows(|rows| rows[0].starts_with("[1 Sessions]") && !rows[22].is_empty())
        .await;
    assert!(rows[2].starts_with(" 0 sessions"), "{rows:#?}");

    let this = Build::this();
    let line = this.semver().unwrap();
    let newer = Build {
        version: Semver {
            patch: line.patch + 1,
            ..line
        }
        .to_string(),
        id: "3f2a91c00000".into(),
        time: this.time.clone(),
    };
    let new = bin.path().join("new");
    isolated::write_program(&new, format!("#!/bin/sh\necho riff {newer}\n"));
    std::fs::rename(&new, &riff).unwrap();
    let want = format!(
        " riff {} 3f2a91c is installed: press q, then start riff again",
        newer.version
    );
    let rows = tty.shows(|rows| rows[2] == want).await;
    assert!(rows[3].starts_with(" 0 sessions"), "{rows:#?}");
    assert_ne!(tty.color_at(2), vt100::Color::Default);

    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
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
    use riff_client::daemons::{Facts, Parts, RepoParts};
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
        parts: vec![RepoParts {
            repo: "o/r".into(),
            parts: Parts {
                workers: true,
                lead: false,
            },
        }],
        waiting: vec!["riff-issue-7-1a2b3c4d".into()],
        test: Some(riff_client::daemons::Test {
            tag: "v4.1.1".into(),
            commit: "90e36bfd1c2b".into(),
            secs: 185,
        }),
        ..Facts::default()
    };
    api.machine(&daemon, &facts.line()).await.unwrap();

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("4").await;
    // riff writes a frame in parts, from row 0 to row 23. The keys line
    // in row 23 and the time of the call in the link row tell that the
    // whole frame of the Machines tab is on the screen (#1936).
    let rows = tty
        .shows(|rows| {
            rows[0].contains("[4 Machines]")
                && rows[2] == " 1 host"
                && rows[23] == MACHINES_KEYS
                && rows
                    .iter()
                    .any(|row| row.starts_with(" link ok: ") && row.contains(" ms · "))
        })
        .await;
    assert_eq!(
        rows[3],
        "     HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN"
    );
    assert!(
        rows[4].starts_with(" >   thelio     0/3   1.5/16   24/32 GB    200/455 GB  56%   1     "),
        "{rows:#?}"
    );
    // 01M6WPJHM4XTAQ1BQ8BQH862S5: the detail names the start that waits
    // and the real-session test, from the facts that the daemon sent.
    let at = rows
        .iter()
        .position(|row| row.starts_with(" limit 3 sessions and a floor of 8 GB"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    assert_eq!(
        rows[at..at + 3],
        [
            " limit 3 sessions and a floor of 8 GB · 1 start waits",
            " waits for room: riff-issue-7-1a2b3c4d",
            " real-session test of v4.1.1 on 90e36bfd, for 3 min",
        ]
    );
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    assert!(
        rows.iter()
            .any(|row| row.starts_with(&format!("──thelio · riff daemon build {build} ─"))),
        "{rows:#?}"
    );
    assert!(
        rows.contains(&" GitHub: no riff daemon here counts the points".to_owned()),
        "{rows:#?}"
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

    // Enter: the Sessions tab, with the filter of the host. The
    // summary line starts with it, and the note line says so.
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| {
            rows[0].starts_with("[1 Sessions]")
                && rows[22] == " The sessions of thelio: Esc shows all."
                && rows[23].contains("  Esc clear the filter  ")
        })
        .await;
    assert!(
        rows[2].starts_with(" filter host:thelio · ") && rows[2].ends_with(" by repo"),
        "{rows:#?}"
    );
    // Esc clears the filter and the note.
    tty.press("\x1b").await;
    tty.shows(|rows| !rows[2].starts_with(" filter") && !rows[22].contains("Esc shows all"))
        .await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6XTS1GCW7PTWSMM1HKQTHSN: the header of the TUI at 80 x 24 says the
/// state of an open release at each step, from the facts that the daemon of
/// thelio sent to riff-server and from the pause of the riff. The line under
/// the header moves the body of the tab down. With no open release, it goes.
#[tokio::test(flavor = "multi_thread")]
async fn the_header_says_the_state_of_an_open_release_at_each_step() {
    use riff_client::daemons::{Facts, Release, ReleaseStep};
    use riff_core::wire::RiffState;

    let server = crate::common::start_signed().await;
    let daemon: riff_core::name::SessionUri =
        "riff://brett@thelio/como-technologies/riff?session=d1"
            .parse()
            .unwrap();
    let api = server.api("brett", Some("d1"));
    api.register(&daemon).await.unwrap();
    let _watch = api.watch(&daemon).await.unwrap();
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    let person = crate::common::api_of(&server, &brett);

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    let steps: [(Option<ReleaseStep>, RiffState, &[&str]); 5] = [
        (
            Some(ReleaseStep::Waits),
            RiffState::Running,
            &[" v3.3.0: the deploy waits for a person (riff release approve v3.3.0)"],
        ),
        (
            Some(ReleaseStep::Runs),
            RiffState::Paused,
            &[" v3.3.0: the deploy runs; the riff is paused"],
        ),
        (
            Some(ReleaseStep::Passed),
            RiffState::Paused,
            &[
                " v3.3.0: the deploy passed; it waits for the check of the lead of",
                " como-technologies/riff",
            ],
        ),
        (
            Some(ReleaseStep::CheckFailed),
            RiffState::Paused,
            &[" v3.3.0: the check failed: an admin resumes or rolls back"],
        ),
        (None, RiffState::Running, &[]),
    ];
    for (step, state, lines) in steps {
        let facts = Facts {
            build: riff_core::build::Build::this().to_string(),
            limit: 3,
            releases: step
                .map(|step| Release {
                    repo: "como-technologies/riff".into(),
                    tag: "v3.3.0".into(),
                    step,
                })
                .into_iter()
                .collect(),
            ..Facts::default()
        };
        api.machine(&daemon, &facts.line()).await.unwrap();
        person
            .set_pause(&brett, &riff_client::api::PauseScope::Riff, state)
            .await
            .unwrap();
        tty.press("r").await;
        let word = match state {
            RiffState::Paused => "paused",
            RiffState::Running => "running",
        };
        let header = format!(" riff {word} · build {build} · link ok ");
        let rows = tty
            .shows(|rows| {
                rows[1].starts_with(&header)
                    && rows[2..2 + lines.len()] == *lines
                    && rows[2 + lines.len()].starts_with(" 1 session")
            })
            .await;
        assert!(rows[23].starts_with(" ↑↓ select"), "{step:?}: {rows:#?}");
    }
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M75R6FPVZA3JA5C48PQ26A85, 01M7642H27T4862WZENFAN4277: the header of the
/// TUI at 80 x 24 says the step of a release from its release issue, from the
/// facts that the watch of the wave of the daemon of thelio sent to
/// riff-server: the release build passed, the real-session test runs, it
/// passed and the lead cuts next, and the deploy waits for the approve. Of
/// one tag, the latest step that a host saw wins.
#[tokio::test(flavor = "multi_thread")]
async fn the_header_says_the_step_of_a_release_from_the_test_to_the_approve() {
    use riff_client::daemons::{Facts, Release, ReleaseStep};

    let server = crate::common::start_signed().await;
    let daemon: riff_core::name::SessionUri =
        "riff://brett@thelio/como-technologies/riff?session=d1"
            .parse()
            .unwrap();
    let api = server.api("brett", Some("d1"));
    api.register(&daemon).await.unwrap();
    let _watch = api.watch(&daemon).await.unwrap();
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    crate::common::api_of(&server, &brett)
        .set_pause(
            &brett,
            &riff_client::api::PauseScope::Riff,
            riff_core::wire::RiffState::Running,
        )
        .await
        .unwrap();

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    let header = format!(" riff running · build {build} · link ok ");
    let release = |step| Release {
        repo: "como-technologies/riff".into(),
        tag: "v3.3.0".into(),
        step,
    };
    let steps: [(&[ReleaseStep], &[&str]); 4] = [
        (
            &[ReleaseStep::Merged],
            &[
                " v3.3.0: the release build passed; it waits for the real-session test of the",
                " lead of como-technologies/riff",
            ],
        ),
        (
            &[ReleaseStep::Merged, ReleaseStep::TestRuns],
            &[" v3.3.0: the real-session test runs"],
        ),
        (
            &[ReleaseStep::Tested],
            &[
                " v3.3.0: the real-session test passed; it waits for the cut of the lead of",
                " como-technologies/riff",
            ],
        ),
        // The watch of the wave read the cut; the watch of the deploy saw
        // the wait for the approve after it.
        (
            &[ReleaseStep::Published, ReleaseStep::Waits],
            &[" v3.3.0: the deploy waits for a person (riff release approve v3.3.0)"],
        ),
    ];
    for (seen, lines) in steps {
        let facts = Facts {
            build: riff_core::build::Build::this().to_string(),
            limit: 3,
            releases: seen.iter().copied().map(release).collect(),
            ..Facts::default()
        };
        api.machine(&daemon, &facts.line()).await.unwrap();
        tty.press("r").await;
        let rows = tty
            .shows(|rows| {
                rows[1].starts_with(&header)
                    && rows[2..2 + lines.len()] == *lines
                    && rows[2 + lines.len()].starts_with(" 1 session")
            })
            .await;
        assert!(rows[23].starts_with(" ↑↓ select"), "{seen:?}: {rows:#?}");
    }
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M75N6YNHQQHGS0VGKTKSNJTR: the header of the TUI at 80 x 24 says that
/// the current wave is complete and that its release waits for its cut, and
/// why, from the facts that the daemon of thelio sent to riff-server. When
/// the cut opens the release, the watch of the wave gives no complete wave to
/// the facts, and the rows go.
#[tokio::test(flavor = "multi_thread")]
async fn the_header_says_that_a_complete_wave_waits_for_its_cut() {
    use riff_client::daemons::{Complete, CutWaits, Facts};

    let server = crate::common::start_signed().await;
    let daemon: riff_core::name::SessionUri =
        "riff://brett@thelio/como-technologies/riff?session=d1"
            .parse()
            .unwrap();
    let api = server.api("brett", Some("d1"));
    api.register(&daemon).await.unwrap();
    let _watch = api.watch(&daemon).await.unwrap();
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    crate::common::api_of(&server, &brett)
        .set_pause(
            &brett,
            &riff_client::api::PauseScope::Riff,
            riff_core::wire::RiffState::Running,
        )
        .await
        .unwrap();

    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    let build: String = riff_core::build::Build::this().id.chars().take(7).collect();
    let header = format!(" riff running · build {build} · link ok ");
    let steps: [(Option<CutWaits>, &[&str]); 3] = [
        (
            Some(CutWaits::MainFailed),
            &[
                " Wave 41 is complete: the release waits for its cut; the full check of main",
                " failed",
            ],
        ),
        (
            Some(CutWaits::Lead),
            &[
                " Wave 41 is complete: the release waits for its cut by the lead of",
                " como-technologies/riff",
            ],
        ),
        // The cut made the release issue of the wave.
        (None, &[]),
    ];
    for (waits, lines) in steps {
        let facts = Facts {
            build: riff_core::build::Build::this().to_string(),
            limit: 3,
            complete: waits
                .map(|waits| Complete {
                    repo: "como-technologies/riff".into(),
                    wave: 41,
                    waits: Some(waits),
                })
                .into_iter()
                .collect(),
            ..Facts::default()
        };
        api.machine(&daemon, &facts.line()).await.unwrap();
        tty.press("r").await;
        let rows = tty
            .shows(|rows| {
                rows[1].starts_with(&header)
                    && rows[2..2 + lines.len()] == *lines
                    && rows[2 + lines.len()].starts_with(" 1 session")
            })
            .await;
        assert!(rows[23].starts_with(" ↑↓ select"), "{waits:?}: {rows:#?}");
    }
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
    // A new riff starts paused: the admin brett resumes it, as `riff
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
    // The tree is repository › person › session. The groups need no
    // look: each is one folded line that names its host, with the
    // counts.
    let rows = tty
        .shows(|rows| rows.iter().any(|row| row.starts_with(" ▶ strata › mike")))
        .await;
    assert!(rows[2].starts_with(" 2 sessions: 2 busy"), "{rows:#?}");
    assert!(rows[2].ends_with(" by repo"), "{rows:#?}");
    let riff = rows
        .iter()
        .find(|row| row.starts_with(" ▶ riff › mike"))
        .unwrap();
    assert!(riff.contains(" · pangolin "), "{rows:#?}");
    assert!(riff.ends_with("1 session · 1 busy"), "{rows:#?}");

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
    // The summary line starts with the filter.
    assert!(
        rows[2].starts_with(" filter #12 · 1 of 2 sessions: 1 busy · Esc clears"),
        "{rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("9c0d1e2f")),
        "{rows:#?}"
    );
    let row = rows.iter().find(|row| row.contains("5b1e2a90")).unwrap();
    assert!(row.contains("5b1e2a90  #12 "), "{rows:#?}");
    assert!(row.contains(" pangolin "), "{rows:#?}");
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
    tty.shows(|rows| rows[2].starts_with(" filter #12 · 1 of 2 sessions: 1 paused"))
        .await;
    assert!(!rows.is_empty());

    // Esc clears the filter, and `q` ends the TUI.
    tty.press("\x1b").await;
    tty.shows(|rows| rows[2].starts_with(" 2 sessions")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// A fake riff daemon of mike on pangolin in `o/r`, as the session
/// `d0d0d0d0` on the riff-server `server`: its status is the status of
/// a daemon, and its facts name the container `riff-issue-88-9c0d1e2f`.
/// It judges each request as a daemon does, keeps its body, and replies
/// with a direct note to mike, as a daemon replies. It gives the bodies.
async fn fake_daemon(server: &str) -> Arc<Mutex<Vec<String>>> {
    use riff::daemon::remote::{self, Request};
    use riff_client::daemons::{Facts, Session};

    let me: riff_core::name::SessionUri =
        "riff://mike@pangolin/o/r?session=d0d0d0d0".parse().unwrap();
    let api = crate::common::api_of(server, &me);
    api.register(&me).await.unwrap();
    let step = remote::status_line(812, "3.3.0", 1);
    api.status(&me, &riff_core::wire::Status { step })
        .await
        .unwrap();
    let facts = Facts {
        build: riff_core::build::Build::this().to_string(),
        sessions: vec![Session {
            name: "riff-issue-88-9c0d1e2f".into(),
            item: "issue-88".into(),
            phase: riff_client::sessions::Phase::Works,
            last: String::new(),
            digest: String::new(),
        }],
        ..Facts::default()
    };
    api.machine(&me, &facts.line()).await.unwrap();
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&got);
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&me).await.unwrap();
        open.send(()).unwrap();
        loop {
            let inbox = api.inbox(&me, None, false).await.unwrap_or_default();
            for checked in inbox.into_iter().flat_map(|i| i.messages) {
                let Some(judged) = remote::judge(&checked, &me) else {
                    continue;
                };
                let reply = match &judged {
                    Ok(Request::End(name)) => Ok(format!(
                        "{}\n{}",
                        riff::text::daemon_freed("issue-88"),
                        riff::text::daemon_ends(name)
                    )),
                    Ok(Request::Leave { repo, .. }) => Ok(riff::text::leave_stops(repo)),
                    Ok(_) => Ok(String::new()),
                    Err(refused) => Err(refused.clone()),
                };
                kept.lock().unwrap().push(match judged {
                    Ok(request) => request.to_string(),
                    Err(refused) => refused,
                });
                let body = remote::reply_body(checked.message.seq, "pangolin", &reply);
                let to = [riff_core::selector::Selector {
                    user: Some("mike".into()),
                    ..riff_core::selector::Selector::default()
                }];
                let kind = riff_core::wire::Kind::Note;
                api.post(&me, None, &to, &body, kind).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    opened.await.unwrap();
    got
}

/// 01M6WMAXEVZB2XJN4XPR879J6W: the TUI of mike on thelio acts on
/// pangolin through the signed requests of `--host`. `x` on a session of
/// pangolin asks first: `n` sends nothing, and `y` sends `daemon end
/// NAME` to the session of the daemon of pangolin, and the note line
/// shows its reply. `x` and `y` on the row of the daemon send `leave
/// REPO`, and the note says that its sessions go on. `t` there names
/// the step on pangolin.
#[tokio::test(flavor = "multi_thread")]
async fn x_on_a_session_of_another_host_asks_the_daemon_of_that_host() {
    let server = crate::common::start_signed().await;
    // A new riff starts paused: the admin brett resumes it. The clients
    // of mike come after this one.
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    crate::common::api_of(&server, &brett)
        .set_pause(
            &brett,
            &riff_client::api::PauseScope::Riff,
            riff_core::wire::RiffState::Running,
        )
        .await
        .unwrap();
    let got = fake_daemon(&server).await;
    session(
        &server,
        "riff://mike@pangolin/o/r?session=9c0d1e2f",
        "issue-88",
    )
    .await;
    let mut tty = Tty::start(&server, "mike", "thelio", true);
    tty.shows(|rows| rows.iter().any(|row| row.contains("pangolin")))
        .await;
    // The filter selects the worker of pangolin.
    tty.press("/").await;
    for key in ["9", "c", "0", "d"] {
        tty.press(key).await;
    }
    tty.press("\r").await;
    tty.shows(|rows| rows.iter().any(|row| row.starts_with(" > ● 9c0d1e2f")))
        .await;

    // `t`: the step on pangolin.
    tty.press("t").await;
    let step = riff::text::takeover_there("pangolin", Some("riff-issue-88-9c0d1e2f"));
    tty.shows(|rows| rows[22] == format!(" {step}")).await;

    // `x` asks first, and `n` sends nothing.
    tty.press("x").await;
    let ask = format!(
        " {} y/N",
        riff::text::ask_end_there("riff-issue-88-9c0d1e2f", "pangolin")
    );
    tty.shows(|rows| rows[22] == ask).await;
    tty.press("n").await;
    tty.shows(|rows| rows[22] == format!(" {}", riff::tui::NOTHING_CHANGED))
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(got.lock().unwrap().is_empty(), "{:?}", got.lock().unwrap());

    // `x` and `y`: the request goes to the daemon of pangolin, and the
    // note line shows its reply.
    tty.press("x").await;
    tty.shows(|rows| rows[22] == ask).await;
    tty.press("y").await;
    let reply = riff::text::host_replied(
        "pangolin",
        &format!(
            "{}\n{}",
            riff::text::daemon_freed("issue-88"),
            riff::text::daemon_ends("riff-issue-88-9c0d1e2f")
        ),
    );
    let shown: String = format!(" {reply}").chars().take(80).collect();
    tty.shows(|rows| rows[22] == shown.trim_end()).await;
    assert_eq!(*got.lock().unwrap(), ["daemon end riff-issue-88-9c0d1e2f"]);

    // The row of the daemon: `x` and `y` send `leave o/r`.
    tty.press("\x1b").await;
    tty.press("/").await;
    for key in ["d", "0", "d", "0"] {
        tty.press(key).await;
    }
    tty.press("\r").await;
    tty.shows(|rows| {
        rows.iter().any(|row| {
            row.starts_with(" > ") && row.contains("d0d0d0d0  ") && row.contains("daemon")
        })
    })
    .await;
    tty.press("x").await;
    tty.shows(|rows| rows[22] == " pangolin stops taking work for o/r (riff host leave): y/N")
        .await;
    tty.press("y").await;
    let left: String = format!(" {}", riff::text::host_left("pangolin", "o/r"))
        .chars()
        .take(80)
        .collect();
    tty.shows(|rows| rows[22] == left.trim_end()).await;
    assert_eq!(
        *got.lock().unwrap(),
        ["daemon end riff-issue-88-9c0d1e2f", "leave o/r"]
    );
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
    use riff_client::daemons::{Facts, Parts, RepoParts};
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
        // The hosts take work: they show no mark
        // (01M6WJYQN2WY5NH3DVDBAAY8ST).
        parts: vec![RepoParts {
            repo: "o/r".into(),
            parts: Parts {
                workers: true,
                lead: false,
            },
        }],
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
    // riff writes a frame in parts, from row 0 to row 23: each wait
    // names the last row that it reads (#1936).
    tty.shows(|rows| {
        rows[0].contains("[4 Machines]")
            && rows[2] == " 2 hosts"
            && rows[4].starts_with(" >   pangolin ")
            && rows[5].starts_with("     thelio ")
    })
    .await;

    // `/`, then a word: only pangolin shows.
    tty.press("/").await;
    tty.press("pang").await;
    let rows = tty
        .shows(|rows| {
            rows[2] == " 1 of 2 hosts · /pang"
                && rows[22] == " /pang█"
                && rows[23] == " Enter keep the filter  Esc clear it"
        })
        .await;
    assert!(rows[4].starts_with(" >   pangolin "), "{rows:#?}");
    assert_eq!(rows[5], "");
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
    tty.shows(|rows| {
        rows[2] == " 2 hosts"
            && rows[4].starts_with(" >   pangolin ")
            && rows[5].starts_with("     thelio ")
    })
    .await;
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
/// `Chat•N`, and Enter sends the line as the person, with the names that
/// it holds. `@riff` wakes the lead, and the line shows once. No line is
/// refused for a name.
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
        .shows(|rows| shown(rows, "<riff> are you there?") == 1)
        .await;
    assert_eq!(
        rows[0],
        " 1 Sessions  [2 Chat]  3 Waves  4 Machines  5 Devices  6 Reports"
    );
    assert!(rows[2].starts_with(" chat · online: brett"), "{rows:#?}");
    assert_eq!(shown(&rows, "<mike> hello brett"), 1, "{rows:#?}");
    assert_eq!(rows[21], ">");
    assert_eq!(
        rows[23],
        " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit"
    );

    // The note line says nothing of whom the line wakes. Enter sends the
    // line with the name of the project: the lead of brett in riff wakes.
    tty.press("@riff is #12 done?").await;
    tty.shows(|rows| rows[21] == "> @riff is #12 done?" && rows[22].is_empty())
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
        .shows(|rows| shown(rows, "<brett> @riff is #12 done?") == 1)
        .await;
    assert_eq!(rows[21], ">", "{rows:#?}");

    // No line is refused for a name: a line with no name goes with no
    // name, and a name that no one has goes with its name
    // (01M6Y1A5PTCVZ6GRJ0FXMART2T).
    tty.press("hello all\r").await;
    tty.press("@lead:strata hi\r").await;
    tty.shows(|rows| shown(rows, "<brett> @lead:strata hi") == 1)
        .await;
    let read = lead_api.read(&lead, &chat, true).await.unwrap();
    let to = |body: &str| -> Vec<String> {
        let line = read.iter().find(|c| c.message.body == body).unwrap();
        line.message.to.iter().map(ToString::to_string).collect()
    };
    assert_eq!(to("@riff is #12 done?"), ["name=riff"]);
    assert!(to("hello all").is_empty());
    assert_eq!(to("@lead:strata hi"), ["name=lead"]);

    // Esc leaves the prompt, and `q` ends the TUI.
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// `@` in the Chat tab shows the names of the chat from riff-server
/// (01M6Y1EKGKNW2MH970PDF6PJK1, 01M6Y1EKKCCZ5215V49Z5T3HKH,
/// 01M6Y1ES82XHFGANKZS7AF3CP2): `@r` shows `riff`, the project of the
/// lead of brett, over the prompt, and `Tab` completes it. Then Enter
/// sends the line with the name.
#[tokio::test]
async fn at_in_the_chat_tab_shows_the_names_and_tab_completes_one() {
    use riff_core::name::SessionUri;

    let server = crate::common::start_signed().await;
    let lead: SessionUri = "riff://brett@thelio/como-technologies/riff?session=brett-lead"
        .parse()
        .unwrap();
    crate::common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    crate::common::name_lead(&server.api("brett", None), &lead).await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press("@r").await;
    // `@` shows the same note: wait for the prompt too.
    let rows = tty
        .shows(|rows| {
            rows[21] == "> @r" && rows[22] == " Tab or Enter picks riff. Esc closes the list."
        })
        .await;
    assert_eq!(
        rows[20..22],
        ["  riff  project  online", "> @r"],
        "{rows:#?}"
    );
    assert_eq!(rows[23], " Tab pick  Esc close  Ctrl-C quit");
    tty.press("\t").await;
    let rows = tty.shows(|rows| rows[21] == "> @riff").await;
    assert_eq!(
        rows[0],
        " 1 Sessions  [2 Chat]  3 Waves  4 Machines  5 Devices  6 Reports"
    );
    assert_eq!(rows[20], "", "{rows:#?}");
    tty.press("is #12 done?\r").await;
    tty.shows(|rows| {
        rows.iter()
            .any(|r| r.ends_with("<brett> @riff is #12 done?"))
    })
    .await;
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// `#` in the Chat tab shows the open issues from `gh`
/// (01M74CX2V9YAF0KFPJDRM4DA83, 01M74CX9H7WT1T4EN70AVD2TGZ,
/// 01M74CX9KVGQG4Y7ATT6268NMY, 01M74CXE1703NASW3BWWA6ESGY): in a clone
/// of como-technologies/riff, `#186` shows #1862 and #1860 over the
/// prompt, the newest first, and `Tab` puts `#1862 ` in the line. Then
/// Enter sends the line. The read is the conditional read of the open
/// issues.
#[tokio::test]
async fn hash_in_the_chat_tab_shows_the_open_issues_and_tab_puts_one() {
    let server = crate::common::start_signed().await;
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), crate::common::ISSUES_GH);
    let mut tty = Tty::start_in(
        crate::common::clone_of("como-technologies/riff"),
        &server,
        "brett",
        "thelio",
        true,
        &[("PATH", crate::common::path_with(bin.path()))],
    );
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press("is #186").await;
    // `#18` shows the same rows and note: wait for the prompt too.
    let rows = tty
        .shows(|rows| {
            rows[21] == "> is #186" && rows[22] == " Tab or Enter picks #1862. Esc closes the list."
        })
        .await;
    assert_eq!(
        rows[19..22],
        [
            "  #1862  riff  Projects A: riff-server keeps the projects of the riff",
            "  #1860  riff  Pick an issue with #",
            "> is #186",
        ],
        "{rows:#?}"
    );
    assert_eq!(rows[23], " Tab pick  Esc close  Ctrl-C quit");
    tty.press("\t").await;
    let rows = tty.shows(|rows| rows[21] == "> is #1862").await;
    assert_eq!(rows[20], "", "{rows:#?}");
    let span = Span::start();
    while tty.cursor() != (21, 11, true) {
        assert!(span.within(WAIT), "the cursor in time: {:?}", tty.cursor());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tty.press("done?\r").await;
    tty.shows(|rows| rows.iter().any(|r| r.ends_with("<brett> is #1862 done?")))
        .await;
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("api -i repos/como-technologies/riff/issues?state=open"),
        "{calls}"
    );
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// The TUI turns on the bracketed paste of the terminal. A paste of
/// three lines in the Chat tab goes into the prompt, and Enter sends
/// the three lines as one message: the log shows one sender, and each
/// more line under it. The end of the TUI turns the bracketed paste off.
#[tokio::test]
async fn a_paste_of_three_lines_in_the_chat_tab_sends_one_message() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    assert!(tty.bracketed_paste());
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    // A terminal gives each line end of a paste as `\r`.
    tty.press("\x1b[200~one\rtwo\rthree\x1b[201~").await;
    tty.shows(|rows| rows[19..22] == ["> one↵", "  two↵", "  three"])
        .await;
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| rows.iter().any(|r| r.ends_with("<brett> one")))
        .await;
    let at = rows
        .iter()
        .position(|r| r.ends_with("<brett> one"))
        .unwrap();
    assert_eq!(
        rows[at + 1..at + 3],
        ["       two", "       three"],
        "{rows:#?}"
    );
    assert_eq!(rows[21], ">", "{rows:#?}");
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
    let span = Span::start();
    while tty.bracketed_paste() {
        assert!(span.within(WAIT), "the bracketed paste is off in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 01M6Z8AHEHCPR1A3Z3CEAN59AR: while the log of the Chat tab is
/// scrolled up, a new line stays under the window, and the row over the
/// prompt says `1 new line below: End`. `End` in the empty prompt shows
/// the line, and the row goes.
#[tokio::test]
async fn a_new_line_under_the_window_of_the_chat_tab_gives_a_row_and_end_shows_it() {
    use riff_core::name::SessionUri;
    use riff_core::wire::Kind;

    let server = crate::common::start_signed().await;
    let chat = riff::chat::thread();
    // 20 lines of mike before the TUI starts: more rows than the log has.
    let mike: SessionUri = "riff://mike@heron".parse().unwrap();
    let mike_api = server.api("mike", None);
    mike_api.join(&mike, &chat).await.unwrap();
    for n in 0..20 {
        mike_api
            .post(&mike, Some(&chat), &[], &format!("line {n}"), Kind::Message)
            .await
            .unwrap();
    }
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[20].ends_with("<mike> line 19") && rows[21] == ">")
        .await;
    // PgUp: the log shows older lines.
    tty.press("\x1b[5~").await;
    tty.shows(|rows| rows[20].ends_with("<mike> line 16")).await;

    mike_api
        .post(&mike, Some(&chat), &[], "a new line", Kind::Message)
        .await
        .unwrap();
    let rows = tty.shows(|rows| rows[20] == " 1 new line below: End").await;
    assert!(rows[19].ends_with("<mike> line 16"), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.ends_with("a new line")), "{rows:#?}");

    // End shows the line, and the row goes.
    tty.press("\x1b[F").await;
    let rows = tty
        .shows(|rows| rows[20].ends_with("<mike> a new line"))
        .await;
    assert!(rows[19].ends_with("<mike> line 19"), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.contains("below")), "{rows:#?}");
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// The Chat tab keeps the line when the window of the terminal loses the
/// focus and gets it back (#1762, 01M6XT6MCZ38XEN7ZHWTRF50Z5). The TUI
/// asks the terminal for focus events. After `Esc`, the loss of the
/// focus (`CSI O`) and its return (`CSI I`), and after a click on the
/// log, the keys go into the line, and the cursor shows at the end of
/// the text. At its end, the TUI turns the focus events off.
#[tokio::test]
async fn the_chat_tab_keeps_the_line_when_the_window_loses_and_gets_back_the_focus() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.wrote("\x1b[?1004h").await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press("hi").await;
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("\x1b[O").await;
    tty.press("\x1b[I").await;
    tty.shows(|rows| rows[23].starts_with(" Enter send")).await;
    // A click on the log, as the click that gives the window the focus.
    tty.press("\x1b[<0;11;6M\x1b[<0;11;6m").await;
    tty.press(" all").await;
    tty.shows(|rows| rows[21] == "> hi all" && tty.cursor() == (21, 8, true))
        .await;
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
    tty.wrote("\x1b[?1004l").await;
}

/// A line of the Chat tab has at most 64 KiB, also when the person
/// types: after a paste of 64 KiB, a key types nothing, and the note
/// line says the limit until the next key. A Backspace then works, and
/// a key types again.
#[tokio::test]
async fn a_key_over_the_limit_of_the_chat_line_types_nothing() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    let full = "x".repeat(riff::tui::chat::POST_MAX);
    tty.press(&format!("\x1b[200~{full}\x1b[201~")).await;
    tty.shows(|rows| rows[21].ends_with('x') && rows[22].is_empty())
        .await;
    let too_long = format!(" {}", riff::tui::chat::too_long());
    tty.press("y").await;
    let rows = tty.shows(|rows| rows[22] == too_long).await;
    assert!(rows[21].ends_with('x'), "{rows:#?}");
    // Backspace deletes a character, and the note goes.
    tty.press("\x7f").await;
    tty.shows(|rows| rows[22].is_empty()).await;
    tty.press("y").await;
    let rows = tty.shows(|rows| rows[21].ends_with('y')).await;
    assert!(rows[22].is_empty(), "{rows:#?}");
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6XXDDYQG7M6WYNRXP35DZ1Y: a long line in the Chat tab wraps over
/// more rows of the terminal (#1777). The prompt grows up, the cursor is
/// at the end of the line, and `Home` and `↓` move it through the rows.
#[tokio::test]
async fn a_long_line_in_the_chat_tab_wraps_over_more_rows() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    tty.press(
        "typing in the TUI chat does not wrap lines, they just scroll off the screen \
         to the left. Now the line wraps.",
    )
    .await;
    tty.shows(|rows| {
        rows[20..22]
            == [
                "> typing in the TUI chat does not wrap lines, they just scroll off the screen",
                "  to the left. Now the line wraps.",
            ]
            && tty.cursor() == (21, 34, true)
    })
    .await;
    // Home, then ↓: the second row, at the same column.
    tty.press("\x1b[H").await;
    tty.shows(|_| tty.cursor() == (20, 2, true)).await;
    tty.press("\x1b[B").await;
    tty.shows(|_| tty.cursor() == (21, 2, true)).await;
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6XTQHMY4VS04YGWY17BSTM8, the cause of #1763: Enter on a command
/// that the chat does not know sends nothing, and the line stays. The
/// row under the prompt says why, and the note goes with the line: no
/// old note shows under an empty prompt, and not in another tab.
#[tokio::test]
async fn the_note_of_an_unknown_command_goes_with_the_line() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    let unknown = format!(" {}", riff::tui::chat::unknown("foo"));
    tty.press("/foo").await;
    tty.shows(|rows| rows[22] == unknown).await;
    tty.press("\r").await;
    let rows = tty.shows(|rows| rows[21] == "> /foo").await;
    assert_eq!(rows[22], unknown, "{rows:#?}");

    // Ctrl-U empties the prompt: the row under it is empty.
    tty.press("\x15").await;
    let rows = tty.shows(|rows| rows[21] == ">").await;
    assert_eq!(rows[22], "", "{rows:#?}");
    // Another tab has no old note either.
    tty.press("\x1b").await;
    tty.shows(|rows| rows[23].starts_with(" i type")).await;
    tty.press("1").await;
    let rows = tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    assert!(!rows[22].contains("Unknown command"), "{rows:#?}");
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// 01M6Y8N8W5RE5H6MW3TNDH5BSY: with a typed text, `↑` in the Chat tab
/// keeps the text and goes to its start (#1788). On an empty line, `↑`
/// brings back the last sent line, and `↓` empties the line again.
#[tokio::test]
async fn up_in_the_chat_tab_keeps_the_typed_text_and_brings_back_a_sent_line_on_an_empty_line() {
    let server = crate::common::start_signed().await;
    let mut tty = Tty::start(&server, "brett", "thelio", true);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    tty.press("2").await;
    tty.shows(|rows| rows[21] == ">").await;
    let shown = |rows: &[String], text: &str| rows.iter().filter(|r| r.ends_with(text)).count();
    tty.press("the first line\r").await;
    tty.shows(|rows| shown(rows, "<brett> the first line") == 1 && rows[21] == ">")
        .await;

    tty.press("my words").await;
    tty.press("\x1b[A").await;
    tty.shows(|rows| rows[21] == "> my words" && tty.cursor() == (21, 2, true))
        .await;
    tty.press("Note: ").await;
    tty.shows(|rows| rows[21] == "> Note: my words").await;
    tty.press("\r").await;
    tty.shows(|rows| shown(rows, "<brett> Note: my words") == 1 && rows[21] == ">")
        .await;

    tty.press("\x1b[A").await;
    tty.shows(|rows| rows[21] == "> Note: my words").await;
    tty.press("\x1b[A").await;
    tty.shows(|rows| rows[21] == "> the first line").await;
    tty.press("\x1b[B").await;
    tty.shows(|rows| rows[21] == "> Note: my words").await;
    tty.press("\x1b[B").await;
    tty.shows(|rows| rows[21] == ">" && tty.cursor() == (21, 2, true))
        .await;
    tty.press("\x03").await;
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
    printf '{"number":13,"title":"Hold this one","milestone":{"number":5,"title":"Wave 3"}},'
    printf '{"number":14,"title":"Which name?","milestone":{"number":6,"title":"Wave 4"},"labels":[{"name":"question"}]}]' ;;
  *'issues?state=closed'*) printf 'HTTP/2.0 200 OK\r\n\r\n[]' ;;
  'api repos/acme/app') echo '{"default_branch":"main"}' ;;
  *) echo '[]' ;;
esac
"#;

/// 01M6VFG882T794B1CTHK5AKPDG, 01M6VFGK9XXWCA8X7CE8EZNSJV,
/// 01M6VFGK4P2FWMFQHBZKR3JXSZ, 01M6Y0ZQWDYVC23WA5CVN0QSH8: in a clone of
/// acme/app, the Waves tab shows the board of its current wave from `gh`,
/// with the question of a later wave, and holds, frees and pauses with
/// the code of `riff plan hold`, `riff plan free` and `riff pause`, each
/// after `y`.
#[tokio::test]
async fn the_waves_tab_shows_the_board_and_holds_frees_and_pauses() {
    // acme/app is a project of the riff (01M77F7R7RERVQH7XS39GWJJKF).
    let server = crate::common::start_signed_log(vec![crate::common::project("acme/app")]).await;
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), FAKE_GH);
    let mut tty = Tty::start_in(
        crate::common::clone_of("acme/app"),
        &server,
        "brett",
        "thelio",
        true,
        &[("PATH", crate::common::path_with(bin.path()))],
    );
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
    // 01M6Y0ZQWDYVC23WA5CVN0QSH8: the question of Wave 4, a wave that is
    // not current.
    assert_eq!(
        rows[6], "   ?   asked   #14    Wave 4 · Which name?            waits for a person   -",
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

/// A fake `gh` whose read of the open issues is slow: it starts, writes
/// the file `reading` in its folder, and answers only when the file `go`
/// is there, or after 2 minutes. It logs each call to the file `calls`.
const SLOW_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  *'issues?state=open'*)
    touch "$dir/reading"
    n=0
    while [ ! -e "$dir/go" ] && [ "$n" -lt 120 ]; do sleep 1; n=$((n + 1)); done
    printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[]' ;;
  *) echo '[]' ;;
esac
"#;

/// 01M6VGHCDTDJ69B7C02B5K61PW, 01M6VT7TXBW8EJKY0FTYQ0H49M,
/// 01M6VT7TTV8J90C5E3QAZ454GY, #1707: with a live session in acme/app,
/// the Sessions tab reads the titles of its issues with the conditional
/// read of the open issues, not with `gh issue list`. `q` while that
/// read runs and does not answer ends the TUI within 1 second, with exit
/// 0, and gives back the screen: the main screen, and the terminal mode
/// of a shell.
#[tokio::test]
async fn q_ends_the_tui_at_once_while_a_slow_read_of_gh_runs() {
    use nix::sys::termios::{LocalFlags, tcgetattr};

    let server = crate::common::start_signed().await;
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
        "riff://mike@pangolin/acme/app?session=5b1e2a90",
        "issue-12",
    )
    .await;
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), SLOW_GH);
    let mut tty = Tty::start_in(
        tempfile::tempdir().unwrap(),
        &server,
        "brett",
        "thelio",
        true,
        &[("PATH", crate::common::path_with(bin.path()))],
    );
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    let span = Span::start();
    while !bin.path().join("reading").exists() {
        assert!(span.within(WAIT), "the read of the titles starts in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let span = Span::start();
    tty.press("q").await;
    let code = tty.exits().await;
    let ended = span.wall();
    std::fs::write(bin.path().join("go"), "").unwrap();
    assert_eq!(code, Some(0));
    assert!(
        span.within(Duration::from_secs(1)),
        "the TUI ended in {ended:?}"
    );
    tty.shows(|rows| !rows.iter().any(|row| row.contains("Sessions")))
        .await;
    let mode = tcgetattr(&tty.master).unwrap();
    assert!(
        mode.local_flags
            .contains(LocalFlags::ICANON | LocalFlags::ECHO),
        "{mode:?}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("api -i repos/acme/app/issues?state=open&per_page=100&page=1"),
        "{calls}"
    );
    assert!(!calls.contains("issue list"), "{calls}");
}

/// 01M6VMACJGF4A0ZXC6YK13KNCH: a post of the Chat tab that fails while
/// the prompt holds a line shows `cannot send the line` in the note line
/// at once, as a warning. After the next key, it stays as the result of
/// the last line, with no color. The server
/// holds the post of the line `this line fails` until the prompt shows
/// each key of the next line, then it refuses the post: a key that the
/// TUI reads after the refusal ends the note (01M6WH1FN4N3DDFY6FR1CH1M7X).
#[tokio::test]
async fn a_post_that_fails_while_the_prompt_holds_a_line_shows_until_the_next_key() {
    use axum::body::{Body, to_bytes};
    use axum::extract::Request;
    use axum::http::StatusCode;
    use axum::middleware::{Next, from_fn};
    use axum::response::Response;
    use tokio::sync::Notify;

    // `held`: the server holds the post. `go`: the server refuses it.
    let (held, go) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let (hold, gate) = (Arc::clone(&held), Arc::clone(&go));
    let server = crate::common::start_signed_with(move |router| {
        router.layer(from_fn(move |request: Request, next: Next| {
            let (hold, gate) = (Arc::clone(&hold), Arc::clone(&gate));
            async move {
                let (head, body) = request.into_parts();
                let bytes = to_bytes(body, usize::MAX).await.unwrap();
                let fails = head.uri.path() == "/v1/post"
                    && serde_json::from_slice::<serde_json::Value>(&bytes)
                        .is_ok_and(|post| post["body"] == "this line fails");
                if fails {
                    hold.notify_one();
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
    in_time(WAIT, held.notified())
        .await
        .expect("the server holds the post in time");
    tty.press("hi all").await;
    tty.shows(|rows| rows[21] == "> hi all" && rows[22].is_empty())
        .await;

    // The post fails while the prompt holds a line: the failure shows,
    // as a warning.
    go.notify_one();
    let failed = " riff: cannot send the line: post failed (400 Bad Request): no post now";
    tty.shows(|rows| rows[21] == "> hi all" && rows[22] == failed)
        .await;
    assert_ne!(tty.color_at(22), vt100::Color::Default);

    // The next key ends the warning: the failure stays as the result of
    // the last line, with no color.
    tty.press("!").await;
    tty.shows(|rows| rows[21] == "> hi all!" && rows[22] == failed)
        .await;
    assert_eq!(tty.color_at(22), vt100::Color::Default);
    tty.press("\x03").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// A fake `gh` for the Reports tab in the clone of acme/app: the waves
/// 2 (closed) and 3, the issues 12 and 13 of Wave 3, the comment of a
/// claim of #12 from the file `comments-12`, and no pull request. It
/// logs each call to the file `calls` of its folder.
const REPORTS_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  'api repos/acme/app/milestones?state=all&per_page=100')
    echo '[{"title":"Wave 2","created_at":"2026-10-01T00:00:00Z","closed_at":"2026-10-31T00:00:00Z"},'
    echo '{"title":"Wave 3","created_at":"2026-11-01T00:00:00Z","closed_at":null}]' ;;
  'issue list --repo acme/app --milestone Wave 3 '*)
    echo '[{"number":12,"title":"Show the board"},{"number":13,"title":"Hold this one"}]' ;;
  'issue list --repo acme/app --state all '*)
    echo '[{"number":12,"milestone":{"title":"Wave 3"},"body":"","closedAt":null,"comments":[]}]' ;;
  'api --paginate repos/acme/app/issues/12/comments '*) cat "$dir/comments-12" ;;
  'api --paginate '*) ;;
  *) echo '[]' ;;
esac
"#;

/// 01M6VT3JFEKD2FEQGNGGKHC8X5, 01M6VT3JHVFQ27VJE8987WET34,
/// 01M6VT3JMABMEXSBPE414MBJZP, 01M6VT3JPQXRWMX9PWYADT4MJX: in a clone of
/// acme/app, the Reports tab shows the usage of the current wave from
/// `gh` with the code of `riff usage --wave N`, the claims of an issue
/// on `Enter`, the audit of the wave on `Enter` in the Audit report, and
/// a Session log with no session.
#[tokio::test]
async fn the_reports_tab_shows_the_usage_the_audit_and_the_log() {
    use riff::usage::{Kind, Report, Tokens};

    let server = crate::common::start_signed().await;
    let clone = crate::common::clone_of("acme/app");
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), REPORTS_GH);
    let claim = Report {
        item: "issue-12".into(),
        kind: Kind::Work,
        session: "a6cf0123".into(),
        from_ms: 1_790_931_600_000,
        to_ms: 1_790_935_200_000,
        models: [(
            "claude-opus-5-5".to_owned(),
            Tokens {
                output: 1_940_000,
                ..Tokens::default()
            },
        )]
        .into(),
    };
    let comment = serde_json::json!({ "id": 1, "login": "brett", "body": claim.comment() });
    std::fs::write(bin.path().join("comments-12"), format!("{comment}\n")).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut tty = Tty::start_in(clone, &server, "brett", "thelio", true, &[("PATH", path)]);
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;

    // `6` opens the Reports tab: the usage of Wave 3, the open wave.
    tty.press("6").await;
    let rows = tty
        .shows(|rows| rows.iter().any(|r| r.contains("#13")))
        .await;
    assert_eq!(rows[2], " [Usage]  Audit  Session log", "{rows:#?}");
    assert_eq!(
        rows[3],
        " USAGE OF WAVE 3 · acme/app · 1.9M tokens in 2 issues"
    );
    assert_eq!(
        rows[5],
        " > #12    Show the board                   1.9M    opus 5.5"
    );
    assert_eq!(
        rows[6],
        "   #13    Hold this one                    0       -"
    );
    assert!(rows[22].starts_with(" Read from the comments of the issues with gh"));
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("issue list --repo acme/app --milestone Wave 3 --state all"),
        "{calls}"
    );

    // Enter shows the claims of #12, and Esc goes back.
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| rows[3] == " CLAIMS OF #12 · Show the board")
        .await;
    assert!(rows[4].starts_with(" #12: 1,940,000 tokens"), "{rows:#?}");
    tty.press("\x1b").await;
    tty.shows(|rows| rows[5].starts_with(" > #12")).await;

    // `→` shows the audit: it runs only on Enter.
    tty.press("\x1b[C").await;
    let rows = tty
        .shows(|rows| rows[2] == " Usage  [Audit]  Session log")
        .await;
    assert!(rows[4].starts_with(" The audit reads the log"), "{rows:#?}");
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(!calls.contains("pr list"), "no audit yet: {calls}");
    tty.press("\r").await;
    let rows = tty
        .shows(|rows| rows.iter().any(|r| r.starts_with(" Rule 1: ")))
        .await;
    assert!(
        rows[4].starts_with(" Audit of Wave 3 in acme/app, from 2026-11-01"),
        "{rows:#?}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(calls.contains("pr list --repo acme/app"), "{calls}");

    // `→` shows the Session log, with no session yet.
    tty.press("\x1b[C").await;
    let rows = tty
        .shows(|rows| rows[2] == " Usage  Audit  [Session log]")
        .await;
    assert_eq!(rows[3], " SESSION LOG");
    assert_eq!(rows[4], format!(" {}", riff::tui::reports::NO_LOG));

    tty.press("q").await;
    assert_eq!(tty.exits().await, Some(0));
}

/// A fake `gh` whose read of the milestones does not answer until the
/// file `go` of its folder is there. It makes the file `reading` when
/// that read starts.
const SLOW_WAVES_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  *milestones*)
    touch "$dir/reading"
    n=0
    while [ ! -e "$dir/go" ] && [ "$n" -lt 120 ]; do sleep 1; n=$((n + 1)); done
    echo '[]' ;;
  *) echo '[]' ;;
esac
"#;

/// 01M6VT7TTV8J90C5E3QAZ454GY, 01M6VT3JHVFQ27VJE8987WET34: in a clone of
/// acme/app, `q` while the Usage report reads the waves from a `gh` that
/// does not answer ends the TUI within 1 second, with exit 0.
#[tokio::test]
async fn q_ends_the_tui_at_once_while_the_reports_tab_reads_gh() {
    let server = crate::common::start_signed().await;
    let clone = crate::common::clone_of("acme/app");
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), SLOW_WAVES_GH);
    let mut tty = Tty::start_in(
        clone,
        &server,
        "brett",
        "thelio",
        true,
        &[("PATH", crate::common::path_with(bin.path()))],
    );
    tty.shows(|rows| rows[0].starts_with("[1 Sessions]")).await;
    assert!(
        !bin.path().join("reading").exists(),
        "no read of the waves yet"
    );
    tty.press("6").await;
    let span = Span::start();
    while !bin.path().join("reading").exists() {
        assert!(span.within(WAIT), "the read of the usage starts in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let span = Span::start();
    tty.press("q").await;
    let code = tty.exits().await;
    let ended = span.wall();
    std::fs::write(bin.path().join("go"), "").unwrap();
    assert_eq!(code, Some(0));
    assert!(
        span.within(Duration::from_secs(1)),
        "the TUI ended in {ended:?}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("api repos/acme/app/milestones?state=all&per_page=100"),
        "{calls}"
    );
}

/// A fake `gh` for acme/app: the waves 2 (closed), `3: Boards` and 4,
/// the issues 12 and 13 of Wave 3, and the comments of each issue from
/// the file `comments-N` of its folder.
const USAGE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$*" in
  'api repos/acme/app/milestones?state=all&per_page=100')
    echo '[{"title":"Wave 2","closed_at":"2026-10-01T00:00:00Z"},{"title":"Wave 3: Boards","closed_at":null},{"title":"Wave 4","closed_at":null},{"title":"Backlog","closed_at":null}]' ;;
  'issue list --repo acme/app --milestone Wave 3: Boards '*)
    echo '[{"number":13,"title":"Hold this one"},{"number":12,"title":"Show the board"}]' ;;
  'api --paginate repos/acme/app/issues/12/comments '*) cat "$dir/comments-12" ;;
  'api --paginate repos/acme/app/issues/13/comments '*) ;;
  *) echo "the fake gh does not know: $*" >&2; exit 1 ;;
esac
"#;

/// 01M6VT3JHVFQ27VJE8987WET34: the Usage report reads with the code of
/// `riff usage --wave N`: the current wave is the open wave with the
/// lowest number, the issues come in number order, and the tokens come
/// from the comments of each issue.
#[test]
fn the_usage_report_reads_what_riff_usage_reads() {
    use riff::tui::reports::{Wave, read_usage};
    use riff::usage::{Kind, Report, Tokens};

    let dir = tempfile::tempdir().unwrap();
    let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), USAGE_GH));
    let claim = Report {
        item: "issue-12".into(),
        kind: Kind::Work,
        session: "a6cf0123".into(),
        from_ms: 1_790_931_600_000,
        to_ms: 1_790_935_200_000,
        models: [(
            "claude-opus-5-5".to_owned(),
            Tokens {
                output: 1_500,
                ..Tokens::default()
            },
        )]
        .into(),
    };
    let comment = serde_json::json!({ "id": 1, "login": "mike", "body": claim.comment() });
    std::fs::write(dir.path().join("comments-12"), format!("{comment}\n")).unwrap();

    let usage = read_usage(&gh, "acme/app", None).unwrap();
    assert_eq!(
        usage.wave,
        Wave {
            number: 3,
            title: "Wave 3: Boards".into(),
            waves: vec![2, 3, 4],
        }
    );
    let numbers: Vec<u64> = usage.issues.iter().map(|i| i.number).collect();
    assert_eq!(numbers, [12, 13]);
    assert_eq!(usage.issues[0].tokens().output, 1_500);
    assert_eq!(usage.issues[0].models(), ["claude-opus-5-5"]);
    assert_eq!(usage.issues[1].tokens().total(), 0);
    assert_eq!(usage.tokens().output, 1_500);
    // The same wave by its number.
    assert_eq!(read_usage(&gh, "acme/app", Some(3)).unwrap(), usage);
    // A wave that the repository does not have names the open waves.
    let error = read_usage(&gh, "acme/app", Some(9))
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        riff::text::waves_no_such("9", &["Wave 3: Boards", "Wave 4"])
    );
}
