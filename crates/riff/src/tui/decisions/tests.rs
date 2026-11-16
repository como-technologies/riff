//! The screen tests of the Decisions tab: the mock-ups at 80 x 24 and
//! at 40 x 24, the persons, the keys, the clicks and the reads.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use riff_core::build::Build;

use super::*;
use crate::tui::reads::Read;
use crate::tui::view::{compute_view, render};
use crate::tui::{Look, News, Tab};

/// The time of the screen tests, in milliseconds since the epoch.
const NOW: i64 = 1_794_000_000_000;

const RIFF: &str = "como-technologies/riff";

const STRATA: &str = "como-technologies/strata";

fn build(id: &str) -> Build {
    format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
}

/// The thread of the repository `name`.
fn thread(name: &str) -> ThreadName {
    name.parse().unwrap()
}

/// A decision of `repo` with the labels `labels`, in `milestone`, whose
/// body has the heading `## Questions for` the names `named`.
fn decision(
    repo: &str,
    number: u64,
    title: &str,
    milestone: Option<&str>,
    labels: &[&str],
    named: &[&str],
) -> Decision {
    let issue = IssueRef {
        repo: thread(repo),
        number,
    };
    let labels = labels.iter().map(|l| (*l).to_owned()).collect();
    let body = match named {
        [] => String::new(),
        named => format!("## Questions for {}", named.join(" and ")),
    };
    Decision::new(issue, title.into(), milestone, labels, &body).unwrap()
}

/// The decisions of riff in the mock-up of the module: two for mike, one
/// of them as `Mike`, one for nate, a design with no question, and a
/// question with no milestone.
fn riff_decisions() -> Vec<Decision> {
    let wave = Some("Wave 60: the lead does the host steps");
    vec![
        decision(
            RIFF,
            2189,
            "Design: a Plan tab in the TUI for the planning of the waves",
            wave,
            &["design", "question"],
            &["nate"],
        ),
        decision(
            RIFF,
            2173,
            "The verify of a pull request does not converge",
            wave,
            &["design", "question"],
            &["mike"],
        ),
        decision(
            RIFF,
            1489,
            "Design: one place where a person sees each decision",
            Some("Backlog"),
            &["question"],
            &["Mike"],
        ),
        decision(
            RIFF,
            1739,
            "Design: take over or attach a session of another host",
            Some("Backlog"),
            &["design"],
            &[],
        ),
        decision(
            RIFF,
            1532,
            "A wave item whose result is a report in its issue",
            None,
            &["question"],
            &[],
        ),
    ]
}

/// The state of the mock-up of the Decisions tab before the first read:
/// the person `me`, in a clone of riff, a project of a riff with mike and
/// nate, 2 seconds after a good look.
fn opened(me: &str) -> State {
    let me = me.parse().unwrap();
    let mut state = State::new(build("10df8a4c0ffe"), me).in_clone(Some(RIFF.to_owned()));
    let mut look = Look {
        server: Some(build("10df8a4c0ffe")),
        ..Look::default()
    };
    look.who.repos = vec![thread(RIFF)];
    look.who.projects = vec![thread(RIFF)];
    look.who.people = serde_json::from_value(serde_json::json!([
        {"user": "mike", "role": "admin", "live": true},
        {"user": "nate", "role": "member", "live": false},
    ]))
    .unwrap();
    state.looked(look, NOW - 2_000);
    state.news(Tab::Chat, News::Lines(2));
    state.on_key(Key::Open(Tab::Decisions));
    state
}

/// The state of the mock-up of the Decisions tab: mike, 41 seconds after
/// the read of the decisions.
fn mock_up() -> State {
    let mut state = opened("riff://mike@thelio");
    state.read_decisions(vec![(thread(RIFF), Ok(riff_decisions()))], NOW - 41_000);
    state.drew(20, NOW);
    state
}

/// `state` with the project `repo` too.
fn with_project(mut state: State, repo: &str) -> State {
    let mut look = state.last_look().unwrap().0.clone();
    look.who.repos.push(thread(repo));
    look.who.projects.push(thread(repo));
    state.looked(look, NOW);
    state
}

/// The mock-up with the project strata too.
fn two_projects() -> State {
    with_project(mock_up(), STRATA)
}

