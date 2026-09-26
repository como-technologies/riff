use assert_cmd::Command;

#[test]
fn version_names_the_binary() {
    Command::cargo_bin("riff-server")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(concat!("riff-server ", env!("CARGO_PKG_VERSION"), "\n"));
}
