//! The items of a wave at a restart of the host (#2114,
//! 01M7CGA8PN39YQ67S3N0MAHR7Y): "What the wave does" in `development.md`
//! says what happens to them, with the line of the log as the daemon
//! writes it, and the command that shows it.

use crate::book;

/// The how-to "At a restart of the host", up to the next heading.
fn how_to() -> String {
    let page = book::page("development.md");
    let start = page
        .find("\n### What the wave does\n")
        .expect("development.md has \"What the wave does\"");
    let wave = &page[start..];
    let heading = "\n#### At a restart of the host\n";
    let how = wave.split(heading).nth(1).expect("the how-to of a restart");
    how[..how.find("\n#").unwrap_or(how.len())].to_owned()
}

#[test]
fn the_book_says_what_happens_to_the_items_of_the_wave_at_a_restart_of_the_host() {
    let how = how_to();
    let locked = riff_client::secrets::Locked {
        host: "thelio".into(),
    };
    let line = riff::text::wave_end_locked(
        "riff-issue-12-1a2b3c4d",
        "issue-12",
        &locked.to_string(),
        std::time::Duration::from_secs(30),
    );
    assert!(
        how.contains(&format!("\n{line}\n")),
        "the how-to has no {line:?}"
    );
    let sh =
        "```sh\njournalctl --user -u riff | grep -e 'steps after the end' -e 'claims issue-'\n```";
    assert!(how.contains(sh), "the how-to has no {sh:?}");
    // The words of the how-to on one line, as the wrap breaks the lines.
    let prose = how.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "it releases the item, and ends the old session on riff-server.",
        "Then the wave starts the item again in a new session.",
        "You run no `riff host join`.",
        "the daemon tries again each 30 seconds.",
        "the wave starts no session of the item.",
    ] {
        assert!(prose.contains(words), "the how-to has no {words:?}");
    }
}

/// The claim of a wave that another session holds is no failed start
/// (01M7CGA8M0RARH6KG3PV7EYNJE): the book says it, and the line of the log
/// says it.
#[test]
fn the_book_says_that_a_claim_that_another_session_holds_does_not_count() {
    let page = book::page("development.md");
    let start = page.find("\n### What the wave does\n").unwrap();
    let wave = &page[start..];
    let wave = &wave[..wave[4..].find("\n### ").map_or(wave.len(), |i| i + 4)];
    let prose = wave.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "Another session can hold the item, or the lead can hold it.",
        "Then the start does not count either, and your lead gets no message.",
        "The daemon claims the item again 30 seconds later at the earliest.",
    ] {
        assert!(prose.contains(words), "the book has no {words:?}");
    }
    let line = riff::text::wave_not_claimed("o/r", "issue-12");
    assert!(line.ends_with("The start does not count."), "{line}");
}
