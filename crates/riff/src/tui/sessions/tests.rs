//! The screen tests of the Sessions tab: the mock-ups of #994 at 80 x 24
//! and at 40 x 24, the keys, the clicks and the actions.

use super::*;
use crate::daemon::Status;
use crate::tui::view::{compute_view, render};
use crate::tui::{Look, News, Tab};
use ratatui::buffer::Buffer;
use riff_core::wire::{BlockedInfo, Person, RiffBinary};

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
/// admin mike, the sessions of [`sessions`], the containers of the
/// daemon of thelio, the title of #994, two new lines in the chat and
/// news in the waves.
fn mock_up() -> State {
    let mut state = State::new(build("10df8a4c0ffe"), RIFF.parse().unwrap());
    let mut look = Look::default();
    look.who.people = vec![
        Person {
            user: "mike".into(),
            role: PersonRole::Admin,
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
/// (01M6VAZKNXZV1RTG7MT0GMHKT3), in the tree of #1735: repository ›
/// person › session, with the host in each session row. The rows of a
/// group come by their worst mark (01M6VAZKRTVZTY73PQV5GE2WY6), and the
/// counts too. The folded row names its people and its hosts.
#[test]
fn the_sessions_tab_at_80_x_24_is_the_mock_up() {
    let mut state = mock_up();
    state.note("The lead of riff runs on. (Ctrl-P Ctrl-Q detached.)");
    let mut want = vec![
        "[1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports".to_owned(),
        " riff running · build 10df8a4 · link ok 2s".to_owned(),
        " 6 sessions: 1 blocked · 1 done · 3 busy · 1 idle · 1 to update         by repo"
            .to_owned(),
        " ▼ riff  lead thelio  › mike  admin  online                          4 sessions"
            .to_owned(),
        " > ! 3a3f8d5d  #994   worker     thelio   blocked 32m  which of two designs?".to_owned(),
        "   ✓ 8f1c2d3e  #1599  container  thelio   done         waits for a verify of PR"
            .to_owned(),
        "   ● 5b1e2a90  #1590  container  thelio   busy 12m     Bash: just check".to_owned(),
        "   ○ 4e54d4e5  ↑      lead       thelio   idle 2h      monitoring work".to_owned(),
        " ▶ strata · brett, mike · kadomony, pangolin                2 sessions · 2 busy"
            .to_owned(),
    ];
    want.resize(12, String::new());
    want.extend([
        rule("──3a3f8d5d · #994 Design: the TUI of riff · thelio ", 80),
        " which of two designs? (32m ago)".to_owned(),
        " the lead gave no answer".to_owned(),
        " working on #994 Design: the TUI of riff".to_owned(),
        " claim #994   pull request none   riff build current".to_owned(),
    ]);
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
/// detail pane. Under 50 columns, a session row has no host, and a
/// folded row with no room for its names has none.
#[test]
fn the_sessions_tab_at_40_x_24_is_the_narrow_mock_up() {
    let mut state = mock_up();
    state.note("The lead of riff runs on.");
    let mut want = vec![
        "‹ 1/6 Sessions ›",
        "riff running · link ok 2s",
        "1 blocked · 1 done · 3 busy · 1 idle",
        "▼ riff › mike                4 sessions",
        "> !  3a3f  #994   blocked 32m",
        "  ✓  8f1c  #1599  done",
        "  ●  5b1e  #1590  busy 12m",
        "  ○  4e54  ↑      lead idle 2h",
        "▶ strata                     2 sessions",
    ];
    want.resize(22, "");
    want.extend([
        "The lead of riff runs on.",
        "↑↓ select  Enter open  ? help  q quit",
    ]);
    assert_eq!(drawn(&state, 40, 24), want);
}

/// From 50 columns, the narrow layout has the host in each session row,
/// up to 8 columns, and a folded row names what it holds when it fits.
#[test]
fn the_narrow_layout_from_50_columns_has_the_host() {
    let state = mock_up();
    let rows = drawn(&state, 60, 24);
    assert_eq!(
        rows[3..9],
        [
            "▼ riff › mike                                    4 sessions",
            "> !  3a3f  #994   thelio   blocked 32m",
            "  ✓  8f1c  #1599  thelio   done",
            "  ●  5b1e  #1590  thelio   busy 12m",
            "  ○  4e54  ↑      thelio   lead idle 2h",
            "▶ strata · brett, mike · kadomony, pangolin      2 sessions",
        ]
    );
    assert!(drawn(&state, 49, 24)[4].starts_with("> !  3a3f  #994   blocked"));
    // A host of more than 8 columns is cut.
    let mut state = mock_up();
    let (mut look, at) = state.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    let mut far = info(
        "riff://mike@strata-ci-runner/como-technologies/riff?session=fafafafa&claim=issue-5",
        SessionState::Blocked,
    );
    far.worker = true;
    look.who.sessions.push(far);
    state.looked(look, at);
    let rows = drawn(&state, 60, 24);
    assert!(
        rows.iter()
            .any(|r| r.starts_with("  !  fafa  #5     strata-… blocked")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|r| r.contains("#994   thelio   blocked")),
        "{rows:#?}"
    );
}

/// `b` gives person › repository › session, with the host in each
/// session row, at 80 x 24 and at 40 x 24. A person with no session
/// shows, and a folded row names its repository and its host.
#[test]
fn b_puts_the_person_at_the_top() {
    let mut state = mock_up();
    state.on_key(Key::Char('b'));
    assert_eq!(state.sessions.by(), By::Person);
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[2..12],
        [
            " 6 sessions: 1 blocked · 1 done · 3 busy · 1 idle · 1 to update       by person",
            " ▼ mike  admin  online        5 sessions · 1 blocked · 1 done · 2 busy · 1 idle",
            "   ▼ riff  lead thelio        4 sessions · 1 blocked · 1 done · 1 busy · 1 idle",
            " >   ! 3a3f8d5d  #994   worker     thelio   blocked 32m  which of two designs?",
            "     ✓ 8f1c2d3e  #1599  container  thelio   done         waits for a verify of P",
            "     ● 5b1e2a90  #1590  container  thelio   busy 12m     Bash: just check",
            "     ○ 4e54d4e5  ↑      lead       thelio   idle 2h      monitoring work",
            "   ▶ strata · pangolin                                       1 session · 1 busy",
            " ▶ brett  online  › strata · kadomony                        1 session · 1 busy",
            " ▶ ann  admin  offline  seen 1h ago",
        ]
    );
    assert_eq!(
        book("### Put the people at the top of the tab", " 6 sessions"),
        rows[2..12]
    );
    let rows = drawn(&state, 40, 24);
    assert_eq!(
        rows[2..12],
        [
            "1 blocked · 1 done · 3 busy · 1 idle",
            "▼ mike                       5 sessions",
            "  ▼ riff                     4 sessions",
            "> !  3a3f  #994   blocked 32m",
            "  ✓  8f1c  #1599  done",
            "  ●  5b1e  #1590  busy 12m",
            "  ○  4e54  ↑      lead idle 2h",
            "  ▶ strata · pangolin         1 session",
            "▶ brett › strata · kadomony   1 session",
            "▶ ann                           offline",
        ]
    );
    // `b` again: the repository at the top.
    state.on_key(Key::Char('b'));
    assert!(drawn(&state, 80, 24)[2].ends_with("by repo"));
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▼ riff  lead thelio  › mike"));
}

/// 01M6VAZKNXZV1RTG7MT0GMHKT3: the filter `host:pangolin` at 80 x 24
/// and at 40 x 24. The summary line starts with the filter, and the key
/// line has `Esc`.
#[test]
fn an_active_filter_is_plain_to_see() {
    let mut state = mock_up();
    state.sessions.set_filter("host:pangolin".into());
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[2..5],
        [
            " filter host:pangolin · 1 of 6 sessions: 1 busy · Esc clears            by repo",
            " ▼ strata › mike  admin  online                              1 session · 1 busy",
            " > ● 9c0d1e2f  #88    worker     pangolin busy 10m",
        ]
    );
    assert_eq!(
        rows[23],
        " ↑↓ select  Enter open  l lead  t take over  Esc clear the filter  ? help"
    );
    let mut shown = rows[2..5].to_vec();
    shown.push(rows[23].clone());
    assert_eq!(book("### Filter the sessions", " filter"), shown);
    let rows = drawn(&state, 40, 24);
    assert_eq!(rows[2], "filter host:pangolin · 1 of 6 sessions");
    assert_eq!(rows[23], "Enter open  Esc clear the filter  ? help");
    // A click on `Esc clear the filter` presses Esc.
    let view = compute_view(&state, Rect::new(0, 0, 40, 24), NOW);
    assert_eq!(view.hit(12, 23), Some(Key::Esc));
    state.on_click(&view, 12, 23, false);
    assert!(!state.sessions.filtered());
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].starts_with(" 6 sessions: 1 blocked"), "{rows:#?}");
    assert!(rows[23].contains("/ filter"), "{rows:#?}");
    // A filter of spaces is no filter.
    state.sessions.set_filter("  ".into());
    assert!(drawn(&state, 80, 24)[2].starts_with(" 6 sessions: 1 blocked"));
}

