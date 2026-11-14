//! The members of the council of a verify (#581): the form of a member
//! file, the default set that riff builds in, and `riff council list`
//! (01M7CFBZ7839WHMFARR4T6W13W, 01M7CFBZ9TJ83FNHN0APY41175,
//! 01M7CFBZMCE36ZRXD0GBCM4MFX). [`pick`] picks the council of a pull
//! request, for `riff council show`.
//!
//! # Design
//!
//! Each member of the council has one file, `.riff/council/NAME.md`
//! ([`FOLDER`]). Its name is the name of the file with no `.md`
//! ([`Name`], 01M7CN1GZADT442NSWGYZCNDTH). riff builds in the default
//! set: the files of `.riff/council/` of the riff repository
//! ([`default_set`]). The members of a folder are the files of
//! `.riff/council/` at the top of its git work tree over the default set
//! ([`of_folder`]). The members of a branch are the files of
//! `.riff/council/` of its tree over the default set ([`of_branch`]). A
//! file with the name of a default member takes its place, and a file
//! with a new name adds a member. The default set comes first, in the
//! order of 01M7CFBZHQZ8FDT3Q0NGYKBZWB, then each other member in the
//! order of its name (01M7CN1H1XGQVBGFH30MRE1BP4).
//!
//! ```mermaid
//! flowchart TD
//!     D["the default set:<br/>.riff/council/ of riff, built in"] --> M["the members of the folder"]
//!     F[".riff/council/NAME.md<br/>at the top of the work tree"] --> P{"riff reads the file?"}
//!     P -- no --> E["an error that names the file and the line"]
//!     P -- "the name of a default member" --> R["it takes the place of that member"]
//!     P -- "a new name" --> A["it adds a member after the default set"]
//!     R --> M
//!     A --> M
//!     M --> L["riff council list"]
//! ```
//!
//! A member file starts with a TOML table between two lines `+++`. The
//! key `files` lists the globs of the paths of its area, and the key
//! `labels` lists the labels of its area. A member with neither key joins
//! each council ([`Joins`]). After the table come a `#` heading with the
//! name of the member, the text of its area, and one `##` heading for
//! each rule ([`Rule`]). A `##` line in a code block is no rule.
//!
//! ```
//! use riff::council::{Joins, Member, Source};
//!
//! # // In the source of this test, rustdoc reads `##` at the start of a
//! # // line as `#`, and hides a line that starts with `# `.
//! let text = r#"+++
//! files = ["crates/riff-server/**"]
//! labels = ["performance"]
//! +++
//!
//! ## Performance
//!
//! You review the time and the memory of riff-server.
//!
//! ### A read of the log reads each line one time
//!
//! A second pass over the log is a finding.
//! "#;
//! let member = Member::parse("performance".parse()?, Source::Riff, text)?;
//! assert_eq!(member.title, "Performance");
//! assert_eq!(member.area, "You review the time and the memory of riff-server.");
//! assert_eq!(
//!     member.joins,
//!     Joins::Area {
//!         files: vec!["crates/riff-server/**".into()],
//!         labels: vec!["performance".into()],
//!     }
//! );
//! assert_eq!(member.rules[0].heading, "A read of the log reads each line one time");
//! assert_eq!(member.rules[0].text, "A second pass over the log is a finding.");
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! riff refuses a file with no table, a table that is not TOML, a key
//! that it does not know, a value that is not a list of strings, an
//! empty list, an empty string in a list, or no `#` heading
//! (01M7CN1GWPHVBMGAGRVE9ZGVCK). The error names the file and the line
//! ([`Fault`]):
//!
//! ```
//! use riff::council::{Member, Source, Wrong};
//!
//! let fault = Member::parse("rust".parse()?, Source::Riff, "+++\nfile = []\n+++\n# Rust\n")
//!     .unwrap_err();
//! assert_eq!(fault.wrong, Wrong::UnknownKey("file".into()));
//! assert_eq!(
//!     fault.to_string(),
//!     ".riff/council/rust.md:2: riff does not know the key file. The keys are files and labels."
//! );
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! `riff council list` prints a row for each member: its name, its
//! source, the count of its rules and when it joins ([`list`]).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow};

use crate::session_image::top;
use crate::{text, view};

pub mod pick;

/// The folder of the member files, from the top of the work tree.
pub const FOLDER: &str = ".riff/council";

/// The line that opens and closes the table of a member file.
const FENCE: &str = "+++";

