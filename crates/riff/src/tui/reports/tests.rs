//! The screen tests of the Reports tab: the mock-ups of #994 at 80 x 24
//! and at 40 x 24, the keys, the clicks and the reads.

use super::*;
use crate::audit::{Finding, Rule, Verdict};
use crate::tui::view::{compute_view, render};
use crate::tui::{Look, News};
use crate::usage::{Counted, Kind, Tokens};
use ratatui::buffer::Buffer;
use riff_core::build::Build;

/// The time of the screen tests.
const NOW: i64 = 1_000_000_000;

/// The repository of the mock-up.
const RIFF: &str = "como-technologies/riff";

fn build(id: &str) -> Build {
    format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
}

/// An issue of a wave with one work claim for each of `models`: the
/// name of the model and its output tokens.
fn issue(number: u64, title: &str, models: &[(&str, u64)]) -> IssueUsage {
    let counted = models
        .iter()
        .enumerate()
        .map(|(i, (model, output))| Counted {
            report: usage::Report {
                item: format!("issue-{number}"),
                kind: Kind::Work,
                session: format!("a6cf012{i}"),
                from_ms: 1_790_931_600_000,
                to_ms: 1_790_935_200_000,
                models: [(
                    format!("claude-{model}"),
                    Tokens {
                        output: *output,
                        ..Tokens::default()
                    },
                )]
                .into(),
            },
            login: "mike".into(),
        })
        .collect();
    IssueUsage {
        number,
        title: title.into(),
        counted,
        uncounted: None,
    }
}

/// The usage of Wave 48 of the mock-up of #994.
fn usage() -> Usage {
    Usage {
        repo: RIFF.into(),
        wave: Wave {
            number: 48,
            title: "Wave 48".into(),
            waves: vec![46, 47, 48, 49, 50],
        },
        issues: vec![
            issue(
                994,
                "Design: the TUI of riff",
                &[("fable-5-1", 1_200_000), ("sonnet-5-5", 740_000)],
            ),
            issue(
                1590,
                "An idle look asks GitHub first",
                &[("sonnet-5-5", 3_120_000)],
            ),
            issue(
                1594,
                "riff counts the gh calls",
                &[("sonnet-5-5", 2_400_000)],
            ),
            issue(1599, "The wave takes an issue", &[("sonnet-5-5", 700_000)]),
            issue(
                1600,
                "The book tells how to detach",
                &[("sonnet-5-5", 400_000)],
            ),
            issue(
                1601,
                "Fix the help",
                &[("sonnet-5-5", 4_000_000), ("opus-5-5", 1_700_000)],
            ),
        ],
    }
}

/// The state of a TUI of mike on thelio in a clone of riff, 2 seconds
/// after a good look, with two new lines in the chat and news in the
/// waves, in the Reports tab before the first read.
fn opened() -> State {
    let me = format!("riff://mike@thelio/{RIFF}").parse().unwrap();
    let mut state = State::new(build("10df8a4c0ffe"), me).in_clone(Some(RIFF.into()));
    let look = Look {
        server: Some(build("10df8a4c0ffe")),
        ..Look::default()
    };
    state.looked(look, NOW - 2_000);
    state.news(Tab::Chat, News::Lines(2));
    state.news(Tab::Waves, News::Dot);
    state.on_key(Key::Open(Tab::Reports));
    state
}

/// The state of the mock-up of the Reports tab of #994: the usage of
/// Wave 48, read 3 minutes ago.
fn mock_up() -> State {
    let mut state = opened();
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), None)));
    state.usage_started();
    state.usage_read(None, Ok(usage()), NOW - 180_000);
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

/// The rows of `text` in the body at 80 columns: wrapped in 79 columns
/// after the margin, with no spaces at their ends.
fn shown(text: &str) -> Vec<String> {
    wrapped(text, 79, |_| Style::new())
        .iter()
        .map(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            format!(" {text}").trim_end().to_owned()
        })
        .collect()
}

/// Draws `state` at 80 x 24 and keeps what the person saw, as the loop
/// of the TUI does.
fn draw(state: &mut State) -> Vec<String> {
    let view = compute_view(state, Rect::new(0, 0, 80, 24), NOW);
    state.drawn(&view);
    drawn(state, 80, 24)
}

