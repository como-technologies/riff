//! riff compacts the lead at the end of a wave.
//!
//! # Design
//!
//! A worker gets a fresh context after each item ([`crate::next`]). A
//! lead keeps its context over many waves, and the auto-compact of the
//! agent tool comes late and at any moment. So riff compacts the lead at
//! a safe point: the end of a wave (01M3Q88G1K7N2EMPBA07X069A7). The
//! code decides and acts. The lead does not need to remember.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead (claude)
//!     participant H as riff hook stop
//!     participant C as riff hook compact
//!     participant S as riff-server
//!     participant G as gh
//!     L->>H: the turn ends
//!     H->>C: start, detached
//!     C->>C: wait for the quiet time
//!     C->>S: paused? lead? claims? unread?
//!     C->>G: wave done? release out? open pull requests?
//!     alt no handoff asked for this wave
//!         C->>S: tell the lead, as its person: post a handoff note
//!         S->>L: wake
//!         L->>S: post "handoff: Wave N ..."
//!         L->>H: the turn ends
//!         H->>C: start again
//!         C->>S: the handoff note of the lead
//!     end
//!     C->>L: /compact INSTRUCTIONS
//! ```
//!
//! - The Stop hook of each session that is not a worker starts
//!   `riff hook compact`, detached, and returns at once. With
//!   `lead.compact` off, it starts nothing (01M3Q88GBSRJRP4VGVDV3EJZ4R).
//! - The check stops at once when the session is not the lead.
//! - [`decide`] is pure: it takes the [`Facts`] and gives the next
//!   [`Step`]. Each condition that does not hold is a [`Blocker`]
//!   (01M3Q88G45ERD0XJNYNB5C1RVN).
//! - A later Stop hook starts a new check. So a blocker ends the check;
//!   it does not wait.
//! - The [`Record`] in the local directory keeps the wave and its step,
//!   so riff compacts the lead only once for each wave. Two turns can
//!   end in the quiet time, so two checks can run at the same time. Only
//!   one acts: it holds [`crate::local::compact_lock`] from the load of
//!   the record to its save.
//! - riff asks for the handoff note as the person of the lead, the way
//!   the wrapper of a worker tells the lead (01M3Q88G6PM8KM5PR875PZXTRZ).
//!   It finds the note with a read as the person, so the unread messages
//!   of the lead stay unread.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use serde::Deserialize;

use crate::next::{Agent, ClaudeCode};
use crate::pr::Gh;
use crate::terminal::{Terminal, Tmux};
use crate::top::wave_number;

/// The start of the body of a handoff note.
pub const HANDOFF: &str = "handoff:";

/// A condition that does not hold, so riff does not compact the lead
/// now (01M3Q88G45ERD0XJNYNB5C1RVN). The order is the order of the
/// checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    /// 1. The riff runs: it is not paused.
    Running,
    /// 2. No wave is done with its release out.
    WaveOpen,
    /// 3. A session holds a claim, or a pull request of a wave is open.
    WorkInFlight,
    /// 4. The turn of the lead did not end, or it has unread messages.
    LeadBusy,
    /// 5. The person typed in the pane of the lead in the quiet time, or
    ///    its input line holds text.
    PersonHere,
    /// 6. The last message of the lead asks its user a question.
    AsksUser,
    /// 7. riff compacted the lead for this wave already.
    Done,
}

impl Blocker {
    /// The reason in words.
    ///
    /// ```
    /// use riff::compact::Blocker;
    /// assert_eq!(Blocker::Running.words(), "the riff is not paused");
    /// ```
    pub fn words(self) -> &'static str {
        match self {
            Blocker::Running => "the riff is not paused",
            Blocker::WaveOpen => "no wave is done with its release out",
            Blocker::WorkInFlight => "a claim or a pull request of a wave is open",
            Blocker::LeadBusy => "the lead is not idle",
            Blocker::PersonHere => "the person is at the pane of the lead",
            Blocker::AsksUser => "the lead asks its user a question",
            Blocker::Done => "riff compacted the lead for this wave already",
        }
    }
}