/// The default set: the name and the text of each file of
/// `.riff/council/` of riff, in the order of 01M7CFBZHQZ8FDT3Q0NGYKBZWB.
const DEFAULT_SET: [(&str, &str); 9] = [
    ("design", include_str!("../../../.riff/council/design.md")),
    ("testing", include_str!("../../../.riff/council/testing.md")),
    ("docs", include_str!("../../../.riff/council/docs.md")),
    ("rust", include_str!("../../../.riff/council/rust.md")),
    (
        "security",
        include_str!("../../../.riff/council/security.md"),
    ),
    (
        "attacker",
        include_str!("../../../.riff/council/attacker.md"),
    ),
    (
        "requirements",
        include_str!("../../../.riff/council/requirements.md"),
    ),
    (
        "newcomer",
        include_str!("../../../.riff/council/newcomer.md"),
    ),
    (
        "operations",
        include_str!("../../../.riff/council/operations.md"),
    ),
];

/// The name of a member: the name of its file with no `.md`. It has
/// lowercase letters, digits and `-`, with a letter first
/// (01M7CN1GZADT442NSWGYZCNDTH).
///
/// ```
/// use riff::council::Name;
///
/// assert_eq!("rust".parse::<Name>()?.as_str(), "rust");
/// assert!("big-o2".parse::<Name>().is_ok());
/// for bad in ["", "Rust", "2fast", "-x", "a b", "a:b", "a.md"] {
///     assert!(bad.parse::<Name>().is_err(), "{bad}");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Name {
    type Err = BadName;

    fn from_str(text: &str) -> Result<Name, BadName> {
        let mut chars = text.chars();
        let first = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        if first && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            Ok(Name(text.to_owned()))
        } else {
            Err(BadName(text.to_owned()))
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A text that is not the [`Name`] of a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadName(pub String);

impl fmt::Display for BadName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not the name of a member. A name has lowercase letters, digits and -, with a letter first, for example performance.md.",
            text::safe(&self.0)
        )
    }
}

impl std::error::Error for BadName {}

/// Where riff found the file of a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The default set that riff builds in.
    Riff,
    /// A file of the repository: its path from the top of the work tree.
    File(PathBuf),
}

impl Source {
    /// The path of the file of the member `name`, from the top of the
    /// work tree. A member of the default set has the path of its file
    /// in riff.
    fn path(&self, name: &Name) -> PathBuf {
        match self {
            Source::Riff => Path::new(FOLDER).join(format!("{name}.md")),
            Source::File(path) => path.clone(),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Riff => f.write_str("riff"),
            Source::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// A key of the table of a member file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// `files`: the globs of the paths of the area.
    Files,
    /// `labels`: the labels of the area.
    Labels,
}

impl Key {
    /// The key with the name `name`, if riff knows it.
    fn of(name: &str) -> Option<Key> {
        match name {
            "files" => Some(Key::Files),
            "labels" => Some(Key::Labels),
            _ => None,
        }
    }

    /// The name of the key in the table.
    fn name(self) -> &'static str {
        match self {
            Key::Files => "files",
            Key::Labels => "labels",
        }
    }

    /// A value of the key, for the text of an error.
    fn example(self) -> &'static str {
        match self {
            Key::Files => "docs/**",
            Key::Labels => "security",
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// When a member joins a council.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Joins {
    /// Each council: the table has neither `files` nor `labels`.
    Each,
    /// The council of a change in its area: a path that a glob of
    /// `files` matches, or a label of `labels`. A key that the table
    /// does not have gives an empty list.
    Area {
        /// The globs of `files`.
        files: Vec<String>,
        /// The labels of `labels`.
        labels: Vec<String>,
    },
}

impl fmt::Display for Joins {
    /// `each council`, or `files GLOB, GLOB; labels LABEL`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Joins::Each => f.write_str("each council"),
            Joins::Area { files, labels } => {
                let parts: Vec<String> = [(Key::Files, files), (Key::Labels, labels)]
                    .into_iter()
                    .filter(|(_, list)| !list.is_empty())
                    .map(|(key, list)| format!("{key} {}", list.join(", ")))
                    .collect();
                f.write_str(&parts.join("; "))
            }
        }
    }
}

/// One rule of a member: a `##` heading and the text under it. A
/// finding cites the rule by its heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The text of the `##` heading.
    pub heading: String,
    /// The text under the heading, up to the next `##` heading.
    pub text: String,
}

/// A member of the council.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name of its file with no `.md`.
    pub name: Name,
    /// Where riff found its file.
    pub source: Source,
    /// When it joins a council: the keys of its table.
    pub joins: Joins,
    /// The text of its `#` heading.
    pub title: String,
    /// The text of its area: from the `#` heading to the first rule.
    pub area: String,
    /// Its rules, in the order of the file.
    pub rules: Vec<Rule>,
    /// The text of its file after the table: the `#` heading, the area
    /// and the rules.
    pub text: String,
}

