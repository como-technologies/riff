//! Retire a release line on GitHub: `riff retire LINE --issue N` (#795,
//! 01M550XYBAW5QYXEMQ49Z34M4N).
//!
//! # Design
//!
//! `riff update` installs the release that GitHub marks "Latest"
//! ([`crate::release`], 01M510MPR1S0EQNRK2195XTXG5), and GitHub never
//! marks a prerelease "Latest". So a line is retired when each GitHub
//! release of it is a prerelease, and the first line of its notes says
//! that the line is retired and names the issue
//! (01M510MPTKK285C9Z2TW54M5P7). Each step of an admin is one `riff`
//! command (#671), so `riff retire` does it for each release of the
//! line, as a thin wrapper around the `gh` of the machine. `gh` takes the
//! repository of the current folder, or `GH_REPO`.
//!
//! | Step | `gh` call |
//! |---|---|
//! | The releases, and which one is Latest | `release list --json tagName,isLatest,isDraft --limit 1000` |
//! | The notes of one release of the line | `api repos/{owner}/{repo}/releases/tags/TAG` |
//! | Retire it | `api -X PATCH repos/{owner}/{repo}/releases/ID` with `prerelease` and `body` ([`Gh::send`]) |
//!
//! A release is of the line `N` when its tag is `vN.Y.Z` ([`line_of`]).
//! riff keeps these releases of the line as they are, and says so
//! ([`Done`]):
//!
//! - The release that GitHub marks "Latest". A prerelease is never
//!   Latest, so a change leaves `riff update` with no release to install.
//!   Make a release of another line the Latest first.
//! - A draft: no person installs it.
//! - A release that is retired already: a prerelease whose notes start
//!   with [`MARK`] ([`retired`]). So a second run changes nothing.
//!
//! A release that lacks one of the two parts gets the part that it
//! lacks. The old notes stay under the new first line ([`notes`]).
//!
//! ```mermaid
//! flowchart TD
//!     L["gh release list"] --> E{"each release vN.Y.Z"}
//!     E -- "Latest, or a draft" --> K["keep it, and say so"]
//!     E -- "each other" --> V["gh api: its notes"]
//!     V --> R{"a prerelease, and the notes<br/>start with **Retired.**?"}
//!     R -- "yes" --> S["retired already: no change"]
//!     R -- "no" --> P["PATCH: a prerelease,<br/>the first line on top of the notes"]
//! ```
//!
//! ```
//! use riff::retire::{line_of, notes, retired_line};
//!
//! assert_eq!(line_of("v1.8.1"), Some(1));
//! assert_eq!(line_of("v1.8"), None);
//! let first = retired_line(2, 706);
//! assert_eq!(first, "**Retired.** The line 2.x is retired (#706). Do not install this release.");
//! assert_eq!(notes(&first, "Level: patch.\n"), format!("{first}\n\nLevel: patch.\n"));
//! ```

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::pr::Gh;
use crate::text;

/// The start of the first line of the notes of a retired release.
pub const MARK: &str = "**Retired.**";

/// The most releases that `gh release list` gives.
const LIMIT: &str = "1000";

/// One release in the list of `gh release list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub tag_name: String,
    /// GitHub marks it "Latest".
    pub is_latest: bool,
    pub is_draft: bool,
}

/// One release, as `gh api .../releases/tags/TAG` gives it: only the
/// parts that the retire reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub id: u64,
    pub prerelease: bool,
    /// The notes. GitHub gives `null` for a release with no notes.
    pub body: Option<String>,
}

/// What [`retire`] did with one release of the line, by its tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// It is a prerelease now, and its notes start with the line of
    /// [`retired_line`].
    Retired(String),
    /// It was retired already: riff changed nothing.
    Already(String),
    /// GitHub marks it "Latest": riff kept it.
    Latest(String),
    /// A draft: riff kept it.
    Draft(String),
}

/// The line `N` of the argument `N.x` or `N`, for example 1 of `1.x`.
///
/// ```
/// use riff::retire::parse_line;
///
/// assert_eq!(parse_line("1.x"), Ok(1));
/// assert_eq!(parse_line("2"), Ok(2));
/// assert!(parse_line("1.8").is_err());
/// assert!(parse_line("v1").is_err());
/// ```
pub fn parse_line(arg: &str) -> Result<u64, String> {
    let number = arg.strip_suffix(".x").unwrap_or(arg);
    match number.bytes().all(|b| b.is_ascii_digit()) {
        true => number.parse().map_err(|_| text::retire_bad_line(arg)),
        false => Err(text::retire_bad_line(arg)),
    }
}

/// The line of the release tag `tag`: `N` of `vN.Y.Z`. `None` for each
/// other tag.
pub fn line_of(tag: &str) -> Option<u64> {
    if !crate::cloud::release_tag(tag) {
        return None;
    }
    tag[1..].split('.').next()?.parse().ok()
}