/// The open question `number` of riff in the Backlog, with the body
/// `body`, as the read gives it.
fn asked(number: u64, body: &str) -> Decision {
    let entry: Entry = serde_json::from_value(serde_json::json!(
        {"number": number, "title": "Which name?", "labels": [{"name": "question"}],
         "milestone": {"number": 1, "title": "Backlog"}, "body": body}
    ))
    .unwrap();
    Decision::of(&thread(RIFF), entry).unwrap()
}

/// The rows of `state` drawn into a `Buffer` of `width` x `height`, with
/// no spaces at their ends.
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

/// The numbers of the issues of the rows of `state`, in their order.
fn numbers(state: &State) -> Vec<u64> {
    state
        .decision_rows()
        .iter()
        .filter_map(|row| row.pick().map(|issue| issue.number))
        .collect()
}

/// The number of the issue of the selected row of `state`.
fn selected(state: &State) -> Option<u64> {
    let rows = state.decision_rows();
    let at = state.decisions().selected(&rows)?;
    rows[at].pick().map(|issue| issue.number)
}

/// 01M7FYVHB1APFXPXA71PMD18EH, 01M7FYVHDJJ288RRV8EBRKSY49: the screen
/// test of the Decisions tab at 80 x 24 is the mock-up of the rustdoc of
/// the module, and its body is the mock-up of the book. The questions for
/// mike come first, also one that names `Mike`, and the row shows the
/// USER `mike`. Each row has the number, the wave or `Backlog`, the
/// title, the labels and the persons.
#[test]
fn the_decisions_tab_at_80_x_24_is_the_mock_up() {
    let screen = drawn(&mock_up(), 80, 24);
    let doc = include_str!("../decisions.rs");
    let want = crate::tui::text_block(doc, "# Design", "//! ");
    assert_eq!(screen, want);
    let book = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/how-it-works.md"
    ))
    .unwrap();
    let book = crate::tui::text_block(&book, "### Work through the decisions in the TUI", "");
    assert_eq!(book, screen[2..12]);
}

/// 01M7FYVHB1APFXPXA71PMD18EH: the screen test of the narrow layout of
/// the Decisions tab at 40 x 24 (01M6VAZKK4KHCDCX4RKG9B95D7): the mark,
/// the item, the wave and the title.
#[test]
fn the_decisions_tab_at_40_x_24_is_the_narrow_mock_up() {
    let mut want = vec![
        "‹ 7/7 Decisions ›",
        "riff running · link ok 2s",
        "QUESTIONS FOR MIKE · 2",
        "> ? #2173  Wave 60   The verify of a pu…",
        "  ? #1489  Backlog   Design: one place …",
        "",
        "OTHER DECISIONS · 3",
        "  ? #2189  Wave 60   Design: a Plan tab…",
        "  · #1739  Backlog   Design: take over …",
        "  ? #1532  -         A wave item whose …",
    ];
    want.resize(22, "");
    want.extend([
        "The decisions come from gh, 41s ago.",
        "↑↓ select  Enter open  ? help  q quit",
    ]);
    assert_eq!(drawn(&mock_up(), 40, 24), want);
}

/// The open entries of riff for the tests of the read: #12, a question
/// in the Backlog with the labels `labels` and the body `body`, #13, a
/// design issue of Wave 3, and #14, neither.
fn entries(labels: &str, body: &str) -> Vec<Entry> {
    serde_json::from_value(serde_json::json!([
        {"number": 12, "title": "Which name?",
         "labels": serde_json::from_str::<serde_json::Value>(labels).unwrap(),
         "milestone": {"number": 1, "title": "Backlog"}, "body": body},
        {"number": 13, "title": "Design: the board", "labels": [{"name": "design"}],
         "milestone": {"number": 5, "title": "Wave 3"}},
        {"number": 14, "title": "Fix it", "milestone": {"number": 5, "title": "Wave 3"}},
    ]))
    .unwrap()
}

/// The decisions of the entries `entries` of riff.
fn found(entries: Vec<Entry>) -> Vec<Decision> {
    entries
        .into_iter()
        .filter_map(|e| Decision::of(&thread(RIFF), e))
        .collect()
}