/// What riff knows when it checks.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The riff is paused.
    pub paused: bool,
    /// The last done wave with its release out ([`done_wave`],
    /// [`Release`]).
    pub wave: Option<String>,
    /// A session in the repository holds a claim, or a pull request of a
    /// wave is open.
    pub in_flight: bool,
    /// The turn of the lead ended, and no new turn started
    /// ([`turn_ended`]).
    pub turn_ended: bool,
    /// The unread messages of the lead.
    pub unread: usize,
    /// No input came to the lead for the quiet time.
    pub quiet: bool,
    /// The input line of the pane of the lead is empty. `None` with no
    /// pane.
    pub input_empty: Option<bool>,
    /// The last message of the lead asks its user ([`asks_user`]).
    pub asks_user: bool,
    /// The step of riff for each wave.
    pub record: Record,
    /// The number of the handoff note of the lead for the wave.
    pub note: Option<u64>,
}

/// The next step of riff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Do nothing now.
    Wait(Blocker),
    /// Ask the lead for a handoff note of the wave.
    AskHandoff(String),
    /// The lead has the ask; its note did not come yet.
    WaitHandoff(String),
    /// Type the compact into the pane of the lead
    /// (01M3Q88G98MKH364WEQGT4ZE7A).
    Compact { wave: String, note: u64 },
    /// Tell the lead to ask its user to compact: it has no pane.
    TellUser { wave: String, note: u64 },
}

/// The next step for `facts` (01M3Q88G45ERD0XJNYNB5C1RVN).
///
/// ```
/// use riff::compact::{decide, Blocker, Facts, Record, Step};
///
/// let ready = Facts {
///     paused: true,
///     wave: Some("Wave 13".into()),
///     turn_ended: true,
///     quiet: true,
///     input_empty: Some(true),
///     ..Facts::default()
/// };
/// assert_eq!(decide(&ready), Step::AskHandoff("Wave 13".into()));
///
/// let asked = Facts { record: Record::asked("Wave 13"), ..ready.clone() };
/// assert_eq!(decide(&asked), Step::WaitHandoff("Wave 13".into()));
///
/// let noted = Facts { note: Some(40), ..asked.clone() };
/// assert_eq!(decide(&noted), Step::Compact { wave: "Wave 13".into(), note: 40 });
///
/// let no_pane = Facts { input_empty: None, ..noted.clone() };
/// assert_eq!(decide(&no_pane), Step::TellUser { wave: "Wave 13".into(), note: 40 });
///
/// let done = Facts { record: Record::done("Wave 13"), ..noted };
/// assert_eq!(decide(&done), Step::Wait(Blocker::Done));
/// ```
pub fn decide(facts: &Facts) -> Step {
    if !facts.paused {
        return Step::Wait(Blocker::Running);
    }
    let Some(wave) = &facts.wave else {
        return Step::Wait(Blocker::WaveOpen);
    };
    if facts.in_flight {
        return Step::Wait(Blocker::WorkInFlight);
    }
    if !facts.turn_ended || facts.unread > 0 {
        return Step::Wait(Blocker::LeadBusy);
    }
    if !facts.quiet || facts.input_empty == Some(false) {
        return Step::Wait(Blocker::PersonHere);
    }
    if facts.asks_user {
        return Step::Wait(Blocker::AsksUser);
    }
    match facts.record.step(wave) {
        Some(RecordStep::Done) => Step::Wait(Blocker::Done),
        None => Step::AskHandoff(wave.clone()),
        Some(RecordStep::Asked) => match (facts.note, facts.input_empty) {
            (None, _) => Step::WaitHandoff(wave.clone()),
            (Some(note), Some(_)) => Step::Compact {
                wave: wave.clone(),
                note,
            },
            (Some(note), None) => Step::TellUser {
                wave: wave.clone(),
                note,
            },
        },
    }
}

/// The step of riff for one wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStep {
    /// riff asked the lead for its handoff note.
    Asked,
    /// riff compacted the lead, or told it to ask its user.
    Done,
}