/// The detail of a container session of this machine names its host,
/// its container, its phase and its last step, as the view of the
/// daemon did. The detail of a group has its counts.
#[test]
fn the_detail_shows_the_container_of_this_machine() {
    let mut state = mock_up();
    state.on_key(Key::Down);
    state.on_key(Key::Down);
    let rows = drawn(&state, 80, 24);
    assert!(rows[6].starts_with(" > ● 5b1e2a90"), "{rows:#?}");
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

    // The group riff › mike: its counts, and the lead of riff with its
    // host (01M77XYN04KX35QZ06NDPSXHRP).
    state.on_key(Key::First);
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[3].starts_with(" ▼ riff  lead thelio  › mike"),
        "{rows:#?}"
    );
    assert_eq!(
        rows[12..15],
        [
            rule("──riff · lead thelio › mike · admin · online ", 80),
            " 4 sessions · 1 blocked · 1 waiting · 1 busy · 1 idle".to_owned(),
            " The lead of como-technologies/riff runs on thelio: mike@thelio:riff (4e54d4e5)."
                .to_owned(),
        ]
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
    assert!(rows[5].starts_with(" > ◐ 8f1c2d3e"), "{rows:#?}");
    // `←` on a session selects its group, `←` again folds it. The
    // folded row names its host.
    state.on_key(Key::Left);
    state.on_key(Key::Left);
    assert_eq!(
        state.sessions.selected,
        Some(Selected::Group(vec![
            "como-technologies/riff".into(),
            "mike".into()
        ]))
    );
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[3].starts_with(" ▶ riff  lead thelio  › mike  admin  online · thelio "),
        "{rows:#?}"
    );
    assert!(rows[4].starts_with(" ▶ strata"), "{rows:#?}");
    // `→` opens it, Space folds it, Enter opens it.
    state.on_key(Key::Right);
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▼ riff"));
    state.on_key(Key::Char(' '));
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▶ riff"));
    state.on_key(Key::Enter);
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▼ riff"));
    // `G` and `g`: the last and the first row.
    state.on_key(Key::Char('G'));
    assert!(drawn(&state, 80, 24)[8].starts_with(" ▶ strata"));
    state.on_key(Key::Char('g'));
    assert_eq!(state.sessions.at, 0);
    // `e` opens each group, and again folds the groups that need no
    // look.
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 30).join("\n");
    assert!(text.contains("\n ▼ strata  "), "{text}");
    assert!(text.contains("\n   ▼ brett  online  "), "{text}");
    assert!(
        text.contains("\n     ● 9c0d1e2f  #88    worker     pangolin busy 10m"),
        "{text}"
    );
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 30).join("\n");
    assert!(text.contains("\n ▶ strata · brett, mike"), "{text}");
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
        rows[2].starts_with(" filter strata · 2 of 6 sessions: 2 busy · Esc clears")
            && rows[2].ends_with(" by repo"),
        "{rows:#?}"
    );
    // With a filter, each group is open.
    assert!(rows[3].starts_with(" ▼ strata"), "{rows:#?}");
    assert!(rows[4].starts_with("   ▼ brett"), "{rows:#?}");
    assert!(rows.iter().any(|r| r.contains("7a8b9c0d")), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.contains("thelio")), "{rows:#?}");
    // `host:` matches only a host, and a state matches too.
    state.sessions.filter = "host:thelio blocked".into();
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[2].starts_with(" filter host:thelio blocked · 1 of 6 sessions: 1 blocked"),
        "{rows:#?}"
    );
    // `user:` matches only a person, and `repo:` only a repository.
    state.sessions.filter = "user:brett".into();
    assert!(drawn(&state, 80, 24)[2].starts_with(" filter user:brett · 1 of 6 sessions"));
    state.sessions.filter = "repo:riff".into();
    assert!(drawn(&state, 80, 24)[2].starts_with(" filter repo:riff · 4 of 6 sessions"));
    // With the person at the top, a person with no session that the
    // filter matches shows.
    state.on_key(Key::Char('b'));
    state.sessions.filter = "ann".into();
    let rows = drawn(&state, 80, 24);
    assert!(rows[3].starts_with(" ▶ ann  admin"), "{rows:#?}");
    state.on_key(Key::Char('b'));
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
    // The repository strata has no lead here: the first lead.
    assert_eq!(state.on_key(Key::Char('l')), Some(lead.clone()));
    state.on_key(Key::Char('g'));
    for _ in 0..4 {
        state.on_key(Key::Down);
    }
    assert!(drawn(&state, 80, 24)[7].starts_with(" > ○ 4e54d4e5"));
    assert_eq!(state.on_key(Key::Enter), Some(lead));
    // `t` on the lead says that `l` attaches.
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::no_takeover_of_lead("riff-lead-como-technologies-riff").as_str())
    );
}

