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
//! | `riff wave list` (01M509TJRBE2J7HYKKHRJ1ZFR0) | `api repos/{owner}/{repo}/milestones?state=open&per_page=100`, then `issue list` and `pr list` for the items ([`items`]) |
//! | `riff wave make TITLE` (01M509TJTV7TC32EKXW0TEQ5NZ) | the list, `label create tracker ... --force` (01M5ND16V2SVN8A9K7W90VC6X9), then `api -X POST repos/{owner}/{repo}/milestones -f title=TITLE` |
//! | `riff wave rename N NAME` (01M518A0BEQBX9RH9H0CEFYF9Q) | the list, then `api -X PATCH repos/{owner}/{repo}/milestones/NUMBER -f title=TITLE` |
//! | `riff wave end N` (01M509TJXE04CTG5D1K751WXJY) | the list, then `api -X PATCH repos/{owner}/{repo}/milestones/NUMBER -f state=closed` |
//! | `riff wave move ITEM... --to WAVE` (01M5924014WN63RCHJWGZS4AFZ) | the list, then `issue edit N --milestone TITLE` for each item |
//! | `riff wave end N --move-open-to-backlog` (01M592403Q4TNZEDNR76C7NK9M) | the list, `api repos/{owner}/{repo}/issues?milestone=NUMBER&state=open&per_page=100`, `issue edit N --milestone Backlog` for each item, then the PATCH of `riff wave end` |
//! | `--wave WAVE` of `riff usage` and `riff audit` (01M588K2M7G0CV3RNJ4N074FTG) | `api repos/OWNER/REPO/milestones?state=all&per_page=100` |
//!
//! - `riff wave list` marks the current wave, the first wave, with
//!   `current`, and names each host whose riff daemon runs its loop.
//!   Under each wave come its open issues in the groups of the board of
//!   `riff top` ([`Items`], 01M58ATZ408RBCX0PM0D0ZCY4B). The claims, the
//!   holds and the facts of the daemons come from riff-server: `who`
//!   and `plan show`. With no read of riff-server, it shows only the
//!   milestones, and one line on stderr says why
//!   (01M58ATZ6FA5PJMD3D848CEWBJ).
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
//! - `riff wave move` and `--move-open-to-backlog` refuse an item with a
//!   claim on riff-server before they change any item. The lead needs
//!   no `gh issue edit` for the waves.
//! - `--wave` takes the number of the wave, `33` or `Wave 33`, as each
//!   other step does ([`title`]). `gh issue list --milestone` matches
//!   only the full title, for example `Wave 33: Hosts`, so riff finds
//!   it first, in the open and the closed milestones.
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

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use riff_client::sessions::Sessions;
use riff_client::waves::{Board, Issues, Milestone, TRACKER};
use riff_core::name::ThreadName;
use riff_core::wire::{HoldInfo, SessionInfo};
use serde::Deserialize;

use crate::pr::Gh;
use crate::reach::Reach;
use crate::text;

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
/// It makes the label [`TRACKER`] when it is missing
/// (01M5ND16V2SVN8A9K7W90VC6X9).
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
    // A label that is there needs no change, and no label is no reason
    // to refuse the wave.
    let _ = gh.run(
        &[
            "label",
            "create",
            TRACKER,
            "--color",
            "5319E7",
            "--description",
            text::TRACKER_LABEL,
            "--force",
        ],
        None,
    );
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

/// The title of the milestone that `to` names, from the open
/// `milestones`: `Backlog`, or a wave by its number, `6` or `Wave 6`
/// (01M5924014WN63RCHJWGZS4AFZ). It refuses a name that no open
/// milestone has.
///
/// ```
/// use riff::waves::target;
/// use riff_client::waves::Milestone;
///
/// let m = |title: &str| Milestone { number: 1, title: title.into(), open_issues: 0, closed_issues: 0 };
/// let open = [m("Wave 6: Cloud"), m("Backlog")];
/// assert_eq!(target(&open, "6").unwrap(), "Wave 6: Cloud");
/// assert_eq!(target(&open, "Wave 6").unwrap(), "Wave 6: Cloud");
/// assert_eq!(target(&open, "backlog").unwrap(), "Backlog");
/// assert!(target(&open, "7").is_err());
/// ```
pub fn target(open: &[Milestone], to: &str) -> Result<String> {
    let to = to.trim();
    if to.eq_ignore_ascii_case("backlog") {
        return match open.iter().find(|m| m.title == "Backlog") {
            Some(m) => Ok(m.title.clone()),
            None => bail!(text::waves_no_backlog()),
        };
    }
    let number = to.parse().ok().or_else(|| wave_number(to));
    match open
        .iter()
        .find(|m| number.is_some() && wave_number(&m.title) == number)
    {
        Some(m) => Ok(m.title.clone()),
        None => bail!(text::waves_move_target(to)),
    }
}