/// The step of riff for the last wave that it acted on. It is one line
/// in the file `compact-OWNER-REPO` of the local directory: the wave, a
/// tab, and `asked` or `done`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    wave: Option<(String, RecordStep)>,
}

impl Record {
    /// riff asked for the handoff note of `wave`.
    pub fn asked(wave: &str) -> Self {
        Self {
            wave: Some((wave.to_owned(), RecordStep::Asked)),
        }
    }

    /// riff compacted the lead for `wave`.
    pub fn done(wave: &str) -> Self {
        Self {
            wave: Some((wave.to_owned(), RecordStep::Done)),
        }
    }

    /// The step of `wave`. `None` when riff did nothing for it yet.
    ///
    /// ```
    /// use riff::compact::{Record, RecordStep};
    /// let record = Record::asked("Wave 13");
    /// assert_eq!(record.step("Wave 13"), Some(RecordStep::Asked));
    /// assert_eq!(record.step("Wave 14"), None);
    /// ```
    pub fn step(&self, wave: &str) -> Option<RecordStep> {
        self.wave
            .as_ref()
            .filter(|(w, _)| w == wave)
            .map(|(_, step)| *step)
    }

    /// The record in `text`. An empty or broken text is no record.
    ///
    /// ```
    /// use riff::compact::Record;
    /// assert_eq!(Record::parse("Wave 13\tdone\n"), Record::done("Wave 13"));
    /// assert_eq!(Record::parse(&Record::asked("Wave 9").text()), Record::asked("Wave 9"));
    /// assert_eq!(Record::parse("junk"), Record::default());
    /// ```
    pub fn parse(text: &str) -> Self {
        let Some((wave, step)) = text.trim_end().split_once('\t') else {
            return Self::default();
        };
        let step = match step {
            "asked" => RecordStep::Asked,
            "done" => RecordStep::Done,
            _ => return Self::default(),
        };
        Self {
            wave: Some((wave.to_owned(), step)),
        }
    }

    /// The line of the file.
    pub fn text(&self) -> String {
        match &self.wave {
            Some((wave, RecordStep::Asked)) => format!("{wave}\tasked\n"),
            Some((wave, RecordStep::Done)) => format!("{wave}\tdone\n"),
            None => String::new(),
        }
    }

    /// The file of the record of `repo` (`OWNER/REPO`) in `dir`.
    ///
    /// ```
    /// let path = riff::compact::Record::path("/run/riff".as_ref(), "como-technologies/riff");
    /// assert_eq!(path, std::path::Path::new("/run/riff/compact-como-technologies-riff"));
    /// ```
    pub fn path(dir: &Path, repo: &str) -> PathBuf {
        dir.join(format!("compact-{}", repo.replace('/', "-")))
    }

    /// The record in `path`. No file is no record.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// Writes the record to `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.text())?;
        Ok(())
    }
}

/// A milestone in the JSON of the forge.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
    pub open_issues: u64,
    pub closed_issues: u64,
}

/// The last done wave: the wave with the highest number that has items,
/// none of them open, and no open wave before it. On GitHub an open pull
/// request of the wave is an open item of its milestone.
///
/// ```
/// use riff::compact::{done_wave, Milestone};
/// let m = |title: &str, open, closed| Milestone { title: title.into(), open_issues: open, closed_issues: closed };
///
/// let waves = [m("Wave 12", 0, 15), m("Wave 13", 0, 9), m("Wave 14", 3, 0), m("Backlog", 7, 1)];
/// assert_eq!(done_wave(&waves).as_deref(), Some("Wave 13"));
///
/// let open = [m("Wave 12", 0, 15), m("Wave 13", 1, 8)];
/// assert_eq!(done_wave(&open).as_deref(), Some("Wave 12"));
///
/// let gap = [m("Wave 12", 1, 15), m("Wave 13", 0, 9)];
/// assert_eq!(done_wave(&gap), None);
/// ```
pub fn done_wave(milestones: &[Milestone]) -> Option<String> {
    let mut waves: Vec<(u64, &Milestone)> = milestones
        .iter()
        .filter_map(|m| Some((wave_number(&m.title)?, m)))
        .collect();
    waves.sort_by_key(|(n, _)| *n);
    let first_open = waves
        .iter()
        .position(|(_, m)| m.open_issues > 0)
        .unwrap_or(waves.len());
    waves[..first_open]
        .iter()
        .rev()
        .find(|(_, m)| m.closed_issues > 0)
        .map(|(_, m)| m.title.clone())
}