/// [`mock_up`] after the end of the lead of riff: riff-server names riff
/// with no live lead, and the riff daemon of thelio runs its wave, also
/// in the status of the daemon of this machine.
fn with_no_lead() -> State {
    let mut state = mock_up();
    let (look, at) = state.last_look().unwrap();
    let mut look = look.clone();
    look.pauses.no_lead = vec!["como-technologies/riff".parse().unwrap()];
    look.who.sessions.retain(|s| !s.uri.lead());
    let facts = riff_client::daemons::Facts {
        waves: vec![riff_client::daemons::Wave {
            repo: "como-technologies/riff".into(),
            word: text::wave_no_lead("como-technologies/riff"),
            image: None,
            waits: Default::default(),
        }],
        ..riff_client::daemons::Facts::default()
    };
    let mut daemon = info(&format!("{RIFF}?session=d1"), SessionState::Idle);
    daemon.machine = Some(riff_core::wire::MachineInfo {
        numbers: facts.line(),
        age_secs: 5,
    });
    look.who.sessions.push(daemon);
    state.looked(look, at);
    let mut daemon = match &state.daemon {
        Some(Ok(status)) => status.clone(),
        _ => status(Vec::new()),
    };
    daemon.waves = vec![crate::daemon::WaveShown {
        repo: "como-technologies/riff".into(),
        word: text::wave_no_lead("como-technologies/riff"),
        waits: Default::default(),
    }];
    state.daemon_looked(Ok(daemon));
    state
}