/// 01M7FYVHB1APFXPXA71PMD18EH: a question in the Backlog and a design
/// issue in a wave are both in the tab, read from the REST list of the
/// open issues. After the label `question` of the question goes, the next
/// read takes it out of the tab. The design stays.
#[test]
fn a_question_in_the_backlog_and_a_design_in_a_wave_show_and_a_question_with_no_label_goes() {
    let mut state = mock_up();
    let body = "## Questions for mike\n\nQ1. Which name?";
    state.read_decisions(
        vec![(
            thread(RIFF),
            Ok(found(entries(r#"[{"name": "question"}]"#, body))),
        )],
        NOW,
    );
    assert_eq!(numbers(&state), [12, 13]);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " QUESTIONS FOR MIKE · 1", "{rows:#?}");
    assert!(
        rows[4].starts_with(" > ?   #12    Backlog   Which name?"),
        "{rows:#?}"
    );
    assert!(rows[4].ends_with("question          mike"), "{rows:#?}");
    assert!(
        rows[8].starts_with("   ·   #13    Wave 3    Design: the board"),
        "{rows:#?}"
    );
    // The label goes: the question leaves the tab at the next read.
    state.read_decisions(
        vec![(thread(RIFF), Ok(found(entries("[]", body))))],
        NOW + 60_000,
    );
    assert_eq!(numbers(&state), [13]);
    // With no decision open, the body says so.
    state.read_decisions(vec![(thread(RIFF), Ok(Vec::new()))], NOW + 120_000);
    assert_eq!(state.decision_rows(), [Row::Text(NONE_OPEN.to_owned())]);
}

/// 01M7G7SCW5AR1P73R1CMA2QWF7: only a person of the riff counts. The
/// heading `Questions for the discussion` names nobody, and `Questions
/// for nate about the board` names only nate.
#[test]
fn a_heading_names_only_the_persons_of_the_riff() {
    let mut state = mock_up();
    let question = r#"[{"name": "question"}]"#;
    let mut read = found(entries(question, "## Questions for the discussion"));
    read.push(asked(15, "## Questions for nate about the board"));
    state.read_decisions(vec![(thread(RIFF), Ok(read))], NOW);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " OTHER DECISIONS · 3", "{rows:#?}");
    assert!(rows[4].starts_with(" > ·   #13 "), "{rows:#?}");
    assert!(rows[5].starts_with("   ?   #12 "), "{rows:#?}");
    assert!(rows[5].ends_with("question          -"), "{rows:#?}");
    assert!(rows[6].starts_with("   ?   #15 "), "{rows:#?}");
    assert!(rows[6].ends_with("question          nate"), "{rows:#?}");
}

/// 01M7G7SCW5AR1P73R1CMA2QWF7, 01M7FYVHB1APFXPXA71PMD18EH: a USER with
/// a `.` gets the questions for that USER first, also when the look does
/// not list the person of the TUI. `ada` is another person. A list of
/// persons wider than the column `FOR` ends with `…`.
#[test]
fn a_user_with_a_dot_sees_the_questions_for_that_user_first() {
    let mut state = opened("riff://ada.lovelace@thelio");
    let mut look = state.last_look().unwrap().0.clone();
    look.who.people = serde_json::from_value(serde_json::json!([
        {"user": "mike", "role": "admin", "live": true},
        {"user": "ada", "role": "member", "live": false},
    ]))
    .unwrap();
    state.looked(look, NOW);
    state.read_decisions(
        vec![(
            thread(RIFF),
            Ok(vec![
                asked(12, "## Questions for ada"),
                asked(13, "## Questions for Ada.Lovelace and ada"),
            ]),
        )],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " QUESTIONS FOR ADA.LOVELACE · 1", "{rows:#?}");
    assert!(
        rows[4].starts_with(" > ?   #13    Backlog   Which name?"),
        "{rows:#?}"
    );
    assert!(
        rows[4].ends_with("question          ada.lovel…"),
        "{rows:#?}"
    );
    assert_eq!(rows[6], " OTHER DECISIONS · 1", "{rows:#?}");
    assert!(rows[8].ends_with("question          ada"), "{rows:#?}");
}

/// 01M7G7SCW5AR1P73R1CMA2QWF7: a comma with no space after it names the
/// next person too. Labels wider than the column `LABELS` end with `…`.
#[test]
fn a_comma_with_no_space_names_the_next_person() {
    let mut state = mock_up();
    let labels = ["design", "question", "enhancement"];
    let wide = decision(RIFF, 13, "Design: the board", Some("Backlog"), &labels, &[]);
    state.read_decisions(
        vec![(
            thread(RIFF),
            Ok(vec![asked(12, "## Questions for nate,mike"), wide]),
        )],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " QUESTIONS FOR MIKE · 1", "{rows:#?}");
    assert!(
        rows[4].ends_with("question          nate mike"),
        "{rows:#?}"
    );
    assert!(rows[8].starts_with("   ?   #13 "), "{rows:#?}");
    assert!(rows[8].ends_with("design question…  -"), "{rows:#?}");
}

/// 01M7G7SCW5AR1P73R1CMA2QWF7: a comma alone, or two commas, between
/// two names name the next person too. A line that starts with one `*`
/// is no heading, so it names nobody.
#[test]
fn a_comma_alone_names_the_next_person_and_one_star_is_no_heading() {
    let mut state = mock_up();
    state.read_decisions(
        vec![(
            thread(RIFF),
            Ok(vec![
                asked(12, "## Questions for nate , mike"),
                asked(13, "## Questions for nate,,mike"),
                asked(14, "* Questions for mike: see below"),
            ]),
        )],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " QUESTIONS FOR MIKE · 2", "{rows:#?}");
    assert!(rows[4].starts_with(" > ?   #12 "), "{rows:#?}");
    assert!(
        rows[4].ends_with("question          nate mike"),
        "{rows:#?}"
    );
    assert!(rows[5].starts_with("   ?   #13 "), "{rows:#?}");
    assert!(
        rows[5].ends_with("question          nate mike"),
        "{rows:#?}"
    );
    assert_eq!(rows[7], " OTHER DECISIONS · 1", "{rows:#?}");
    assert!(rows[9].starts_with("   ?   #14 "), "{rows:#?}");
    assert!(rows[9].ends_with("question          -"), "{rows:#?}");
}

/// 01M7FYVHDJJ288RRV8EBRKSY49: the help popup of the Decisions tab names
/// each key of the tab: the keys that select a row, `Enter`, and the keys
/// of an issue, as in the Waves tab.
#[test]
fn the_help_popup_names_each_key_of_the_tab() {
    let mut state = mock_up();
    state.on_key(Key::Help);
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[3].contains("┌ Keys of the Decisions tab ─"),
        "{rows:#?}"
    );
    let keys: Vec<&str> = rows[4..11]
        .iter()
        .map(|row| row.split('│').nth(1).unwrap_or_default().trim())
        .collect();
    assert_eq!(
        keys,
        [
            "↑ ↓, j k   select a row",
            "PgUp PgDn  move a page",
            "g G        the first or last row",
            "Enter      open the issue in the browser",
            "o          show the URL",
            "a          mark as answered",
            "y n        answer the question",
        ],
        "{rows:#?}"
    );
    assert!(rows[11].contains("│ 1 to 7     open a tab "), "{rows:#?}");
}

