//! The screen tests of the Sessions tab: the mock-ups of #994 at 80 x 24
//! and at 40 x 24, the keys, the clicks and the actions.

use super::*;
use crate::daemon::Status;
use crate::tui::view::{compute_view, render};
use crate::tui::{Look, News, Tab};
use ratatui::buffer::Buffer;
use riff_core::wire::{BlockedInfo, Person, RiffBinary, RiffOwner};

/// The time of the screen tests.
const NOW: i64 = 1_000_000_000;

fn build(id: &str) -> Build {
    format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
}

/// A session of riff-server at `uri` in `state`.
fn info(uri: &str, state: SessionState) -> SessionInfo {
    let mut s: SessionInfo = serde_json::from_value(serde_json::json!({
        "uri": uri,
        "live": state != SessionState::Offline,
    }))
    .unwrap();
    s.state = Some(state);
    s
}

/// A container session of the daemon of this machine.
fn live(name: &str, session: &str, phase: Phase, last: &str) -> Live {
    Live {
        name: name.into(),
        session: session.into(),
        item: name
            .split('-')
            .skip(1)
            .take(2)
            .collect::<Vec<_>>()
            .join("-"),
        image: "riff-session".into(),
        digest: String::new(),
        since_ms: NOW - 300_000,
        phase,
        last: last.into(),
        lead: phase == Phase::Terminal,
    }
}

/// The status of a daemon with the container sessions `sessions`.
fn status(sessions: Vec<Live>) -> Status {
    let mut status: Status = serde_json::from_value(serde_json::json!({
        "pid": 4242, "build": "b", "since_ms": 0, "dir": "/d", "sessions": [],
    }))
    .unwrap();
    status.sessions = sessions;
    status
}

const RIFF: &str = "riff://mike@thelio/como-technologies/riff";

/// The sessions of the mock-up of #994: on thelio a blocked worker, a
/// busy container, a container whose turn ended, and a lead that runs an
/// old riff; a busy worker of mike on pangolin, and of brett on
/// kadomony.
fn sessions() -> Vec<SessionInfo> {
    let mut blocked = info(
        &format!("{RIFF}?session=3a3f8d5d&claim=issue-994"),
        SessionState::Blocked,
    );
    blocked.worker = true;
    blocked.claims_secs = 4320;
    blocked.blocked = Some(BlockedInfo {
        reason: "which of two designs?".into(),
        secs: 1920,
        answered: false,
        woken_again: true,
        unanswered: true,
    });
    let mut busy = info(
        &format!("{RIFF}?session=5b1e2a90&claim=issue-1590"),
        SessionState::Busy,
    );
    (busy.worker, busy.container, busy.claims_secs) = (true, true, 720);
    let mut done = info(
        &format!("{RIFF}?session=8f1c2d3e&claim=issue-1599"),
        SessionState::Waiting,
    );
    (done.worker, done.container, done.claims_secs) = (true, true, 300);
    done.waits = Some(Waits::Verify { pull: 1614 });
    let mut lead = info(
        &format!("{RIFF}?session=4e54d4e5&lead=true"),
        SessionState::Idle,
    );
    lead.claims_secs = 7200;
    lead.binary = Some(RiffBinary {
        build: build("10df8a4c0ffe").to_string(),
        replaced: true,
    });
    let mut pangolin = info(
        "riff://mike@pangolin/como-technologies/strata?session=9c0d1e2f&claim=issue-88",
        SessionState::Busy,
    );
    (pangolin.worker, pangolin.claims_secs) = (true, 600);
    let mut brett = info(
        "riff://brett@kadomony/como-technologies/strata?session=7a8b9c0d&claim=issue-12",
        SessionState::Busy,
    );
    (brett.worker, brett.claims_secs) = (true, 900);
    vec![blocked, busy, done, lead, pangolin, brett]
}