/// What is wrong with a member file (01M7CFBZ9TJ83FNHN0APY41175,
/// 01M7CN1GWPHVBMGAGRVE9ZGVCK).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrong {
    /// The first line is not `+++`.
    NoTable,
    /// No line `+++` closes the table.
    OpenTable,
    /// The table is not TOML: the words of the parser.
    NotToml(String),
    /// A key of the table that riff does not know.
    UnknownKey(String),
    /// The value of the key is not a list of strings.
    NotStrings(Key),
    /// The value of the key is an empty list.
    EmptyList(Key),
    /// The list of the key has an empty string.
    EmptyString(Key),
    /// No `#` heading with the name of the member comes after the table.
    NoHeading,
}

impl fmt::Display for Wrong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Wrong::NoTable => write!(
                f,
                "the file has no table. A member file starts with a TOML table between two lines {FENCE}."
            ),
            Wrong::OpenTable => write!(f, "no line {FENCE} closes the table."),
            Wrong::NotToml(why) => write!(f, "the table is not TOML: {}", text::safe(why)),
            Wrong::UnknownKey(key) => write!(
                f,
                "riff does not know the key {}. The keys are files and labels.",
                text::safe(key)
            ),
            Wrong::NotStrings(key) => write!(
                f,
                "{key} is not a list of strings. Write it so: {key} = [\"{}\"]",
                key.example()
            ),
            Wrong::EmptyList(key) => write!(
                f,
                "{key} is an empty list. Give one value or more, or remove the key."
            ),
            Wrong::EmptyString(key) => write!(f, "{key} has an empty string."),
            Wrong::NoHeading => write!(
                f,
                "no # heading with the name of the member comes after the table."
            ),
        }
    }
}

/// A member file that riff cannot read: the file, the line and what is
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// The path of the file, from the top of the work tree.
    pub file: PathBuf,
    /// The line of the file, from 1.
    pub line: usize,
    /// What is wrong.
    pub wrong: Wrong,
}

impl fmt::Display for Fault {
    /// `FILE:LINE: WHAT`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            text::safe(&self.file.display().to_string()),
            self.line,
            self.wrong
        )
    }
}

impl std::error::Error for Fault {}

impl Member {
    /// The member `name` from the text of its file, found at `source`.
    pub fn parse(name: Name, source: Source, text: &str) -> Result<Member, Fault> {
        let fault = |line, wrong| Fault {
            file: source.path(&name),
            line,
            wrong,
        };
        let lines: Vec<&str> = text.lines().collect();
        if lines.first().map(|line| line.trim_end()) != Some(FENCE) {
            return Err(fault(1, Wrong::NoTable));
        }
        let Some(end) = lines
            .iter()
            .skip(1)
            .position(|line| line.trim_end() == FENCE)
            .map(|at| at + 1)
        else {
            return Err(fault(1, Wrong::OpenTable));
        };
        // Line N of the table is line N + 1 of the file.
        let joins =
            joins(&lines[1..end].join("\n")).map_err(|(line, wrong)| fault(line + 1, wrong))?;
        // Each line after the table, with its index in `lines`.
        let mut body = lines
            .iter()
            .enumerate()
            .skip(end + 1)
            .skip_while(|(_, line)| line.trim().is_empty());
        // With no line after the table, the fault is on the line `+++`
        // that closes it.
        let (at, first) = body.next().unwrap_or((end, &lines[end]));
        let Some(title) = heading(first, "#") else {
            return Err(fault(at + 1, Wrong::NoHeading));
        };
        let after_table = format!("{}\n", lines[at..].join("\n").trim_end());
        let mut area: Vec<&str> = Vec::new();
        let mut rules: Vec<(String, Vec<&str>)> = Vec::new();
        let mut fenced = false;
        for (_, line) in body {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            match heading(line, "##").filter(|_| !fenced) {
                Some(heading) => rules.push((heading, Vec::new())),
                None => match rules.last_mut() {
                    Some((_, text)) => text.push(line),
                    None => area.push(line),
                },
            }
        }
        Ok(Member {
            name,
            source,
            joins,
            title,
            area: area.join("\n").trim().to_owned(),
            rules: rules
                .into_iter()
                .map(|(heading, text)| Rule {
                    heading,
                    text: text.join("\n").trim().to_owned(),
                })
                .collect(),
            text: after_table,
        })
    }
}