/// 01M7GBGD8EH2B1DXZ8NADZ1A85, 01M7GFH1D93M7F7S82E788EFHG: while the
/// first read runs, the body has no line, and the note line says that
/// the first read runs. After a first read that fails, the body names
/// the project that no read gave, and the note line names the fault.
/// With two projects, the decisions of the project that read well show,
/// and the last line names the other one. When each project read well
/// and has no decision, the body says so.
#[test]
fn a_first_read_that_fails_names_the_project_with_no_read() {
    let mut state = opened("riff://mike@thelio");
    assert_eq!(state.decision_rows(), []);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2..22], [""; 20]);
    assert_eq!(
        rows[22],
        " The decisions come from gh: the first read runs."
    );
    let failed = || Err("gh: HTTP 502".to_owned());
    state.read_decisions(vec![(thread(RIFF), failed())], NOW);
    let unread = |repo: &str| Row::Text(text::decisions_unread(&[&thread(repo)]));
    assert_eq!(state.decision_rows(), [unread(RIFF)]);
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[2],
        " No read of the decisions of como-technologies/riff worked yet."
    );
    assert_eq!(rows[22], " riff: gh: HTTP 502");
    let mut state = with_project(state, STRATA);
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(STRATA), failed()),
        ],
        NOW + 60_000,
    );
    assert_eq!(numbers(&state), [2173, 1489, 2189, 1739, 1532]);
    let rows = state.decision_rows();
    assert_eq!(rows[rows.len() - 2..], [Row::Blank, unread(STRATA)]);
    state.read_decisions(
        vec![(thread(RIFF), Ok(vec![])), (thread(STRATA), Ok(vec![]))],
        NOW + 120_000,
    );
    assert_eq!(state.decision_rows(), [Row::Text(NONE_OPEN.to_owned())]);
}