/// The state of the mock-up of the Sessions tab of #994: the riff of the
/// owner mike, the sessions of [`sessions`], the containers of the
/// daemon of thelio, the title of #994, two new lines in the chat and
/// news in the waves.
fn mock_up() -> State {
    let mut state = State::new(build("10df8a4c0ffe"), RIFF.parse().unwrap());
    let mut look = Look::default();
    look.who.owner = RiffOwner::Owner {
        user: "mike".into(),
        email: "m@x.io".into(),
    };
    look.who.people = vec![
        Person {
            user: "mike".into(),
            role: PersonRole::Owner,
            live: true,
            seen_secs: Some(0),
        },
        Person {
            user: "brett".into(),
            role: PersonRole::Member,
            live: true,
            seen_secs: Some(0),
        },
        Person {
            user: "ann".into(),
            role: PersonRole::Admin,
            live: false,
            seen_secs: Some(3600),
        },
    ];
    look.who.sessions = sessions();
    look.server = Some(build("10df8a4c0ffe"));
    state.looked(look, NOW - 2_000);
    state.daemon_looked(Ok(status(vec![
        live(
            "riff-issue-1590-5b1e2a90",
            "5b1e2a90",
            Phase::Works,
            "> Bash: just check",
        ),
        live(
            "riff-issue-1599-8f1c2d3e",
            "8f1c2d3e",
            Phase::Ends,
            "end: success, 8 turns",
        ),
    ])));
    let issues = crate::top::parse_issues(
        r#"[{"number": 994, "title": "Design: the TUI of riff", "milestone": null}]"#,
    )
    .unwrap();
    state.titled(vec![("como-technologies/riff".into(), Ok(issues))]);
    state.news(Tab::Chat, News::Lines(2));
    state.news(Tab::Waves, News::Dot);
    state
}

/// [`mock_up`] with the lead of riff in its container on thelio.
fn with_lead() -> State {
    let mut state = mock_up();
    let mut containers = match &state.daemon {
        Some(Ok(status)) => status.sessions.clone(),
        _ => Vec::new(),
    };
    containers.push(live(
        "riff-lead-como-technologies-riff",
        "4e54d4e5",
        Phase::Terminal,
        "",
    ));
    state.daemon_looked(Ok(status(containers)));
    state
}