/// When the member with the TOML table `table` joins a council. A
/// fault gives the line of `table`, from 1.
fn joins(table: &str) -> Result<Joins, (usize, Wrong)> {
    let doc = toml_edit::Document::parse(table).map_err(|e| {
        let at = e.span().map_or(0, |span| span.start);
        (line_at(table, at), Wrong::NotToml(e.message().to_owned()))
    })?;
    let (mut files, mut labels) = (None, None);
    for (name, item) in doc.iter() {
        let span = doc
            .as_table()
            .key(name)
            .and_then(toml_edit::Key::span)
            .or_else(|| item.span());
        let at = line_at(table, span.map_or(0, |span| span.start));
        let Some(key) = Key::of(name) else {
            return Err((at, Wrong::UnknownKey(name.to_owned())));
        };
        let list = Some(strings(table, key, item, at)?);
        match key {
            Key::Files => files = list,
            Key::Labels => labels = list,
        }
    }
    Ok(match (files, labels) {
        (None, None) => Joins::Each,
        (files, labels) => Joins::Area {
            files: files.unwrap_or_default(),
            labels: labels.unwrap_or_default(),
        },
    })
}

/// The strings of the value `item` of `key`, on the line `at` of
/// `table`. A fault gives the line of `table`, from 1.
fn strings(
    table: &str,
    key: Key,
    item: &toml_edit::Item,
    at: usize,
) -> Result<Vec<String>, (usize, Wrong)> {
    let Some(list) = item.as_array() else {
        return Err((at, Wrong::NotStrings(key)));
    };
    if list.is_empty() {
        return Err((at, Wrong::EmptyList(key)));
    }
    list.iter()
        .map(|value| {
            let line = value.span().map_or(at, |span| line_at(table, span.start));
            match value.as_str() {
                None => Err((line, Wrong::NotStrings(key))),
                Some("") => Err((line, Wrong::EmptyString(key))),
                Some(text) => Ok(text.to_owned()),
            }
        })
        .collect()
}

/// The line, from 1, of the byte `at` of `text`.
fn line_at(text: &str, at: usize) -> usize {
    text.get(..at).unwrap_or(text).matches('\n').count() + 1
}

/// The text of `line` when it is a heading with the marks `marks`, `#`
/// or `##`, and a text.
fn heading(line: &str, marks: &str) -> Option<String> {
    let text = line.strip_prefix(marks)?.strip_prefix(' ')?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The members of the default set that riff builds in, in the order of
/// 01M7CFBZHQZ8FDT3Q0NGYKBZWB.
pub fn default_set() -> Result<Vec<Member>> {
    DEFAULT_SET
        .iter()
        .map(|(name, text)| Ok(Member::parse(name.parse()?, Source::Riff, text)?))
        .collect()
}

/// The members of the folder `dir`: the files of [`FOLDER`] at the top
/// of its git work tree over the default set ([`of_tree`]).
pub fn of_folder(dir: &Path) -> Result<Vec<Member>> {
    of_tree(&top(dir)?)
}

/// The members of the work tree `top`: the files of [`FOLDER`] there
/// over the default set. A file with the name of a default member takes
/// its place. Each other member comes after the default set, in the
/// order of its name (01M7CN1H1XGQVBGFH30MRE1BP4).
///
/// ```
/// use riff::council::{Source, of_tree};
///
/// let top = tempfile::tempdir()?;
/// let names = |members: &[riff::council::Member]| {
///     members.iter().map(|m| m.name.to_string()).collect::<Vec<_>>()
/// };
/// let defaults = of_tree(top.path())?;
/// assert_eq!(names(&defaults).len(), 9);
///
/// std::fs::create_dir_all(top.path().join(".riff/council"))?;
/// std::fs::write(top.path().join(".riff/council/big-o.md"), "+++\n+++\n# Big O\n")?;
/// std::fs::write(top.path().join(".riff/council/notes.txt"), "not a member")?;
/// let members = of_tree(top.path())?;
/// assert_eq!(names(&members)[..9], names(&defaults)[..]);
/// assert_eq!(names(&members)[9], "big-o");
/// assert_eq!(members[9].source, Source::File(".riff/council/big-o.md".into()));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_tree(top: &Path) -> Result<Vec<Member>> {
    let mut read = Vec::new();
    for (name, path) in files(top)? {
        let text = fs::read_to_string(top.join(&path))
            .with_context(|| format!("cannot read the member file {}", path.display()))?;
        read.push((name, path, text));
    }
    over_default_set(read)
}

