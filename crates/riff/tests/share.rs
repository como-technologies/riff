//! The member commands of a riff of peers at the CLI (#843): `riff
//! invite USER`, `riff join STRING`, `riff leave`, `riff remove USER` and
//! `riff members`, with the real `riff daemon` of this build
//! (01M519Z3SGEHVMS85HZD169CAP, 01M519Z4823WC7D3VVTFSDYBS4,
//! 01M4Z60B8JEX15M1Q2435FZSDM, 01M521J6QBQFRDWRPR0NGHE7TM).
//!
//! The daemons of a test do not find each other: the test of a join of
//! two daemons over iroh is in `riff::daemon::peers`.

use std::io::Read as _;
use std::process::{Command, ExitStatus};

use riff::peer::invite::{FILE, Invite, PREFIX};
use riff::text;

use crate::daemon::{Here, stop};

/// Runs `cmd` with a new pseudo-terminal as its standard input, output
/// and error: as a person in a terminal. It gives the exit status and
/// the output, with `\n` at the end of each line.
async fn in_terminal(mut cmd: Command) -> (ExitStatus, String) {
    tokio::task::spawn_blocking(move || {
        let pty = nix::pty::openpty(None, None).unwrap();
        let slave = std::fs::File::from(pty.slave);
        let mut child = cmd
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .spawn()
            .unwrap();
        // The read ends when each copy of the slave is closed.
        drop(cmd);
        let mut master = std::fs::File::from(pty.master);
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n @ 1..) = master.read(&mut buf) {
            out.extend_from_slice(&buf[..n]);
        }
        let status = child.wait().unwrap();
        let shown = String::from_utf8_lossy(&out).replace("\r\n", "\n");
        (status, shown)
    })
    .await
    .unwrap()
}

/// 01M4Z60B8JEX15M1Q2435FZSDM, 01M521J6QBQFRDWRPR0NGHE7TM: each member
/// command that changes the riff refuses with no terminal: an agent
/// session runs a command with no terminal. It asks no daemon.
#[tokio::test]
async fn the_member_commands_of_a_riff_of_peers_refuse_with_no_terminal() {
    let here = Here::new();
    let cases: [(&[&str], &str); 5] = [
        (&["invite", "brett"], "riff invite"),
        (&["remove", "brett"], "riff remove"),
        (&["remove", "brett", "--host", "kadomony"], "riff remove"),
        (&["join", "riff1-abc"], "riff join"),
        (&["leave"], "riff leave"),
    ];
    for (args, command) in cases {
        let out = here.run(false, args).await;
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}");
        assert!(
            stderr.contains(&text::share_no_terminal(command)),
            "{args:?}: {stderr}"
        );
    }
    assert!(!here.daemon_dir().join(FILE).exists());
}

/// 01M519Z3SGEHVMS85HZD169CAP, 01M519Z4823WC7D3VVTFSDYBS4: riff invite
/// in a terminal prints a string of the riff of the daemon, for the user,
/// and the line that the person runs. The daemon keeps no secret in its
/// folder. riff members needs no terminal, and shows the owner. The steps
/// that cannot go on here say why.
#[tokio::test]
async fn riff_invite_in_a_terminal_prints_the_string_and_riff_members_shows_the_owner() {
    let here = Here::new();
    let daemon = here.daemon("daemon.out").await;
    let key = here.status_line("Its key: ").await["Its key: ".len()..].to_owned();

    let (status, shown) = in_terminal(here.command(false, &["invite", "brett"])).await;
    assert!(status.success(), "{shown}");
    let string = shown
        .lines()
        .find(|line| line.starts_with(PREFIX))
        .unwrap_or_else(|| panic!("no invite string in\n{shown}"));
    let invite = Invite::parse(string).unwrap();
    assert_eq!(invite.riff.to_string(), key);
    assert_eq!(invite.user, "brett");
    assert!(
        shown.contains(&format!("\nriff join {string}\n")),
        "{shown}"
    );
    assert!(
        shown.contains("It works one time, until "),
        "{shown}"
    );
    let file = std::fs::read(here.daemon_dir().join(FILE)).unwrap();
    let secret = invite.secret();
    assert!(!file.windows(32).any(|w| w == secret.bytes()));
    assert!(!file.windows(64).any(|w| w == secret.hex().as_bytes()));
    assert!(!String::from_utf8_lossy(&file).contains(&string[PREFIX.len()..]));

    // riff members needs no terminal.
    let out = here.run(false, &["members"]).await;
    let members = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{members}");
    let user = std::env::var("USER").unwrap().to_lowercase();
    assert!(
        members.starts_with(&format!("The riff {} of {user}:\n", &key[..8])),
        "{members}"
    );
    let row = members
        .lines()
        .skip(2)
        .find(|line| line.contains(&key[..8]))
        .unwrap();
    assert!(row.ends_with("owner, this machine"), "{members}");

    // The other steps say why they cannot go on.
    let refused = [
        (&["remove", "brett"][..], text::remove_no_key("brett", None)),
        (&["leave"], text::LEAVE_OWNER.to_owned()),
        (&["join", string], text::JOIN_OWN_RIFF.to_owned()),
        (&["join", "riff1-abc"], text::INVITE_NOT_A_STRING.to_owned()),
    ];
    for (args, why) in refused {
        let (status, shown) = in_terminal(here.command(false, args)).await;
        assert!(!status.success(), "{args:?}: {shown}");
        assert!(shown.contains(&why), "{args:?}: {shown}");
    }
    stop(daemon).await;
}