/// The rows of `state` drawn into a `Buffer` of `width` x `height`,
/// with no spaces at their ends.
fn drawn(state: &State, width: u16, height: u16) -> Vec<String> {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render(&compute_view(state, area, NOW), &mut buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// `n` lines of `─`, for the rule over the detail.
fn rule(head: &str, width: usize) -> String {
    let used = Line::raw(head).width();
    format!("{head}{}", "─".repeat(width.saturating_sub(used)))
}

/// The mock-up of the Sessions tab of #994 at 80 x 24
/// (01M6VAZKNXZV1RTG7MT0GMHKT3). The rows of a group come by their
/// worst mark (01M6VAZKRTVZTY73PQV5GE2WY6), and the counts too.
#[test]
fn the_sessions_tab_at_80_x_24_is_the_mock_up() {
    let mut state = mock_up();
    state.note("The lead of riff runs on. (Ctrl-P Ctrl-Q detached.)");
    let mut want = vec![
        "[1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports".to_owned(),
        " riff running · owner mike · build 10df8a4 · link ok 2s".to_owned(),
        " 6 sessions: 1 blocked · 1 done · 3 busy · 1 idle · 1 to update       by person"
            .to_owned(),
        " ▼ mike  owner  online        5 sessions · 1 blocked · 1 done · 2 busy · 1 idle"
            .to_owned(),
        "   ▼ thelio › riff            4 sessions · 1 blocked · 1 done · 1 busy · 1 idle"
            .to_owned(),
        " >   ! 3a3f8d5d  #994   worker     blocked 32m  which of two designs?".to_owned(),
        "     ✓ 8f1c2d3e  #1599  container  done         waits for a verify of PR #1614".to_owned(),
        "     ● 5b1e2a90  #1590  container  busy 12m     Bash: just check".to_owned(),
        "     ○ 4e54d4e5  ↑      lead       idle 2h      monitoring work".to_owned(),
        "   ▶ pangolin › strata                                       1 session · 1 busy"
            .to_owned(),
        " ▶ brett  online  › kadomony › strata                        1 session · 1 busy"
            .to_owned(),
        " ▶ ann  admin  offline  seen 1h ago".to_owned(),
        rule("──3a3f8d5d · #994 Design: the TUI of riff · thelio ", 80),
        " which of two designs? (32m ago)".to_owned(),
        " the lead gave no answer".to_owned(),
        " working on #994 Design: the TUI of riff".to_owned(),
        " claim #994   pull request none   riff build current".to_owned(),
    ];
    want.resize(22, String::new());
    want.extend([
        " The lead of riff runs on. (Ctrl-P Ctrl-Q detached.)".to_owned(),
        " ↑↓ select  ←→ fold  Enter open  l lead  t take over  p pause  / filter  ? help"
            .to_owned(),
    ]);
    assert_eq!(drawn(&state, 80, 24), want);
}

/// The mock-up of the narrow layout of #994 at 40 x 24
/// (01M6VAZKK4KHCDCX4RKG9B95D7): one column of rows, fewer words, and no
/// detail pane.
#[test]
fn the_sessions_tab_at_40_x_24_is_the_narrow_mock_up() {
    let mut state = mock_up();
    state.note("The lead of riff runs on.");
    let mut want = vec![
        "‹ 1/6 Sessions ›",
        "riff running · link ok 2s",
        "1 blocked · 1 done · 3 busy · 1 idle",
        "▼ mike                       5 sessions",
        "  ▼ thelio › riff            4 sessions",
        "> !  3a3f  #994   blocked 32m",
        "  ✓  8f1c  #1599  done",
        "  ●  5b1e  #1590  busy 12m",
        "  ○  4e54  ↑      lead idle 2h",
        "  ▶ pangolin › strata         1 session",
        "▶ brett                       1 session",
        "▶ ann                           offline",
    ];
    want.resize(22, "");
    want.extend([
        "The lead of riff runs on.",
        "↑↓ select  Enter open  ? help  q quit",
    ]);
    assert_eq!(drawn(&state, 40, 24), want);
}

/// The detail of a container session of this machine names its
/// container, its phase and its last step, as the view of the daemon
/// did. The detail of the host of this machine has the line of the
/// daemon.
#[test]
fn the_detail_shows_the_container_and_the_daemon_of_this_machine() {
    let mut state = mock_up();
    state.on_key(Key::Down);
    state.on_key(Key::Down);
    let rows = drawn(&state, 80, 24);
    assert!(rows[7].starts_with(" >   ● 5b1e2a90"), "{rows:#?}");
    assert_eq!(
        rows[12],
        rule("──5b1e2a90 · #1590 · thelio · 5m ", 80),
        "{rows:#?}"
    );
    let text = rows.join("\n");
    assert!(
        text.contains("\n container riff-issue-1590-5b1e2a90 · works for 5m\n"),
        "{text}"
    );
    assert!(
        text.contains("\n last step: > Bash: just check\n"),
        "{text}"
    );

    // The host thelio: the line of the daemon of this machine.
    state.on_key(Key::First);
    state.on_key(Key::Down);
    let rows = drawn(&state, 80, 24);
    assert!(rows[4].starts_with(" > ▼ thelio › riff"), "{rows:#?}");
    let text = rows.join("\n");
    assert!(
        text.contains("\n riff daemon b, pid 4242, 0 waiting\n"),
        "{text}"
    );
}

/// The keys select a row, fold and open a group, and a `✓` goes when the
/// person selects its row.
#[test]
fn the_keys_select_fold_and_open() {
    let mut state = mock_up();
    // The first session is selected: the worst one.
    assert_eq!(
        state.sessions.selected, None,
        "nothing chosen yet: the first session shows selected"
    );
    state.on_key(Key::Down);
    // The `✓` goes once the person selects its row.
    let rows = drawn(&state, 80, 24);
    assert!(rows[6].starts_with(" >   ◐ 8f1c2d3e"), "{rows:#?}");
    // `←` on a session selects its group, `←` again folds it.
    state.on_key(Key::Left);
    state.on_key(Key::Left);
    let rows = drawn(&state, 80, 24);
    assert!(rows[4].starts_with(" > ▶ thelio › riff"), "{rows:#?}");
    assert!(rows[5].starts_with("   ▶ pangolin"), "{rows:#?}");
    // `→` opens it, Space folds it, Enter opens it.
    state.on_key(Key::Right);
    assert!(drawn(&state, 80, 24)[4].starts_with(" > ▼ thelio"));
    state.on_key(Key::Char(' '));
    assert!(drawn(&state, 80, 24)[4].starts_with(" > ▶ thelio"));
    state.on_key(Key::Enter);
    assert!(drawn(&state, 80, 24)[4].starts_with(" > ▼ thelio"));
    // `G` and `g`: the last and the first row.
    state.on_key(Key::Char('G'));
    assert!(drawn(&state, 80, 24)[11].starts_with(" ▶ ann"));
    state.on_key(Key::Char('g'));
    assert_eq!(state.sessions.at, 0);
    // `e` opens each group, and again folds the groups that need no
    // look.
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 30).join("\n");
    assert!(text.contains("\n   ▼ pangolin › strata"), "{text}");
    assert!(text.contains("     ● 9c0d1e2f  #88"), "{text}");
    assert!(text.contains("\n ▼ brett"), "{text}");
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 30).join("\n");
    assert!(text.contains("\n   ▶ pangolin › strata"), "{text}");
}

