//! Which versions of `riff` and `riff-server` talk
//! (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK,
//! 01M3MX1E65XGWDZ062PQ9YXQ5T, 01M3MX1E8M9TKBN90P4DYKH3H8,
//! 01M5RG6BXBQVFABA1N4NRZBMQF). The tests in
//! `crates/riff/tests/build.rs` and `crates/riff-server/tests/build.rs`
//! run the same rule with the binaries. The tests of the rule take the
//! version of this build at run time: no release number is in them.

use riff_core::build::{Build, Mismatch, Semver, Side, UPDATE_URL, compatible, other_build};

/// The version of this build.
fn this() -> Semver {
    Build::this().semver().unwrap()
}

/// The line before this one, and the line before that.
fn lines_before() -> (Semver, Semver) {
    let before = this().line_before().unwrap();
    (before, before.line_before().unwrap())
}

fn at(version: Semver) -> Build {
    Build {
        version: version.to_string(),
        commit: format!("c{}", version.to_string().replace('.', "")),
        time: "2026-09-28T12:00:00Z".into(),
    }
}

fn refused(riff: &Build, server: &Build) -> String {
    Mismatch {
        riff: Some(riff.clone()),
        server: Some(server.clone()),
        seen: None,
    }
    .to_string()
}

/// The server of this build takes a `riff` of its line and of the line
/// before, and no other: two lines before and a later line do not talk.
#[test]
fn the_line_before_talks_and_two_lines_before_do_not() {
    let server = at(this());
    let (before, two_before) = lines_before();
    for riff in [
        this(),
        Semver {
            patch: this().patch + 3,
            ..this()
        },
        before,
        Semver { patch: 9, ..before },
    ] {
        assert!(compatible(&at(riff), &server), "{riff}");
    }
    for riff in [
        two_before,
        Semver {
            patch: 9,
            ..two_before
        },
        this().line_after(),
    ] {
        assert!(!compatible(&at(riff), &server), "{riff}");
    }
    assert!(!compatible(&Build::this(), &at(two_before)));
}

/// A server of the line after this one takes this `riff` as the line
/// before. A server two lines after does not.
#[test]
fn a_server_of_the_next_line_talks_and_one_two_lines_after_does_not() {
    let riff = Build::this();
    assert!(compatible(&riff, &at(this().line_after())));
    assert!(!compatible(&riff, &at(this().line_after().line_after())));
}

/// The error names the builds, each line that the server talks with, the
/// side to update and the link.
#[test]
fn the_error_names_the_lines_that_the_server_talks_with() {
    let (before, two_before) = lines_before();
    let (server, old) = (at(this()), at(two_before));
    assert_eq!(
        refused(&old, &server),
        format!(
            "this riff ({old}) and its riff-server ({server}) do not match. riff-server {} \
             talks only with riff {} and {}. Update riff on this machine, then start your \
             sessions again. See {UPDATE_URL}",
            this().line(),
            this().line(),
            before.line()
        )
    );
}

/// A riff of a later line than the server: the server is the older side.
#[test]
fn a_later_riff_is_told_to_update_the_server() {
    let (riff, server) = (at(this().line_after()), at(this()));
    assert!(!compatible(&riff, &server));
    let mismatch = Mismatch {
        riff: Some(riff.clone()),
        server: Some(server.clone()),
        seen: None,
    };
    assert_eq!(mismatch.older(), Side::Server);
    assert!(mismatch.to_string().ends_with(&format!(
        "Update riff-server, on the machine of the riff. See {UPDATE_URL}"
    )));
}

#[test]
fn the_same_line_and_a_newer_server_talk_with_the_note() {
    let riff = at(this());
    let server = at(Semver {
        patch: this().patch + 3,
        ..this()
    });
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        format!(
            "riff-server runs build {server}; this riff runs build {riff}. Run riff update \
             when you can."
        )
    );
}

/// 01M56Q55SHHJSKJWNHMW79X7HA: on the same line, a newer riff tells
/// you to update the server, not riff.
#[test]
fn the_same_line_and_a_newer_riff_note_the_older_server() {
    let riff = at(Semver {
        patch: this().patch + 3,
        ..this()
    });
    let server = at(this());
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        format!(
            "riff-server runs build {server}; this riff runs build {riff}. Update riff-server \
             when you can."
        )
    );
    // The same version from an older commit is the older side too.
    let older = Build {
        time: "2026-09-27T12:00:00Z".into(),
        commit: "old".into(),
        ..at(this())
    };
    let newer = Build {
        time: "2026-09-28T12:00:00Z".into(),
        ..at(this())
    };
    assert!(other_build(&newer, &older).ends_with(" Update riff-server when you can."));
    assert!(other_build(&older, &newer).ends_with(" Run riff update when you can."));
}

/// The line before the server talks, and the note says that the next
/// line of the server refuses it.
#[test]
fn the_line_before_talks_with_the_update_note() {
    let (before, _) = lines_before();
    let (riff, server) = (at(before), at(this()));
    assert!(compatible(&riff, &server));
    assert_eq!(
        other_build(&riff, &server),
        format!(
            "riff-server runs build {server}; this riff runs build {riff}. riff-server {} will \
             refuse riff {}. Run riff update soon.",
            this().line_after().line(),
            before.line()
        )
    );
    // The server takes one line back, not one line ahead.
    assert!(!compatible(&server, &riff));
}

/// Before the major 1, the minor is the line. After it, the major is.
/// The versions here are data of the arithmetic.
#[test]
fn before_1_0_the_minor_is_the_line_and_after_it_the_major() {
    let talk = |riff: &str, server: &str| {
        compatible(&at(riff.parse().unwrap()), &at(server.parse().unwrap()))
    };
    assert!(talk("0.4.0", "0.4.3"));
    assert!(talk("0.3.2", "0.4.0"));
    assert!(!talk("0.4.0", "0.3.2"));
    assert!(!talk("0.2.0", "0.4.0"));
    assert!(talk("1.0.0", "1.7.2"));
    assert!(talk("1.9.0", "2.0.0"));
    assert!(!talk("2.9.0", "4.0.0"));
    assert!(!talk("0.9.0", "1.0.0"));
}
