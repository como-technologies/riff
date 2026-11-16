//! The tests of the reads of `gh` of each tab (#2209,
//! 01M7GFY7EBGFJJY27VZ6544B7W): a fake clock steps the loop of the TUI.

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use riff_client::waves::{Issue, Issues};
use riff_core::build::Build;
use riff_core::name::ThreadName;

use super::*;
use crate::tui::forge::Forge;
use crate::tui::view::{compute_view, render};
use crate::tui::{Key, Look};

/// The time of the start of the TUI, in milliseconds since the epoch.
const NOW: i64 = 1_794_000_000_000;

const SECOND: i64 = 1_000;

const MINUTE: i64 = 60 * SECOND;

const RIFF: &str = "como-technologies/riff";

fn build() -> Build {
    "0.1.0 10df8a4c0ffe 2026-11-06T12:00:00Z".parse().unwrap()
}

/// The TUI of mike at its start, after its first look: riff is a project
/// of mike, and the Sessions tab shows.
fn started() -> State {
    let mut state = State::new(build(), "riff://mike@thelio".parse().unwrap());
    let mut look = Look {
        server: Some(build()),
        ..Look::default()
    };
    let repos: Vec<ThreadName> = vec![RIFF.parse().unwrap()];
    look.who.repos.clone_from(&repos);
    look.who.projects = repos;
    state.looked(look, NOW);
    state
}

/// The forge of riff: the item #12 of Wave 3, with the title `title`.
fn forge(title: &str) -> Forge {
    let issue = Issue {
        number: 12,
        title: title.into(),
        milestone: Some("Wave 3".into()),
        tracker: false,
        question: false,
        needs: vec![],
    };
    Forge {
        issues: Issues::of(vec![issue]),
        milestones: BTreeMap::from([("Wave 3".to_owned(), 3)]),
        ..Forge::default()
    }
}

/// One turn of the loop of the TUI at `now_ms` for the forge: when a read
/// starts ([`State::starts`]), it reads `forge` for each board, and ends
/// at once. True when a read started.
fn turn(state: &mut State, now_ms: i64, forge: &Forge) -> bool {
    let starts = state.starts(Read::Forge, now_ms);
    if starts {
        let read = state
            .boards()
            .into_iter()
            .map(|repo| (repo, Ok(forge.clone())))
            .collect();
        state.read_forge(read, now_ms);
    }
    starts
}

/// The reads of the forge in the turns of the loop of each second from
/// `from_ms` to `to_ms`.
fn reads(state: &mut State, from_ms: i64, to_ms: i64) -> usize {
    let forge = forge("Show the board");
    (from_ms..=to_ms)
        .step_by(1_000)
        .filter(|&now_ms| turn(state, now_ms, &forge))
        .count()
}