/// The help popup has the keys of the tab first, and it takes no key of
/// the tab: a key of the tab neither moves the selection nor asks.
#[test]
fn the_help_takes_no_key_of_the_tab() {
    let mut state = mock_up();
    state.on_key(Key::Char('?'));
    let text = drawn(&state, 80, 24).join("\n");
    assert!(text.contains("Keys of the Sessions tab"), "{text}");
    for (key, does) in help_keys() {
        assert!(text.contains(key) && text.contains(does), "{key}: {text}");
    }
    for key in [Key::Down, Key::Char('j'), Key::Char('p'), Key::Char('/')] {
        assert_eq!(state.on_key(key), None);
    }
    assert!(state.help_shown());
    assert_eq!(state.sessions.at, 0);
    assert_eq!(state.asked(), None);
    assert!(!state.sessions.typing);
    state.on_key(Key::Esc);
    assert!(!state.help_shown());
}

/// `b` puts the repository at the top of the tree, as `riff top --by
/// repo` does.
#[test]
fn b_puts_the_repository_at_the_top() {
    let mut state = mock_up();
    state.on_key(Key::Char('b'));
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].ends_with("by repo"), "{rows:#?}");
    assert!(
        rows[3].starts_with(" ▼ riff › mike  owner  online  › thelio  "),
        "{rows:#?}"
    );
    assert!(rows.iter().any(|r| r.starts_with(" ▶ strata")), "{rows:#?}");
    state.on_key(Key::Char('b'));
    assert!(drawn(&state, 80, 24)[2].ends_with("by person"));
}

/// `/` filters the rows by a word: a user, a host, a repository, an
/// item or a state. Enter keeps the filter, Esc clears it.
#[test]
fn the_filter_shows_the_rows_that_match() {
    let mut state = mock_up();
    state.on_key(Key::Char('/'));
    assert!(state.sessions.typing());
    for c in "strata".chars() {
        // A digit and `q` are letters of the filter while it is typed.
        state.on_key(Key::Char(c));
    }
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[22], " /strata█", "{rows:#?}");
    assert_eq!(rows[23], " Enter keep the filter  Esc clear it");
    state.on_key(Key::Enter);
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[2].starts_with(" 2 sessions: 2 busy") && rows[2].ends_with("/strata · by person"),
        "{rows:#?}"
    );
    // With a filter, each group is open.
    assert!(rows[3].starts_with(" ▼ brett"), "{rows:#?}");
    assert!(rows.iter().any(|r| r.contains("7a8b9c0d")), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.contains("thelio")), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.contains("ann")), "{rows:#?}");
    // `host:` matches only a host, and a state matches too.
    state.sessions.filter = "host:thelio blocked".into();
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].starts_with(" 1 session: 1 blocked"), "{rows:#?}");
    // Esc clears the filter.
    state.on_key(Key::Esc);
    assert_eq!(state.sessions.filter(), "");
    assert!(drawn(&state, 80, 24)[2].starts_with(" 6 sessions"));
    // `q` while the person types is a letter, and Ctrl-C ends the TUI.
    state.on_key(Key::Char('/'));
    assert_eq!(state.on_key(Key::Char('q')), None);
    assert_eq!(state.sessions.filter(), "q");
    assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
}