/// 01M7FYVHB1APFXPXA71PMD18EH: in a part, the rows of one place go by
/// the repository and then by the number, also when the read gives them
/// in another order, as GitHub gives the newest first.
#[test]
fn the_rows_of_one_place_go_by_the_repository_and_the_number() {
    let mut state = two_projects();
    let backlog =
        |repo, number| decision(repo, number, "Which?", Some("Backlog"), &["question"], &[]);
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(vec![backlog(RIFF, 30), backlog(RIFF, 10)])),
            (thread(STRATA), Ok(vec![backlog(STRATA, 5)])),
        ],
        NOW,
    );
    assert_eq!(numbers(&state), [10, 30, 5]);
}

/// 01M7FYVHB1APFXPXA71PMD18EH, 01M7GFGRJMAYZVG68EP3B916T8: when two
/// projects have the same name, an item names the owner too. Each row
/// still fits the line: at 80 columns the labels and the title get fewer
/// columns, and `FOR` shows whole. At 40 columns, an item loses its
/// start, and the title keeps 10 columns.
#[test]
fn two_projects_with_the_same_name_name_the_owner_and_each_row_fits() {
    let fork = "n8behavior/riff";
    let mut state = with_project(mock_up(), fork);
    let seven = decision(
        fork,
        7,
        "Which store?",
        Some("Wave 2"),
        &["question"],
        &["mike"],
    );
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(fork), Ok(vec![seven])),
        ],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[3].ends_with("  WAVE      TITLE     LABELS        FOR"),
        "{rows:#?}"
    );
    assert_eq!(
        rows[4],
        " > ?   n8behavior/riff#7            Wave 2    Which s…  question      mike"
    );
    assert_eq!(
        rows[5],
        "   ?   como-technologies/riff#2173  Wave 60   The ver…  design ques…  mike"
    );
    assert!(rows.iter().all(|row| width(row) <= 80), "{rows:#?}");
    let narrow = drawn(&state, 40, 24);
    assert_eq!(narrow[3], "> ? …havior/riff#7  Wave 2    Which sto…");
    assert_eq!(narrow[4], "  ? …ies/riff#2173  Wave 60   The verif…");
}

/// 01M7GFGRJMAYZVG68EP3B916T8: at 80 columns too, an item wider than the
/// columns that the others leave loses its start, so that its number
/// shows. The title keeps 10 columns, and the labels get none.
#[test]
fn a_wide_item_loses_its_start_at_80_columns_too() {
    let long = "como-technologies/a-repository-with-a-long-name";
    let fork = "n8behavior/a-repository-with-a-long-name";
    let mut state = with_project(with_project(mock_up(), long), fork);
    let two = decision(
        long,
        2,
        "Which store?",
        Some("Wave 2"),
        &["question"],
        &["mike"],
    );
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(Vec::new())),
            (thread(long), Ok(vec![two])),
            (thread(fork), Ok(Vec::new())),
        ],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert!(rows[3].ends_with("  WAVE      TITLE     FOR"), "{rows:#?}");
    assert_eq!(
        rows[4],
        " > ?   …nologies/a-repository-with-a-long-name#2  Wave 2    Which s…  mike"
    );
    assert!(rows.iter().all(|row| width(row) <= 80), "{rows:#?}");
}

/// 01M7GFH1D93M7F7S82E788EFHG: with no project of the person, the tab
/// says so, as the Waves tab.
#[test]
fn with_no_project_the_tab_says_so() {
    let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
    state.looked(Look::default(), NOW);
    state.on_key(Key::Open(Tab::Decisions));
    assert_eq!(
        drawn(&state, 80, 24)[2..4],
        crate::tui::waves::NO_BOARD.map(|line| format!(" {line}"))
    );
}