/// The release of a wave: its issue `Release vX.Y.Z`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The tag, for example `v0.8.0`.
    pub tag: String,
}

impl Release {
    /// The release in the titles of the items of a wave.
    ///
    /// ```
    /// use riff::compact::Release;
    /// let titles = ["Show the wave".to_owned(), "Release v0.8.0".to_owned()];
    /// assert_eq!(Release::find(&titles).unwrap().tag, "v0.8.0");
    /// assert_eq!(Release::find(&["Release notes".to_owned()]), None);
    /// ```
    pub fn find(titles: &[String]) -> Option<Self> {
        titles.iter().find_map(|t| {
            let tag = t.strip_prefix("Release ")?;
            let version = tag.strip_prefix('v')?;
            let parts: Vec<&str> = version.split('.').collect();
            let ok = parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
            ok.then(|| Self {
                tag: tag.to_owned(),
            })
        })
    }

    /// The title of its issue and of its pull request.
    pub fn title(&self) -> String {
        format!("Release {}", self.tag)
    }
}

/// The forge part: the facts of the waves from GitHub, with `gh`. Each
/// call names the repository `repo` (`OWNER/REPO`).
pub mod forge {
    use super::*;

    #[derive(Deserialize)]
    struct Title {
        #[serde(deserialize_with = "crate::text::forge_de")]
        title: String,
    }

    #[derive(Deserialize)]
    struct Run {
        conclusion: Option<String>,
    }

    #[derive(Deserialize)]
    struct Pr {
        milestone: Option<Title>,
    }

    /// The last done wave with its release out: its issue
    /// `Release vX.Y.Z` is closed, its pull request is merged, and the CI
    /// run of its tag passed, so the deploy is done.
    pub fn released_wave(gh: &Gh, repo: &str) -> Result<Option<String>> {
        let milestones: Vec<Milestone> = gh.json(&[
            "api",
            &format!("repos/{repo}/milestones?state=all&per_page=100"),
        ])?;
        let Some(wave) = done_wave(&milestones) else {
            return Ok(None);
        };
        let issues: Vec<Title> = gh.json(&[
            "issue",
            "list",
            "-R",
            repo,
            "--milestone",
            &wave,
            "--state",
            "closed",
            "--search",
            "Release in:title",
            "--json",
            "title",
        ])?;
        let titles: Vec<String> = issues.into_iter().map(|t| t.title).collect();
        let Some(release) = Release::find(&titles) else {
            return Ok(None);
        };
        let prs: Vec<Title> = gh.json(&[
            "pr",
            "list",
            "-R",
            repo,
            "--state",
            "merged",
            "--search",
            &format!("\"{}\" in:title", release.title()),
            "--json",
            "title",
        ])?;
        if !prs.iter().any(|p| p.title == release.title()) {
            return Ok(None);
        }
        let runs: Vec<Run> = gh.json(&[
            "run",
            "list",
            "-R",
            repo,
            "--workflow",
            "CI",
            "--branch",
            &release.tag,
            "--event",
            "push",
            "--json",
            "conclusion",
        ])?;
        let deployed = runs
            .iter()
            .any(|r| r.conclusion.as_deref() == Some("success"));
        Ok(deployed.then_some(wave))
    }

    /// True when a pull request of a wave is open.
    pub fn open_wave_prs(gh: &Gh, repo: &str) -> Result<bool> {
        let prs: Vec<Pr> = gh.json(&[
            "pr",
            "list",
            "-R",
            repo,
            "--state",
            "open",
            "--json",
            "milestone",
        ])?;
        Ok(prs.iter().any(|p| {
            p.milestone
                .as_ref()
                .is_some_and(|m| wave_number(&m.title).is_some())
        }))
    }
}

