//! The pool of build jobs of a machine (01M3ZGZMJ9RF1C4AHG78GQ2NM4,
//! 01M3ZGZMNH1YM56GYNYBMH7AWM). Real `cargo` builds take their jobs from
//! a pool that the test holds. The test runner of riff takes the test
//! threads from it.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use riff::jobserver::{self, Pool};

fn script(path: &Path, body: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_owned()
}

/// A workspace of `n` small library crates and one crate that needs
/// them all, so that cargo can compile `n` crates at once.
fn workspace(dir: &Path, n: usize) {
    let members: Vec<String> = (0..n).map(|i| format!("\"c{i}\"")).collect();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\"top\", {}]\n",
            members.join(", ")
        ),
    )
    .unwrap();
    let mut deps = String::new();
    for i in 0..n {
        let c = dir.join(format!("c{i}"));
        std::fs::create_dir_all(c.join("src")).unwrap();
        std::fs::write(
            c.join("Cargo.toml"),
            format!("[package]\nname = \"c{i}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        std::fs::write(c.join("src/lib.rs"), format!("pub fn f{i}() {{}}\n")).unwrap();
        deps.push_str(&format!("c{i} = {{ path = \"../c{i}\" }}\n"));
    }
    let top = dir.join("top");
    std::fs::create_dir_all(top.join("src")).unwrap();
    std::fs::write(
        top.join("Cargo.toml"),
        format!(
            "[package]\nname = \"top\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{deps}"
        ),
    )
    .unwrap();
    std::fs::write(top.join("src/lib.rs"), "").unwrap();
}

/// A `RUSTC_WRAPPER` that counts the compile jobs that run at once in
/// the directory `runs`, and writes each count to `counts`.
fn counter(dir: &Path, runs: &Path, counts: &Path) -> PathBuf {
    std::fs::create_dir_all(runs).unwrap();
    script(
        &dir.join("count-rustc"),
        &format!(
            "#!/bin/sh\n\
             touch '{runs}/run.'$$\n\
             ls '{runs}' | grep -c '^run' >> '{counts}'\n\
             sleep 0.3\n\
             \"$@\"\n\
             code=$?\n\
             rm -f '{runs}/run.'$$\n\
             exit $code\n",
            runs = runs.display(),
            counts = counts.display(),
        ),
    )
}

/// A pool of N tokens, two cargo builds at the same time: at most N
/// compile jobs at once, and one more for each build. A jobserver
/// client has one job with no token, so riff makes the pool that much
/// smaller (01M3ZGZMJ9RF1C4AHG78GQ2NM4).
#[test]
fn two_builds_take_their_jobs_from_one_pool() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    workspace(&src, 6);
    let runs = root.path().join("runs");
    let counts = root.path().join("counts");
    let wrapper = counter(root.path(), &runs, &counts);
    let pool = Pool::hold(&root.path().join("jobs"), 2, 1).unwrap();

    let build = |n: usize| {
        Command::new(env!("CARGO"))
            .args(["build", "--offline", "--quiet", "--workspace"])
            .current_dir(&src)
            .env_remove("CARGO_BUILD_JOBS")
            .env_remove("CARGO_MAKEFLAGS")
            .env_remove("MFLAGS")
            .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
            .env("MAKEFLAGS", pool.makeflags())
            .env("RUSTC_WRAPPER", &wrapper)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_TARGET_DIR", root.path().join(format!("target{n}")))
            .spawn()
            .unwrap()
    };
    let mut one = build(1);
    let mut two = build(2);
    assert!(one.wait().unwrap().success());
    assert!(two.wait().unwrap().success());

    let counts: Vec<usize> = std::fs::read_to_string(&counts)
        .unwrap()
        .lines()
        .map(|n| n.trim().parse().unwrap())
        .collect();
    let most = counts.iter().copied().max().unwrap();
    assert!(
        most <= 2 + 2,
        "a pool of 2 and 2 builds: {most} jobs at once"
    );
    assert!(
        counts.len() >= 14,
        "each crate of each build ran: {counts:?}"
    );
    assert_eq!(
        jobserver::state(&root.path().join("jobs")).map(|s| s.free),
        Some(2),
        "each build gave its tokens back"
    );
}

/// Waits until `path` exists.
fn wait_for(path: &Path) {
    let end = Instant::now() + Duration::from_secs(60);
    while !path.exists() {
        assert!(Instant::now() < end, "no {}", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A pool of N: a test run and a build at the same time use at most N
/// tokens in all. A killed test run gives its tokens back
/// (01M3ZGZMNH1YM56GYNYBMH7AWM).
#[test]
fn a_test_run_takes_its_threads_from_the_pool_and_gives_them_back() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("jobs");
    let pool = Pool::hold(&dir, 3, 1).unwrap();
    // A test program of cargo is in a `deps` directory.
    let program = script(
        &root.path().join("deps/riff-1a2b"),
        "#!/bin/sh\necho $$ > \"$1/pid.tmp\"\nmv \"$1/pid.tmp\" \"$1/pid\"\nsleep 60\n",
    );
    let mut runner = Isolated::shared()
        .riff()
        .args(["workers", "test-run"])
        .arg(&program)
        .arg(root.path())
        .env("MAKEFLAGS", pool.makeflags())
        .env("RUST_TEST_THREADS", "2")
        .spawn()
        .unwrap();
    let pid = root.path().join("pid");
    wait_for(&pid);
    assert_eq!(jobserver::state(&dir).map(|s| s.free), Some(1));

    // A build takes the rest, and no more.
    let build = jobserver::take(&pool.fifo(), 3, Duration::from_millis(200))
        .unwrap()
        .unwrap();
    assert_eq!(
        build.count(),
        1,
        "2 for the test and 1 for the build: 3 in all"
    );
    drop(build);

    let pid = std::fs::read_to_string(&pid).unwrap();
    let kill = Command::new("kill")
        .args(["-KILL", pid.trim()])
        .status()
        .unwrap();
    assert!(kill.success());
    let status = runner.wait().unwrap();
    assert_eq!(status.code(), Some(128 + 9), "{status:?}");
    assert_eq!(
        jobserver::state(&dir).map(|s| s.free),
        Some(3),
        "the killed test run gave its tokens back"
    );
}

/// A program that is not a test program, for example of `cargo run`,
/// takes no token: it can run for a long time.
#[test]
fn a_program_of_cargo_run_takes_no_token() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("jobs");
    let pool = Pool::hold(&dir, 2, 1).unwrap();
    let seen = root.path().join("seen");
    let program = script(
        &root.path().join("debug/riff"),
        &format!("#!/bin/sh\necho \"$@\" > '{}'\nexit 3\n", seen.display()),
    );
    let out = Isolated::shared()
        .riff()
        .args(["workers", "test-run"])
        .arg(&program)
        .args(["--flag", "x"])
        .env("MAKEFLAGS", pool.makeflags())
        .env("RUST_TEST_THREADS", "2")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "the exit code of the program");
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "--flag x\n");
    assert_eq!(jobserver::state(&dir).map(|s| s.free), Some(2));
}
