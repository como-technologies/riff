//! The local folder of riff is private: mode 0700, and each file in it
//! mode 0600 (01M56RB1DQ396DQ75Q23G87C28). A riff process fixes a wider
//! mode at its start, or says why it cannot
//! (01M56RB1FZ575KQYHW9FF716CZ).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use isolated::Isolated;

/// Variables of the environment: `None` removes one.
type Vars<'a> = Vec<(&'a str, Option<&'a Path>)>;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Runs a riff process that does nothing more in `env`, with the
/// variables `vars` (`None` removes one). It gives the stderr.
fn start(env: &Isolated, vars: &[(&str, Option<&Path>)]) -> String {
    let mut cmd = env.riff();
    cmd.arg("statusline")
        .env("RIFF_ON", "0")
        .stdin(std::process::Stdio::null());
    for (name, value) in vars {
        match value {
            Some(value) => cmd.env(name, value),
            None => cmd.env_remove(name),
        };
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stderr).unwrap()
}

/// A local folder with mode 0775, and a file in it with mode 0664, as
/// an older riff left them.
fn wide(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join("statusline-a6cf");
    std::fs::write(&file, "{}").unwrap();
    set(dir, 0o775);
    set(&file, 0o664);
    file
}

#[test]
fn each_place_of_the_local_folder_gets_0700_and_its_files_0600() {
    let env = Isolated::new();
    let home = env.home();
    let run = env.path().join("run");
    let state = home.join(".local/state");
    let places: [(PathBuf, Vars); 4] = [
        (env.riff_home().join("state"), vec![]),
        (run.join("riff"), vec![("RIFF_HOME", None)]),
        (
            state.join("riff"),
            vec![("RIFF_HOME", None), ("XDG_RUNTIME_DIR", None)],
        ),
        (
            state.join("riff"),
            vec![
                ("RIFF_HOME", None),
                ("XDG_RUNTIME_DIR", None),
                ("XDG_STATE_HOME", None),
            ],
        ),
    ];
    for (dir, vars) in places {
        let file = wide(&dir);
        assert_eq!(start(&env, &vars), "", "{}", dir.display());
        assert_eq!(mode(&dir), 0o700, "{}", dir.display());
        assert_eq!(mode(&file), 0o600, "{}", file.display());
    }
}

#[test]
fn a_new_file_of_riff_has_0600_in_a_folder_of_0700() {
    let run = tempfile::tempdir().unwrap();
    let dir = run.path().join("riff");
    let kept = riff::local::Kept {
        server: "http://127.0.0.1:7878".into(),
        at_ms: 1_000,
        build: None,
        line: "riff a6cf".into(),
    };
    riff::local::keep_statusline(&dir, "a6cf", &kept).unwrap();
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("statusline-a6cf")), 0o600);
}

#[test]
fn riff_says_why_it_cannot_make_the_folder_private() {
    let env = Isolated::new();
    // A file in the place of the folder: riff cannot read it as a folder.
    let state = env.riff_home().join("state");
    std::fs::create_dir_all(env.riff_home()).unwrap();
    std::fs::write(&state, "").unwrap();
    let said = start(&env, &[]);
    assert!(
        said.starts_with(&format!(
            "riff cannot make {} private (mode 0700, each file 0600): ",
            state.display()
        )),
        "{said}"
    );
}