/// The rows of `state` at `now_ms`, drawn at 80 x 24, with no spaces at
/// their ends.
fn drawn(state: &State, now_ms: i64) -> Vec<String> {
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    render(&compute_view(state, area, now_ms), &mut buffer);
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// At the start, each read of `gh` of a tab starts, whichever tab shows.
#[test]
fn each_read_starts_at_the_start_whichever_tab_shows() {
    for tab in Tab::ALL {
        let mut state = started();
        state.on_key(Key::Open(tab));
        for read in Read::ALL {
            assert!(state.starts(read, NOW), "{read:?} on the tab {tab:?}");
            assert!(!state.starts(read, NOW + SECOND), "{read:?} on {tab:?}");
        }
    }
}

/// A TUI that starts on the Sessions tab reads the forge once. A press of
/// `3` after that read shows the boards with no new read.
#[test]
fn a_tui_on_the_sessions_tab_reads_the_forge_once_and_3_shows_the_board() {
    let mut state = started();
    assert_eq!(state.tab(), Tab::Sessions);
    assert_eq!(reads(&mut state, NOW, NOW + 30 * SECOND), 1);

    state.on_key(Key::Char('3'));
    assert_eq!(state.tab(), Tab::Waves);
    assert_eq!(reads(&mut state, NOW + 31 * SECOND, NOW + 59 * SECOND), 0);
    let rows = drawn(&state, NOW + 31 * SECOND);
    assert!(
        rows[2].starts_with(" ▼ como-technologies/riff · Wave 3"),
        "{rows:#?}"
    );
    assert!(
        rows[4].starts_with(" > ○   free    #12    Show the board"),
        "{rows:#?}"
    );
    assert_eq!(rows[22], " The boards come from gh, 31s ago.", "{rows:#?}");
}

/// With the Sessions tab on screen for 10 minutes, the forge is read 3
/// times: at the start, then each 5 minutes. With the Waves tab on
/// screen, it is read each minute.
#[test]
fn another_tab_reads_the_forge_each_5_minutes_and_the_waves_tab_each_minute() {
    let mut state = started();
    assert_eq!(reads(&mut state, NOW, NOW + 10 * MINUTE), 3);

    let mut state = started();
    state.on_key(Key::Char('3'));
    assert_eq!(reads(&mut state, NOW, NOW + 10 * MINUTE), 11);
}

/// A press of `3` when the last read is 2 minutes old shows the old
/// boards with their age, and starts a read.
#[test]
fn three_with_a_read_2_minutes_old_shows_the_old_board_with_its_age_and_reads() {
    let mut state = started();
    assert!(turn(&mut state, NOW, &forge("Show the board")));
    assert_eq!(reads(&mut state, NOW + SECOND, NOW + 2 * MINUTE), 0);

    state.on_key(Key::Char('3'));
    let at = NOW + 2 * MINUTE;
    let rows = drawn(&state, at);
    assert!(
        rows[4].starts_with(" > ○   free    #12    Show the board"),
        "{rows:#?}"
    );
    assert_eq!(rows[22], " The boards come from gh, 2m ago.", "{rows:#?}");
    // The read starts at once, and its end shows the new facts.
    assert!(turn(&mut state, at, &forge("Show the new board")));
    let rows = drawn(&state, at + SECOND);
    assert!(
        rows[4].starts_with(" > ○   free    #12    Show the new board"),
        "{rows:#?}"
    );
    assert_eq!(rows[22], " The boards come from gh, 1s ago.", "{rows:#?}");
}

/// A press of `3` when the last read is 30 seconds old shows the board,
/// and starts no read: the tab reads at its time of each minute.
#[test]
fn three_with_a_read_30_seconds_old_reads_at_the_minute() {
    let mut state = started();
    assert_eq!(reads(&mut state, NOW, NOW + 30 * SECOND), 1);
    state.on_key(Key::Char('3'));
    assert_eq!(reads(&mut state, NOW + 30 * SECOND, NOW + 59 * SECOND), 0);
    assert_eq!(reads(&mut state, NOW + MINUTE, NOW + MINUTE), 1);
}

/// A wake of GitHub, `r` or the end of an action asks for a read at once
/// only while the tab shows. The ask waits for the tab, and a read of 5
/// minutes takes it.
#[test]
fn an_ask_waits_for_its_tab() {
    let mut state = started();
    assert_eq!(reads(&mut state, NOW, NOW), 1);
    for read in Read::ALL {
        state.read_again(read);
    }
    assert_eq!(reads(&mut state, NOW + SECOND, NOW + 10 * SECOND), 0);
    state.on_key(Key::Char('3'));
    assert_eq!(reads(&mut state, NOW + 11 * SECOND, NOW + 11 * SECOND), 1);
    assert_eq!(reads(&mut state, NOW + 12 * SECOND, NOW + 20 * SECOND), 0);
    // `r` in the Waves tab reads again at once.
    state.on_key(Key::Again);
    assert_eq!(reads(&mut state, NOW + 21 * SECOND, NOW + 21 * SECOND), 1);
    // The ask of the Sessions tab waits: the read of 5 minutes takes it.
    state.on_key(Key::Char('1'));
    state.read_again(Read::Forge);
    assert_eq!(reads(&mut state, NOW + 22 * SECOND, NOW + 5 * MINUTE), 0);
    assert_eq!(
        reads(
            &mut state,
            NOW + 5 * MINUTE + 21 * SECOND,
            NOW + 5 * MINUTE + 21 * SECOND
        ),
        1
    );
    state.on_key(Key::Char('3'));
    assert_eq!(
        reads(
            &mut state,
            NOW + 5 * MINUTE + 22 * SECOND,
            NOW + 5 * MINUTE + 30 * SECOND
        ),
        0
    );
}