/// The rows of the detail pane of `rows` at 80 x 24, after its rule, as
/// one text: a long line of the detail wraps between two words.
fn pane_text(rows: &[String]) -> String {
    rows[13..22]
        .iter()
        .map(|row| row.trim())
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 01M77XYN04KX35QZ06NDPSXHRP: the group of a repository whose wave waits
/// for a lead says so, and its detail says why and gives the step: the
/// riff daemon of a host that joined the project starts the lead. `l`
/// starts no lead, also on the host that runs the wave: on each row of
/// the repository, the note gives the step. The narrow layout says the
/// wait in the same words.
#[test]
fn the_group_with_no_lead_says_why_and_l_gives_the_step() {
    let mut state = with_no_lead();
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[3].starts_with(" ▼ riff  waits for a lead  › mike  admin  online"),
        "{rows:#?}"
    );
    // The detail of the group.
    state.on_key(Key::Char('g'));
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[12].starts_with("──riff · waits for a lead › mike"),
        "{rows:#?}"
    );
    let step = text::group_no_lead("como-technologies/riff");
    assert!(pane_text(&rows).contains(&step), "{rows:#?}");
    assert!(
        rows[14].starts_with(" The wave of como-technologies/riff waits for a lead."),
        "{rows:#?}"
    );
    // `l` asks nothing and starts nothing: the note gives the step.
    assert_eq!(state.on_key(Key::Char('l')), None);
    assert_eq!(state.asked(), None);
    assert_eq!(state.shown_note(), Some(step.as_str()));
    // A session row of the repository gives the same note.
    state.note("");
    state.on_key(Key::Down);
    assert_eq!(state.on_key(Key::Char('l')), None);
    assert_eq!(state.asked(), None);
    assert_eq!(state.shown_note(), Some(step.as_str()));
    state.on_key(Key::Esc);
    // The narrow layout.
    assert!(
        drawn(&state, 40, 24)[3].starts_with("▼ riff  waits for a lead"),
        "{:#?}",
        drawn(&state, 40, 24)
    );
}

/// [`mock_up`] with the lead of riff on pangolin, not on thelio, the host
/// of the TUI.
fn with_lead_on_pangolin() -> State {
    let mut state = mock_up();
    let (look, at) = state.last_look().unwrap();
    let mut look = look.clone();
    let lead = look.who.sessions.iter_mut().find(|s| s.uri.lead()).unwrap();
    lead.uri = "riff://mike@pangolin/como-technologies/riff?session=4e54d4e5&lead=true"
        .parse()
        .unwrap();
    state.looked(look, at);
    state
}

