use isolated::Isolated;

#[test]
fn version_names_the_binary() {
    Isolated::shared()
        .assert_riff()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("riff {}\n", riff_core::build::VERSION));
}

#[test]
fn login_says_when_it_cannot_reach_the_server() {
    let stderr = Isolated::shared()
        .assert_riff()
        .args(["login", "--server", "http://127.0.0.1:9"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("cannot reach riff-server"), "{stderr}");
}

#[test]
fn the_default_server_is_the_local_server() {
    let out = Isolated::shared()
        .assert_riff()
        .args(["help", "server"])
        .env_remove("RIFF_SERVER")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    assert!(
        help.contains("of this machine, http://127.0.0.1:7878."),
        "{help}"
    );
}
