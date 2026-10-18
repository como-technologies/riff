//! A timed check passes a step that waits on a machine that is slow for
//! a reason other than the CPU, and fails a step that waits with no load
//! (01M49HCWEHTKSVQ6176G7C6H2S, 01M56QW660NSKW0HC5W518EZZQ).

use std::path::Path;
use std::time::Duration;

use isolated::Span;

const LIMIT: Duration = Duration::from_millis(20);

fn write(dir: &Path, file: &str, total: u64) {
    let text = format!("some avg10=0.00 avg60=0.00 avg300=0.00 total={total}\nfull total=0\n");
    std::fs::write(dir.join(file), text).unwrap();
}

/// A span over a step that waits: it uses no CPU time, as a hook that
/// waits for a slow disk.
fn waited(dir: &Path) -> Span {
    for file in ["cpu", "io", "memory"] {
        write(dir, file, 0);
    }
    let span = Span::with_pressure_dir(dir);
    std::thread::sleep(LIMIT * 2);
    span
}

#[test]
fn a_step_that_waits_while_a_task_waits_for_io_is_slow_under_load() {
    let dir = tempfile::tempdir().unwrap();
    let span = waited(dir.path());
    write(dir.path(), "io", 1_000_000);
    assert!(span.within(LIMIT), "{:?}", span.pressure());
}

#[test]
fn a_step_that_waits_while_a_task_waits_for_memory_is_slow_under_load() {
    let dir = tempfile::tempdir().unwrap();
    let span = waited(dir.path());
    write(dir.path(), "memory", 1_000_000);
    assert!(span.within(LIMIT), "{:?}", span.pressure());
}

#[test]
fn a_step_that_waits_with_no_pressure_fails() {
    let dir = tempfile::tempdir().unwrap();
    let span = waited(dir.path());
    write(dir.path(), "cpu", 100);
    write(dir.path(), "io", 100);
    assert!(!span.within(LIMIT), "{:?}", span.pressure());
}