/// The screen test of a project whose lead runs on another host
/// (01M77XYN04KX35QZ06NDPSXHRP): the group of the repository names the
/// host of its lead, muted, and its detail names the lead. `l` on the
/// group, on a worker of the repository and on the lead gives the note
/// that names that host and the step there. The narrow layout has no host
/// in the group.
#[test]
fn the_group_names_the_host_of_its_lead_and_l_names_the_step_there() {
    let mut state = with_lead_on_pangolin();
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[3],
        " ▼ riff  lead pangolin  › mike  admin  online                        4 sessions"
    );
    assert!(
        rows[7].starts_with("   ○ 4e54d4e5  ↑      lead       pangolin idle 2h"),
        "{rows:#?}"
    );
    // The host of the lead is muted.
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    render(&compute_view(&state, area, NOW), &mut buffer);
    let at = rows[3].find("lead pangolin").unwrap();
    let x = u16::try_from(rows[3][..at].chars().count()).unwrap();
    assert_eq!(Some(buffer[(x, 3)].fg), muted().fg);

    // The detail of the group names the lead and its host.
    state.on_key(Key::Char('g'));
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[12].starts_with("──riff · lead pangolin › mike · admin · online "),
        "{rows:#?}"
    );
    let line = text::group_lead(
        "como-technologies/riff",
        "pangolin",
        "mike@pangolin:riff (4e54d4e5)",
    );
    assert!(pane_text(&rows).contains(&line), "{rows:#?}");
    assert!(
        rows[14].starts_with(" The lead of como-technologies/riff runs on pangolin:"),
        "{rows:#?}"
    );
    // The book shows the group row and its detail.
    let shown = [&rows[3], &rows[12], &rows[13], &rows[14], &rows[15]].map(String::clone);
    let heading = "### See where the lead of a project runs";
    assert_eq!(book(heading, " ▼ riff"), shown, "{rows:#?}");

    // `l` on the group: the note names pangolin and the step there.
    let there = text::lead_there("pangolin", Some("como-technologies/riff"));
    assert!(there.starts_with("The lead runs on pangolin: ssh pangolin"));
    assert_eq!(state.on_key(Key::Char('l')), None);
    assert_eq!(state.asked(), None);
    assert_eq!(state.shown_note(), Some(there.as_str()));
    assert!(
        drawn(&state, 80, 24)[22].starts_with(" The lead runs on pangolin: ssh pangolin"),
        "{:#?}",
        drawn(&state, 80, 24)
    );
    // On a worker of the repository, and on the lead.
    for id in ["3a3f8d5d", "4e54d4e5"] {
        state.note("");
        state.on_key(Key::Row(row_of(&state, id)));
        assert_eq!(state.on_key(Key::Char('l')), None, "{id}");
        assert_eq!(state.shown_note(), Some(there.as_str()), "{id}");
    }
    // The narrow layout has no host in the group.
    assert!(
        drawn(&state, 40, 24)[3].starts_with("▼ riff › mike "),
        "{:#?}",
        drawn(&state, 40, 24)
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
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(state.shown_note(), Some(text::NO_CONTAINER_HERE));
    // A session of another host: the step there. No daemon of pangolin
    // names its container.
    state.on_key(Key::Char('e'));
    let rows = drawn(&state, 80, 30);
    let at = rows.iter().position(|r| r.contains("9c0d1e2f")).unwrap() - 3;
    state.on_key(Key::Row(at));
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::takeover_there("pangolin", None).as_str())
    );
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(state.asked(), None);
    assert_eq!(
        state.shown_note(),
        Some(text::no_container_there("pangolin").as_str())
    );
    // A group row is no session.
    state.on_key(Key::Char('g'));
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(state.shown_note(), Some(text::PICK_A_SESSION));
}

/// The session of the riff daemon `uri` in `who`, with the facts that
/// name the containers `names`.
fn daemon_of(uri: &str, names: &[&str]) -> SessionInfo {
    let facts = riff_client::daemons::Facts {
        build: "0.1.0 d2ae0801c0de 2026-11-06T12:00:00Z".into(),
        sessions: names
            .iter()
            .map(|name| riff_client::daemons::Session {
                name: (*name).into(),
                item: "issue-88".into(),
                phase: Phase::Works,
                last: String::new(),
                digest: String::new(),
            })
            .collect(),
        ..riff_client::daemons::Facts::default()
    };
    let mut daemon = info(uri, SessionState::Idle);
    daemon.machine = Some(riff_core::wire::MachineInfo {
        numbers: facts.line(),
        age_secs: 10,
    });
    let step = crate::daemon::remote::status_line(812, "3.3.0", names.len());
    daemon.status = Some(riff_core::wire::StatusInfo {
        status: riff_core::wire::Status { step },
        age_secs: 1,
        stale: false,
    });
    daemon
}

/// [`mock_up`] with the daemon of mike on pangolin, which names the
/// container of 9c0d1e2f, the lead of mike on pangolin, and the daemon of
/// brett on kadomony. Each group is open.
fn with_pangolin() -> State {
    let mut state = mock_up();
    let (mut look, at) = state.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    look.who.sessions.extend([
        daemon_of(
            "riff://mike@pangolin/como-technologies/strata?session=d0d0d0d0",
            &["riff-issue-88-9c0d1e2f"],
        ),
        info(
            "riff://mike@pangolin/como-technologies/strata?session=6d2b7c1a&lead=true",
            SessionState::Idle,
        ),
        daemon_of(
            "riff://brett@kadomony/como-technologies/strata?session=e0e0e0e0",
            &[],
        ),
    ]);
    state.looked(look, at);
    state.on_key(Key::Char('e'));
    state
}