/// The first line of the notes of a release of the retired line `line`,
/// with the issue that retired it.
pub fn retired_line(line: u64, issue: u64) -> String {
    format!("{MARK} The line {line}.x is retired (#{issue}). Do not install this release.")
}

/// The notes `body` with the line `first` on top. The old notes stay
/// under it.
pub fn notes(first: &str, body: &str) -> String {
    match body.trim().is_empty() {
        true => format!("{first}\n"),
        false => format!("{first}\n\n{body}"),
    }
}

/// True when the notes `body` start with [`MARK`].
pub fn marked(body: &str) -> bool {
    body.trim_start().starts_with(MARK)
}

/// True when `release` is retired: a prerelease whose notes start with
/// [`MARK`].
///
/// ```
/// use riff::retire::{Release, retired};
///
/// let release = |prerelease, body: &str| Release { id: 1, prerelease, body: Some(body.into()) };
/// assert!(retired(&release(true, "**Retired.** The line 2.x is retired (#706).")));
/// assert!(!retired(&release(false, "**Retired.** The line 2.x is retired (#706).")));
/// assert!(!retired(&release(true, "Level: patch.")));
/// ```
pub fn retired(release: &Release) -> bool {
    release.prerelease && marked(release.body.as_deref().unwrap_or_default())
}

