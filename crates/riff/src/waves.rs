//! The steps of the lead on the waves of a repository: see the open
//! waves, make a wave, rename a wave, and end a wave.
//!
//! # Design
//!
//! The lead plans the waves (R217). Each step is one `riff` command, a
//! thin wrapper around the `gh` of the machine. So the allow rule
//! `Bash(riff *)` covers each step, and no permission rule names a path
//! of the API of GitHub ([`crate::permissions`],
//! 01M509TJNREVNYR9MNN4P9P4RA). `gh` takes the repository of the current
//! folder, or `GH_REPO`: the path `repos/{owner}/{repo}/...` is a form
//! that `gh api` fills.
//!
//! On GitHub a wave is a milestone named `Wave N`. A name can follow, as
//! in `Wave 5: Cloud` (R223).
//!
//! | Command | `gh` calls |
//! |---|---|
//! | `riff wave list` (01M509TJRBE2J7HYKKHRJ1ZFR0) | `api repos/{owner}/{repo}/milestones?state=open&per_page=100` |
//! | `riff wave make TITLE` (01M509TJTV7TC32EKXW0TEQ5NZ) | the list, then `api -X POST repos/{owner}/{repo}/milestones -f title=TITLE` |
//! | `riff wave rename N NAME` (01M518A0BEQBX9RH9H0CEFYF9Q) | the list, then `api -X PATCH repos/{owner}/{repo}/milestones/NUMBER -f title=TITLE` |
//! | `riff wave end N` (01M509TJXE04CTG5D1K751WXJY) | the list, then `api -X PATCH repos/{owner}/{repo}/milestones/NUMBER -f state=closed` |
//!
//! - `riff wave make` makes only a wave: its title is `Wave N`, with an
//!   optional `: NAME`. It refuses a number that an open wave has.
//! - `riff wave rename N NAME` gives the open wave N the title
//!   `Wave N: NAME`. The number stays: it is the wave. The trailer
//!   `Milestone: Wave N` of a pull request names the wave by its number,
//!   so a rename breaks no pull request ([`hygiene::names_milestone`]).
//! - `riff wave end N` takes the number of the wave, not of the
//!   milestone. It refuses a wave with an open item: a wave ends only
//!   when each of its items is closed (R216, R224). On GitHub an open
//!   pull request of the wave is an open item of its milestone.
//!
//! ```mermaid
//! flowchart LR
//!     L["the lead"] -- "riff wave list" --> G["gh api: the open milestones"]
//!     L -- "riff wave make 'Wave 6'" --> P["POST: a new milestone"]
//!     L -- "riff wave rename 6 Cloud" --> T["PATCH: title Wave 6: Cloud"]
//!     L -- "riff wave end 6" --> C{"open items?"}
//!     C -- "none" --> X["PATCH: state closed"]
//!     C -- "some" --> R["refuse"]
//! ```

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::pr::Gh;
use crate::text;
use riff_client::waves::Milestone;

/// An open milestone, as `gh api .../milestones` gives it: a
/// [`Milestone`] with the texts of the forge made safe.
#[derive(Deserialize)]
struct Forge {
    number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    title: String,
    open_issues: u64,
    closed_issues: u64,
}

impl From<Forge> for Milestone {
    fn from(m: Forge) -> Milestone {
        Milestone {
            number: m.number,
            title: m.title,
            open_issues: m.open_issues,
            closed_issues: m.closed_issues,
        }
    }
}

/// The milestone in the answer of `gh args`.
fn milestone(gh: &Gh, args: &[&str]) -> Result<Milestone> {
    gh.json::<Forge>(args).map(Milestone::from)
}

pub use hygiene::wave_number;