/// The numbers of the items that a session of `repo` (`OWNER/REPO`)
/// claims, in `who` of riff-server, in order.
pub fn claimed(who: &[SessionInfo], repo: &str) -> Vec<u64> {
    let mut items: Vec<u64> = who
        .iter()
        .filter(|s| s.uri.place().repo_text() == repo)
        .flat_map(|s| s.uri.claims().iter())
        .filter_map(|c| riff_client::waves::issue_of(c))
        .collect();
    items.sort_unstable();
    items.dedup();
    items
}

/// Puts the `items` in the milestone `title` with `gh issue edit`. It
/// refuses an item with a claim, and then it calls nothing.
fn put(gh: &Gh, items: &[u64], title: &str, claimed: &[u64]) -> Result<()> {
    let held: Vec<u64> = items
        .iter()
        .copied()
        .filter(|n| claimed.contains(n))
        .collect();
    if !held.is_empty() {
        bail!(text::waves_claimed(&held));
    }
    for n in items {
        gh.run(
            &["issue", "edit", &n.to_string(), "--milestone", title],
            None,
        )?;
    }
    Ok(())
}

/// Moves the `items` to `to`: a wave by its number, or `Backlog`
/// (01M5924014WN63RCHJWGZS4AFZ). `claimed` holds the items with a claim.
/// It refuses an item with a claim, and a target that no open milestone
/// has. It returns the title of the target.
pub fn move_items(gh: &Gh, items: &[u64], to: &str, claimed: &[u64]) -> Result<String> {
    let title = target(&open(gh)?, to)?;
    put(gh, items, &title, claimed)?;
    Ok(title)
}

/// Moves the open items of the open wave `number` to the Backlog, then
/// ends the wave (01M592403Q4TNZEDNR76C7NK9M). It refuses when an open
/// item has a claim, and then it moves and closes nothing. It returns
/// the ended wave and the moved items.
pub fn end_moving_open(gh: &Gh, number: u64, claimed: &[u64]) -> Result<(Milestone, Vec<u64>)> {
    let milestones = open(gh)?;
    let Some(wave) = milestones
        .iter()
        .find(|m| wave_number(&m.title) == Some(number))
    else {
        bail!(text::waves_not_open(number));
    };
    let backlog = target(&milestones, "Backlog")?;
    #[derive(Deserialize)]
    struct Item {
        number: u64,
    }
    let open_items: Vec<Item> = gh.json(&[
        "api",
        &format!(
            "repos/{{owner}}/{{repo}}/issues?milestone={}&state=open&per_page=100",
            wave.number
        ),
    ])?;
    let moved: Vec<u64> = open_items.into_iter().map(|i| i.number).collect();
    put(gh, &moved, &backlog, claimed)?;
    let ended = milestone(
        gh,
        &[
            "api",
            "-X",
            "PATCH",
            &format!("repos/{{owner}}/{{repo}}/milestones/{}", wave.number),
            "-f",
            "state=closed",
        ],
    )?;
    Ok((ended, moved))
}