/// The members of the branch `branch` of the clone `top`, for example
/// `origin/main`: the files of [`FOLDER`] of its tree over the default
/// set, as [`of_tree`] reads them in a work tree. It reads no file of the
/// work tree (01M7CFBZ7839WHMFARR4T6W13W). A branch with no such folder
/// has the default set.
///
/// ```
/// use std::process::Command;
/// use riff::council::{Source, of_branch};
///
/// let top = tempfile::tempdir()?;
/// let git = |args: &[&str]| {
///     let out = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .args(args)
///         .current_dir(top.path())
///         .output()
///         .unwrap();
///     assert!(out.status.success(), "{out:?}");
/// };
/// git(&["init", "-q", "-b", "main"]);
/// git(&["commit", "-q", "--allow-empty", "-m", "start"]);
/// assert_eq!(of_branch(top.path(), "main")?.len(), 9);
///
/// std::fs::create_dir_all(top.path().join(".riff/council"))?;
/// std::fs::write(top.path().join(".riff/council/big-o.md"), "+++\n+++\n# Big O\n")?;
/// git(&["add", "."]);
/// git(&["commit", "-q", "-m", "big-o"]);
/// // A file of the work tree is not a file of the branch.
/// std::fs::write(top.path().join(".riff/council/perf.md"), "+++\n+++\n# Perf\n")?;
/// let members = of_branch(top.path(), "main")?;
/// assert_eq!(members.len(), 10);
/// assert_eq!(members[9].name.as_str(), "big-o");
/// assert_eq!(members[9].source, Source::File(".riff/council/big-o.md".into()));
/// assert!(of_branch(top.path(), "no-such-branch").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_branch(top: &Path, branch: &str) -> Result<Vec<Member>> {
    let folder = format!("{FOLDER}/");
    let listed = crate::run::output("git", top, &["ls-tree", "-z", branch, "--", &folder])
        .with_context(|| format!("riff cannot read {FOLDER} of {branch}"))?;
    let mut files = Vec::new();
    // Each entry is `MODE TYPE OBJECT<tab>PATH`. A link or a folder is no
    // member file.
    for entry in listed.split('\0').filter(|entry| !entry.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let meta: Vec<&str> = meta.split(' ').collect();
        let [mode, _, object] = meta[..] else {
            continue;
        };
        if !matches!(mode, "100644" | "100755") {
            continue;
        }
        let path = PathBuf::from(path);
        let Some(name) = member_name(&path)? else {
            continue;
        };
        let text = crate::run::output("git", top, &["cat-file", "blob", object])
            .with_context(|| format!("riff cannot read {} of {branch}", path.display()))?;
        files.push((name, path, text));
    }
    files.sort();
    over_default_set(files)
}

/// The members of the member files `files` over the default set: the
/// name, the path from the top of the work tree and the text of each
/// file, in the order of the name. A file with the name of a default
/// member takes its place. Each other member comes after the default
/// set.
fn over_default_set(files: Vec<(Name, PathBuf, String)>) -> Result<Vec<Member>> {
    let mut members = default_set()?;
    let mut added = Vec::new();
    for (name, path, text) in files {
        let member = Member::parse(name, Source::File(path), &text)?;
        match members.iter_mut().find(|m| m.name == member.name) {
            Some(default) => *default = member,
            None => added.push(member),
        }
    }
    members.extend(added);
    Ok(members)
}

/// Each member file of the work tree `top`, in the order of its name:
/// each file `NAME.md` of [`FOLDER`], with its path from `top`. A tree
/// with no such folder has none.
fn files(top: &Path) -> Result<Vec<(Name, PathBuf)>> {
    let entries = match fs::read_dir(top.join(FOLDER)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("cannot read the folder {FOLDER}")),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read the folder {FOLDER}"))?;
        let path = Path::new(FOLDER).join(entry.file_name());
        if !entry.path().is_file() {
            continue;
        }
        if let Some(name) = member_name(&path)? {
            files.push((name, path));
        }
    }
    files.sort();
    Ok(files)
}