/// `milestones` in the order of `riff wave list`: the waves in number
/// order, then each other milestone by its title.
///
/// ```
/// use riff::waves::in_order;
/// use riff_client::waves::Milestone;
///
/// let m = |title: &str| Milestone { number: 1, title: title.into(), open_issues: 0, closed_issues: 0 };
/// let list = in_order(vec![m("Backlog"), m("Wave 10"), m("Wave 9: Cloud")]);
/// let titles: Vec<&str> = list.iter().map(|m| m.title.as_str()).collect();
/// assert_eq!(titles, ["Wave 9: Cloud", "Wave 10", "Backlog"]);
/// ```
pub fn in_order(mut milestones: Vec<Milestone>) -> Vec<Milestone> {
    milestones.sort_by(|a, b| {
        let key = |m: &Milestone| {
            (
                wave_number(&m.title).map_or(u64::MAX, |n| n),
                m.title.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    milestones
}

/// The open milestones of the repository, in the order of
/// [`in_order`] (01M509TJRBE2J7HYKKHRJ1ZFR0).
pub fn open(gh: &Gh) -> Result<Vec<Milestone>> {
    let open: Vec<Forge> = gh.json(&[
        "api",
        "repos/{owner}/{repo}/milestones?state=open&per_page=100",
    ])?;
    Ok(in_order(open.into_iter().map(Milestone::from).collect()))
}

/// Makes the wave `title` (01M509TJTV7TC32EKXW0TEQ5NZ). It refuses a
/// title that is not a wave title, and a number that an open wave has.
pub fn make(gh: &Gh, title: &str) -> Result<Milestone> {
    let Some(number) = wave_number(title) else {
        bail!(text::waves_bad_title(title));
    };
    if let Some(there) = open(gh)?
        .into_iter()
        .find(|m| wave_number(&m.title) == Some(number))
    {
        bail!(text::waves_is_there(&there.title));
    }
    milestone(
        gh,
        &[
            "api",
            "-X",
            "POST",
            "repos/{owner}/{repo}/milestones",
            "-f",
            &format!("title={title}"),
        ],
    )
}

/// Gives the open wave with the number `number` the title
/// `Wave N: NAME` (01M518A0BEQBX9RH9H0CEFYF9Q). It refuses a blank name,
/// and a number that no open wave has. It returns the wave before and
/// after the rename.
pub fn rename(gh: &Gh, number: u64, name: &str) -> Result<(Milestone, Milestone)> {
    let title = format!("Wave {number}: {}", name.trim());
    if wave_number(&title) != Some(number) {
        bail!(text::waves_bad_name(name));
    }
    let Some(wave) = open(gh)?
        .into_iter()
        .find(|m| wave_number(&m.title) == Some(number))
    else {
        bail!(text::waves_not_open(number));
    };
    let renamed = milestone(
        gh,
        &[
            "api",
            "-X",
            "PATCH",
            &format!("repos/{{owner}}/{{repo}}/milestones/{}", wave.number),
            "-f",
            &format!("title={title}"),
        ],
    )?;
    Ok((wave, renamed))
}

/// Ends the open wave with the number `number` (01M509TJXE04CTG5D1K751WXJY).
/// It refuses when no open wave has the number, and when the wave has an
/// open item.
pub fn end(gh: &Gh, number: u64) -> Result<Milestone> {
    let Some(wave) = open(gh)?
        .into_iter()
        .find(|m| wave_number(&m.title) == Some(number))
    else {
        bail!(text::waves_not_open(number));
    };
    if wave.open_issues > 0 {
        bail!(text::waves_open_items(&wave.title, wave.open_issues));
    }
    milestone(
        gh,
        &[
            "api",
            "-X",
            "PATCH",
            &format!("repos/{{owner}}/{{repo}}/milestones/{}", wave.number),
            "-f",
            "state=closed",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The open milestones of the fake `gh`: a done wave, a wave with
    /// open items, and the backlog.
    const OPEN: &str = r#"[{"number":10,"title":"Backlog","open_issues":61,"closed_issues":4},
        {"number":33,"title":"Wave 28","open_issues":2,"closed_issues":0},
        {"number":32,"title":"Wave 27: Cloud","open_issues":0,"closed_issues":9}]"#;

    /// A fake `gh` in `dir`. It writes each call to `dir/calls`, one line
    /// each. It answers the list with [`OPEN`], and a POST, a PATCH of a
    /// title or another PATCH with a milestone.
    ///
    /// A child `sh` writes the file, so this process never holds a write
    /// fd of it. A child that another test thread starts gets a copy of
    /// each open file until it runs its program. With a copy of a write
    /// fd, the run of the fake fails with `ETXTBSY`
    /// (01M513N34NJ2G3TK3EH91M10FJ).
    fn fake_gh(dir: &Path) -> Gh {
        let path = dir.join("gh");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/calls\"\ncase \"$*\" in\n\
             'api repos/{{owner}}/{{repo}}/milestones?state=open&per_page=100') printf '%s' '{}' ;;\n\
             'api -X POST '*) printf '%s' '{{\"number\":34,\"title\":\"Wave 29\",\"open_issues\":0,\"closed_issues\":0}}' ;;\n\
             'api -X PATCH '*'-f title='*) printf '%s' '{{\"number\":33,\"title\":\"Wave 28: Fixes\",\"open_issues\":2,\"closed_issues\":0}}' ;;\n\
             'api -X PATCH '*) printf '%s' '{{\"number\":32,\"title\":\"Wave 27: Cloud\",\"open_issues\":0,\"closed_issues\":9}}' ;;\n\
             *) exit 1 ;;\nesac\n",
            OPEN.replace('\n', "")
        );
        let out = std::process::Command::new("sh")
            .args(["-c", "printf '%s' \"$1\" > \"$0\" && chmod 755 \"$0\""])
            .arg(&path)
            .arg(script)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        Gh::at(path)
    }

    fn calls(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(Into::into)
            .collect()
    }

    const LIST: &str = "api repos/{owner}/{repo}/milestones?state=open&per_page=100";

    #[test]
    fn open_gives_the_waves_in_number_order_then_the_backlog() {
        let dir = tempfile::tempdir().unwrap();
        let open = open(&fake_gh(dir.path())).unwrap();
        let titles: Vec<&str> = open.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["Wave 27: Cloud", "Wave 28", "Backlog"]);
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn make_posts_the_title_after_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let made = make(&fake_gh(dir.path()), "Wave 29: The view").unwrap();
        assert_eq!(made.number, 34);
        assert_eq!(
            calls(dir.path()),
            [
                LIST,
                "api -X POST repos/{owner}/{repo}/milestones -f title=Wave 29: The view"
            ]
        );
    }

    #[test]
    fn make_refuses_a_title_that_is_not_a_wave_and_calls_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = make(&fake_gh(dir.path()), "Sprint 3").unwrap_err();
        assert_eq!(error.to_string(), text::waves_bad_title("Sprint 3"));
        assert!(calls(dir.path()).is_empty());
    }

    #[test]
    fn make_refuses_the_number_of_an_open_wave() {
        let dir = tempfile::tempdir().unwrap();
        let error = make(&fake_gh(dir.path()), "Wave 28: Again").unwrap_err();
        assert_eq!(error.to_string(), text::waves_is_there("Wave 28"));
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn end_closes_the_milestone_by_its_own_number() {
        let dir = tempfile::tempdir().unwrap();
        let ended = end(&fake_gh(dir.path()), 27).unwrap();
        assert_eq!(ended.title, "Wave 27: Cloud");
        assert_eq!(
            calls(dir.path()),
            [
                LIST,
                "api -X PATCH repos/{owner}/{repo}/milestones/32 -f state=closed"
            ]
        );
    }

    #[test]
    fn end_refuses_a_wave_with_an_open_item_and_closes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = end(&fake_gh(dir.path()), 28).unwrap_err();
        assert_eq!(error.to_string(), text::waves_open_items("Wave 28", 2));
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn end_refuses_a_number_that_no_open_wave_has() {
        let dir = tempfile::tempdir().unwrap();
        let error = end(&fake_gh(dir.path()), 26).unwrap_err();
        assert_eq!(error.to_string(), text::waves_not_open(26));
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn rename_patches_the_title_of_the_milestone_by_its_own_number() {
        let dir = tempfile::tempdir().unwrap();
        let (old, new) = rename(&fake_gh(dir.path()), 28, " Fixes ").unwrap();
        assert_eq!(old.title, "Wave 28");
        assert_eq!(new.title, "Wave 28: Fixes");
        assert_eq!(
            calls(dir.path()),
            [
                LIST,
                "api -X PATCH repos/{owner}/{repo}/milestones/33 -f title=Wave 28: Fixes"
            ]
        );
    }

    #[test]
    fn rename_refuses_a_blank_name_and_calls_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = rename(&fake_gh(dir.path()), 28, "  ").unwrap_err();
        assert_eq!(error.to_string(), text::waves_bad_name("  "));
        assert!(calls(dir.path()).is_empty());
    }

    #[test]
    fn rename_refuses_a_number_that_no_open_wave_has() {
        let dir = tempfile::tempdir().unwrap();
        let error = rename(&fake_gh(dir.path()), 26, "Old").unwrap_err();
        assert_eq!(error.to_string(), text::waves_not_open(26));
        assert_eq!(calls(dir.path()), [LIST]);
    }
}