/// The full title of the wave `wave`, from `milestones`: the title of
/// each milestone, open and closed, and whether it is open
/// (01M588K2M7G0CV3RNJ4N074FTG). `wave` is the number of the wave, `33`
/// or `Wave 33`, or a full title. A title that is the same wins. Else
/// the number of `wave` finds the wave, with its name or with none. It
/// refuses a wave that matches no milestone, and names the open waves.
///
/// ```
/// use riff::waves::find;
///
/// let milestones = [("Wave 32", false), ("Wave 33: Hosts", true), ("Backlog", true)];
/// for wave in ["33", "Wave 33", "Wave 33: Hosts", " Wave 33 "] {
///     assert_eq!(find(&milestones, wave).unwrap(), "Wave 33: Hosts");
/// }
/// assert_eq!(find(&milestones, "Wave 32").unwrap(), "Wave 32");
/// assert_eq!(find(&milestones, "Backlog").unwrap(), "Backlog");
/// assert_eq!(
///     find(&milestones, "Wave 3").unwrap_err().to_string(),
///     riff::text::waves_no_such("Wave 3", &["Wave 33: Hosts"])
/// );
/// ```
pub fn find(milestones: &[(&str, bool)], wave: &str) -> Result<String> {
    let wave = wave.trim();
    if let Some((title, _)) = milestones.iter().find(|(title, _)| *title == wave) {
        return Ok((*title).to_owned());
    }
    let number = wave.parse().ok().or_else(|| wave_number(wave));
    let of_number: Vec<&str> = milestones
        .iter()
        .map(|(title, _)| *title)
        .filter(|title| number.is_some() && wave_number(title) == number)
        .collect();
    match of_number[..] {
        [title] => Ok(title.to_owned()),
        [] => {
            let mut open: Vec<(u64, &str)> = milestones
                .iter()
                .filter(|(_, open)| *open)
                .filter_map(|(title, _)| wave_number(title).map(|n| (n, *title)))
                .collect();
            open.sort();
            let open: Vec<&str> = open.into_iter().map(|(_, title)| title).collect();
            bail!(text::waves_no_such(wave, &open))
        }
        _ => bail!(text::waves_many(wave, &of_number)),
    }
}

/// The full title of the wave `wave` in `repo` (`OWNER/REPO`), with
/// [`find`] over each milestone, open and closed. The title is as the
/// forge has it, so `gh` matches it: print it through
/// [`text::forge`].
pub fn title(gh: &Gh, repo: &str, wave: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Any {
        title: String,
        closed_at: Option<String>,
    }
    let all: Vec<Any> = gh.json(&[
        "api",
        &format!("repos/{repo}/milestones?state=all&per_page=100"),
    ])?;
    let milestones: Vec<(&str, bool)> = all
        .iter()
        .map(|m| (m.title.as_str(), m.closed_at.is_none()))
        .collect();
    find(&milestones, wave)
}

/// What riff-server adds to `riff wave list`
/// (01M58ATZ408RBCX0PM0D0ZCY4B): the board of each wave with an open
/// issue, and each host whose riff daemon runs the loop of the wave of
/// the repository. The loop runs the current wave.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Items {
    /// The board of each wave with an open issue, in number order.
    pub boards: Vec<Board>,
    /// Each host whose daemon runs the loop, by name.
    pub hosts: Vec<String>,
}