/// 01M7FYVHDJJ288RRV8EBRKSY49, 01M7GFH204647G2B9NTX72JKFR: Enter opens
/// the issue of the selected row in the browser, and puts its URL in the
/// note line, also in the narrow layout. `o` puts the URL there too. `a`
/// asks `y` or `n`, and `y` marks the questions as answered with the code
/// of `riff questions --answered`. On a design with no label `question`,
/// `a` asks nothing.
#[test]
fn enter_opens_the_issue_o_shows_its_url_and_a_marks_it_as_answered() {
    let mut state = mock_up();
    let issue = |number| IssueRef {
        repo: thread(RIFF),
        number,
    };
    assert_eq!(state.on_key(Key::Enter), Some(Action::Browse(issue(2173))));
    let url = "https://github.com/como-technologies/riff/issues/2173";
    assert_eq!(state.shown_note(), Some(url));
    assert_eq!(drawn(&state, 54, 24)[22], url);
    state.on_key(Key::Char('j'));
    assert_eq!(state.on_key(Key::Char('o')), None);
    assert_eq!(
        state.shown_note(),
        Some("https://github.com/como-technologies/riff/issues/1489")
    );
    assert_eq!(state.on_key(Key::Char('a')), None);
    assert_eq!(
        state.asked().as_deref(),
        Some("Mark the questions of #1489 of como-technologies/riff as answered? y/N")
    );
    let answered = Effect::Answered {
        repo: RIFF.to_owned(),
        item: 1489,
    };
    assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(answered)));
    // A design with no label question: `a` changes nothing.
    state.on_key(Key::Char('G'));
    state.on_key(Key::Char('k'));
    assert_eq!(state.on_key(Key::Char('a')), None);
    assert_eq!(
        state.shown_note(),
        Some(text::questions_not_asked(RIFF, 1739).as_str())
    );
    assert_eq!(state.asked(), None);
}

/// 01M7FYVHDJJ288RRV8EBRKSY49: a click on a row selects it, and a click
/// on the selected row opens it. A double click opens the row that it
/// hits, also when the row was not selected.
#[test]
fn a_click_selects_a_row_and_a_double_click_opens_it() {
    let mut state = mock_up();
    let browse = |number| {
        Some(Action::Browse(IssueRef {
            repo: thread(RIFF),
            number,
        }))
    };
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    // Row 9 is #2189, and row 10 is #1739.
    assert_eq!(state.on_click(&view, 10, 9, false), None);
    assert_eq!(selected(&state), Some(2189));
    assert_eq!(state.on_click(&view, 10, 9, true), browse(2189));
    assert_eq!(state.on_click(&view, 10, 10, true), browse(1739));
    assert_eq!(selected(&state), Some(1739));
    // The title of a part is no row to select.
    assert_eq!(state.on_click(&view, 10, 7, true), None);
}

/// 01M6VAZKG8FS4HEA9VSA4Z4YF1, 01M7FYVHDJJ288RRV8EBRKSY49: a click on a
/// key of the key line presses it. `↑↓` is no place to click.
#[test]
fn a_click_on_a_key_of_the_key_line_presses_it() {
    let mut state = mock_up();
    let keys = " ↑↓ select  Enter open  o URL  a answered  r read again  ? help  q quit";
    assert_eq!(drawn(&state, 80, 24)[23], keys);
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    let at = |word: &str| u16::try_from(keys[..keys.find(word).unwrap()].chars().count()).unwrap();
    assert_eq!(view.hit(at("↑↓"), 23), None);
    let issue = IssueRef {
        repo: thread(RIFF),
        number: 2173,
    };
    assert_eq!(
        state.on_click(&view, at("Enter"), 23, false),
        Some(Action::Browse(issue))
    );
    state.note("an old note");
    assert_eq!(state.on_click(&view, at("o URL"), 23, false), None);
    assert_eq!(
        state.shown_note(),
        Some("https://github.com/como-technologies/riff/issues/2173")
    );
    assert_eq!(state.on_click(&view, at("a answered"), 23, false), None);
    assert_eq!(
        state.asked().as_deref(),
        Some("Mark the questions of #2173 of como-technologies/riff as answered? y/N")
    );
    state.on_key(Key::Char('n'));
    assert_eq!(
        state.on_click(&view, at("read again"), 23, false),
        Some(Action::Look)
    );
    assert_eq!(view.hit(at("? help"), 23), Some(Key::Help));
    assert_eq!(view.hit(at("q quit"), 23), Some(Key::Quit));
}