/// The name of the member of the file `path` of [`FOLDER`], or `None`
/// for a file that is not `.md`. A file `.md` with a name that is not the
/// name of a member is an error that names the file
/// (01M7CN1GZADT442NSWGYZCNDTH).
fn member_name(path: &Path) -> Result<Option<Name>> {
    if path.extension().is_none_or(|ext| ext != "md") {
        return Ok(None);
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    stem.parse::<Name>()
        .map(Some)
        .map_err(|e| anyhow!("{}: {e}", text::safe(&path.display().to_string())))
}

/// The table of `riff council list`: one row for each member, with its
/// name, its source, the count of its rules and when it joins
/// (01M7CN1H1XGQVBGFH30MRE1BP4).
///
/// ```
/// let members = riff::council::default_set()?;
/// let plain = anstream::adapter::strip_str(&riff::council::list(&members)).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert!(lines[0].starts_with("MEMBER"), "{plain}");
/// assert!(lines[1].starts_with("design ") && lines[1].ends_with("each council"), "{plain}");
/// assert!(lines[4].starts_with("rust ") && lines[4].ends_with("files **/*.rs"), "{plain}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn list(members: &[Member]) -> String {
    let rows: Vec<Vec<String>> = members
        .iter()
        .map(|member| {
            vec![
                member.name.to_string(),
                text::safe(&member.source.to_string()),
                member.rules.len().to_string(),
                text::safe(&member.joins.to_string()),
            ]
        })
        .collect();
    view::table(&["MEMBER", "SOURCE", "RULES", "JOINS"], &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder `.riff/council/` of this repository.
    fn folder() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(FOLDER)
    }

    /// The member `name` of the text `text`, from a file of a repository.
    fn parse(name: &str, text: &str) -> Result<Member, Fault> {
        let path = Path::new(FOLDER).join(format!("{name}.md"));
        Member::parse(name.parse().unwrap(), Source::File(path), text)
    }

    /// The headings `## ` of `text` outside a code block.
    fn headings(text: &str) -> Vec<String> {
        let mut fenced = false;
        let mut out = Vec::new();
        for line in text.lines() {
            if line.starts_with("```") {
                fenced = !fenced;
            } else if let Some(heading) = line.strip_prefix("## ").filter(|_| !fenced) {
                out.push(heading.trim().to_owned());
            }
        }
        out
    }

    /// The globs and the labels of `member`.
    fn area(member: &Member) -> (&[String], &[String]) {
        match &member.joins {
            Joins::Each => panic!("{} joins each council", member.name),
            Joins::Area { files, labels } => (files, labels),
        }
    }

    /// 01M7CFBZ7839WHMFARR4T6W13W: each file of `.riff/council/` is a
    /// member of the default set that riff builds in, with the text of
    /// the file. The set has the 9 members, in the order of
    /// 01M7CFBZHQZ8FDT3Q0NGYKBZWB, with their keys
    /// (01M7CFBZCDFGG1MV4DEAACQVXK, 01M7CFBZF1BZ0E7H0D4Q7KKY1T) and
    /// their rules.
    #[test]
    fn the_default_set_is_each_file_of_the_folder_of_riff() {
        let set = default_set().unwrap();
        let names: Vec<&str> = set.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "design",
                "testing",
                "docs",
                "rust",
                "security",
                "attacker",
                "requirements",
                "newcomer",
                "operations"
            ]
        );
        let mut on_disk: Vec<String> = fs::read_dir(folder())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut built_in: Vec<String> = names.iter().map(|name| format!("{name}.md")).collect();
        built_in.sort();
        assert_eq!(on_disk, built_in, "each file of {FOLDER} is in DEFAULT_SET");
        for member in &set {
            let text = fs::read_to_string(folder().join(format!("{}.md", member.name))).unwrap();
            let from_disk = Member::parse(member.name.clone(), Source::Riff, &text).unwrap();
            assert_eq!(
                &from_disk, member,
                "{} is the text of its file",
                member.name
            );
            assert_eq!(member.source, Source::Riff);
            assert!(
                !member.title.is_empty() && !member.area.is_empty(),
                "{member:?}"
            );
            let rules: Vec<&str> = member.rules.iter().map(|r| r.heading.as_str()).collect();
            assert_eq!(rules, headings(&text), "{}", member.name);
            assert!(!rules.is_empty(), "{} has no rule", member.name);
            for rule in &member.rules {
                assert!(!rule.text.is_empty(), "{}: {rule:?}", member.name);
            }
        }
        let member = |name: &str| set.iter().find(|m| m.name.as_str() == name).unwrap();
        for name in ["design", "testing", "docs"] {
            assert_eq!(member(name).joins, Joins::Each, "{name}");
        }
        assert_eq!(area(member("rust")), (&["**/*.rs".to_owned()][..], &[][..]));
        let rust: Vec<&str> = member("rust")
            .rules
            .iter()
            .map(|r| r.heading.as_str())
            .collect();
        for pattern in [
            "An invalid state cannot be made",
            "One type for each layout",
            "Each conversion names each field",
        ] {
            assert!(rust.contains(&pattern), "{pattern}: {rust:?}");
        }
        let (files, labels) = area(member("security"));
        assert!(
            files.contains(&"crates/riff-server/**".to_owned()),
            "{files:?}"
        );
        assert_eq!(labels, ["security"]);
        assert_eq!(member("attacker").joins, member("security").joins);
        assert_eq!(area(member("operations")).1, ["operations"]);
        let (files, labels) = area(member("requirements"));
        assert!(files.contains(&"**/requirements*.md".to_owned()) && labels.is_empty());
        let (files, labels) = area(member("newcomer"));
        assert!(files.contains(&"docs/**".to_owned()) && labels.is_empty());
    }

    /// 01M7CFBZ9TJ83FNHN0APY41175: a file with no table, a table that
    /// does not close, a table that is not TOML, a key that riff does not
    /// know and a file with no `#` heading each give an error that names
    /// the file and the line.
    #[test]
    fn each_fault_of_the_form_names_the_file_and_the_line() {
        for (text, line, wrong) in [
            ("# Perf\n\n## A rule\n", 1, Wrong::NoTable),
            ("", 1, Wrong::NoTable),
            ("\n+++\n+++\n# Perf\n", 1, Wrong::NoTable),
            ("+++\nfiles = [\"a\"]\n# Perf\n", 1, Wrong::OpenTable),
            (
                "+++\nlabels = [\"a\"]\nfiles = [\"a\"\n+++\n# Perf\n",
                3,
                Wrong::NotToml(String::new()),
            ),
            (
                "+++\nfiles = [\"a\"]\nfiles = [\"b\"]\n+++\n# Perf\n",
                3,
                Wrong::NotToml(String::new()),
            ),
            (
                "+++\nfiles = [\"a\"]\n\nlabel = [\"b\"]\n+++\n# Perf\n",
                4,
                Wrong::UnknownKey("label".into()),
            ),
            (
                "+++\nfiles = [\"a\"]\n[extra]\nx = 1\n+++\n# Perf\n",
                3,
                Wrong::UnknownKey("extra".into()),
            ),
            ("+++\n+++\n", 2, Wrong::NoHeading),
            ("+++\n+++\n\n\nYou review.\n", 5, Wrong::NoHeading),
            ("+++\n+++\n## A rule\n", 3, Wrong::NoHeading),
            ("+++\n+++\n#Perf\n", 3, Wrong::NoHeading),
            ("+++\n+++\n# \n", 3, Wrong::NoHeading),
        ] {
            let fault = parse("perf", text).unwrap_err();
            assert_eq!(fault.file, Path::new(".riff/council/perf.md"), "{text:?}");
            assert_eq!(fault.line, line, "{text:?}: {fault}");
            match (&fault.wrong, &wrong) {
                (Wrong::NotToml(why), Wrong::NotToml(_)) => assert!(!why.is_empty()),
                _ => assert_eq!(fault.wrong, wrong, "{text:?}"),
            }
            let shown = fault.to_string();
            assert!(
                shown.starts_with(&format!(".riff/council/perf.md:{line}: ")),
                "{shown}"
            );
        }
    }

    /// 01M7CN1GWPHVBMGAGRVE9ZGVCK: a value that is not a list of strings,
    /// an empty list and an empty string each give an error that names
    /// the file and the line.
    #[test]
    fn each_fault_of_a_value_names_the_file_and_the_line() {
        for (table, line, wrong) in [
            ("files = \"**/*.rs\"", 2, Wrong::NotStrings(Key::Files)),
            ("labels = 3", 2, Wrong::NotStrings(Key::Labels)),
            ("files.x = [\"a\"]", 2, Wrong::NotStrings(Key::Files)),
            (
                "files = [\n  \"a\",\n  4,\n]",
                4,
                Wrong::NotStrings(Key::Files),
            ),
            ("labels = []", 2, Wrong::EmptyList(Key::Labels)),
            ("files = [\"a\", \"\"]", 2, Wrong::EmptyString(Key::Files)),
        ] {
            let text = format!("+++\n{table}\n+++\n# Perf\n");
            let fault = parse("perf", &text).unwrap_err();
            assert_eq!(
                (fault.line, &fault.wrong),
                (line, &wrong),
                "{text:?}: {fault}"
            );
            assert!(
                fault
                    .to_string()
                    .starts_with(&format!(".riff/council/perf.md:{line}: {}", wrong)),
                "{fault}"
            );
        }
    }

    /// The area and the rules: a `##` line in a code block is no rule,
    /// and a member with an empty table joins each council.
    #[test]
    fn a_member_has_its_area_and_its_rules() {
        let text = "+++\n+++\n\n# Perf\n\nYou review the time.\n\nAnd the memory.\n\n## One\n\nText one.\n\n```sh\n## not a rule\n```\n\n## Two\n## Three\n\nText three.\n";
        let member = parse("perf", text).unwrap();
        assert_eq!(member.joins, Joins::Each);
        assert_eq!(member.title, "Perf");
        assert_eq!(member.area, "You review the time.\n\nAnd the memory.");
        let rules: Vec<(&str, &str)> = member
            .rules
            .iter()
            .map(|r| (r.heading.as_str(), r.text.as_str()))
            .collect();
        assert_eq!(
            rules,
            [
                ("One", "Text one.\n\n```sh\n## not a rule\n```"),
                ("Two", ""),
                ("Three", "Text three.")
            ]
        );
        let member = parse("perf", "+++\r\nlabels = [\"x\"]\r\n+++\r\n# Perf\r\n").unwrap();
        assert_eq!(
            member.joins,
            Joins::Area {
                files: vec![],
                labels: vec!["x".into()]
            }
        );
        assert_eq!(member.joins.to_string(), "labels x");
    }

    /// A member file in the work tree.
    fn write(top: &Path, file: &str, text: &str) {
        fs::create_dir_all(top.join(FOLDER)).unwrap();
        fs::write(top.join(FOLDER).join(file), text).unwrap();
    }

    /// 01M7CFBZ7839WHMFARR4T6W13W: a file `rust.md` of the folder takes
    /// the place of the default `rust`, and a file `performance.md` adds a
    /// member after the default set.
    #[test]
    fn a_file_of_the_folder_takes_the_place_of_a_default_or_adds_a_member() {
        let top = tempfile::tempdir().unwrap();
        write(
            top.path(),
            "rust.md",
            "+++\nfiles = [\"src/**\"]\n+++\n# Our Rust\n\n## Each time has a type\n\nUse a newtype.\n",
        );
        write(
            top.path(),
            "performance.md",
            "+++\nlabels = [\"performance\"]\n+++\n# Performance\n",
        );
        write(top.path(), "attic.md", "+++\n+++\n# Attic\n");
        let members = of_tree(top.path()).unwrap();
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "design",
                "testing",
                "docs",
                "rust",
                "security",
                "attacker",
                "requirements",
                "newcomer",
                "operations",
                "attic",
                "performance"
            ]
        );
        let rust = &members[3];
        assert_eq!(rust.source, Source::File(".riff/council/rust.md".into()));
        assert_eq!(rust.title, "Our Rust");
        assert_eq!(rust.rules.len(), 1);
        assert_eq!(rust.joins.to_string(), "files src/**");
        let performance = &members[10];
        assert_eq!(
            performance.source,
            Source::File(".riff/council/performance.md".into())
        );
        assert_eq!(performance.joins.to_string(), "labels performance");
        for member in &members[..9] {
            if member.name.as_str() != "rust" {
                assert_eq!(member.source, Source::Riff, "{}", member.name);
            }
        }
    }

    /// 01M7CN1GZADT442NSWGYZCNDTH: a file `.md` with a name that is not
    /// the name of a member is an error that names the file. riff reads
    /// no other file, and no folder.
    #[test]
    fn only_a_file_md_with_a_name_of_a_member_counts() {
        let top = tempfile::tempdir().unwrap();
        write(top.path(), "README", "not a member");
        write(top.path(), "notes.txt", "not a member");
        fs::create_dir_all(top.path().join(FOLDER).join("old.md")).unwrap();
        assert_eq!(of_tree(top.path()).unwrap().len(), 9);
        write(top.path(), "Perf.md", "+++\n+++\n# Perf\n");
        let error = of_tree(top.path()).unwrap_err().to_string();
        assert!(
            error.starts_with(".riff/council/Perf.md: \"Perf\" is not the name of a member."),
            "{error}"
        );
    }

    /// A fault in a file of the folder stops the read, and names the
    /// file and the line.
    #[test]
    fn a_bad_file_of_the_folder_is_an_error() {
        let top = tempfile::tempdir().unwrap();
        write(top.path(), "perf.md", "# Perf\n");
        let error = of_tree(top.path()).unwrap_err();
        let fault = error.downcast_ref::<Fault>().unwrap();
        assert_eq!(fault.file, Path::new(".riff/council/perf.md"));
        assert_eq!((fault.line, &fault.wrong), (1, &Wrong::NoTable));
    }

    /// A text of the file goes to the terminal with no escape sequence.
    #[test]
    fn the_list_and_the_errors_print_no_escape_sequence() {
        let top = tempfile::tempdir().unwrap();
        write(
            top.path(),
            "perf.md",
            "+++\nfiles = [\"a\\u001b]0;x\\u0007b\"]\n+++\n# Perf\n",
        );
        let members = of_tree(top.path()).unwrap();
        let shown = list(&members);
        assert!(shown.contains("files ab"), "{shown}");
        assert!(!shown.contains('\u{7}'), "{shown:?}");
        let fault = parse("perf", "+++\n\"a\\u001b[31m\" = 1\n+++\n# Perf\n").unwrap_err();
        assert!(!fault.to_string().contains('\u{1b}'), "{fault}");
    }
}
