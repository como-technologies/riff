//! riff asks the lead for a handoff and a compact only at the end of a
//! wave, not at a later pause of the lead (01M56P28EH72XA0F5S1B3ZP4Q5,
//! #1051).
//!
//! A fake `gh` gives the milestones of the forge. Each wave has its
//! release out.

use std::path::Path;

use riff::compact::{Facts, Step, ask, decide, forge::released_wave};
use riff::pr::Gh;

/// A fake `gh` in `bin` that gives `milestones` and a release that is
/// out for each wave.
fn fake_gh(bin: &Path, milestones: &str) -> Gh {
    let path = bin.join("gh");
    let script = r#"#!/bin/sh
case "$1 $2" in
'api repos/acme/app/milestones?state=all&per_page=100') printf '%s' 'MILESTONES' ;;
'issue list')
    case "$*" in
    *'Wave 30'*) printf '%s' '[{"title":"Release v1.8.1"}]' ;;
    *'Wave 31'*) printf '%s' '[{"title":"Release v3.0.0"}]' ;;
    *) printf '[]' ;;
    esac ;;
'pr list') printf '%s' '[{"title":"Release v1.8.1"},{"title":"Release v3.0.0"}]' ;;
'run list') printf '%s' '[{"conclusion":"success"}]' ;;
esac
exit 0
"#
    .replace("MILESTONES", milestones);
    isolated::write_program(&path, script);
    Gh::at(path)
}

/// The step of a lead that is paused and idle, in the wave of `gh`.
fn step(gh: &Gh) -> Step {
    let facts = Facts {
        paused: true,
        wave: released_wave(gh, "acme/app").unwrap(),
        turn_ended: true,
        quiet: true,
        input_empty: Some(true),
        ..Facts::default()
    };
    decide(&facts)
}

#[test]
fn a_pause_of_the_lead_with_no_wave_end_asks_for_no_handoff() {
    let bin = tempfile::tempdir().unwrap();
    // Wave 30 ended hours ago. Wave 31 runs: its release is not out.
    let gh = fake_gh(
        bin.path(),
        r#"[{"title":"Wave 30: Fixes","open_issues":0,"closed_issues":29},
            {"title":"Wave 31: Build","open_issues":1,"closed_issues":30}]"#,
    );
    assert_eq!(step(&gh), Step::Wait(riff::compact::Blocker::WaveOpen));
}

#[test]
fn a_wave_end_asks_for_the_handoff_of_that_wave() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(
        bin.path(),
        r#"[{"title":"Wave 30: Fixes","open_issues":0,"closed_issues":29},
            {"title":"Wave 31: Build","open_issues":0,"closed_issues":31},
            {"title":"Wave 32: Next","open_issues":14,"closed_issues":0}]"#,
    );
    let wave = "Wave 31: Build";
    assert_eq!(step(&gh), Step::AskHandoff(wave.into()));
    let text = ask(wave);
    assert!(text.starts_with("riff: Wave 31: Build is done"), "{text}");
    assert!(!text.contains("Wave 30"), "{text}");
}