/// The lines of the first `text` block of `doc` after the line
/// `after`, with `strip` taken from the start of each line, and no
/// spaces at their ends.
fn text_block(doc: &str, after: &str, strip: &str) -> Vec<String> {
    let start = doc.find(after).unwrap_or_else(|| panic!("{after}"));
    doc[start..]
        .lines()
        .map(|line| {
            line.strip_prefix(strip)
                .unwrap_or(line.trim_start_matches(strip.trim_end()))
        })
        .skip_while(|line| *line != "```text")
        .skip(1)
        .take_while(|line| *line != "```")
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// The mock-up in the rustdoc of this module and in the book is the
/// screen of [`mock_up`]: the book shows the body only.
#[test]
fn the_rustdoc_and_the_book_show_the_screen() {
    let screen = drawn(&mock_up(), 80, 24);
    let rustdoc = text_block(include_str!("../reports.rs"), "//! # Design", "//! ");
    assert_eq!(rustdoc, screen);
    let book = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/development.md"
    ))
    .unwrap();
    let book = text_block(&book, "### See the tokens in the TUI", "");
    assert_eq!(book, screen[2..11]);
}

/// The mock-up of the Reports tab of #994 at 80 x 24
/// (01M6VAZKNXZV1RTG7MT0GMHKT3, 01M6VT3JFEKD2FEQGNGGKHC8X5,
/// 01M6VT3JHVFQ27VJE8987WET34): the row of the reports, the usage of the
/// current wave with the models of each issue, and the age of the read
/// in the note line.
#[test]
fn the_reports_tab_at_80_x_24_is_the_mock_up() {
    let mut want: Vec<String> = [
        " 1 Sessions  2 Chat•2  3 Waves•  4 Machines  5 Devices  [6 Reports]",
        " riff running · build 10df8a4 · link ok 2s",
        " [Usage]  Audit  Session log",
        " USAGE OF WAVE 48 · como-technologies/riff · 14.3M tokens in 6 issues",
        "   ISSUE  TITLE                            TOKENS  MODELS",
        " > #994   Design: the TUI of riff          1.9M    fable 5.1 · sonnet 5.5",
        "   #1590  An idle look asks GitHub first   3.1M    sonnet 5.5",
        "   #1594  riff counts the gh calls         2.4M    sonnet 5.5",
        "   #1599  The wave takes an issue          700k    sonnet 5.5",
        "   #1600  The book tells how to detach     400k    sonnet 5.5",
        "   #1601  Fix the help                     5.7M    sonnet 5.5 · opus 5.5",
    ]
    .map(String::from)
    .to_vec();
    want.resize(22, String::new());
    want.push(" Read from the comments of the issues with gh 3m ago.".into());
    want.push(" ↑↓ select  ←→ report  [ ] wave  Enter claims  r read again  ? help  q quit".into());
    assert_eq!(drawn(&mock_up(), 80, 24), want);
}

/// The narrow layout of the Reports tab at 40 x 24
/// (01M6VAZKK4KHCDCX4RKG9B95D7): a title with no repository, a row with
/// the issue, the tokens and the title, and a short note.
#[test]
fn the_reports_tab_at_40_x_24_is_the_narrow_mock_up() {
    let mut want: Vec<String> = [
        "‹ 6/6 Reports ›",
        "riff running · link ok 2s",
        "[Usage]  Audit  Session log",
        "WAVE 48 · 14.3M tokens in 6 issues",
        "  ISSUE  TOKENS TITLE",
        "> #994   1.9M   Design: the TUI of riff",
        "  #1590  3.1M   An idle look asks GitHu…",
        "  #1594  2.4M   riff counts the gh calls",
        "  #1599  700k   The wave takes an issue",
        "  #1600  400k   The book tells how to d…",
        "  #1601  5.7M   Fix the help",
    ]
    .map(String::from)
    .to_vec();
    want.resize(22, String::new());
    want.push("Read with gh 3m ago.".into());
    want.push("↑↓ select  ←→ report  ? help  q quit".into());
    assert_eq!(drawn(&mock_up(), 40, 24), want);
}