impl Items {
    /// The items of `repo` (`OWNER/REPO`): the boards from its open
    /// `issues`, the sessions `who` of riff-server and the `holds` of
    /// its thread, and the hosts from the facts of the daemons in `who`
    /// ([`riff_client::daemons::hosts`]).
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff::waves::Items;
    /// use riff_client::daemons::{Facts, Wave};
    /// use riff_client::waves::{Issue, Issues};
    /// use riff_core::wire::{HoldInfo, MachineInfo, SessionInfo};
    ///
    /// let session = |uri: &str, facts: Option<&Facts>| -> SessionInfo {
    ///     let mut s: SessionInfo = serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap();
    ///     s.machine = facts.map(|f| MachineInfo { numbers: f.line(), age_secs: 5 });
    ///     s
    /// };
    /// let runs = |repo: &str| Facts {
    ///     waves: vec![Wave { repo: repo.into(), word: "no free work now".into() }],
    ///     ..Facts::default()
    /// };
    /// let who = vec![
    ///     session("riff://mike@thelio/o/r?session=d1", Some(&runs("o/r"))),
    ///     session("riff://mike@pangolin/o/s?session=d2", Some(&runs("o/s"))),
    ///     session("riff://mike@thelio/o/r?session=a1&claim=issue-12", None),
    /// ];
    /// let issue = |number, wave: &str| Issue { number, title: "t".into(), milestone: Some(wave.into()), tracker: false };
    /// let issues = Issues::of(vec![issue(12, "Wave 3"), issue(13, "Wave 3"), issue(20, "Wave 4")]);
    /// let holds = BTreeMap::from([(
    ///     "issue-13".to_owned(),
    ///     HoldInfo { reason: "a reading".into(), by: None, at_ms: 0 },
    /// )]);
    /// let items = Items::of("o/r", &issues, who, &holds);
    /// assert_eq!(items.hosts, ["thelio"]);
    /// let groups: Vec<Vec<String>> = items.boards.iter().map(|b| b.groups(false)).collect();
    /// assert_eq!(groups, [vec!["held: #13 (a reading)", "claimed: #12"], vec!["free: #20"]]);
    /// ```
    pub fn of(
        repo: &str,
        issues: &Issues,
        who: Vec<SessionInfo>,
        holds: &BTreeMap<String, HoldInfo>,
    ) -> Items {
        let mut hosts: Vec<String> = riff_client::daemons::hosts(&who)
            .into_iter()
            .filter(|h| h.facts.waves.iter().any(|w| w.repo == repo))
            .map(|h| h.host)
            .collect();
        hosts.sort();
        hosts.dedup();
        let sessions = Sessions::new(who, Vec::new());
        Items {
            boards: Board::each(repo, issues, &sessions, holds),
            hosts,
        }
    }
}