/// `l` attaches to the lead of the repository of the row, and Enter on
/// the lead too. The attach asks nothing.
#[test]
fn l_and_enter_attach_to_the_lead() {
    let mut state = mock_up();
    // No lead runs in a container of this machine.
    assert_eq!(state.on_key(Key::Char('l')), None);
    assert_eq!(state.shown_note(), Some(text::NO_LEAD_HERE));
    let mut state = with_lead();
    let lead = Action::Attach("riff-lead-como-technologies-riff".into());
    assert_eq!(state.on_key(Key::Char('l')), Some(lead.clone()));
    state.on_key(Key::Char('G'));
    // The row of ann has no repository: the first lead.
    assert_eq!(state.on_key(Key::Char('l')), Some(lead.clone()));
    state.on_key(Key::Char('g'));
    for _ in 0..5 {
        state.on_key(Key::Down);
    }
    assert!(drawn(&state, 80, 24)[8].starts_with(" >   ○ 4e54d4e5"));
    assert_eq!(state.on_key(Key::Enter), Some(lead));
    // `t` on the lead says that `l` attaches.
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::no_takeover_of_lead("riff-lead-como-technologies-riff").as_str())
    );
}

/// `t` and `x` ask `y` or `n` first, then take over or end the selected
/// container session of this machine.
#[test]
fn t_and_x_ask_first() {
    let mut state = mock_up();
    state.on_key(Key::Down);
    state.on_key(Key::Down);
    assert_eq!(state.on_key(Key::Char('t')), None);
    let name = "riff-issue-1590-5b1e2a90";
    let ask = format!("{} y/N", text::ask_takeover(name));
    assert_eq!(state.asked(), Some(ask.clone()));
    assert_eq!(drawn(&state, 80, 24)[22], format!(" {ask}"));
    assert_eq!(
        state.on_key(Key::Char('y')),
        Some(Action::Takeover(name.into()))
    );
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(state.asked(), Some(format!("{} y/N", text::ask_end(name))));
    // `n` changes nothing.
    assert_eq!(state.on_key(Key::Char('n')), None);
    assert_eq!(state.shown_note(), Some(crate::tui::NOTHING_CHANGED));
    state.on_key(Key::Char('x'));
    assert_eq!(state.on_key(Key::Char('y')), Some(Action::End(name.into())));
    // A session with no container of this machine.
    state.on_key(Key::Char('g'));
    state.on_key(Key::Down);
    state.on_key(Key::Down);
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(state.shown_note(), Some(text::NO_CONTAINER_HERE));
    // A session of another host: only there.
    state.on_key(Key::Char('e'));
    let rows = drawn(&state, 80, 30);
    let at = rows.iter().position(|r| r.contains("9c0d1e2f")).unwrap() - 3;
    state.on_key(Key::Row(at));
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::on_host_only("pangolin").as_str())
    );
    // A group row is no session.
    state.on_key(Key::Char('g'));
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(state.shown_note(), Some(text::PICK_A_SESSION));
}

/// `p` pauses the repository of the row, and `P` the whole riff, after
/// `y`, as in the Waves tab ([`Effect::Pause`]).
#[test]
fn p_and_capital_p_pause_after_a_question() {
    let mut state = mock_up();
    state.on_key(Key::Char('p'));
    assert_eq!(
        state.asked().as_deref(),
        Some("Pause como-technologies/riff? y/N")
    );
    assert_eq!(
        state.on_key(Key::Char('y')),
        Some(Action::Do(Effect::Pause {
            repo: Some("como-technologies/riff".into()),
            state: RiffState::Paused
        }))
    );
    state.on_key(Key::Char('P'));
    assert_eq!(state.asked().as_deref(), Some("Pause the whole riff? y/N"));
    assert_eq!(
        state.on_key(Key::Char('Y')),
        Some(Action::Do(Effect::Pause {
            repo: None,
            state: RiffState::Paused
        }))
    );
    // Another repository, and a pause that holds: a resume.
    let mut paused = mock_up();
    let (mut look, at) = paused.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    look.pauses.repositories = vec![riff_core::wire::RepositoryPause {
        repository: "como-technologies/strata".parse().unwrap(),
        pause: riff_core::wire::PauseInfo { by: None, at_ms: 0 },
    }];
    paused.looked(look, at);
    paused.on_key(Key::Char('G'));
    paused.on_key(Key::Up);
    paused.on_key(Key::Char('p'));
    assert_eq!(
        paused.asked().as_deref(),
        Some("Resume como-technologies/strata? y/N")
    );
    assert_eq!(
        paused.on_key(Key::Char('y')),
        Some(Action::Do(Effect::Pause {
            repo: Some("como-technologies/strata".into()),
            state: RiffState::Running
        }))
    );
    // A row of no one repository.
    paused.on_key(Key::Char('g'));
    paused.on_key(Key::Char('p'));
    assert_eq!(paused.shown_note(), Some(text::PICK_A_REPOSITORY));
}

