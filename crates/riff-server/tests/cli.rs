use isolated::Isolated;

#[test]
fn version_names_the_binary() {
    Isolated::shared()
        .assert_riff_server()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("riff-server {}\n", riff_core::build::VERSION));
}

#[test]
fn a_bucket_and_a_directory_do_not_go_together() {
    let assert = Isolated::shared()
        .assert_riff_server()
        .args(["--bucket", "b", "--dir", "state"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(stderr.contains("cannot be used with"), "{stderr}");
}