/// The items of the repository `thread`, the repository of the folder
/// (01M58ATZ408RBCX0PM0D0ZCY4B): its open issues and pull requests from
/// `gh`, and its sessions and holds from riff-server, by the way `reach`.
pub async fn items(reach: &Reach, thread: Option<ThreadName>, gh: &Gh) -> Result<Items> {
    let Some(thread) = thread else {
        bail!(text::waves_no_repo());
    };
    let repo = thread.to_string();
    let who = reach.who(false).await?;
    let holds = reach.holds(&thread).await?;
    let Some(issues) = crate::top::issues_from_gh(gh, &repo) else {
        bail!(text::waves_no_issues(&repo));
    };
    Ok(Items::of(&repo, &issues, who, &holds))
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

    /// Each milestone of the repository `o/r` of the fake `gh`, open and
    /// closed: [`OPEN`] and a closed wave with a name.
    const ALL: &str = r#"[{"number":10,"title":"Backlog","closed_at":null},
        {"number":33,"title":"Wave 28","closed_at":null},
        {"number":32,"title":"Wave 27: Cloud","closed_at":null},
        {"number":31,"title":"Wave 26: Old","closed_at":"2026-10-01T00:00:00Z"}]"#;

    /// A fake `gh` in `dir`. It writes each call to `dir/calls`, one line
    /// each. It answers the list with [`OPEN`], the list of `o/r` with
    /// [`ALL`], and a POST, a PATCH of a title or another PATCH with a
    /// milestone.
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
             'api repos/o/r/milestones?state=all&per_page=100') printf '%s' '{}' ;;\n\
             'api -X POST '*) printf '%s' '{{\"number\":34,\"title\":\"Wave 29\",\"open_issues\":0,\"closed_issues\":0}}' ;;\n\
             'api -X PATCH '*'-f title='*) printf '%s' '{{\"number\":33,\"title\":\"Wave 28: Fixes\",\"open_issues\":2,\"closed_issues\":0}}' ;;\n\
             'api -X PATCH '*) printf '%s' '{{\"number\":32,\"title\":\"Wave 27: Cloud\",\"open_issues\":0,\"closed_issues\":9}}' ;;\n\
             'api repos/{{owner}}/{{repo}}/issues?milestone=33&state=open&per_page=100') printf '%s' '[{{\"number\":71}},{{\"number\":72}}]' ;;\n\
             'issue edit '*) ;;\n\
             'label create '*) ;;\n\
             *) exit 1 ;;\nesac\n",
            OPEN.replace('\n', ""),
            ALL.replace('\n', "")
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
                &format!(
                    "label create tracker --color 5319E7 --description {} --force",
                    text::TRACKER_LABEL
                ),
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
    fn move_puts_each_item_in_the_wave_by_its_number() {
        let dir = tempfile::tempdir().unwrap();
        let title = move_items(&fake_gh(dir.path()), &[71, 72], "27", &[]).unwrap();
        assert_eq!(title, "Wave 27: Cloud");
        assert_eq!(
            calls(dir.path()),
            [
                LIST,
                "issue edit 71 --milestone Wave 27: Cloud",
                "issue edit 72 --milestone Wave 27: Cloud"
            ]
        );
    }

    #[test]
    fn move_puts_the_items_in_the_backlog() {
        let dir = tempfile::tempdir().unwrap();
        let title = move_items(&fake_gh(dir.path()), &[71], "Backlog", &[]).unwrap();
        assert_eq!(title, "Backlog");
        assert_eq!(
            calls(dir.path()),
            [LIST, "issue edit 71 --milestone Backlog"]
        );
    }

    #[test]
    fn move_refuses_a_wave_that_is_not_open_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = move_items(&fake_gh(dir.path()), &[71], "26", &[]).unwrap_err();
        assert_eq!(error.to_string(), text::waves_move_target("26"));
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn move_refuses_an_item_with_a_claim_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = move_items(&fake_gh(dir.path()), &[71, 72], "27", &[72]).unwrap_err();
        assert_eq!(error.to_string(), text::waves_claimed(&[72]));
        assert_eq!(calls(dir.path()), [LIST]);
    }

    #[test]
    fn claimed_reads_the_claims_of_the_repository_only() {
        let session = |uri: &str| -> SessionInfo {
            serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
        };
        let who = vec![
            session("riff://mike@thelio/o/r?session=a1&claim=issue-12"),
            session("riff://mike@thelio/o/r?session=a2&claim=verify-issue-9"),
            session("riff://mike@thelio/o/s?session=a3&claim=issue-5"),
        ];
        assert_eq!(claimed(&who, "o/r"), [9, 12]);
    }

    #[test]
    fn end_moving_moves_the_open_items_to_the_backlog_then_closes() {
        let dir = tempfile::tempdir().unwrap();
        let (ended, moved) = end_moving_open(&fake_gh(dir.path()), 28, &[]).unwrap();
        assert_eq!(ended.title, "Wave 27: Cloud");
        assert_eq!(moved, [71, 72]);
        assert_eq!(
            calls(dir.path()),
            [
                LIST,
                "api repos/{owner}/{repo}/issues?milestone=33&state=open&per_page=100",
                "issue edit 71 --milestone Backlog",
                "issue edit 72 --milestone Backlog",
                "api -X PATCH repos/{owner}/{repo}/milestones/33 -f state=closed"
            ]
        );
    }

    #[test]
    fn end_moving_refuses_an_item_with_a_claim_and_closes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let error = end_moving_open(&fake_gh(dir.path()), 28, &[71]).unwrap_err();
        assert_eq!(error.to_string(), text::waves_claimed(&[71]));
        assert!(
            !calls(dir.path())
                .iter()
                .any(|c| c.contains("PATCH") || c.starts_with("issue edit"))
        );
    }

    #[test]
    fn end_moving_refuses_a_number_that_no_open_wave_has() {
        let dir = tempfile::tempdir().unwrap();
        let error = end_moving_open(&fake_gh(dir.path()), 26, &[]).unwrap_err();
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

    const ALL_LIST: &str = "api repos/o/r/milestones?state=all&per_page=100";

    #[test]
    fn title_finds_a_wave_with_a_name_by_its_number() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path());
        for wave in ["Wave 27", "27", "Wave 27: Cloud"] {
            assert_eq!(title(&gh, "o/r", wave).unwrap(), "Wave 27: Cloud");
        }
        assert_eq!(title(&gh, "o/r", "Wave 28").unwrap(), "Wave 28");
        assert_eq!(calls(dir.path()), [ALL_LIST; 4]);
    }

    #[test]
    fn title_finds_a_closed_wave() {
        let dir = tempfile::tempdir().unwrap();
        let found = title(&fake_gh(dir.path()), "o/r", "Wave 26").unwrap();
        assert_eq!(found, "Wave 26: Old");
    }

    #[test]
    fn title_refuses_a_wave_that_no_milestone_has_and_names_the_open_waves() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path());
        for wave in ["Wave 3", "3", "Sprint 3"] {
            let error = title(&gh, "o/r", wave).unwrap_err();
            assert_eq!(
                error.to_string(),
                text::waves_no_such(wave, &["Wave 27: Cloud", "Wave 28"])
            );
        }
    }

    /// The list marks the current wave with the host of its loop, and
    /// shows the groups of each wave: a held item is in `held` with its
    /// reason, not in `free` (01M58ATZ408RBCX0PM0D0ZCY4B). A daemon of
    /// another repository runs no loop of this one.
    #[test]
    fn the_list_marks_the_current_wave_and_where_its_loop_runs() {
        use riff_client::daemons::{Facts, Wave};
        use riff_client::waves::Issue;
        use riff_core::wire::MachineInfo;

        let session = |uri: &str, facts: Option<Facts>| -> SessionInfo {
            let mut s: SessionInfo =
                serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap();
            s.machine = facts.map(|f| MachineInfo {
                numbers: f.line(),
                age_secs: 5,
            });
            s
        };
        let runs = |repo: &str| Facts {
            waves: vec![Wave {
                repo: repo.into(),
                word: "no free work now".into(),
            }],
            ..Facts::default()
        };
        let who = vec![
            session("riff://mike@thelio/o/r?session=d1", Some(runs("o/r"))),
            session("riff://mike@pangolin/o/s?session=d2", Some(runs("o/s"))),
            session("riff://mike@thelio/o/r?session=a1&claim=issue-1103", None),
        ];
        let issue = |number, wave: &str| Issue {
            number,
            title: "t".into(),
            milestone: Some(wave.into()),
            tracker: false,
        };
        let issues = Issues::of(vec![
            issue(1003, "Wave 33: Hosts"),
            issue(1008, "Wave 33: Hosts"),
            issue(1103, "Wave 33: Hosts"),
            issue(1106, "Wave 34"),
            Issue {
                tracker: true,
                ..issue(1248, "Wave 33: Hosts")
            },
        ]);
        let hold = HoldInfo {
            reason: "VmLck reading at 19:40 UTC".into(),
            by: None,
            at_ms: 0,
        };
        let holds = BTreeMap::from([
            ("issue-1003".to_owned(), hold.clone()),
            ("issue-1008".to_owned(), hold),
        ]);
        let items = Items::of("o/r", &issues, who, &holds);
        let m = |title: &str, open, closed| Milestone {
            number: 1,
            title: title.into(),
            open_issues: open,
            closed_issues: closed,
        };
        let milestones = in_order(vec![
            m("Backlog", 61, 4),
            m("Wave 34", 1, 0),
            m("Wave 33: Hosts", 4, 21),
        ]);
        assert_eq!(
            text::waves_list(&milestones, Some(&items)),
            "Wave 33: Hosts  current  runs on thelio  4 open, 21 closed\n  \
             held: #1003 #1008 (VmLck reading at 19:40 UTC)\n  \
             claimed: #1103\n  \
             tracker: #1248\n\
             Wave 34  1 open, 0 closed\n  \
             free: #1106\n\
             Backlog  61 open, 4 closed"
        );
        // With no read of riff-server: the milestones and the mark only.
        assert_eq!(
            text::waves_list(&milestones, None),
            "Wave 33: Hosts  current  4 open, 21 closed\n\
             Wave 34  1 open, 0 closed\n\
             Backlog  61 open, 4 closed"
        );
    }

    #[test]
    fn find_refuses_a_number_of_more_than_one_milestone() {
        let milestones = [("Wave 6", false), ("Wave 6: Cloud", true)];
        let error = find(&milestones, "6").unwrap_err();
        assert_eq!(
            error.to_string(),
            text::waves_many("6", &["Wave 6", "Wave 6: Cloud"])
        );
        // The full title still finds each one.
        assert_eq!(find(&milestones, "Wave 6").unwrap(), "Wave 6");
        assert_eq!(find(&milestones, "Wave 6: Cloud").unwrap(), "Wave 6: Cloud");
    }
}