/// The narrow layout: Enter on a session opens its detail on the whole
/// screen, and Esc goes back. A terminal under 24 rows has no pane.
#[test]
fn enter_opens_the_detail_on_the_whole_screen() {
    let mut state = mock_up();
    assert_eq!(state.on_key(Key::Enter), None);
    assert!(state.sessions.full());
    let rows = drawn(&state, 40, 24);
    assert_eq!(
        rows[2],
        rule("──3a3f8d5d · #994 Design: the TUI of riff · thelio ", 40)
            .chars()
            .take(40)
            .collect::<String>()
    );
    assert_eq!(rows[3], "which of two designs? (32m ago)", "{rows:#?}");
    assert_eq!(rows[23], "Esc back  l lead  t take over  ? help");
    state.on_key(Key::Esc);
    assert!(!state.sessions.full());
    // 80 x 20: the tree has the body, and no pane.
    let rows = drawn(&state, 80, 20);
    assert!(!rows.iter().any(|r| r.starts_with("──")), "{rows:#?}");
}

/// A click selects a row, a click on `▶` or `▼` folds or opens its
/// group, and a double click presses Enter (01M6VAZKG8FS4HEA9VSA4Z4YF1).
#[test]
fn a_click_selects_a_row_and_folds_a_group() {
    let mut state = mock_up();
    let area = Rect::new(0, 0, 80, 24);
    let view = compute_view(&state, area, NOW);
    // The row of 5b1e2a90 is the screen row 7.
    assert_eq!(view.hit(30, 7), Some(Key::Row(4)));
    state.on_click(&view, 30, 7, false);
    assert!(drawn(&state, 80, 24)[7].starts_with(" >   ● 5b1e2a90"));
    // The `▼` of thelio at the column 3 of the row 4.
    assert_eq!(view.hit(3, 4), Some(Key::Fold(1)));
    let view = compute_view(&state, area, NOW);
    state.on_click(&view, 3, 4, false);
    assert!(drawn(&state, 80, 24)[4].starts_with(" > ▶ thelio"));
    // A double click on the group opens it.
    let view = compute_view(&state, area, NOW);
    state.on_click(&view, 30, 4, true);
    assert!(drawn(&state, 80, 24)[4].starts_with(" > ▼ thelio"));
    // A click on a key of the key line.
    let view = compute_view(&state, area, NOW);
    let keys = " ↑↓ select  ←→ fold  Enter open  l lead  t take over  p pause  / filter  ? help";
    let at =
        |word: &str| u16::try_from(Line::raw(&keys[..keys.find(word).unwrap()]).width()).unwrap();
    assert_eq!(view.hit(at("/ filter"), 23), Some(Key::Char('/')));
    assert_eq!(view.hit(at("? help"), 23), Some(Key::Help));
    assert_eq!(view.hit(at("↑↓"), 23), None);
}

/// Many sessions: the window shows the rows that fit, keeps the selected
/// row in it, and the summary line names the place.
#[test]
fn a_long_tree_shows_its_window() {
    let mut state = mock_up();
    let (mut look, at) = state.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    for n in 0..40 {
        let mut s = info(
            &format!("{RIFF}?session=b{n:07}&claim=issue-{}", 2000 + n),
            SessionState::Blocked,
        );
        s.worker = true;
        look.who.sessions.push(s);
    }
    state.looked(look, at);
    // With no room for the top level, the summary line has the place.
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].ends_with("  1-9 of 49"), "{rows:#?}");
    let area = Rect::new(0, 0, 80, 24);
    for _ in 0..2 {
        state.drawn(&compute_view(&state, area, NOW));
        state.on_key(Key::PageDown);
    }
    state.drawn(&compute_view(&state, area, NOW));
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].ends_with("  13-21 of 49"), "{rows:#?}");
    assert_eq!(state.sessions.at, 20);
    assert!(rows.iter().any(|r| r.starts_with(" >   !")), "{rows:#?}");
}

