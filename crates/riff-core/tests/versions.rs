//! Which versions of `riff` and `riff-server` talk
//! (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK,
//! 01M3MX1E65XGWDZ062PQ9YXQ5T, 01M3MX1E8M9TKBN90P4DYKH3H8,
//! 01M55FCZNNZGKM4EKAXZQFTDWR). The tests in
//! `crates/riff/tests/build.rs` and `crates/riff-server/tests/build.rs`
//! run the same rule with the binaries.

use riff_core::build::{Build, Mismatch, UPDATE_URL, compatible, other_build, server_takes};

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

/// 01M56Q55SHHJSKJWNHMW79X7HA: on the same line, a newer riff tells
/// you to update the server, not riff.
#[test]
fn riff_0_4_3_and_riff_server_0_4_0_note_the_older_server() {
    let (riff, server) = (build("0.4.3"), build("0.4.0"));
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        "riff-server runs build 0.4.0 c040 2026-09-28T12:00:00Z; this riff runs build 0.4.3 \
         c043 2026-09-28T12:00:00Z. Update riff-server when you can."
    );
    // The same version from an older commit is the older side too.
    let older = Build {
        time: "2026-09-27T12:00:00Z".into(),
        commit: "old".into(),
        ..build("0.4.3")
    };
    assert!(other_build(&riff, &older).ends_with(" Update riff-server when you can."));
    assert!(other_build(&older, &riff).ends_with(" Run riff update when you can."));
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
    assert!(!compatible(&build("2.9.0"), &build("4.0.0")));
    assert!(!compatible(&build("0.9.0"), &build("1.0.0")));
}

/// The bridge: the lines 1 and 3 talk, in both directions. The same
/// line and the line before still talk. Each other pair is refused
/// (01M55FCZNNZGKM4EKAXZQFTDWR).
#[test]
fn the_lines_1_and_3_talk_in_both_directions() {
    // riff, riff-server, they talk.
    let pairs = [
        ("1.8.2", "3.0.0", true),
        ("3.0.0", "1.8.2", true),
        ("1.0.0", "3.4.1", true),
        ("3.4.1", "1.0.0", true),
        ("1.8.1", "1.8.2", true),
        ("1.8.2", "1.8.1", true),
        ("2.0.1", "3.0.0", true),
        ("3.0.0", "2.0.1", false),
        ("3.0.0", "4.0.0", true),
        ("4.0.0", "3.0.0", false),
        ("1.8.2", "4.0.0", false),
        ("4.0.0", "1.8.2", false),
        ("1.8.2", "2.0.0", true),
        ("2.0.0", "1.8.2", false),
        ("0.9.0", "3.0.0", false),
        ("3.0.0", "0.9.0", false),
    ];
    for (riff, server, talk) in pairs {
        assert_eq!(
            compatible(&build(riff), &build(server)),
            talk,
            "riff {riff}, riff-server {server}"
        );
    }
}

/// riff-server of the line 3 takes a riff of the line 1 only from 1.8.2,
/// the first build that knows the bridge. riff 1.8.1 refuses each reply
/// of riff-server 3, so riff-server 3 refuses its call first, and does
/// nothing. Each other pair is as for riff (01M55FCZNNZGKM4EKAXZQFTDWR).
#[test]
fn riff_server_3_takes_riff_1_only_from_the_bridge() {
    // riff, riff-server, the server takes the call.
    let pairs = [
        ("1.8.2", "3.0.0", true),
        ("1.8.9", "3.0.0", true),
        ("1.9.0", "3.4.1", true),
        ("1.8.1", "3.0.0", false),
        ("1.0.0", "3.0.0", false),
        ("1.8.1", "3.4.1", false),
        ("3.0.0", "1.8.2", true),
        ("3.4.1", "1.0.0", true),
        ("1.0.0", "1.8.2", true),
        ("1.8.2", "1.8.1", true),
        ("2.0.0", "3.0.0", true),
        ("3.0.0", "4.0.0", true),
        ("1.8.2", "4.0.0", false),
        ("0.9.0", "3.0.0", false),
    ];
    for (riff, server, takes) in pairs {
        assert_eq!(
            server_takes(&build(riff), &build(server)),
            takes,
            "riff {riff}, riff-server {server}"
        );
    }
    // The rule of riff does not change: riff 1.8.1 takes a reply of
    // riff-server 3.
    assert!(compatible(&build("1.8.1"), &build("3.0.0")));
}

/// Across the bridge, the error names each line that the server takes,
/// and the note names the older side.
#[test]
fn the_texts_name_the_bridge() {
    let refused = |riff: &str, server: &str| {
        Mismatch {
            riff: Some(build(riff)),
            server: Some(build(server)),
            seen: None,
        }
        .to_string()
    };
    let three = refused("0.9.0", "3.0.0");
    assert!(
        three.contains("riff-server 3 talks only with riff 3, 2 and 1 from 1.8.2."),
        "{three}"
    );
    // A riff of the line 1 before the bridge is told to update riff.
    let before = refused("1.8.1", "3.0.0");
    assert_eq!(
        before,
        format!(
            "this riff (1.8.1 c181 2026-09-28T12:00:00Z) and its riff-server (3.0.0 c300 \
             2026-09-28T12:00:00Z) do not match. riff-server 3 talks only with riff 3, 2 and 1 \
             from 1.8.2. Update riff on this machine, then start your sessions again. See \
             {UPDATE_URL}"
        )
    );
    let one = refused("0.9.0", "1.8.2");
    assert!(
        one.contains("riff-server 1 talks only with riff 1 and 3."),
        "{one}"
    );
    assert_eq!(
        other_build(&build("1.8.2"), &build("3.0.0")),
        "riff-server runs build 3.0.0 c300 2026-09-28T12:00:00Z; this riff runs build 1.8.2 \
         c182 2026-09-28T12:00:00Z. riff-server 4 will refuse riff 1. Run riff update soon."
    );
    assert_eq!(
        other_build(&build("3.0.0"), &build("1.8.2")),
        "riff-server runs build 1.8.2 c182 2026-09-28T12:00:00Z; this riff runs build 3.0.0 \
         c300 2026-09-28T12:00:00Z. Update riff-server when you can."
    );
}