/// 01M7FYVHB1APFXPXA71PMD18EH: with more than one project, an item names
/// its repository, and the rows of each project are in one list, also
/// when only one project has decisions.
#[test]
fn with_two_projects_an_item_names_its_repository() {
    let mut state = two_projects();
    let strata = decision(
        STRATA,
        7,
        "Which store?",
        Some("Wave 2"),
        &["question"],
        &["mike"],
    );
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(STRATA), Ok(vec![strata])),
        ],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " QUESTIONS FOR MIKE · 3", "{rows:#?}");
    assert!(
        rows[4].starts_with(" > ?   strata#7   Wave 2    Which store?"),
        "{rows:#?}"
    );
    assert!(
        rows[5].starts_with("   ?   riff#2173  Wave 60"),
        "{rows:#?}"
    );
    // strata has no decision: the items still name their repository.
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(STRATA), Ok(Vec::new())),
        ],
        NOW,
    );
    let rows = drawn(&state, 80, 24);
    assert!(
        rows[4].starts_with(" > ?   riff#2173  Wave 60"),
        "{rows:#?}"
    );
}

/// 01M7G7SHRDEA6BCNSYQA8RGZ23: a read that fails keeps the last decisions
/// of its project, while the other project reads well, and the note line
/// names the fault once. After a good read, the next fault shows again.
#[test]
fn a_read_that_fails_keeps_the_last_decisions_and_names_the_fault_once() {
    let mut state = two_projects();
    let strata = |number| decision(STRATA, number, "Which store?", None, &["design"], &[]);
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(STRATA), Ok(vec![strata(7)])),
        ],
        NOW,
    );
    let failed = |number| {
        vec![
            (thread(RIFF), Err("gh: HTTP 502".to_owned())),
            (thread(STRATA), Ok(vec![strata(number)])),
        ]
    };
    state.read_decisions(failed(8), NOW + 60_000);
    assert_eq!(numbers(&state), [2173, 1489, 2189, 1739, 1532, 8]);
    assert_eq!(state.shown_note(), Some("riff: gh: HTTP 502"));
    assert_eq!(drawn(&state, 80, 24)[22], " riff: gh: HTTP 502");
    // The same fault again: the note line keeps the last note.
    state.on_key(Key::Char('o'));
    let url = "https://github.com/como-technologies/riff/issues/2173";
    state.read_decisions(failed(9), NOW + 120_000);
    assert_eq!(state.shown_note(), Some(url));
    assert_eq!(numbers(&state), [2173, 1489, 2189, 1739, 1532, 9]);
    // A good read, then the fault again.
    state.read_decisions(
        vec![
            (thread(RIFF), Ok(riff_decisions())),
            (thread(STRATA), Ok(vec![])),
        ],
        NOW + 180_000,
    );
    state.read_decisions(failed(9), NOW + 240_000);
    assert_eq!(state.shown_note(), Some("riff: gh: HTTP 502"));
}