/// 01M6WMAXEVZB2XJN4XPR879J6W: on another host, `x` asks first, then
/// gives the request `daemon end NAME` for the daemon of that host, with
/// the name of the container in the facts of that daemon. `x` on the row
/// of a daemon asks first, then gives `leave REPO`. `t` and `l` there put
/// the step there in the note line.
#[test]
fn x_on_another_host_asks_its_daemon_and_t_and_l_name_the_step_there() {
    use crate::daemon::remote::Request;

    let mut state = with_pangolin();
    let name = "riff-issue-88-9c0d1e2f";
    state.on_key(Key::Row(row_of(&state, "9c0d1e2f")));
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(
        state.asked(),
        Some(format!("{} y/N", text::ask_end_there(name, "pangolin")))
    );
    // `n` changes nothing.
    assert_eq!(state.on_key(Key::Char('n')), None);
    assert_eq!(state.shown_note(), Some(crate::tui::NOTHING_CHANGED));
    state.on_key(Key::Char('x'));
    assert_eq!(
        state.on_key(Key::Char('y')),
        Some(Action::Host {
            host: "pangolin".into(),
            repo: "como-technologies/strata".into(),
            request: Request::End(name.into()),
        })
    );
    // `t` names the step on pangolin.
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::takeover_there("pangolin", Some(name)).as_str())
    );

    // The row of the daemon of pangolin: the host stops taking work.
    state.on_key(Key::Row(row_of(&state, "d0d0d0d0")));
    assert!(
        drawn(&state, 80, 40)
            .iter()
            .any(|row| row.contains("d0d0d0d0") && row.contains("daemon "))
    );
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(
        state.asked().as_deref(),
        Some("pangolin stops taking work for como-technologies/strata (riff host leave): y/N")
    );
    assert_eq!(
        state.on_key(Key::Char('y')),
        Some(Action::Host {
            host: "pangolin".into(),
            repo: "como-technologies/strata".into(),
            request: Request::Leave {
                repo: "como-technologies/strata".into(),
                now: false,
            },
        })
    );
    // The daemon of brett is not of mike.
    state.on_key(Key::Row(row_of(&state, "e0e0e0e0")));
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(state.asked(), None);
    assert_eq!(
        state.shown_note(),
        Some(text::not_your_daemon("kadomony").as_str())
    );
    // A worker of brett: no daemon of mike names its container.
    state.on_key(Key::Row(row_of(&state, "7a8b9c0d")));
    assert_eq!(state.on_key(Key::Char('x')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::no_container_there("kadomony").as_str())
    );

    // `l` and `t` on the lead of pangolin name `riff lead start` there.
    state.on_key(Key::Row(row_of(&state, "6d2b7c1a")));
    let there = text::lead_there("pangolin", Some("como-technologies/strata"));
    assert_eq!(state.on_key(Key::Char('l')), None);
    assert_eq!(state.shown_note(), Some(there.as_str()));
    state.note("");
    assert_eq!(state.on_key(Key::Char('t')), None);
    assert_eq!(state.shown_note(), Some(there.as_str()));
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
    // A row of no one repository: a person with two.
    paused.on_key(Key::Char('b'));
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

/// The place of the row of the session `id` in the tree of `state`.
fn row_of(state: &State, id: &str) -> usize {
    let tree = state.tree();
    let rows = state.rows(&tree);
    rows.iter()
        .position(|row| matches!(row, Row::Session { item, .. } if item.id() == id))
        .unwrap_or_else(|| panic!("no row of {id}"))
}

