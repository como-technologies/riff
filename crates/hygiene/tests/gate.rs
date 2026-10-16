//! The Gate on GitHub: its parts run at the same time, together they run
//! each check of `just ci-full`, and the job `Gate` fails when a part
//! does not pass (01M3WNN7VQJKN5MJH7JN50VF4D, 01M513W2K3HNRWPQTQNBQPT8MZ,
//! 01M513W2NSE303E6XAM18BW7T6, 01M513W2RERJES76ZXKKHCV0DP,
//! 01M515Z3Y7GSFNTJGXJD727MQK). The tests read
//! `.github/workflows/ci.yml`, the justfile and `deploy/session-test.sh`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::SPAWN;

/// The parts of the Gate: the jobs that the job `gate` needs.
const PARTS: [&str; 3] = ["lint", "test", "checks"];

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workflow() -> String {
    std::fs::read_to_string(top().join(".github/workflows/ci.yml")).unwrap()
}

/// The job `name` of the CI workflow, up to the next job.
fn job(name: &str) -> String {
    let text = workflow();
    let jobs = text.split_once("\njobs:").unwrap().1;
    let job = jobs
        .split_once(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("ci.yml has no job {name}"))
        .1;
    let end = job
        .match_indices("\n  ")
        .find(|(at, _)| job[at + 3..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map_or(job.len(), |(at, _)| at);
    job[..end].to_owned()
}

/// The value of the line `key: VALUE` of `job`, at the first level of
/// the job.
fn field(job: &str, key: &str) -> String {
    let start = format!("\n    {key}: ");
    let rest = job
        .split_once(&start)
        .unwrap_or_else(|| panic!("no {key}:\n{job}"))
        .1;
    rest.lines().next().unwrap().to_owned()
}

/// The names after the colon of the recipe `name` of the justfile.
fn recipe(name: &str) -> Vec<String> {
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    let start = format!("{name}:");
    let line = justfile
        .lines()
        .find(|line| line.starts_with(&start))
        .unwrap_or_else(|| panic!("the justfile has no recipe {name}"));
    line[start.len()..]
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The step of the job `test`: a part of the tests in the session image,
/// with `just test-part` (01M515Z3Y7GSFNTJGXJD727MQK).
const TEST_STEP: &str = "\n      - run: deploy/session-test.sh --part ${{ matrix.part }}\n";

/// The recipes that the steps `run: just ...` of `job` name. The
/// recipe `test-part`, and [`TEST_STEP`], run a part of `test`.
fn recipes(job: &str) -> Vec<String> {
    job.lines()
        .filter_map(|line| line.trim().strip_prefix("- run: just "))
        .flat_map(|line| {
            line.split_whitespace()
                .take_while(|w| !w.starts_with("${{"))
        })
        .chain(job.contains(TEST_STEP).then_some("test"))
        .map(|word| if word == "test-part" { "test" } else { word })
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_parts_of_the_gate_run_each_check_of_ci_full() {
    let mut each = Vec::new();
    for part in PARTS {
        let found = recipes(&job(part));
        assert!(!found.is_empty(), "the job {part} runs no recipe");
        each.extend(found);
    }
    let ran: BTreeSet<String> = each.iter().cloned().collect();
    assert_eq!(ran.len(), each.len(), "a check runs twice: {each:?}");
    let full: BTreeSet<String> = recipe("ci-checks").into_iter().collect();
    assert_eq!(ran, full, "the Gate and just ci-full differ");
    assert!(!workflow().contains("run: just ci\n"), "the Gate runs ci");
}

#[test]
fn the_gate_needs_each_part_and_runs_on_the_same_events() {
    let gate = job("gate");
    assert_eq!(field(&gate, "name"), "Gate");
    assert_eq!(field(&gate, "needs"), format!("[{}]", PARTS.join(", ")));
    let when = "github.event_name != 'schedule' && github.ref_type != 'tag'";
    // A skipped job counts as a pass of a required check, so the Gate
    // runs when a part fails.
    assert_eq!(field(&gate, "if"), format!("always() && {when}"));
    for part in PARTS {
        assert_eq!(field(&job(part), "if"), when, "the job {part}");
    }
}

#[test]
fn the_tests_run_in_parts_that_together_hold_each_test() {
    let test = job("test");
    assert_eq!(field(&test, "name"), "Tests ${{ matrix.part }}");
    assert!(test.contains("\n      fail-fast: false\n"), "{test}");
    assert!(test.contains(TEST_STEP), "{test}");
    let script = std::fs::read_to_string(top().join("deploy/session-test.sh")).unwrap();
    assert!(
        script.contains("    run=(just test-part \"$2\")\n"),
        "{script}"
    );
    let parts = test
        .split_once("\n        part: [")
        .unwrap_or_else(|| panic!("no matrix part:\n{test}"))
        .1
        .split_once(']')
        .unwrap()
        .0;
    let parts: Vec<&str> = parts.split(", ").map(|p| p.trim_matches('"')).collect();
    let count = parts.len();
    let want: Vec<String> = (1..=count).map(|i| format!("{i}/{count}")).collect();
    assert_eq!(parts, want);
    for part in parts {
        assert!(part.parse::<hygiene::part::Part>().is_ok(), "{part}");
    }
}

#[test]
fn each_job_of_ci_runs_on_the_runner_of_riff_runner() {
    let text = workflow();
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.trim_start().starts_with("runs-on:"))
        .collect();
    assert!(lines.len() > PARTS.len(), "{lines:?}");
    for line in lines {
        assert_eq!(
            line.trim(),
            "runs-on: ${{ vars.RIFF_RUNNER || 'ubuntu-latest' }}"
        );
    }
}

/// Runs the script of the step of the job `gate` with `RESULTS`, as
/// GitHub runs it: bash with `-e`. It gives the exit code, and stdout
/// and stderr.
fn gate_step(results: &str) -> (Option<i32>, String) {
    let gate = job("gate");
    let script = gate
        .split_once("        run: |\n")
        .unwrap_or_else(|| panic!("no script:\n{gate}"))
        .1;
    let script: String = script
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .map(|line| format!("{line}\n"))
        .collect();
    let child = {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        Command::new("bash")
            .args(["--noprofile", "--norc", "-e", "-o", "pipefail", "-c"])
            .arg(&script)
            .env("RESULTS", results)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let out = child.wait_with_output().unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code(), text)
}

#[test]
fn the_gate_fails_when_a_part_does_not_pass() {
    let gate = job("gate");
    let results: Vec<String> = PARTS
        .iter()
        .map(|part| format!("{part}=${{{{ needs.{part}.result }}}}"))
        .collect();
    assert!(
        gate.contains(&format!("RESULTS: {}\n", results.join(" "))),
        "{gate}"
    );

    let all = |result: &str, at: usize| {
        PARTS
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let r = if i == at { result } else { "success" };
                format!("{part}={r}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (code, text) = gate_step(&all("success", usize::MAX));
    assert_eq!(code, Some(0), "{text}");
    for (at, part) in PARTS.iter().enumerate() {
        for result in ["failure", "cancelled", "skipped"] {
            let (code, text) = gate_step(&all(result, at));
            assert_eq!(code, Some(1), "{part}={result}: {text}");
            assert!(
                text.contains(&format!(
                    "::error::The part {part} of the Gate ended with {result}."
                )),
                "{text}"
            );
        }
    }
}
