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
    _dir: tempfile::TempDir,
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
            _dir: dir,
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