/// One line of the transcript of the agent tool.
#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    message: Option<EntryMessage>,
}

#[derive(Deserialize)]
struct EntryMessage {
    #[serde(default)]
    content: serde_json::Value,
}

fn entries(transcript: &str) -> impl DoubleEndedIterator<Item = Entry> + '_ {
    transcript
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
}

/// True when the last turn in the transcript ended, and no new turn
/// started: an end-of-turn line comes after the last line of the user
/// or of the agent.
///
/// ```
/// use riff::compact::turn_ended;
/// let ended = r#"{"type":"user","message":{"content":"hi"}}
/// {"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}
/// {"type":"system","subtype":"stop_hook_summary"}
/// {"type":"system","subtype":"turn_duration"}"#;
/// assert!(turn_ended(ended));
/// let again = format!("{ended}\n{}", r#"{"type":"user","message":{"content":"more"}}"#);
/// assert!(!turn_ended(&again));
/// assert!(!turn_ended(""));
/// ```
pub fn turn_ended(transcript: &str) -> bool {
    for entry in entries(transcript).rev() {
        match (entry.kind.as_str(), entry.subtype.as_deref()) {
            ("system", Some("stop_hook_summary" | "turn_duration")) => return true,
            ("user" | "assistant", _) => return false,
            _ => {}
        }
    }
    false
}

/// The number of prompts in the transcript: the lines of the user that
/// are not the result of a tool. Each turn starts with one, so a higher
/// number shows that a new turn started.
///
/// ```
/// use riff::compact::prompts;
/// let turn = r#"{"type":"user","message":{"content":"hi"}}
/// {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}
/// {"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}
/// {"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}
/// {"type":"system","subtype":"stop_hook_summary"}"#;
/// assert_eq!(prompts(turn), 1);
/// let wake = r#"{"type":"user","message":{"content":[{"type":"text","text":"a wake"}]}}"#;
/// assert_eq!(prompts(&format!("{turn}\n{wake}")), 2);
/// assert_eq!(prompts(""), 0);
/// ```
pub fn prompts(transcript: &str) -> usize {
    entries(transcript)
        .filter(|e| e.kind == "user")
        .filter(|e| {
            let content = e.message.as_ref().map(|m| &m.content);
            let items = content.and_then(|c| c.as_array());
            !items.is_some_and(|items| items.iter().any(|c| c["type"] == "tool_result"))
        })
        .count()
}

/// The text of the last message of the agent in the transcript.
///
/// ```
/// use riff::compact::last_text;
/// let t = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"One."}]}}
/// {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#;
/// assert_eq!(last_text(t).as_deref(), Some("One."));
/// ```
pub fn last_text(transcript: &str) -> Option<String> {
    entries(transcript)
        .rev()
        .filter(|e| e.kind == "assistant")
        .find_map(|e| {
            let content = e.message?.content;
            let text: Vec<&str> = content
                .as_array()?
                .iter()
                .filter(|c| c["type"] == "text")
                .filter_map(|c| c["text"].as_str())
                .collect();
            (!text.is_empty()).then(|| text.join("\n"))
        })
}

/// True when `text` asks the user a question: its last line ends with a
/// question mark.
///
/// ```
/// use riff::compact::asks_user;
/// assert!(asks_user("Wave 13 is done.\n\nShall I start Wave 14?"));
/// assert!(asks_user("Which one: **A** or **B?**"));
/// assert!(!asks_user("Wave 13 is done. What is next? The board says Wave 14."));
/// ```
pub fn asks_user(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .is_some_and(|l| l.trim_end_matches(['*', '_', '`', ')']).ends_with('?'))
}

/// The time since the last change of `path`. `None` when riff cannot
/// read it.
pub fn age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default(),
    )
}

