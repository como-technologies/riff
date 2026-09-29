//! Which versions of `riff` and `riff-server` talk
//! (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK,
//! 01M3MX1E65XGWDZ062PQ9YXQ5T, 01M3MX1E8M9TKBN90P4DYKH3H8). The tests in
//! `crates/riff/tests/build.rs` and `crates/riff-server/tests/build.rs`
//! run the same rule with the binaries.

use riff_core::build::{Build, Mismatch, UPDATE_URL, compatible, other_build};

fn build(version: &str) -> Build {
    Build {
        version: version.into(),
        commit: format!("c{}", version.replace('.', "")),
        time: "2026-09-28T12:00:00Z".into(),
    }
}

#[test]
fn riff_0_4_0_and_riff_server_0_4_3_talk_with_the_note() {
    let (riff, server) = (build("0.4.0"), build("0.4.3"));
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        "riff-server runs build 0.4.3 c043 2026-09-28T12:00:00Z; this riff runs build 0.4.0 \
         c040 2026-09-28T12:00:00Z. Run riff update when you can."
    );
}

#[test]
fn riff_0_3_2_and_riff_server_0_4_0_talk_with_the_update_note() {
    let (riff, server) = (build("0.3.2"), build("0.4.0"));
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        "riff-server runs build 0.4.0 c040 2026-09-28T12:00:00Z; this riff runs build 0.3.2 \
         c032 2026-09-28T12:00:00Z. riff-server 0.5 will refuse riff 0.3. Run riff update soon."
    );
    // The server takes one line back, not one line ahead.
    assert!(!compatible(&server, &riff));
}

#[test]
fn riff_0_2_0_and_riff_server_0_4_0_refuse_with_the_error_and_the_link() {
    let (riff, server) = (build("0.2.0"), build("0.4.0"));
    assert!(!compatible(&riff, &server));
    let error = Mismatch {
        riff: Some(riff),
        server: Some(server),
        seen: None,
    }
    .to_string();
    assert_eq!(
        error,
        format!(
            "this riff (0.2.0 c020 2026-09-28T12:00:00Z) and its riff-server (0.4.0 c040 \
             2026-09-28T12:00:00Z) do not match. riff-server 0.4 talks only with riff 0.4 and \
             0.3. Update riff on this machine, then start your sessions again. See {UPDATE_URL}"
        )
    );
}

#[test]
fn after_1_0_the_major_is_the_line() {
    assert!(compatible(&build("1.0.0"), &build("1.7.2")));
    assert!(compatible(&build("1.9.0"), &build("2.0.0")));
    assert!(!compatible(&build("1.9.0"), &build("3.0.0")));
    assert!(!compatible(&build("0.9.0"), &build("1.0.0")));
}