/// 01M6VT3JFEKD2FEQGNGGKHC8X5: `←` and `→` show the report before or
/// after, and a click on a name shows it.
#[test]
fn the_arrows_and_a_click_show_a_report() {
    let mut state = mock_up();
    state.on_key(Key::Right);
    assert_eq!(state.reports().report(), Report::Audit);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[2], " Usage  [Audit]  Session log");
    assert_eq!(rows[3], " AUDIT OF WAVE 48 · como-technologies/riff");
    let text = rows[4..8].join(" ");
    let words: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(words, AUDIT_FIRST.split_whitespace().collect::<Vec<_>>());
    assert_eq!(
        rows[23],
        " ↑↓ scroll  ←→ report  [ ] wave  Enter audit  ? help  q quit"
    );
    state.on_key(Key::Right);
    assert_eq!(state.reports().report(), Report::Log);
    state.on_key(Key::Right);
    assert_eq!(state.reports().report(), Report::Usage);
    state.on_key(Key::Left);
    assert_eq!(state.reports().report(), Report::Log);

    // A click on `Audit`, then on `Usage`.
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    let at =
        |word: &str| u16::try_from(" Usage  Audit  [Session log]".find(word).unwrap()).unwrap();
    assert_eq!(view.hit(at("Audit"), 2), Some(Key::Report(Report::Audit)));
    state.on_click(&view, at("Audit") + 2, 2, false);
    assert_eq!(state.reports().report(), Report::Audit);
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    state.on_click(&view, at("Usage"), 2, false);
    assert_eq!(state.reports().report(), Report::Usage);
    // The space between two names is no place.
    assert_eq!(view.hit(at("Usage") + 6, 2), None);
}

/// 01M6VT3JHVFQ27VJE8987WET34: `↓` selects an issue, `Enter` shows its
/// claims as `riff usage ISSUE` prints them, and `Esc` goes back. A
/// click selects a row, and a double click shows its claims.
#[test]
fn enter_shows_the_claims_of_an_issue_and_esc_goes_back() {
    let mut state = mock_up();
    state.on_key(Key::Down);
    let rows = drawn(&state, 80, 24);
    assert!(rows[6].starts_with(" > #1590"), "{rows:#?}");
    state.on_key(Key::Enter);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[3], " CLAIMS OF #1590 · An idle look asks GitHub first");
    let text = usage::issue_text(1590, &usage().issues[1].counted);
    let want = shown(&text);
    assert_eq!(rows[4..4 + want.len()], want[..], "{rows:#?}");
    assert!(rows[4].starts_with(" #1590: 3,120,000 tokens"), "{rows:#?}");
    assert_eq!(rows[23], " ↑↓ scroll  Esc back  ←→ report  ? help  q quit");
    state.on_key(Key::Esc);
    let rows = drawn(&state, 80, 24);
    assert!(rows[6].starts_with(" > #1590"), "{rows:#?}");

    // `G` selects the last issue, and a click selects a row.
    state.on_key(Key::Char('G'));
    assert!(drawn(&state, 80, 24)[10].starts_with(" > #1601"));
    let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
    assert_eq!(view.hit(20, 7), Some(Key::Row(2)));
    state.on_click(&view, 20, 7, true);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[3], " CLAIMS OF #1594 · riff counts the gh calls");
}

/// 01M6VT3JFEKD2FEQGNGGKHC8X5: `[` and `]` show the wave before or
/// after, and a wave with no read asks for one. A read of the wave
/// before counts for nothing.
#[test]
fn the_brackets_show_another_wave_and_ask_for_its_read() {
    let mut state = mock_up();
    assert_eq!(state.usage_due(), None);
    state.on_key(Key::Char(']'));
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[3], " USAGE OF WAVE 49 · como-technologies/riff");
    assert_eq!(
        rows[4],
        " riff reads the comments of the issues of the wave with gh."
    );
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), Some(49))));
    state.usage_started();
    // The read of the current wave ends late: it counts for nothing.
    state.usage_read(None, Ok(usage()), NOW);
    assert_eq!(
        drawn(&state, 80, 24)[3],
        " USAGE OF WAVE 49 · como-technologies/riff"
    );
    state.on_key(Key::Char('['));
    state.on_key(Key::Char('['));
    state.on_key(Key::Char('['));
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), Some(46))));
    state.on_key(Key::Char('['));
    assert_eq!(
        state.shown_note(),
        Some("Wave 46 is the first wave of the repository.")
    );
    for _ in 0..4 {
        state.on_key(Key::Char(']'));
    }
    state.on_key(Key::Char(']'));
    assert_eq!(
        state.shown_note(),
        Some("Wave 50 is the last wave of the repository.")
    );
    // With no read yet, riff knows no wave.
    let mut state = opened();
    state.on_key(Key::Char('['));
    assert_eq!(state.shown_note(), Some(NO_WAVES_YET));
}

