use assert_cmd::Command;

#[test]
fn version_names_the_binary() {
    Command::cargo_bin("sensomatic")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(concat!("sensomatic ", env!("CARGO_PKG_VERSION"), "\n"));
}