/// Retires each GitHub release of the line `line` for the issue `issue`
/// (01M550XYBAW5QYXEMQ49Z34M4N), and gives what it did with each one, in
/// the order of `gh release list`. It fails when no release has a tag
/// of the line, and changes nothing then.
pub fn retire(gh: &Gh, line: u64, issue: u64) -> Result<Vec<Done>> {
    let list: Vec<Listed> = gh.json(&[
        "release",
        "list",
        "--json",
        "tagName,isLatest,isDraft",
        "--limit",
        LIMIT,
    ])?;
    let first = retired_line(line, issue);
    let mut done = Vec::new();
    for listed in list {
        if line_of(&listed.tag_name) != Some(line) {
            continue;
        }
        let tag = listed.tag_name;
        if listed.is_latest {
            done.push(Done::Latest(tag));
            continue;
        }
        if listed.is_draft {
            done.push(Done::Draft(tag));
            continue;
        }
        let release: Release = gh.json(&[
            "api",
            &format!("repos/{{owner}}/{{repo}}/releases/tags/{tag}"),
        ])?;
        if retired(&release) {
            done.push(Done::Already(tag));
            continue;
        }
        let body = release.body.unwrap_or_default();
        let body = match marked(&body) {
            true => body,
            false => notes(&first, &body),
        };
        gh.send(
            "PATCH",
            &format!("repos/{{owner}}/{{repo}}/releases/{}", release.id),
            &serde_json::json!({ "prerelease": true, "body": body }),
        )?;
        done.push(Done::Retired(tag));
    }
    if done.is_empty() {
        bail!(text::retire_no_release(line));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The list of the fake `gh`: the Latest release and two other
    /// releases of the line 1.x, a draft of 1.x, a release of the line
    /// 2.x and a release of the line 0.x.
    const LIST: &str = r#"[{"tagName":"v1.8.1","isLatest":true,"isDraft":false},
        {"tagName":"v1.9.0","isLatest":false,"isDraft":true},
        {"tagName":"v1.8.0","isLatest":false,"isDraft":false},
        {"tagName":"v2.0.1","isLatest":false,"isDraft":false},
        {"tagName":"v1.7.0","isLatest":false,"isDraft":false},
        {"tagName":"v0.8.0","isLatest":false,"isDraft":false}]"#;

    /// A fake GitHub API: a fake `gh` in `dir` that keeps each release
    /// in `dir/ID.json`, and its tag in `dir/TAG.id`. It writes each call
    /// to `dir/calls`, and the body of each PATCH after its call. A PATCH
    /// writes the new state of the release, so a later view gives it.
    fn fake_gh(dir: &Path) -> Gh {
        let releases = [
            ("v1.8.1", 81, false, "Level: patch."),
            ("v1.8.0", 80, false, "Level: minor."),
            (
                "v2.0.1",
                201,
                true,
                "**Retired.** The line 2.x is retired (#706).",
            ),
            ("v1.7.0", 70, false, ""),
            ("v0.8.0", 8, false, "Level: minor."),
        ];
        for (tag, id, prerelease, body) in releases {
            let release = serde_json::json!({"id": id, "prerelease": prerelease, "body": body});
            std::fs::write(dir.join(format!("{id}.json")), release.to_string()).unwrap();
            std::fs::write(dir.join(format!("{tag}.id")), id.to_string()).unwrap();
        }
        std::fs::write(dir.join("list.json"), LIST).unwrap();
        let script = "#!/bin/sh\nd=\"$(dirname \"$0\")\"\necho \"$*\" >> \"$d/calls\"\n\
             case \"$*\" in\n\
             'release list --json tagName,isLatest,isDraft --limit 1000') cat \"$d/list.json\" ;;\n\
             'api repos/{owner}/{repo}/releases/tags/'*) cat \"$d/$(cat \"$d/${2##*/}.id\").json\" ;;\n\
             'api -X PATCH repos/{owner}/{repo}/releases/'*' --include --input '*)\n\
             id=\"${4##*/}\"; cat \"$7\" >> \"$d/calls\"; echo >> \"$d/calls\"\n\
             { printf '{\"id\":%s,' \"$id\"; tail -c +2 \"$7\"; } > \"$d/$id.new\"\n\
             mv \"$d/$id.new\" \"$d/$id.json\"; printf 'HTTP/2.0 200 OK\\r\\n\\r\\n{}' ;;\n\
             *) exit 1 ;;\nesac\n";
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    fn calls(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(Into::into)
            .collect()
    }

    fn view(tag: &str) -> String {
        format!("api repos/{{owner}}/{{repo}}/releases/tags/{tag}")
    }

    const LIST_CALL: &str = "release list --json tagName,isLatest,isDraft --limit 1000";

    fn patch(id: u64) -> String {
        format!("api -X PATCH repos/{{owner}}/{{repo}}/releases/{id} --include --input ")
    }

    #[test]
    fn retire_marks_each_release_of_the_line_and_keeps_the_latest_and_the_draft() {
        let dir = tempfile::tempdir().unwrap();
        let done = retire(&fake_gh(dir.path()), 1, 961).unwrap();
        assert_eq!(
            done,
            [
                Done::Latest("v1.8.1".into()),
                Done::Draft("v1.9.0".into()),
                Done::Retired("v1.8.0".into()),
                Done::Retired("v1.7.0".into()),
            ]
        );
        let calls = calls(dir.path());
        assert_eq!(calls.len(), 7, "{calls:#?}");
        assert_eq!(calls[0], LIST_CALL);
        assert_eq!(calls[1], view("v1.8.0"));
        assert!(calls[2].starts_with(&patch(80)), "{}", calls[2]);
        let first = retired_line(1, 961);
        let sent: serde_json::Value = serde_json::from_str(&calls[3]).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"prerelease": true, "body": format!("{first}\n\nLevel: minor.")})
        );
        assert_eq!(calls[4], view("v1.7.0"));
        assert!(calls[5].starts_with(&patch(70)), "{}", calls[5]);
        // A release with no notes gets only the first line.
        let sent: serde_json::Value = serde_json::from_str(&calls[6]).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"prerelease": true, "body": format!("{first}\n")})
        );
    }

    #[test]
    fn a_release_of_another_line_stays_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path());
        let before = |id: u64| std::fs::read_to_string(dir.path().join(format!("{id}.json")));
        let (two, zero) = (before(201).unwrap(), before(8).unwrap());
        retire(&gh, 1, 961).unwrap();
        assert_eq!(before(201).unwrap(), two);
        assert_eq!(before(8).unwrap(), zero);
        let calls = calls(dir.path());
        for tag in ["v2.0.1", "v0.8.0", "v1.8.1", "v1.9.0"] {
            assert!(!calls.contains(&view(tag)), "{tag}: {calls:#?}");
        }
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path());
        retire(&gh, 1, 961).unwrap();
        std::fs::remove_file(dir.path().join("calls")).unwrap();
        let again = retire(&gh, 1, 961).unwrap();
        assert_eq!(
            again,
            [
                Done::Latest("v1.8.1".into()),
                Done::Draft("v1.9.0".into()),
                Done::Already("v1.8.0".into()),
                Done::Already("v1.7.0".into()),
            ]
        );
        let calls = calls(dir.path());
        assert!(!calls.iter().any(|c| c.contains(" -X ")), "{calls:#?}");
        // The line 2.x was retired before, by hand: no change either.
        assert_eq!(
            retire(&gh, 2, 706).unwrap(),
            [Done::Already("v2.0.1".into())]
        );
    }

    #[test]
    fn a_release_with_the_first_line_that_is_no_prerelease_keeps_its_notes() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path());
        let body = "**Retired.** The line 1.x is retired (#900).\n\nLevel: minor.";
        let release = serde_json::json!({"id": 80, "prerelease": false, "body": body});
        std::fs::write(dir.path().join("80.json"), release.to_string()).unwrap();
        retire(&gh, 1, 961).unwrap();
        let calls = calls(dir.path());
        let sent: serde_json::Value = serde_json::from_str(&calls[3]).unwrap();
        assert_eq!(sent, serde_json::json!({"prerelease": true, "body": body}));
    }

    #[test]
    fn retire_refuses_a_line_with_no_release_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = retire(&fake_gh(dir.path()), 7, 961).unwrap_err();
        assert_eq!(error.to_string(), text::retire_no_release(7));
        assert_eq!(calls(dir.path()), [LIST_CALL]);
    }
}
