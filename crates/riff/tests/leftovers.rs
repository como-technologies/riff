//! No test of this binary leaves a process after it ends
//! (01M77HHYZWY1WWK1T6W6KX2SHV, 01M77HHZ2FP07ES4ZYG5KYVZ7E). The guard
//! runs a sample of the tests that start a process that can outlive
//! them, with `isolated::left_after`, and fails on each process of the
//! run that is left: a `riff daemon` of a fake `systemctl`, a
//! `riff relay` of a fake `podman`, a fake `gh` that still waits.

/// The sample: tests that start a `riff daemon`, a session with its
/// `riff relay`, or a fake `gh` that waits after the end of the test.
const SAMPLE: &[&str] = &[
    "daemon::riff_daemon_install_writes_the_unit_and_starts_the_daemon",
    "first_run::riff_init_installs_the_daemon_and_the_daemon_pulls_the_image",
    "first_run::riff_daemon_status_waits_while_the_unit_starts",
    "run::a_session_image_with_an_old_claude_tells_the_lead_once",
    "tui::q_ends_the_tui_at_once_while_a_slow_read_of_gh_runs",
    "tui::q_ends_the_tui_at_once_while_the_reports_tab_reads_gh",
];

#[test]
fn no_test_of_the_sample_leaves_a_process_after_it_ends() {
    let exe = std::env::current_exe().unwrap();
    let left = isolated::left_after(&exe, SAMPLE);
    // The dir of an environment that dropped is gone: its drop missed the
    // process. The dir of an environment that did not drop is there.
    let there: Vec<bool> = left
        .iter()
        .map(|p| p.dir.as_ref().is_some_and(|dir| dir.exists()))
        .collect();
    assert!(
        left.is_empty(),
        "the tests left these processes: {left:#?}\nthe dir of each is there: {there:?}"
    );
}