/// The message that asks the lead for its handoff note
/// (01M3Q88G6PM8KM5PR875PZXTRZ).
///
/// ```
/// let ask = riff::compact::ask("Wave 13");
/// assert!(ask.contains("handoff: Wave 13"), "{ask}");
/// ```
pub fn ask(wave: &str) -> String {
    format!(
        "riff: {wave} is done and the riff is paused. riff compacts your context now. First \
         post a handoff note: call the riff post tool with kind note, to the repository, and \
         a body that starts with `{HANDOFF} {wave}`. Say the state, the next wave, and the \
         open decisions for your user. Then end your turn. Start no other work."
    )
}

/// The message that tells a lead with no pane to ask its user to
/// compact (01M3Q88G98MKH364WEQGT4ZE7A).
///
/// ```
/// let tell = riff::compact::tell_user("Wave 13", 40);
/// assert!(tell.contains("/compact") && tell.contains("message 40"), "{tell}");
/// ```
pub fn tell_user(wave: &str, note: u64) -> String {
    format!(
        "riff: {wave} is done. Your handoff note is message {note}. Tell your user to run \
         /compact in this session now. riff cannot type it: this session does not run in \
         tmux."
    )
}

/// The instructions for the compact: what to keep and what to drop.
///
/// ```
/// let text = riff::compact::instructions("mike", "como-technologies/riff", 40);
/// assert!(text.contains("lead of mike in como-technologies/riff"), "{text}");
/// assert!(text.contains("message 40"), "{text}");
/// assert!(!text.contains('\n'));
/// ```
pub fn instructions(user: &str, repo: &str, note: u64) -> String {
    format!(
        "You are the riff lead of {user} in {repo}. Keep: who is the owner and who are the \
         admins; the rules of your user (memory); the next wave and its items; the open \
         decisions and questions for your user; the handoff note (message {note}). Drop: tool \
         output, verify details, the details of merged pull requests, the messages of \
         finished items. The riff state is on the server and in the forge: after the \
         compact, call the riff read tool, and start the watch again."
    )
}

/// The number of the handoff note of `lead` for `wave` in `messages`.
///
/// ```
/// use riff::compact::handoff_note;
/// use riff_core::wire::Message;
/// let lead: riff_core::name::SessionUri =
///     "riff://mike@thelio/como-technologies/riff?session=l1".parse().unwrap();
/// let other: riff_core::name::SessionUri =
///     "riff://mike@thelio/como-technologies/riff?session=w1".parse().unwrap();
/// let msg = |seq, from: &riff_core::name::SessionUri, body: &str| Message {
///     seq, from: from.clone(), to: vec![], body: body.into(), at_ms: 0,
///     kind: Default::default(), sig: None, payload: None,
/// };
/// let messages = [
///     msg(3, &lead, "handoff: Wave 12. Old."),
///     msg(7, &other, "handoff: Wave 13. Not the lead."),
///     msg(9, &lead, "handoff: Wave 13. Next: Wave 14."),
/// ];
/// assert_eq!(handoff_note(&messages, lead.who(), "Wave 13"), Some(9));
/// assert_eq!(handoff_note(&messages, lead.who(), "Wave 14"), None);
/// ```
pub fn handoff_note(
    messages: &[riff_core::wire::Message],
    lead: &riff_core::name::Who,
    wave: &str,
) -> Option<u64> {
    let start = format!("{HANDOFF} {wave}");
    messages
        .iter()
        .rev()
        .find(|m| {
            m.from.who() == lead
                && m.body
                    .trim_start()
                    .strip_prefix(&start)
                    .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()))
        })
        .map(|m| m.seq)
}

/// What the Stop hook gives to one check.
#[derive(Debug, Clone)]
pub struct Check {
    /// The session ID of the agent tool.
    pub session: String,
    /// The transcript of the session.
    pub transcript: Option<PathBuf>,
    /// The tmux pane of the session.
    pub pane: Option<String>,
}

/// The longest time that one check waits for the quiet time.
pub const MAX_WAIT: Duration = Duration::from_secs(600);