/// 01M6VT3JFEKD2FEQGNGGKHC8X5, 01M6VT3JHVFQ27VJE8987WET34: the usage is
/// read when its report shows with no read yet, and on `r`, never on a
/// timer. A read that fails says why. With no repository, the report
/// says so and reads nothing.
#[test]
fn the_usage_is_read_when_it_shows_and_on_r() {
    let mut state = opened();
    state.on_key(Key::Right);
    assert_eq!(state.usage_due(), None);
    state.on_key(Key::Left);
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), None)));
    state.usage_started();
    assert_eq!(state.usage_due(), None);
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[3],
        " USAGE OF THE CURRENT WAVE · como-technologies/riff"
    );
    state.usage_read(None, Err("gh: HTTP 502".into()), NOW);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[4], " riff cannot read the usage: gh: HTTP 502");
    assert_eq!(rows[5], " Press r to read it again.");
    assert_eq!(state.usage_due(), None);
    assert_eq!(state.on_key(Key::Char('r')), Some(Action::Look));
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), None)));
    state.usage_started();
    state.usage_read(None, Ok(usage()), NOW - 1_000);
    assert_eq!(state.usage_due(), None);
    // A second `r` while a read runs asks for one more.
    state.on_key(Key::Char('r'));
    state.usage_started();
    state.on_key(Key::Char('r'));
    assert_eq!(drawn(&state, 80, 24)[22], " riff reads the usage again.");
    state.usage_read(None, Ok(usage()), NOW);
    assert_eq!(state.usage_due(), Some((RIFF.to_owned(), None)));

    // No clone and no live session: no repository.
    let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
    state.looked(Look::default(), NOW);
    state.on_key(Key::Open(Tab::Reports));
    assert_eq!(state.usage_due(), None);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[3], " USAGE OF THE CURRENT WAVE");
    let text = rows[4..6].join(" ");
    let words: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(words, NO_REPOSITORY.split_whitespace().collect::<Vec<_>>());
}

/// 01M6VT3JMABMEXSBPE414MBJZP: the audit runs only on `Enter`, shows
/// each rule as `riff audit` prints it, with the color of its verdict,
/// and runs again on `r`. It shows the count of the faults that a verify
/// missed as `riff audit` prints it too (01M7CF3P6251E29KNSBG24YJH3).
#[test]
fn the_audit_runs_on_enter_and_shows_each_rule() {
    let mut state = mock_up();
    state.on_key(Key::Right);
    assert_eq!(state.audit_due(), None);
    // `r` before an audit runs none.
    state.on_key(Key::Char('r'));
    assert_eq!(state.audit_due(), None);
    state.on_key(Key::Enter);
    assert_eq!(state.audit_due(), Some((RIFF.to_owned(), None)));
    state.audit_started();
    assert_eq!(state.audit_due(), None);
    assert_eq!(drawn(&state, 80, 24)[4], format!(" {AUDITING}"));

    let report = crate::audit::Report {
        wave: "Wave 48".into(),
        from_ms: 0,
        to_ms: None,
        pruned: None,
        rules: vec![
            Rule {
                number: 1,
                findings: vec![Finding {
                    verdict: Verdict::Fail,
                    text: "#994 has no pull request.".into(),
                }],
                none: String::new(),
            },
            Rule {
                number: 7,
                findings: Vec::new(),
                none: "no session posted a request".into(),
            },
        ],
        escaped: crate::audit::Escapes {
            counted: vec![(1001, 990)],
            no_cause: vec![],
        },
    };
    let audited = Audited {
        repo: RIFF.into(),
        wave: usage().wave,
        report: report.clone(),
    };
    state.audited(None, Ok(audited), NOW - 60_000);
    let rows = drawn(&state, 80, 24);
    let want = shown(&crate::audit::render(&report, RIFF));
    assert_eq!(rows[4..4 + want.len()], want[..], "{rows:#?}");
    let fail = rows.iter().position(|r| r == " Rule 1: fail").unwrap();
    assert!(
        rows.contains(&" Rule 7: not checked".to_owned()),
        "{rows:#?}"
    );
    assert!(rows.contains(&"   fail: #994 has no pull request.".to_owned()));
    assert!(rows.contains(&" Faults that a verify missed: 1".to_owned()));
    assert!(rows.contains(&"   #1001: the verify of #990 passed with the fault.".to_owned()));
    assert_eq!(rows[22], " The audit ran 1m ago.");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&compute_view(&state, buffer.area, NOW), &mut buffer);
    let y = u16::try_from(fail).unwrap();
    assert_eq!(buffer[(1, y)].fg, ratatui::style::Color::Red);

    // `r` runs it again, and the last audit shows while it runs.
    state.on_key(Key::Char('r'));
    assert_eq!(state.audit_due(), Some((RIFF.to_owned(), None)));
    state.audit_started();
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[22], " riff runs the audit again.");
    assert!(rows.contains(&" Rule 1: fail".to_owned()));
    // A fault says why.
    state.audited(None, Err("only an admin reads the log".into()), NOW);
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[4],
        " riff cannot audit the wave: only an admin reads the log"
    );
}