/// 01M6VT3JS3HRKXEJKDV0C97VZS: Enter on a worker in a container opens
/// its Session log in the Reports tab. With no pane, Enter first shows
/// the detail on the whole screen, and Enter there opens the log. A
/// worker of another host of mike gets the container that its daemon
/// names, and a session with no container that riff knows gets none.
#[test]
fn enter_on_a_worker_opens_its_session_log() {
    use crate::tui::reports::{NO_LOG_OF_SESSION, Report, Source};

    let busy = Source {
        name: "riff-issue-1590-5b1e2a90".into(),
        host: None,
    };
    // 80 x 24: the pane shows, and Enter opens the log at once.
    let mut state = mock_up();
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    state.drawn(&view);
    state.on_key(Key::Row(row_of(&state, "5b1e2a90")));
    assert_eq!(state.on_key(Key::Enter), None);
    assert_eq!(state.tab(), Tab::Reports);
    assert_eq!(state.reports().report(), Report::Log);
    assert_eq!(state.log_wanted(), Some(&busy));

    // 40 x 24: no pane. The detail first, then the log.
    let mut state = mock_up();
    let view = compute_view(&state, Rect::new(0, 0, 40, 24), NOW);
    state.drawn(&view);
    state.on_key(Key::Row(row_of(&state, "5b1e2a90")));
    state.on_key(Key::Enter);
    assert!(state.sessions.full());
    assert_eq!(state.tab(), Tab::Sessions);
    assert_eq!(
        drawn(&state, 40, 24)[23],
        "Esc back  Enter log  l lead  ? help"
    );
    assert_eq!(
        drawn(&state, 80, 24)[23],
        " Esc back  Enter log  l lead  t take over  ? help"
    );
    state.on_key(Key::Enter);
    assert_eq!(state.tab(), Tab::Reports);
    assert_eq!(state.log_wanted(), Some(&busy));

    // The blocked worker runs in no container that riff knows.
    let mut state = mock_up();
    state.on_key(Key::Enter);
    assert!(state.sessions.full());
    state.on_key(Key::Enter);
    assert_eq!(state.tab(), Tab::Sessions);
    assert_eq!(state.shown_note(), Some(NO_LOG_OF_SESSION));

    // The daemon of mike on pangolin names the container of 9c0d1e2f.
    let mut state = mock_up();
    let (mut look, at) = state.last_look().map(|(l, at)| (l.clone(), at)).unwrap();
    let facts = riff_client::daemons::Facts {
        build: "0.1.0 d2ae0801c0de 2026-11-06T12:00:00Z".into(),
        sessions: vec![riff_client::daemons::Session {
            name: "riff-issue-88-9c0d1e2f".into(),
            item: "issue-88".into(),
            phase: Phase::Works,
            last: String::new(),
            digest: String::new(),
        }],
        ..riff_client::daemons::Facts::default()
    };
    let mut daemon = info(
        "riff://mike@pangolin/como-technologies/strata?session=d0d0d0d0",
        SessionState::Idle,
    );
    daemon.machine = Some(riff_core::wire::MachineInfo {
        numbers: facts.line(),
        age_secs: 10,
    });
    look.who.sessions.push(daemon);
    state.looked(look, at);
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    state.drawn(&view);
    state.on_key(Key::Char('e'));
    state.on_key(Key::Row(row_of(&state, "9c0d1e2f")));
    state.on_key(Key::Enter);
    assert_eq!(
        state.log_wanted(),
        Some(&Source {
            name: "riff-issue-88-9c0d1e2f".into(),
            host: Some("pangolin".into()),
        })
    );
    // A worker of brett: his daemon is not of mike.
    state.on_key(Key::Open(Tab::Sessions));
    state.on_key(Key::Row(row_of(&state, "7a8b9c0d")));
    state.on_key(Key::Enter);
    assert_eq!(state.tab(), Tab::Sessions);
    assert!(state.sessions.full());
}

/// A click selects a row, a click on `▶` or `▼` folds or opens its
/// group, and a double click presses Enter (01M6VAZKG8FS4HEA9VSA4Z4YF1).
#[test]
fn a_click_selects_a_row_and_folds_a_group() {
    let mut state = mock_up();
    let area = Rect::new(0, 0, 80, 24);
    let view = compute_view(&state, area, NOW);
    // The row of 5b1e2a90 is the screen row 6.
    assert_eq!(view.hit(30, 6), Some(Key::Row(3)));
    state.on_click(&view, 30, 6, false);
    assert!(drawn(&state, 80, 24)[6].starts_with(" > ● 5b1e2a90"));
    // The `▼` of riff at the column 1 of the row 3.
    assert_eq!(view.hit(1, 3), Some(Key::Fold(0)));
    let view = compute_view(&state, area, NOW);
    state.on_click(&view, 1, 3, false);
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▶ riff"));
    // A double click on the group opens it.
    let view = compute_view(&state, area, NOW);
    state.on_click(&view, 30, 3, true);
    assert!(drawn(&state, 80, 24)[3].starts_with(" ▼ riff"));
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
    assert!(rows[2].ends_with("  1-9 of 46"), "{rows:#?}");
    let area = Rect::new(0, 0, 80, 24);
    for _ in 0..2 {
        state.drawn(&compute_view(&state, area, NOW));
        state.on_key(Key::PageDown);
    }
    state.drawn(&compute_view(&state, area, NOW));
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].ends_with("  12-20 of 46"), "{rows:#?}");
    assert_eq!(state.sessions.at, 19);
    assert!(rows.iter().any(|r| r.starts_with(" > !")), "{rows:#?}");
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
    // of the TUI, with no repository, on the host of the TUI.
    let mut state = mock_up();
    state.daemon_looked(Ok(status(vec![live(
        "riff-issue-77-cccc3333",
        "cccc3333",
        Phase::Holds,
        "",
    )])));
    state.on_key(Key::Char('e'));
    let text = drawn(&state, 80, 40).join("\n");
    assert!(text.contains("\n ▼ - › mike  "), "{text}");
    assert!(
        text.contains("\n     cccc3333  #77    container  thelio   holds for a takeover"),
        "{text}"
    );
    let top = crate::top::Top {
        pauses: &RiffState::Running.into(),
        server: None,
        sessions: &state.model,
        people: &[],
        issues: &BTreeMap::new(),
        holds: &BTreeMap::new(),
        repo: None,
        projects: &[],
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
    let drawn = drawn(&mock_up(), 80, 24);
    let drawn: Vec<String> = drawn.into_iter().filter(|row| !row.is_empty()).collect();
    assert_eq!(book("## See what each session does", "[1 Sessions]"), drawn);
}