/// 01M6VAZK1R932VQMJBC32MTTJB: a session that turns `!` or `✓` while
/// the person is in another tab gives the Sessions tab the news `•`.
#[test]
fn a_new_block_is_news_of_the_sessions_tab() {
    let mut state = mock_up();
    state.on_key(Key::Char('2'));
    let (mut look, at) = state.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    state.looked(look.clone(), at);
    assert_eq!(state.news_of(Tab::Sessions), None);
    look.who.sessions[4].state = Some(SessionState::Blocked);
    state.looked(look, at);
    assert_eq!(state.news_of(Tab::Sessions), Some(News::Dot));
    assert!(drawn(&state, 80, 24)[0].starts_with(" 1 Sessions•  [2 Chat]"));
    state.on_key(Key::Char('1'));
    assert_eq!(state.news_of(Tab::Sessions), None);
}

/// No riff daemon on this machine is no fault: the tab shows the
/// sessions of riff-server, and the note says so.
#[test]
fn no_daemon_here_is_a_note() {
    let mut state = mock_up();
    state.daemon_looked(Err("no socket".into()));
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[22], format!(" {}", text::NO_DAEMON_HERE));
    // The containers are gone from the rows: no `✓`, and the server
    // states.
    assert!(
        rows[2].starts_with(" 6 sessions: 1 blocked · 1 waiting · 3 busy · 1 idle"),
        "{rows:#?}"
    );
    state.note("a note");
    assert_eq!(drawn(&state, 80, 24)[22], " a note");
}

/// 01M56Y8KD05JH2J7AMK636WGT7: the tab draws one sessions model: a
/// container session that is on riff-server too is one row, and `riff
/// top --once` draws the same model with no container.
#[test]
fn the_tab_and_riff_top_draw_one_sessions_model() {
    let state = mock_up();
    let ids: Vec<Option<&str>> = state.model.entries.iter().map(Entry::id).collect();
    assert_eq!(ids.len(), 6, "{ids:?}");
    assert_eq!(state.model.entries[1].phase(), Some(Phase::Works));
    let rows = drawn(&state, 80, 24);
    for id in ["5b1e2a90", "8f1c2d3e"] {
        assert_eq!(
            rows.iter().filter(|r| r.contains(id)).count(),
            1,
            "{rows:#?}"
        );
    }
    // A container with no session on riff-server shows under the person
    // and the host of the TUI, with no repository.
    let mut state = mock_up();
    state.daemon_looked(Ok(status(vec![live(
        "riff-issue-77-cccc3333",
        "cccc3333",
        Phase::Holds,
        "",
    )])));
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 40).join("\n");
    assert!(text.contains("\n   ▼ thelio  "), "{text}");
    assert!(text.contains("\n     ▼ -"), "{text}");
    assert!(
        text.contains("         cccc3333  #77    container  holds for a takeover"),
        "{text}"
    );
    let top = crate::top::Top {
        pauses: &RiffState::Running.into(),
        owner: &RiffOwner::NoSignIn,
        server: None,
        sessions: &state.model,
        people: &[],
        issues: &BTreeMap::new(),
        holds: &BTreeMap::new(),
        repo: None,
        show: &riff_client::sessions::Show::default(),
        width: 100,
        fault: None,
        machines: &[],
    };
    let text = anstream::adapter::strip_str(&top.view()).to_string();
    assert!(!text.contains("cccc3333"), "{text}");
    assert_eq!(text.matches("5b1e2a90").count(), 1, "{text}");
}

/// The screen of the Sessions tab in "See what each session does" of
/// the book is the screen that the tab draws at 80 x 24, with no empty
/// row (01M6VGGW73KKG6Q0TZNBGYN7E0).
#[test]
fn the_book_shows_the_screen_of_the_tab() {
    let page = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/how-it-works.md"
    ))
    .unwrap();
    let part = page
        .split("\n## See what each session does\n")
        .nth(1)
        .unwrap();
    let screen = part
        .split("```text\n")
        .find(|block| block.starts_with("[1 Sessions]"))
        .unwrap();
    let screen: Vec<&str> = screen.split("```").next().unwrap().lines().collect();
    let drawn = drawn(&mock_up(), 80, 24);
    let drawn: Vec<&str> = drawn
        .iter()
        .map(String::as_str)
        .filter(|row| !row.is_empty())
        .collect();
    assert_eq!(screen, drawn);
}