/// 01M7FYVHDJJ288RRV8EBRKSY49: `j` `k` and the arrows select the next
/// or the decision before, over the titles of the parts. `PgUp` and
/// `PgDn` move a page, and `g` and `G` go to the first and the last
/// decision. A move stops at the first and the last decision. When the
/// selected decision leaves the tab, the first one is selected.
#[test]
fn the_move_keys_select_each_decision_and_stop_at_the_ends() {
    let mut state = mock_up();
    assert_eq!(selected(&state), Some(2173));
    state.on_key(Key::Char('k'));
    assert_eq!(selected(&state), Some(2173));
    state.on_key(Key::Char('j'));
    assert_eq!(selected(&state), Some(1489));
    state.on_key(Key::Down);
    assert_eq!(selected(&state), Some(2189));
    state.on_key(Key::Up);
    assert_eq!(selected(&state), Some(1489));
    state.on_key(Key::Char('G'));
    assert_eq!(selected(&state), Some(1532));
    state.on_key(Key::Down);
    assert_eq!(selected(&state), Some(1532));
    state.on_key(Key::Char('g'));
    assert_eq!(selected(&state), Some(2173));
    state.on_key(Key::PageDown);
    assert_eq!(selected(&state), Some(1532));
    // A page of 2 rows.
    state.drew(2, NOW);
    state.on_key(Key::PageUp);
    assert_eq!(selected(&state), Some(2189));
    state.on_key(Key::PageUp);
    assert_eq!(selected(&state), Some(2173));
    state.on_key(Key::PageDown);
    assert_eq!(selected(&state), Some(2189));
    // #2189 leaves the tab: the first decision is selected.
    let mut read = riff_decisions();
    read.retain(|d| d.issue.number != 2189);
    state.read_decisions(vec![(thread(RIFF), Ok(read))], NOW);
    assert_eq!(selected(&state), Some(2173));
}

/// A list longer than the body shows the lines up to the selected row:
/// `G` shows the last decision on the last line of the body, and `g`
/// shows the first lines again.
#[test]
fn a_long_list_shows_the_lines_up_to_the_selected_row() {
    let mut state = mock_up();
    let many: Vec<Decision> = (1..=30)
        .map(|n| {
            let title = format!("Question {n}");
            decision(RIFF, n, &title, Some("Backlog"), &["question"], &[])
        })
        .collect();
    state.read_decisions(vec![(thread(RIFF), Ok(many))], NOW);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " OTHER DECISIONS · 30", "{rows:#?}");
    assert!(rows[4].starts_with(" > ?   #1 "), "{rows:#?}");
    assert!(rows[21].starts_with("   ?   #18 "), "{rows:#?}");
    state.on_key(Key::Char('G'));
    let rows = drawn(&state, 80, 24);
    assert!(rows[2].starts_with("   ?   #11 "), "{rows:#?}");
    assert!(rows[21].starts_with(" > ?   #30 "), "{rows:#?}");
    state.on_key(Key::Char('g'));
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " OTHER DECISIONS · 30", "{rows:#?}");
}

/// 01M7GFY7EBGFJJY27VZ6544B7W: the tab reads its decisions at the start,
/// each 60 seconds while it shows, and each 5 minutes while another tab
/// shows. `r` and a wake of GitHub read at once while it shows: the ask
/// of a wake waits for the tab. A tab that opens with a read 60 seconds
/// old reads at once.
#[test]
fn the_tab_reads_each_minute_and_each_5_minutes_while_another_tab_shows() {
    let mut state = mock_up();
    // The first read of the project starts at once.
    assert_eq!(state.decisions_due(NOW), Some(vec![thread(RIFF)]));
    assert_eq!(state.decisions_due(NOW), None);
    assert_eq!(state.on_key(Key::Char('r')), Some(Action::Look));
    assert!(state.decisions_due(NOW).is_some());
    assert_eq!(state.decisions_due(NOW), None);
    state.read_again(Read::Decisions);
    assert!(state.decisions_due(NOW).is_some());
    // 60 seconds after the last read.
    assert_eq!(state.decisions_due(NOW + 59_999), None);
    assert!(state.decisions_due(NOW + 60_000).is_some());
    // Another tab: the ask of a wake waits, and a read starts each 5
    // minutes.
    state.on_key(Key::Char('3'));
    state.read_again(Read::Decisions);
    assert_eq!(state.decisions_due(NOW + 120_000), None);
    assert_eq!(state.decisions_due(NOW + 359_999), None);
    assert!(state.decisions_due(NOW + 360_000).is_some());
    // The tab opens 30 seconds after the last read: no read. The read of
    // 5 minutes took the ask of the wake.
    assert_eq!(state.on_key(Key::Char('7')), None);
    assert_eq!(state.decisions_due(NOW + 390_000), None);
    // The tab opens 80 seconds after the last read: a read at once.
    state.on_key(Key::Char('3'));
    state.on_key(Key::Char('7'));
    assert!(state.decisions_due(NOW + 440_000).is_some());
    // `7` in the tab that shows asks for no read.
    state.on_key(Key::Char('7'));
    assert_eq!(state.decisions_due(NOW + 440_000), None);
}
