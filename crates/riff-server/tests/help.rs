//! `riff-server --help` wraps at 80 columns and shows no value of an
//! environment variable (01M3NJDSQ23FFRMH8ZD4GC57WY).

use isolated::Isolated;

/// The stdout of `riff-server ARGS` in a terminal of 200 columns, with
/// the environment `env`.
fn help(args: &[&str], env: &[(&str, &str)]) -> String {
    let out = Isolated::shared()
        .riff_server()
        .args(args)
        .env("COLUMNS", "200")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(out.status.success(), "riff-server {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn each_line_of_the_help_fits_in_80_columns() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["log", "--help"],
        &["log", "verify", "--help"],
        &["log", "cut", "--help"],
    ] {
        for line in help(args, &[]).lines() {
            assert!(line.chars().count() <= 80, "{args:?}: {line:?}");
        }
    }
}

#[test]
fn the_help_shows_no_value_of_an_environment_variable() {
    let help = help(
        &["--help"],
        &[
            ("RIFF_OIDC_CLIENT_SECRET", "secret-value"),
            ("RIFF_BUCKET", "bucket-value"),
        ],
    );
    assert!(help.contains("RIFF_OIDC_CLIENT_SECRET]"), "{help}");
    assert!(!help.contains("secret-value"), "{help}");
    assert!(!help.contains("bucket-value"), "{help}");
}