/// The facts of the transcript: the turn ended, no input came for
/// `quiet`, and the last message asks the user.
fn transcript_facts(facts: &mut Facts, transcript: Option<&Path>, quiet: Duration) {
    let Some(path) = transcript else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    facts.turn_ended = turn_ended(&text);
    facts.quiet = age(path).is_some_and(|a| a >= quiet);
    facts.asks_user = last_text(&text).is_some_and(|t| asks_user(&t));
}

/// Waits until the transcript had no change for `quiet`, or the turn
/// of the lead did not end. It stops after [`MAX_WAIT`].
async fn wait_quiet(transcript: Option<&Path>, quiet: Duration) {
    let Some(path) = transcript else {
        return;
    };
    let end = tokio::time::Instant::now() + MAX_WAIT;
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let Some(age) = age(path) else {
            return;
        };
        if !turn_ended(&text) || age >= quiet || tokio::time::Instant::now() >= end {
            return;
        }
        tokio::time::sleep(quiet - age + Duration::from_millis(50)).await;
    }
}

/// One check after a turn of a session ends. It waits for the quiet
/// time, finds the [`Facts`], and does the [`Step`] of [`decide`].
/// `None` when riff compacts no lead on this machine, the session is
/// not the lead, or another check acts now.
pub async fn run(check: &Check, server: &str) -> Result<Option<Step>> {
    use crate::{api::Api, identity, local, settings, worker};
    use riff_core::wire::RiffState;

    let settings = settings::path()?;
    if !settings::lead_compact(&settings)? {
        return Ok(None);
    }
    let quiet = Duration::from_secs(settings::lead_quiet(&settings)?);
    let transcript = check.transcript.as_deref();
    wait_quiet(transcript, quiet).await;

    let place = identity::place(&identity::working_dir()?)?;
    let me = identity::agent(&place, &check.session, server)?;
    let api = Api::new(server).signed_in(Some(&check.session))?;
    let who = api.who(&me, false).await?;
    let lead = who.iter().any(|s| s.uri.who() == me.who() && s.uri.lead());
    if !lead {
        return Ok(None);
    }

    let mut facts = Facts {
        paused: api.riff(&me).await? == RiffState::Paused,
        ..Facts::default()
    };
    if !facts.paused {
        return Ok(Some(decide(&facts)));
    }
    let repo = place.repo_text();
    let gh = Gh::default();
    facts.wave = forge::released_wave(&gh, &repo)?;
    let Some(wave) = facts.wave.clone() else {
        return Ok(Some(decide(&facts)));
    };
    facts.in_flight = who
        .iter()
        .any(|s| s.uri.place().repo() == place.repo() && !s.uri.claims().is_empty())
        || forge::open_wave_prs(&gh, &repo)?;
    facts.unread = api.threads(&me).await?.iter().map(|t| t.unread).sum();
    transcript_facts(&mut facts, transcript, quiet);
    let dir = local::dir();
    // Only one check acts at a time: it holds the lock from the load of
    // the record to its save.
    let _lock = match &dir {
        Some(dir) => match local::compact_lock(dir, &repo)? {
            Some(held) => Some(held),
            None => return Ok(None),
        },
        None => None,
    };
    let record_path = dir.map(|dir| Record::path(&dir, &repo));
    facts.record = record_path.as_deref().map(Record::load).unwrap_or_default();
    if facts.record.step(&wave) == Some(RecordStep::Asked)
        && let Some(thread) = place.default_thread()
    {
        let person = identity::person(&place, server)?;
        let messages = Api::new(server)
            .signed_in(None)?
            .read(&person, &thread, true)
            .await?;
        let messages: Vec<_> = messages.into_iter().map(|c| c.message).collect();
        facts.note = handoff_note(&messages, me.who(), &wave);
    }

    let tmux = check.pane.as_ref().map(|_| Tmux::machine());
    if let (Some(tmux), Some(pane)) = (&tmux, &check.pane) {
        // A screen that riff cannot read counts as a person at work.
        let screen = tmux.screen(pane).unwrap_or_default();
        facts.input_empty = Some(ClaudeCode.input_empty(&screen));
    }

    let step = decide(&facts);
    let save = |record: Record| match &record_path {
        Some(path) => record.save(path),
        None => Ok(()),
    };
    match &step {
        Step::AskHandoff(wave) => {
            worker::tell_lead(Some(&place), server, &ask(wave)).await?;
            save(Record::asked(wave))?;
        }
        Step::TellUser { wave, note } => {
            worker::tell_lead(Some(&place), server, &tell_user(wave, *note)).await?;
            save(Record::done(wave))?;
        }
        Step::Compact { wave, note } => {
            if let (Some(tmux), Some(pane)) = (&tmux, &check.pane) {
                let text = instructions(me.who().user(), &repo, *note);
                tmux.type_line(pane, &ClaudeCode.compact(&text))?;
                save(Record::done(wave))?;
            }
        }
        Step::Wait(_) | Step::WaitHandoff(_) => {}
    }
    Ok(Some(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All conditions hold, and the lead posted its note.
    fn ready() -> Facts {
        Facts {
            paused: true,
            wave: Some("Wave 13".into()),
            in_flight: false,
            turn_ended: true,
            unread: 0,
            quiet: true,
            input_empty: Some(true),
            asks_user: false,
            record: Record::asked("Wave 13"),
            note: Some(40),
        }
    }

    #[test]
    fn all_conditions_give_the_compact() {
        assert_eq!(
            decide(&ready()),
            Step::Compact {
                wave: "Wave 13".into(),
                note: 40
            }
        );
    }

    #[test]
    fn each_condition_that_does_not_hold_stops_the_compact() {
        type BreakIt = fn(&mut Facts);
        let cases: [(Blocker, BreakIt); 7] = [
            (Blocker::Running, |f| f.paused = false),
            (Blocker::WaveOpen, |f| f.wave = None),
            (Blocker::WorkInFlight, |f| f.in_flight = true),
            (Blocker::LeadBusy, |f| f.unread = 1),
            (Blocker::PersonHere, |f| f.input_empty = Some(false)),
            (Blocker::AsksUser, |f| f.asks_user = true),
            (Blocker::Done, |f| f.record = Record::done("Wave 13")),
        ];
        for (blocker, break_it) in cases {
            let mut facts = ready();
            break_it(&mut facts);
            assert_eq!(decide(&facts), Step::Wait(blocker), "{blocker:?}");
        }
    }

    #[test]
    fn a_new_turn_or_input_in_the_quiet_time_stops_the_compact() {
        let mut facts = ready();
        facts.turn_ended = false;
        assert_eq!(decide(&facts), Step::Wait(Blocker::LeadBusy));
        let mut facts = ready();
        facts.quiet = false;
        assert_eq!(decide(&facts), Step::Wait(Blocker::PersonHere));
    }

    #[test]
    fn a_new_wave_asks_again() {
        let mut facts = ready();
        facts.record = Record::done("Wave 12");
        facts.note = None;
        assert_eq!(decide(&facts), Step::AskHandoff("Wave 13".into()));
    }

    #[test]
    fn the_note_of_wave_1_is_not_the_note_of_wave_13() {
        let lead: riff_core::name::SessionUri =
            "riff://mike@thelio/como-technologies/riff?session=l1"
                .parse()
                .unwrap();
        let note = riff_core::wire::Message {
            seq: 5,
            from: lead.clone(),
            to: vec![],
            body: "handoff: Wave 13 done".into(),
            at_ms: 0,
            kind: Default::default(),
            sig: None,
            payload: None,
        };
        let notes = [note];
        assert_eq!(handoff_note(&notes, lead.who(), "Wave 1"), None);
        assert_eq!(handoff_note(&notes, lead.who(), "Wave 13"), Some(5));
    }

    #[test]
    fn a_record_survives_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = Record::path(dir.path(), "o/r");
        assert_eq!(Record::load(&path), Record::default());
        Record::asked("Wave 3").save(&path).unwrap();
        assert_eq!(Record::load(&path), Record::asked("Wave 3"));
    }
}