/// The lines of the first `text` block that starts with `first` under
/// the heading `heading` of "How riff works" in the book.
fn book(heading: &str, first: &str) -> Vec<String> {
    let page = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/how-it-works.md"
    ))
    .unwrap();
    let part = page.split(&format!("\n{heading}\n")).nth(1).unwrap();
    let block = part
        .split("```text\n")
        .find(|block| block.starts_with(first))
        .unwrap_or_else(|| panic!("no block {first} under {heading}"));
    block
        .split("```")
        .next()
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `riff top --once` keeps its own tree of the shared sessions model,
/// apart from the shape of the Sessions tab
/// (01M6WKCC98EGKB3MSYKN3X2JMH): person › host › repository › session
/// with `By::Person`, as `--by person` shows it
/// (01M77F8JJ8WC0TSCYP3ARJNQGS), and repository › person › host ›
/// session with `By::Repo`.
#[test]
fn riff_top_keeps_its_own_tree() {
    let state = mock_up();
    let (look, _) = state.last_look().unwrap();
    let tree = |by: By| {
        let show = riff_client::sessions::Show {
            by,
            ..riff_client::sessions::Show::default()
        };
        let top = crate::top::Top {
            pauses: &RiffState::Running.into(),
            server: look.server.as_ref(),
            sessions: &state.model,
            people: &look.who.people,
            issues: &BTreeMap::new(),
            holds: &BTreeMap::new(),
            repo: None,
            projects: &[],
            show: &show,
            width: 100,
            fault: None,
            machines: &[],
        };
        let text = anstream::adapter::strip_str(&top.view()).to_string();
        // The header and the blocks come first, each after an empty line.
        text.split("\n\n").last().unwrap().to_owned()
    };
    assert_eq!(
        tree(By::Person).lines().collect::<Vec<_>>(),
        [
            "ann  admin  offline  seen 1h ago",
            "brett  online  › kadomony  › strata  1 session: 1 worker; 1 busy, 1 claim",
            "│  no numbers",
            "└─ 7a8b9c0d  worker  busy",
            "     working on #12",
            "mike  admin  online  5 sessions: 2 busy, 1 idle, 1 blocked, 4 claims",
            "├─ pangolin  › strata  1 session: 1 worker; 1 busy, 1 claim",
            "│  │  no numbers",
            "│  └─ 9c0d1e2f  worker  busy",
            "│       working on #88",
            "└─ thelio  › riff  4 sessions: 1 lead, 2 containers, 1 worker; 1 to update; 1 busy, 1 idle, 1 block…",
            "   │  no numbers",
            "   ├─ 3a3f8d5d  worker  blocked",
            "   │    which of two designs? (32m ago)",
            "   │    the lead gave no answer",
            "   │    working on #994",
            "   ├─ 4e54d4e5  ↑  lead  idle",
            "   │    monitoring work for 2h",
            "   ├─ 5b1e2a90  container  busy",
            "   │    working on #1590",
            "   └─ 8f1c2d3e  container  waiting",
            "        working on #1599",
            "        waits for a verify of PR #1614",
        ]
    );
    assert_eq!(
        tree(By::Repo).lines().collect::<Vec<_>>(),
        [
            "riff  › mike  admin  online  › thelio  4 sessions: 1 lead, 2 containers, 1 worker; 1 to update; 1 b…",
            "│  no numbers",
            "├─ 3a3f8d5d  worker  blocked",
            "│    which of two designs? (32m ago)",
            "│    the lead gave no answer",
            "│    working on #994",
            "├─ 4e54d4e5  ↑  lead  idle",
            "│    monitoring work for 2h",
            "├─ 5b1e2a90  container  busy",
            "│    working on #1590",
            "└─ 8f1c2d3e  container  waiting",
            "     working on #1599",
            "     waits for a verify of PR #1614",
            "strata  2 sessions: 2 busy, 2 claims",
            "├─ brett  online  › kadomony  1 session: 1 worker; 1 busy, 1 claim",
            "│  │  no numbers",
            "│  └─ 7a8b9c0d  worker  busy",
            "│       working on #12",
            "└─ mike  admin  online  › pangolin  1 session: 1 worker; 1 busy, 1 claim",
            "   │  no numbers",
            "   └─ 9c0d1e2f  worker  busy",
            "        working on #88",
        ]
    );
}
