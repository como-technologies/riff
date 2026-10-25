//! The riff daemon (#716, 01M4Z60B7FCREZDCTVR73WWRY1): it owns the
//! sessions of riff on this machine. A systemd user unit runs it
//! ([`crate::service`]).
//!
//! # Design
//!
//! The daemon starts and stops each session through a [`Runtime`]: on
//! Linux, rootless podman. It gives each session a Unix socket of its
//! own (01M4Z60B7W6F3CC5XJE0VS7Y86). `riff` talks to the daemon on the
//! control socket. In each container, `riff relay` talks to the daemon
//! on the socket of its session ([`crate::session_relay`]).
//!
//! ```mermaid
//! flowchart LR
//!     R["riff run issue-12"] -- "control socket" --> D["riff daemon<br/>systemd user unit"]
//!     D -- "Runtime: podman" --> C1
//!     D -- "Runtime: podman" --> C2
//!     subgraph C1["container riff-issue-12-ID8"]
//!         L1["riff relay"] --> K1["claude -p"]
//!         S1["/run/riff/riff.sock"]
//!     end
//!     subgraph C2["container riff-issue-9-ID8"]
//!         L2["riff relay"] --> K2["claude -p"]
//!         S2["/run/riff/riff.sock"]
//!     end
//!     L1 -- "its own socket" --> D
//!     L2 -- "its own socket" --> D
//! ```
//!
//! # The folder of the daemon
//!
//! The daemon keeps its sockets in [`dir`]: `daemon` in the data folder
//! of riff ([`crate::run::data_dir`]), with mode 0700
//! (01M4ZDTVQEHD87WD5TSN93XFSS).
//!
//! `RIFF_DATA` ([`crate::run::DATA_VAR`]) moves the data folder
//! (01M53YP97S9BDMFBN0X5G444MY). The daemon of the real-session test runs
//! there, beside the daemon of the machine: with its own socket and its
//! own session folders. It never acts as the daemon of the unit
//! ([`crate::run::machine_data`], 01M53YP9AHPZ9YPNQFJ1SYZGWF).
//!
//! | Path | Holds |
//! |---|---|
//! | `riff.sock` | The control socket: `riff run`, `riff daemon status`. |
//! | `sessions/NAME/` | The folder of the socket of the session `NAME`, with mode 0700. |
//! | `sessions/NAME/riff.sock` | The socket of that session. |
//! | `live/NAME.json` | The [`Record`] of the session `NAME`, with no secret. |
//! | `limit.json` | The limit of the host ([`crate::room`]), when `riff daemon limit` set it. |
//! | `waves.json` | The waves that the daemon runs ([`crate::wave`]). |
//! | `watches.json` | The async operations that the daemon watches ([`crate::watches`]). |
//!
//! The old riff of peers kept its state in this folder too: `state/`,
//! each `state.lost-MS/`, `invites.json` and `forges.json`. The daemon
//! does not read, change or delete them (01M53NGWFBWR1WMJWC2DHVHJ3T).
//!
//! The daemon binds the folder of a session at [`IN_SESSION`] in its
//! container, with write, and sets [`SOCKET_VAR`]
//! (01M4ZDTVSP0T1YENKAWGKA114D). Its listener tells the daemon which
//! session asks: one listener for each session. No other session has
//! that bind. The daemon refuses a spec that binds its folder, a folder
//! above it, or a path in it ([`Daemon::check`]). So a session cannot
//! reach the socket of another session, or the control socket. The
//! folder of a session goes at the end of the session.
//!
//! A path of a socket can be longer than the address of a Unix socket
//! takes (108 bytes). On Linux, riff binds and connects through a
//! descriptor of the folder: `/proc/self/fd/N/riff.sock`.
//!
//! # The daemon and riff-server
//!
//! The daemon keeps no state of the riff and has no key of its own
//! (01M53NGW3FR555TWP8Z8HHRVDJ). riff-server holds the claims, the
//! messages and the wakes. The daemon makes the claim of each item on
//! riff-server as the worker session of the item, with the sign-in of
//! the person on this machine (01M53NGWTNC3655ZZJ76M1R5R7). The riff
//! tools of each session act on riff-server through the daemon
//! ([`Daemon::tools_of`]).
//!
//! # The relay
//!
//! The daemon binds its own binary at [`RELAY_BIN`] in each container,
//! with no write, and runs `RELAY_BIN relay -- COMMAND`: `riff relay` is
//! the entrypoint (01M4Z60B7KEV0H161RBZ1SDRY2, 01M4ZHSZZDH0C01A5EVD0KFY14).
//! The relay gives `claude` each input, and sends the daemon each event
//! ([`crate::session_relay`]).
//!
//! A session image can have an older libc than the host. So before each
//! start, the daemon reads the ELF of that binary ([`Daemon::check_relay`]).
//! When it is not static, for example a riff of `cargo install`, the
//! daemon starts no container, and the error names `riff update`
//! (01M510NW8YEVMQKD6RWJPW38N3). It reads the file at each start: after
//! `riff update`, the path holds the static riff, and the next start
//! works with no restart of the daemon.
//!
//! # The protocol
//!
//! JSON, one message on each line (01M4ZDTVVV56X964S28GS6K2Z2). The
//! first line of each connection on the control socket is a [`Hello`]:
//! the version [`PROTOCOL`] and one [`Ask`]. The daemon refuses another
//! version, and says to start it again with this riff. The daemon
//! answers with [`Say`] lines.
//!
//! | Ask | The daemon says |
//! |---|---|
//! | [`Ask::Status`] | [`Say::Status`] |
//! | [`Ask::Image`] | [`Say::Line`] for a pull, then [`Say::Done`] or [`Say::Failed`] |
//! | [`Ask::Start`] | [`Say::Started`], each [`Say::Event`] and [`Say::Stderr`], then [`Say::Ended`] or [`Say::Failed`] |
//! | [`Ask::Follow`] | each [`Say::Event`] after a number, and each [`Say::Line`] of the daemon about the session in its place, then as [`Ask::Start`]. For a session that ended in the last [`KEEP_END`]: then [`Say::Ended`] |
//! | [`Ask::Wave`] | [`Say::Done`] when the daemon runs the wave, or [`Say::Failed`] |
//! | [`Ask::WaveStop`] | [`Say::Done`] when the daemon ran the wave, or [`Say::Failed`] |
//! | [`Ask::Takeover`] | [`Say::Line`] while the turn stops or ends, [`Say::Held`], then [`Say::Done`] after the go on, or [`Say::Failed`] |
//! | [`Ask::Sweep`] | [`Say::Line`] for each folder that the daemon removed, and for each folder whose work is over that it kept, or one line that it removed none, then [`Say::Done`] ([`sweep`]) |
//! | [`Ask::End`] | [`Say::Line`] for the free item and the stop, then [`Say::Done`], or [`Say::Failed`] ([`Daemon::end_session`]) |
//! | [`Ask::Restart`] | [`Say::Line`] with the decision, then [`Say::Done`] ([`Daemon::restart_now`]) |
//! | [`Ask::Restarting`] | [`Say::Line`] for each lead on the host that got the note of the new start, then [`Say::Done`] ([`Daemon::tell_leads_restart`]) |
//! | [`Ask::Serve`] | [`Say::Line`], then [`Say::Done`] when the daemon serves the lead of the repository ([`remote`]), or [`Say::Failed`] |
//!
//! After [`Ask::Start`] or [`Ask::Follow`], `riff` can send
//! [`Then::Stop`]: the daemon stops the container. When `riff` closes the
//! connection, the session goes on: the daemon owns it. After
//! [`Say::Held`], `riff takeover` sends [`Then::GoOn`].
//!
//! `riff run` asks [`Ask::Follow`] after a new start of the daemon, with
//! the number of the last event that it showed
//! (01M4ZKX0F25S284R0N7KZVTHNA). `riff daemon events NAME` asks it with
//! the number 0 ([`show_events`]).
//!
//! On the socket of a session, the first line is a [`SessionAsk`]. The
//! daemon takes a version of the protocol from [`PROTOCOL`] - 1 to
//! [`PROTOCOL`] + 1 there ([`compatible`], 01M4ZHSZPRYK42C20ETXM3HJZ2):
//!
//! | Ask | The daemon |
//! |---|---|
//! | [`SessionAsk::Hello`] | says [`Say::Session`]: the session ID, the item and the name of the container |
//! | [`SessionAsk::Relay`] | relays the session ([`crate::session_relay`]) |
//! | [`SessionAsk::Mcp`] | serves the riff tools of the session that only talk, on its riff-server ([`Daemon::tools_of`]) |
//! | [`SessionAsk::Watch`] | says [`Say::Line`] at the next wake of the session, from riff-server ([`World::wake`]) |
//! | [`SessionAsk::Follow`] | says [`Say::Line`] at each wake of the session, until the relay leaves ([`World::wakes`]) |
//! | [`SessionAsk::Denied`] | posts the refusals of auto mode of the session, and says [`Say::Posted`] ([`Daemon::post_denials`]) |
//! | [`SessionAsk::Prompt`] | ends the block of the lead on riff-server, and says [`Say::Done`] ([`Daemon::prompted`]) |
//!
//! # A session
//!
//! Each start waits for room in the limit of the host ([`crate::room`]).
//! While a start of `riff run` waits, the daemon says why with
//! [`Say::Line`]. A [`Then::Stop`], or the end of the connection, before
//! the start starts nothing (01M4ZV3B6VS0RAR0P7MRHKE8CQ). The waves of
//! the daemon start their sessions in the same line ([`crate::wave`]).
//!
//! The daemon takes each event of the relay in order, takes each secret
//! out of it ([`Scrub`]), writes it to `events.jsonl` in the folder of
//! the session, and sends it to each `riff` that follows the session.
//! The event with the sequence number N is line N of `events.jsonl`. At
//! the `result` of the turn, it closes the input: `claude` ends. When
//! the daemon cannot keep an event, it closes the connection of the
//! relay with no ack. The relay connects again and sends the event
//! again. The daemon acks the exit of `claude` only when it has each
//! event before it (01M4ZHSZMH4Y8CB41FYGE3C3R2).
//!
//! A session ends when the daemon has the exit of the relay, or when its
//! container ends (01M4ZHT03MAH0B5Q4S6GZC5JRP). A [`Then::Stop`] stops
//! the container: the relay gives `claude` the SIGTERM.
//!
//! At the end of each session but the lead, the daemon sums the tokens
//! of its claim from the transcripts in its Claude folder, and puts the
//! sum on the issue of its item in the background
//! ([`crate::usage::Meter::session_ended`], 01M588N85Z94PX1ZP0BDXTPX83).
//! The marks and the `gh` come from [`World::meter`].
//!
//! The daemon keeps each line that it says about a session, for example
//! why it stops the container. A new follower gets each one in its place
//! among the events (01M57GCZXZZRPGGWC5HZS49DGM). After the end, the
//! daemon keeps the end of the session for [`KEEP_END`]: the log of its
//! events, its lines and how it ended (01M57GCZVJAHAMPEYY1T6GA48P). A
//! follow in that time gets them, as a follower that stayed. So `riff
//! daemon events --host` gets the end of a session that ended between
//! two pages ([`remote`]).
//!
//! `claude -p` does not end while one of its background tasks runs. The
//! end of each task starts a new turn. So a session can stay up after
//! the close. When no event comes for the quiet limit after the close
//! ([`QUIET_LIMIT`]), the daemon stops the container, and tells each
//! `riff` that follows the session why (01M4ZSZX77AJDGWNQMGBPV2QXG). Each
//! event moves the end of the quiet.
//!
//! A session of a wave has the shorter [`EXIT_GRACE`] in place of the
//! quiet limit, but only while `claude` runs no background task. The
//! last `background_tasks_changed` event says it ([`background_tasks`]).
//! While it lists one task or more, the session of a wave has the quiet
//! limit too (01M5P26VPJA73WCPVZY6CPG9HD, 01M5R0QFM265CCKTADK4T5XREQ).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon
//!     participant C as riff relay and claude -p
//!     C->>D: result
//!     D->>C: close the input
//!     C->>D: background_tasks_changed: one task runs
//!     Note over D,C: a wave session: the quiet limit, not the grace
//!     Note over C: the task ends: a new turn
//!     C->>D: background_tasks_changed: no task
//!     Note over D,C: a wave session: the grace counts now
//!     C->>D: init, ..., result
//!     Note over D,C: no event for the quiet limit
//!     D->>C: stop the container
//! ```
//!
//! # The watches
//!
//! A riff command that starts an async operation of the release cycle
//! registers a watch in [`FILE`](crate::watches::FILE): on the host, the
//! command writes the file; in a container, the lead asks the daemon on
//! its socket ([`ServerCall::Watch`]). [`Daemon::watch_every`] looks at
//! each watch, and posts one message to the lead of the repository when
//! the operation ends (01M5RGCW5NCKQYA61RM63914TX,
//! 01M5RGD0RTXYA7FF5K1TH72MGX). See [`crate::watches`] for the design.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as riff release publish
//!     participant D as riff daemon
//!     participant G as GitHub
//!     participant L as the lead
//!     C->>D: ServerCall::Watch (or the file, on the host)
//!     loop each minute
//!         D->>G: the state of the CI run
//!     end
//!     D->>L: one message at the end, then the watch goes
//! ```
//!
//! # The note before a new start
//!
//! A new start of the daemon on a new riff cuts the riff tools of the
//! lead for a moment. So before it, the daemon posts one note to each lead
//! on its host, as the person of the lead
//! ([`Daemon::tell_leads_restart`], 01M5SNV7H3739PXW73H14XM4B8). It does
//! so in each of the three ways to start again: `riff daemon restart`,
//! its own look at the end of each session
//! ([`Daemon::restart_when_old`]), and `riff update`, which asks it
//! first ([`Ask::Restarting`]). A daemon of an older riff knows no such
//! ask, and `riff update` starts it again all the same.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant U as riff update
//!     participant D as riff daemon
//!     participant L as each lead on the host
//!     participant M as systemd
//!     U->>D: Ask::Restarting
//!     D->>L: one note: the daemon starts again, keep your state in your notes
//!     D-->>U: Say::Done
//!     U->>M: try-restart riff.service
//! ```
//!
//! # The wakes of a session
//!
//! A worker or a verifier gets each `riff tell` to it and each message
//! that wakes it, as a session in a container does
//! (01M5792DP1KZR2XGSCMRWDHCGZ). While the session runs, the daemon
//! watches its wakes on riff-server ([`World::wakes`]). While the input
//! of `claude` is open, from the start until the first `result`, the
//! daemon gives each wake as one input: the line of `riff watch --once`
//! ([`text::wake_line`]) on the stdin of `claude`.
//! During a takeover, the wake waits, and `claude` gets it after the go
//! on. A new watch of riff-server gives the newest unread wake again:
//! the daemon gives each wake once.
//!
//! After the close, the session ended its work. The daemon tells the
//! sender of each wake that the session does not read the message
//! ([`World::refuse`], [`text::wake_refused`],
//! 01M5792DRA12JD032F3H8JVC0Z), once for each sender
//! (01M57AVKBHFVYJBRT0W2ZTAJDK). After the end of the session on
//! riff-server, riff-server refuses a `riff tell` to it. The lead waits
//! for its wakes itself: `riff relay watch`.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead
//!     participant E as riff-server
//!     participant D as riff daemon
//!     participant C as riff relay and claude -p
//!     D->>E: watch the wakes of the session
//!     D->>C: the start text
//!     L->>E: riff tell SESSION "..."
//!     E-->>D: wake
//!     D->>C: the wake line, as one input
//!     C->>D: result
//!     D->>C: close the input
//!     L->>E: riff tell SESSION "..."
//!     E-->>D: wake
//!     D->>E: tell the lead: the session ended its work
//! ```
//!
//! # The lead
//!
//! The lead is a session with a terminal (01M4Z60B81RTZT71PN5HV0GZHX,
//! 01M501PA2C087SEN4M4QTK5J1E, [`crate::lead`]). [`Ask::Start`] with [`Start::terminal`] starts it
//! with [`Runtime::start_terminal`]: conmon holds its terminal, and
//! `riff` attaches to it. Its command is `riff relay --terminal --
//! claude ...`: the relay adds the riff tools. The lead does not wait
//! for room, and does not count in the limit of the host: it runs no
//! build of an item. While it runs, the daemon keeps its session on
//! riff-server alive, and at its end it ends that session
//! ([`World::resume`], [`World::leave`]).
//!
//! # The lead on a new riff
//!
//! A bind of a file keeps the old file, so a lead keeps the riff that it
//! started with, also after `riff update`. The daemon starts it again
//! ([`Daemon::leads_again`], 01M5TC24PB3YC2CC8BA275WSEN,
//! 01M5TC2919TB9XPF42YFNJBP79, 01M5TC29A4Q94NJP7HFC75XCF3). A new daemon
//! looks at its start, when it takes up the sessions that run, and each
//! [`AGAIN_EVERY`]. A lead is old when the file that its container binds
//! at [`RELAY_BIN`] has no link in the container ([`crate::lead::old`]).
//! It is idle when its transcript got no write in [`LEAD_IDLE`]
//! ([`crate::lead::quiet`]): the daemon sees no turn of a lead, because the
//! relay of a lead relays no event, but each message and each tool call
//! of a turn writes the transcript. A lead in a turn waits for the next
//! look.
//!
//! ```mermaid
//! flowchart TD
//!     L[look: each lead] --> O{old: the bind has no link?}
//!     O -- no --> N[nothing to do]
//!     O -- yes --> I{idle: no write to the transcript in 5 minutes?}
//!     I -- no --> W[wait for the next look]
//!     I -- yes --> P[make the start: wave, host, image, relay]
//!     P -- fault --> F[one line in chat, try again at the next look]
//!     P -- ok --> S[podman stop, no leave]
//!     S --> R[podman run: claude --resume ID]
//!     R -- ok --> C[one line in chat: started again]
//!     R -- 3 faults --> E[leave, one line in chat: riff lead start]
//! ```
//!
//! The new container has the same name, session, clone and Claude folder.
//! `claude --resume` gives it the conversation back. Its first prompt
//! ([`text::lead_again`]) tells the lead to start its Monitor again: the
//! old one ended with the old container. The session of the lead on
//! riff-server stays, and so does its place as the lead of the person.
//!
//! # A takeover
//!
//! A person takes over a worker with [`Ask::Takeover`]
//! (01M4Z60B83ZH10HYKW7X1C0SSS, 01M501PABB010P4S8YF7R29XXS). The daemon
//! keeps the takeover in the [`Record`], and tells the relay
//! [`Say::Hold`]: `claude` ends after its turn, and the relay waits
//! ([`crate::session_relay`]). The hold interrupts the turn at once, when
//! the input of `claude` is open (01M51KJ1R9K7A7RW8T8XH8VW43,
//! 01M5RG6C25DB49ZKM03Y93QB1F). With `at_end`, it waits for the end of
//! the turn. The result of a turn that a takeover interrupts is no fail
//! of the session ([`Record::interrupt`]). At the
//! [`Up::Held`] of the relay, the daemon says [`Say::Held`] to `riff
//! takeover`, which runs `claude --resume ID` in the container with the
//! terminal of the person. At [`Then::GoOn`], or at the end of the
//! connection of `riff takeover`, the daemon gives the session back: it
//! tells the relay [`Say::GoOn`], and gives `claude` one new input
//! ([`text::TAKEOVER_GO_ON`]). The quiet limit does not count during a
//! takeover. A restart of the daemon keeps the takeover: a new `riff
//! takeover NAME` gets [`Say::Held`] at once. The daemon refuses a
//! takeover of a session with a terminal, and of a relay of a protocol
//! before 4.
//!
//! [`Up::Held`]: crate::session_relay::Up::Held
//!
//! # A stop of the daemon
//!
//! A stop or a restart of the daemon stops no container
//! (01M4ZDTW00B7N6T9P3MK066KA7). conmon holds each container in a scope
//! of its own ([`crate::podman`]). The relay keeps `claude` running and
//! keeps its events while the daemon is away (01M4Z60B7Z24WBD6HV6ER0WN9K).
//!
//! The daemon keeps a [`Record`] of each session in `live/NAME.json`.
//! At its start, a new daemon reads each record ([`Daemon::recover`]).
//! It takes up each session whose container runs, or when podman cannot
//! say: it binds the socket of the session again in the same folder, and
//! the relay connects again. It removes the record of each other one
//! (01M4ZHT01HH88KPA8628M7VBEY).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff run
//!     participant D1 as riff daemon
//!     participant L as riff relay
//!     participant D2 as new riff daemon
//!     R->>D1: start
//!     D1->>D1: live/NAME.json
//!     L->>D1: event 1, event 2
//!     D1-->>R: event 1, event 2
//!     Note over D1: SIGTERM: the daemon stops
//!     Note over L: keeps event 3
//!     D2->>D2: live/NAME.json: the container runs
//!     L->>D2: hello relay
//!     D2-->>L: resume: events 2
//!     L->>D2: event 3
//!     R->>D2: follow NAME after 2
//!     D2-->>R: event 3
//! ```
//!
//! # A new riff
//!
//! `riff update` puts a new riff on disk, and the daemon goes on with
//! the old one. The daemon runs an old riff when its binary was
//! replaced, or when its unit names another riff ([`stale`]). Two steps
//! start it again on the new riff, only when it is idle: it owns no
//! session but the lead (the new daemon takes the lead up, with its
//! container, its terminal and its socket), no start of `riff run` waits for room, no job of a wave
//! runs, and no real-session test runs ([`Status::busy`],
//! 01M5QVBTXE8YRY44A1C1VP71QS). A start of a wave that waits for room does
//! not count, also with the limit 0: the wave is in `waves.json`, so the
//! new daemon waits for room for it again (01M5MS1Q78AFEY3ZVXNAQXM1FJ).
//!
//! - `riff update` ([`after_update`]): when the `ExecStart` of the unit
//!   is not the `riff` on `PATH`, it writes the unit again with that riff
//!   and the same variables (01M517BTDRKDXD3X84P6AEAF7P). When the
//!   daemon runs an old riff and is idle, it starts the unit again. Else
//!   it says that the daemon starts again after its last session
//!   (01M517BTJ9EV4ZGPYRB6HXJJC9).
//! - The daemon ([`Daemon::restart_when_old`]): at the end of each
//!   session and of each job, and each [`OLD_LOOK`], it looks. When it
//!   is idle, runs an old riff, and is the process of the unit, it asks
//!   systemd once to start the unit again (01M517BTP6BAGTRBYWPX9GPVE0).
//!   The stop stops no container (01M4ZDTW00B7N6T9P3MK066KA7).
//!
//! ```mermaid
//! flowchart TD
//!     U["riff update"] --> X{"ExecStart is the riff on PATH?"}
//!     X -- no --> W["write the unit with the riff on PATH,<br/>systemctl --user daemon-reload"]
//!     X -- yes --> O
//!     W --> O{"the daemon runs an old riff?"}
//!     O -- no --> N["nothing"]
//!     O -- yes --> B{"a session, a start of riff run that waits,<br/>or a job of a wave?"}
//!     B -- no --> R["systemctl --user --no-block try-restart riff.service"]
//!     B -- yes --> L["the daemon starts again<br/>after its last session"]
//!     L --> E["the end of a session or a job, each minute"]
//!     E --> B
//! ```
//!
//! At each start, with `update.auto` on, the daemon of a release does
//! each step of `riff init` that needs no person and that is not in
//! step ([`crate::update_steps`], 01M51QFPJAABDP1V7WFY8NQR60): it writes
//! its unit again, and pulls the session image of each wave. When it wrote its
//! unit, it starts again on it as for an old riff. So a new release
//! needs no `riff init`.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::build::Build;
use riff_core::name::SessionUri;
use riff_core::wire::Wake;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::room::{Limit, Room, Slot};
use crate::run::{Scrub, user_line};
use crate::runtime::{Bind, End, Env, Runtime, SessionSpec, Source};
use crate::session_relay::Up;
use crate::wave::{Job, Wave, World};
use crate::{text, usage};
use riff_client::sessions::{Live, Phase};

pub mod facts;
pub mod release_test;
pub mod remote;
pub mod sweep;

/// The version of the protocol between `riff` and the daemon, and
/// between the relay and the daemon. Version 3 adds the waves
/// ([`Ask::Wave`], [`Ask::WaveStop`]). Version 4 adds the sessions with a
/// terminal, the takeover and the watch of a session.
pub const PROTOCOL: u32 = 4;

/// The wait of `riff daemon restart` before the ask to systemd: the
/// reply to the lead of another host goes first.
pub const RESTART_DELAY: Duration = Duration::from_secs(5);

/// The time between two looks of the daemon at the sign-in of each lead
/// ([`Daemon::stale_leads`]).
pub const STALE_EVERY: Duration = Duration::from_secs(60);

/// How many times the daemon tries to start a lead again after it stopped
/// the old container ([`Daemon::lead_again`]).
const LEAD_STARTS: u32 = 3;

/// The wait between two of those tries: podman removes the old container
/// a moment after its end.
const LEAD_START_WAIT: Duration = Duration::from_secs(1);

/// The time between two looks of the daemon at the riff of each lead
/// ([`Daemon::leads_again`]).
pub const AGAIN_EVERY: Duration = Duration::from_secs(60);

/// A lead is idle when its transcript got no write for this long
/// ([`crate::lead::quiet`], 01M5TC24PB3YC2CC8BA275WSEN). A turn writes the
/// transcript at each message and each tool call.
pub const LEAD_IDLE: Duration = Duration::from_secs(5 * 60);

/// The variable that gives [`LEAD_IDLE`] of a `riff daemon` in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::daemon::LEAD_IDLE_VAR, "RIFF_DAEMON_LEAD_IDLE_MS");
/// ```
pub const LEAD_IDLE_VAR: &str = "RIFF_DAEMON_LEAD_IDLE_MS";

/// The first protocol of a relay that takes [`Say::Hold`].
pub const TAKEOVER_PROTOCOL: u32 = 4;

/// The name of each socket of the daemon.
pub const SOCKET: &str = "riff.sock";

/// The folder of its socket in a session.
pub const IN_SESSION: &str = "/run/riff";

/// The variable that names the socket in a session.
pub const SOCKET_VAR: &str = "RIFF_SOCKET";

/// The path of the binary of riff in a session (01M4ZHSZZDH0C01A5EVD0KFY14).
pub const RELAY_BIN: &str = "/opt/riff/riff";

/// The folder of the records of the live sessions, in the folder of the
/// daemon.
pub const LIVE: &str = "live";

/// The log of the events of a session, in its folder.
pub const EVENTS: &str = "events.jsonl";

/// After the end of a container, or of the connection of a relay, the
/// longest wait for the exit of the relay.
const GONE_WAIT: Duration = Duration::from_secs(1);

/// How often the daemon looks at the container of a session that it took
/// up while no relay is connected: a relay that gave up does not come.
const LOOK_EVERY: Duration = Duration::from_secs(30);

/// The quiet limit: after the input of `claude` closed, the longest time
/// with no event of the session. Then the daemon stops its container
/// (01M4ZSZX77AJDGWNQMGBPV2QXG). It is longer than the longest check of
/// a session: a `just ci` can take 15 minutes.
pub const QUIET_LIMIT: Duration = Duration::from_secs(30 * 60);

/// The grace of a session of a wave: after the input of `claude` closed,
/// the longest time with no event while `claude` runs no background task.
/// Then the daemon stops the container (01M5P26VPJA73WCPVZY6CPG9HD). A
/// session of a wave leaves no background task at its end. While a task
/// runs, the session has the [`QUIET_LIMIT`].
pub const EXIT_GRACE: Duration = Duration::from_secs(60);

/// How long the daemon keeps the end of a session after the end: its
/// events, its lines and how it ended (01M57GCZVJAHAMPEYY1T6GA48P). It
/// is longer than the gap between two pages of `riff daemon events
/// --host` ([`remote::follow_events`]).
pub const KEEP_END: Duration = Duration::from_secs(5 * 60);

/// The end of a session whose container did not start.
const START_FAILED: Ended = Ended {
    code: None,
    failed: true,
};

/// The variable that gives the quiet limit of a `riff daemon` in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::daemon::QUIET_VAR, "RIFF_DAEMON_QUIET_MS");
/// ```
pub const QUIET_VAR: &str = "RIFF_DAEMON_QUIET_MS";

/// [`QUIET_LIMIT`], or the milliseconds of [`QUIET_VAR`].
fn quiet_limit() -> Duration {
    std::env::var(QUIET_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(QUIET_LIMIT, Duration::from_millis)
}

/// [`LEAD_IDLE`], or the milliseconds of [`LEAD_IDLE_VAR`].
fn lead_idle() -> Duration {
    std::env::var(LEAD_IDLE_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(LEAD_IDLE, Duration::from_millis)
}

/// The folder of the daemon in the data folder `data` of riff.
///
/// ```
/// use std::path::Path;
///
/// let dir = riff::daemon::dir_in(Path::new("/h/.local/share/riff"));
/// assert_eq!(dir, Path::new("/h/.local/share/riff/daemon"));
/// assert_eq!(riff::daemon::socket(&dir), Path::new("/h/.local/share/riff/daemon/riff.sock"));
/// assert_eq!(
///     riff::daemon::session_dir(&dir, "riff-issue-12-1a2b3c4d"),
///     Path::new("/h/.local/share/riff/daemon/sessions/riff-issue-12-1a2b3c4d")
/// );
/// assert_eq!(
///     riff::daemon::record_path(&dir, "riff-issue-12-1a2b3c4d"),
///     Path::new("/h/.local/share/riff/daemon/live/riff-issue-12-1a2b3c4d.json")
/// );
/// ```
pub fn dir_in(data: &Path) -> PathBuf {
    data.join("daemon")
}

/// The folder of the daemon of this process: [`dir_in`] the data folder
/// of riff ([`crate::run::data_dir`]).
pub fn dir() -> Result<PathBuf> {
    Ok(dir_in(&crate::run::data_dir()?))
}

/// The control socket in the folder `dir` of the daemon.
pub fn socket(dir: &Path) -> PathBuf {
    dir.join(SOCKET)
}

/// The folder of the socket of the session `name`.
pub fn session_dir(dir: &Path, name: &str) -> PathBuf {
    dir.join("sessions").join(name)
}

/// The [`Record`] of the session `name`.
pub fn record_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(LIVE).join(format!("{name}.json"))
}

/// The path of the socket in a session: `/run/riff/riff.sock`.
pub fn in_session() -> PathBuf {
    Path::new(IN_SESSION).join(SOCKET)
}

/// True when the daemon takes a session of the protocol `theirs`: one
/// version before its own, its own, and one after
/// (01M4ZHSZPRYK42C20ETXM3HJZ2).
///
/// ```
/// use riff::daemon::{PROTOCOL, compatible};
///
/// assert!(compatible(PROTOCOL) && compatible(PROTOCOL - 1) && compatible(PROTOCOL + 1));
/// assert!(!compatible(PROTOCOL - 2) && !compatible(PROTOCOL + 2));
/// ```
pub fn compatible(theirs: u32) -> bool {
    theirs.abs_diff(PROTOCOL) <= 1
}

/// The binary of this process: the daemon binds it into each session.
/// After an update of riff, the path holds the new binary
/// (01M4Z60B800BH5BHX0H9JVDG6A).
pub fn own_binary() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot find the path of riff")?;
    let text = exe.to_string_lossy();
    Ok(match text.strip_suffix(" (deleted)") {
        Some(path) => PathBuf::from(path),
        None => exe,
    })
}

/// How often the daemon looks for a new riff (01M517BTP6BAGTRBYWPX9GPVE0).
/// The end of each session and of each job looks too.
pub const OLD_LOOK: Duration = Duration::from_secs(60);

/// True when a daemon that runs the binary `own` runs an old riff: its
/// binary was `replaced`, or its `unit` names another riff
/// (01M517BTJ9EV4ZGPYRB6HXJJC9).
///
/// ```
/// use std::path::Path;
/// use riff::daemon::stale;
/// use riff::service::Unit;
///
/// let own = Path::new("/home/mike/.cargo/bin/riff");
/// let unit = |exe: &str| Unit { exe: exe.into(), env: vec![] };
/// let cargo = unit("/home/mike/.cargo/bin/riff");
/// assert!(!stale(Some(&cargo), own, false));
/// assert!(stale(Some(&cargo), own, true));
/// let check = unit("/home/mike/.cache/riff/check/target/static/release/riff");
/// assert!(stale(Some(&check), own, false));
/// assert!(!stale(None, own, false));
/// ```
pub fn stale(unit: Option<&crate::service::Unit>, own: &Path, replaced: bool) -> bool {
    replaced || unit.is_some_and(|unit| !crate::binary::same_file(&unit.exe, own))
}

/// True when this daemon runs an old riff ([`stale`]) or an old unit
/// that its own step of the unit wrote again
/// ([`crate::update_steps::unit_written`], 01M51QFPQ68JHS08TWXNNWZ0D7),
/// and the unit of `manager` runs it (01M517BTP6BAGTRBYWPX9GPVE0). The
/// daemon of a `RIFF_HOME` or a `RIFF_DATA` is never the daemon of the
/// unit ([`crate::run::machine_data`]). While an update of riff runs
/// ([`update_waits`]), it waits (01M58AW9YR84SKSYBYBJ6V7TNJ).
fn old_here(manager: &impl crate::service::ServiceManager) -> bool {
    if !crate::run::machine_data() || crate::local::dir().is_some_and(|dir| update_waits(&dir)) {
        return false;
    }
    let Ok(own) = own_binary() else {
        return false;
    };
    let unit = manager.installed().ok().flatten();
    let replaced = crate::binary::replaced(Path::new("/proc/self/exe"));
    (stale(unit.as_ref(), &own, replaced) || crate::update_steps::unit_written())
        && manager.main_pid().ok().flatten() == Some(std::process::id())
}

/// True while an update of riff holds the update lock in the local
/// files `dir` of riff. Then the daemon does not start again: its new
/// start stops each process of its unit, also the update that it
/// started, before that update tells the lead
/// (01M58AW9YR84SKSYBYBJ6V7TNJ).
///
/// ```standalone_crate
/// use riff::daemon::update_waits;
///
/// let dir = tempfile::tempdir()?;
/// assert!(!update_waits(dir.path()));
/// let lock = riff::local::update(dir.path())?.expect("free");
/// assert!(update_waits(dir.path()));
/// drop(lock);
/// assert!(!update_waits(dir.path()));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn update_waits(dir: &Path) -> bool {
    crate::local::updating(dir)
}

/// Posts the note of a new start to the lead of each of `records` that has
/// a terminal, and gives a line for each (01M5SNV7H3739PXW73H14XM4B8).
async fn tell_leads_restart(world: &Arc<dyn World>, records: &[Record]) -> Vec<String> {
    let mut lines = Vec::new();
    for record in records.iter().filter(|r| r.terminal) {
        world
            .note_lead(record, text::LEAD_DAEMON_STARTS_AGAIN)
            .await;
        lines.push(text::daemon_told_lead(&record.live.name));
    }
    lines
}

/// The step of `riff update` for the daemon of this machine, whose
/// folder is `dir`, with the new `riff` on `PATH`: one line for each
/// change.
///
/// When the `ExecStart` of the unit of `manager` is not `riff`, it
/// writes the unit again with `riff` and the same variables
/// (01M517BTDRKDXD3X84P6AEAF7P). When the daemon answers, runs an old
/// riff ([`stale`]) and is idle ([`Status::busy`]), it starts the unit
/// again. When it is not idle, the daemon starts again after its last
/// session ([`Daemon::restart_when_old`], 01M517BTJ9EV4ZGPYRB6HXJJC9).
/// With no unit, it does nothing.
pub async fn after_update(
    manager: &impl crate::service::ServiceManager,
    riff: &Path,
    dir: &Path,
) -> Result<Vec<String>> {
    let Some(old) = manager.installed()? else {
        return Ok(Vec::new());
    };
    let mut lines = Vec::new();
    let unit = crate::service::Unit {
        exe: riff.to_owned(),
        env: old.env.clone(),
    };
    if !crate::binary::same_file(&old.exe, riff) {
        let path = manager.rewrite(&unit)?;
        lines.push(text::daemon_unit_moved(&path, &old.exe, riff));
    }
    let Ok(status) = status(dir).await else {
        return Ok(lines);
    };
    let exe = std::fs::read_link(format!("/proc/{}/exe", status.pid))
        .map(crate::binary::on_disk)
        .unwrap_or(old.exe);
    if !stale(Some(&unit), &exe, crate::binary::runs_replaced(status.pid)) {
        return Ok(lines);
    }
    match status.busy() {
        0 => {
            // The leads hear of it first. A daemon of an older riff
            // knows no such ask: the new start goes on all the same.
            let _ = ask_done(dir, Ask::Restarting, |_| {}).await;
            manager.try_restart()?;
            lines.push(text::DAEMON_STARTS_AGAIN.to_owned());
        }
        busy => lines.push(text::daemon_starts_again_later(busy)),
    }
    Ok(lines)
}

/// Makes the folder `dir` with mode 0700.
fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot set the mode of {}", dir.display()))
}

/// Runs `with` on a path of the socket `name` in the folder `dir` that
/// fits in the address of a Unix socket. On Linux, it is a path through
/// a descriptor of the folder: `/proc/self/fd/N/NAME`.
fn short_path<T>(dir: &Path, name: &str, with: impl FnOnce(&Path) -> T) -> Result<T> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let folder =
            std::fs::File::open(dir).with_context(|| format!("cannot open {}", dir.display()))?;
        let path = PathBuf::from(format!("/proc/self/fd/{}/{name}", folder.as_raw_fd()));
        let out = with(&path);
        drop(folder);
        Ok(out)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(with(&dir.join(name)))
    }
}

/// Binds the socket `name` in the folder `dir`, with mode 0600.
fn bind_in(dir: &Path, name: &str) -> Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let listener = short_path(dir, name, |path| UnixListener::bind(path))?
        .with_context(|| format!("cannot listen on {}", dir.join(name).display()))?;
    std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Connects to the socket `name` in the folder `dir`.
pub(crate) async fn connect_in(dir: &Path, name: &str) -> std::io::Result<UnixStream> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let folder = std::fs::File::open(dir)?;
        let path = format!("/proc/self/fd/{}/{name}", folder.as_raw_fd());
        let stream = UnixStream::connect(path).await;
        drop(folder);
        stream
    }
    #[cfg(not(target_os = "linux"))]
    {
        UnixStream::connect(dir.join(name)).await
    }
}

/// The first line of each connection: the version of the protocol, and
/// the ask.
#[derive(Serialize, Deserialize)]
pub struct Hello {
    /// [`PROTOCOL`] of the side that asks.
    pub protocol: u32,
    /// What it asks.
    pub ask: Ask,
}

/// What `riff` asks the daemon.
#[derive(Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum Ask {
    /// The state of the daemon and its sessions.
    Status,
    /// Have the image on this machine: pull it when it is not here.
    Image {
        /// The image.
        image: String,
    },
    /// Start a session, and send its events.
    Start(Box<Start>),
    /// Send the events of a live session after the number `after`, and
    /// each later one (01M4ZHT05QHQRQ2KGVJFB9F7HD).
    Follow {
        /// The name of its container.
        name: String,
        /// The sequence number of the last event that `riff` has.
        after: u64,
    },
    /// Run the current wave of a repository ([`crate::wave`]).
    Wave {
        /// The wave.
        wave: Wave,
    },
    /// Start no new session for the wave of the repository `repo`.
    WaveStop {
        /// The repository, as `OWNER/REPO`.
        repo: String,
    },
    /// Take over the live session in the container `name`
    /// (01M4Z60B83ZH10HYKW7X1C0SSS, 01M51KJ1R9K7A7RW8T8XH8VW43).
    Takeover {
        /// The name of its container.
        name: String,
        /// True to wait for the end of the turn. Else the takeover
        /// interrupts the turn at once.
        #[serde(default)]
        at_end: bool,
    },
    /// `riff daemon sweep`: remove the folder of each session that does
    /// not run, when the forge says that its work is over and its clone
    /// holds no work of its own (01M53NJ5Y2CMY5MWF9328KW7DE,
    /// 01M53S7WAXD46YPN0DR96XSTMG, [`Daemon::sweep`]).
    Sweep,
    /// `riff daemon sweep --remove NAME`: remove the folder `name` of one
    /// ended session on purpose, when its work is on a merged branch, or
    /// with `force` (01M5P26VW5GBJDQZGEYW9A77Y0, [`Daemon::remove_one`]).
    Remove {
        /// The name of the folder, `ITEM-ID8`.
        name: String,
        /// True to remove it with no check of the forge.
        #[serde(default)]
        force: bool,
    },
    /// `riff daemon end NAME`: free the item of the session in the
    /// container `name` and stop it ([`Daemon::end_session`]).
    End {
        /// The name of its container.
        name: String,
    },
    /// `riff daemon restart`: start again on the new riff when no
    /// session runs ([`Daemon::restart_now`]).
    Restart,
    /// `riff update` is about to start the daemon again on a new riff: tell
    /// each lead on this host first ([`Daemon::tell_leads_restart`],
    /// 01M5SNV7H3739PXW73H14XM4B8). The daemon gives a line for each lead.
    Restarting,
    /// `riff daemon host`: serve the lead of the repository of `wave`
    /// ([`Daemon::serve_repo`]).
    Serve {
        /// The repository, its main clone, its riff-server and this host.
        wave: Wave,
    },
}

/// A session to start. It holds the values of the host and the
/// secrets, so it has no `Debug`: no log shows it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Start {
    /// The riff session ID.
    pub session: String,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The folder of the session: `events.jsonl` goes there.
    pub folder: PathBuf,
    /// The spec. The daemon adds the bind of the socket and of the
    /// relay.
    pub spec: SessionSpec,
    /// The start text: the first message to `claude`.
    pub start: String,
    /// The value of each [`Env::Host`] of the spec.
    pub values: Vec<(String, String)>,
    /// The secrets that no event shows ([`Scrub`]).
    pub secrets: Vec<String>,
    /// The riff-server of the session: its riff tools act there.
    pub server: String,
    /// The URI of the session.
    pub me: SessionUri,
    /// True for a session with a terminal: the lead. Its start text is
    /// in the command of the spec.
    #[serde(default)]
    pub terminal: bool,
}

/// A line of `riff` after [`Ask::Start`], [`Ask::Follow`] or
/// [`Ask::Takeover`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "then", rename_all = "snake_case")]
pub enum Then {
    /// Stop the container.
    Stop,
    /// The terminal of the takeover ended: the session goes on.
    GoOn,
}

/// What a session asks on its own socket: the first line of a
/// connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum SessionAsk {
    /// Who am I?
    Hello {
        /// [`PROTOCOL`] of the session.
        protocol: u32,
    },
    /// The relay of the session ([`crate::session_relay`]).
    Relay {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
        /// The number of inputs that the relay gave `claude`.
        inputs: u64,
        /// True when the relay closed the stdin of `claude`.
        closed: bool,
        /// True when `claude` ended for a takeover, and the relay waits
        /// for [`Say::GoOn`].
        #[serde(default)]
        held: bool,
    },
    /// The riff tools of the session, over MCP
    /// (01M4ZHSZXAY7F7E17B3CMHG562).
    Mcp {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
        /// The initialize request of `claude`, after a new start of the
        /// daemon: the daemon skips the handshake.
        client: Option<Value>,
    },
    /// Wait for the next wake of the session: `riff relay watch`.
    Watch {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
    },
    /// Say each wake of the session until the relay leaves:
    /// `riff relay watch --follow`.
    Follow {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
    },
    /// Post these refusals of auto mode as the session:
    /// `riff relay denied --send` ([`crate::denied`]).
    Denied {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
        /// The refusals, oldest first, with no secret.
        denials: Vec<crate::denied::Denial>,
    },
    /// The person of the session gave a prompt: `riff relay prompt`,
    /// the `UserPromptSubmit` hook (01M5GVQQ8NK3RD9CM3VTGXEHZQ).
    Prompt {
        /// [`PROTOCOL`] of the relay.
        protocol: u32,
    },
    /// Who runs a command of the release cycle in this session: the
    /// lead, or a work session. The socket of the session tells the
    /// daemon which one asks, so a work session cannot pose as the lead
    /// (01M59269HHZ5C82DR3RG3QHAM3).
    Caller {
        /// [`PROTOCOL`] of the session.
        protocol: u32,
    },
    /// A call on the riff-server of the session. The daemon makes it with
    /// the sign-in of the person on this machine ([`crate::reach`],
    /// 01M5GVPNAKAPA596K9361HSAAV). The answer is [`Say::Served`].
    Server {
        /// [`PROTOCOL`] of the session.
        protocol: u32,
        /// The call.
        call: ServerCall,
    },
}

/// A call of [`SessionAsk::Server`]: the reads of `riff wave list`,
/// `riff release check`, `riff members`, `riff who`, `riff whoami`,
/// `riff audit` and `riff session why`, the resume of the riff after
/// `riff release check`, the run of an item by the lead, and the events
/// of a session for the lead (#1338, 01M5QVX8D3JE51FAYRWXMAYFAY,
/// 01M5QW31TFA1W2GWPGRS2Y65GP, 01M5QVX8FFG2FPWZEW61A985CF,
/// 01M5SMP2FFEHJPZBJVGAWBXNNP).
///
/// ```
/// use riff::daemon::ServerCall;
///
/// let run = ServerCall::Run { item: "issue-12".into() };
/// assert_eq!(serde_json::to_string(&run)?, r#"{"call":"run","item":"issue-12"}"#);
/// let events = ServerCall::Events { name: "riff-issue-12-1a2b3c4d".into(), after: 7, notes: 1 };
/// assert_eq!(
///     serde_json::to_string(&events)?,
///     r#"{"call":"events","name":"riff-issue-12-1a2b3c4d","after":7,"notes":1}"#
/// );
/// assert_eq!(serde_json::to_string(&ServerCall::Members)?, r#"{"call":"members"}"#);
/// assert_eq!(serde_json::to_string(&ServerCall::Ends)?, r#"{"call":"ends"}"#);
/// let log = ServerCall::Log { repo: "o/r".parse().unwrap() };
/// assert_eq!(serde_json::to_string(&log)?, r#"{"call":"log","repo":"o/r"}"#);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum ServerCall {
    /// The version of the build of riff-server, or `null` when it names
    /// none.
    Build,
    /// The sessions of the riff, as `riff who` reads them. `all` adds
    /// the gone sessions.
    Who {
        /// True for the gone sessions too.
        all: bool,
    },
    /// The holds of the thread `thread`, by their item.
    Holds {
        /// The repository thread.
        thread: riff_core::name::ThreadName,
    },
    /// Resume the whole riff. Only the lead may. The answer is the line
    /// for the person.
    ResumeRiff,
    /// Pause the whole riff, for `riff release approve`. Only the lead
    /// may. The answer is the line for the person.
    PauseRiff,
    /// The user of the session, the owner and the people of the riff, as
    /// [`crate::reach::Roster`], for `riff release cut`, `publish` and
    /// `approve`.
    Roster,
    /// Post `body` in the thread `chat`. The daemon picks the sessions
    /// that the line wakes ([`crate::chat::wakes`]). The answer is
    /// `null`.
    ChatPost {
        /// The line.
        body: String,
    },
    /// Each kept message of the thread `chat`, as a list of
    /// [`crate::cycle::ChatLine`].
    ChatRead,
    /// The [`Status`] of this daemon, for `riff daemon status` in a
    /// container (#1239). A session that is not the lead may ask it.
    Status,
    /// One [`remote::Page`] of the events of the session `name` of this
    /// daemon, after the event `after` and the line `notes` of the
    /// daemon, as `riff daemon events NAME` follows it in a container
    /// (#1441). Only the lead may. The answer is the text of the page.
    Events {
        /// The name of the container.
        name: String,
        /// The last event that the caller has: 0 for none.
        after: u64,
        /// The number of lines of the daemon that the caller has.
        notes: u64,
    },
    /// A request to the daemon of the host `host` through riff-server, as
    /// `riff daemon status --host` makes it on a host
    /// ([`remote::Request`], #1239). Only the lead may. The answer is
    /// the text of the reply.
    Host {
        /// The host.
        host: String,
        /// The body of the request, as [`remote::Request`] writes it.
        request: String,
    },
    /// The lines of the Cloud Run log that `filter` matches, read with the
    /// `gcloud` and the sign-in of the person on the host
    /// (`riff release check`, #1247). Only the lead may. The answer is
    /// the output of `gcloud logging read` ([`crate::cycle::log_read_args`]).
    CloudLog {
        /// The Cloud Run service.
        service: String,
        /// The project of the service.
        project: String,
        /// The filter of the lines.
        filter: String,
    },
    /// What `riff whoami` reads: [`crate::reach::WhoamiFacts`] (#1338).
    Whoami,
    /// What `riff who` reads: [`crate::reach::WhoFacts`] (#1338). `all`
    /// adds the gone sessions.
    WhoFacts {
        /// True for the gone sessions too.
        all: bool,
    },
    /// Who may join the riff, as `riff members` reads it
    /// ([`riff_core::wire::MembersReply`], #1338).
    Members,
    /// The records of the repository thread `repo` for `riff audit`
    /// ([`riff_core::wire::LogReply`], #1338). Only the lead may, and
    /// riff-server answers only for the owner and the admins.
    Log {
        /// The repository thread.
        repo: riff_core::name::ThreadName,
    },
    /// The records of the thread `ends`, oldest first, as `riff session
    /// why` reads them (a list of [`crate::ends::Record`], #1338).
    Ends,
    /// Run the work item `item` as a job of the wave of the repository,
    /// as `riff run` does on the host (#1338). Only the lead may. The
    /// answer is the line for the person: the daemon starts the session
    /// when the host has room.
    Run {
        /// The work item, for example `issue-12`.
        item: String,
    },
    /// Post `body` as the note of a step of a release to each other lead of
    /// the riff ([`crate::hold::tell_leads`], 01M5SNV75DRFHR26BT2M8243VV).
    /// Each session may: a worker cuts a release. The answer is the number
    /// of leads told.
    ReleaseNews {
        /// The note.
        body: String,
    },
    /// Hold the version `tag` as the lead of the session, or end its hold
    /// when `reason` is `None` ([`crate::hold`], 01M5SNV78EBAYY4SZ891DM2MQG).
    /// Only the lead may. The answer is a [`crate::hold::Held`].
    ReleaseHold {
        /// The release tag, `vX.Y.Z`.
        tag: String,
        /// Why the lead asks to wait, or `None` to end its hold.
        reason: Option<String>,
    },
    /// Each message of the thread `release` as a list of
    /// [`crate::hold::Line`], for the holds. Each session may.
    ReleaseLines,
    /// Register the watch of an async operation that a command of the release
    /// cycle started, for the repository of the session
    /// ([`crate::watches`], 01M5RGCW5NCKQYA61RM63914TX). Each session may:
    /// the watch only wakes the lead. The answer is [`crate::watches::Registered`].
    Watch {
        /// The operation.
        op: crate::watches::Op,
    },
}

/// Makes the call `call` of the session of `record` on its riff-server,
/// and gives the answer as JSON (01M5GVPNAKAPA596K9361HSAAV). A session
/// that is not the lead cannot resume the riff.
async fn server_call(record: &Record, call: &ServerCall) -> Result<Value> {
    use riff_client::api::{Api, PauseScope};
    let api = Api::new(&record.server);
    let signed = || api.clone().signed_in(Some(&record.live.session));
    Ok(match call {
        ServerCall::Build => {
            let probe = api.probe(crate::lifecycle::PROBE_WAIT).await?;
            serde_json::to_value(probe.build.map(|b| b.version.to_string()))?
        }
        ServerCall::Who { all } => serde_json::to_value(signed()?.who(&record.me, *all).await?)?,
        ServerCall::Holds { thread } => {
            serde_json::to_value(signed()?.plan_show(&record.me, thread).await?.holds)?
        }
        ServerCall::ResumeRiff => {
            if !record.terminal {
                bail!(text::DAEMON_RESUME_ONLY_LEAD);
            }
            let state = riff_core::wire::RiffState::Running;
            let (reply, posted) = signed()?
                .set_pause(&record.me, &PauseScope::Riff, state)
                .await?;
            Value::String(text::riff_set(None, state, &reply, &posted))
        }
        ServerCall::PauseRiff => {
            if !record.terminal {
                bail!(text::DAEMON_PAUSE_ONLY_LEAD);
            }
            let state = riff_core::wire::RiffState::Paused;
            let (reply, posted) = signed()?
                .set_pause(&record.me, &PauseScope::Riff, state)
                .await?;
            Value::String(text::riff_set(None, state, &reply, &posted))
        }
        ServerCall::Roster => {
            let reply = signed()?.roster(&record.me, false).await?;
            serde_json::to_value(crate::reach::Roster {
                user: record.me.who().user().to_owned(),
                owner: reply.owner,
                people: reply.people,
            })?
        }
        ServerCall::ChatPost { body } => {
            let to = crate::chat::wakes(body, record.me.who().user());
            let thread = crate::chat::thread();
            let kind = riff_core::wire::Kind::Message;
            signed()?
                .post(&record.me, Some(&thread), &to, body, kind)
                .await?;
            Value::Null
        }
        ServerCall::ChatRead => {
            let lines = signed()?
                .read(&record.me, &crate::chat::thread(), true)
                .await?;
            serde_json::to_value(crate::cycle::chat_lines(lines))?
        }
        ServerCall::Host { host, request } => {
            if !record.terminal {
                bail!(text::DAEMON_HOST_ONLY_LEAD);
            }
            let request: remote::Request = request
                .parse()
                .map_err(|()| anyhow::anyhow!("the request {request:?} is unknown"))?;
            let wait = remote::REPLY_WAIT;
            let reply = remote::ask(&signed()?, &record.me, host, &request, wait).await?;
            Value::String(reply)
        }
        ServerCall::Whoami => {
            serde_json::to_value(crate::reach::WhoamiFacts::read(&signed()?, &record.me).await)?
        }
        ServerCall::WhoFacts { all } => {
            let facts = crate::reach::WhoFacts::read(&signed()?, &record.me, *all).await?;
            serde_json::to_value(facts)?
        }
        ServerCall::Members => serde_json::to_value(signed()?.members().await?)?,
        ServerCall::Log { repo } => {
            if !record.terminal {
                bail!(text::DAEMON_AUDIT_ONLY_LEAD);
            }
            serde_json::to_value(signed()?.log(repo).await?)?
        }
        ServerCall::Ends => {
            let records = crate::ends::read(&signed()?, &record.me).await?;
            serde_json::to_value(records)?
        }
        ServerCall::ReleaseNews { body } => {
            let woken = crate::hold::tell_leads(&signed()?, &record.me, true, body).await?;
            serde_json::to_value(woken.len())?
        }
        ServerCall::ReleaseHold { tag, reason } => {
            if !record.terminal {
                bail!(text::DAEMON_HOLD_ONLY_LEAD);
            }
            let api = signed()?;
            let held = match reason {
                Some(reason) => crate::hold::hold(&api, &record.me, tag, reason).await?,
                None => crate::hold::end(&api, &record.me, tag).await?,
            };
            serde_json::to_value(held)?
        }
        ServerCall::ReleaseLines => {
            let lines = crate::hold::read_as_person(&record.server, record.me.place()).await?;
            serde_json::to_value(lines)?
        }
        ServerCall::CloudLog { .. } => bail!("the daemon reads the log itself"),
        ServerCall::Run { .. } => bail!("the daemon runs the item itself"),
        ServerCall::Watch { .. } => bail!("the daemon watches itself"),
        ServerCall::Status => bail!("the daemon answers the status itself"),
        ServerCall::Events { .. } => bail!("the daemon answers the events itself"),
    })
}

/// The state of the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The process ID of the daemon.
    pub pid: u32,
    /// The build of the daemon.
    pub build: String,
    /// The start of the daemon, in milliseconds since the epoch.
    pub since_ms: i64,
    /// The folder of the daemon.
    pub dir: PathBuf,
    /// Its sessions, by name.
    pub sessions: Vec<Live>,
    /// The limit of the host ([`crate::room`]).
    #[serde(default)]
    pub limit: Option<Limit>,
    /// The starts that wait for room, in order.
    #[serde(default)]
    pub waiting: Vec<String>,
    /// The waves that the daemon runs, by repository.
    #[serde(default)]
    pub waves: Vec<WaveShown>,
    /// The jobs of the waves that run: from the pick of the item to the
    /// end of the steps after its session.
    #[serde(default)]
    pub jobs: usize,
    /// The real-session test that the daemon runs
    /// ([`release_test::running`], 01M5QVBTXE8YRY44A1C1VP71QS). An old
    /// daemon gives none.
    #[serde(default)]
    pub test: Option<riff_client::daemons::Test>,
    /// The MCP servers of each new session ([`crate::session_mcp`]).
    /// An old daemon gives none.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// The token of `claude setup-token` for the sessions
    /// (01M58B7HA8CRCW0MJ17QP3R5QG). An old daemon gives
    /// [`Token::Unknown`](riff_client::daemons::Token::Unknown).
    #[serde(default)]
    pub claude_token: riff_client::daemons::Token,
    /// The watches of the async operations that the daemon follows, but
    /// the watch of a wave ([`crate::watches`]). An old daemon gives none.
    #[serde(default)]
    pub watches: Vec<WatchShown>,
    /// The leads that lost their network at the restart of the daemon,
    /// while they run (01M5SMQBTA3HQXPKKK01ZGFB4H). An old daemon gives
    /// none.
    #[serde(default)]
    pub lost: Vec<LostShown>,
}

impl Status {
    /// The work of the daemon: its sessions but the lead, the starts of
    /// `riff run` that wait for room, the jobs of its waves, and the
    /// real-session test that runs (one, 01M5QVBTXE8YRY44A1C1VP71QS). The
    /// lead does not count: a new daemon takes it up with its container,
    /// its terminal and its socket ([`Daemon::recover`]). The daemon starts
    /// again on a new riff only at 0 (01M517BTJ9EV4ZGPYRB6HXJJC9).
    ///
    /// A start of a wave that waits for room does not count: the wave is
    /// in `waves.json`, and the new daemon waits for room for it again.
    /// With the limit 0 it waits always (01M5MS1Q78AFEY3ZVXNAQXM1FJ). A
    /// start of a wave has the name [`text::wave_slot`] of a wave in
    /// [`Status::waves`]. An old daemon gives these names too, so
    /// `riff update` decides the same for it.
    ///
    /// ```
    /// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
    ///     "pid": 7, "build": "1.4.0", "since_ms": 0, "dir": "/d", "sessions": [],
    /// }))?;
    /// assert_eq!(status.busy(), 0);
    /// let run = riff::daemon::Status { waiting: vec!["riff-issue-12".into()], ..status.clone() };
    /// assert_eq!(run.busy(), 1);
    /// let wave = riff::daemon::Status {
    ///     waiting: vec![riff::text::wave_slot("o/r")],
    ///     waves: vec![riff::daemon::WaveShown { repo: "o/r".into(), word: "waits".into() }],
    ///     ..status.clone()
    /// };
    /// assert_eq!(wave.busy(), 0);
    /// assert_eq!(riff::daemon::Status { jobs: 2, ..status.clone() }.busy(), 2);
    /// // A real-session test that runs counts as one.
    /// let test = riff_client::daemons::Test {
    ///     tag: "v4.1.1".into(),
    ///     commit: "90e36bfd".into(),
    ///     secs: 0,
    /// };
    /// assert_eq!(riff::daemon::Status { test: Some(test), ..status }.busy(), 1);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn busy(&self) -> usize {
        let of_a_wave = |name: &String| {
            self.waves
                .iter()
                .any(|wave| text::wave_slot(&wave.repo) == *name)
        };
        let runs = self.waiting.iter().filter(|name| !of_a_wave(name)).count();
        let works = self.sessions.iter().filter(|live| !live.lead).count();
        works + runs + self.jobs + usize::from(self.test.is_some())
    }
}

/// A watch of the daemon, as `riff daemon status` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchShown {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// What the watch waits for, in words ([`crate::watches::Op::what`]).
    pub what: String,
    /// The start of the watch, in milliseconds since the epoch.
    pub since_ms: i64,
}

/// A lead with no network, as `riff daemon status` shows it
/// ([`crate::text::lead_lost_network`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LostShown {
    /// The repository of the lead, as `OWNER/REPO`.
    pub repo: String,
    /// The container of the lead.
    pub name: String,
}

/// A wave that the daemon runs, as `riff daemon status` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveShown {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// What the wave did last, in a few words.
    pub word: String,
}

/// How a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ended {
    /// The exit code of `claude`, or none after a signal.
    pub code: Option<i32>,
    /// True when a turn of `claude` ended with an error, a stop ended the
    /// session, or the container ended with no exit of the relay.
    pub failed: bool,
}

/// What the daemon says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case")]
pub enum Say {
    /// A line for the person.
    Line {
        /// The text.
        text: String,
    },
    /// The session started.
    Started {
        /// The name of its container.
        name: String,
        /// The folder of its socket on the host.
        socket: PathBuf,
    },
    /// One event of `claude`, with no secret.
    Event {
        /// Its sequence number: its line in `events.jsonl`.
        seq: u64,
        /// The line.
        line: String,
    },
    /// One line of the output of the container, with no secret: the
    /// notes of the relay and the stderr of `claude`.
    Stderr {
        /// The line.
        line: String,
    },
    /// The session ended.
    Ended(Ended),
    /// The state of the daemon.
    Status(Box<Status>),
    /// On the socket of a session: who that session is.
    Session {
        /// The riff session ID.
        session: String,
        /// The work item.
        item: String,
        /// The name of the container.
        name: String,
    },
    /// To the relay: the daemon has this many events.
    Resume {
        /// The number of events in `events.jsonl`.
        events: u64,
    },
    /// To the relay: one line for the stdin of `claude`.
    Input {
        /// The line.
        line: String,
    },
    /// To the relay: close the stdin of `claude`.
    Close,
    /// To the relay: the daemon has each line up to this number.
    Ack {
        /// The sequence number.
        seq: u64,
    },
    /// To the relay: a takeover. Close the stdin of `claude`, and keep
    /// the container at its end.
    Hold {
        /// True to send `claude` an interrupt before the close: it ends
        /// its turn at once (01M51KJ1V0STB4QCN4N5SBYGSP,
        /// 01M5RG6C25DB49ZKM03Y93QB1F). Else the relay waits for the end
        /// of the turn.
        interrupt: bool,
    },
    /// To the relay: the takeover ended. Start `claude` again.
    GoOn,
    /// To `riff takeover`: `claude` ended its turn. Run the terminal of
    /// the person now.
    Held {
        /// The riff session ID: `claude --resume` takes it.
        session: String,
    },
    /// The ask is done.
    Done,
    /// To `riff relay denied --send`: the daemon posted this many
    /// refusals, the first ones.
    Posted {
        /// The number of refusals.
        count: usize,
    },
    /// To `riff release`: the session on this socket, as the daemon
    /// knows it.
    Caller {
        /// True for the lead: the session with a terminal.
        lead: bool,
        /// The role of the user of the session in the riff. `None` in a
        /// riff with no sign-in.
        role: Option<riff_core::wire::PersonRole>,
        /// True when the daemon could not ask riff-server for the role.
        role_unknown: bool,
    },
    /// The answer of riff-server to a [`SessionAsk::Server`], as JSON.
    Served {
        /// The answer.
        reply: Value,
    },
    /// The ask failed.
    Failed {
        /// Why.
        error: String,
    },
}

/// The lines that the daemon says on one connection. A send never
/// waits: a task writes the lines. When `riff` left, the lines go
/// nowhere, and the session goes on.
#[derive(Clone)]
pub struct Out(mpsc::UnboundedSender<Say>);

impl Out {
    /// Says `say`.
    pub fn say(&self, say: Say) {
        let _ = self.0.send(say);
    }

    /// An `Out` that keeps each line, for a test.
    pub fn keep() -> (Out, mpsc::UnboundedReceiver<Say>) {
        let (send, read) = mpsc::unbounded_channel();
        (Out(send), read)
    }

    /// True while its connection is open.
    fn open(&self) -> bool {
        !self.0.is_closed()
    }

    /// True when `other` says on the same connection.
    fn same(&self, other: &Out) -> bool {
        self.0.same_channel(&other.0)
    }
}

/// The [`Out`] of the write half `write`, and its task. The task ends
/// when each [`Out`] is gone and each line is written.
fn writer(mut write: OwnedWriteHalf) -> (Out, JoinHandle<()>) {
    let (out, mut read) = Out::keep();
    let task = tokio::spawn(async move {
        while let Some(say) = read.recv().await {
            let Ok(mut line) = serde_json::to_string(&say) else {
                continue;
            };
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
        let _ = write.shutdown().await;
    });
    (out, task)
}

/// The time now in milliseconds since the epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `path` with each link resolved, when it exists.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// What the daemon keeps on disk of a live session, in `live/NAME.json`
/// of its folder. It holds no secret (01M4ZHT01HH88KPA8628M7VBEY).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The session.
    pub live: Live,
    /// The folder of the session: `events.jsonl` is there.
    pub folder: PathBuf,
    /// The riff-server of the session: its riff tools act there.
    pub server: String,
    /// The URI of the session.
    pub me: SessionUri,
    /// Each input for `claude`, in order: one stream-json line each.
    pub inputs: Vec<String>,
    /// True when the input of `claude` closes after the last input.
    pub closed: bool,
    /// True when a turn ended with an error.
    pub failed: bool,
    /// The job of a session that the daemon started for a wave: the
    /// daemon does its steps after the end, also after a restart
    /// ([`crate::wave`]). `None` for a session of `riff run`.
    #[serde(default)]
    pub job: Option<Job>,
    /// True for a session with a terminal: the lead.
    #[serde(default)]
    pub terminal: bool,
    /// True from the ask of a takeover until the daemon gives the
    /// session back.
    #[serde(default)]
    pub takeover: bool,
    /// True from the ask of a takeover that interrupts the turn until
    /// the session goes on: the result of that turn is no fail
    /// (01M51KJ1R9K7A7RW8T8XH8VW43).
    #[serde(default)]
    pub interrupt: bool,
}

/// What a link of a session tells the task that waits for its end.
enum Note {
    /// The relay sent the exit of `claude`, and the daemon took it.
    Exit(Option<i32>),
    /// The connection of the relay ended.
    Gone,
}

/// A session that the daemon owns, in memory.
struct Owned {
    /// The name of its container.
    name: String,
    /// The folder of the daemon.
    dir: PathBuf,
    /// The folder of its socket.
    socket: PathBuf,
    /// Its [`Record`] on disk.
    record_path: PathBuf,
    /// The secrets that no event shows. A daemon that takes up a session
    /// after a restart knows none: the relay took them out.
    scrub: Scrub,
    state: Mutex<SessionState>,
    /// The links tell the task of the end.
    notes: mpsc::UnboundedSender<Note>,
    /// A `riff` asked to stop the container.
    stop: Notify,
    /// Look at the container now: the daemon stopped the container of a lead
    /// that it took up ([`Daemon::lead_again`]), and does not wait for
    /// [`LOOK_EVERY`].
    look: Notify,
    /// True while the daemon stops the container of a lead to start it again
    /// on a new riff: the end of the lead does not end its session on
    /// riff-server ([`Daemon::leads_again`]).
    renewing: std::sync::atomic::AtomicBool,
    /// The quiet limit of the session moved: the input of `claude`
    /// closed, or the number of its background tasks changed.
    closing: Notify,
    /// The task of the socket of the session.
    door: Mutex<Option<JoinHandle<()>>>,
}

/// The state of an [`Owned`] session.
struct SessionState {
    record: Record,
    /// The number of events in `events.jsonl`.
    seq: u64,
    /// The time of the last event, or of the start of the ownership.
    last: tokio::time::Instant,
    /// The number of background tasks that `claude` runs, from its last
    /// `background_tasks_changed` event.
    tasks: usize,
    log: std::fs::File,
    /// The connections of `riff` that follow the session.
    followers: Vec<Out>,
    /// The connection of the relay.
    relay: Option<Out>,
    /// The protocol of the last relay that connected, or 0.
    relay_protocol: u32,
    /// True when `claude` ended for a takeover, and the relay waits.
    held: bool,
    /// The connection of `riff takeover`.
    taker: Option<Out>,
    /// The last step of the session ([`Live::last`]).
    last_step: String,
    /// True after the end of the ownership: its record is gone.
    ended: bool,
    /// How the session ended, from the take of its followers: a later
    /// follower gets it at once.
    end: Option<Ended>,
    /// The input of each wake during a takeover: `claude` gets them
    /// after the go on.
    waiting: Vec<String>,
    /// Each line of the daemon about the session ([`Say::Line`]), with
    /// the number of events before it: a new follower gets each one in
    /// its place (01M57GCZXZZRPGGWC5HZS49DGM).
    lines: Vec<(u64, String)>,
}

impl SessionState {
    /// What the session does now.
    fn phase(&self) -> Phase {
        let record = &self.record;
        if record.terminal {
            Phase::Terminal
        } else if record.takeover && self.held {
            Phase::Taken
        } else if record.takeover {
            Phase::Holds
        } else if record.closed {
            Phase::Ends
        } else {
            Phase::Works
        }
    }

    /// Gives the session back after a takeover, when `claude` ended for
    /// it: the relay starts `claude` again, with one new input.
    fn go_on(&mut self) {
        if !self.held || self.record.takeover {
            return;
        }
        self.held = false;
        let line = user_line(text::TAKEOVER_GO_ON);
        self.record.inputs.push(line.clone());
        self.record.closed = false;
        self.record.interrupt = false;
        if let Some(relay) = &self.relay {
            relay.say(Say::GoOn);
            relay.say(Say::Input { line });
        }
        self.give_waiting();
    }

    /// Gives `claude` the input `line`: the relay writes it on its stdin.
    fn give(&mut self, line: String) {
        self.record.inputs.push(line.clone());
        if let Some(relay) = &self.relay {
            relay.say(Say::Input { line });
        }
    }

    /// Gives `claude` the input of each wake of the takeover.
    fn give_waiting(&mut self) {
        for line in std::mem::take(&mut self.waiting) {
            self.give(line);
        }
    }

    /// Gives `claude` the wake `wake` as one input, while its input is
    /// open. During a takeover, the input waits for the go on. False
    /// when the input closed: the session ended its work
    /// (01M5792DP1KZR2XGSCMRWDHCGZ).
    fn wake(&mut self, wake: &Wake) -> bool {
        let line = user_line(&text::wake_line(wake));
        if self.record.takeover || self.held {
            self.waiting.push(line);
        } else if self.record.closed {
            return false;
        } else {
            self.give(line);
        }
        true
    }

    /// The session as `riff daemon status` shows it.
    fn live(&self) -> Live {
        Live {
            phase: self.phase(),
            last: self.last_step.clone(),
            ..self.record.live.clone()
        }
    }
}

impl Owned {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the record of `state` in place of the one on disk. After
    /// the end of the ownership, it writes nothing: a record of a
    /// session that ended would make a new daemon do its end again.
    fn save(&self, state: &SessionState) -> Result<()> {
        if state.ended {
            return Ok(());
        }
        let tmp = self.record_path.with_extension("json.new");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&state.record)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.record_path)
            .with_context(|| format!("cannot write {}", self.record_path.display()))
    }

    /// Says `say` to each `riff` that follows the session.
    fn tell(state: &mut SessionState, say: &Say) {
        state.followers.retain(Out::open);
        for out in &state.followers {
            out.say(say.clone());
        }
    }

    /// Says the line `text` of the daemon about the session to each
    /// `riff` that follows it, and keeps it for each later follower.
    fn say_line(state: &mut SessionState, text: String) {
        state.lines.push((state.seq, text.clone()));
        Self::tell(state, &Say::Line { text });
    }

    /// `claude` of the session refused its sign-in. With a token set, the
    /// daemon marks the token refused, so that it starts no new session,
    /// and says why in the lines of the session
    /// (01M58B7H7HD0H48A0C79F8M4CR).
    fn token_refused(&self, state: &mut SessionState) {
        if !crate::claude_token::held() {
            return;
        }
        if !crate::claude_token::refused(&self.dir) {
            if let Err(e) = crate::claude_token::refuse(&self.dir) {
                eprintln!("riff daemon: {e:#}");
            }
            eprintln!("riff daemon: {}: {}", self.name, text::CLAUDE_TOKEN_REFUSED);
        }
        Self::say_line(state, text::CLAUDE_TOKEN_REFUSED.to_owned());
    }

    /// Takes the event `seq` of the relay, and gives the number of
    /// events that the daemon has. The daemon takes only the next event:
    /// it has each one before it.
    fn event(&self, seq: u64, line: &str) -> u64 {
        let mut state = self.lock();
        if seq != state.seq + 1 {
            return state.seq;
        }
        let line = self.scrub.clean(line).into_owned();
        if state.log.write_all(format!("{line}\n").as_bytes()).is_err() {
            return state.seq;
        }
        state.seq = seq;
        state.last = tokio::time::Instant::now();
        let event = serde_json::from_str::<Value>(&line).ok();
        if let Some(tasks) = event.as_ref().and_then(background_tasks)
            && tasks != state.tasks
        {
            // The limit depends on the tasks: the watch counts again.
            state.tasks = tasks;
            self.closing.notify_one();
        }
        if let Some(step) = event
            .as_ref()
            .and_then(|e| crate::run::event_lines(e).pop())
        {
            state.last_step = step;
        }
        if event.as_ref().is_some_and(crate::claude_token::refusal) {
            self.token_refused(&mut state);
        }
        if let Some(event) = event
            && event["type"] == "result"
        {
            // A turn that a takeover interrupts ends with an error: that
            // is no fail of the session.
            state.record.failed |= event["is_error"] == true && !state.record.interrupt;
            // The end of the one turn: claude ends when its input
            // closes.
            if !state.record.closed {
                state.record.closed = true;
                if let Some(relay) = &state.relay {
                    relay.say(Say::Close);
                }
                self.closing.notify_one();
            }
            if let Err(e) = self.save(&state) {
                eprintln!("riff daemon: {e:#}");
            }
        }
        Self::tell(&mut state, &Say::Event { seq, line });
        seq
    }

    /// Follows the session on `out`: each event after `after` and each
    /// line of the daemon in its place ([`replay`]), then each new one.
    /// After the take of the followers, it says the end at once.
    fn follow(&self, out: Out, after: u64) -> Result<()> {
        let mut state = self.lock();
        let log = if after < state.seq {
            let events = state.record.folder.join(EVENTS);
            std::fs::read_to_string(&events)
                .with_context(|| format!("cannot read {}", events.display()))?
        } else {
            String::new()
        };
        replay(&out, &log, state.seq, after, &state.lines);
        match state.end {
            Some(ended) => out.say(Say::Ended(ended)),
            None => state.followers.push(out),
        }
        Ok(())
    }

    /// Relays the session on the connection of its relay: the hello said
    /// `protocol`, `inputs`, `closed` and `held`.
    async fn relay(
        &self,
        protocol: u32,
        (inputs, closed, held): (u64, bool, bool),
        mut lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    ) {
        let (out, written) = writer(write);
        {
            let mut state = self.lock();
            out.say(Say::Resume { events: state.seq });
            state.relay_protocol = protocol;
            state.held = held;
            // A takeover that ended while the relay was away: go on
            // first, so that the new input reaches the new claude.
            let gone_on = held && !state.record.takeover;
            if gone_on {
                out.say(Say::GoOn);
                state.held = false;
                state.record.interrupt = false;
            }
            let given = usize::try_from(inputs).unwrap_or(usize::MAX);
            for line in state.record.inputs.iter().skip(given) {
                out.say(Say::Input { line: line.clone() });
            }
            if gone_on && usize::try_from(inputs).is_ok_and(|n| n >= state.record.inputs.len()) {
                let line = user_line(text::TAKEOVER_GO_ON);
                state.record.inputs.push(line.clone());
                out.say(Say::Input { line });
                state.record.closed = false;
            }
            if gone_on {
                for line in std::mem::take(&mut state.waiting) {
                    state.record.inputs.push(line.clone());
                    out.say(Say::Input { line });
                }
            }
            if state.record.closed && !closed && !gone_on {
                out.say(Say::Close);
            }
            if state.record.takeover && !held {
                out.say(Say::Hold {
                    interrupt: state.record.interrupt,
                });
            }
            if held
                && state.record.takeover
                && let Some(taker) = &state.taker
            {
                taker.say(Say::Held {
                    session: state.record.live.session.clone(),
                });
            }
            state.relay = Some(out.clone());
            if let Err(e) = self.save(&state) {
                eprintln!("riff daemon: {e:#}");
            }
        }
        while let Ok(Some(line)) = lines.next_line().await {
            // A line of a newer relay that this daemon does not know:
            // skip it.
            match serde_json::from_str::<Up>(&line) {
                Ok(Up::Event { seq, line }) => {
                    let has = self.event(seq, &line);
                    if has < seq {
                        // The daemon could not keep the event: the relay
                        // connects again and sends it again.
                        break;
                    }
                    out.say(Say::Ack { seq: has });
                }
                Ok(Up::Exit { seq, code }) => {
                    // The exit counts only after each event before it.
                    if self.lock().seq + 1 < seq {
                        break;
                    }
                    out.say(Say::Ack { seq });
                    let _ = self.notes.send(Note::Exit(code));
                }
                Ok(Up::Held) => {
                    let mut state = self.lock();
                    state.held = true;
                    if state.record.takeover {
                        if let Some(taker) = &state.taker {
                            taker.say(Say::Held {
                                session: state.record.live.session.clone(),
                            });
                        }
                    } else {
                        // The person left before the end of the turn.
                        state.go_on();
                        if let Err(e) = self.save(&state) {
                            eprintln!("riff daemon: {e:#}");
                        }
                    }
                }
                Err(_) => {}
            }
        }
        {
            let mut state = self.lock();
            if state.relay.as_ref().is_some_and(|r| r.same(&out)) {
                state.relay = None;
            }
        }
        let _ = self.notes.send(Note::Gone);
        drop(out);
        let _ = written.await;
    }
}

/// The number of background tasks that the event `event` of `claude`
/// lists, or `None` when it is not a `background_tasks_changed` event.
///
/// ```
/// use riff::daemon::background_tasks;
/// use serde_json::json;
///
/// let one = json!({"type":"system","subtype":"background_tasks_changed",
///     "tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]});
/// assert_eq!(background_tasks(&one), Some(1));
/// let none = json!({"type":"system","subtype":"background_tasks_changed","tasks":[]});
/// assert_eq!(background_tasks(&none), Some(0));
/// assert_eq!(background_tasks(&json!({"type":"result"})), None);
/// ```
pub fn background_tasks(event: &Value) -> Option<usize> {
    (event["type"] == "system" && event["subtype"] == "background_tasks_changed")
        .then(|| event["tasks"].as_array().map_or(0, Vec::len))
}

/// The number of background tasks in the last `background_tasks_changed`
/// event of the log `path`: 0 when there is none.
fn tasks_in(path: &Path) -> usize {
    std::fs::read_to_string(path).map_or(0, |log| {
        log.lines()
            .rev()
            .find_map(|line| background_tasks(&serde_json::from_str::<Value>(line).ok()?))
            .unwrap_or(0)
    })
}

/// The number of lines of the file `path`: 0 when it is not there.
fn lines_of(path: &Path) -> u64 {
    std::fs::read(path).map_or(0, |bytes| {
        bytes.iter().filter(|&&b| b == b'\n').count() as u64
    })
}

/// Says on `out` each event of the log `log` after the event `after`,
/// up to the event `seq`, and each line of `lines` that came after the
/// event `after`, in its place (01M57GCZXZZRPGGWC5HZS49DGM).
fn replay(out: &Out, log: &str, seq: u64, after: u64, lines: &[(u64, String)]) {
    let mut lines = lines.iter().filter(|(at, _)| *at >= after).peekable();
    for (n, line) in (1..=seq).zip(log.lines()) {
        while let Some((_, text)) = lines.next_if(|(at, _)| *at < n) {
            out.say(Say::Line { text: text.clone() });
        }
        if n > after {
            out.say(Say::Event {
                seq: n,
                line: line.to_owned(),
            });
        }
    }
    for (_, text) in lines {
        out.say(Say::Line { text: text.clone() });
    }
}

/// The end of a session, which the daemon keeps for [`KEEP_END`] after
/// the end (01M57GCZVJAHAMPEYY1T6GA48P). A follow that comes after the
/// end, for example the next page of `riff daemon events --host`, gets
/// what a follower that stayed got. The folder of the session can go at
/// its end, so the daemon keeps the log itself.
struct Gone {
    /// The log of the events, with no secret.
    log: String,
    /// The number of events.
    seq: u64,
    /// Each line of the daemon about the session.
    lines: Vec<(u64, String)>,
    /// How the session ended.
    ended: Ended,
    /// The time of the end.
    at: tokio::time::Instant,
}

impl Gone {
    /// Says on `out` each event after `after`, each line in its place,
    /// then the end.
    fn follow(&self, out: &Out, after: u64) {
        replay(out, &self.log, self.seq, after, &self.lines);
        out.say(Say::Ended(self.ended));
    }
}

/// The daemon: the sessions of this machine, on the runtime `R`.
pub struct Daemon<R> {
    runtime: R,
    dir: PathBuf,
    since_ms: i64,
    /// The binary that the daemon binds into each session.
    relay_bin: PathBuf,
    /// True when [`crate::release::DYNAMIC`] lets the daemon bind a
    /// binary that is not static: a test.
    any_relay: bool,
    /// The quiet limit of each session ([`QUIET_LIMIT`]).
    quiet: Duration,
    /// The quiet limit of a session of a wave ([`EXIT_GRACE`]).
    grace: Duration,
    live: Mutex<BTreeMap<String, Arc<Owned>>>,
    /// The end of each session that ended in the last `keep_end`.
    gone: Mutex<BTreeMap<String, Gone>>,
    /// How long the daemon keeps the end of a session ([`KEEP_END`]).
    keep_end: Duration,
    /// The limit of the host, and the starts that wait for room.
    room: Arc<Room>,
    /// The forge and the riff of the waves ([`crate::wave::Live`]).
    world: Arc<dyn World>,
    /// The waves that run, by repository.
    waves: Mutex<BTreeMap<String, Running>>,
    /// The time between two looks of a wave at `main` and the pull requests
    /// ([`crate::look::LOOK_EVERY`]).
    look_every: Duration,
    /// A session ended, or a job of a wave ended: the waves look again.
    changed: Notify,
    /// The folders of the sessions: the jobs that run, and one sweep at
    /// a time ([`sweep`]).
    folders: Arc<sweep::Folders>,
    /// The new start on a new riff, for the lead of another host. With
    /// none, the daemon has no session on riff-server: a test
    /// ([`remote`]).
    again: Option<Arc<remote::Again>>,
    /// The task of the session on riff-server of each repository that
    /// the daemon serves ([`remote::serve`]).
    serving: Mutex<BTreeMap<String, (remote::Served, JoinHandle<()>)>>,
    /// The notes for the lead of each repository that a session of
    /// [`remote::serve`] sends ([`facts::News`]).
    news: facts::News,
    /// The reason of the last end of each item of a wave ([`crate::ends`]).
    ends: Mutex<BTreeMap<String, String>>,
    /// The pull of the session image that failed for the wave of each
    /// repository, until a pull works (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
    pulls: Mutex<BTreeMap<String, Pulled>>,
    /// The `gcloud` of the host, for [`ServerCall::CloudLog`].
    gcloud: Arc<crate::cloud::Gcloud>,
    /// The session of each lead whose stale sign-in the person heard of
    /// in riff chat ([`Daemon::stale_leads`]).
    stale: Mutex<std::collections::BTreeSet<String>>,
    /// How long the transcript of a lead stays quiet before the lead counts as
    /// idle ([`LEAD_IDLE`], [`crate::lead::quiet`]).
    lead_idle: Duration,
    /// The session of each lead whose start again on the new riff waits, and
    /// the person heard of it in riff chat ([`Daemon::leads_again`]).
    again_told: Mutex<std::collections::BTreeSet<String>>,
    /// The session of each lead that lost its network at the restart of the
    /// daemon: the person heard of it in riff chat, and `riff daemon status`
    /// shows it ([`Daemon::recover`]).
    lost: Mutex<std::collections::BTreeSet<String>>,
    /// The repositories and images whose tools the daemon looked at since it
    /// started ([`Daemon::check_tools`]).
    looked: Mutex<BTreeSet<String>>,
    /// The forge and the posts of the watches ([`crate::watches`]).
    lookout: Arc<dyn crate::watches::Lookout>,
}

/// A pull of the session image that failed. [`Daemon::have_image`] gives
/// it as its error, so a wave can tell it from each other fault
/// (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
#[derive(Debug)]
pub(crate) struct PullFailed {
    /// The image.
    pub(crate) image: String,
    /// Why podman could not pull it.
    pub(crate) reason: String,
}

impl std::fmt::Display for PullFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", text::run_no_image(&self.image), self.reason)
    }
}

impl std::error::Error for PullFailed {}

/// The last failed pull of the wave of a repository.
struct Pulled {
    image: String,
    reason: String,
    /// When it failed.
    at: std::time::Instant,
}

/// A failed pull as the wave reads it ([`Daemon::failed_pull`]).
pub(crate) struct FailedPull {
    /// The image.
    pub(crate) image: String,
    /// Why podman could not pull it.
    pub(crate) reason: String,
    /// True while the wave waits for its next try.
    pub(crate) waits: bool,
}

/// A wave that runs in the daemon.
struct Running {
    /// The wave.
    wave: Wave,
    /// The task of its loop.
    task: JoinHandle<()>,
    /// The task of its look ([`crate::wave::watch`]).
    look: JoinHandle<()>,
    /// The items of its jobs that the daemon does not own yet.
    taken: Arc<Mutex<std::collections::BTreeSet<String>>>,
    /// The starts of each item in this run of the wave.
    starts: crate::wave::Starts,
    /// What it did last.
    word: Arc<Mutex<String>>,
}

impl Running {
    /// Ends its loop and its look. Its jobs go on.
    fn abort(&self) {
        self.task.abort();
        self.look.abort();
    }
}

/// Binds the control socket in the folder `dir` of the daemon. It
/// refuses when a daemon answers there. It takes the place of a socket
/// that no daemon answers.
pub async fn listen(dir: &Path) -> Result<UnixListener> {
    private_dir(dir)?;
    let path = socket(dir);
    if std::fs::symlink_metadata(&path).is_ok() {
        if let Ok(status) = status(dir).await {
            bail!("{}", text::daemon_runs_already(status.pid, &path));
        }
        std::fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
    }
    bind_in(dir, SOCKET)
}

impl<R: Runtime + 'static> Daemon<R> {
    /// A daemon with no session, on `runtime`, with its folder `dir`. It
    /// binds its own binary into each session ([`own_binary`]). Only one
    /// daemon may open a folder: call [`listen`] first. Its quiet limit
    /// is [`QUIET_LIMIT`], or the value of [`QUIET_VAR`].
    pub fn new(runtime: R, dir: PathBuf) -> Result<Self> {
        private_dir(&dir)?;
        let room = Room::new(dir.clone(), Arc::new(crate::room::free_memory));
        Ok(Daemon {
            runtime,
            dir,
            since_ms: now_ms(),
            relay_bin: own_binary().unwrap_or_else(|_| PathBuf::from("riff")),
            any_relay: crate::release::takes_dynamic(),
            quiet: quiet_limit(),
            grace: quiet_limit().min(EXIT_GRACE),
            live: Mutex::new(BTreeMap::new()),
            gone: Mutex::new(BTreeMap::new()),
            keep_end: KEEP_END,
            room,
            world: Arc::new(crate::wave::Live::default()),
            waves: Mutex::new(BTreeMap::new()),
            look_every: crate::look::LOOK_EVERY,
            changed: Notify::new(),
            folders: Arc::default(),
            again: None,
            serving: Mutex::new(BTreeMap::new()),
            news: facts::News::default(),
            ends: Mutex::new(BTreeMap::new()),
            pulls: Mutex::new(BTreeMap::new()),
            gcloud: Arc::default(),
            stale: Mutex::default(),
            lead_idle: lead_idle(),
            again_told: Mutex::default(),
            lost: Mutex::default(),
            looked: Mutex::default(),
            lookout: Arc::new(crate::watches::Here),
        })
    }

    /// The daemon with the quiet limit `quiet` in place of
    /// [`QUIET_LIMIT`].
    #[must_use]
    pub fn with_quiet(mut self, quiet: Duration) -> Self {
        self.quiet = quiet;
        self.grace = quiet.min(EXIT_GRACE);
        self
    }

    /// The daemon with the grace `grace` of the sessions of a wave in
    /// place of [`EXIT_GRACE`], for a test.
    #[must_use]
    pub fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// The daemon that counts a lead as idle after `idle` in place of
    /// [`LEAD_IDLE`], for a test.
    #[must_use]
    pub fn with_lead_idle(mut self, idle: Duration) -> Self {
        self.lead_idle = idle;
        self
    }

    /// The daemon that keeps the end of a session for `keep` in place of
    /// [`KEEP_END`], for a test.
    #[must_use]
    pub fn with_keep_end(mut self, keep: Duration) -> Self {
        self.keep_end = keep;
        self
    }

    /// The daemon with the free memory of `memory`, for a test.
    #[must_use]
    pub fn with_memory(mut self, memory: crate::room::Memory) -> Self {
        self.room = Room::new(self.dir.clone(), memory);
        self
    }

    /// The daemon that runs `gcloud` at `gcloud`, for a test.
    #[must_use]
    pub fn with_gcloud(mut self, gcloud: crate::cloud::Gcloud) -> Self {
        self.gcloud = Arc::new(gcloud);
        self
    }

    /// The daemon that binds `riff` into each session in place of its
    /// own binary, for a test.
    #[must_use]
    pub fn with_relay(mut self, riff: PathBuf) -> Self {
        self.relay_bin = riff;
        self
    }

    /// The daemon with the forge and the riff of `world` for its waves,
    /// for a test.
    #[must_use]
    pub fn with_world(mut self, world: Arc<dyn World>) -> Self {
        self.world = world;
        self
    }

    /// The daemon whose watches look at the forge and post through
    /// `lookout`, for a test ([`crate::watches`]).
    #[must_use]
    pub fn with_lookout(mut self, lookout: Arc<dyn crate::watches::Lookout>) -> Self {
        self.lookout = lookout;
        self
    }

    /// The daemon whose waves look at `main` and the pull requests each
    /// `every` in place of [`crate::look::LOOK_EVERY`], for a test.
    #[must_use]
    pub fn with_look_every(mut self, every: Duration) -> Self {
        self.look_every = every;
        self
    }

    /// The time between two looks of a wave at `main` and the pull
    /// requests.
    pub(crate) fn look_every(&self) -> Duration {
        self.look_every
    }

    /// The daemon with a session on riff-server for each repository that
    /// it serves, and the new start of `again` ([`remote`]).
    #[must_use]
    pub fn with_again(mut self, again: remote::Again) -> Self {
        self.again = Some(Arc::new(again));
        self
    }

    /// Serves the lead of the repository of `wave` from now on: it keeps
    /// it in [`remote::SERVED_FILE`], and starts its session on
    /// riff-server, or starts it again with the new facts of `wave`
    /// (01M579Z9JE6HY9HG3PWR1RR0RQ).
    pub fn serve_repo(&self, wave: &Wave) -> Result<()> {
        let served = remote::serve_in_file(&self.dir, wave)?;
        self.spawn_served(served);
        Ok(())
    }

    /// Starts the session on riff-server of each repository of
    /// [`remote::SERVED_FILE`]. A repository whose wave the daemon runs
    /// ([`crate::wave::WAVES_FILE`]) and that is not there yet goes in
    /// first, with the server of the wave.
    pub fn serve_repos(&self) {
        match crate::wave::read_waves(&self.dir) {
            Ok(waves) => {
                let served = remote::read_served(&self.dir).unwrap_or_default();
                for wave in waves.values().filter(|w| !served.contains_key(&w.repo)) {
                    if let Err(e) = remote::serve_in_file(&self.dir, wave) {
                        eprintln!("riff daemon: {e:#}");
                    }
                }
            }
            Err(e) => eprintln!("riff daemon: {e:#}"),
        }
        match remote::read_served(&self.dir) {
            Ok(served) => served.into_values().for_each(|s| self.spawn_served(s)),
            Err(e) => eprintln!("riff daemon: {e:#}"),
        }
    }

    /// Starts the session of `served`, when it does not run with the
    /// same facts.
    fn spawn_served(&self, served: remote::Served) {
        if self.again.is_none() {
            return;
        }
        let mut serving = self.serving.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((old, task)) = serving.get(&served.wave.repo) {
            if *old == served && !task.is_finished() {
                return;
            }
            task.abort();
        }
        let news = self.news.clone();
        let task = tokio::spawn(remote::serve(self.dir.clone(), served.clone(), news));
        serving.insert(served.wave.repo.clone(), (served, task));
    }

    /// `riff daemon restart`: when the daemon runs an old riff and is
    /// idle ([`Status::busy`]), it asks systemd to start it again after
    /// [`RESTART_DELAY`], so the reply reaches the lead of another host
    /// first (01M579Z9SAN24QGN1VV5C7EHM9). It gives the line of the
    /// decision.
    pub async fn restart_now(&self) -> String {
        let status = self.status().await;
        let old = self.again.as_ref().is_some_and(|again| (again.old)());
        let decided = remote::Restart::of(old, status.busy());
        if decided == remote::Restart::Now
            && let Some(again) = self.again.clone()
        {
            let (world, records) = (self.world.clone(), self.records());
            tokio::spawn(async move {
                // The leads hear of it first (01M5SNV7H3739PXW73H14XM4B8).
                tell_leads_restart(&world, &records).await;
                tokio::time::sleep(RESTART_DELAY).await;
                if let Err(e) = (again.restart)() {
                    let error = text::daemon_no_restart(&format!("{e:#}"));
                    eprintln!("riff daemon: {error}");
                }
            });
        }
        text::daemon_restart(decided, &status.build)
    }

    /// Ends the session in the container `name` for a person: it frees
    /// the item of the session on riff-server at once, then stops the
    /// container (01M579Z9VRD12FRK6DSYVKDHQB). It refuses the lead: a
    /// person ends it in its terminal. It gives a line for each step.
    pub async fn end_session(&self, name: &str) -> Result<Vec<String>> {
        let Some(owned) = self.lock().get(name).cloned() else {
            bail!("{}", text::daemon_no_session(name));
        };
        let record = owned.lock().record.clone();
        if record.terminal {
            bail!("{}", text::daemon_end_lead(name));
        }
        let item = &record.live.item;
        let freed = match self.world.free(&record).await {
            Ok(()) => text::daemon_freed(item),
            Err(e) => text::daemon_not_freed(item, &format!("{e:#}")),
        };
        owned.stop.notify_one();
        Ok(vec![freed, text::daemon_ends(name)])
    }

    /// The folder of the daemon.
    /// Keeps `reason` as the reason of the last end of `item`.
    pub(crate) fn note_end(&self, item: &str, reason: String) {
        self.ends
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(item.to_owned(), reason);
    }

    /// The reason of the last end of `item` that this daemon saw.
    pub(crate) fn last_end(&self, item: &str) -> Option<String> {
        self.ends
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(item)
            .cloned()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The limit of the host and the line of the starts that wait.
    pub fn room(&self) -> &Arc<Room> {
        &self.room
    }

    /// The world of the waves and of the sessions: the forge and
    /// riff-server ([`crate::wave::Live`]), or the world of a test.
    pub(crate) fn world(&self) -> Arc<dyn World> {
        self.world.clone()
    }

    /// The number of sessions that the daemon owns, but the sessions
    /// with a terminal: the lead counts in no limit.
    pub fn live_count(&self) -> usize {
        self.lock()
            .values()
            .filter(|owned| !owned.lock().record.terminal)
            .count()
    }

    /// The record of each session that the daemon owns.
    pub fn records(&self) -> Vec<Record> {
        self.lock()
            .values()
            .map(|owned| owned.lock().record.clone())
            .collect()
    }

    /// Tells the person in riff chat, once for each lead, that the
    /// sign-in of `claude` in the lead is stale
    /// (01M5NJ87YEHHGJBANM7CRYV56Z): the lead holds the sign-in file of
    /// the host, and the host replaced it ([`crate::lead::stale`]).
    /// `home` is the home of the person. It gives a line for each lead
    /// that it told of.
    pub async fn stale_leads(&self, home: &Path) -> Vec<String> {
        let mut lines = Vec::new();
        for record in self.records().into_iter().filter(|r| r.terminal) {
            let session = &record.live.session;
            if self
                .stale
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(session)
                || !crate::lead::stale(&self.runtime, &record.live.name, home).await
            {
                continue;
            }
            self.stale
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(session.clone());
            let body = text::lead_sign_in_stale(&record.live.name);
            self.world.chat(&record, &body).await;
            lines.push(body);
        }
        lines
    }

    /// Looks at the sign-in of each lead each [`STALE_EVERY`], until the
    /// daemon stops ([`Daemon::stale_leads`]).
    pub fn stale_every(self: &Arc<Self>) {
        let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
            return;
        };
        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                for line in daemon.stale_leads(Path::new(&home)).await {
                    println!("riff daemon: {line}");
                }
                tokio::time::sleep(STALE_EVERY).await;
            }
        });
    }

    /// Starts each lead that runs an old riff again on the riff of this
    /// daemon, when the lead is idle (01M5TC24PB3YC2CC8BA275WSEN). A lead
    /// runs an old riff when `riff update` replaced the file that its
    /// container binds at [`RELAY_BIN`] ([`crate::lead::old`]). It is idle
    /// when its transcript got no write in [`LEAD_IDLE`]
    /// ([`crate::lead::quiet`]): a lead in a turn waits for the next look.
    /// It gives one line for each lead that it started again, or that it
    /// first told the person of: the line is in riff chat too.
    pub async fn leads_again(self: &Arc<Self>) -> Vec<String> {
        let mut lines = Vec::new();
        for record in self.records().into_iter().filter(|r| r.terminal) {
            if crate::lead::old(&self.runtime, &record.live.name).await
                && crate::lead::quiet(&record, self.lead_idle)
                && let Some(line) = self.lead_again(record).await
            {
                lines.push(line);
            }
        }
        lines
    }

    /// Looks at the riff of each lead each [`AGAIN_EVERY`], from now until
    /// the daemon stops ([`Daemon::leads_again`]).
    pub fn leads_again_every(self: &Arc<Self>) {
        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                for line in daemon.leads_again().await {
                    println!("riff daemon: {line}");
                }
                tokio::time::sleep(AGAIN_EVERY).await;
            }
        });
    }

    /// Starts the lead of `record` again: the same session, the same
    /// clone and Claude folder, `claude --resume` of its conversation, and
    /// the bind of the riff of this daemon (01M5TC2919TB9XPF42YFNJBP79).
    ///
    /// ```mermaid
    /// sequenceDiagram
    ///     participant D as riff daemon
    ///     participant C as lead container
    ///     participant S as riff-server
    ///     D->>D: the start: host facts, image, relay, spec
    ///     D->>C: podman stop
    ///     Note over D,S: the lead keeps its session: no leave
    ///     D->>C: podman run: claude --resume ID
    ///     D->>S: chat: the lead started again
    /// ```
    ///
    /// All that can fail comes before the stop: the wave of the
    /// repository for its main clone, the facts of the host, the image,
    /// the relay. A fault there leaves the lead on the old riff. The
    /// person hears of it one time, and the daemon tries again at the next
    /// look ([`text::lead_again_waits`]). After the stop, a start that
    /// fails three times ends the lead: the daemon ends its session, and
    /// the person starts it ([`text::lead_again_failed`]). It gives the
    /// line for the person, or `None` when the lead ended on its own.
    async fn lead_again(self: &Arc<Self>, record: Record) -> Option<String> {
        let name = record.live.name.clone();
        let owned = self.lock().get(&name).cloned()?;
        let start = match self.lead_start(&record).await {
            Ok(start) => start,
            Err(e) => return self.lead_waits(&record, &format!("{e:#}")).await,
        };
        println!("{}", text::daemon_stops_lead(&name));
        owned
            .renewing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Err(e) = self.runtime.stop(&name).await {
            owned
                .renewing
                .store(false, std::sync::atomic::Ordering::SeqCst);
            return self.lead_waits(&record, &format!("{e:#}")).await;
        }
        // A lead that this daemon took up has no end of its own: it looks.
        owned.look.notify_one();
        // The task of the old container closes it: its socket, its record.
        for _ in 0..300 {
            if !self.lock().contains_key(&name) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut why = String::new();
        for attempt in 0..LEAD_STARTS {
            if attempt > 0 {
                tokio::time::sleep(LEAD_START_WAIT).await;
            }
            match self.lead_launch(start.clone()).await {
                Ok(()) => {
                    // The next update can fail on its own: the person hears of it.
                    self.again_told
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&record.live.session);
                    let line = text::lead_started_again(&name);
                    self.world.chat(&record, &line).await;
                    return Some(line);
                }
                Err(e) => why = e,
            }
        }
        self.world.leave(&record).await;
        let line = text::lead_again_failed(&name, &why);
        self.world.chat(&record, &line).await;
        Some(line)
    }

    /// The [`Start`] of the lead of `record` on the riff of this daemon,
    /// with all that can fail checked before the stop: the main clone of
    /// the wave, the facts of the host, the spec, the relay and the image.
    async fn lead_start(&self, record: &Record) -> Result<Start> {
        let repo = record.me.place().repo_text();
        let wave = crate::wave::read_waves(&self.dir)?
            .remove(&repo)
            .ok_or_else(|| anyhow!("{}", text::lead_again_no_wave(&repo)))?;
        let start = self.world.lead_again(&wave, record).await?;
        self.check(&start.spec)?;
        self.check_relay()?;
        self.have_image(&start.spec.image, |_| {}).await?;
        Ok(start)
    }

    /// Starts the container of `start`, a lead, and waits until it runs.
    /// The lead goes on in the task that [`Daemon::launch`] runs.
    async fn lead_launch(self: &Arc<Self>, start: Start) -> Result<(), String> {
        let (out, mut said) = Out::keep();
        let daemon = self.clone();
        tokio::spawn(async move { daemon.launch(start, None, None, None, &out).await });
        loop {
            match said.recv().await {
                Some(Say::Done) => return Ok(()),
                Some(Say::Failed { error }) => return Err(error),
                Some(_) => {}
                None => return Err(text::DAEMON_GONE.to_owned()),
            }
        }
    }

    /// The lead of `record` cannot start again now: it runs the old riff
    /// on. The person hears of it one time for each lead (riff chat); the
    /// next look tries again. It gives the line the first time.
    async fn lead_waits(&self, record: &Record, why: &str) -> Option<String> {
        let first = self
            .again_told
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(record.live.session.clone());
        if !first {
            return None;
        }
        let line = text::lead_again_waits(&record.live.name, why);
        self.world.chat(record, &line).await;
        Some(line)
    }

    /// Looks at the watches each `every`, until the daemon
    /// stops ([`crate::watches`], 01M5RGD0RTXYA7FF5K1TH72MGX). A look that
    /// fails shows its fault on stderr one time until the fault changes.
    /// A test stops the loop with the task that it gives.
    pub fn watch_every(self: &Arc<Self>, every: Duration) -> JoinHandle<()> {
        let daemon = self.clone();
        tokio::spawn(async move {
            let watches = crate::watches::Watches::new(&daemon.dir);
            let mut last_error = None;
            loop {
                match watches.look_all(&daemon.lookout, now_ms()).await {
                    Ok(()) => last_error = None,
                    Err(e) => {
                        let e = format!("{e:#}");
                        if last_error.as_ref() != Some(&e) {
                            eprintln!("riff daemon: watches: {e}");
                        }
                        last_error = Some(e);
                    }
                }
                tokio::time::sleep(every).await;
            }
        })
    }

    /// Registers the watch of `op` for the repository of the session of
    /// `record` ([`ServerCall::Watch`]).
    pub(crate) fn watch_op(&self, record: &Record, op: crate::watches::Op) -> Result<Value> {
        let repo = record.me.place().repo_text();
        let registered = crate::watches::Watches::new(&self.dir).register(&repo, op, now_ms())?;
        Ok(serde_json::to_value(registered)?)
    }

    /// Waits until a session ends, or a job of a wave ends.
    pub(crate) async fn changed(&self) {
        self.changed.notified().await;
    }

    /// Tells the waves and the starts that wait: a session or a job
    /// ended.
    pub(crate) fn tell_changed(&self) {
        self.changed.notify_waiters();
        self.room.ended();
    }

    /// Gives the session of `spec` the MCP servers of the host
    /// (01M57840R2HNXGP9B2ENSKTWD3): [`crate::session_mcp::VAR`] with its
    /// value in `values`, and the secrets of the servers in `secrets`.
    /// When riff cannot read them, it says so, and the session starts
    /// with riff only.
    fn give_mcp(
        &self,
        spec: &mut SessionSpec,
        values: &mut Vec<(String, String)>,
        secrets: &mut Vec<String>,
    ) {
        use crate::session_mcp::VAR;
        let servers = match crate::session_mcp::read(&self.dir) {
            Ok(servers) => servers,
            Err(e) => {
                println!("{}", text::session_mcp_unread(&format!("{e:#}")));
                return;
            }
        };
        spec.env
            .retain(|e| !matches!(e, Env::Host(name) | Env::Value(name, _) if name == VAR));
        values.retain(|(name, _)| name != VAR);
        if servers.is_empty() {
            return;
        }
        secrets.extend(crate::session_mcp::secrets(&servers));
        spec.env.push(Env::Host(VAR.into()));
        values.push((VAR.into(), Value::Object(servers).to_string()));
    }

    /// The state of the daemon.
    pub async fn status(&self) -> Status {
        let dir = self.dir.clone();
        let claude_token = tokio::task::spawn_blocking(move || crate::claude_token::state(&dir))
            .await
            .unwrap_or_default();
        // Before the struct: its `sessions` holds the lock of the sessions.
        let lost = self.lost_shown();
        Status {
            pid: std::process::id(),
            build: Build::this().to_string(),
            since_ms: self.since_ms,
            dir: self.dir.clone(),
            sessions: self
                .lock()
                .values()
                .map(|owned| owned.lock().live())
                .collect(),
            limit: Some(self.room.limit()),
            waiting: self.room.waiting(),
            waves: self.waves_shown(),
            jobs: self
                .waves_lock()
                .values()
                .map(|running| {
                    running
                        .taken
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .len()
                })
                .sum(),
            test: release_test::running(),
            mcp: crate::session_mcp::read(&self.dir)
                .map(|servers| crate::session_mcp::names(&servers))
                .unwrap_or_default(),
            claude_token,
            watches: self.watches_shown(),
            lost,
        }
    }

    /// The leads that run with no network ([`Daemon::lost_network`]).
    fn lost_shown(&self) -> Vec<LostShown> {
        let lost = self.lost.lock().unwrap_or_else(PoisonError::into_inner);
        self.records()
            .into_iter()
            .filter(|r| r.terminal && lost.contains(&r.live.session))
            .map(|r| LostShown {
                repo: r.me.place().repo_text(),
                name: r.live.name,
            })
            .collect()
    }

    /// The watches in the file, but the watch of a wave: a wave shows in
    /// [`Status::waves`].
    fn watches_shown(&self) -> Vec<WatchShown> {
        let all = crate::watches::Watches::new(&self.dir).read();
        all.unwrap_or_default()
            .into_iter()
            .filter(|w| w.op != crate::watches::Op::Wave)
            .map(|w| WatchShown {
                what: w.op.what(),
                repo: w.repo,
                since_ms: w.since_ms,
            })
            .collect()
    }

    /// Tells each lead on this host that the daemon starts again on a new
    /// riff, with one note for each lead (01M5SNV7H3739PXW73H14XM4B8). It
    /// gives a line for each lead that it told.
    pub async fn tell_leads_restart(&self) -> Vec<String> {
        tell_leads_restart(&self.world, &self.records()).await
    }

    /// Asks for a new start of the daemon with `restart` when it runs an
    /// old riff (`old`) and is idle ([`Status::busy`] is 0,
    /// 01M517BTP6BAGTRBYWPX9GPVE0). It looks at the end of each session
    /// and of each job, and each [`OLD_LOOK`]. It asks once, then ends.
    pub async fn restart_when_old(
        &self,
        old: impl Fn() -> bool,
        restart: impl FnOnce() -> Result<()>,
    ) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.status().await.busy() == 0 && old() {
                println!("{}", text::DAEMON_NEW_RIFF);
                for line in self.tell_leads_restart().await {
                    println!("{line}");
                }
                if let Err(e) = restart() {
                    eprintln!(
                        "riff daemon: {}",
                        text::daemon_no_restart(&format!("{e:#}"))
                    );
                }
                return;
            }
            tokio::select! {
                () = &mut changed => {}
                () = tokio::time::sleep(OLD_LOOK) => {}
            }
        }
    }

    /// Each wave that the daemon runs, with what it did last, or the
    /// image that podman cannot pull for it (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
    pub fn waves_shown(&self) -> Vec<WaveShown> {
        self.waves_lock()
            .iter()
            .map(|(repo, running)| WaveShown {
                repo: repo.clone(),
                // A pull that failed shows until a pull works.
                word: self.failed_pull(repo).map_or_else(
                    || {
                        running
                            .word
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone()
                    },
                    |pull| text::wave_pull_word(&pull.image, &pull.reason),
                ),
            })
            .collect()
    }

    fn waves_lock(&self) -> MutexGuard<'_, BTreeMap<String, Running>> {
        self.waves.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs the wave `wave`: it keeps it in [`crate::wave::WAVES_FILE`],
    /// and starts its loop when it does not run.
    pub fn run_wave(self: &Arc<Self>, wave: Wave) -> Result<()> {
        let mut waves = crate::wave::read_waves(&self.dir)?;
        waves.insert(wave.repo.clone(), wave.clone());
        crate::wave::write_waves(&self.dir, &waves)?;
        self.spawn_wave(wave);
        Ok(())
    }

    /// Starts the loop of `wave`, when it does not run.
    /// A loop that runs with other facts of the wave, for example
    /// another main clone, starts again with the new facts. Its jobs go
    /// on. A loop that runs with the same facts goes on, and the starts
    /// of each item go back to 0 (01M57A7A48A70Y7PTD95A3MBYC).
    fn spawn_wave(self: &Arc<Self>, wave: Wave) {
        let mut running = self.waves_lock();
        let taken = match running.get(&wave.repo) {
            Some(old) if old.wave == wave && !old.task.is_finished() => {
                old.starts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clear();
                drop(running);
                self.tell_changed();
                return;
            }
            Some(old) => {
                old.abort();
                old.taken.clone()
            }
            None => Arc::new(Mutex::new(std::collections::BTreeSet::new())),
        };
        let starts = crate::wave::Starts::default();
        let word = Arc::new(Mutex::new(text::wave_starts()));
        let task = tokio::spawn(crate::wave::run(
            self.clone(),
            wave.clone(),
            taken.clone(),
            starts.clone(),
            word.clone(),
        ));
        let look = tokio::spawn(crate::wave::watch(self.clone(), wave.clone()));
        running.insert(
            wave.repo.clone(),
            Running {
                wave,
                task,
                look,
                taken,
                starts,
                word,
            },
        );
    }

    /// Starts no new session for the wave of `repo`. The sessions that
    /// run go on. False when the daemon did not run that wave.
    pub fn stop_wave(&self, repo: &str) -> Result<bool> {
        let mut waves = crate::wave::read_waves(&self.dir)?;
        let kept = waves.remove(repo).is_some();
        crate::wave::write_waves(&self.dir, &waves)?;
        let running = self.waves_lock().remove(repo);
        if let Some(running) = &running {
            running.abort();
        }
        Ok(kept || running.is_some())
    }

    /// Starts the loop of each wave of [`crate::wave::WAVES_FILE`]. Call
    /// it after [`Daemon::recover`].
    pub fn run_waves(self: &Arc<Self>) {
        match crate::wave::read_waves(&self.dir) {
            Ok(waves) => {
                for wave in waves.into_values() {
                    println!("{}", text::wave_runs(&wave.repo));
                    self.spawn_wave(wave);
                }
            }
            Err(e) => eprintln!("riff daemon: {e:#}"),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Arc<Owned>>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn gone_lock(&self) -> MutexGuard<'_, BTreeMap<String, Gone>> {
        self.gone.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answers each connection of `listener` until `stop` ends. The stop
    /// stops no session (01M4ZDTW00B7N6T9P3MK066KA7).
    pub async fn serve(
        self: Arc<Self>,
        listener: UnixListener,
        stop: impl Future<Output = ()>,
    ) -> Result<()> {
        tokio::pin!(stop);
        loop {
            tokio::select! {
                () = &mut stop => return Ok(()),
                accepted = listener.accept() => match accepted {
                    Ok((conn, _)) => {
                        let daemon = self.clone();
                        tokio::spawn(async move {
                            if let Err(e) = daemon.answer(conn).await {
                                eprintln!("riff daemon: {e:#}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("riff daemon: cannot accept a connection: {e}");
                        // For example no file descriptor is free: wait
                        // a moment, and do not spin.
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                },
            }
        }
    }

    /// Answers one connection.
    async fn answer(self: Arc<Self>, conn: UnixStream) -> Result<()> {
        let (read, write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        let (out, written) = writer(write);
        if let Some(first) = lines.next_line().await? {
            match hello(&first) {
                Ok(Ask::Status) => out.say(Say::Status(Box::new(self.status().await))),
                Ok(Ask::Image { image }) => self.image(&image, &out).await,
                Ok(Ask::Start(start)) => self.start(*start, lines, &out).await,
                Ok(Ask::Follow { name, after }) => self.follow(&name, after, lines, &out),
                Ok(Ask::Wave { wave }) => {
                    let repo = wave.repo.clone();
                    out.say(match self.run_wave(wave.clone()) {
                        Ok(()) => {
                            println!("{}", text::wave_runs(&repo));
                            if let Err(e) = self.serve_repo(&wave) {
                                eprintln!("riff daemon: {e:#}");
                            }
                            Say::Done
                        }
                        Err(e) => Say::Failed {
                            error: format!("{e:#}"),
                        },
                    });
                }
                Ok(Ask::WaveStop { repo }) => out.say(match self.stop_wave(&repo) {
                    Ok(true) => {
                        println!("{}", text::wave_stops(&repo));
                        Say::Done
                    }
                    Ok(false) => Say::Failed {
                        error: text::wave_not_run(&repo),
                    },
                    Err(e) => Say::Failed {
                        error: format!("{e:#}"),
                    },
                }),
                Ok(Ask::Takeover { name, at_end }) => {
                    self.takeover(&name, at_end, lines, &out).await;
                }
                Ok(Ask::Sweep) => {
                    let mut swept: Vec<String> =
                        self.sweep(None).await.into_iter().map(|s| s.line).collect();
                    for line in &swept {
                        println!("{line}");
                    }
                    if swept.is_empty() {
                        swept.push(text::SWEPT_NONE.into());
                    }
                    say_lines(&out, Ok(swept));
                }
                Ok(Ask::Remove { name, force }) => {
                    let removed = self.remove_one(&name, force).await.map(|line| vec![line]);
                    if let Ok(lines) = &removed {
                        lines.iter().for_each(|line| println!("{line}"));
                    }
                    say_lines(&out, removed);
                }
                Ok(Ask::End { name }) => {
                    let ended = self.end_session(&name).await;
                    if let Ok(lines) = &ended {
                        lines.iter().for_each(|line| println!("{line}"));
                    }
                    say_lines(&out, ended);
                }
                Ok(Ask::Restart) => {
                    let line = self.restart_now().await;
                    println!("{line}");
                    say_lines(&out, Ok(vec![line]));
                }
                Ok(Ask::Restarting) => {
                    let lines = self.tell_leads_restart().await;
                    lines.iter().for_each(|line| println!("{line}"));
                    say_lines(&out, Ok(lines));
                }
                Ok(Ask::Serve { wave }) => {
                    let line = text::daemon_serves(&wave.repo, &wave.host);
                    say_lines(&out, self.serve_repo(&wave).map(|()| vec![line]));
                }
                Err(error) => out.say(Say::Failed { error }),
            }
        }
        drop(out);
        let _ = written.await;
        Ok(())
    }

    /// Has the image `image` on this machine: it pulls it when it is not
    /// here (01M4Z81B5A456HDKW7G0WFGCY6).
    async fn image(&self, image: &str, out: &Out) {
        let shown = |text: String| out.say(Say::Line { text });
        out.say(match self.have_image(image, shown).await {
            Ok(()) => Say::Done,
            Err(e) => Say::Failed {
                error: format!("{e:#}"),
            },
        });
    }

    /// Has the image `image` on this machine: it pulls it when it is not
    /// here, and gives `show` the line of the pull. A pull that fails is a
    /// [`PullFailed`].
    pub(crate) async fn have_image(&self, image: &str, show: impl FnOnce(String)) -> Result<()> {
        if !self.runtime.has_image(image).await? {
            show(text::run_pulling(image));
            self.runtime.pull(image).await.map_err(|e| PullFailed {
                image: image.to_owned(),
                reason: format!("{e:#}"),
            })?;
        }
        Ok(())
    }

    /// Keeps the failed pull `pull` for the wave of `repo`. True when the
    /// lead has not heard of this image yet: the first failure, or the
    /// first one after a pull that worked
    /// ([`Daemon::pull_worked`], 01M5SMSB3H1E9NXMBDN8ZDJ6CD).
    pub(crate) fn pull_failed(&self, repo: &str, pull: &PullFailed) -> bool {
        let mut pulls = self.pulls.lock().unwrap_or_else(PoisonError::into_inner);
        let first = pulls.get(repo).is_none_or(|old| old.image != pull.image);
        pulls.insert(
            repo.to_owned(),
            Pulled {
                image: pull.image.clone(),
                reason: pull.reason.clone(),
                at: std::time::Instant::now(),
            },
        );
        first
    }

    /// The image of the wave of `repo` is on this machine: forgets the
    /// failed pull.
    pub(crate) fn pull_worked(&self, repo: &str) {
        self.pulls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(repo);
    }

    /// The failed pull of the wave of `repo`, until a pull works. The
    /// wave waits for [`Daemon::look_every`] after a failure, then tries
    /// the pull again (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
    pub(crate) fn failed_pull(&self, repo: &str) -> Option<FailedPull> {
        let pulls = self.pulls.lock().unwrap_or_else(PoisonError::into_inner);
        pulls.get(repo).map(|pulled| FailedPull {
            image: pulled.image.clone(),
            reason: pulled.reason.clone(),
            waits: pulled.at.elapsed() < self.look_every,
        })
    }

    /// At the first job of a wave of a repository with the image `image`
    /// after the daemon starts, reads the version of each tool of
    /// [`crate::image_tools::Tool::ALL`] in the image. When one is older
    /// than this riff needs, it tells the lead of the repository one
    /// message with the `riff-base` to build `FROM`
    /// (01M5SNTPHHXP54YBTV09JNRY3F). A tool with no version, or an image
    /// that does not run, gives no message and a line in the log: the job
    /// goes on. Call it after [`Daemon::have_image`].
    pub(crate) async fn check_tools(&self, wave: &Wave, image: &str) {
        let first = self
            .looked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(format!("{} {image}", wave.repo));
        if !first {
            return;
        }
        let mut old = Vec::new();
        for tool in crate::image_tools::Tool::ALL {
            match self.runtime.output(image, &tool.command()).await {
                Ok(printed) => old.extend(tool.old_in(&printed)),
                Err(e) => println!(
                    "riff daemon: {}: cannot read the version of {} in {image}: {e:#}",
                    wave.repo,
                    tool.name()
                ),
            }
        }
        if !old.is_empty() {
            let note = text::image_old_tools(&wave.repo, &old, &crate::session_image::base());
            self.world.tell(wave, &note).await;
        }
    }

    /// Refuses to start a session when the binary that the daemon binds
    /// at [`RELAY_BIN`] is not static (01M510NW8YEVMQKD6RWJPW38N3). It
    /// reads the file each time, so a `riff update` needs no restart of
    /// the daemon. [`crate::release::DYNAMIC`] takes each binary.
    pub fn check_relay(&self) -> Result<()> {
        if self.any_relay {
            return Ok(());
        }
        crate::release::check_file(&self.relay_bin)
            .map_err(|why| anyhow!("{}", text::daemon_relay_not_static(&self.relay_bin, &why)))
    }

    /// Refuses a spec that would show the folder of the daemon to the
    /// session: a bind of the folder, of a folder above it, or of a path
    /// in it, or a bind at [`IN_SESSION`] or at [`RELAY_BIN`]
    /// (01M4ZDTVQEHD87WD5TSN93XFSS).
    pub fn check(&self, spec: &SessionSpec) -> Result<()> {
        let ours = resolved(&self.dir);
        for bind in &spec.binds {
            if bind.target.starts_with(IN_SESSION) || bind.target.starts_with(RELAY_BIN) {
                bail!("{}", text::daemon_bind_in_session(&bind.target));
            }
            if let Source::Path(source) = &bind.source {
                let source = resolved(source);
                if ours.starts_with(&source) || source.starts_with(&ours) {
                    bail!("{}", text::daemon_bind_shows(&source));
                }
            }
        }
        Ok(())
    }

    /// Owns the session of `record`: its log, its record on disk, and
    /// its socket. With `again`, the daemon takes up a session that runs:
    /// it goes on with the log, and binds the socket again in the folder
    /// that the container sees.
    fn own(
        self: &Arc<Self>,
        record: Record,
        scrub: Scrub,
        again: bool,
    ) -> Result<(Arc<Owned>, mpsc::UnboundedReceiver<Note>)> {
        let name = record.live.name.clone();
        let events = record.folder.join(EVENTS);
        // The log comes first: a session that cannot log starts no
        // container.
        let (log, seq, tasks) = if again {
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&events);
            (log, lines_of(&events), tasks_in(&events))
        } else {
            (std::fs::File::create(&events), 0, 0)
        };
        let log = log.with_context(|| format!("cannot write {}", events.display()))?;
        private_dir(&self.dir.join(LIVE))?;
        let (notes, noted) = mpsc::unbounded_channel();
        let owned = Arc::new(Owned {
            dir: self.dir.clone(),
            socket: session_dir(&self.dir, &name),
            record_path: record_path(&self.dir, &name),
            name: name.clone(),
            scrub,
            state: Mutex::new(SessionState {
                record,
                seq,
                last: tokio::time::Instant::now(),
                tasks,
                log,
                followers: Vec::new(),
                relay: None,
                relay_protocol: 0,
                held: false,
                taker: None,
                last_step: String::new(),
                ended: false,
                end: None,
                waiting: Vec::new(),
                lines: Vec::new(),
            }),
            notes,
            stop: Notify::new(),
            look: Notify::new(),
            renewing: std::sync::atomic::AtomicBool::new(false),
            closing: Notify::new(),
            door: Mutex::new(None),
        });
        {
            let mut live = self.lock();
            if live.contains_key(&name) {
                bail!("{}", text::daemon_runs_session(&name));
            }
            live.insert(name.clone(), owned.clone());
        }
        let opened = owned
            .save(&owned.lock())
            .and_then(|()| self.door(&owned, again));
        match opened {
            Ok(door) => {
                *owned.door.lock().unwrap_or_else(PoisonError::into_inner) = Some(door);
                Ok((owned, noted))
            }
            Err(e) => {
                self.lock().remove(&name);
                let _ = std::fs::remove_file(&owned.record_path);
                Err(e)
            }
        }
    }

    /// Starts the session of `start` when the host has room
    /// ([`crate::room`]), says its events on `out`, and waits for its
    /// end. A [`Then::Stop`] on `from` stops it. A stop, or the end of
    /// `from`, while the start waits for room starts nothing.
    async fn start(
        self: &Arc<Self>,
        start: Start,
        mut from: Lines<BufReader<OwnedReadHalf>>,
        out: &Out,
    ) {
        let name = start.spec.name.clone();
        // The lead waits for no room: it runs no build of an item.
        if start.terminal {
            self.launch(start, None, None, Some(from), out).await;
            return;
        }
        let Some(slot) = self.room_for(&name, &mut from, out).await else {
            return;
        };
        self.launch(start, None, Some(slot), Some(from), out).await;
    }

    /// Waits for room in the limit of the host for the session `name`,
    /// and says why it waits on `out`. A [`Then::Stop`] on `from`, or the
    /// end of `from`, ends the wait: it says [`Say::Failed`] and gives
    /// `None` (01M4ZV3B6VS0RAR0P7MRHKE8CQ).
    pub(crate) async fn room_for(
        &self,
        name: &str,
        from: &mut Lines<BufReader<OwnedReadHalf>>,
        out: &Out,
    ) -> Option<Slot> {
        let admit = self.room.admit(
            name,
            || self.live_count(),
            |full| {
                out.say(Say::Line {
                    text: text::daemon_waits(name, full),
                });
            },
        );
        tokio::pin!(admit);
        loop {
            tokio::select! {
                slot = &mut admit => return Some(slot),
                line = from.next_line() => match line {
                    Ok(Some(line)) if !matches!(serde_json::from_str::<Then>(&line), Ok(Then::Stop)) => {}
                    _ => {
                        out.say(Say::Failed {
                            error: text::daemon_no_start(name),
                        });
                        return None;
                    }
                },
            }
        }
    }

    /// Starts the session of `start` in the room of `slot`, with the job
    /// `job` of a wave, says its events on `out`, and waits for its end.
    /// A [`Then::Stop`] on `from` stops it. It gives the record of the
    /// session. A session with a terminal takes no slot: it says
    /// [`Say::Done`] when its container runs. The container gets the
    /// caps of the host now (01M56Q4AQ86V99RC8CA1KGCMSZ), and the size of
    /// the compile cache (01M57818168N3AQGE3G5GDYDDD): a later change of
    /// the limit or of the size does not change it.
    pub(crate) async fn launch(
        self: &Arc<Self>,
        start: Start,
        job: Option<Job>,
        slot: Option<Slot>,
        from: Option<Lines<BufReader<OwnedReadHalf>>>,
        out: &Out,
    ) -> Record {
        let Start {
            session,
            item,
            folder,
            mut spec,
            start,
            mut values,
            mut secrets,
            server,
            me,
            terminal,
        } = start;
        let limit = self.room.limit();
        limit.caps(&mut spec);
        limit.effort(&mut spec, &item);
        self.give_mcp(&mut spec, &mut values, &mut secrets);
        let name = spec.name.clone();
        let record = Record {
            live: Live {
                name: name.clone(),
                session: session.clone(),
                item: item.clone(),
                image: spec.image.clone(),
                since_ms: now_ms(),
                phase: Phase::default(),
                last: String::new(),
                lead: terminal,
            },
            folder,
            server,
            me,
            inputs: if terminal {
                Vec::new()
            } else {
                vec![user_line(&start)]
            },
            closed: false,
            failed: false,
            job,
            terminal,
            takeover: false,
            interrupt: false,
        };
        if let Err(e) = self.check(&spec).and_then(|()| self.check_relay()) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return record;
        }
        let scrub = Scrub::new(secrets);
        let (owned, notes) = match self.own(record.clone(), scrub.clone(), false) {
            Ok(owned) => owned,
            Err(e) => {
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return record;
            }
        };
        // The daemon owns the session now: it counts as live, and its
        // slot goes.
        drop(slot);
        owned.lock().followers.push(out.clone());
        spec.binds.push(Bind {
            source: Source::Path(owned.socket.clone()),
            target: IN_SESSION.into(),
            write: true,
        });
        spec.binds.push(Bind {
            source: Source::Path(self.relay_bin.clone()),
            target: RELAY_BIN.into(),
            write: false,
        });
        spec.env.push(Env::Value(
            SOCKET_VAR.into(),
            in_session().to_string_lossy().into_owned(),
        ));
        // The riff commands of the session know their user without the
        // keyring, which a container has not.
        spec.env
            .retain(|e| !matches!(e, Env::Value(name, _) if name == "RIFF_USER"));
        spec.env.push(Env::Value(
            "RIFF_USER".into(),
            record.me.who().user().to_owned(),
        ));
        let relay: &[&str] = if terminal {
            &[RELAY_BIN, "relay", "--terminal", "--"]
        } else {
            &[RELAY_BIN, "relay", "--"]
        };
        let mut command: Vec<String> = relay.iter().map(|a| (*a).to_owned()).collect();
        command.append(&mut spec.command);
        spec.command = command;
        println!("{}", text::daemon_session_starts(&name, &item, &session));
        out.say(Say::Started {
            name: name.clone(),
            socket: owned.socket.clone(),
        });
        if terminal {
            let end = match self.runtime.start_terminal(&spec, &values).await {
                Ok(end) => end,
                Err(e) => {
                    self.close(&owned, START_FAILED);
                    println!("{}", text::daemon_session_failed(&name));
                    out.say(Say::Failed {
                        error: format!("{e:#}"),
                    });
                    return record;
                }
            };
            self.world.resume(&record);
            out.say(Say::Done);
            let reader = from.map(|from| stop_on(from, owned.clone()));
            self.clone().watch(owned, notes, Some(end)).await;
            if let Some(reader) = reader {
                reader.abort();
            }
            return record;
        }
        let running = match self.runtime.start(&spec, &values).await {
            Ok(running) => running,
            Err(e) => {
                self.close(&owned, START_FAILED);
                println!("{}", text::daemon_session_failed(&name));
                out.say(Say::Failed {
                    error: format!("{e:#}"),
                });
                return record;
            }
        };
        // The output of the container: the notes of the relay and the
        // stderr of claude.
        for output in [running.events, running.errors] {
            let (scrub, owned) = (scrub.clone(), owned.clone());
            tokio::spawn(async move {
                let mut lines = BufReader::new(output).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = scrub.clean(&line).into_owned();
                    Owned::tell(&mut owned.lock(), &Say::Stderr { line });
                }
            });
        }
        let reader = from.map(|from| stop_on(from, owned.clone()));
        // The input of the container stays open until the end: the relay
        // gives claude its input.
        let input = running.input;
        self.clone().watch(owned, notes, Some(running.end)).await;
        drop(input);
        if let Some(reader) = reader {
            reader.abort();
        }
        record
    }

    /// Follows the live session `name` on `out`: each event after
    /// `after`, then as [`Daemon::start`]. A [`Then::Stop`] on `from`
    /// stops it. A session that ended in the last `keep_end` gives its
    /// end ([`Gone`]).
    fn follow(&self, name: &str, after: u64, from: Lines<BufReader<OwnedReadHalf>>, out: &Out) {
        // A session is in `gone` before it leaves `live` ([`Daemon::close`]).
        let live = self.lock().get(name).cloned();
        let Some(owned) = live else {
            match self.gone_lock().get(name) {
                Some(gone) if gone.at.elapsed() < self.keep_end => gone.follow(out, after),
                _ => out.say(Say::Failed {
                    error: text::daemon_no_session(name),
                }),
            }
            return;
        };
        if let Err(e) = owned.follow(out.clone(), after) {
            out.say(Say::Failed {
                error: format!("{e:#}"),
            });
            return;
        }
        // The reader ends with the connection.
        stop_on(from, owned);
    }

    /// The quiet limit of the session of `record`, with `tasks` background
    /// tasks: the grace of a session of a wave while `claude` runs none,
    /// else [`QUIET_LIMIT`] (01M5P26VPJA73WCPVZY6CPG9HD, 01M5R0QFM265CCKTADK4T5XREQ).
    fn limit_of(&self, record: &Record, tasks: usize) -> Duration {
        if record.job.is_some() && tasks == 0 {
            self.grace
        } else {
            self.quiet
        }
    }

    /// Waits for the end of the session `owned`: the exit of its relay,
    /// the end of its container, or, with no `end` of this daemon, the
    /// end of the relay in a container that does not run. After the
    /// input of `claude` closed, it stops the container when no event
    /// comes for the quiet limit, and says why
    /// (01M4ZSZX77AJDGWNQMGBPV2QXG).
    async fn watch(
        self: Arc<Self>,
        owned: Arc<Owned>,
        mut notes: mpsc::UnboundedReceiver<Note>,
        end: Option<End>,
    ) {
        let mut end = end.map(|end| Box::pin(end.wait()));
        let mut stopped = false;
        // The lead waits for its wakes itself: `riff relay watch`.
        let wakes = (!owned.lock().record.terminal)
            .then(|| tokio::spawn(give_wakes(self.world.clone(), owned.clone())));
        let (code, exited) = loop {
            // Each event moves the end of the quiet: a new turn after a
            // task of claude ended is no quiet.
            let quiet_at = {
                let state = owned.lock();
                (state.record.closed && !state.record.takeover)
                    .then(|| state.last + self.limit_of(&state.record, state.tasks))
            };
            tokio::select! {
                () = owned.closing.notified() => {}
                () = tokio::time::sleep_until(quiet_at.unwrap_or_else(tokio::time::Instant::now)),
                    if quiet_at.is_some() && !stopped =>
                {
                    let (quiet, limit) = {
                        let state = owned.lock();
                        let limit = self.limit_of(&state.record, state.tasks);
                        (state.last.elapsed() >= limit && !state.record.takeover, limit)
                    };
                    if quiet {
                        stopped = true;
                        let line = text::daemon_quiet(&owned.name, limit);
                        println!("{line}");
                        Owned::say_line(&mut owned.lock(), line);
                        let _ = self.runtime.stop(&owned.name).await;
                    }
                }
                note = notes.recv() => match note {
                    Some(Note::Exit(code)) => break (code, true),
                    Some(Note::Gone) if end.is_none() => {
                        tokio::time::sleep(GONE_WAIT).await;
                        let back = owned.lock().relay.is_some();
                        if !back && !self.runtime.running(&owned.name).await.unwrap_or(true) {
                            break (None, false);
                        }
                    }
                    Some(Note::Gone) => {}
                    None => break (None, false),
                },
                // A session that this daemon took up has no end of its
                // own. Its relay can have given up before the daemon
                // came back: then no relay connects.
                () = async {
                    tokio::select! {
                        () = tokio::time::sleep(LOOK_EVERY) => {}
                        () = owned.look.notified() => {}
                    }
                }, if end.is_none() => {
                    let back = owned.lock().relay.is_some();
                    if !back && !self.runtime.running(&owned.name).await.unwrap_or(true) {
                        break (None, false);
                    }
                }
                exit = async { end.as_mut().expect("an end").await }, if end.is_some() => {
                    // The container ended. Its relay may have sent its
                    // exit just before: take it.
                    let code = exit.ok().and_then(|e| e.code);
                    let exit = tokio::time::timeout(GONE_WAIT, async {
                        loop {
                            match notes.recv().await {
                                Some(Note::Exit(code)) => return Some(code),
                                Some(Note::Gone) => {}
                                None => return None,
                            }
                        }
                    });
                    break match exit.await {
                        Ok(Some(code)) => (code, true),
                        _ => (code, false),
                    };
                }
                () = owned.stop.notified(), if !stopped => {
                    stopped = true;
                    println!("{}", text::daemon_stops(&owned.name));
                    let _ = self.runtime.stop(&owned.name).await;
                }
            }
        };
        if let Some(wakes) = wakes {
            wakes.abort();
        }
        let (failed, record) = {
            let state = owned.lock();
            (state.record.failed, state.record.clone())
        };
        let ended = Ended {
            code,
            failed: failed || stopped || !exited,
        };
        println!("{}", text::daemon_session_ends(&owned.name, code));
        if record.terminal {
            // A lead that the daemon starts again on a new riff keeps its
            // session on riff-server: the next container is the same lead.
            if !owned.renewing.load(std::sync::atomic::Ordering::SeqCst) {
                let world = self.world.clone();
                tokio::spawn(async move { world.leave(&record).await });
            }
        } else {
            self.count_tokens(&record);
        }
        let followers = self.close(&owned, ended);
        for out in followers {
            out.say(Say::Ended(ended));
        }
    }

    /// Puts the tokens of the ended container session of `record` on the
    /// issue of its item, from its transcripts in its Claude folder
    /// (01M588N85Z94PX1ZP0BDXTPX83). It sums the claim at once, and
    /// writes the comment in the background: the end of the session never
    /// waits for `gh`, and never fails by the report
    /// (01M3Y1YP39VFX6GH33H7B8A8KR). The line of the report goes to the
    /// log of the daemon.
    fn count_tokens(&self, record: &Record) {
        let Some(thread) = record.me.default_thread().map(|t| t.to_string()) else {
            return;
        };
        let Some(meter) = self.world.meter(&thread) else {
            return;
        };
        let live = record.live.clone();
        let transcripts = usage::transcripts_in(&record.folder.join("claude"), &live.session);
        let claim = (thread.as_str(), live.item.as_str());
        let from_ms = u64::try_from(live.since_ms).unwrap_or(0);
        let Some(ended) =
            meter.session_ended(&live.session, claim, from_ms, &transcripts, usage::now_ms())
        else {
            return;
        };
        tokio::task::spawn_blocking(move || {
            let line = match ended {
                Ok(claim) => {
                    usage::reported(&claim, &usage::report(&meter.gh, &live.session, &claim))
                }
                Err(e) => format!("riff cannot count the tokens of {}: {e}.", live.item),
            };
            println!("riff daemon: {}: {line}", live.name);
        });
    }

    /// Ends the ownership of the session `owned`, which ended as `ended`:
    /// its socket, its folder and its record go. The daemon keeps its end
    /// for `keep_end` ([`Gone`]). It gives the connections that followed
    /// it.
    fn close(&self, owned: &Owned, ended: Ended) -> Vec<Out> {
        // From now on, no step writes its record again.
        owned.lock().ended = true;
        if let Some(door) = owned
            .door
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            door.abort();
        }
        let _ = std::fs::remove_dir_all(&owned.socket);
        let _ = std::fs::remove_file(&owned.record_path);
        let (gone, followers) = {
            let mut state = owned.lock();
            state.end = Some(ended);
            let events = state.record.folder.join(EVENTS);
            let gone = Gone {
                log: std::fs::read_to_string(events).unwrap_or_default(),
                seq: state.seq,
                lines: state.lines.clone(),
                ended,
                at: tokio::time::Instant::now(),
            };
            (gone, std::mem::take(&mut state.followers))
        };
        {
            let mut kept = self.gone_lock();
            kept.retain(|_, gone| gone.at.elapsed() < self.keep_end);
            kept.insert(owned.name.clone(), gone);
        }
        self.lock().remove(&owned.name);
        self.tell_changed();
        followers
    }

    /// Makes the folder of the socket of the session `owned`, and
    /// answers each connection on its socket. With `again`, the folder
    /// stays: the container sees it. The task ends at its abort, with
    /// each of its connections.
    fn door(self: &Arc<Self>, owned: &Arc<Owned>, again: bool) -> Result<JoinHandle<()>> {
        let dir = &owned.socket;
        if again {
            let _ = std::fs::remove_file(dir.join(SOCKET));
            private_dir(dir)?;
        } else {
            let _ = std::fs::remove_dir_all(dir);
            private_dir(dir)?;
        }
        let listener = bind_in(dir, SOCKET)?;
        let (daemon, owned) = (self.clone(), owned.clone());
        Ok(tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((conn, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                connections.spawn(daemon.clone().answer_session(conn, owned.clone()));
            }
        }))
    }

    /// Answers one connection on the socket of the session `owned`.
    async fn answer_session(self: Arc<Self>, conn: UnixStream, owned: Arc<Owned>) {
        let (read, write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        let Ok(Some(first)) = lines.next_line().await else {
            return;
        };
        match session_hello(&first) {
            Ok(SessionAsk::Relay {
                protocol,
                inputs,
                closed,
                held,
            }) => {
                owned
                    .relay(protocol, (inputs, closed, held), lines, write)
                    .await;
            }
            Ok(SessionAsk::Mcp { client, .. }) => self.mcp(&owned, client, lines, write).await,
            Ok(SessionAsk::Denied { denials, .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                out.say(match self.post_denials(&record, &denials).await {
                    Ok(count) => Say::Posted { count },
                    Err(e) => Say::Failed {
                        error: format!("{e:#}"),
                    },
                });
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Prompt { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                out.say(match self.prompted(&record).await {
                    Ok(()) => Say::Done,
                    Err(e) => Say::Failed {
                        error: format!("{e:#}"),
                    },
                });
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Caller { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                out.say(self.caller_of(&record).await);
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Server { call, .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                let done = match call {
                    ServerCall::Status => {
                        serde_json::to_value(self.status().await).map_err(Into::into)
                    }
                    ServerCall::CloudLog {
                        service,
                        project,
                        filter,
                    } => self.cloud_log(&record, &service, &project, &filter).await,
                    ServerCall::Events { name, after, notes } => {
                        self.events_page(&record, &name, (after, notes)).await
                    }
                    ServerCall::Run { item } => self.run_item(&record, &item),
                    ServerCall::Watch { op } => self.watch_op(&record, op),
                    _ => server_call(&record, &call).await,
                };
                out.say(match done {
                    Ok(reply) => Say::Served { reply },
                    Err(e) => Say::Failed {
                        error: format!("{e:#}"),
                    },
                });
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Watch { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                let wake = self.world.wake(&record);
                // The wait ends when the relay leaves.
                let gone = async { while let Ok(Some(_)) = lines.next_line().await {} };
                tokio::select! {
                    woke = wake => out.say(match woke {
                        Ok(text) => Say::Line { text },
                        Err(e) => Say::Failed { error: format!("{e:#}") },
                    }),
                    () = gone => {}
                }
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Follow { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.clone();
                let (to, mut wakes) = mpsc::unbounded_channel();
                let watch = self.world.wakes(&record, to);
                tokio::pin!(watch);
                // The wait ends when the relay leaves.
                let gone = async { while let Ok(Some(_)) = lines.next_line().await {} };
                tokio::pin!(gone);
                let mut seen = BTreeSet::new();
                loop {
                    tokio::select! {
                        () = &mut watch => break,
                        () = &mut gone => break,
                        wake = wakes.recv() => match wake {
                            Some(wake) if seen.insert((wake.thread.to_string(), wake.seq)) => {
                                out.say(Say::Line { text: text::wake_line(&wake) });
                            }
                            Some(_) => {}
                            None => break,
                        },
                    }
                }
                drop(out);
                let _ = written.await;
            }
            Ok(SessionAsk::Hello { .. }) => {
                let (out, written) = writer(write);
                let record = owned.lock().record.live.clone();
                let who = Say::Session {
                    session: record.session,
                    item: record.item,
                    name: record.name,
                };
                out.say(who.clone());
                while let Ok(Some(line)) = lines.next_line().await {
                    out.say(match session_hello(&line) {
                        Ok(SessionAsk::Hello { .. }) => who.clone(),
                        Ok(_) => Say::Failed {
                            error: text::DAEMON_NO_HELLO.into(),
                        },
                        Err(error) => Say::Failed { error },
                    });
                }
                drop(out);
                let _ = written.await;
            }
            Err(error) => {
                let (out, written) = writer(write);
                out.say(Say::Failed { error });
                drop(out);
                let _ = written.await;
            }
        }
    }

    /// The riff tools of the session of `record`. They act on the
    /// riff-server of the session, with the sign-in of
    /// the person on this machine (01M4ZHSZV46RF3XDXDTQXWGXRG). That
    /// session is the worker session
    /// of `riff run` or of a wave, which holds the claim of the item. The
    /// session gets only the tools that talk, and act on no folder of the
    /// host and no claim ([`crate::tools::CONTAINER_TOOLS`],
    /// 01M4ZV3B6T2KB09WJ59F07HXET).
    pub fn tools_of(&self, record: &Record) -> Result<crate::tools::Tools> {
        let api =
            riff_client::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
        let tools = crate::tools::Tools::new(api, record.me.clone()).as_worker(!record.terminal);
        Ok(if record.terminal {
            tools.in_lead_container()
        } else {
            tools.in_container()
        })
    }

    /// The facts of the session of `record` for the commands of the
    /// release cycle: whether it is the lead (the session with a
    /// terminal), and the role of its user in the riff.
    async fn caller_of(&self, record: &Record) -> Say {
        let lead = record.terminal;
        let roster = async {
            let api =
                riff_client::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
            api.roster(&record.me, false).await
        };
        match roster.await {
            Ok(who) => Say::Caller {
                lead,
                role: (!who.people.is_empty()).then(|| {
                    who.people
                        .iter()
                        .find(|p| p.user == record.me.who().user())
                        .map_or(riff_core::wire::PersonRole::Member, |p| p.role)
                }),
                role_unknown: false,
            },
            Err(_) => Say::Caller {
                lead,
                role: None,
                role_unknown: true,
            },
        }
    }

    /// `riff run ITEM` in the container of the lead of `record` (#1338).
    /// The container has no clone, no `gh` token of the host and no
    /// control socket of the daemon, so the daemon runs the item as a job
    /// of the wave of the repository ([`crate::wave::job`]): the steps of
    /// `riff run` on the host, in the room of the limit. The answer is
    /// the line for the person, at once. A start that fails tells the
    /// lead. A session that is not the lead gets a refusal.
    ///
    /// The job joins the loop of the wave, when it runs, so the loop
    /// does not pick the item at the same time.
    pub(crate) fn run_item(self: &Arc<Self>, record: &Record, item: &str) -> Result<Value> {
        if !record.terminal {
            bail!(text::DAEMON_RUN_ONLY_LEAD);
        }
        let issue = crate::run::issue_of(item)?;
        let repo = record.me.place().repo_text();
        let wave = crate::watches::wave_of(&self.dir, &repo)
            .map_err(|_| anyhow::anyhow!(text::run_no_wave(&repo)))?;
        let (taken, starts) = match self.waves_lock().get(&repo) {
            Some(running) => (running.taken.clone(), running.starts.clone()),
            None => Default::default(),
        };
        let work = crate::wave::Work::Build { issue };
        let runs = self.records().iter().any(|r| r.live.item == item);
        let mut taken_now = taken.lock().unwrap_or_else(PoisonError::into_inner);
        if runs || !taken_now.insert(work.item()) {
            bail!(text::run_already(item));
        }
        drop(taken_now);
        let daemon = self.clone();
        tokio::spawn(async move {
            let slot = daemon
                .room()
                .admit(
                    &text::run_slot(&work.item()),
                    || daemon.live_count(),
                    |_| {},
                )
                .await;
            crate::wave::job(daemon.clone(), wave, work, slot, taken, starts, true).await;
        });
        Ok(Value::String(text::run_asked(item)))
    }

    /// The answer to [`ServerCall::Events`]: the text of one
    /// [`remote::Page`] of the session `name`, as `riff daemon events
    /// --host` gets it from the daemon of another host. Only the lead may.
    async fn events_page(&self, record: &Record, name: &str, after: (u64, u64)) -> Result<Value> {
        if !record.terminal {
            bail!(text::DAEMON_EVENTS_ONLY_LEAD);
        }
        let page = remote::page(&self.dir, name, after, remote::EVENT_HOLD).await?;
        Ok(Value::String(page.to_string()))
    }

    /// `gcloud logging read` for the lead of `record`, with the `gcloud`
    /// and the sign-in of the person on this host (#1247). A session that
    /// is not the lead gets a refusal. When the sign-in of `gcloud`
    /// ended, the daemon also tells the lead in chat.
    async fn cloud_log(
        &self,
        record: &Record,
        service: &str,
        project: &str,
        filter: &str,
    ) -> Result<Value> {
        if !record.terminal {
            bail!(text::DAEMON_LOG_ONLY_LEAD);
        }
        let args = crate::cycle::log_read_args(service, project, filter);
        let gcloud = self.gcloud.clone();
        let done = tokio::task::spawn_blocking(move || gcloud.run(&args, None)).await?;
        match done {
            Ok(out) => Ok(Value::String(out)),
            Err(e) if crate::cycle::gcloud_missing(&e) => bail!(text::DAEMON_NO_GCLOUD),
            Err(e) => {
                let ended = e
                    .downcast_ref::<crate::cloud::Failed>()
                    .is_some_and(crate::cloud::Failed::signin_ended);
                if ended {
                    let api = riff_client::api::Api::new(&record.server)
                        .signed_in(Some(&record.live.session));
                    if let Ok(api) = api {
                        let lead = riff_client::api::LEAD;
                        let _ = api.tell(&record.me, lead, text::DAEMON_GCLOUD_SIGNIN).await;
                    }
                }
                Err(e)
            }
        }
    }

    /// The person of the session of `record` gave a prompt
    /// (01M5GVQQ8NK3RD9CM3VTGXEHZQ). For the lead, the session with a
    /// terminal, it tells riff-server: the prompt ends the block of the
    /// lead (01M48VDWPDYRPEAXHR1MYDN1M7). A prompt of a worker is a wake
    /// of the daemon, not of a person: it changes nothing. A session that
    /// left the riff makes no call ([`World::marks`]).
    pub async fn prompted(&self, record: &Record) -> Result<()> {
        let session = &record.live.session;
        if !record.terminal
            || (self.world.marks()).is_some_and(|dir| riff_client::marks::left(&dir, session))
        {
            return Ok(());
        }
        let api =
            riff_client::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
        api.prompted_in_container(&record.me).await?;
        Ok(())
    }

    /// Posts each refusal of auto mode of `denials` as the session of
    /// `record`, and gives the number that it posted
    /// (01M57XY3Q6HEYQG27FE0CEQ7HB). The session with a terminal is the
    /// lead, with no claim. Each other session is a worker in a
    /// container, with the claim of its item. A session that left the
    /// riff posts none, and gives the number of `denials`: the relay
    /// keeps none of them ([`World::marks`]).
    pub async fn post_denials(
        &self,
        record: &Record,
        denials: &[crate::denied::Denial],
    ) -> Result<usize> {
        use crate::denied::{Role, Where};
        let session = &record.live.session;
        if (self.world.marks()).is_some_and(|dir| riff_client::marks::left(&dir, session)) {
            return Ok(denials.len());
        }
        let api =
            riff_client::api::Api::new(&record.server).signed_in(Some(&record.live.session))?;
        let at = if record.terminal {
            Where::of(&record.me, Role::Lead, None)
        } else {
            Where::of(&record.me, Role::Container, Some(&record.live.item))
        };
        let lines: Vec<String> = denials
            .iter()
            .map(|d| crate::denied::line(d, &at))
            .collect();
        Ok(crate::denied::post(&api, &record.me, &lines).await)
    }

    /// Takes over the live session `name` for the person of `out`
    /// (01M4Z60B83ZH10HYKW7X1C0SSS): see "A takeover" in the design. It
    /// interrupts the turn, or with `at_end` waits for its end. It gives
    /// the session back at a [`Then::GoOn`] on `from`, or at the end of
    /// `from`.
    async fn takeover(
        &self,
        name: &str,
        at_end: bool,
        mut from: Lines<BufReader<OwnedReadHalf>>,
        out: &Out,
    ) {
        let Some(owned) = self.lock().get(name).cloned() else {
            out.say(Say::Failed {
                error: text::daemon_no_session(name),
            });
            return;
        };
        {
            let mut state = owned.lock();
            let refused = if state.record.terminal {
                Some(text::takeover_terminal(name))
            } else if state.taker.as_ref().is_some_and(Out::open) {
                Some(text::takeover_runs(name))
            } else if !state.record.takeover
                && (state.relay.is_none() || state.relay_protocol < TAKEOVER_PROTOCOL)
            {
                Some(text::takeover_no_relay(name))
            } else {
                None
            };
            if let Some(error) = refused {
                out.say(Say::Failed { error });
                return;
            }
            state.taker = Some(out.clone());
            state.followers.push(out.clone());
            if state.held && state.record.takeover {
                out.say(Say::Held {
                    session: state.record.live.session.clone(),
                });
            } else {
                if !state.record.takeover {
                    state.record.takeover = true;
                    // The relay can interrupt only while the stdin of
                    // claude is open. An interrupt of an earlier takeover
                    // can still run.
                    state.record.interrupt |= !at_end && !state.record.closed;
                    if let Err(e) = owned.save(&state) {
                        eprintln!("riff daemon: {e:#}");
                    }
                    if let Some(relay) = &state.relay {
                        relay.say(Say::Hold {
                            interrupt: state.record.interrupt,
                        });
                    }
                }
                out.say(Say::Line {
                    text: if state.record.interrupt {
                        text::takeover_interrupts(name)
                    } else {
                        text::takeover_waits(name)
                    },
                });
            }
        }
        println!("{}", text::takeover_asked(name));
        loop {
            match from.next_line().await {
                Ok(Some(line)) if matches!(serde_json::from_str::<Then>(&line), Ok(Then::GoOn)) => {
                    break;
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let mut state = owned.lock();
        if state.taker.as_ref().is_some_and(|t| t.same(out)) {
            state.taker = None;
        }
        // A session that ended has no record to give back.
        if state.ended {
            return;
        }
        state.record.takeover = false;
        state.go_on();
        if let Err(e) = owned.save(&state) {
            eprintln!("riff daemon: {e:#}");
        }
        println!("{}", text::takeover_ended(name));
        out.say(Say::Done);
    }

    /// Serves the riff tools of the session `owned` on one connection.
    /// With `client`, the initialize request of `claude`, it skips the
    /// handshake.
    async fn mcp(
        &self,
        owned: &Owned,
        client: Option<Value>,
        lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    ) {
        use rmcp::ServiceExt;
        let record = owned.lock().record.clone();
        let tools = match self.tools_of(&record) {
            Ok(tools) => tools,
            Err(e) => {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
                return;
            }
        };
        // The reader may hold the first lines of the client already.
        let stream = tokio::io::join(lines.into_inner(), write);
        let client = client.and_then(|c| serde_json::from_value(c).ok());
        let served = match client {
            Some(client) => Ok(rmcp::service::serve_directly(tools, stream, Some(client))),
            None => tools.serve(stream).await,
        };
        if let Ok(service) = served {
            let _ = service.waiting().await;
        }
    }

    /// Looks if the container of the session `record`, that this daemon took
    /// up, reaches the network (01M5SMQBTA3HQXPKKK01ZGFB4H): a container that
    /// an older daemon started has its network process in the cgroup of the
    /// unit, and the restart of the daemon ends it. The container runs on
    /// with no network, and `claude` in it cannot reach its API. A worker or
    /// a verify with no network ends as `riff daemon end` ends it: the daemon
    /// frees its item and stops the container, so the wave starts the item
    /// again. A lead with no network gets one line in riff chat and in
    /// [`Status::lost`]: the person ends it. When podman cannot say, the
    /// session counts as connected.
    async fn lost_network(self: Arc<Self>, record: Record) {
        let name = &record.live.name;
        if self.runtime.reaches(name).await.unwrap_or(true) {
            return;
        }
        if record.terminal {
            self.lost
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(record.live.session.clone());
            let line = text::lead_lost_network(&record.me.place().repo_text(), name);
            println!("{line}");
            self.world.chat(&record, &line).await;
            return;
        }
        println!("{}", text::daemon_no_network(name));
        match self.end_session(name).await {
            Ok(lines) => lines.iter().for_each(|line| println!("{line}")),
            Err(e) => eprintln!("riff daemon: {name}: {e:#}"),
        }
    }

    /// Takes up each session of a record in the folder of the daemon
    /// whose container runs, or when podman cannot say, and removes each
    /// other record
    /// (01M4ZHT01HH88KPA8628M7VBEY). Call it after [`listen`], before
    /// [`Daemon::serve`]. For each session that it took up, a task looks if
    /// the container reaches the network (`lost_network`).
    pub async fn recover(self: &Arc<Self>) {
        let Ok(entries) = std::fs::read_dir(self.dir.join(LIVE)) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let record = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Record>(&text).ok());
            let Some(record) = record else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            let name = record.live.name.clone();
            // When podman cannot say, the daemon takes up the session,
            // as its watch does.
            if !self.runtime.running(&name).await.unwrap_or(true) {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_dir_all(session_dir(&self.dir, &name));
                println!("{}", text::daemon_session_gone(&name));
                if record.terminal {
                    let (world, record) = (self.world.clone(), record.clone());
                    tokio::spawn(async move { world.leave(&record).await });
                }
                // The job of a wave does its steps after the end now:
                // the session ended while no daemon ran. The sweep keeps
                // its folder from now on (01M53S7WDPQ2GWHJWJ6SZXZ5PN).
                if record.job.is_some() {
                    let ended = Ended {
                        code: None,
                        failed: true,
                    };
                    let job = self.in_job(&record.live.item);
                    tokio::spawn(crate::wave::finish(self.clone(), record, ended, job));
                }
                continue;
            }
            match self.own(record.clone(), Scrub::default(), true) {
                Ok((owned, notes)) => {
                    println!("{}", text::daemon_takes_up(&name));
                    let looked = record.clone();
                    // The lead keeps its session on riff-server alive.
                    if record.terminal {
                        self.world.resume(&record);
                    }
                    // The job of a wave goes on: the keep-alive of its
                    // claim, and its steps after the end.
                    if record.job.is_some() {
                        let (out, said) = Out::keep();
                        owned.lock().followers.push(out);
                        self.world.resume(&record);
                        let job = self.in_job(&record.live.item);
                        tokio::spawn(crate::wave::after(self.clone(), record, said, job));
                    }
                    tokio::spawn(self.clone().watch(owned, notes, None));
                    tokio::spawn(self.clone().lost_network(looked));
                }
                Err(e) => eprintln!("riff daemon: {name}: {e:#}"),
            }
        }
    }
}

/// Gives the session `owned` each wake of riff-server from `world`, until
/// the task ends: while the input of `claude` is open, as one input
/// each; after the close, as a refusal to the sender, once for each
/// sender (01M5792DP1KZR2XGSCMRWDHCGZ, 01M5792DRA12JD032F3H8JVC0Z,
/// 01M57AVKBHFVYJBRT0W2ZTAJDK). So two closed sessions do not refuse
/// the refusals of each other with no end. A new watch of riff-server
/// gives the newest unread wake again: the session gets it once.
async fn give_wakes(world: Arc<dyn World>, owned: Arc<Owned>) {
    let record = owned.lock().record.clone();
    let (to, mut wakes) = mpsc::unbounded_channel();
    let watch = world.wakes(&record, to);
    tokio::pin!(watch);
    let mut seen = BTreeSet::new();
    let mut refused = BTreeSet::new();
    loop {
        let wake = tokio::select! {
            () = &mut watch => return,
            wake = wakes.recv() => match wake {
                Some(wake) => wake,
                None => return,
            },
        };
        if !seen.insert((wake.thread.to_string(), wake.seq)) {
            continue;
        }
        let given = {
            let mut state = owned.lock();
            let given = state.wake(&wake);
            if let Err(e) = owned.save(&state) {
                eprintln!("riff daemon: {e:#}");
            }
            given
        };
        if !given && refused.insert(wake.from.to_string()) {
            world.refuse(&record, &wake).await;
        }
    }
}

/// Reads each line of `from` until its end: a [`Then::Stop`] asks to
/// stop the session `owned`.
fn stop_on(mut from: Lines<BufReader<OwnedReadHalf>>, owned: Arc<Owned>) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(Some(line)) = from.next_line().await {
            if matches!(serde_json::from_str::<Then>(&line), Ok(Then::Stop)) {
                owned.stop.notify_one();
            }
        }
    })
}

/// Reads the [`Hello`] of the line `first`: an error when it is of
/// another version, or not a hello. The error shows no part of the
/// line: it can hold a secret.
fn hello(first: &str) -> Result<Ask, String> {
    #[derive(Deserialize)]
    struct Version {
        protocol: u32,
    }
    match serde_json::from_str::<Version>(first) {
        Ok(Version { protocol }) if protocol != PROTOCOL => {
            return Err(text::daemon_protocol(protocol, PROTOCOL));
        }
        Ok(_) => {}
        Err(_) => return Err(text::DAEMON_NO_HELLO.into()),
    }
    serde_json::from_str::<Hello>(first)
        .map(|hello| hello.ask)
        .map_err(|_| text::DAEMON_NO_HELLO.into())
}

/// Reads the [`SessionAsk`] of the line `first` on the socket of a
/// session: an error when its version is not [`compatible`], or when it
/// is not an ask. The error shows no part of the line.
fn session_hello(first: &str) -> Result<SessionAsk, String> {
    #[derive(Deserialize)]
    struct Version {
        protocol: u32,
    }
    match serde_json::from_str::<Version>(first) {
        Ok(Version { protocol }) if !compatible(protocol) => {
            return Err(text::daemon_protocol(protocol, PROTOCOL));
        }
        Ok(_) => {}
        Err(_) => return Err(text::DAEMON_NO_HELLO.into()),
    }
    serde_json::from_str::<SessionAsk>(first).map_err(|_| text::DAEMON_NO_HELLO.into())
}

/// The longest time that `riff` waits for the daemon of the unit while
/// systemd starts it ([`Client::connect`]).
pub const START_WAIT: Duration = Duration::from_secs(5);

/// True when the state `state` of `systemctl is-active` is of a unit
/// that runs or starts: its daemon answers soon.
///
/// ```
/// assert!(riff::daemon::starts("active\n"));
/// assert!(riff::daemon::starts("activating"));
/// assert!(!riff::daemon::starts("inactive"));
/// assert!(!riff::daemon::starts("failed"));
/// ```
pub fn starts(state: &str) -> bool {
    matches!(state.trim(), "active" | "activating" | "reloading")
}

/// True when the daemon of this machine has a unit that runs or starts.
/// With `RIFF_HOME` or `RIFF_DATA`, the daemon is not the daemon of the
/// unit: false.
fn unit_starts() -> bool {
    use crate::service::ServiceManager;
    crate::run::machine_data()
        && crate::service::Systemd::here()
            .and_then(|unit| unit.state())
            .is_ok_and(|state| starts(&state))
}

/// A connection of `riff` to the daemon.
pub struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
}

impl Client {
    /// Connects to the daemon in the folder `dir`. While systemd starts
    /// the unit of the daemon, it tries again for [`START_WAIT`]: the
    /// daemon listens a short time after its start (01M5MS2T8VY8NX4VBMQTCKWDKH). With no
    /// daemon, the error says how to start it.
    pub async fn connect(dir: &Path) -> Result<Client> {
        let started = std::time::Instant::now();
        let mut waits: Option<bool> = None;
        let conn = loop {
            match connect_in(dir, SOCKET).await {
                Ok(conn) => break conn,
                Err(_)
                    if started.elapsed() < START_WAIT && *waits.get_or_insert_with(unit_starts) =>
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(_) => bail!("{}", text::daemon_none(&socket(dir))),
            }
        };
        let (read, write) = conn.into_split();
        Ok(Client {
            lines: BufReader::new(read).lines(),
            write,
        })
    }

    async fn line(&mut self, value: &impl Serialize) -> Result<()> {
        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        self.write.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Sends the ask `ask`: the first line of the connection.
    pub async fn ask(&mut self, ask: Ask) -> Result<()> {
        self.line(&Hello {
            protocol: PROTOCOL,
            ask,
        })
        .await
    }

    /// Asks the daemon to stop the session of this connection.
    pub async fn stop(&mut self) -> Result<()> {
        self.line(&Then::Stop).await
    }

    /// Tells the daemon that the terminal of the takeover of this
    /// connection ended: the session goes on.
    pub async fn go_on(&mut self) -> Result<()> {
        self.line(&Then::GoOn).await
    }

    /// The next line of the daemon, or `None` when the daemon closed
    /// the connection.
    pub async fn next(&mut self) -> Result<Option<Say>> {
        match self.lines.next_line().await? {
            Some(line) => Ok(Some(
                serde_json::from_str(&line)
                    .context("the riff daemon said a line that riff cannot read")?,
            )),
            None => Ok(None),
        }
    }
}

/// Says each line of `done` and [`Say::Done`], or [`Say::Failed`].
fn say_lines(out: &Out, done: Result<Vec<String>>) {
    match done {
        Ok(lines) => {
            for text in lines {
                out.say(Say::Line { text });
            }
            out.say(Say::Done);
        }
        Err(e) => out.say(Say::Failed {
            error: format!("{e:#}"),
        }),
    }
}

/// The state of the daemon in the folder `dir`.
pub async fn status(dir: &Path) -> Result<Status> {
    let mut client = Client::connect(dir).await?;
    client.ask(Ask::Status).await?;
    match client.next().await? {
        Some(Say::Status(status)) => Ok(*status),
        Some(Say::Failed { error }) => bail!("{error}"),
        _ => bail!("{}", text::DAEMON_GONE),
    }
}

/// The longest wait of the look of the rollout for the status of the
/// daemon ([`status_within`], 01M558EY88CACNZ55FWMRT8RP7).
pub const STATUS_WAIT: Duration = Duration::from_secs(5);

/// [`status`], with the time limit `limit`. A daemon that takes the
/// connection and gives no answer in `limit`, for example a status that
/// waits on a lock, gives an error: it stops no caller.
pub async fn status_within(dir: &Path, limit: Duration) -> Result<Status> {
    tokio::time::timeout(limit, status(dir))
        .await
        .map_err(|_| anyhow!("{}", text::daemon_silent(limit)))?
}

/// Asks the daemon in the folder `dir` to have the image `image`. It
/// shows each line of the daemon with `show`.
pub async fn image(dir: &Path, image: &str, show: impl FnMut(&str)) -> Result<()> {
    let ask = Ask::Image {
        image: image.into(),
    };
    ask_done(dir, ask, show).await
}

/// Sends `ask` to the daemon in the folder `dir`, shows each line of
/// the daemon with `show`, and waits for [`Say::Done`].
pub async fn ask_done(dir: &Path, ask: Ask, mut show: impl FnMut(&str)) -> Result<()> {
    let mut client = Client::connect(dir).await?;
    client.ask(ask).await?;
    loop {
        match client.next().await? {
            Some(Say::Line { text }) => show(&text),
            Some(Say::Done) => return Ok(()),
            Some(Say::Failed { error }) => bail!("{error}"),
            _ => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff daemon sweep`: asks the daemon in the folder `dir` to sweep the
/// folders of its sessions now ([`Ask::Sweep`],
/// 01M53NJ5Y2CMY5MWF9328KW7DE). It prints each line of the daemon.
pub async fn ask_sweep(dir: &Path) -> Result<()> {
    ask_done(dir, Ask::Sweep, |line| println!("{line}")).await
}

/// `riff daemon events NAME`: shows each event of the live session in
/// the container `name` from its start, as `riff run` does, and each new
/// one until the end of the session (01M4ZKX0KMS58NR2E9AF8229A8). A
/// Ctrl-C ends the view: the session goes on. It gives the exit code: 1
/// when the session failed.
pub async fn show_events(dir: &Path, name: &str) -> Result<i32> {
    let mut client = Client::connect(dir).await?;
    client
        .ask(Ask::Follow {
            name: name.into(),
            after: 0,
        })
        .await?;
    let mut view = crate::run::View::default();
    loop {
        match client.next().await? {
            Some(Say::Event { line, .. }) => view.show(&line),
            Some(Say::Stderr { line }) => eprintln!("{line}"),
            Some(Say::Line { text }) => println!("{text}"),
            Some(Say::Ended(ended)) => {
                println!("{}", text::daemon_session_ends(name, ended.code));
                return Ok(i32::from(ended.failed));
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff daemon`: runs the daemon of this machine until a SIGTERM or a
/// SIGINT. It takes up each session that runs ([`Daemon::recover`]).
/// The stop stops no session (01M4ZDTVH57VE77YT20G65A4BQ).
pub async fn serve_here() -> Result<()> {
    // Its working folder can be the work tree of another repository
    // (01M58AW5SBG3ZP7149ZY66VTYW).
    crate::auto_update::no_place();
    #[cfg(target_os = "linux")]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let dir = dir()?;
        let listener = listen(&dir).await?;
        let mut term = signal(SignalKind::terminate())?;
        let mut int = signal(SignalKind::interrupt())?;
        let daemon = Daemon::new(crate::podman::Podman, dir.clone())?.with_again(again_here());
        let daemon = Arc::new(daemon);
        let status = daemon.status().await;
        println!(
            "{}",
            text::daemon_listens(&socket(&dir), status.pid, &status.build)
        );
        daemon.recover().await;
        daemon.run_waves();
        daemon.serve_repos();
        daemon.sweep_every();
        daemon.stale_every();
        daemon.leads_again_every();
        daemon.watch_every(crate::watches::every());
        // The steps of an update of this release: the rules of Claude
        // Code, the unit, the images (01M51QFPJAABDP1V7WFY8NQR60).
        let steps = dir.clone();
        tokio::spawn(async move {
            crate::update_steps::at_start(&crate::podman::Podman, &steps).await;
        });
        // A new riff on disk (01M517BTP6BAGTRBYWPX9GPVE0).
        if let Ok(manager) = crate::service::Systemd::here() {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let old = || old_here(&manager);
                let restart = || crate::service::ServiceManager::try_restart(&manager);
                daemon.restart_when_old(old, restart).await;
            });
        }
        let stop = async move {
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
            }
        };
        daemon.clone().serve(listener, stop).await?;
        let _ = std::fs::remove_file(socket(&dir));
        println!("{}", text::DAEMON_STOPS);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        bail!("{}", text::DAEMON_LINUX_ONLY)
    }
}

/// The new start of the daemon of this machine on a new riff, for
/// `riff daemon restart` ([`remote::Again`]): systemd, as
/// [`Daemon::restart_when_old`] does. With no systemd, it never runs an
/// old riff.
#[cfg(target_os = "linux")]
fn again_here() -> remote::Again {
    match crate::service::Systemd::here() {
        Ok(manager) => {
            let manager = Arc::new(manager);
            let old = manager.clone();
            remote::Again {
                old: Box::new(move || old_here(&*old)),
                restart: Box::new(move || crate::service::ServiceManager::try_restart(&*manager)),
            }
        }
        Err(_) => remote::Again {
            old: Box::new(|| false),
            restart: Box::new(|| bail!("{}", text::DAEMON_LINUX_ONLY)),
        },
    }
}

/// `riff daemon host`: asks the daemon in the folder `dir` to serve the
/// lead of the repository of the place `here`, on the riff-server
/// `server`, with the main clone of the folder `work`
/// (01M579Z9JE6HY9HG3PWR1RR0RQ). It prints each line of the daemon.
pub async fn ask_serve(
    dir: &Path,
    work: &Path,
    here: &riff_core::name::Place,
    server: &str,
) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let (main, _) = crate::run::main_clone(work)?;
    let wave = Wave {
        repo: here.repo_text(),
        main,
        server: server.to_owned(),
        host: here.host().to_owned(),
    };
    ask_done(dir, Ask::Serve { wave }, |line| println!("{line}")).await
}

/// `riff daemon end NAME`: asks the daemon in the folder `dir` to free
/// the item of the session in the container `name` and to stop it
/// (01M579Z9VRD12FRK6DSYVKDHQB). It prints each line of the daemon.
pub async fn ask_end(dir: &Path, name: &str) -> Result<()> {
    let ask = Ask::End { name: name.into() };
    ask_done(dir, ask, |line| println!("{line}")).await
}

/// `riff daemon status`: shows the daemon in the folder `dir` and its
/// sessions, or, with no daemon, the state of its unit. It gives the
/// exit code: 1 when no daemon answers (01M4ZDTVNBKDM64043GGWQEV4S).
pub async fn show_status(dir: &Path, manager: &impl crate::service::ServiceManager) -> i32 {
    match status(dir).await {
        Ok(status) => {
            print!("{}", text::daemon_status(&status, now_ms()));
            println!("{}", crate::compile_cache::shown());
            0
        }
        Err(e) => {
            println!("{e:#}");
            match manager.state() {
                Ok(state) => println!("{}", text::daemon_unit_state(&state)),
                Err(e) => println!("{e:#}"),
            }
            1
        }
    }
}

/// `riff daemon install`: installs the unit of the daemon with
/// `manager`, starts it again, and waits up to `wait` for the daemon in
/// the folder `dir` to answer (01M4ZDTVK74596M0ZA46N51NKV).
pub async fn install(
    manager: &impl crate::service::ServiceManager,
    unit: &crate::service::Unit,
    dir: &Path,
    wait: Duration,
) -> Result<()> {
    if !crate::run::machine_data() {
        bail!("{}", text::DAEMON_INSTALL_RIFF_HOME);
    }
    let path = manager.install(unit)?;
    println!("{}", text::daemon_installed(&path));
    let start = std::time::Instant::now();
    loop {
        if let Ok(status) = status(dir).await {
            println!("{}", text::daemon_runs(&status));
            return Ok(());
        }
        if start.elapsed() >= wait {
            let state = manager.state().unwrap_or_else(|e| format!("{e:#}"));
            bail!("{}", text::daemon_no_answer(&state, &socket(dir)));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `riff daemon uninstall`: stops the daemon, and removes its unit with
/// `manager` (01M501M0T0JBWGMMHCT7HT95T9). It keeps the folder of the
/// daemon and each container. It gives the line for the person.
pub fn uninstall(
    manager: &impl crate::service::ServiceManager,
    unit_path: &Path,
) -> Result<String> {
    if !crate::run::machine_data() {
        bail!("{}", text::DAEMON_UNINSTALL_RIFF_HOME);
    }
    Ok(match manager.uninstall()? {
        Some(path) => text::daemon_uninstalled(&path),
        None => text::daemon_not_installed(unit_path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::HIDDEN;
    use crate::runtime::{Exit, Fake};
    use crate::service::{ServiceManager, Unit};
    use tokio::sync::oneshot;

    pub(super) const IMAGE: &str = "localhost/riff-session:latest";

    /// The events of one turn of a fake `claude`, with a token in a
    /// tool result.
    const TURN: [&str; 3] = [
        r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"gho_secret1"}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1}"#,
    ];

    pub(super) fn me(name: &str) -> SessionUri {
        format!("riff://mike@thelio/como-technologies/riff?session={name}-session#issue-1")
            .parse()
            .unwrap()
    }

    fn spec(name: &str, clone: &Path) -> SessionSpec {
        SessionSpec {
            name: name.into(),
            image: IMAGE.into(),
            home: "/home/mike".into(),
            workdir: clone.to_owned(),
            binds: vec![Bind {
                source: Source::Path(clone.to_owned()),
                target: clone.to_owned(),
                write: true,
            }],
            env: vec![Env::Host("GH_TOKEN".into())],
            memory: "8g".into(),
            cpus: 4,
            command: vec!["claude".into()],
        }
    }

    /// A daemon on `fake` in a temp folder, with a session folder for
    /// each session.
    pub(super) struct Here {
        pub(super) root: tempfile::TempDir,
        pub(super) dir: PathBuf,
        pub(super) fake: Fake,
        quiet: Duration,
        keep_end: Duration,
        grace: Option<Duration>,
        world: Option<Arc<dyn World>>,
        pub(super) daemon: Arc<Daemon<Fake>>,
        stop: Option<oneshot::Sender<()>>,
        served: JoinHandle<Result<()>>,
    }

    /// A daemon on `fake` in the folder `dir`, which serves until its
    /// stop.
    type Served = (
        Arc<Daemon<Fake>>,
        oneshot::Sender<()>,
        JoinHandle<Result<()>>,
    );

    /// A static riff beside the folder `dir` of a daemon, for its
    /// sessions: the riff of a test is not static.
    fn static_riff(dir: &Path) -> PathBuf {
        crate::release::fake_riff(&dir.with_file_name("static-riff"), None)
    }

    /// The riff-server of the daemon is `world` when it is given, else
    /// the riff-server of this machine.
    async fn serve_in(
        fake: &Fake,
        dir: &Path,
        (quiet, keep_end, grace): (Duration, Duration, Option<Duration>),
        world: Option<Arc<dyn World>>,
    ) -> Served {
        let listener = listen(dir).await.unwrap();
        riff_client::secrets::mock_store();
        let mut daemon = Daemon::new(fake.clone(), dir.to_owned())
            .unwrap()
            .with_quiet(quiet)
            .with_keep_end(keep_end)
            .with_relay(static_riff(dir))
            .with_gcloud(crate::cloud::Gcloud::at(dir.with_file_name("gcloud")));
        if let Some(world) = world {
            daemon = daemon.with_world(world);
        }
        if let Some(grace) = grace {
            daemon = daemon.with_grace(grace);
        }
        let daemon = Arc::new(daemon);
        let (stop, stopped) = oneshot::channel::<()>();
        let served = tokio::spawn(daemon.clone().serve(listener, async {
            let _ = stopped.await;
        }));
        (daemon, stop, served)
    }

    impl Here {
        pub(super) async fn new(fake: Fake) -> Here {
            Here::with_quiet(fake, QUIET_LIMIT).await
        }

        /// A daemon on `fake` with the quiet limit `quiet`.
        async fn with_quiet(fake: Fake, quiet: Duration) -> Here {
            Here::with_world(fake, quiet, None).await
        }

        /// A daemon on `fake` with the quiet limit `quiet` and the
        /// riff-server of `world`.
        pub(super) async fn with_world(
            fake: Fake,
            quiet: Duration,
            world: Option<Arc<dyn World>>,
        ) -> Here {
            Here::with_keep_end(fake, (quiet, KEEP_END), world).await
        }

        /// A daemon on `fake` with the quiet limit and the keep of the
        /// end of a session of `times`, and the riff-server of `world`.
        async fn with_keep_end(
            fake: Fake,
            times: (Duration, Duration),
            world: Option<Arc<dyn World>>,
        ) -> Here {
            let (quiet, keep_end) = times;
            Here::with_times(fake, (quiet, keep_end, None), world).await
        }

        /// A daemon on `fake` with the quiet limit `quiet` and the grace
        /// `grace` of the sessions of a wave.
        async fn with_grace(fake: Fake, quiet: Duration, grace: Duration) -> Here {
            Here::with_times(fake, (quiet, KEEP_END, Some(grace)), None).await
        }

        /// A daemon on `fake` with the quiet limit, the keep of the end
        /// of a session and the grace of `times`, and the riff-server of
        /// `world`.
        async fn with_times(
            fake: Fake,
            times: (Duration, Duration, Option<Duration>),
            world: Option<Arc<dyn World>>,
        ) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let (daemon, stop, served) = serve_in(&fake, &dir, times, world.clone()).await;
            let (quiet, keep_end, grace) = times;
            Here {
                root,
                dir,
                fake,
                quiet,
                keep_end,
                grace,
                world,
                daemon,
                stop: Some(stop),
                served,
            }
        }

        /// Stops the daemon, and starts a new one in the same folder.
        async fn restart(&mut self) {
            self.stop().await;
            let times = (self.quiet, self.keep_end, self.grace);
            let (daemon, stop, served) =
                serve_in(&self.fake, &self.dir, times, self.world.clone()).await;
            self.daemon = daemon;
            self.stop = Some(stop);
            self.served = served;
        }

        /// The [`Start`] of the session `name` of `item`.
        pub(super) fn start(&self, name: &str, item: &str) -> Start {
            start_in(self.root.path(), name, item)
        }

        /// Starts `start` as the daemon starts the session of a wave: with
        /// a job. Gives the name of the session, the task that ends with it,
        /// and what the daemon says, after the line `Started`.
        async fn launched(
            &self,
            start: Start,
        ) -> (String, JoinHandle<Record>, mpsc::UnboundedReceiver<Say>) {
            let job = Job {
                wave: Wave {
                    repo: "o/r".into(),
                    main: self.root.path().to_owned(),
                    server: start.server.clone(),
                    host: "h".into(),
                },
                work: crate::wave::Work::Build { issue: 12 },
            };
            let (out, mut said) = Out::keep();
            let daemon = self.daemon.clone();
            let task =
                tokio::spawn(
                    async move { daemon.launch(start, Some(job), None, None, &out).await },
                );
            loop {
                match said.recv().await {
                    Some(Say::Started { name, .. }) => return (name, task, said),
                    Some(Say::Failed { error }) => panic!("{error}"),
                    other => assert!(other.is_some(), "no start"),
                }
            }
        }

        /// Asks the daemon to start `start`. Gives the client after the
        /// line `Started`.
        pub(super) async fn started(&self, start: Start) -> (Client, String) {
            let mut client = Client::connect(&self.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            match client.next().await.unwrap() {
                Some(Say::Started { name, socket }) => {
                    assert_eq!(socket, session_dir(&self.dir, &name));
                    (client, name)
                }
                other => panic!("{other:?}"),
            }
        }

        pub(super) async fn stop(&mut self) {
            let _ = self.stop.take().unwrap().send(());
            (&mut self.served).await.unwrap().unwrap();
        }
    }

    /// The [`Start`] of the session `name` of `item`, with its folder in
    /// `root`.
    fn start_in(root: &Path, name: &str, item: &str) -> Start {
        let folder = root.join("sessions").join(name);
        let clone = folder.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        Start {
            session: format!("{name}-session"),
            item: item.into(),
            folder,
            spec: spec(name, &clone),
            start: "Do it.".into(),
            values: vec![("GH_TOKEN".into(), "gho_secret1".into())],
            secrets: vec!["gho_secret1".into()],
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            terminal: false,
        }
    }

    /// The relay of a session, as the test plays it.
    struct Relay {
        lines: Lines<BufReader<OwnedReadHalf>>,
        write: OwnedWriteHalf,
    }

    impl Relay {
        /// Connects to the socket of the session in the folder `dir` as
        /// its relay, and gives the resume.
        async fn connect(dir: &Path, inputs: u64, closed: bool) -> (Relay, u64) {
            Relay::connect_as(
                dir,
                SessionAsk::Relay {
                    protocol: PROTOCOL,
                    inputs,
                    closed,
                    held: false,
                },
            )
            .await
        }

        /// Connects with the hello `hello`, and gives the resume.
        async fn connect_as(dir: &Path, hello: SessionAsk) -> (Relay, u64) {
            let conn = connect_in(dir, SOCKET).await.unwrap();
            let (read, write) = conn.into_split();
            let mut relay = Relay {
                lines: BufReader::new(read).lines(),
                write,
            };
            relay.send(&hello).await;
            match relay.next().await {
                Say::Resume { events } => (relay, events),
                other => panic!("{other:?}"),
            }
        }

        async fn send(&mut self, value: &impl Serialize) {
            let line = serde_json::to_string(value).unwrap();
            self.write
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }

        async fn next(&mut self) -> Say {
            let line = self.lines.next_line().await.unwrap().unwrap();
            serde_json::from_str(&line).unwrap()
        }

        /// Sends the event `seq`, and waits for its ack.
        async fn event(&mut self, seq: u64, line: &str) {
            self.send(&Up::Event {
                seq,
                line: line.into(),
            })
            .await;
            loop {
                match self.next().await {
                    Say::Ack { seq: acked } if acked >= seq => return,
                    Say::Ack { .. } | Say::Close => {}
                    other => panic!("{other:?}"),
                }
            }
        }

        /// Sends the exit, and waits for its ack.
        async fn exit(&mut self, seq: u64, code: i32) {
            self.send(&Up::Exit {
                seq,
                code: Some(code),
            })
            .await;
            loop {
                match self.next().await {
                    Say::Ack { seq: acked } if acked == seq => return,
                    Say::Close | Say::Ack { .. } => {}
                    other => panic!("{other:?}"),
                }
            }
        }
    }

    /// Each line of `client` up to the end of the session.
    async fn to_the_end(client: &mut Client) -> (Vec<Say>, Option<Say>) {
        let mut said = Vec::new();
        while let Some(say) = client.next().await.unwrap() {
            match say {
                Say::Ended(_) | Say::Failed { .. } => return (said, Some(say)),
                say => said.push(say),
            }
        }
        (said, None)
    }

    /// Asks the socket of a session in the folder `dir` who it is, with
    /// the version `protocol`.
    async fn who_in(dir: &Path, protocol: u32) -> Say {
        let conn = connect_in(dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let hello = serde_json::to_string(&SessionAsk::Hello { protocol }).unwrap();
        write
            .write_all(format!("{hello}\n").as_bytes())
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }

    /// Asks the socket of a session in the folder `dir` whether it is the
    /// lead, for the release cycle.
    async fn lead_in(dir: &Path) -> bool {
        let conn = connect_in(dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let ask = serde_json::to_string(&SessionAsk::Caller { protocol: PROTOCOL }).unwrap();
        write
            .write_all(format!("{ask}\n").as_bytes())
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        match serde_json::from_str(&line).unwrap() {
            Say::Caller { lead, .. } => lead,
            other => panic!("{other:?}"),
        }
    }

    /// The events of `said`.
    fn events(said: &[Say]) -> Vec<(u64, String)> {
        said.iter()
            .filter_map(|s| match s {
                Say::Event { seq, line } => Some((*seq, line.clone())),
                _ => None,
            })
            .collect()
    }

    /// The caps of a container: `--cpus` and `--memory`, as the daemon
    /// gives them to the runtime.
    fn caps(spec: &SessionSpec) -> (u32, &str) {
        (spec.cpus, spec.memory.as_str())
    }

    #[tokio::test]
    async fn each_new_container_gets_the_caps_of_the_host_and_keeps_them() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_first, first) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let spec = here.fake.spec(&first).unwrap();
        assert_eq!(caps(&spec), (8, "8g"));
        // riff sets no build jobs (01M5SMW28KT5D5HPRH7KAZ8TVC).
        assert!(
            !spec
                .env
                .iter()
                .any(|e| matches!(e, Env::Value(n, _) if n == "CARGO_BUILD_JOBS"))
        );

        let change = crate::room::Change {
            cpus: Some(6),
            memory_gb: Some(12),
            ..crate::room::Change::default()
        };
        crate::room::set(&here.dir, change).unwrap();
        let (_second, second) = here
            .started(here.start("riff-issue-13-2b3c4d5e", "issue-13"))
            .await;
        let spec = here.fake.spec(&second).unwrap();
        assert_eq!(caps(&spec), (6, "12g"));
        // The session that runs keeps the caps of its start.
        let spec = here.fake.spec(&first).unwrap();
        assert_eq!(caps(&spec), (8, "8g"));
    }

    /// The claude command of the lead, of a work item and of a verify has
    /// `--effort xhigh` by default, and the level that `riff daemon limit`
    /// set for its role. A session that runs keeps its effort.
    #[tokio::test]
    async fn each_new_session_gets_the_effort_of_its_role() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let effort = |spec: &SessionSpec| {
            let at = spec.command.iter().position(|a| a == "--effort").unwrap();
            assert_eq!(spec.command[at - 1], "claude");
            spec.command[at + 1].clone()
        };
        let mut first = Vec::new();
        for (name, item) in [
            ("riff-lead-o-r", "lead"),
            ("riff-issue-12-1a2b3c4d", "issue-12"),
            ("riff-verify-issue-12-1a2b3c4d", "verify-issue-12"),
        ] {
            let (_c, id) = here.started(here.start(name, item)).await;
            first.push(id);
        }
        for id in &first {
            assert_eq!(effort(&here.fake.spec(id).unwrap()), "xhigh");
        }

        let change = crate::room::Change {
            sessions: Some(9),
            effort: Some(crate::room::Effort::High),
            effort_lead: Some(crate::room::Effort::Max),
            effort_verify: Some(crate::room::Effort::Low),
            ..crate::room::Change::default()
        };
        crate::room::set(&here.dir, change).unwrap();
        let mut levels = Vec::new();
        for (name, item) in [
            ("riff-lead-o-x", "lead"),
            ("riff-issue-13-2b3c4d5e", "issue-13"),
            ("riff-verify-issue-13-2b3c4d5e", "verify-issue-13"),
        ] {
            let (_c, id) = here.started(here.start(name, item)).await;
            levels.push(effort(&here.fake.spec(&id).unwrap()));
        }
        assert_eq!(levels, ["max", "high", "low"]);
        assert_eq!(effort(&here.fake.spec(&first[0]).unwrap()), "xhigh");
    }

    /// 01M57840K7SNPS243CYMYNYVFX, 01M57840R2HNXGP9B2ENSKTWD3.
    #[tokio::test]
    async fn an_added_mcp_server_is_in_the_mcp_config_of_each_new_session() {
        use crate::session_mcp::{self, VAR};
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_first, first) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let spec = here.fake.spec(&first).unwrap();
        assert!(!spec.env.contains(&Env::Host(VAR.into())));
        assert_eq!(status(&here.dir).await.unwrap().mcp, ["riff"]);

        let person = serde_json::json!({
            "docs": {"type": "http", "url": "https://d", "headers": {"Authorization": "Bearer t1"}},
        });
        let names =
            session_mcp::add(&here.dir, "docs", person.as_object().unwrap(), |_| Ok(true)).unwrap();
        assert_eq!(names, ["riff", "docs"]);
        assert!(session_mcp::remove(&here.dir, "riff").is_err());
        let (_second, second) = here
            .started(here.start("riff-issue-13-2b3c4d5e", "issue-13"))
            .await;
        // The servers go beside the spec, as a token does.
        let spec = here.fake.spec(&second).unwrap();
        assert!(spec.env.contains(&Env::Host(VAR.into())));
        assert!(!format!("{spec:?}").contains("https://d"));
        let values = here.fake.values(&second);
        let (_, value) = values.iter().find(|(name, _)| name == VAR).unwrap();
        let config = crate::session_relay::mcp_config(
            RELAY_BIN.as_ref(),
            &session_mcp::from_var(Some(value)),
        );
        assert_eq!(config["mcpServers"]["docs"]["url"], "https://d");
        assert_eq!(
            config["mcpServers"]["riff"]["args"],
            serde_json::json!(["relay", "mcp"])
        );
        assert_eq!(status(&here.dir).await.unwrap().mcp, ["riff", "docs"]);
        // The session that runs keeps the servers of its start.
        assert!(here.fake.values(&first).iter().all(|(name, _)| name != VAR));
    }

    #[tokio::test]
    async fn a_session_runs_through_its_relay_and_ends_at_the_exit_of_the_relay() {
        let here = Here::new(Fake::default().with_image(IMAGE).with_events(["a note"])).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let log = start.folder.join(EVENTS);
        let (mut client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);

        // The relay gets the start text, and the close after the result.
        let (mut relay, had) = Relay::connect(&socket, 0, false).await;
        assert_eq!(had, 0);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: user_line("Do it.")
            }
        );
        for (seq, line) in (1..).zip(TURN) {
            relay.event(seq, line).await;
        }
        assert!(
            serde_json::from_str::<Record>(
                &std::fs::read_to_string(record_path(&here.dir, &name)).unwrap()
            )
            .unwrap()
            .closed
        );
        relay.exit(4, 0).await;

        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        // Each event, with no secret and its number, to riff and to the
        // log. The output of the container is a line of stderr.
        let lines = events(&said);
        assert_eq!(lines.len(), 3, "{said:?}");
        assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(lines[1].1.contains(HIDDEN) && !lines[1].1.contains("gho_secret1"));
        assert!(said.contains(&Say::Stderr {
            line: "a note".into()
        }));
        let logged: Vec<String> = lines.into_iter().map(|l| l.1).collect();
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            logged.join("\n") + "\n"
        );
        // The relay is the entrypoint: the riff of the daemon, with no
        // write, and the command of the spec after it. The riff of the
        // test daemon is a static fake.
        let spec = here.fake.spec(&name).unwrap();
        assert_eq!(
            spec.command,
            [
                "/opt/riff/riff",
                "relay",
                "--",
                "claude",
                "--effort",
                "xhigh"
            ]
        );
        assert!(spec.binds.contains(&Bind {
            source: Source::Path(static_riff(&here.dir)),
            target: RELAY_BIN.into(),
            write: false,
        }));
        // The token goes to the runtime beside the spec.
        assert_eq!(
            here.fake.values(&name),
            [("GH_TOKEN".to_owned(), "gho_secret1".to_owned())]
        );
        // The daemon owns the session no more: its socket and its record
        // are gone, and the input of the container closed.
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
        assert!(!socket.exists());
        assert!(!record_path(&here.dir, &name).exists());
        assert_eq!(here.fake.exit(&name), Some(Exit::SUCCESS));
    }

    #[tokio::test]
    async fn a_relay_that_connects_again_gets_no_input_twice_and_resends_what_the_daemon_lacks() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let log = start.folder.join(EVENTS);
        let (mut client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;
        drop(relay);

        // The relay took the input, and connects again: the daemon has
        // one event. An event that it has already gets no second line.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[1]).await;
        relay.event(3, TURN[2]).await;
        drop(relay);
        // After the result, a relay that did not close gets the close.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 3);
        assert_eq!(relay.next().await, Say::Close);
        relay.exit(4, 0).await;

        let (said, _) = to_the_end(&mut client).await;
        assert_eq!(
            events(&said).iter().map(|l| l.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 3);
    }

    /// 01M58B7H7HD0H48A0C79F8M4CR: `claude` refuses the token. The daemon
    /// marks it refused, says why in the lines of the session, and the
    /// next host check fails. The value of the token is in no event.
    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn a_refused_token_is_marked_said_and_stops_each_new_start() {
        let _token = crate::claude_token::test_token();
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let token = "sk-ant-oat01-secret1";
        crate::claude_token::store(&here.dir, token).unwrap();
        let mut start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        start.secrets.push(token.into());
        let (mut client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        let refused = format!(
            r#"{{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login {token}"}}"#
        );
        // The result closes the input: the ack of the event comes after
        // the close.
        relay.event(1, &refused).await;
        relay.exit(2, 1).await;

        let (said, _) = to_the_end(&mut client).await;
        let lines: Vec<&str> = said
            .iter()
            .filter_map(|s| match s {
                Say::Line { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&text::CLAUDE_TOKEN_REFUSED), "{said:?}");
        assert!(events(&said).iter().all(|(_, l)| !l.contains(token)));
        assert!(crate::claude_token::refused(&here.dir));
        assert_eq!(
            status(&here.dir).await.unwrap().claude_token,
            riff_client::daemons::Token::Refused
        );
        let credentials = here.dir.join("none.json");
        assert!(matches!(
            crate::run::claude_of(&here.dir, &credentials, 0),
            Err(crate::run::HostFault::TokenRefused)
        ));
    }

    /// The daemon acks no line that it lacks: it closes the connection,
    /// and the relay sends the line again at the next one
    /// (01M4ZHSZMH4Y8CB41FYGE3C3R2).
    #[tokio::test]
    async fn the_daemon_acks_no_line_that_it_lacks_and_the_relay_sends_it_again() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;

        // An exit before the events 2 and 3.
        relay
            .send(&Up::Exit {
                seq: 4,
                code: Some(0),
            })
            .await;
        assert_eq!(relay.lines.next_line().await.unwrap(), None);
        // An event after a gap.
        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay
            .send(&Up::Event {
                seq: 3,
                line: TURN[2].into(),
            })
            .await;
        assert_eq!(relay.lines.next_line().await.unwrap(), None);

        let (mut relay, had) = Relay::connect(&socket, 1, false).await;
        assert_eq!(had, 1);
        relay.event(2, TURN[1]).await;
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(
            events(&said).iter().map(|l| l.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
    }

    #[tokio::test]
    async fn a_follower_gets_each_event_after_its_number_and_the_end() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[1]).await;

        let mut follower = Client::connect(&here.dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: name.clone(),
                after: 1,
            })
            .await
            .unwrap();
        match follower.next().await.unwrap() {
            Some(Say::Event { seq: 2, line }) => assert!(line.contains(HIDDEN)),
            other => panic!("{other:?}"),
        }
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut follower).await;
        assert_eq!(events(&said).iter().map(|l| l.0).collect::<Vec<_>>(), [3]);
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );

        // A session that the daemon does not own.
        let mut follower = Client::connect(&here.dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: "riff-issue-9-00000000".into(),
                after: 0,
            })
            .await
            .unwrap();
        assert!(matches!(
            follower.next().await.unwrap(),
            Some(Say::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn a_turn_that_ends_with_an_error_fails() {
        let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, error).await;
        relay.exit(2, 0).await;
        let (_, end) = to_the_end(&mut client).await;
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: true
            }))
        );
    }

    /// An event of `claude`: one background task runs.
    const ONE_TASK: &str = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]}"#;

    /// After the result, no event comes for the quiet limit: the daemon
    /// stops the container, and says why (01M4ZSZX77AJDGWNQMGBPV2QXG).
    #[tokio::test]
    async fn the_daemon_stops_a_session_with_no_event_for_the_quiet_limit_after_the_result() {
        let quiet = Duration::from_millis(500);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        // No stop while the input is open: a turn can be quiet for long.
        relay.event(1, TURN[0]).await;
        tokio::time::sleep(quiet * 2).await;
        assert_eq!(here.fake.calls(), [format!("start {name}")]);

        relay.event(2, TURN[2]).await;
        let closed = tokio::time::Instant::now();
        relay.event(3, ONE_TASK).await;
        let (said, end) = to_the_end(&mut client).await;
        assert!(closed.elapsed() >= quiet);
        assert!(
            said.contains(&Say::Line {
                text: text::daemon_quiet(&name, quiet)
            }),
            "{said:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
        assert_eq!(
            here.fake.calls(),
            [format!("start {name}"), format!("stop {name}")]
        );
        assert!(!record_path(&here.dir, &name).exists());
    }

    /// Each event after the result moves the end of the quiet: a new
    /// turn after the end of a background task is no quiet.
    #[tokio::test]
    async fn each_event_after_the_result_moves_the_end_of_the_quiet() {
        let quiet = Duration::from_millis(1000);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, ONE_TASK).await;
        relay.event(3, TURN[2]).await;
        // Two times the quiet limit, with an event each fifth of it.
        let mut seq = 3;
        for _ in 0..10 {
            tokio::time::sleep(quiet / 5).await;
            seq += 1;
            relay.event(seq, TURN[0]).await;
        }
        relay.exit(seq + 1, 0).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(events(&said).len(), 13);
        assert!(
            !said.iter().any(|s| matches!(s, Say::Line { .. })),
            "{said:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
    }

    /// An event of `claude`: each background task ended.
    const NO_TASK: &str = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#;

    /// Each line of `said` up to the end of the session, as
    /// [`to_the_end`].
    async fn to_the_end_of(said: &mut mpsc::UnboundedReceiver<Say>) -> (Vec<Say>, Option<Say>) {
        let mut lines = Vec::new();
        while let Some(say) = said.recv().await {
            match say {
                Say::Ended(_) | Say::Failed { .. } => return (lines, Some(say)),
                say => lines.push(say),
            }
        }
        (lines, None)
    }

    /// The grace of a session of a wave counts only while no background
    /// task runs: a task that runs holds the container, past the grace,
    /// until the new turn that its end starts (01M5P26VPJA73WCPVZY6CPG9HD, 01M5R0QFM265CCKTADK4T5XREQ).
    #[tokio::test]
    async fn a_wave_session_with_a_background_task_stays_up_past_the_grace_for_its_new_turn() {
        let grace = Duration::from_millis(400);
        let here = Here::with_grace(
            Fake::default().with_image(IMAGE),
            Duration::from_secs(20),
            grace,
        )
        .await;
        let (name, task, mut said) = here
            .launched(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[2]).await;
        relay.event(3, ONE_TASK).await;

        // Three times the grace with no event, and the task runs on.
        tokio::time::sleep(grace * 3).await;
        assert_eq!(here.fake.calls(), [format!("start {name}")]);

        // The task ends. The new turn pushes: its events come at once.
        relay.event(4, NO_TASK).await;
        relay.event(5, TURN[0]).await;
        relay.event(6, TURN[2]).await;
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        relay.exit(7, 0).await;
        let (lines, end) = to_the_end_of(&mut said).await;
        assert!(
            !lines.iter().any(|s| matches!(s, Say::Line { .. })),
            "{lines:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        task.await.unwrap();
    }

    /// A background task that never ends holds a session of a wave until
    /// the quiet limit.
    #[tokio::test]
    async fn a_wave_session_with_a_background_task_that_never_ends_stops_at_the_quiet_limit() {
        let (quiet, grace) = (Duration::from_millis(1500), Duration::from_millis(200));
        let here = Here::with_grace(Fake::default().with_image(IMAGE), quiet, grace).await;
        let (name, task, mut said) = here
            .launched(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[2]).await;
        let closed = tokio::time::Instant::now();
        relay.event(3, ONE_TASK).await;

        let (lines, end) = to_the_end_of(&mut said).await;
        assert!(closed.elapsed() >= quiet, "{:?}", closed.elapsed());
        assert!(
            lines.contains(&Say::Line {
                text: text::daemon_quiet(&name, quiet)
            }),
            "{lines:?}"
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
        assert_eq!(
            here.fake.calls(),
            [format!("start {name}"), format!("stop {name}")]
        );
        task.await.unwrap();
    }

    /// The end of the last background task gives the grace back: the
    /// session of a wave that stays quiet then stops after the grace, not
    /// after the quiet limit.
    #[tokio::test]
    async fn a_wave_session_has_the_grace_again_when_each_background_task_ended() {
        let (quiet, grace) = (Duration::from_secs(20), Duration::from_millis(300));
        let here = Here::with_grace(Fake::default().with_image(IMAGE), quiet, grace).await;
        let (name, task, mut said) = here
            .launched(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[2]).await;
        relay.event(3, ONE_TASK).await;
        // The watch has counted the quiet limit past the grace by now.
        tokio::time::sleep(grace * 2).await;
        let ended = tokio::time::Instant::now();
        relay.event(4, NO_TASK).await;

        let (lines, end) = to_the_end_of(&mut said).await;
        assert!(ended.elapsed() >= grace, "{:?}", ended.elapsed());
        assert!(ended.elapsed() < quiet, "{:?}", ended.elapsed());
        assert!(
            lines.contains(&Say::Line {
                text: text::daemon_quiet(&name, grace)
            }),
            "{lines:?}"
        );
        assert!(matches!(end, Some(Say::Ended(_))), "{end:?}");
        assert_eq!(
            here.fake.calls(),
            [format!("start {name}"), format!("stop {name}")]
        );
        task.await.unwrap();
    }

    /// A daemon that takes up a session of a wave knows its background
    /// tasks from the log: the last event says it.
    #[test]
    fn the_background_tasks_of_a_log_are_in_its_last_event_about_them() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join(EVENTS);
        assert_eq!(tasks_in(&log), 0);
        std::fs::write(&log, [TURN[0], ONE_TASK, TURN[2]].join("\n") + "\n").unwrap();
        assert_eq!(tasks_in(&log), 1);
        std::fs::write(
            &log,
            [TURN[0], ONE_TASK, NO_TASK, TURN[2]].join("\n") + "\n",
        )
        .unwrap();
        assert_eq!(tasks_in(&log), 0);
    }

    #[tokio::test]
    async fn a_session_with_no_image_on_the_machine_does_not_start() {
        let here = Here::new(Fake::default()).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        match to_the_end(&mut client).await {
            (_, Some(Say::Failed { error })) => assert!(error.contains("no image"), "{error}"),
            other => panic!("{other:?}"),
        }
        // The daemon owns no session, and the socket folder is gone.
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
        assert!(!session_dir(&here.dir, &name).exists());
        assert!(!record_path(&here.dir, &name).exists());
    }

    #[tokio::test]
    async fn each_session_gets_its_own_socket_and_no_other() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut one, a) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        let (mut two, b) = here
            .started(here.start("riff-issue-2-bbbbbbbb", "issue-2"))
            .await;

        for (name, other, item) in [(&a, &b, "issue-1"), (&b, &a, "issue-2")] {
            let spec = here.fake.spec(name).unwrap();
            let own = session_dir(&here.dir, name);
            // Its own socket folder, at /run/riff, with write.
            assert!(spec.binds.contains(&Bind {
                source: Source::Path(own.clone()),
                target: IN_SESSION.into(),
                write: true,
            }));
            assert!(
                spec.env
                    .contains(&Env::Value(SOCKET_VAR.into(), "/run/riff/riff.sock".into()))
            );
            // No bind shows the folder of the daemon, other than its own
            // socket folder.
            for bind in &spec.binds {
                if let Source::Path(source) = &bind.source
                    && source != &own
                {
                    assert!(!source.starts_with(&here.dir) && !here.dir.starts_with(source));
                }
                if let Source::Path(source) = &bind.source {
                    assert!(!source.starts_with(session_dir(&here.dir, other)));
                }
            }
            // Its socket tells the daemon which session asks, also to a
            // session of the protocol before.
            let who = Say::Session {
                session: format!("{name}-session"),
                item: item.into(),
                name: name.clone(),
            };
            assert_eq!(who_in(&own, PROTOCOL).await, who);
            // A work session is no lead, and knows its user.
            assert!(!lead_in(&own).await);
            assert!(
                spec.env
                    .iter()
                    .any(|e| matches!(e, Env::Value(n, _) if n == "RIFF_USER"))
            );
            assert_eq!(who_in(&own, PROTOCOL - 1).await, who);
            assert!(matches!(
                who_in(&own, PROTOCOL + 2).await,
                Say::Failed { .. }
            ));
        }
        let mode = |p: &Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777
        };
        assert_eq!(mode(&here.dir), 0o700);
        assert_eq!(mode(&here.dir.join(LIVE)), 0o700);
        assert_eq!(mode(&session_dir(&here.dir, &a)), 0o700);
        assert_eq!(mode(&session_dir(&here.dir, &a).join(SOCKET)), 0o600);
        let names: Vec<String> = status(&here.dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, [a.clone(), b.clone()]);

        // A stop: the runtime stops the container, and its end comes
        // with no exit of the relay.
        for client in [&mut one, &mut two] {
            client.stop().await.unwrap();
            let (_, end) = to_the_end(client).await;
            assert_eq!(
                end,
                Some(Say::Ended(Ended {
                    code: None,
                    failed: true
                }))
            );
        }
        assert_eq!(
            here.fake.calls(),
            [
                format!("start {a}"),
                format!("start {b}"),
                format!("stop {a}"),
                format!("stop {b}")
            ]
        );
    }

    #[tokio::test]
    async fn the_daemon_starts_no_session_whose_bind_shows_its_folder() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let data = here.root.path().to_owned();
        let shows = [
            data.clone(),
            here.dir.clone(),
            here.dir.join("sessions"),
            session_dir(&here.dir, "riff-issue-1-aaaaaaaa"),
        ];
        for source in shows {
            let mut start = here.start("riff-issue-2-bbbbbbbb", "issue-2");
            start.spec.binds.push(Bind {
                source: Source::Path(source.clone()),
                target: "/data".into(),
                write: false,
            });
            let mut client = Client::connect(&here.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            match client.next().await.unwrap() {
                Some(Say::Failed { error }) => {
                    assert!(error.contains("the folder of the riff daemon"), "{error}");
                }
                other => panic!("{source:?}: {other:?}"),
            }
        }
        // A bind at the place of the socket or of the relay in the
        // session.
        for target in ["/run/riff", RELAY_BIN] {
            let mut start = here.start("riff-issue-2-bbbbbbbb", "issue-2");
            start.spec.binds[0].target = target.into();
            let mut client = Client::connect(&here.dir).await.unwrap();
            client.ask(Ask::Start(Box::new(start))).await.unwrap();
            assert!(matches!(
                client.next().await.unwrap(),
                Some(Say::Failed { .. })
            ));
        }
        assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
    }

    /// 01M510NW8YEVMQKD6RWJPW38N3: the daemon starts no container with a
    /// riff that is not static, and names `riff update`. After the
    /// update, the next start works with no restart of the daemon.
    #[tokio::test]
    async fn the_daemon_starts_no_container_with_a_riff_that_is_not_static() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let riff = static_riff(&here.dir);
        crate::release::fake_riff(&riff, Some("/lib64/ld-linux-x86-64.so.2"));
        let mut client = Client::connect(&here.dir).await.unwrap();
        let start = here.start("riff-issue-1-aaaaaaaa", "issue-1");
        client.ask(Ask::Start(Box::new(start))).await.unwrap();
        match client.next().await.unwrap() {
            Some(Say::Failed { error }) => {
                assert!(
                    error.starts_with(&format!(
                        "The riff daemon starts no session with {}: it needs the shared \
                         libraries of /lib64/ld-linux-x86-64.so.2: it is not static.",
                        riff.display()
                    )),
                    "{error}"
                );
                assert!(error.contains("Run riff update"), "{error}");
            }
            other => panic!("{other:?}"),
        }
        assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
        assert!(here.daemon.status().await.sessions.is_empty());

        // riff update puts the static riff at the same path.
        crate::release::fake_riff(&riff, None);
        let (_client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        assert_eq!(name, "riff-issue-1-aaaaaaaa");

        // A daemon binds its own binary: the test binary, which is not
        // static with glibc.
        let root = tempfile::tempdir().unwrap();
        riff_client::secrets::mock_store();
        let own = Daemon::new(Fake::default(), dir_in(root.path())).unwrap();
        assert_eq!(own.relay_bin, own_binary().unwrap());
        if cfg!(target_env = "gnu") && !crate::release::takes_dynamic() {
            let error = own.check_relay().unwrap_err().to_string();
            assert!(error.contains("Run riff update"), "{error}");
        }
    }

    #[tokio::test]
    async fn a_stop_of_the_daemon_stops_no_session() {
        let mut here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;
        here.stop().await;
        // No daemon answers, and the daemon stopped no container. A
        // connect alone can still work: a child that another test forks
        // holds the listener until its exec.
        assert!(status(&here.dir).await.is_err());
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
        assert_eq!(here.fake.exit(&name), None);
        // The record stays for the next daemon.
        assert!(record_path(&here.dir, &name).exists());
    }

    /// A session of a wave has the grace of 60 s after the result while no
    /// background task runs, and another session has the quiet limit
    /// (01M5P26VPJA73WCPVZY6CPG9HD, 01M5R0QFM265CCKTADK4T5XREQ).
    #[tokio::test]
    async fn a_session_of_a_wave_has_the_grace_in_place_of_the_quiet_limit() {
        let root = tempfile::tempdir().unwrap();
        let daemon = Daemon::new(Fake::default(), dir_in(root.path())).unwrap();
        let name = "riff-issue-1-aaaaaaaa";
        let mut record = Record {
            live: Live {
                name: name.into(),
                session: format!("{name}-session"),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Phase::default(),
                last: String::new(),
                lead: false,
            },
            folder: root.path().to_owned(),
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: Vec::new(),
            closed: true,
            failed: false,
            job: None,
            terminal: false,
            takeover: false,
            interrupt: false,
        };
        assert_eq!(daemon.limit_of(&record, 0), QUIET_LIMIT);
        record.job = Some(Job {
            wave: Wave {
                repo: "o/r".into(),
                main: root.path().to_owned(),
                server: record.server.clone(),
                host: "h".into(),
            },
            work: crate::wave::Work::Build { issue: 1 },
        });
        assert_eq!(daemon.limit_of(&record, 0), EXIT_GRACE);
        // A background task that runs holds the session: the quiet limit.
        assert_eq!(daemon.limit_of(&record, 1), QUIET_LIMIT);
        assert_eq!(EXIT_GRACE, Duration::from_secs(60));
    }

    /// When podman cannot say whether the container of a record runs, a
    /// new daemon takes up the session (01M4ZHT01HH88KPA8628M7VBEY).
    #[tokio::test]
    async fn a_new_daemon_takes_up_a_session_when_podman_cannot_say() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let name = "riff-issue-1-aaaaaaaa";
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        private_dir(&dir.join(LIVE)).unwrap();
        let record = Record {
            live: Live {
                name: name.into(),
                session: format!("{name}-session"),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Phase::default(),
                last: String::new(),
                lead: false,
            },
            folder,
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![user_line("Do it.")],
            closed: false,
            failed: false,
            job: None,
            terminal: false,
            takeover: false,
            interrupt: false,
        };
        std::fs::write(
            record_path(&dir, name),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        private_dir(&session_dir(&dir, name)).unwrap();

        let fake = Fake::default().with_image(IMAGE).with_no_answer();
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake, dir.clone()).unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let names: Vec<String> = status(&dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, [name]);
        assert!(record_path(&dir, name).exists());
        let (_relay, had) = Relay::connect(&session_dir(&dir, name), 1, false).await;
        assert_eq!(had, 0);
    }

    /// The lines of the chat that `server` got.
    fn chats(server: &Server) -> Vec<String> {
        server
            .said()
            .into_iter()
            .filter(|s| s.starts_with("chat "))
            .collect()
    }

    /// 01M5SMQBTA3HQXPKKK01ZGFB4H: the sessions that an old daemon started
    /// have their network process in the cgroup of its unit. A new daemon
    /// takes up a session whose container has no network: it stops a
    /// worker and frees its item, and tells the person of a lead one time.
    #[tokio::test]
    async fn a_new_daemon_stops_a_worker_with_no_network_and_tells_the_person_of_a_lead() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let mut here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let (_lost, worker) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let (_kept, kept) = here
            .started(here.start("riff-issue-13-5e6f7a8b", "issue-13"))
            .await;
        let mut leads = Vec::new();
        for name in ["riff-lead-como-technologies-riff", "riff-lead-o-r"] {
            let mut start = here.start(name, "lead");
            start.terminal = true;
            let (mut client, name) = here.started(start).await;
            assert_eq!(client.next().await.unwrap(), Some(Say::Done));
            leads.push(name);
        }
        let [lost, good] = [leads[0].clone(), leads[1].clone()];

        // The restart of the daemon ended the network of two containers.
        here.fake
            .clone()
            .with_no_network(&worker)
            .with_no_network(&lost);
        here.restart().await;
        here.daemon.recover().await;

        // The worker: its item is free, and its container is stopped.
        until(|| server.said().contains(&"free issue-12".to_owned())).await;
        until(|| here.fake.calls().contains(&format!("stop {worker}"))).await;
        // The lead: one line in riff chat, and one in the status.
        let line = text::lead_lost_network("como-technologies/riff", &lost);
        until(|| chats(&server) == [format!("chat {lost}: {line}")]).await;
        let shown = status(&here.dir).await.unwrap();
        assert_eq!(
            shown.lost,
            [LostShown {
                repo: "como-technologies/riff".into(),
                name: lost.clone()
            }]
        );
        assert!(text::daemon_status(&shown, now_ms()).contains(&format!("{line}\n")));
        // The others have their network: no stop, no free, no line. A lead
        // is never stopped.
        let calls = here.fake.calls();
        for name in [&kept, &good, &lost] {
            assert!(!calls.contains(&format!("stop {name}")), "{calls:?}");
        }
        assert!(!server.said().contains(&"free issue-13".to_owned()));
        assert_eq!(chats(&server).len(), 1);
    }

    /// 01M5SMQBTA3HQXPKKK01ZGFB4H: when podman cannot say whether a
    /// container reaches the network, the session counts as connected.
    #[tokio::test]
    async fn a_new_daemon_leaves_a_session_when_podman_cannot_say_if_it_has_a_network() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let mut here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let (_worker, worker) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (mut client, _) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));

        here.fake.clone().with_no_answer();
        here.restart().await;
        here.daemon.recover().await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(!here.fake.calls().contains(&format!("stop {worker}")));
        assert!(server.said().iter().all(|s| s.starts_with("resume ")));
        assert!(status(&here.dir).await.unwrap().lost.is_empty());
    }

    /// 01M5R56GN3DC09GAPAF69GENEZ (#1404): the live record of a wave job
    /// that an older build wrote has a wave of four fields, with no default
    /// branch. A new daemon reads it and takes the session up: its job
    /// goes on with the keep-alive of its claim. It does not remove the
    /// record as one that does not parse.
    #[tokio::test]
    async fn a_new_daemon_takes_up_a_session_with_a_wave_job_of_an_older_build() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let name = "riff-issue-1-aaaaaaaa";
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        private_dir(&dir.join(LIVE)).unwrap();
        let record = Record {
            live: Live {
                name: name.into(),
                session: format!("{name}-session"),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Phase::default(),
                last: String::new(),
                lead: false,
            },
            folder,
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![user_line("Do it.")],
            closed: false,
            failed: false,
            job: Some(Job {
                wave: Wave {
                    repo: "o/r".into(),
                    main: root.path().join("r"),
                    server: "http://127.0.0.1:1".into(),
                    host: "thelio".into(),
                },
                work: crate::wave::Work::Build { issue: 1 },
            }),
            terminal: false,
            takeover: false,
            interrupt: false,
        };
        // The wave of an older build had these four fields, and no more.
        let old = serde_json::to_value(&record).unwrap();
        let mut fields: Vec<&str> = old["job"]["wave"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["host", "main", "repo", "server"]);
        std::fs::write(record_path(&dir, name), old.to_string()).unwrap();
        private_dir(&session_dir(&dir, name)).unwrap();

        let fake = Fake::default().with_image(IMAGE).with_no_answer();
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let daemon = Arc::new(Daemon::new(fake, dir.clone()).unwrap().with_world(world));
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let names: Vec<String> = status(&dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, [name]);
        assert!(record_path(&dir, name).exists());
        assert_eq!(server.said(), [format!("resume {name}-session")]);
    }

    #[tokio::test]
    async fn a_new_daemon_takes_up_a_session_that_runs_and_drops_one_that_ended() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_image(IMAGE);
        let run = |start: &Start| {
            let mut spec = start.spec.clone();
            spec.env.clear();
            spec
        };
        // Two records of an old daemon: one container runs, one ended.
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(EVENTS), format!("{}\n{}\n", TURN[0], TURN[1])).unwrap();
        private_dir(&dir.join(LIVE)).unwrap();
        for (name, item) in [
            ("riff-issue-1-aaaaaaaa", "issue-1"),
            ("riff-issue-2-bbbbbbbb", "issue-2"),
        ] {
            let start = Start {
                session: format!("{name}-session"),
                item: item.into(),
                folder: folder.clone(),
                spec: spec(name, &folder),
                start: "Do it.".into(),
                values: vec![],
                secrets: vec![],
                server: "http://127.0.0.1:1".into(),
                me: me(name),
                terminal: false,
            };
            let record = Record {
                live: Live {
                    name: name.into(),
                    session: start.session.clone(),
                    item: item.into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                    phase: Phase::default(),
                    last: String::new(),
                    lead: false,
                },
                folder: folder.clone(),
                server: start.server.clone(),
                me: start.me.clone(),
                inputs: vec![user_line("Do it.")],
                closed: false,
                failed: false,
                job: None,
                terminal: false,
                takeover: false,
                interrupt: false,
            };
            std::fs::write(
                record_path(&dir, name),
                serde_json::to_string(&record).unwrap(),
            )
            .unwrap();
            private_dir(&session_dir(&dir, name)).unwrap();
            if name.contains("issue-1") {
                let _running = fake.start(&run(&start), &[]).await.unwrap();
                std::mem::forget(_running);
            }
        }

        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake.clone(), dir.clone()).unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let names: Vec<String> = status(&dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["riff-issue-1-aaaaaaaa"]);
        assert!(!record_path(&dir, "riff-issue-2-bbbbbbbb").exists());
        assert!(!session_dir(&dir, "riff-issue-2-bbbbbbbb").exists());

        // The relay connects again: the daemon has two events, and gives
        // the input that the relay did not take.
        let socket = session_dir(&dir, "riff-issue-1-aaaaaaaa");
        let (mut relay, had) = Relay::connect(&socket, 0, false).await;
        assert_eq!(had, 2);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        let mut follower = Client::connect(&dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: "riff-issue-1-aaaaaaaa".into(),
                after: 0,
            })
            .await
            .unwrap();
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let (said, end) = to_the_end(&mut follower).await;
        assert_eq!(
            events(&said).iter().map(|l| l.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
        assert_eq!(
            std::fs::read_to_string(folder.join(EVENTS))
                .unwrap()
                .lines()
                .count(),
            3
        );
        assert!(!record_path(&dir, "riff-issue-1-aaaaaaaa").exists());
    }

    #[tokio::test]
    async fn a_taken_up_session_whose_relay_and_container_end_ends() {
        assert_eq!(
            take_up_and_end(true).await,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_taken_up_session_whose_relay_never_comes_ends_with_its_container() {
        assert_eq!(
            take_up_and_end(false).await,
            Some(Say::Ended(Ended {
                code: None,
                failed: true
            }))
        );
    }

    /// A new daemon takes up a session whose container runs. With
    /// `relay`, its relay connects. Then the container ends with no exit
    /// of the relay. It gives the last line that a follower got.
    async fn take_up_and_end(relay: bool) -> Option<Say> {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_image(IMAGE);
        let folder = root.path().join("sessions/issue-1");
        std::fs::create_dir_all(&folder).unwrap();
        let name = "riff-issue-1-aaaaaaaa";
        let record = Record {
            live: Live {
                name: name.into(),
                session: "s".into(),
                item: "issue-1".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Phase::default(),
                last: String::new(),
                lead: false,
            },
            folder: folder.clone(),
            server: "http://127.0.0.1:1".into(),
            me: me(name),
            inputs: vec![],
            closed: false,
            failed: false,
            job: None,
            terminal: false,
            takeover: false,
            interrupt: false,
        };
        private_dir(&dir.join(LIVE)).unwrap();
        std::fs::write(
            record_path(&dir, name),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        let mut spec = spec(name, &folder);
        spec.env.clear();
        let running = fake.start(&spec, &[]).await.unwrap();

        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake.clone(), dir.clone()).unwrap());
        daemon.recover().await;
        tokio::spawn(daemon.serve(listener, std::future::pending()));
        let mut follower = Client::connect(&dir).await.unwrap();
        follower
            .ask(Ask::Follow {
                name: name.into(),
                after: 0,
            })
            .await
            .unwrap();
        let relay = match relay {
            true => Some(Relay::connect(&session_dir(&dir, name), 0, false).await),
            false => None,
        };
        fake.stop(name).await.unwrap();
        drop((relay, running));
        to_the_end(&mut follower).await.1
    }

    #[tokio::test]
    async fn the_daemon_pulls_only_an_image_that_the_machine_does_not_have() {
        let (local, remote, missing) = ("img-local", "img-remote", "img-missing");
        let here = Here::new(Fake::default().with_image(local).with_registry(remote)).await;
        let mut shown = Vec::new();
        image(&here.dir, local, |l| shown.push(l.to_owned()))
            .await
            .unwrap();
        assert!(shown.is_empty());
        image(&here.dir, remote, |l| shown.push(l.to_owned()))
            .await
            .unwrap();
        assert_eq!(shown, [text::run_pulling(remote)]);
        let error = image(&here.dir, missing, |_| {})
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(&text::run_no_image(missing)), "{error}");
        assert_eq!(
            here.fake.calls(),
            [format!("pull {remote}"), format!("pull {missing}")]
        );
    }

    #[tokio::test]
    async fn the_daemon_refuses_another_protocol_and_a_second_daemon() {
        let here = Here::new(Fake::default()).await;
        let conn = connect_in(&here.dir, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        write
            .write_all(b"{\"protocol\":99,\"ask\":{\"ask\":\"status\"}}\n")
            .await
            .unwrap();
        let line = BufReader::new(read)
            .lines()
            .next_line()
            .await
            .unwrap()
            .unwrap();
        let said: Say = serde_json::from_str(&line).unwrap();
        assert_eq!(
            said,
            Say::Failed {
                error: text::daemon_protocol(99, PROTOCOL)
            }
        );
        // A line that is not a hello: the error shows no part of it.
        let mut client = Client::connect(&here.dir).await.unwrap();
        client
            .line(&serde_json::json!({"gho_secret1": 1}))
            .await
            .unwrap();
        match client.next().await.unwrap() {
            Some(Say::Failed { error }) => assert!(!error.contains("gho_secret1"), "{error}"),
            other => panic!("{other:?}"),
        }

        let error = listen(&here.dir).await.unwrap_err().to_string();
        assert!(error.contains("A riff daemon runs already"), "{error}");
    }

    #[tokio::test]
    async fn a_new_daemon_takes_the_place_of_a_socket_that_no_daemon_answers() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        drop(listen(&dir).await.unwrap());
        // The socket file stays, and no daemon answers on it. A connect
        // alone can still work: a child that another test forks holds
        // the listener until its exec.
        assert!(socket(&dir).exists());
        assert!(status(&dir).await.is_err());
        listen(&dir).await.unwrap();
    }

    #[test]
    fn a_start_and_a_say_go_through_json() {
        let root = Path::new("/r");
        let start = Start {
            session: "s".into(),
            item: "issue-12".into(),
            folder: root.join("f"),
            spec: spec("n", &root.join("clone")),
            start: "Do it.".into(),
            values: vec![("GH_TOKEN".into(), "v".into())],
            secrets: vec!["v".into()],
            server: "http://127.0.0.1:1".into(),
            me: me("n"),
            terminal: false,
        };
        let line = serde_json::to_string(&Hello {
            protocol: PROTOCOL,
            ask: Ask::Start(Box::new(start)),
        })
        .unwrap();
        let Ok(Ask::Start(back)) = hello(&line) else {
            panic!("{line}")
        };
        assert_eq!(back.spec, spec("n", &root.join("clone")));
        assert_eq!(back.values, [("GH_TOKEN".to_owned(), "v".to_owned())]);
        assert_eq!(back.me, me("n"));
        let ended = Say::Ended(Ended {
            code: None,
            failed: true,
        });
        let line = serde_json::to_string(&ended).unwrap();
        assert_eq!(line, r#"{"say":"ended","code":null,"failed":true}"#);
        assert_eq!(serde_json::from_str::<Say>(&line).unwrap(), ended);
        assert_eq!(
            serde_json::to_string(&Then::Stop).unwrap(),
            r#"{"then":"stop"}"#
        );
        // The lines of the relay protocol.
        assert_eq!(
            serde_json::to_string(&Say::Ack { seq: 3 }).unwrap(),
            r#"{"say":"ack","seq":3}"#
        );
        assert_eq!(
            serde_json::to_string(&SessionAsk::Relay {
                protocol: 2,
                inputs: 1,
                closed: false,
                held: false,
            })
            .unwrap(),
            r#"{"ask":"relay","protocol":2,"inputs":1,"closed":false,"held":false}"#
        );
        // The hello of a relay of protocol 3 has no held.
        assert_eq!(
            serde_json::from_str::<SessionAsk>(
                r#"{"ask":"relay","protocol":3,"inputs":1,"closed":true}"#
            )
            .unwrap(),
            SessionAsk::Relay {
                protocol: 3,
                inputs: 1,
                closed: true,
                held: false,
            }
        );
        assert_eq!(
            serde_json::to_string(&Say::Hold { interrupt: true }).unwrap(),
            r#"{"say":"hold","interrupt":true}"#
        );
        assert_eq!(
            serde_json::to_string(&Say::Hold { interrupt: false }).unwrap(),
            r#"{"say":"hold","interrupt":false}"#
        );
        // The takeover from before at_end has no at_end: it interrupts.
        let line = format!(r#"{{"protocol":{PROTOCOL},"ask":{{"ask":"takeover","name":"n"}}}}"#);
        let Ok(Ask::Takeover { at_end, .. }) = hello(&line) else {
            panic!("{line}")
        };
        assert!(!at_end);
        assert_eq!(
            serde_json::to_string(&Up::Event {
                seq: 1,
                line: "{}".into()
            })
            .unwrap(),
            r#"{"relay":"event","seq":1,"line":"{}"}"#
        );
    }

    /// The riff-server of the lead, as the test plays it: it notes each
    /// keep-alive and each end, and gives a wake at once. It gives each
    /// wake of [`Server::wake_all`] to the watch of each worker, and
    /// notes each refusal. No wave runs.
    /// The forge gives the fate of the folders of each issue of `fates`,
    /// and [`Fate::Stays`](sweep::Fate::Stays) for each other issue. With
    /// `gh`, the fate comes from that `gh` ([`sweep::fate_on`]). With
    /// `gate`, the steps after the end of a job wait for a permit of it.
    #[derive(Default)]
    pub(super) struct Server {
        said: Mutex<Vec<String>>,
        fates: BTreeMap<u64, sweep::Fate>,
        comments: Mutex<Vec<(String, u64, u64, String)>>,
        /// The branches `riff-kept/*` of the forge ([`World::delete_old_kept`]).
        kept: Mutex<Vec<sweep::KeptBranch>>,
        gh: Option<crate::pr::Gh>,
        gate: Option<Arc<tokio::sync::Semaphore>>,
        watches: Mutex<Vec<mpsc::UnboundedSender<Wake>>>,
        /// The marks and the `gh` of [`World::meter`].
        meter: Option<(PathBuf, crate::pr::Gh)>,
        /// The leave marks of [`World::marks`].
        marks: Option<PathBuf>,
        /// Why [`World::lead_again`] fails, when it does.
        again_fault: Mutex<Option<String>>,
    }

    impl Server {
        /// The forge with the fate of each issue of `fates`.
        pub(super) fn with_fates(fates: &[(u64, sweep::Fate)]) -> Server {
            Server {
                fates: fates.iter().cloned().collect(),
                ..Server::default()
            }
        }

        /// The same forge, with the branches `riff-kept/*` `kept`.
        pub(super) fn with_kept(self, kept: Vec<sweep::KeptBranch>) -> Server {
            Server {
                kept: Mutex::new(kept),
                ..self
            }
        }

        /// The names of the branches `riff-kept/*` that are left.
        pub(super) fn kept_left(&self) -> Vec<String> {
            let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            kept.iter().map(|b| b.name.clone()).collect()
        }

        /// The forge of `gh`.
        pub(super) fn with_gh(gh: crate::pr::Gh) -> Server {
            Server {
                gh: Some(gh),
                ..Server::default()
            }
        }

        /// The same forge, with the marks in `dir` and `gh` for the tokens
        /// of each container session.
        pub(super) fn with_meter(self, dir: &Path, gh: crate::pr::Gh) -> Server {
            Server {
                meter: Some((dir.to_owned(), gh)),
                ..self
            }
        }

        /// The same forge, with the leave marks in `dir`.
        pub(super) fn with_marks(self, dir: &Path) -> Server {
            Server {
                marks: Some(dir.to_owned()),
                ..self
            }
        }

        /// The same forge: the steps after the end of a job wait for a
        /// permit of `gate`.
        pub(super) fn with_gate(self, gate: Arc<tokio::sync::Semaphore>) -> Server {
            Server {
                gate: Some(gate),
                ..self
            }
        }

        /// Each comment `Merged in`: the repository, the issue, the pull
        /// request and the commit.
        pub(super) fn comments(&self) -> Vec<(String, u64, u64, String)> {
            self.comments
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        pub(super) fn said(&self) -> Vec<String> {
            self.said
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        /// The number of the watches of the workers.
        pub(super) fn watches(&self) -> usize {
            self.watches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len()
        }

        /// Gives `wake` to each watch of a worker.
        pub(super) fn wake_all(&self, wake: &Wake) {
            for to in self
                .watches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
            {
                let _ = to.send(wake.clone());
            }
        }

        fn note(&self, line: String) {
            self.said
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line);
        }
    }

    impl World for Server {
        fn look<'a>(
            &'a self,
            _: &'a Wave,
        ) -> crate::wave::BoxFuture<'a, Result<crate::wave::Look>> {
            Box::pin(async { bail!("no wave in this test") })
        }

        fn pinned<'a>(
            &'a self,
            _: &'a Wave,
        ) -> crate::wave::BoxFuture<'a, Result<crate::wave::Pinned>> {
            Box::pin(async { bail!("no wave in this test") })
        }

        fn host<'a>(
            &'a self,
            _: &'a Wave,
        ) -> crate::wave::BoxFuture<'a, Result<(), crate::run::HostFault>> {
            Box::pin(async { Ok(()) })
        }

        fn begin<'a>(
            &'a self,
            _: &'a Wave,
            _: &'a crate::wave::Work,
            _: &'a crate::wave::Pinned,
        ) -> crate::wave::BoxFuture<'a, Result<Option<Start>>> {
            Box::pin(async { bail!("no wave in this test") })
        }

        fn resume(&self, record: &Record) {
            self.note(format!("resume {}", record.live.session));
        }

        fn finish<'a>(
            &'a self,
            record: &'a Record,
            _: Ended,
        ) -> crate::wave::BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move {
                self.note(format!("finish {}", record.live.item));
                if let Some(gate) = &self.gate {
                    gate.acquire().await?.forget();
                }
                self.note(format!("finished {}", record.live.item));
                Ok(Vec::new())
            })
        }

        fn meter(&self, _: &str) -> Option<crate::usage::Meter> {
            let (dir, gh) = self.meter.clone()?;
            Some(crate::usage::Meter { dir, gh })
        }

        fn marks(&self) -> Option<PathBuf> {
            self.marks.clone()
        }

        fn fate<'a>(
            &'a self,
            _: &'a str,
            issue: u64,
        ) -> crate::wave::BoxFuture<'a, Result<sweep::Fate>> {
            if let Some(gh) = self.gh.clone() {
                return Box::pin(async move {
                    tokio::task::spawn_blocking(move || sweep::fate_on(&gh, issue)).await?
                });
            }
            let fate = self
                .fates
                .get(&issue)
                .cloned()
                .unwrap_or(sweep::Fate::Stays);
            Box::pin(async move { Ok(fate) })
        }

        fn delete_old_kept<'a>(
            &'a self,
            _: &'a str,
        ) -> crate::wave::BoxFuture<'a, Result<Vec<String>>> {
            let now = sweep::date_seconds("2026-10-24T12:00:00Z").unwrap_or(0);
            let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            let old = sweep::old_kept(&kept, now);
            kept.retain(|b| !old.contains(&b.name));
            Box::pin(async move { Ok(old) })
        }

        fn merged_in<'a>(
            &'a self,
            repo: &'a str,
            issue: u64,
            pull: u64,
            commit: &'a str,
        ) -> crate::wave::BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                self.comments
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((repo.to_owned(), issue, pull, commit.to_owned()));
                Ok(())
            })
        }

        fn tell<'a>(&'a self, _: &'a Wave, _: &'a str) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        /// The `gh` of the forge, or a `gh` that is not there: no test
        /// reaches the real forge.
        fn gh(&self, _: &str) -> Arc<crate::pr::Gh> {
            let gh = self.gh.clone();
            Arc::new(gh.unwrap_or_else(|| crate::pr::Gh::at("/nonexistent/gh")))
        }

        fn claims<'a>(
            &'a self,
            _: &'a Wave,
        ) -> crate::wave::BoxFuture<'a, Result<std::collections::HashSet<String>>> {
            Box::pin(async { Ok(std::collections::HashSet::new()) })
        }

        fn post<'a>(
            &'a self,
            _: &'a Wave,
            _: Option<&'a str>,
            _: &'a str,
            _: riff_core::wire::Kind,
        ) -> crate::wave::BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn chat<'a>(&'a self, record: &'a Record, body: &'a str) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async move { self.note(format!("chat {}: {body}", record.live.name)) })
        }

        fn note_lead<'a>(
            &'a self,
            record: &'a Record,
            body: &'a str,
        ) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async move { self.note(format!("note {}: {body}", record.live.name)) })
        }

        fn leave<'a>(&'a self, record: &'a Record) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async move { self.note(format!("leave {}", record.live.session)) })
        }

        /// The start of the lead as the real world makes it ([`crate::lead::again`]):
        /// the spec of `claude --resume`, with a value for each variable of the host.
        fn lead_again<'a>(
            &'a self,
            _: &'a Wave,
            record: &'a Record,
        ) -> crate::wave::BoxFuture<'a, Result<Start>> {
            Box::pin(async move {
                self.note(format!("start again {}", record.live.name));
                if let Some(why) = self
                    .again_fault
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .as_ref()
                {
                    bail!("{why}");
                }
                let facts = crate::run::Facts {
                    session: record.live.session.clone(),
                    item: crate::lead::ITEM.into(),
                    image: IMAGE.into(),
                    home: "/home/mike".into(),
                    user: "mike".into(),
                    clone: record.folder.join("clone"),
                    objects: record.folder.join("objects"),
                    claude: record.folder.join("claude"),
                    sign_in: crate::run::ClaudeSignIn::Token,
                    gitconfig: None,
                    gh_config: None,
                    setup: Default::default(),
                };
                let repo = record.me.place().repo_text();
                let mut spec = crate::lead::spec_again(&facts, &repo, &text::lead_again(&repo));
                spec.name = record.live.name.clone();
                let values = spec
                    .env
                    .iter()
                    .filter_map(|e| match e {
                        Env::Host(name) => Some((name.clone(), "v".to_owned())),
                        Env::Value(..) => None,
                    })
                    .collect();
                Ok(Start {
                    session: record.live.session.clone(),
                    item: crate::lead::ITEM.into(),
                    folder: record.folder.clone(),
                    start: text::lead_again(&repo),
                    spec,
                    values,
                    secrets: Vec::new(),
                    server: record.server.clone(),
                    me: record.me.clone(),
                    terminal: true,
                })
            })
        }

        fn free<'a>(&'a self, record: &'a Record) -> crate::wave::BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                self.note(format!("free {}", record.live.item));
                Ok(())
            })
        }

        fn wake<'a>(&'a self, record: &'a Record) -> crate::wave::BoxFuture<'a, Result<String>> {
            Box::pin(async move { Ok(format!("a wake of {}", record.live.item)) })
        }

        fn wakes<'a>(
            &'a self,
            _: &'a Record,
            to: mpsc::UnboundedSender<Wake>,
        ) -> crate::wave::BoxFuture<'a, ()> {
            self.watches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(to);
            Box::pin(std::future::pending())
        }

        fn refuse<'a>(
            &'a self,
            record: &'a Record,
            wake: &'a Wake,
        ) -> crate::wave::BoxFuture<'a, ()> {
            Box::pin(async move {
                self.note(format!("refuse {} {}", record.live.session, wake.seq));
            })
        }
    }

    /// Waits until `done` is true, for at most 5 s.
    pub(super) async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..500 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("it did not happen in 5 s");
    }

    /// The phase of the session `name` in the status of the daemon.
    async fn phase_of(dir: &Path, name: &str) -> Phase {
        status(dir)
            .await
            .unwrap()
            .sessions
            .into_iter()
            .find(|s| s.name == name)
            .map(|s| s.phase)
            .unwrap()
    }

    /// Asks a takeover of `name` that interrupts the turn, and gives the
    /// client after the first line of the daemon.
    async fn take_over(dir: &Path, name: &str) -> (Client, Say) {
        take_over_with(dir, name, false).await
    }

    /// Asks a takeover of `name`, with `at_end` or not.
    async fn take_over_with(dir: &Path, name: &str, at_end: bool) -> (Client, Say) {
        let mut taker = Client::connect(dir).await.unwrap();
        taker
            .ask(Ask::Takeover {
                name: name.into(),
                at_end,
            })
            .await
            .unwrap();
        let first = taker.next().await.unwrap().unwrap();
        (taker, first)
    }

    /// The next line of `taker` that is not an event of the session.
    async fn next_of(taker: &mut Client) -> Option<Say> {
        loop {
            match taker.next().await.unwrap() {
                Some(Say::Event { .. } | Say::Stderr { .. }) => {}
                other => return other,
            }
        }
    }

    /// A worker with its relay, after its start text and its first event.
    async fn a_worker(here: &Here) -> (Client, String, Relay) {
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        relay.event(1, TURN[0]).await;
        (client, name, relay)
    }

    /// `riff relay watch` of the session `name`: the line of its next
    /// wake.
    fn wake_of(dir: &Path, name: &str) -> JoinHandle<String> {
        let socket = session_dir(dir, name);
        tokio::spawn(async move {
            let conn = connect_in(&socket, SOCKET).await.unwrap();
            let (read, mut write) = conn.into_split();
            let ask = serde_json::to_string(&SessionAsk::Watch { protocol: PROTOCOL }).unwrap();
            write
                .write_all(format!("{ask}\n").as_bytes())
                .await
                .unwrap();
            let line = BufReader::new(read)
                .lines()
                .next_line()
                .await
                .unwrap()
                .unwrap();
            drop(write);
            match serde_json::from_str::<Say>(&line).unwrap() {
                Say::Line { text } => text,
                other => panic!("{other:?}"),
            }
        })
    }

    /// A fake `gcloud` beside the folder `dir` of a daemon. It logs each
    /// call in `gcloud.log` beside it, then prints one line of the log,
    /// or fails as `gcloud` does when its sign-in ended with `ended`.
    fn fake_gcloud(dir: &Path, ended: bool) {
        use std::os::unix::fs::PermissionsExt;
        let body = if ended {
            "echo 'ERROR: (gcloud.logging.read) Reauthentication failed. Please run: gcloud auth login' >&2\nexit 1\n"
        } else {
            "echo '2026-10-22T10:00:00Z riff-00001'\n"
        };
        let path = dir.with_file_name("gcloud");
        let script = format!("#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/gcloud.log\"\n{body}");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// #1247: the lead in a container reads the Cloud Run log through
    /// its socket, with the `gcloud` of the host: each of the 4 log checks
    /// of `riff release check` goes to the fake `gcloud`. A worker on its
    /// own socket gets a refusal, and `gcloud` does not run for it.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_lead_reads_the_cloud_run_log_through_its_socket_and_a_worker_cannot() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        fake_gcloud(&here.dir, false);
        let mut lead = here.start("riff-lead-o-r", "lead");
        lead.terminal = true;
        let (_lead, lead_name) = here.started(lead).await;
        let (_worker, worker_name) = here
            .started(here.start("riff-issue-1-aaaaaaaa", "issue-1"))
            .await;

        let settings = crate::cloud::Settings::new("shared", "como-riff", "123", "europe-west1");
        let log = crate::reach::DaemonLog {
            socket: session_dir(&here.dir, &lead_name).join(SOCKET),
            settings: settings.clone(),
        };
        let served = crate::cycle::Served {
            server: Ok("3.3.0".into()),
            hosts: Ok(vec![]),
        };
        let version = crate::cycle::version("3.3.0").unwrap();
        let lines = crate::cycle::checks(version, "2026-10-22T09:00:00Z", &served, &log);
        let ran = std::fs::read_to_string(here.dir.with_file_name("gcloud.log")).unwrap();
        assert_eq!(ran.lines().count(), 4, "{ran}");
        assert!(
            ran.lines()
                .all(|l| l.starts_with("logging read") && l.contains("--project como-riff")),
            "{ran}"
        );
        // The fake printed one line of one revision: no check is unknown.
        for (name, verdict) in &lines {
            assert!(
                !matches!(verdict, crate::cycle::Verdict::Unknown(_)),
                "{name}: {verdict:?}"
            );
        }

        let worker = crate::reach::DaemonLog {
            socket: session_dir(&here.dir, &worker_name).join(SOCKET),
            settings,
        };
        let refused = crate::cycle::CloudLog::lines(&worker, "severity>=ERROR").unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_LOG_ONLY_LEAD);
        let ran = std::fs::read_to_string(here.dir.with_file_name("gcloud.log")).unwrap();
        assert_eq!(ran.lines().count(), 4, "{ran}");
    }

    /// #1247: when the sign-in of `gcloud` on the host ended, each check
    /// of the log says that a person on the host runs `gcloud auth
    /// login`. When the host has no `gcloud`, the check says to install
    /// it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_host_with_no_sign_in_or_no_gcloud_names_the_step_for_a_person() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut lead = here.start("riff-lead-o-r", "lead");
        lead.terminal = true;
        let (_lead, name) = here.started(lead).await;
        let settings = crate::cloud::Settings::new("shared", "como-riff", "123", "europe-west1");
        let log = crate::reach::DaemonLog {
            socket: session_dir(&here.dir, &name).join(SOCKET),
            settings,
        };
        let served = crate::cycle::Served {
            server: Ok("3.3.0".into()),
            hosts: Ok(vec![]),
        };
        let version = crate::cycle::version("3.3.0").unwrap();
        let unknown = |lines: Vec<(&str, crate::cycle::Verdict)>| -> Vec<String> {
            lines
                .into_iter()
                .filter_map(|(_, v)| match v {
                    crate::cycle::Verdict::Unknown(why) => Some(why),
                    _ => None,
                })
                .collect()
        };

        fake_gcloud(&here.dir, true);
        let ended = unknown(crate::cycle::checks(version, "t", &served, &log));
        assert_eq!(ended.len(), 4, "{ended:?}");
        for why in &ended {
            assert!(
                why.contains("A person on the host runs: gcloud auth login"),
                "{why}"
            );
        }

        std::fs::remove_file(here.dir.with_file_name("gcloud")).unwrap();
        let none = unknown(crate::cycle::checks(version, "t", &served, &log));
        assert_eq!(none.len(), 4, "{none:?}");
        for why in &none {
            assert!(why.contains("gcloud is not on PATH"), "{why}");
        }
    }

    /// 01M5NJ87YEHHGJBANM7CRYV56Z
    #[tokio::test]
    async fn a_lead_whose_sign_in_file_the_host_replaced_gets_one_line_in_riff_chat() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let home = here.root.path().join("home");
        let host = home.join(".claude");
        std::fs::create_dir_all(&host).unwrap();
        let file = host.join(".credentials.json");
        std::fs::write(&file, "old").unwrap();
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        start.spec.binds.push(crate::runtime::Bind {
            source: crate::runtime::Source::Path(file.clone()),
            target: crate::lead::credentials_of(&home),
            write: false,
        });
        let (mut client, name) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
        drop(client);

        // The bind holds the file of the host: no word.
        assert!(here.daemon.stale_leads(&home).await.is_empty());

        // claude on the host signs in again: it writes a new file.
        let new = host.join(".credentials.json.new");
        std::fs::write(&new, "new").unwrap();
        std::fs::rename(&new, &file).unwrap();
        let line = text::lead_sign_in_stale(&name);
        assert_eq!(
            here.daemon.stale_leads(&home).await,
            std::slice::from_ref(&line)
        );
        assert!(server.said().contains(&format!("chat {name}: {line}")));

        // The person heard of it once.
        assert!(here.daemon.stale_leads(&home).await.is_empty());
        let chats = server
            .said()
            .iter()
            .filter(|s| s.starts_with("chat "))
            .count();
        assert_eq!(chats, 1);
    }

    /// The lead of `como-technologies/riff` in a test.
    const LEAD: &str = "riff-lead-como-technologies-riff";

    /// Starts the lead [`LEAD`] on `here`, with the wave of its repository
    /// in the folder of the daemon. Its container started long ago. It
    /// gives its record.
    async fn started_lead(here: &Here) -> Record {
        let wave = Wave {
            repo: "como-technologies/riff".into(),
            main: here.root.path().to_owned(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(&here.dir, &BTreeMap::from([(wave.repo.clone(), wave)])).unwrap();
        let mut start = here.start(LEAD, "lead");
        start.terminal = true;
        let (mut client, _) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
        let owned = here.daemon.lock().get(LEAD).cloned().unwrap();
        owned.lock().record.live.since_ms = 0;
        owned.save(&owned.lock()).unwrap();
        let record = owned.lock().record.clone();
        drop(owned);
        record
    }

    /// The transcript of the lead of `record`: its last write was `secs`
    /// seconds ago.
    fn transcript_of(record: &Record, secs: u64) {
        let project = record.folder.join("claude/projects/-clone");
        std::fs::create_dir_all(&project).unwrap();
        let name = format!("{}.jsonl", record.live.session);
        let file = std::fs::File::create(project.join(name)).unwrap();
        let ago = std::time::SystemTime::now() - Duration::from_secs(secs);
        file.set_modified(ago).unwrap();
    }

    /// `riff update`: a new file of the riff is in the place of the old
    /// one. A container that bound the old file keeps it.
    fn update_riff(here: &Here) {
        let riff = here.dir.with_file_name("static-riff");
        let new = riff.with_extension("new");
        crate::release::fake_riff(&new, None);
        std::fs::rename(&new, &riff).unwrap();
    }

    /// The lines of `said` that begin with `kind`.
    fn said_of(server: &Server, kind: &str) -> Vec<String> {
        let kind = format!("{kind} ");
        let said = server.said();
        said.into_iter().filter(|s| s.starts_with(&kind)).collect()
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[tokio::test]
    async fn an_idle_lead_on_the_old_riff_starts_again_with_claude_resume_and_a_lead_in_a_turn_waits()
     {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let record = started_lead(&here).await;
        let terminal = format!("terminal {LEAD}");

        // The riff on disk is the riff of the lead: nothing to do.
        transcript_of(&record, 3600);
        assert!(here.daemon.leads_again().await.is_empty());

        // `riff update`. The lead is in a turn: its transcript got a write
        // 10 s ago. It waits.
        update_riff(&here);
        transcript_of(&record, 10);
        assert!(here.daemon.leads_again().await.is_empty());
        assert_eq!(here.fake.calls(), std::slice::from_ref(&terminal));

        // The turn ends. The lead is idle: the transcript is quiet.
        transcript_of(&record, 600);
        let line = text::lead_started_again(LEAD);
        assert_eq!(here.daemon.leads_again().await, std::slice::from_ref(&line));
        assert_eq!(
            here.fake.calls(),
            [terminal.clone(), format!("stop {LEAD}"), terminal]
        );

        // The new container: `claude --resume ID` of the same conversation
        // in the same clone, and the bind of the new riff.
        let spec = here.fake.spec(LEAD).unwrap();
        let session = record.live.session.as_str();
        assert_eq!(
            spec.command[..5],
            [RELAY_BIN, "relay", "--terminal", "--", "claude"]
        );
        let resume = spec.command.iter().position(|a| a == "--resume");
        assert_eq!(
            spec.command[resume.expect("--resume")..],
            [
                "--resume",
                session,
                "--dangerously-skip-permissions",
                &text::lead_again("como-technologies/riff")
            ]
        );
        assert!(!spec.command.iter().any(|a| a == "--session-id"));
        let riff = spec.binds.iter().find(|b| b.target == Path::new(RELAY_BIN));
        let riff = riff.expect("the bind of the riff");
        assert_eq!(
            riff.source,
            Source::Path(here.dir.with_file_name("static-riff"))
        );
        assert_eq!(
            here.fake.links(LEAD, Path::new(RELAY_BIN)).await.unwrap(),
            1,
            "the new container binds the file on disk"
        );

        // The person heard once, in riff chat. The lead kept its session on
        // riff-server: no leave, and the keep-alive goes on.
        assert_eq!(said_of(&server, "chat"), [format!("chat {LEAD}: {line}")]);
        assert!(said_of(&server, "leave").is_empty(), "{:?}", server.said());
        assert_eq!(said_of(&server, "resume").len(), 2);
        let status = status(&here.dir).await.unwrap();
        assert!(status.sessions.iter().any(|s| s.name == LEAD && s.lead));

        // On the new riff: the next look does nothing.
        assert!(here.daemon.leads_again().await.is_empty());
        assert_eq!(said_of(&server, "chat").len(), 1);
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[tokio::test]
    async fn a_new_daemon_starts_again_the_lead_that_it_takes_up_on_the_old_riff() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let mut here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let record = started_lead(&here).await;
        transcript_of(&record, 600);

        // `riff update` replaces the riff, and the daemon starts again.
        update_riff(&here);
        here.restart().await;
        here.daemon.recover().await;
        assert_eq!(here.fake.calls(), [format!("terminal {LEAD}")]);

        // The new daemon owns no container of its own: it looks at the
        // container, and it takes the lead again at its start.
        let line = text::lead_started_again(LEAD);
        assert_eq!(here.daemon.leads_again().await, std::slice::from_ref(&line));
        assert_eq!(
            here.fake.calls(),
            [
                format!("terminal {LEAD}"),
                format!("stop {LEAD}"),
                format!("terminal {LEAD}")
            ]
        );
        assert_eq!(said_of(&server, "chat"), [format!("chat {LEAD}: {line}")]);
        assert!(said_of(&server, "leave").is_empty(), "{:?}", server.said());
        let status = status(&here.dir).await.unwrap();
        assert!(status.sessions.iter().any(|s| s.name == LEAD && s.lead));
        assert!(here.daemon.leads_again().await.is_empty());
    }

    /// 01M5TC29A4Q94NJP7HFC75XCF3
    #[tokio::test]
    async fn a_lead_that_cannot_start_again_runs_on_and_the_person_hears_of_it_one_time() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let record = started_lead(&here).await;
        transcript_of(&record, 600);
        update_riff(&here);
        let terminal = format!("terminal {LEAD}");

        // The daemon knows no main clone: the lead stays on the old riff.
        let waves = here.dir.join(crate::wave::WAVES_FILE);
        let kept = std::fs::read(&waves).unwrap();
        std::fs::remove_file(&waves).unwrap();
        let no_wave = text::lead_again_no_wave("como-technologies/riff");
        let line = text::lead_again_waits(LEAD, &no_wave);
        assert_eq!(here.daemon.leads_again().await, std::slice::from_ref(&line));
        assert_eq!(here.fake.calls(), std::slice::from_ref(&terminal));
        assert_eq!(said_of(&server, "chat"), [format!("chat {LEAD}: {line}")]);

        // The host fails the next try: the person heard of it already.
        std::fs::write(&waves, &kept).unwrap();
        *server.again_fault.lock().unwrap() = Some("gh is not signed in".into());
        assert!(here.daemon.leads_again().await.is_empty());
        assert_eq!(here.fake.calls(), std::slice::from_ref(&terminal));
        assert_eq!(said_of(&server, "chat").len(), 1);

        // The fault goes: the next look starts the lead again.
        *server.again_fault.lock().unwrap() = None;
        let started = text::lead_started_again(LEAD);
        assert_eq!(
            here.daemon.leads_again().await,
            std::slice::from_ref(&started)
        );
        let chats = said_of(&server, "chat");
        assert_eq!(chats.len(), 2, "{chats:?}");
        assert_eq!(chats[1], format!("chat {LEAD}: {started}"));
        assert!(said_of(&server, "leave").is_empty());
    }

    /// 01M5TC29A4Q94NJP7HFC75XCF3
    #[tokio::test]
    async fn a_lead_that_stopped_and_cannot_start_again_leaves_riff_server_and_the_person_starts_it()
     {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let record = started_lead(&here).await;
        transcript_of(&record, 600);
        update_riff(&here);

        // podman cannot run the new container.
        here.fake.refuse_start(LEAD);
        let why = format!("the fake refuses to start the session {LEAD}");
        let line = text::lead_again_failed(LEAD, &why);
        assert_eq!(here.daemon.leads_again().await, std::slice::from_ref(&line));
        let calls = here.fake.calls();
        assert_eq!(
            calls[..2],
            [format!("terminal {LEAD}"), format!("stop {LEAD}")]
        );
        assert_eq!(
            calls[2..],
            vec![format!("terminal {LEAD}"); 3],
            "three tries, then the end"
        );
        assert_eq!(said_of(&server, "chat"), [format!("chat {LEAD}: {line}")]);
        assert_eq!(
            said_of(&server, "leave"),
            [format!("leave {}", record.live.session)]
        );
        assert!(
            !status(&here.dir)
                .await
                .unwrap()
                .sessions
                .iter()
                .any(|s| s.name == LEAD)
        );
        assert!(here.daemon.leads_again().await.is_empty());
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[tokio::test]
    async fn a_worker_on_the_old_riff_is_not_started_again() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let (_client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        update_riff(&here);
        assert!(here.daemon.leads_again().await.is_empty());
        assert_eq!(here.fake.calls(), [format!("start {name}")]);
    }

    #[tokio::test]
    async fn the_lead_runs_with_a_terminal_counts_in_no_limit_and_leaves_riff_server_at_its_end() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (mut client, name) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
        drop(client);

        // The daemon starts it with a terminal: the relay runs claude in
        // its own place.
        assert_eq!(here.fake.calls(), ["terminal riff-lead-o-r"]);
        // Its socket says that it is the lead.
        assert!(lead_in(&session_dir(&here.dir, &name)).await);
        let spec = here.fake.spec(&name).unwrap();
        assert_eq!(
            spec.command,
            [
                "/opt/riff/riff",
                "relay",
                "--terminal",
                "--",
                "claude",
                "--effort",
                "xhigh"
            ]
        );
        // It counts in no limit, and keeps its session alive.
        assert_eq!(here.daemon.live_count(), 0);
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Terminal);
        assert_eq!(server.said(), ["resume riff-lead-o-r-session"]);
        let record: Record =
            serde_json::from_str(&std::fs::read_to_string(record_path(&here.dir, &name)).unwrap())
                .unwrap();
        assert!(record.terminal && record.inputs.is_empty());

        // riff relay watch gets the next wake of the session on its
        // riff-server.
        assert_eq!(wake_of(&here.dir, &name).await.unwrap(), "a wake of lead");

        // A takeover of the lead is refused: a person attaches to it.
        let (_, refused) = take_over(&here.dir, &name).await;
        assert_eq!(
            refused,
            Say::Failed {
                error: text::takeover_terminal(&name)
            }
        );

        // At its end, the daemon ends its session on riff-server.
        here.fake.stop(&name).await.unwrap();
        until(|| server.said().len() == 2).await;
        assert_eq!(server.said()[1], "leave riff-lead-o-r-session");
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
    }

    /// 01M579Z9VRD12FRK6DSYVKDHQB: `riff daemon end NAME` frees the item
    /// of the session on riff-server first, then stops its container.
    /// The session ends. The daemon refuses the lead, and a session that
    /// it does not own.
    #[tokio::test]
    async fn an_end_frees_the_item_at_once_and_stops_the_container() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let (mut client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        let mut lines = Vec::new();
        let ask = Ask::End { name: name.clone() };
        ask_done(&here.dir, ask, |line| lines.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(
            lines,
            [text::daemon_freed("issue-12"), text::daemon_ends(&name)]
        );
        assert_eq!(server.said(), ["free issue-12"]);
        let (_, end) = to_the_end(&mut client).await;
        assert!(matches!(end, Some(Say::Ended(Ended { failed: true, .. }))));
        assert!(here.fake.calls().contains(&format!("stop {name}")));
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());

        let gone = ask_done(&here.dir, Ask::End { name: name.clone() }, |_| {}).await;
        assert_eq!(
            gone.unwrap_err().to_string(),
            text::daemon_no_session(&name)
        );
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (_lead, lead) = here.started(start).await;
        let refused = ask_done(&here.dir, Ask::End { name: lead.clone() }, |_| {}).await;
        assert_eq!(
            refused.unwrap_err().to_string(),
            text::daemon_end_lead(&lead)
        );
        assert!(!server.said().contains(&"free lead".to_owned()));
    }

    /// 01M579Z9SAN24QGN1VV5C7EHM9: a daemon that runs the riff on disk
    /// does not start again for `riff daemon restart`, and says so.
    #[tokio::test]
    async fn a_restart_of_a_daemon_on_the_riff_on_disk_does_nothing() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut lines = Vec::new();
        ask_done(&here.dir, Ask::Restart, |line| lines.push(line.to_owned()))
            .await
            .unwrap();
        let build = Build::this().to_string();
        assert_eq!(
            lines,
            [text::daemon_restart(remote::Restart::Newest, &build)]
        );
    }

    /// With the limit 0, the wave of `o/r` of `daemon` in the folder
    /// `dir`: it waits for room always (01M5MS1Q78AFEY3ZVXNAQXM1FJ).
    async fn a_wave_that_waits<R: Runtime + 'static>(daemon: &Arc<Daemon<R>>, dir: &Path) {
        let none = crate::room::Change {
            sessions: Some(0),
            ..crate::room::Change::default()
        };
        crate::room::set(dir, none).unwrap();
        let wave = Wave {
            repo: "o/r".into(),
            main: dir.join("r"),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(dir, &BTreeMap::from([(wave.repo.clone(), wave)])).unwrap();
        daemon.run_waves();
        until(|| daemon.room().waiting().contains(&text::wave_slot("o/r"))).await;
    }

    /// 01M579Z9SAN24QGN1VV5C7EHM9, 01M5MS1Q78AFEY3ZVXNAQXM1FJ: a daemon
    /// on an old riff starts again only when no session runs and no
    /// start of `riff run` waits, after the reply. A start of a wave that
    /// waits for room with the limit 0 does not keep it.
    #[tokio::test]
    async fn a_restart_on_an_old_riff_waits_for_a_start_of_riff_run_and_not_of_a_wave() {
        let root = tempfile::tempdir().unwrap();
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = asked.clone();
        let again = remote::Again {
            old: Box::new(|| true),
            restart: Box::new(move || {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }),
        };
        let dir = dir_in(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let daemon = Arc::new(
            Daemon::new(Fake::default(), dir.clone())
                .unwrap()
                .with_again(again),
        );
        let build = Build::this().to_string();
        a_wave_that_waits(&daemon, &dir).await;
        // A start of `riff run` that waits for room.
        let room = daemon.room().clone();
        let waits =
            tokio::spawn(async move { room.admit("riff-issue-12-1a2b3c4d", || 0, |_| {}).await });
        until(|| daemon.room().waiting().len() == 2).await;
        assert_eq!(
            daemon.restart_now().await,
            text::daemon_restart(remote::Restart::Later(1), &build)
        );
        waits.abort();
        let _ = waits.await;
        assert_eq!(daemon.room().waiting(), [text::wave_slot("o/r")]);
        assert_eq!(
            daemon.restart_now().await,
            text::daemon_restart(remote::Restart::Now, &build)
        );
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);
        tokio::time::sleep(RESTART_DELAY + Duration::from_secs(1)).await;
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// #1262: a daemon that runs the wave of a repository serves it at
    /// its start, with the server of the wave, and keeps a repository
    /// that it serves already.
    #[tokio::test]
    async fn the_start_serves_the_repository_of_each_wave() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let wave = |repo: &str, server: &str| Wave {
            repo: repo.into(),
            main: root.path().join(repo),
            server: server.into(),
            host: "pangolin".into(),
        };
        let waves = [
            ("o/new".to_owned(), wave("o/new", "http://127.0.0.1:1")),
            ("o/old".to_owned(), wave("o/old", "http://127.0.0.1:2")),
        ]
        .into_iter()
        .collect();
        crate::wave::write_waves(&dir, &waves).unwrap();
        let old = remote::serve_in_file(&dir, &wave("o/old", "http://127.0.0.1:3")).unwrap();
        Daemon::new(Fake::default(), dir.clone())
            .unwrap()
            .serve_repos();
        let served = remote::read_served(&dir).unwrap();
        assert_eq!(served["o/new"].wave.server, "http://127.0.0.1:1");
        assert_eq!(served["o/old"], old);
    }

    /// The result of a turn that an interrupt stopped, as `claude -p`
    /// gives it.
    const STOPPED: &str =
        r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":3}"#;

    #[tokio::test]
    async fn a_takeover_interrupts_the_turn_of_the_worker_and_gives_it_back_with_one_input() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name, mut relay) = a_worker(&here).await;
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);

        let (mut taker, first) = take_over(&here.dir, &name).await;
        assert_eq!(
            first,
            Say::Line {
                text: text::takeover_interrupts(&name)
            }
        );
        // The relay interrupts claude and closes its stdin: the turn
        // stops with an error, which is no fail of the session.
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Holds);
        relay.event(2, TURN[1]).await;
        relay.event(3, STOPPED).await;
        relay.send(&Up::Held).await;
        assert_eq!(
            next_of(&mut taker).await,
            Some(Say::Held {
                session: "riff-issue-12-1a2b3c4d-session".into()
            })
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Taken);
        let record = record_of(&here.dir, &name);
        assert!(record.interrupt && record.closed && !record.failed);

        // The terminal of the person ended: the session goes on.
        taker.go_on().await.unwrap();
        assert_eq!(next_of(&mut taker).await, Some(Say::Done));
        assert_eq!(relay.next().await, Say::GoOn);
        let go_on = user_line(text::TAKEOVER_GO_ON);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: go_on.clone()
            }
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);
        let record = record_of(&here.dir, &name);
        assert_eq!(record.inputs, [user_line("Do it."), go_on]);
        assert!(!record.takeover && !record.closed && !record.interrupt);

        // The new turn ends as each turn: the close, then the exit.
        relay.event(4, TURN[2]).await;
        relay.exit(5, 0).await;
        let (said, end) = to_the_end(&mut client).await;
        assert_eq!(events(&said).len(), 4);
        assert_eq!(
            end,
            Some(Say::Ended(Ended {
                code: Some(0),
                failed: false
            }))
        );
    }

    /// A fake `gh` in `dir`: a POST of a comment adds its body to the
    /// file `posted`. It writes a new file and renames it, so a test
    /// that waits for `posted` never reads a part of the body (#1259).
    /// With `down`, each call notes itself in the file `tried` and
    /// fails.
    fn usage_gh(dir: &Path, down: bool) -> crate::pr::Gh {
        let script = match down {
            true => {
                "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/tried\"\necho 'you are not logged in' >&2\nexit 1\n"
            }
            false => concat!(
                "#!/bin/sh\n",
                "d=$(dirname \"$0\")\n",
                "if [ \"$1 $2 $3\" = \"api -X POST\" ]; then\n",
                "  { cat \"$d/posted\" 2>/dev/null; cat \"$7\"; } > \"$d/posted.new\"\n",
                "  mv \"$d/posted.new\" \"$d/posted\"\n",
                "fi\n",
            ),
        };
        crate::pr::Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    /// Runs one turn of the container session `name` of `item` on a
    /// daemon with the marks in `marks` and `gh`: `claude` writes one
    /// reply with tokens in the transcript of the session, in its Claude
    /// folder. Gives how the session ended.
    async fn a_session_with_tokens(
        name: &str,
        item: &str,
        marks: &Path,
        gh: crate::pr::Gh,
    ) -> Ended {
        let server = Arc::new(Server::default().with_meter(marks, gh));
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let start = here.start(name, item);
        let project = start.folder.join("claude/projects/-data-clone");
        let transcript = project.join(format!("{}.jsonl", start.session));
        let (mut client, name) = here.started(start).await;
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 0, false).await;
        assert!(matches!(relay.next().await, Say::Input { .. }));
        std::fs::create_dir_all(&project).unwrap();
        // A reply counts from the start of the claim to before its end.
        std::thread::sleep(Duration::from_millis(5));
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let reply = serde_json::json!({
            "type": "assistant",
            "timestamp": at,
            "message": {
                "id": "m1",
                "model": "claude-opus-5-5",
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 20,
                    "cache_creation_input_tokens": 300,
                    "cache_read_input_tokens": 4000,
                },
            },
        });
        std::fs::write(&transcript, format!("{reply}\n")).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        for (seq, line) in (1..).zip(TURN) {
            relay.event(seq, line).await;
        }
        relay.exit(4, 0).await;
        match to_the_end(&mut client).await.1 {
            Some(Say::Ended(ended)) => ended,
            other => panic!("{other:?}"),
        }
    }

    /// 01M588N85Z94PX1ZP0BDXTPX83: at the end of a container session,
    /// the daemon puts the tokens of its claim on the issue, as one
    /// comment, for a work claim and for a verify claim.
    #[tokio::test]
    async fn the_end_of_a_container_session_puts_the_tokens_of_its_claim_on_the_issue() {
        use crate::usage::Kind;
        let claims = [
            ("issue-12", Kind::Work, "work"),
            ("verify-issue-12", Kind::Verify, "verify"),
        ];
        for (item, of, kind) in claims {
            let (bin, marks) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
            let gh = usage_gh(bin.path(), false);
            let name = format!("riff-{item}-1a2b3c4d");
            let ended = a_session_with_tokens(&name, item, marks.path(), gh).await;
            assert!(!ended.failed);
            let posted = bin.path().join("posted");
            until(|| posted.exists()).await;
            // The fake keeps the body that `gh api --input` sends.
            let sent: Value =
                serde_json::from_str(&std::fs::read_to_string(&posted).unwrap()).unwrap();
            let comment = sent["body"].as_str().unwrap().to_owned();
            let report = crate::usage::Report::parse(&comment).expect(&comment);
            assert_eq!(report.item, item);
            assert_eq!(report.kind, of);
            // The first 8 characters of the session ID.
            let short = &name[..8];
            assert_eq!(report.session, short);
            assert_eq!(
                report.models["claude-opus-5-5"],
                crate::usage::Tokens {
                    input: 1,
                    output: 20,
                    cache_write: 300,
                    cache_read: 4000
                }
            );
            assert!(comment.starts_with(&format!("riff usage: {item}, {kind}, session {short}, ")));
            assert_eq!(comment.matches("riff:usage").count(), 1, "{comment}");
        }
    }

    /// 01M3Y1YP39VFX6GH33H7B8A8KR: a `gh` that fails does not fail the
    /// end of a container session. The sum stays in the marks.
    #[tokio::test]
    async fn a_gh_that_fails_keeps_the_tokens_of_a_container_session_in_the_marks() {
        let (bin, marks) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let gh = usage_gh(bin.path(), true);
        let ended =
            a_session_with_tokens("riff-issue-12-1a2b3c4d", "issue-12", marks.path(), gh).await;
        assert_eq!(
            ended,
            Ended {
                code: Some(0),
                failed: false
            }
        );
        let session = "riff-issue-12-1a2b3c4d-session";
        until(|| {
            crate::usage::load(marks.path(), session)
                .claims
                .iter()
                .any(|c| c.to_ms.is_some())
        })
        .await;
        let claims = crate::usage::load(marks.path(), session).claims;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].item, "issue-12");
        assert_eq!(claims[0].thread, "como-technologies/riff");
        assert_eq!(crate::usage::total(&claims[0].models).total(), 4321);
        // The daemon tried gh, and wrote no comment.
        until(|| bin.path().join("tried").exists()).await;
        assert!(!bin.path().join("posted").exists());
    }

    /// The record of the session `name` on disk.
    fn record_of(dir: &Path, name: &str) -> Record {
        serde_json::from_str(&std::fs::read_to_string(record_path(dir, name)).unwrap()).unwrap()
    }

    /// A daemon on the fake runtime with the riff-server of the test.
    async fn with_server() -> (Here, Arc<Server>) {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        (here, server)
    }

    /// The wake of the message `seq` of the lead in the thread of the
    /// repository.
    fn a_wake(seq: u64) -> Wake {
        Wake {
            thread: "como-technologies/riff".parse().unwrap(),
            seq,
            from: "riff://mike@thelio/como-technologies/riff?session=lead"
                .parse()
                .unwrap(),
            kind: riff_core::wire::Kind::Message,
        }
    }

    /// 01M59235BBXEKPZBRH23C39574: one `riff relay watch --follow` gets
    /// two wakes in a row, and each wake once, with no restart.
    #[tokio::test]
    async fn a_follow_of_the_lead_gets_two_wakes_in_a_row_with_no_restart() {
        let (here, server) = with_server().await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (_client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name);
        let conn = connect_in(&socket, SOCKET).await.unwrap();
        let (read, mut write) = conn.into_split();
        let ask = serde_json::to_string(&SessionAsk::Follow { protocol: PROTOCOL }).unwrap();
        write
            .write_all(format!("{ask}\n").as_bytes())
            .await
            .unwrap();
        let mut lines = BufReader::new(read).lines();
        until(|| server.watches() == 1).await;
        server.wake_all(&a_wake(417));
        server.wake_all(&a_wake(417));
        server.wake_all(&a_wake(418));
        for seq in [417, 418] {
            let line = lines.next_line().await.unwrap().unwrap();
            assert_eq!(
                serde_json::from_str::<Say>(&line).unwrap(),
                Say::Line {
                    text: text::wake_line(&a_wake(seq))
                }
            );
        }
    }
    #[tokio::test]
    async fn a_wake_during_the_turn_reaches_the_worker_and_a_wake_after_the_result_gets_a_refusal()
    {
        let (here, server) = with_server().await;
        let (_client, name, mut relay) = a_worker(&here).await;
        until(|| server.watches() == 1).await;

        // During the turn: the wake is one input on the stdin of claude.
        server.wake_all(&a_wake(7));
        let line = user_line(&text::wake_line(&a_wake(7)));
        assert_eq!(relay.next().await, Say::Input { line: line.clone() });
        assert_eq!(
            record_of(&here.dir, &name).inputs,
            [user_line("Do it."), line.clone()]
        );

        // A new watch of riff-server gives the same wake again: claude
        // gets it once.
        server.wake_all(&a_wake(7));
        server.wake_all(&a_wake(8));
        let eight = user_line(&text::wake_line(&a_wake(8)));
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: eight.clone()
            }
        );

        // A relay that connects again gets no wake twice.
        drop(relay);
        let (mut relay, _) = Relay::connect(&session_dir(&here.dir, &name), 3, false).await;
        relay.event(2, TURN[2]).await;

        // After the result, the input is closed: the sender gets the
        // refusal, and claude gets no input.
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Ends);
        server.wake_all(&a_wake(9));
        until(|| server.said().iter().any(|s| s.starts_with("refuse"))).await;
        assert_eq!(server.said(), ["refuse riff-issue-12-1a2b3c4d-session 9"]);
        assert_eq!(
            record_of(&here.dir, &name).inputs,
            [user_line("Do it."), line, eight]
        );
    }

    #[tokio::test]
    async fn a_closed_session_refuses_each_sender_once() {
        let (here, server) = with_server().await;
        let (_client, name, mut relay) = a_worker(&here).await;
        until(|| server.watches() == 1).await;
        relay.event(2, TURN[2]).await;
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Ends);

        // Another closed session refuses each refusal: each one is a
        // new wake from it. The session refuses it once, so the two
        // do not refuse each other with no end.
        for seq in 9..12 {
            server.wake_all(&a_wake(seq));
        }
        // A wake from another sender gets its own refusal.
        let other = Wake {
            from: me("riff-issue-13-5e6f7a8b"),
            ..a_wake(12)
        };
        server.wake_all(&other);
        until(|| server.said().len() == 2).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            server.said(),
            [
                "refuse riff-issue-12-1a2b3c4d-session 9",
                "refuse riff-issue-12-1a2b3c4d-session 12",
            ]
        );
    }

    #[tokio::test]
    async fn a_wake_during_a_takeover_reaches_the_worker_after_the_go_on() {
        let (here, server) = with_server().await;
        let (_client, name, mut relay) = a_worker(&here).await;
        until(|| server.watches() == 1).await;
        let (mut taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        relay.event(2, STOPPED).await;
        relay.send(&Up::Held).await;
        assert!(matches!(next_of(&mut taker).await, Some(Say::Held { .. })));

        // The terminal of the person holds claude: the wake waits.
        server.wake_all(&a_wake(7));
        until(|| owned_waiting(&here, &name) == 1).await;
        taker.go_on().await.unwrap();
        assert_eq!(relay.next().await, Say::GoOn);
        let go_on = user_line(text::TAKEOVER_GO_ON);
        let wake = user_line(&text::wake_line(&a_wake(7)));
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: go_on.clone()
            }
        );
        assert_eq!(relay.next().await, Say::Input { line: wake.clone() });
        assert_eq!(
            record_of(&here.dir, &name).inputs,
            [user_line("Do it."), go_on, wake]
        );
        assert!(server.said().is_empty(), "{:?}", server.said());
    }

    /// The number of wakes of the session `name` that wait for the go on.
    fn owned_waiting(here: &Here, name: &str) -> usize {
        here.daemon
            .lock()
            .get(name)
            .map_or(0, |o| o.lock().waiting.len())
    }

    #[tokio::test]
    async fn the_daemon_watches_no_wake_of_the_lead() {
        let (here, server) = with_server().await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (mut client, _) = here.started(start).await;
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(server.watches(), 0);
    }

    #[tokio::test]
    async fn a_takeover_waits_for_the_end_of_the_turn_with_at_end_or_after_the_result() {
        let waits = |name: &str| Say::Line {
            text: text::takeover_waits(name),
        };
        let at_end = Say::Hold { interrupt: false };

        // riff takeover --at-end.
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (_taker, first) = take_over_with(&here.dir, &name, true).await;
        assert_eq!(first, waits(&name));
        assert_eq!(relay.next().await, at_end);
        assert!(!record_of(&here.dir, &name).interrupt);

        // After the result, the input of claude is closed: claude ends,
        // or waits for its background tasks.
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        relay.event(2, TURN[2]).await;
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Ends);
        let (_taker, first) = take_over(&here.dir, &name).await;
        assert_eq!(first, waits(&name));
        assert_eq!(relay.next().await, at_end);
    }

    #[tokio::test]
    async fn the_quiet_limit_does_not_stop_a_session_in_a_takeover() {
        let quiet = Duration::from_millis(200);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (mut taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        // The result closes the input, and the quiet limit would count.
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert!(matches!(next_of(&mut taker).await, Some(Say::Held { .. })));
        tokio::time::sleep(quiet * 4).await;
        assert!(
            !here.fake.calls().iter().any(|c| c.starts_with("stop")),
            "{:?}",
            here.fake.calls()
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Taken);
    }

    #[tokio::test]
    async fn the_daemon_refuses_a_takeover_with_no_relay_or_with_a_relay_of_protocol_3() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (_client, name) = here.started(start).await;
        let refused = Say::Failed {
            error: text::takeover_no_relay(&name),
        };
        // No relay yet.
        assert_eq!(take_over(&here.dir, &name).await.1, refused);
        // A relay of protocol 3, before the takeover.
        let socket = session_dir(&here.dir, &name);
        let old = SessionAsk::Relay {
            protocol: 3,
            inputs: 0,
            closed: false,
            held: false,
        };
        let (_relay, _) = Relay::connect_as(&socket, old).await;
        assert_eq!(take_over(&here.dir, &name).await.1, refused);
        // A session that the daemon does not own.
        assert_eq!(
            take_over(&here.dir, "riff-other").await.1,
            Say::Failed {
                error: text::daemon_no_session("riff-other")
            }
        );
    }

    #[tokio::test]
    async fn a_person_who_leaves_before_the_end_of_the_turn_gets_no_terminal_and_the_session_goes_on()
     {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        // Ctrl-C before the end of the turn: the connection ends.
        drop(taker);
        until(|| {
            std::fs::read_to_string(record_path(&here.dir, &name))
                .ok()
                .and_then(|t| serde_json::from_str::<Record>(&t).ok())
                .is_some_and(|r| !r.takeover)
        })
        .await;
        // claude ends its turn for the hold: the daemon goes on at once.
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert_eq!(relay.next().await, Say::GoOn);
        assert_eq!(
            relay.next().await,
            Say::Input {
                line: user_line(text::TAKEOVER_GO_ON)
            }
        );
        assert_eq!(phase_of(&here.dir, &name).await, Phase::Works);
    }

    #[tokio::test]
    async fn a_new_daemon_keeps_a_takeover_and_a_new_riff_takeover_gets_the_terminal_at_once() {
        let mut here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        // The first taker stays: its process went with the old daemon.
        let (mut first, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        relay.send(&Up::Held).await;
        assert!(matches!(next_of(&mut first).await, Some(Say::Held { .. })));

        here.restart().await;
        here.daemon.recover().await;
        let socket = session_dir(&here.dir, &name);
        let held = SessionAsk::Relay {
            protocol: PROTOCOL,
            inputs: 1,
            closed: true,
            held: true,
        };
        let (mut relay, had) = Relay::connect_as(&socket, held).await;
        assert_eq!(had, 1);
        let (mut taker, first_line) = take_over(&here.dir, &name).await;
        assert_eq!(
            first_line,
            Say::Held {
                session: "riff-issue-12-1a2b3c4d-session".into()
            }
        );
        taker.go_on().await.unwrap();
        assert_eq!(next_of(&mut taker).await, Some(Say::Done));
        assert_eq!(relay.next().await, Say::GoOn);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        drop(first);
    }

    #[tokio::test]
    async fn riff_takeover_runs_the_terminal_of_the_person_and_gives_the_session_back() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (_client, name, mut relay) = a_worker(&here).await;
        let (dir, fake, at) = (here.dir.clone(), here.fake.clone(), name.clone());
        let shown = Arc::new(Mutex::new(Vec::new()));
        let lines = shown.clone();
        let taken = tokio::spawn(async move {
            crate::takeover::takeover(&dir, &fake, &at, false, |line| {
                lines
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(line.to_owned());
            })
            .await
        });
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        relay.event(2, TURN[2]).await;
        relay.send(&Up::Held).await;
        assert_eq!(relay.next().await, Say::GoOn);
        assert!(matches!(relay.next().await, Say::Input { .. }));
        assert_eq!(taken.await.unwrap().unwrap(), 0);
        let exec = format!(
            "exec {name} /opt/riff/riff relay --terminal -- claude --resume \
             riff-issue-12-1a2b3c4d-session --permission-mode auto"
        );
        assert!(here.fake.calls().contains(&exec), "{:?}", here.fake.calls());
        let shown = shown.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert_eq!(
            shown,
            [
                text::takeover_interrupts(&name),
                text::takeover_held(&name),
                text::takeover_back(&name)
            ]
        );
    }

    #[tokio::test]
    async fn a_session_that_ends_while_a_takeover_waits_leaves_no_record() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (mut client, name, mut relay) = a_worker(&here).await;
        let (mut taker, _) = take_over(&here.dir, &name).await;
        assert_eq!(relay.next().await, Say::Hold { interrupt: true });
        // claude ended its turn before the hold: an exit, and no held.
        relay.event(2, TURN[2]).await;
        relay.exit(3, 0).await;
        assert!(matches!(next_of(&mut taker).await, Some(Say::Ended(_))));
        let (_, end) = to_the_end(&mut client).await;
        assert!(matches!(end, Some(Say::Ended(_))));
        // The end of riff takeover writes no record of the session.
        drop(taker);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!record_path(&here.dir, &name).exists());
        assert!(status(&here.dir).await.unwrap().sessions.is_empty());
    }

    /// The path of the unit file of [`Units`].
    const UNIT_FILE: &str = "/home/mike/.config/systemd/user/riff.service";

    /// A service manager with its unit in memory. It logs each change.
    #[derive(Default)]
    struct Units {
        unit: Mutex<Option<Unit>>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl Units {
        /// A unit that runs `exe`, with a `PATH`.
        fn of(exe: &Path) -> Units {
            let unit = Unit {
                exe: exe.to_owned(),
                env: vec![("PATH".into(), "/home/mike/.cargo/bin:/usr/bin".into())],
            };
            Units {
                unit: Mutex::new(Some(unit)),
                calls: Mutex::default(),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }

        fn exe(&self) -> Option<PathBuf> {
            self.unit.lock().unwrap().as_ref().map(|u| u.exe.clone())
        }
    }

    impl ServiceManager for Units {
        fn install(&self, _: &Unit) -> Result<PathBuf> {
            bail!("this test installs nothing")
        }

        fn uninstall(&self) -> Result<Option<PathBuf>> {
            bail!("this test removes nothing")
        }

        fn state(&self) -> Result<String> {
            Ok("active".into())
        }

        fn installed(&self) -> Result<Option<Unit>> {
            Ok(self.unit.lock().unwrap().clone())
        }

        fn rewrite(&self, unit: &Unit) -> Result<PathBuf> {
            self.calls.lock().unwrap().push("rewrite");
            *self.unit.lock().unwrap() = Some(unit.clone());
            Ok(UNIT_FILE.into())
        }

        fn try_restart(&self) -> Result<()> {
            self.calls.lock().unwrap().push("try-restart");
            Ok(())
        }

        fn main_pid(&self) -> Result<Option<u32>> {
            Ok(Some(std::process::id()))
        }
    }

    /// The riff of a verify after the release, which a unit can still
    /// run, and the new riff on `PATH`.
    fn check_and_new(here: &Here) -> (PathBuf, PathBuf) {
        let check = PathBuf::from("/home/mike/.cache/riff/check/target/static/release/riff");
        (check, here.root.path().join("bin/riff"))
    }

    /// The unit runs an old riff, and the daemon owns no session: `riff
    /// update` writes the unit with the riff on `PATH`, keeps its
    /// variables, and starts the daemon again at once
    /// (01M517BTDRKDXD3X84P6AEAF7P, 01M517BTJ9EV4ZGPYRB6HXJJC9).
    #[tokio::test]
    async fn after_an_update_an_idle_daemon_runs_the_new_riff_at_once() {
        let here = Here::new(Fake::default()).await;
        let (check, riff) = check_and_new(&here);
        let units = Units::of(&check);
        let lines = after_update(&units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::DAEMON_STARTS_AGAIN.to_owned()
            ]
        );
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);
        let unit = units.installed().unwrap().unwrap();
        assert_eq!(unit.exe, riff);
        assert_eq!(
            unit.env,
            Units::of(&check).installed().unwrap().unwrap().env
        );
    }

    /// A lead does not keep the daemon on the old riff: `riff update`
    /// starts the daemon again at once, and the new daemon takes the lead
    /// up with its container and its socket (#1324).
    #[tokio::test]
    async fn after_an_update_a_daemon_with_only_the_lead_starts_again_and_the_lead_stays() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let mut here = Here::with_world(
            Fake::default().with_image(IMAGE),
            QUIET_LIMIT,
            Some(world.clone()),
        )
        .await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        let (_client, name) = here.started(start).await;
        assert!(status(&here.dir).await.unwrap().sessions[0].lead);
        assert_eq!(status(&here.dir).await.unwrap().busy(), 0);

        let (check, riff) = check_and_new(&here);
        let units = Units::of(&check);
        let lines = after_update(&units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::DAEMON_STARTS_AGAIN.to_owned()
            ]
        );
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);

        // The restart stops no container: the new daemon takes the lead
        // up, and the lead answers on its socket.
        here.restart().await;
        here.daemon.recover().await;
        let status = status(&here.dir).await.unwrap();
        assert_eq!(status.sessions.len(), 1);
        assert_eq!(status.sessions[0].name, name);
        assert!(here.fake.running(&name).await.unwrap());
        assert!(lead_in(&session_dir(&here.dir, &name)).await);
    }

    /// Starts the leads `names` on `here`, each with a terminal.
    async fn leads_on(here: &Here, names: &[&str]) -> Vec<Client> {
        let mut clients = Vec::new();
        for name in names {
            let mut start = here.start(name, "lead");
            start.terminal = true;
            clients.push(here.started(start).await.0);
        }
        clients
    }

    /// The notes of the world that tell a lead of the new start of the
    /// daemon, by the name of the lead.
    fn restart_notes(server: &Server) -> Vec<String> {
        server
            .said()
            .into_iter()
            .filter(|s| s.contains(text::LEAD_DAEMON_STARTS_AGAIN))
            .map(|s| s.split(':').next().unwrap().to_owned())
            .collect()
    }

    /// 01M5SNV7H3739PXW73H14XM4B8: `riff update` tells each lead on the
    /// host first, one note for each, and then starts the daemon again
    /// on the new riff.
    #[tokio::test]
    async fn before_riff_update_starts_the_daemon_again_each_lead_on_the_host_gets_one_note() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let _leads = leads_on(&here, &["riff-lead-o-r", "riff-lead-o-s"]).await;
        assert!(restart_notes(&server).is_empty());

        let (check, riff) = check_and_new(&here);
        // The unit of this test moves the notes behind the restart: a note
        // that comes after the restart shows as a shorter list.
        struct Behind(Units, Arc<Server>, Mutex<Vec<String>>);
        impl ServiceManager for Behind {
            fn install(&self, unit: &Unit) -> Result<PathBuf> {
                self.0.install(unit)
            }
            fn uninstall(&self) -> Result<Option<PathBuf>> {
                self.0.uninstall()
            }
            fn state(&self) -> Result<String> {
                self.0.state()
            }
            fn installed(&self) -> Result<Option<Unit>> {
                self.0.installed()
            }
            fn rewrite(&self, unit: &Unit) -> Result<PathBuf> {
                self.0.rewrite(unit)
            }
            fn try_restart(&self) -> Result<()> {
                *self.2.lock().unwrap() = restart_notes(&self.1);
                self.0.try_restart()
            }
            fn main_pid(&self) -> Result<Option<u32>> {
                self.0.main_pid()
            }
        }
        let units = Behind(Units::of(&check), server.clone(), Mutex::default());
        let lines = after_update(&units, &riff, &here.dir).await.unwrap();
        assert_eq!(lines.last().unwrap(), text::DAEMON_STARTS_AGAIN);
        assert_eq!(units.0.calls(), ["rewrite", "try-restart"]);
        // At the restart, both leads had their note already.
        let mut at_restart = units.2.lock().unwrap().clone();
        at_restart.sort();
        assert_eq!(at_restart, ["note riff-lead-o-r", "note riff-lead-o-s"]);
        assert_eq!(restart_notes(&server).len(), 2, "one note for each lead");
    }

    /// 01M5SNV7H3739PXW73H14XM4B8: the daemon that looks at the end of a
    /// session tells each lead before it asks for its new start.
    #[tokio::test]
    async fn before_the_daemon_asks_for_its_new_start_each_lead_gets_one_note() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let here =
            Here::with_world(Fake::default().with_image(IMAGE), QUIET_LIMIT, Some(world)).await;
        let _leads = leads_on(&here, &["riff-lead-o-r", "riff-lead-o-s"]).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let restart = {
            let (server, seen) = (server.clone(), seen.clone());
            move || {
                *seen.lock().unwrap() = restart_notes(&server);
                Ok(())
            }
        };
        tokio::time::timeout(
            Duration::from_secs(5),
            here.daemon.restart_when_old(|| true, restart),
        )
        .await
        .expect("a daemon with only the leads asks at once");
        let mut at_restart = seen.lock().unwrap().clone();
        at_restart.sort();
        assert_eq!(at_restart, ["note riff-lead-o-r", "note riff-lead-o-s"]);
        assert_eq!(restart_notes(&server).len(), 2);
    }

    /// 01M5SNV7H3739PXW73H14XM4B8: `riff daemon restart` tells each lead
    /// at once, before the wait of the reply and the new start.
    #[tokio::test]
    async fn a_daemon_restart_tells_each_lead_before_the_new_start() {
        let server = Arc::new(Server::default());
        let world: Arc<dyn World> = server.clone();
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let again = {
            let counted = asked.clone();
            remote::Again {
                old: Box::new(|| true),
                restart: Box::new(move || {
                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }),
            }
        };
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let fake = Fake::default().with_image(IMAGE);
        let daemon = Arc::new(
            Daemon::new(fake, dir.clone())
                .unwrap()
                .with_relay(static_riff(&dir))
                .with_world(world)
                .with_again(again),
        );
        let serving = tokio::spawn(daemon.clone().serve(listener, std::future::pending()));
        let mut start = start_in(root.path(), "riff-lead-o-r", "lead");
        start.terminal = true;
        let mut client = Client::connect(&dir).await.unwrap();
        client.ask(Ask::Start(Box::new(start))).await.unwrap();
        assert!(matches!(
            client.next().await.unwrap(),
            Some(Say::Started { .. })
        ));
        let mut lines = Vec::new();
        ask_done(&dir, Ask::Restart, |line| lines.push(line.to_owned()))
            .await
            .unwrap();
        until(|| restart_notes(&server).len() == 1).await;
        assert_eq!(restart_notes(&server), ["note riff-lead-o-r"]);
        assert_eq!(
            asked.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "after the wait"
        );
        serving.abort();
    }

    /// While the daemon runs a session, `riff update` writes the unit and
    /// starts nothing. The daemon looks, and asks for its new start only
    /// at the end of its last session (01M517BTJ9EV4ZGPYRB6HXJJC9,
    /// 01M517BTP6BAGTRBYWPX9GPVE0).
    #[tokio::test]
    async fn after_an_update_a_busy_daemon_starts_again_after_its_last_session() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (check, riff) = check_and_new(&here);
        let units = Arc::new(Units::of(&check));
        let (mut client, _, mut relay) = a_worker(&here).await;

        let lines = after_update(&*units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::daemon_starts_again_later(1)
            ]
        );
        assert_eq!(units.calls(), ["rewrite"]);

        // The daemon runs the riff of the test, not the riff of the unit.
        let watch = {
            let (daemon, units) = (here.daemon.clone(), units.clone());
            tokio::spawn(async move {
                let own = own_binary().unwrap();
                let old = || stale(units.installed().unwrap().as_ref(), &own, false);
                let restart = || units.try_restart();
                daemon.restart_when_old(old, restart).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(units.calls(), ["rewrite"], "a new start with a session");
        assert!(!watch.is_finished());

        relay.exit(2, 0).await;
        let (_, end) = to_the_end(&mut client).await;
        assert!(matches!(end, Some(Say::Ended(_))));
        tokio::time::timeout(Duration::from_secs(5), watch)
            .await
            .expect("the end of the session looks")
            .unwrap();
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);
    }

    /// 01M5QVBTXE8YRY44A1C1VP71QS: a real-session test that runs counts
    /// as one in [`Status::busy`], so `riff update` waits for it. A status
    /// of an old daemon has no test.
    #[test]
    fn a_real_session_test_that_runs_counts_as_one_busy() {
        let mut json = serde_json::json!({
            "pid": 7, "build": "4.1.0", "since_ms": 0, "dir": "/d", "sessions": [],
        });
        let old: Status = serde_json::from_value(json.clone()).unwrap();
        assert_eq!((old.test.clone(), old.busy()), (None, 0));

        json["test"] = serde_json::json!({"tag": "v4.1.1", "commit": "90e36bfd", "secs": 60});
        let testing: Status = serde_json::from_value(json).unwrap();
        assert_eq!(testing.test.as_ref().map(|t| t.secs), Some(60));
        assert_eq!(testing.busy(), 1);
        assert_eq!(
            remote::Restart::of(true, testing.busy()),
            remote::Restart::Later(1)
        );

        let more = Status { jobs: 2, ..testing };
        assert_eq!(more.busy(), 3);
    }

    /// With the limit 0, a start of a wave waits for room always. `riff
    /// update` and the look of the daemon start the daemon again on the
    /// new riff all the same (01M517BTJ9EV4ZGPYRB6HXJJC9,
    /// 01M517BTP6BAGTRBYWPX9GPVE0, 01M5MS1Q78AFEY3ZVXNAQXM1FJ).
    #[tokio::test]
    async fn after_an_update_a_wave_that_waits_for_room_does_not_keep_the_old_riff() {
        let here = Here::new(Fake::default()).await;
        a_wave_that_waits(&here.daemon, &here.dir).await;
        assert_eq!(here.daemon.status().await.busy(), 0);

        let (check, riff) = check_and_new(&here);
        let units = Arc::new(Units::of(&check));
        let lines = after_update(&*units, &riff, &here.dir).await.unwrap();
        assert_eq!(
            lines,
            [
                text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff),
                text::DAEMON_STARTS_AGAIN.to_owned()
            ]
        );
        assert_eq!(units.calls(), ["rewrite", "try-restart"]);

        let asked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let restart = {
            let asked = asked.clone();
            move || {
                asked.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        };
        tokio::time::timeout(
            Duration::from_secs(5),
            here.daemon.restart_when_old(|| true, restart),
        )
        .await
        .expect("a start that waits does not keep the daemon");
        assert!(asked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(here.daemon.room().waiting(), [text::wave_slot("o/r")]);
    }

    /// The unit runs the riff on `PATH`, and the daemon runs that riff:
    /// `riff update` changes nothing, and the daemon asks nothing. With
    /// no unit, riff changes nothing. With no daemon, riff only writes
    /// the unit.
    #[tokio::test]
    async fn after_an_update_a_daemon_on_the_new_riff_stays() {
        let here = Here::new(Fake::default()).await;
        let own = own_binary().unwrap();
        let units = Arc::new(Units::of(&own));
        assert!(
            after_update(&*units, &own, &here.dir)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(units.calls().is_empty());
        let watch = {
            let (daemon, units) = (here.daemon.clone(), units.clone());
            tokio::spawn(async move {
                let own = own_binary().unwrap();
                let old = || stale(units.installed().unwrap().as_ref(), &own, false);
                let restart = || units.try_restart();
                daemon.restart_when_old(old, restart).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!watch.is_finished());
        watch.abort();
        assert!(units.calls().is_empty());

        let none = Units::default();
        assert!(
            after_update(&none, &own, &here.dir)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(none.calls().is_empty());

        let (check, riff) = check_and_new(&here);
        let units = Units::of(&check);
        let away = here.root.path().join("no-daemon");
        let lines = after_update(&units, &riff, &away).await.unwrap();
        assert_eq!(
            lines,
            [text::daemon_unit_moved(Path::new(UNIT_FILE), &check, &riff)]
        );
        assert_eq!(units.calls(), ["rewrite"]);
        assert_eq!(units.exe(), Some(riff));
    }

    /// Two fake hosts, `a` and `b`, whose daemons serve the repository
    /// on one riff-server, and the lead on `a`.
    struct Hosts {
        api: riff_client::api::Api,
        lead: SessionUri,
        a: Here,
        b: Here,
    }

    const REPO: &str = "como-technologies/riff";

    async fn hosts() -> Hosts {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let server = format!("http://{addr}");
        let api = riff_client::api::Api::new(&server);
        let place = crate::identity::place_from_text(&format!("a/{REPO}")).unwrap();
        let user = crate::identity::person(&place, &server).unwrap();
        let lead = SessionUri::new(
            riff_core::name::Who::new(user.who().user(), Some("l1")).unwrap(),
            place,
        );
        api.register(&lead).await.unwrap();
        api.lead(&user, "l1").await.unwrap();
        let mut both = Vec::new();
        for host in ["a", "b"] {
            let here = Here::new(Fake::default().with_image(IMAGE)).await;
            let wave = crate::wave::Wave {
                repo: REPO.into(),
                main: here.root.path().to_owned(),
                server: server.clone(),
                host: host.into(),
            };
            let served = remote::Served {
                wave,
                session: format!("daemon-{host}"),
            };
            tokio::spawn(remote::serve(
                here.dir.clone(),
                served,
                here.daemon.news.clone(),
            ));
            both.push(here);
        }
        let b = both.pop().unwrap();
        let a = both.pop().unwrap();
        let hosts = Hosts { api, lead, a, b };
        for host in ["a", "b"] {
            until_async(|| async {
                let who = hosts.api.who(&hosts.lead, false).await.unwrap();
                remote::find(&who, hosts.lead.who().user(), REPO, host).is_some()
            })
            .await;
        }
        hosts
    }

    /// 01M5SNV75DRFHR26BT2M8243VV, 01M5SNV78EBAYY4SZ891DM2MQG: in a
    /// container, the lead holds a release and ends its hold through the
    /// socket of its session, and each session reads the holds and posts
    /// the note of a step. A work session cannot hold. The note and the
    /// hold wake the lead of the other repository.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_lead_holds_a_release_through_its_socket_and_a_work_session_cannot() {
        use crate::hold::Held;
        use crate::reach::Reach;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let server = format!("http://{addr}");
        let api = riff_client::api::Api::new(&server);
        let here = Here::new(Fake::default().with_image(IMAGE)).await;

        let mut sockets = Vec::new();
        let mut others = Vec::new();
        for (name, item, repo, user, terminal) in [
            ("riff-lead-o-r", "lead", format!("a/{REPO}"), "mike", true),
            (
                "riff-issue-1-aaaaaaaa",
                "issue-1",
                format!("a/{REPO}"),
                "mike",
                false,
            ),
            (
                "riff-lead-n-s",
                "lead",
                "b/n8behavior/sens-o-matic".into(),
                "nate",
                true,
            ),
        ] {
            let place = crate::identity::place_from_text(&repo).unwrap();
            let mut start = here.start(name, item);
            start.server = server.clone();
            start.me = SessionUri::new(
                riff_core::name::Who::new(user, Some(&start.session)).unwrap(),
                place.clone(),
            );
            start.terminal = terminal;
            api.register(&start.me).await.unwrap();
            if terminal {
                let person = SessionUri::new(riff_core::name::Who::new(user, None).unwrap(), place);
                api.lead(&person, &start.session).await.unwrap();
            }
            let me = start.me.clone();
            let (client, name) = here.started(start).await;
            drop(client);
            let reach = Reach::Daemon(session_dir(&here.dir, &name).join(SOCKET));
            if user == "nate" {
                others.push(me);
            } else {
                sockets.push(reach);
            }
        }
        let worker = sockets.pop().unwrap();
        let lead = sockets.pop().unwrap();
        let other = others.pop().unwrap();

        // A work session reads the holds and posts a note, and cannot hold.
        let refused = worker.release_hold("v3.3.0", Some("x")).await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_HOLD_ONLY_LEAD);
        let refused = worker.release_hold("v3.3.0", None).await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_HOLD_ONLY_LEAD);

        // The lead holds: the lead of the other repository is woken.
        let held = lead
            .release_hold("v3.3.0", Some("I migrate a database."))
            .await;
        assert_eq!(held.unwrap(), Held::Set { told: 1 });
        for reach in [&lead, &worker] {
            let lines = reach.release_lines().await.unwrap();
            let holds = crate::hold::holds(&lines, "v3.3.0");
            assert_eq!(holds.len(), 1, "{lines:?}");
            assert_eq!(holds[0].reason, "I migrate a database.");
            assert_eq!(holds[0].repo, REPO);
        }
        let heard = api
            .read(&other, &crate::hold::thread(), false)
            .await
            .unwrap();
        assert_eq!(heard.len(), 1);
        assert!(heard[0].message.body.contains("asks to wait for v3.3.0"));

        // A note of a step wakes the lead of the other repository, and not
        // the lead of the session that posts it.
        assert_eq!(lead.release_news("a note").await.unwrap(), 1);
        assert_eq!(worker.release_news("a note").await.unwrap(), 1);
        assert_eq!(
            api.read(&other, &crate::hold::thread(), false)
                .await
                .unwrap()
                .len(),
            2
        );

        // The lead ends its hold once.
        assert_eq!(
            lead.release_hold("v3.3.0", None).await.unwrap(),
            Held::Ended { told: 1 }
        );
        assert_eq!(
            lead.release_hold("v3.3.0", None).await.unwrap(),
            Held::NoHold
        );
        let lines = worker.release_lines().await.unwrap();
        assert!(crate::hold::holds(&lines, "v3.3.0").is_empty());
    }

    /// 01M5GVPNAKAPA596K9361HSAAV: a riff command in the container of the
    /// lead reaches riff-server through the socket of its session. The
    /// daemon makes the call on the riff-server of the session. A work
    /// session reads too, but cannot resume the riff.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_session_reaches_its_riff_server_through_its_socket() {
        use crate::reach::Reach;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let server = format!("http://{addr}");
        let api = riff_client::api::Api::new(&server);
        let place = crate::identity::place_from_text(&format!("a/{REPO}")).unwrap();
        let user = crate::identity::person(&place, &server).unwrap();
        let here = Here::new(Fake::default().with_image(IMAGE)).await;

        let mut sockets = Vec::new();
        for (name, item, terminal) in [
            ("riff-lead-o-r", "lead", true),
            ("riff-issue-1-aaaaaaaa", "issue-1", false),
        ] {
            let mut start = here.start(name, item);
            start.server = server.clone();
            start.me = SessionUri::new(
                riff_core::name::Who::new(user.who().user(), Some(&start.session)).unwrap(),
                place.clone(),
            );
            start.terminal = terminal;
            api.register(&start.me).await.unwrap();
            if terminal {
                // The person names the lead (01M5NJAXTQ0YKN7Q7XN0A7ZMPF).
                api.lead(&user, &start.session).await.unwrap();
            }
            let (client, name) = here.started(start).await;
            drop(client);
            sockets.push(Reach::Daemon(session_dir(&here.dir, &name).join(SOCKET)));
        }
        let worker = sockets.pop().unwrap();
        let lead = sockets.pop().unwrap();

        for reach in [&lead, &worker] {
            let served = reach.served().await;
            assert_eq!(served.server, Ok(env!("CARGO_PKG_VERSION").to_owned()));
            assert_eq!(served.hosts, Ok(vec![]));
            let who = reach.who(false).await.unwrap();
            assert!(
                who.iter()
                    .any(|s| s.uri.who().session() == Some("riff-lead-o-r-session")),
                "{who:?}"
            );
            let thread: riff_core::name::ThreadName = REPO.parse().unwrap();
            assert!(reach.holds(&thread).await.unwrap().is_empty());
        }
        let refused = worker.resume_riff().await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_RESUME_ONLY_LEAD);
        lead.resume_riff().await.unwrap();

        // #1338: the ends read through the socket, also for a work
        // session. Only the lead reads the log for an audit and runs an
        // item. A riff with no sign-in has no members to read: the daemon
        // gives the answer of riff-server (a signed riff is in
        // `the_lead_reads_the_members_and_the_log_of_a_riff_with_sign_in`).
        for reach in [&lead, &worker] {
            let seen = reach.whoami().await.unwrap();
            assert!(seen.state.is_ok(), "{:?}", seen.state);
            assert!(seen.uri.is_some(), "the server knows the session");
            let facts = reach.who_facts(false).await.unwrap();
            assert!(
                facts
                    .who
                    .sessions
                    .iter()
                    .any(|s| s.uri.who().session() == Some("riff-lead-o-r-session")),
                "{:?}",
                facts.who.sessions
            );
        }
        for reach in [&lead, &worker] {
            assert!(reach.ends().await.unwrap().is_empty());
            let no_sign_in = reach.members().await.unwrap_err().to_string();
            assert!(no_sign_in.contains("Nobody is signed in"), "{no_sign_in}");
        }
        let repo: riff_core::name::ThreadName = REPO.parse().unwrap();
        let refused = worker.log(&repo).await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_AUDIT_ONLY_LEAD);
        let refused = worker.run("issue-1").await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_RUN_ONLY_LEAD);
        let refused = lead.run("verify-issue-1").await.unwrap_err();
        assert_eq!(
            refused.to_string(),
            text::run_not_an_issue("verify-issue-1")
        );
        let refused = lead.run("issue-1").await.unwrap_err();
        assert_eq!(refused.to_string(), text::run_no_wave(REPO));

        // #1239: a --host request of the lead goes through its socket and
        // riff-server to the daemon of the host. A worker is refused.
        let request = remote::Request::Status;
        let no_daemon = lead.host("a", &request).await.unwrap_err().to_string();
        assert!(
            no_daemon.contains("no daemon of your user on a"),
            "{no_daemon}"
        );
        let host_me = SessionUri::new(
            riff_core::name::Who::new(user.who().user(), Some("host-daemon")).unwrap(),
            place.clone(),
        );
        api.register(&host_me).await.unwrap();
        // A watch makes the session live until the test ends.
        let _watch = api.watch(&host_me).await.unwrap();
        api.alive(&host_me).await.unwrap();
        let step = remote::status_line(1, "4.0.0", 0);
        api.status(&host_me, &riff_core::wire::Status { step })
            .await
            .unwrap();
        // Signals reach the state a moment later.
        for _ in 0..100 {
            let sessions = api.who(&host_me, false).await.unwrap();
            if remote::find(&sessions, user.who().user(), REPO, "a").is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let fake = tokio::spawn({
            let (api, me) = (api.clone(), host_me.clone());
            async move {
                loop {
                    let inbox = api.inbox(&me, None, false).await.unwrap();
                    for m in inbox.into_iter().flat_map(|i| i.messages) {
                        if m.message.body == "daemon status" {
                            let reply = Ok("Sessions: none.".to_owned());
                            let body = remote::reply_body(m.message.seq, "a", &reply);
                            let to = [riff_core::selector::Selector::session(
                                m.message.from.who().session().unwrap(),
                            )];
                            let kind = riff_core::wire::Kind::Note;
                            api.post(&me, None, &to, &body, kind).await.unwrap();
                            return;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        });
        let reply = lead.host("a", &request).await.unwrap();
        assert_eq!(reply, "Sessions: none.");
        fake.await.unwrap();
        let refused = worker.host("a", &request).await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_HOST_ONLY_LEAD);

        // `riff daemon status` in a container asks the daemon of the socket.
        let Reach::Daemon(socket) = &worker else {
            unreachable!()
        };
        let status = Reach::daemon_status(socket).await.unwrap();
        assert!(
            status.build.starts_with(env!("CARGO_PKG_VERSION")),
            "{}",
            status.build
        );

        // With no riff-server, the error names the server of the session.
        let dead = Reach::Direct("http://127.0.0.1:9".into());
        assert!(dead.build().await.is_err());
    }

    /// #1338: the lead reads the members and the records of the log
    /// through the socket of its session, from a riff with sign-in. The
    /// daemon makes the calls with the sign-in of the person on the
    /// host. A work session reads the members, and cannot read the log.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_lead_reads_the_members_and_the_log_of_a_riff_with_sign_in() {
        use crate::reach::Reach;
        use riff_server::Service;
        use riff_server::auth::Config;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        riff_client::secrets::mock_store();
        riff_client::login::logout(&server).unwrap();
        let service = Service::new(Config {
            provider: Some(riff_server::oidc::Provider {
                issuer: "https://accounts.example.com".into(),
                client_id: "riff".into(),
                client_secret: None,
                allowed_domains: Vec::new(),
            }),
            ..Config::new(&server)
        });
        let router = service.router();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        // The first person to sign in is the owner.
        let jkt = riff_client::device::key(&server).unwrap().thumbprint();
        let pair = service.admit("ada@gmail.com", false, &jkt).await.unwrap();
        riff_client::login::store(
            &server,
            &riff_client::login::SignIn {
                user: pair.user.clone(),
                access_token: pair.access_token.clone(),
                refresh_token: pair.refresh_token.clone(),
                expires_at: u64::MAX,
                riff_id: service.riff_id(),
            },
        )
        .unwrap();
        let api = riff_client::api::Api::new(&server).signed_in(None).unwrap();
        let place = crate::identity::place_from_text(&format!("a/{REPO}")).unwrap();
        let person = SessionUri::new(
            riff_core::name::Who::new(&pair.user, None).unwrap(),
            place.clone(),
        );
        let here = Here::new(Fake::default().with_image(IMAGE)).await;

        let mut sockets = Vec::new();
        for (name, item, terminal) in [
            ("riff-lead-o-r", "lead", true),
            ("riff-issue-1-aaaaaaaa", "issue-1", false),
        ] {
            let mut start = here.start(name, item);
            start.server = server.clone();
            let who = riff_core::name::Who::new(&pair.user, Some(&start.session)).unwrap();
            start.me = SessionUri::new(who, place.clone());
            start.terminal = terminal;
            let session = api.clone().signed_in(Some(&start.session)).unwrap();
            session
                .start(&start.me, riff_core::wire::StartReason::Process, !terminal)
                .await
                .unwrap();
            if terminal {
                api.lead(&person, &start.session).await.unwrap();
            }
            let (client, name) = here.started(start).await;
            drop(client);
            sockets.push(Reach::Daemon(session_dir(&here.dir, &name).join(SOCKET)));
        }
        let worker = sockets.pop().unwrap();
        let lead = sockets.pop().unwrap();

        for reach in [&lead, &worker] {
            let members = reach.members().await.unwrap();
            assert_eq!(members.owner.as_deref(), Some("ada@gmail.com"));
        }
        let repo: riff_core::name::ThreadName = REPO.parse().unwrap();
        let log = lead.log(&repo).await.unwrap();
        assert!(!log.records.is_empty());
        assert_eq!(log.pruned, None);
        let refused = worker.log(&repo).await.unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_AUDIT_ONLY_LEAD);
    }

    /// Waits until `done` is true.
    async fn until_async<F: std::future::Future<Output = bool>>(mut done: impl FnMut() -> F) {
        let start = std::time::Instant::now();
        while !done().await {
            assert!(start.elapsed() < Duration::from_secs(30), "timed out");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// 01M57D4FX5VHQR150QTDS6BA1C, 01M57D4G1TS8BHJF5A8PW2D79C: the lead
    /// on `a` follows a session of `b` up to its end. It gets the same
    /// lines as `riff daemon events` on `b`, and no secret.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_lead_follows_the_events_of_a_session_of_another_host_with_no_secret() {
        let h = hosts().await;
        let (client, name) =
            h.b.started(h.b.start("riff-issue-12-1a2b3c4d", "issue-12"))
                .await;
        drop(client);
        let socket = session_dir(&h.b.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[1]).await;

        let lines = Arc::new(Mutex::new(Vec::new()));
        let follow = {
            let (api, lead, name, lines) =
                (h.api.clone(), h.lead.clone(), name.clone(), lines.clone());
            tokio::spawn(async move {
                let show = |line: &str| lines.lock().unwrap().push(line.to_owned());
                remote::follow_events(&api, &lead, "b", &name, show).await
            })
        };
        // The lines of `riff daemon events` on b: with no secret.
        let scrub = Scrub::new(["gho_secret1".to_owned()]);
        let mut view = crate::run::View::default();
        let mut on_b = |line: &str| view.shown(&scrub.clean(line));
        let first: Vec<String> = TURN[..2].iter().flat_map(|l| on_b(l)).collect();
        until_async(|| async { lines.lock().unwrap().len() >= first.len() }).await;
        relay.event(3, TURN[2]).await;
        relay.exit(4, 0).await;
        let failed = follow.await.unwrap().unwrap();
        assert!(!failed);

        let mut want = first;
        want.extend(on_b(TURN[2]));
        want.push(text::daemon_session_ends(&name, Some(0)));
        let got = lines.lock().unwrap().clone();
        assert_eq!(got, want);
        assert!(got.iter().all(|l| !l.contains("gho_secret1")), "{got:?}");

        // The daemon of a owns no such session.
        let none = remote::follow_events(&h.api, &h.lead, "a", &name, |_| {}).await;
        assert_eq!(
            none.unwrap_err().to_string(),
            text::daemon_no_session(&name)
        );
    }

    /// 01M57D4FTP2NXAE18XFRXCJ79H, 01M57D4FZGCVE62AP4MVG59HM4: the status
    /// of the daemon of `b` comes to the lead in their direct thread.
    /// Another user does not read it.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_view_of_another_host_comes_in_a_direct_thread_that_no_other_user_reads() {
        let h = hosts().await;
        let (_client, name) =
            h.b.started(h.b.start("riff-issue-12-1a2b3c4d", "issue-12"))
                .await;
        let status = remote::state(&h.api, &h.lead, "b").await.unwrap();
        assert_eq!(status.dir, h.b.dir);
        assert!(status.sessions.iter().any(|s| s.name == name));
        let empty = remote::state(&h.api, &h.lead, "a").await.unwrap();
        assert_eq!(empty.dir, h.a.dir);
        assert!(empty.sessions.is_empty());

        let brett: SessionUri = format!("riff://brett@k/{REPO}?session=b1").parse().unwrap();
        h.api.register(&brett).await.unwrap();
        let threads = h.api.threads(&brett).await.unwrap();
        assert!(threads.iter().all(|t| !t.thread.is_direct()), "{threads:?}");
        let repo = brett.default_thread().unwrap();
        let read = h.api.read(&brett, &repo, true).await.unwrap_or_default();
        assert!(
            read.iter().all(|m| !m.message.body.contains(&name)),
            "{read:?}"
        );
        let mine = h.api.threads(&h.lead).await.unwrap();
        assert!(mine.iter().any(|t| t.thread.is_direct()), "{mine:?}");
    }

    /// 01M57D4FX5VHQR150QTDS6BA1C: with no new event, the page is empty
    /// after the hold, and starts after the same event.
    #[tokio::test]
    async fn a_page_with_no_new_event_ends_after_the_hold() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        let hold = Duration::from_millis(200);
        let page = remote::page(&here.dir, &name, (0, 0), hold).await.unwrap();
        assert_eq!(page.last, 1);
        assert!(!page.lines.is_empty());
        let empty = remote::page(&here.dir, &name, (1, 0), hold).await.unwrap();
        assert_eq!(
            empty,
            remote::Page {
                lines: Vec::new(),
                last: 1,
                notes: 0,
                end: None
            }
        );
    }

    /// 01M5SMP2FFEHJPZBJVGAWBXNNP (#1441): the lead asks a page of the
    /// events of a session on the socket of its own session, and gets the
    /// same page as `remote::page` gives. A worker gets a refusal.
    #[tokio::test]
    async fn the_lead_asks_a_page_of_events_through_its_socket_and_a_worker_is_refused() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let worker = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&worker, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[2]).await;
        let mut lead = here.start("riff-lead-o-r", "lead");
        lead.terminal = true;
        let (_lead_client, lead_name) = here.started(lead).await;
        let lead = session_dir(&here.dir, &lead_name).join(SOCKET);

        let call = ServerCall::Events {
            name: name.clone(),
            after: 0,
            notes: 0,
        };
        let reply = crate::session_relay::ask_server(&lead, &call)
            .await
            .unwrap();
        let page: remote::Page = reply.as_str().unwrap().parse().unwrap();
        let mut view = crate::run::View::default();
        let mut want = view.shown(TURN[0]);
        want.extend(view.shown(TURN[2]));
        assert_eq!((page.last, page.end), (2, None));
        assert_eq!(page.lines, want);

        let error = crate::session_relay::ask_server(&worker.join(SOCKET), &call)
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains(text::DAEMON_EVENTS_ONLY_LEAD),
            "{error:#}"
        );

        let next = ServerCall::Events {
            name: name.clone(),
            after: 2,
            notes: page.notes,
        };
        relay.exit(3, 0).await;
        let reply = crate::session_relay::ask_server(&lead, &next)
            .await
            .unwrap();
        let page: remote::Page = reply.as_str().unwrap().parse().unwrap();
        assert_eq!(page.end, Some(false), "{page:?}");
    }

    /// 01M57GCZXZZRPGGWC5HZS49DGM: a replay gives each line of the
    /// daemon after the event before it, and only the lines that came
    /// after the event `after`.
    #[test]
    fn a_replay_gives_each_line_of_the_daemon_in_its_place() {
        let log = "e1\ne2\ne3\n";
        let lines = [(1, "A".to_owned()), (2, "B".to_owned())];
        let shown = |after: u64| {
            let (out, mut said) = Out::keep();
            replay(&out, log, 3, after, &lines);
            drop(out);
            let mut shown = Vec::new();
            while let Ok(say) = said.try_recv() {
                shown.push(match say {
                    Say::Event { line, .. } => line,
                    Say::Line { text } => text,
                    other => panic!("{other:?}"),
                });
            }
            shown
        };
        assert_eq!(shown(0), ["e1", "A", "e2", "B", "e3"]);
        assert_eq!(shown(1), ["A", "e2", "B", "e3"]);
        assert_eq!(shown(2), ["B", "e3"]);
        assert_eq!(shown(3), Vec::<String>::new());
    }

    /// Waits until the daemon in the folder `dir` runs no session.
    async fn until_no_session(dir: &Path) {
        let start = std::time::Instant::now();
        while !status(dir).await.unwrap().sessions.is_empty() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// 01M57D4FX5VHQR150QTDS6BA1C, 01M57GCZVJAHAMPEYY1T6GA48P: the
    /// session ends between two pages, while no follower is on it. The
    /// next page gives the events after the page before and the end, not
    /// an error.
    #[tokio::test]
    async fn a_session_that_ends_between_two_pages_gives_the_end_on_the_next_page() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        let hold = Duration::from_millis(200);
        let first = remote::page(&here.dir, &name, (0, 0), hold).await.unwrap();
        assert_eq!((first.last, first.end), (1, None));

        relay.event(2, TURN[2]).await;
        relay.exit(3, 0).await;
        until_no_session(&here.dir).await;
        let next = remote::page(&here.dir, &name, (1, 0), hold).await.unwrap();
        let mut view = crate::run::View::default();
        view.shown(TURN[0]);
        let mut want = view.shown(TURN[2]);
        want.push(text::daemon_session_ends(&name, Some(0)));
        assert_eq!(
            next,
            remote::Page {
                lines: want,
                last: 2,
                notes: 0,
                end: Some(false)
            }
        );
    }

    /// 01M57GCZXZZRPGGWC5HZS49DGM: the daemon stops a quiet session while
    /// no follower is on it. A later follow gets the line of the stop in
    /// its place, and the page with the line shows it one time.
    #[tokio::test]
    async fn a_line_of_the_daemon_with_no_follower_comes_to_a_later_follow() {
        let quiet = Duration::from_millis(300);
        let here = Here::with_quiet(Fake::default().with_image(IMAGE), quiet).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.event(2, TURN[2]).await;
        until_no_session(&here.dir).await;
        let stop = text::daemon_quiet(&name, quiet);
        let ended = Ended {
            code: None,
            failed: true,
        };

        // The host: riff daemon events NAME.
        let mut follow = Client::connect(&here.dir).await.unwrap();
        follow
            .ask(Ask::Follow {
                name: name.clone(),
                after: 0,
            })
            .await
            .unwrap();
        let (said, end) = to_the_end(&mut follow).await;
        assert_eq!(events(&said).len(), 2);
        assert_eq!(said.last(), Some(&Say::Line { text: stop.clone() }));
        assert_eq!(end, Some(Say::Ended(ended)));

        // Another host: the line shows on the page after the event 2.
        let hold = Duration::from_millis(200);
        let page = remote::page(&here.dir, &name, (2, 0), hold).await.unwrap();
        let end_line = text::daemon_session_ends(&name, None);
        assert_eq!(page.lines, [stop, end_line.clone()]);
        assert_eq!((page.last, page.notes, page.end), (2, 1, Some(true)));
        let again = remote::page(&here.dir, &name, (2, 1), hold).await.unwrap();
        assert_eq!(again.lines, [end_line]);
    }

    /// 01M57GCZVJAHAMPEYY1T6GA48P: after the keep, the daemon owns no
    /// such session.
    #[tokio::test]
    async fn the_daemon_forgets_the_end_of_a_session_after_the_keep() {
        let keep = Duration::from_millis(300);
        let fake = Fake::default().with_image(IMAGE);
        let here = Here::with_keep_end(fake, (QUIET_LIMIT, keep), None).await;
        let (client, name) = here
            .started(here.start("riff-issue-12-1a2b3c4d", "issue-12"))
            .await;
        drop(client);
        let socket = session_dir(&here.dir, &name);
        let (mut relay, _) = Relay::connect(&socket, 1, false).await;
        relay.event(1, TURN[0]).await;
        relay.exit(2, 0).await;
        until_no_session(&here.dir).await;

        let hold = Duration::from_millis(100);
        let kept = remote::page(&here.dir, &name, (0, 0), hold).await.unwrap();
        assert_eq!(kept.end, Some(false));
        tokio::time::sleep(keep).await;
        let none = remote::page(&here.dir, &name, (0, 0), hold).await;
        assert_eq!(
            none.unwrap_err().to_string(),
            text::daemon_no_session(&name)
        );
    }

    /// A riff-server for the tests of the refusals of auto mode, and a
    /// reader of its thread `denials`.
    async fn denials_server() -> (String, riff_client::api::Api, SessionUri) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let server = format!("http://{addr}");
        let api = riff_client::api::Api::new(&server);
        let reader: SessionUri = "riff://ann@heron/acme/app?session=r1".parse().unwrap();
        api.register(&reader).await.unwrap();
        (server, api, reader)
    }

    /// Gives the denials of `kept` to the session `name` through its
    /// socket, as `riff relay denied --send` does, and gives the bodies
    /// of the thread `denials`: none when the thread is missing.
    async fn send_denials_of(
        here: &Here,
        name: &str,
        api: &riff_client::api::Api,
        reader: &SessionUri,
    ) -> Vec<String> {
        let queue_dir = here.root.path().join("claude-of-the-session");
        let queue = crate::denied::Queue::in_dir(&queue_dir);
        let input: crate::denied::Input = serde_json::from_value(serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": "GH_TOKEN=gho_secret1 git push origin main"},
            "reason": "[Git Push to Default Branch]",
        }))
        .unwrap();
        let scrub = Scrub::new(["gho_secret1".to_owned()]);
        queue
            .keep(&[crate::denied::Denial::of(&input, &scrub, false, 5)])
            .unwrap();
        let socket = session_dir(&here.dir, name).join(SOCKET);
        crate::session_relay::send_denials(&socket, &queue)
            .await
            .unwrap();
        assert!(queue.take().is_empty(), "the daemon posted each denial");
        api.read(reader, &crate::denied::thread(), true)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.message.body)
            .collect()
    }

    /// 01M57XY3Q6HEYQG27FE0CEQ7HB, 01M57XY3SEG8HCXPZFBVATQ1W2,
    /// 01M57XY405BSXEH3K3Z977RWAW: a refusal in a container session makes
    /// one record in the thread `denials`, with the claim of its item and
    /// no secret. The session is no member of the thread after it.
    #[tokio::test]
    async fn a_refusal_in_a_container_session_makes_one_record() {
        let (server, api, reader) = denials_server().await;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        start.server = server;
        api.register(&start.me).await.unwrap();
        let me = start.me.clone();
        let (client, name) = here.started(start).await;
        drop(client);

        let bodies = send_denials_of(&here, &name, &api, &reader).await;
        assert_eq!(
            bodies,
            [
                "denied reason=git-push-to-default-branch tool=Bash head=\"git push\" \
                 role=container user=mike host=thelio repo=como-technologies/riff \
                 session=riff-iss claim=issue-12: GH_TOKEN=[secret] git push origin main"
            ]
        );
        let threads = api.threads(&me).await.unwrap();
        let denials = threads
            .iter()
            .find(|t| t.thread.to_string() == crate::denied::THREAD);
        assert!(
            denials.is_none_or(|t| t.members.iter().all(|m| m.who() != me.who())),
            "{threads:?}"
        );
    }

    /// 01M57XY3Q6HEYQG27FE0CEQ7HB: a refusal in a container session that
    /// left the riff makes no record, and the relay keeps none.
    #[tokio::test]
    async fn a_refusal_in_a_container_session_that_left_makes_no_record() {
        let (server, api, reader) = denials_server().await;
        let marks = tempfile::tempdir().unwrap();
        let world: Arc<dyn World> = Arc::new(Server::default().with_marks(marks.path()));
        let fake = Fake::default().with_image(IMAGE);
        let here = Here::with_world(fake, QUIET_LIMIT, Some(world)).await;
        let mut start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        start.server = server;
        api.register(&start.me).await.unwrap();
        riff_client::marks::leave(marks.path(), &start.session).unwrap();
        let (client, name) = here.started(start).await;
        drop(client);

        let bodies = send_denials_of(&here, &name, &api, &reader).await;
        assert!(bodies.is_empty(), "{bodies:?}");
    }

    /// 01M57XY3Q6HEYQG27FE0CEQ7HB: a refusal in the lead makes one record
    /// with the role lead and no claim.
    #[tokio::test]
    async fn a_refusal_in_the_lead_makes_one_record() {
        let (server, api, reader) = denials_server().await;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        start.server = server;
        api.register(&start.me).await.unwrap();
        let (client, name) = here.started(start).await;
        drop(client);

        let bodies = send_denials_of(&here, &name, &api, &reader).await;
        assert_eq!(bodies.len(), 1, "{bodies:?}");
        let record = crate::denied::Record::parse(&bodies[0], 0).unwrap();
        assert_eq!(record.tag("role"), "lead");
        assert_eq!(record.tag("claim"), "-");
    }

    /// The block of the session `me` on riff-server, as `who` shows it.
    async fn block_of(api: &riff_client::api::Api, reader: &SessionUri, me: &SessionUri) -> bool {
        let who = api.roster(reader, false).await.unwrap();
        let row = who.sessions.iter().find(|s| s.uri.who() == me.who());
        row.expect("the session is in who").blocked.is_some()
    }

    /// 01M5GVQQ8NK3RD9CM3VTGXEHZQ, 01M48VDWPDYRPEAXHR1MYDN1M7: the
    /// prompt hook of the lead ends its block, and `who` no longer shows
    /// it as blocked.
    #[tokio::test]
    async fn a_prompt_in_the_lead_ends_its_block() {
        let (server, api, reader) = denials_server().await;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut start = here.start("riff-lead-o-r", "lead");
        start.terminal = true;
        start.server = server;
        api.register(&start.me).await.unwrap();
        let me = start.me.clone();
        let (client, name) = here.started(start).await;
        drop(client);
        api.blocked(&me, "run riff owner --take").await.unwrap();
        assert!(block_of(&api, &reader, &me).await, "the block holds");

        let socket = session_dir(&here.dir, &name).join(SOCKET);
        crate::session_relay::prompt(&socket).await.unwrap();
        assert!(!block_of(&api, &reader, &me).await, "the prompt ended it");
    }

    /// 01M5GVQQ8NK3RD9CM3VTGXEHZQ: a prompt in a worker is a wake of the
    /// daemon, and ends no block.
    #[tokio::test]
    async fn a_prompt_in_a_worker_ends_no_block() {
        let (server, api, reader) = denials_server().await;
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let mut start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        start.server = server;
        api.register(&start.me).await.unwrap();
        let me = start.me.clone();
        let (client, name) = here.started(start).await;
        drop(client);
        api.blocked(&me, "which of two designs?").await.unwrap();

        let socket = session_dir(&here.dir, &name).join(SOCKET);
        crate::session_relay::prompt(&socket).await.unwrap();
        assert!(block_of(&api, &reader, &me).await, "the block holds");
    }
    /// 01M5RGCW5NCKQYA61RM63914TX: a session registers a watch through
    /// its socket, for the repository of the session, and each session
    /// may (a watch only wakes the lead).
    #[tokio::test]
    async fn a_session_registers_a_watch_through_its_socket() {
        use crate::watches::{Op, Registered, Watches};
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let wave = Wave {
            repo: "como-technologies/riff".into(),
            main: here.root.path().to_owned(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(&here.dir, &BTreeMap::from([(wave.repo.clone(), wave)])).unwrap();
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (_client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name).join(SOCKET);

        let op = Op::Deploy {
            tag: "v1.2.3".into(),
            stale: None,
        };
        let call = ServerCall::Watch { op: op.clone() };
        let reply = crate::session_relay::ask_server(&socket, &call)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_value::<Registered>(reply).unwrap(),
            Registered::New
        );
        let reply = crate::session_relay::ask_server(&socket, &call)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_value::<Registered>(reply).unwrap(),
            Registered::Kept
        );

        let all = Watches::new(&here.dir).read().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            (all[0].repo.as_str(), &all[0].op),
            ("como-technologies/riff", &op)
        );
    }

    /// A session of a repository that the daemon does not serve gets the
    /// fault, and no watch registers.
    #[tokio::test]
    async fn a_watch_for_a_repository_that_the_daemon_does_not_serve_is_refused() {
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let start = here.start("riff-issue-12-1a2b3c4d", "issue-12");
        let (_client, name) = here.started(start).await;
        let socket = session_dir(&here.dir, &name).join(SOCKET);
        let call = ServerCall::Watch {
            op: crate::watches::Op::Wave,
        };
        let error = crate::session_relay::ask_server(&socket, &call)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("does not serve"), "{error:#}");
        assert!(
            crate::watches::Watches::new(&here.dir)
                .read()
                .unwrap()
                .is_empty()
        );
    }
    /// 01M5RGCW5NCKQYA61RM63914TX: the status of the daemon lists the
    /// watches of the operations, and not the watch of a wave: a wave has
    /// its line in `Waves:`.
    #[tokio::test]
    async fn the_status_lists_the_watches_but_not_the_watch_of_a_wave() {
        use crate::watches::{Op, Watches};
        let here = Here::new(Fake::default().with_image(IMAGE)).await;
        let wave = Wave {
            repo: "o/r".into(),
            main: here.root.path().to_owned(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(&here.dir, &BTreeMap::from([(wave.repo.clone(), wave)])).unwrap();
        assert!(here.daemon.status().await.watches.is_empty());
        let watches = Watches::new(&here.dir);
        watches.register("o/r", Op::Wave, 5).unwrap();
        let deploy = Op::Deploy {
            tag: "v1.2.3".into(),
            stale: None,
        };
        watches.register("o/r", deploy, 7).unwrap();
        let shown = here.daemon.status().await.watches;
        assert_eq!(
            shown,
            [WatchShown {
                repo: "o/r".into(),
                what: "the deploy of v1.2.3".into(),
                since_ms: 7
            }]
        );
        let text = text::daemon_status(&here.daemon.status().await, 60_007);
        assert!(
            text.contains("Watches:\n  o/r  the deploy of v1.2.3, for 1m\n"),
            "{text}"
        );
    }
}
