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
