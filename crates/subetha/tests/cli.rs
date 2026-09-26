use assert_cmd::Command;

#[test]
fn version_names_the_binary() {
    Command::cargo_bin("subetha")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(concat!("subetha ", env!("CARGO_PKG_VERSION"), "\n"));
}