/// 01M6VT3JPQXRWMX9PWYADT4MJX: the Session log shows the lines of a
/// session, follows its end until the person scrolls up, and `End`
/// follows it again. It keeps the last lines, and says how it ended.
#[test]
fn the_session_log_follows_the_end_until_the_person_scrolls_up() {
    let mut state = mock_up();
    state.on_key(Key::Left);
    assert_eq!(state.reports().report(), Report::Log);
    let rows = drawn(&state, 80, 24);
    assert_eq!(rows[3], " SESSION LOG");
    assert_eq!(rows[4], format!(" {NO_LOG}"));
    assert_eq!(state.log_wanted(), None);

    let source = Source {
        name: "riff-issue-1590-5b1e2a90".into(),
        host: None,
    };
    state.on_key(Key::Open(Tab::Sessions));
    state.open_log(source.clone());
    assert_eq!(state.tab(), Tab::Reports);
    assert_eq!(state.log_wanted(), Some(&source));
    let rows = drawn(&state, 80, 24);
    assert_eq!(
        rows[3],
        " LOG OF riff-issue-1590-5b1e2a90 · thelio · starts"
    );
    assert_eq!(rows[4], format!(" {LOG_STARTS}"));
    state.log_started();
    for n in 0..30 {
        state.log_came(format!("line {n}"));
    }
    let rows = draw(&mut state);
    assert_eq!(
        rows[3],
        " LOG OF riff-issue-1590-5b1e2a90 · thelio · follows"
    );
    assert_eq!(rows[4], " line 12");
    assert_eq!(rows[21], " line 29");
    assert_eq!(
        rows[23],
        " ↑↓ scroll  ←→ report  End follow  ? help  q quit"
    );

    // `↑` stops the follow: a new line does not move the window.
    state.on_key(Key::Up);
    state.log_came("line 30".into());
    let rows = draw(&mut state);
    assert_eq!(rows[4], " line 11");
    assert_eq!(rows[21], " line 28");
    state.on_key(Key::First);
    assert_eq!(draw(&mut state)[4], " line 0");
    // `End` follows the end again, and so does `↓` at the end.
    state.on_key(Key::Last);
    assert_eq!(draw(&mut state)[21], " line 30");
    state.on_key(Key::PageUp);
    state.on_key(Key::PageDown);
    state.log_came("line 31".into());
    assert_eq!(draw(&mut state)[21], " line 31");

    // The end of the session: no follow, then `r` follows it again.
    state.log_ended(Ok(false));
    assert_eq!(state.log_wanted(), None);
    assert!(drawn(&state, 80, 24)[3].ends_with("· ended"));
    state.on_key(Key::Char('r'));
    assert_eq!(state.log_wanted(), Some(&source));
    state.log_ended(Ok(true));
    assert!(drawn(&state, 80, 24)[3].ends_with("· failed"));

    // A fault of the follow says why.
    state.log_ended(Err("no riff daemon answers".into()));
    let rows = draw(&mut state);
    assert!(rows[3].ends_with("· stopped"), "{rows:#?}");
    assert_eq!(
        rows[21],
        " riff cannot follow the session: no riff daemon answers"
    );

    // The log keeps the last lines.
    state.log_started();
    for n in 0..LOG_LINES + 100 {
        state.log_came(format!("line {n}"));
    }
    let log = state.reports().log().unwrap();
    assert_eq!(log.lines().count(), LOG_LINES);
    assert_eq!(log.lines().next(), Some("line 100"));

    // Another tab: no follow.
    state.log_ended(Ok(false));
    state.on_key(Key::Char('r'));
    state.on_key(Key::Open(Tab::Sessions));
    assert_eq!(state.log_wanted(), None);
    // A session of another host names its host.
    state.open_log(Source {
        name: "riff-issue-88-9c0d1e2f".into(),
        host: Some("pangolin".into()),
    });
    assert_eq!(
        drawn(&state, 80, 24)[3],
        " LOG OF riff-issue-88-9c0d1e2f · pangolin · starts"
    );
}

/// The help popup of the Reports tab names its keys.
#[test]
fn the_help_shows_the_keys_of_the_tab() {
    let mut state = mock_up();
    state.on_key(Key::Help);
    let text = drawn(&state, 80, 24).join("\n");
    assert!(text.contains("Keys of the Reports tab"), "{text}");
    for (key, does) in help_keys() {
        assert!(text.contains(key) && text.contains(does), "{key}: {text}");
    }
}
