//! Whether riff is on in a repository.
//!
//! # Design
//!
//! riff is off in a session until a person turns it on for the
//! repository of that session (01M3XY2SHGXQR9NVXF7QJBN09T). The state
//! is the key `enabledPlugins` of the Claude Code settings, with the
//! entry [`KEY`]. Claude Code reads the same key, so one place holds
//! the state: when the entry is `true`, Claude Code loads the plugin,
//! and riff runs.
//!
//! | Place | File | Command |
//! |---|---|---|
//! | [`Place::Local`] | `.claude/settings.local.json` at the top of the repository | `riff enable` |
//! | [`Place::Shared`] | `.claude/settings.json` at the top of the repository | `riff enable --shared` |
//! | [`Place::Global`] | the user settings ([`crate::plugin::settings_from`]) | `riff enable --global` |
//!
//! The first file that has the entry decides, in the order local,
//! shared, global. Claude Code uses the same order. So `false` in the
//! local settings turns riff off in one repository, and changes
//! nothing in another one.
//!
//! ```mermaid
//! flowchart TD
//!     E{"RIFF_ON=1?"} -- yes --> ON[riff is on]
//!     E -- no --> G{in a git repository?}
//!     G -- no --> OFF[riff is off]
//!     G -- yes --> L{"local settings<br/>have the entry?"}
//!     L -- yes --> V[its value decides]
//!     L -- no --> S{"shared settings<br/>have the entry?"}
//!     S -- yes --> V
//!     S -- no --> U{"user settings<br/>have the entry?"}
//!     U -- yes --> V
//!     U -- no --> OFF
//! ```
//!
//! - A directory that is not in a git repository is off, also when the
//!   user settings have the entry.
//! - riff finds the repository from the `.git` entry, with no `git`
//!   command ([`Repo::of`]): the check runs in each hook, and a
//!   repository with riff off runs no `git`.
//! - The local settings are not in a linked worktree: git ignores the
//!   file. So in a linked worktree riff also reads the files of the
//!   main clone (01M3XY2T2YEV7GT7DKJHSMMHYR), and `riff enable` writes
//!   the local settings of the main clone.
//! - `RIFF_ON=1` turns riff on for the processes that have it, also
//!   outside a repository (01M3XY2SWEK0N8MC3MY4TMYTD3). `just dev` and
//!   the tests set it.
//!
//! Each entry of the plugin asks [`State::here`] first: the hooks,
//! `riff statusline` and `riff mcp`. When riff is off, they call no
//! server, run no `git`, and print nothing
//! (01M3XY2ST8R67SKTXJECAYJZRX).
//!
//! ```
//! use riff::enable::{Place, State, set};
//!
//! let dir = tempfile::tempdir()?;
//! std::fs::create_dir(dir.path().join(".git"))?;
//! let user = dir.path().join("user.json");
//! assert!(!State::of(dir.path(), Some(&user), false).on);
//! let local = dir.path().join(".claude/settings.local.json");
//! set(&local, Some(true))?;
//! let state = State::of(dir.path(), Some(&user), false);
//! assert!(state.on);
//! assert_eq!(state.by, Some((Place::Local, local)));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

/// The entry of the riff plugin in `enabledPlugins`: the plugin and its
/// marketplace.
pub const KEY: &str = "riff@riff";

/// The variable that turns riff on for a process.
pub const VAR: &str = "RIFF_ON";

/// The name of the riff server of the plugin in Claude Code, as
/// `claude mcp list` shows it.
pub const MCP_SERVER: &str = "plugin:riff:riff";

/// True when `RIFF_ON` is `1`.
pub fn forced() -> bool {
    forced_value(std::env::var(VAR).ok().as_deref())
}

/// [`forced`] for the value of `RIFF_ON`.
///
/// ```
/// assert!(riff::enable::forced_value(Some("1")));
/// assert!(!riff::enable::forced_value(Some("0")));
/// assert!(!riff::enable::forced_value(None));
/// ```
pub fn forced_value(value: Option<&str>) -> bool {
    value == Some("1")
}

/// A settings file that can hold the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The local settings of the repository: only this person.
    Local,
    /// The project settings of the repository: checked in, for the
    /// team.
    Shared,
    /// The user settings: each repository on this machine.
    Global,
}

impl Place {
    /// The flag of `riff enable` and `riff disable` for this place.
    pub fn flag(self) -> &'static str {
        match self {
            Place::Local => "--local",
            Place::Shared => "--shared",
            Place::Global => "--global",
        }
    }
}

/// A git repository, found with no `git` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// The top of the working tree.
    pub top: PathBuf,
    /// The top of the main clone, when `top` is a linked worktree.
    pub main: Option<PathBuf>,
}

impl Repo {
    /// The repository of `dir`: the nearest directory at or above `dir`
    /// with a `.git` entry. A `.git` file of a linked worktree names
    /// the main clone.
    ///
    /// ```
    /// use riff::enable::Repo;
    ///
    /// let dir = isolated::outside_git();
    /// assert_eq!(Repo::of(dir.path()), None);
    /// let main = dir.path().join("app");
    /// let tree = main.join(".claude/worktrees/issue-12");
    /// std::fs::create_dir_all(main.join(".git/worktrees/issue-12"))?;
    /// std::fs::create_dir_all(tree.join("src"))?;
    /// let gitdir = main.join(".git/worktrees/issue-12");
    /// std::fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display()))?;
    /// let repo = Repo::of(&tree.join("src")).unwrap();
    /// assert_eq!(repo.top, tree);
    /// assert_eq!(repo.main.as_deref(), Some(main.as_path()));
    /// assert_eq!(Repo::of(&main).unwrap().main, None);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn of(dir: &Path) -> Option<Repo> {
        let top = dir.ancestors().find(|d| d.join(".git").exists())?;
        let main = std::fs::read_to_string(top.join(".git"))
            .ok()
            .and_then(|text| main_of(top, &text));
        Some(Repo {
            top: top.to_owned(),
            main,
        })
    }

    /// The directory that holds the local settings: the main clone of a
    /// linked worktree, else the top.
    pub fn home(&self) -> &Path {
        self.main.as_deref().unwrap_or(&self.top)
    }

    /// The settings file of `place` in the directory `top`.
    fn file(top: &Path, place: Place) -> PathBuf {
        match place {
            Place::Shared => top.join(".claude/settings.json"),
            _ => top.join(".claude/settings.local.json"),
        }
    }

    /// The files of the repository that can hold the entry, in the
    /// order in which riff reads them.
    fn files(&self) -> Vec<(Place, PathBuf)> {
        let tops = || std::iter::once(&self.top).chain(&self.main);
        let of = |place| tops().map(move |top| (place, Repo::file(top, place)));
        of(Place::Local).chain(of(Place::Shared)).collect()
    }
}

/// The entry of the linked worktree at `top` in its main clone, from
/// the text of its `.git` file: `gitdir: MAIN/.git/worktrees/NAME`.
fn entry_of(top: &Path, text: &str) -> Option<PathBuf> {
    Some(top.join(text.strip_prefix("gitdir:")?.trim()))
}

/// The top of the main clone from the text of the `.git` file of the
/// linked worktree at `top`: `gitdir: MAIN/.git/worktrees/NAME`.
fn main_of(top: &Path, text: &str) -> Option<PathBuf> {
    let gitdir = entry_of(top, text)?;
    let worktrees = gitdir.parent()?;
    if worktrees.file_name()? != "worktrees" {
        return None;
    }
    let git = worktrees.parent()?;
    (git.file_name()? == ".git").then(|| git.parent().map(Path::to_owned))?
}

/// The entry of riff in the settings text `text`, when it is `true` or
/// `false`.
///
/// ```
/// use riff::enable::entry;
///
/// assert_eq!(entry(r#"{"enabledPlugins": {"riff@riff": true}}"#), Some(true));
/// assert_eq!(entry(r#"{"enabledPlugins": {"riff@riff": false}}"#), Some(false));
/// assert_eq!(entry(r#"{"enabledPlugins": {"other@riff": true}}"#), None);
/// assert_eq!(entry("not JSON"), None);
/// ```
pub fn entry(text: &str) -> Option<bool> {
    let value: Value = serde_json::from_str(text).ok()?;
    value.get("enabledPlugins")?.get(KEY)?.as_bool()
}

/// [`entry`] of the settings file at `path`.
pub fn entry_at(path: &Path) -> Option<bool> {
    entry(&std::fs::read_to_string(path).ok()?)
}

/// Whether riff is on in a directory, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// True when riff is on.
    pub on: bool,
    /// The file whose entry decides, and its place. `None` when no file
    /// has the entry, or `RIFF_ON` decides.
    pub by: Option<(Place, PathBuf)>,
    /// True when `RIFF_ON` turned riff on.
    pub forced: bool,
    /// The repository of the directory.
    pub repo: Option<Repo>,
}

impl State {
    /// The state in `dir`, with the user settings at `user`. `forced` is
    /// the value of [`forced`].
    ///
    /// The local settings win over the user settings, and a directory
    /// outside git is off:
    ///
    /// ```
    /// use riff::enable::{State, set};
    ///
    /// let dir = isolated::outside_git();
    /// let user = dir.path().join("user.json");
    /// set(&user, Some(true))?;
    /// assert!(!State::of(dir.path(), Some(&user), false).on);
    /// assert!(State::of(dir.path(), Some(&user), true).on);
    /// let repo = dir.path().join("app");
    /// std::fs::create_dir_all(repo.join(".git"))?;
    /// assert!(State::of(&repo, Some(&user), false).on);
    /// set(&repo.join(".claude/settings.local.json"), Some(false))?;
    /// assert!(!State::of(&repo, Some(&user), false).on);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(dir: &Path, user: Option<&Path>, forced: bool) -> State {
        let repo = Repo::of(dir);
        if forced {
            return State {
                on: true,
                by: None,
                forced,
                repo,
            };
        }
        let files = repo.iter().flat_map(Repo::files);
        let user = user.map(|path| (Place::Global, path.to_owned()));
        let found = files
            .chain(user)
            .find_map(|(place, path)| entry_at(&path).map(|on| (on, (place, path))));
        let (on, by) = match found {
            Some((on, by)) => (on && repo.is_some(), Some(by)),
            None => (false, None),
        };
        State {
            on,
            by,
            forced,
            repo,
        }
    }

    /// The state where a worker of `dir` starts: the main clone of the
    /// repository of `dir`. The shared settings of a linked worktree
    /// can turn riff on there while the main clone has it off
    /// (01M3XY2T542DCHBN95H9PX4AGQ).
    ///
    /// ```
    /// use riff::enable::{State, set};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let main = dir.path().join("app");
    /// let tree = main.join(".claude/worktrees/issue-12");
    /// let gitdir = main.join(".git/worktrees/issue-12");
    /// std::fs::create_dir_all(&gitdir)?;
    /// std::fs::create_dir_all(&tree)?;
    /// std::fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display()))?;
    /// set(&tree.join(".claude/settings.json"), Some(true))?;
    /// assert!(State::of(&tree, None, false).on);
    /// assert!(!State::of_workers(&tree, None, false).on);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of_workers(dir: &Path, user: Option<&Path>, forced: bool) -> State {
        let home = Repo::of(dir).map(|repo| repo.home().to_owned());
        State::of(home.as_deref().unwrap_or(dir), user, forced)
    }

    /// The state in the working directory of this process, with the
    /// user settings of Claude Code and `RIFF_ON` of the environment.
    /// With no working directory, riff is off.
    pub fn here() -> State {
        let user = crate::plugin::user_settings();
        match std::env::current_dir() {
            Ok(dir) => State::of(&dir, user.as_deref(), forced()),
            Err(_) => State {
                on: forced(),
                by: None,
                forced: forced(),
                repo: None,
            },
        }
    }
}

/// One member of a JSON object in a text.
struct Member {
    key: String,
    /// The place of the key.
    start: usize,
    /// The place of the value.
    value: usize,
    /// The place after the value.
    end: usize,
}

/// The place of the first byte at or after `i` that is not white space.
fn skip_space(text: &[u8], mut i: usize) -> usize {
    while text.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// The place after the JSON value that starts at `i`.
fn value_end(text: &[u8], i: usize) -> Option<usize> {
    match *text.get(i)? {
        b'"' => {
            let mut j = i + 1;
            loop {
                match *text.get(j)? {
                    b'\\' => j += 2,
                    b'"' => return Some(j + 1),
                    _ => j += 1,
                }
            }
        }
        b'{' | b'[' => {
            let (mut depth, mut j) = (0usize, i);
            loop {
                match *text.get(j)? {
                    b'"' => {
                        j = value_end(text, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
        }
        _ => {
            let stops = |c: &u8| matches!(c, b',' | b'}' | b']') || c.is_ascii_whitespace();
            Some(
                (i..text.len())
                    .find(|&j| stops(&text[j]))
                    .unwrap_or(text.len()),
            )
        }
    }
}

/// The members of the JSON object that starts at `open`, and the place
/// of its `}`. `None` when the text there is no object.
fn members(text: &str, open: usize) -> Option<(Vec<Member>, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = skip_space(bytes, open + 1);
    loop {
        match *bytes.get(i)? {
            b'}' => return Some((out, i)),
            b',' => i = skip_space(bytes, i + 1),
            b'"' => {
                let after_key = value_end(bytes, i)?;
                let key = serde_json::from_str(&text[i..after_key]).ok()?;
                let colon = skip_space(bytes, after_key);
                if bytes.get(colon) != Some(&b':') {
                    return None;
                }
                let value = skip_space(bytes, colon + 1);
                let end = value_end(bytes, value)?;
                out.push(Member {
                    key,
                    start: i,
                    value,
                    end,
                });
                i = skip_space(bytes, end);
            }
            _ => return None,
        }
    }
}

/// The white space that comes before the place `at`.
fn space_before(text: &str, at: usize) -> &str {
    let head = &text[..at];
    &head[head.trim_end().len()..]
}

/// The text with the member `key` of the object at `open` set to
/// `value`. A new member comes after the last one, with the white space
/// that the last one has before it.
fn put(text: &str, open: usize, key: &str, value: &str) -> Option<String> {
    let (members, close) = members(text, open)?;
    if let Some(member) = members.iter().find(|m| m.key == key) {
        let (head, tail) = (&text[..member.value], &text[member.end..]);
        return Some(format!("{head}{value}{tail}"));
    }
    let member = format!("{}: {value}", serde_json::to_string(key).ok()?);
    Some(match members.last() {
        Some(last) => {
            let space = match space_before(text, last.start) {
                "" => " ",
                space => space,
            };
            let (head, tail) = (&text[..last.end], &text[last.end..]);
            format!("{head},{space}{member}{tail}")
        }
        None => format!("{}{member}{}", &text[..=open], &text[close..]),
    })
}

/// The text with no member `key` in the object at `open`. The comma
/// after the member goes with it, else the comma before it.
fn take(text: &str, open: usize, key: &str) -> Option<String> {
    let (members, _) = members(text, open)?;
    let i = members.iter().position(|m| m.key == key)?;
    let member = &members[i];
    let bytes = text.as_bytes();
    let after = skip_space(bytes, member.end);
    if bytes.get(after) == Some(&b',') {
        let next = skip_space(bytes, after + 1);
        return Some(format!("{}{}", &text[..member.start], &text[next..]));
    }
    let from = match i {
        0 => open + 1,
        _ => members[i - 1].end,
    };
    Some(format!("{}{}", &text[..from], &text[member.end..]))
}

/// The settings text with the entry of riff changed as text, so that
/// each other byte of the file stays. `None` when it cannot: the caller
/// then writes the file in the plain form.
fn edit(text: &str, on: Option<bool>) -> Option<String> {
    let open = skip_space(text.as_bytes(), 0);
    let (top, close) = members(text, open)?;
    let plugins = top.iter().find(|m| m.key == "enabledPlugins");
    match (on, plugins) {
        (Some(on), Some(plugins)) => put(text, plugins.value, KEY, &on.to_string()),
        (Some(on), None) => {
            // A file with no key gets the plain form.
            let space = space_before(text, top.last()?.start);
            let key = serde_json::to_string(KEY).ok()?;
            let value = match space.rsplit_once('\n') {
                Some((_, indent)) => format!("{{\n{indent}{indent}{key}: {on}\n{indent}}}"),
                None => format!("{{{key}: {on}}}"),
            };
            put(text, open, "enabledPlugins", &value)
        }
        (None, Some(plugins)) => {
            let (inner, _) = members(text, plugins.value)?;
            if inner.iter().any(|m| m.key != KEY) {
                return take(text, plugins.value, KEY);
            }
            // An `enabledPlugins` that is empty after the change goes.
            match top.len() {
                1 => Some(format!("{}{{}}{}", &text[..open], &text[close + 1..])),
                _ => take(text, open, "enabledPlugins"),
            }
        }
        (None, None) => None,
    }
}

/// The settings text with the entry of riff set to `on`, or with no
/// entry for `None`. `None` when the text has that state already. An
/// `enabledPlugins` that is empty after the change goes.
///
/// It changes only the entry of riff, as text
/// (01M3XY2SKQ27K3TE4NV28FHTVV): each other byte of the file stays, so
/// the file keeps its indent, the order of its keys and its lines. A
/// new entry takes the white space of the member before it.
///
/// ```
/// use riff::enable::with_entry;
///
/// let text = r#"{
///     "model": "opus",
///     "permissions": {"allow": ["Bash(ls)", "Bash(cat:*)"]},
///     "enabledPlugins": {"a@b": true}
/// }
/// "#;
/// let on = with_entry(text, Some(true))?.unwrap();
/// assert_eq!(on, text.replace(r#"{"a@b": true}"#, r#"{"a@b": true, "riff@riff": true}"#));
/// assert_eq!(with_entry(&on, Some(true))?, None);
/// let no = with_entry(&on, Some(false))?.unwrap();
/// assert_eq!(no, on.replace(r#""riff@riff": true"#, r#""riff@riff": false"#));
/// assert_eq!(with_entry(&no, None)?.unwrap(), text);
///
/// // A file with no `enabledPlugins` gets the key in the form of its other keys.
/// let text = "{\n  \"model\": \"opus\"\n}\n";
/// let on = with_entry(text, Some(true))?.unwrap();
/// assert_eq!(on, "{\n  \"model\": \"opus\",\n  \"enabledPlugins\": {\n    \"riff@riff\": true\n  }\n}\n");
/// assert_eq!(with_entry(&on, None)?.unwrap(), text);
///
/// assert_eq!(with_entry(r#"{"enabledPlugins": {"riff@riff": true}}"#, None)?.unwrap(), "{}");
/// assert_eq!(with_entry("{}", None)?, None);
/// assert!(with_entry("[1]", Some(true)).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_entry(text: &str, on: Option<bool>) -> Result<Option<String>> {
    let mut value: Value = serde_json::from_str(text).context("the settings are not valid JSON")?;
    let object = value
        .as_object_mut()
        .context("the settings are not a JSON object")?;
    let have = match object.get("enabledPlugins") {
        Some(plugins) => plugins
            .as_object()
            .context("`enabledPlugins` is not a JSON object")?
            .get(KEY)
            .cloned(),
        None => None,
    };
    if have == on.map(Value::Bool) {
        return Ok(None);
    }
    match on {
        Some(on) => {
            let plugins = object
                .entry("enabledPlugins")
                .or_insert_with(|| Value::Object(Map::new()));
            plugins[KEY] = Value::Bool(on);
        }
        None => {
            let empty = object["enabledPlugins"]
                .as_object_mut()
                .is_some_and(|plugins| {
                    plugins.shift_remove(KEY);
                    plugins.is_empty()
                });
            if empty {
                object.shift_remove("enabledPlugins");
            }
        }
    }
    // The text edit must give the same settings. If it does not, for
    // example for a file with a key two times, riff writes the plain
    // form.
    let same = |new: &String| serde_json::from_str::<Value>(new).is_ok_and(|v| v == value);
    if let Some(new) = edit(text, on).filter(same) {
        return Ok(Some(new));
    }
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(Some(out))
}

/// Sets the entry of riff in the settings file at `path` to `on`, or
/// removes it for `None`. It makes the file when it is not there and
/// `on` is a value. It writes the file only when it changes. True when
/// it changed the file. It writes through a symbolic link: a person
/// who keeps the settings in another place made the link
/// (01M3YCGKGP3VC93S8FA1G4K3QK).
pub fn set(path: &Path, on: Option<bool>) -> Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if on.is_none() {
                return Ok(false);
            }
            "{}".to_owned()
        }
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let Some(new) = with_entry(&text, on).with_context(|| path.display().to_string())? else {
        return Ok(false);
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("make {}", parent.display()))?;
    }
    std::fs::write(path, new).with_context(|| format!("write {}", path.display()))?;
    Ok(true)
}

/// The file that a write to `file` changes: the path with each symbolic
/// link followed (01M3YCGKGP3VC93S8FA1G4K3QK). A file that is not there gives `file`.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let dir = dir.path().canonicalize()?;
/// std::fs::write(dir.join("real.json"), "{}")?;
/// std::os::unix::fs::symlink(dir.join("real.json"), dir.join("link.json"))?;
/// assert_eq!(riff::enable::real(&dir.join("link.json")), dir.join("real.json"));
/// assert_eq!(riff::enable::real(&dir.join("none.json")), dir.join("none.json"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn real(file: &Path) -> PathBuf {
    std::fs::canonicalize(file).unwrap_or_else(|_| file.to_owned())
}

/// The repository of `dir` for a write to the file of `place`.
///
/// The local settings of a linked worktree are in the main clone, and
/// [`Repo::of`] takes the main clone from the text of the `.git` file.
/// A tree from an archive can have a `.git` file that names each
/// directory. So before riff writes the settings of a main clone, two
/// checks must pass (01M3YCGKGP3VC93S8FA1G4K3QK), else the command
/// refuses and says why:
///
/// - git confirms the worktree: the common directory of the worktree
///   ([`crate::identity::common_dir`]) is the `.git` of that main
///   clone.
/// - The main clone names this tree: the file `gitdir` of the entry
///   `MAIN/.git/worktrees/NAME` names the `.git` file of the tree. git
///   writes that file when it makes the worktree. So a `.git` file that
///   names the entry of another worktree of the main clone is refused.
///   The `.git` of the tree is that file itself: a symbolic link at
///   `TOP/.git` is refused, and the check does not follow a link at
///   the last part of a path (01M3ZGT8ST7HCK6J7VZJ09XE0M).
fn for_write(dir: &Path, place: Place) -> Result<Option<Repo>> {
    let repo = Repo::of(dir);
    if let (
        Place::Local,
        Some(Repo {
            top,
            main: Some(main),
        }),
    ) = (place, &repo)
    {
        let same = |a: &Path, b: &Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b);
        let common = crate::identity::common_dir(top);
        let confirmed = common.is_some_and(|common| same(&common, &main.join(".git")));
        let git_file = top.join(".git");
        if git_file
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            anyhow::bail!(
                "{} is a symbolic link, not the .git file of a worktree of {}, so riff writes \
                 no settings there. Run the command in the main clone, or use --shared",
                git_file.display(),
                main.display()
            );
        }
        // The same file: the directories with each link followed, and
        // the same last part, with no link followed.
        let same_file = |a: &Path, b: &Path| {
            let dir = |p: &Path| Some(p.parent()?.canonicalize().ok()?.join(p.file_name()?));
            matches!((dir(a), dir(b)), (Some(a), Some(b)) if a == b)
        };
        // The path in `gitdir` can be relative to the entry.
        let named = std::fs::read_to_string(&git_file)
            .ok()
            .and_then(|text| entry_of(top, &text))
            .and_then(|entry| {
                let back = std::fs::read_to_string(entry.join("gitdir")).ok()?;
                Some(entry.join(back.trim()))
            });
        if !confirmed || !named.is_some_and(|named| same_file(&named, &git_file)) {
            anyhow::bail!(
                "git does not confirm that {} is a worktree of {}, so riff writes no settings \
                 there. Run the command in the main clone, or use --shared",
                top.display(),
                main.display()
            );
        }
    }
    Ok(repo)
}

/// What `riff enable` or `riff disable` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The file of the command: the real path of the file that it
    /// wrote, with each symbolic link followed.
    pub file: PathBuf,
    /// True when the command changed a file.
    pub changed: bool,
    /// The file where `riff disable` wrote `false`, because riff was
    /// still on through another file.
    pub denied: Option<PathBuf>,
    /// The state after the command.
    pub state: State,
}

/// The file of `place` for the repository `repo`, with the user
/// settings at `user`. The local settings are in the main clone. An
/// error when the place needs a repository or the user settings, and
/// there is none.
pub fn file(place: Place, repo: Option<&Repo>, user: Option<&Path>) -> Result<PathBuf> {
    let no_repo = "this directory is not in a git repository: go to a repository, or use --global";
    match place {
        Place::Global => user
            .map(Path::to_owned)
            .context("set HOME or CLAUDE_CONFIG_DIR"),
        Place::Local => Ok(Repo::file(repo.context(no_repo)?.home(), place)),
        Place::Shared => Ok(Repo::file(&repo.context(no_repo)?.top, place)),
    }
}

/// `riff enable`: sets the entry to `true` in the file of `place` for
/// the directory `dir` (01M3XY2SKQ27K3TE4NV28FHTVV).
///
/// ```
/// use riff::enable::{Place, disable, enable};
///
/// let dir = tempfile::tempdir()?;
/// std::fs::create_dir(dir.path().join(".git"))?;
/// let user = dir.path().join("user.json");
/// let done = enable(dir.path(), Place::Shared, Some(&user))?;
/// assert_eq!(done.file, dir.path().join(".claude/settings.json"));
/// assert!(done.changed && done.state.on);
/// // The shared settings still turn riff on, so the local settings say no.
/// let done = disable(dir.path(), Place::Local, Some(&user))?;
/// assert_eq!(done.denied, Some(dir.path().join(".claude/settings.local.json")));
/// assert!(!done.state.on);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn enable(dir: &Path, place: Place, user: Option<&Path>) -> Result<Changed> {
    let file = file(place, for_write(dir, place)?.as_ref(), user)?;
    let changed = set(&file, Some(true))?;
    Ok(Changed {
        file: real(&file),
        changed,
        denied: None,
        state: State::of(dir, user, false),
    })
}

/// `riff disable`: removes the entry from the file of `place` for the
/// directory `dir`. For [`Place::Local`], when riff is then still on
/// through another file, it writes `false` to the local settings: the
/// repository is off, and no other repository changes
/// (01M3XY2SKQ27K3TE4NV28FHTVV).
pub fn disable(dir: &Path, place: Place, user: Option<&Path>) -> Result<Changed> {
    let file = file(place, for_write(dir, place)?.as_ref(), user)?;
    let mut changed = set(&file, None)?;
    let mut denied = None;
    if place == Place::Local && State::of(dir, user, false).on {
        changed |= set(&file, Some(false))?;
        denied = Some(real(&file));
    }
    Ok(Changed {
        file: real(&file),
        changed,
        denied,
        state: State::of(dir, user, false),
    })
}

/// Where a person wants riff on: the answer to the scope question of
/// `riff connect claude` (01M3XY2SNXQJRSH5QX82AFVM2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Only in the repository of the working directory.
    Repo,
    /// In each repository on this machine.
    Global,
    /// Nowhere now. The person runs `riff enable` later.
    None,
}

impl Scope {
    /// The name of the scope in `--scope` and in the settings of riff.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Repo => "repo",
            Scope::Global => "global",
            Scope::None => "none",
        }
    }

    /// The scope with the name `name`.
    ///
    /// ```
    /// use riff::enable::Scope;
    ///
    /// assert_eq!(Scope::parse("global"), Some(Scope::Global));
    /// assert_eq!(Scope::parse(Scope::Repo.as_str()), Some(Scope::Repo));
    /// assert_eq!(Scope::parse("user"), None);
    /// ```
    pub fn parse(name: &str) -> Option<Scope> {
        [Scope::Repo, Scope::Global, Scope::None]
            .into_iter()
            .find(|scope| scope.as_str() == name)
    }
}

/// The scope question of `riff connect claude`.
pub const ASK_SCOPE: &str = "Where do you want riff on?\n  \
     1) Only in this repository (default)\n  \
     2) In each repository on this machine\n  \
     3) Not now: I run `riff enable` later\n\
     Your choice [1]: ";

/// Asks the scope question when `terminal` is true. It writes
/// [`ASK_SCOPE`] to `out` and reads one line of `input`: Enter or `1`
/// is [`Scope::Repo`], `2` is [`Scope::Global`], and each other answer
/// is [`Scope::None`], the least of the three. `None` when it did not
/// ask, or the input ended.
///
/// ```
/// use riff::enable::{Scope, ask_scope};
///
/// let mut out = Vec::new();
/// assert_eq!(ask_scope(false, &mut &b"2\n"[..], &mut out)?, None);
/// assert!(out.is_empty());
/// assert_eq!(ask_scope(true, &mut &b"\n"[..], &mut out)?, Some(Scope::Repo));
/// assert_eq!(String::from_utf8(out)?, riff::enable::ASK_SCOPE);
/// let mut out = Vec::new();
/// assert_eq!(ask_scope(true, &mut &b"2\n"[..], &mut out)?, Some(Scope::Global));
/// assert_eq!(ask_scope(true, &mut &b"x\n"[..], &mut out)?, Some(Scope::None));
/// assert_eq!(ask_scope(true, &mut &b""[..], &mut out)?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn ask_scope(
    terminal: bool,
    input: &mut impl io::BufRead,
    out: &mut impl io::Write,
) -> Result<Option<Scope>> {
    if !terminal {
        return Ok(None);
    }
    write!(out, "{ASK_SCOPE}")?;
    out.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        return Ok(None);
    }
    Ok(Some(match answer.trim() {
        "" | "1" => Scope::Repo,
        "2" => Scope::Global,
        _ => Scope::None,
    }))
}

/// What [`scope`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoped {
    /// The answer that it applied. `None` when nobody answered now: it
    /// kept the earlier choice.
    pub answer: Option<Scope>,
    /// The repositories that used riff, when it took an entry `true`
    /// out of the user settings: the person answered `repo` or `none`.
    /// `None` when it took none out.
    pub moved: Option<Vec<PathBuf>>,
    /// The state of the working directory after the change.
    pub state: State,
    /// True when the user settings turn riff on after the change.
    pub global: bool,
    /// True when the install is old, and riff kept its choice with no
    /// question: each repository.
    pub kept: bool,
}

/// The files that [`scope`] reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct Files<'a> {
    /// The user settings of Claude Code.
    pub user: Option<&'a Path>,
    /// The settings of riff.
    pub riff: &'a Path,
    /// The state file of Claude Code.
    pub claude: Option<&'a Path>,
}

/// The scope step of `riff connect claude` in the directory `dir`
/// (01M3XY2SNXQJRSH5QX82AFVM2S). `flag` is `--scope`. `ask` asks the
/// person, and gives `None` with no terminal.
///
/// ```mermaid
/// flowchart TD
///     F{"--scope?"} -- yes --> A[apply it, keep it in the settings of riff]
///     F -- no --> R{"an earlier answer<br/>in the settings of riff?"}
///     R -- yes --> K[change nothing]
///     R -- no --> O{"user settings<br/>turn riff on?"}
///     O -- "yes: an old install" --> G2["keep the entry, record the answer global"]
///     O -- no --> T{a terminal?}
///     T -- yes --> Q[ask] --> A
///     T -- "no: an update" --> K
///     A --> G{"the answer is repo or none,<br/>and the user settings turn riff on?"}
///     G -- yes --> M[take the entry out, name the repositories]
/// ```
///
/// - It asks one time: an answer in the settings of riff stops the
///   question.
/// - Only the answer `global` writes `true` to the user settings. So an
///   update, which has no terminal, never turns the global scope on,
///   and keeps an earlier choice (01M3XY2SR3VJZAKEPC6CBCS292).
/// - An old install, of a release up to v0.8.0, has `true` in the user
///   settings and no answer: that release installed the plugin in the
///   user scope. Its choice is each repository. riff asks nothing, with
///   a terminal and with no terminal: it keeps the entry, and records
///   the answer `global`, so that no later run asks. The first update
///   from v0.8.0 runs the old `riff update`, which gives this command
///   the terminal of the person: a question there, with Enter for
///   "this repository", would turn riff off in each repository.
/// - `--scope repo` or `--scope none` takes the entry of the user
///   settings out. riff then names each repository that used riff
///   ([`used`]).
///
/// ```
/// use riff::enable::{Files, Scope, scope, set};
///
/// let dir = tempfile::tempdir()?;
/// let user = dir.path().join("user.json");
/// let files = Files { user: Some(&user), riff: &dir.path().join("config.toml"), claude: None };
/// // An old install: riff asks nothing, and stays on in each repository.
/// set(&user, Some(true))?;
/// let done = scope(dir.path(), files, None, || unreachable!())?;
/// assert_eq!((done.answer, done.kept, done.global), (Some(Scope::Global), true, true));
/// // The answer is kept: the next run changes nothing, and asks nothing.
/// let done = scope(dir.path(), files, None, || unreachable!())?;
/// assert_eq!((done.answer, done.kept, done.global), (None, false, true));
/// // `--scope none`: the entry of the user settings goes.
/// let done = scope(dir.path(), files, Some(Scope::None), || unreachable!())?;
/// assert_eq!((done.moved, done.global), (Some(vec![]), false));
/// // A new install with a terminal gets the question.
/// let new = Files { riff: &dir.path().join("new.toml"), ..files };
/// let done = scope(dir.path(), new, None, || Ok(Some(Scope::None)))?;
/// assert_eq!((done.answer, done.kept), (Some(Scope::None), false));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn scope(
    dir: &Path,
    files: Files,
    flag: Option<Scope>,
    ask: impl FnOnce() -> Result<Option<Scope>>,
) -> Result<Scoped> {
    let earlier = crate::settings::connect_scope(files.riff)?;
    let global = || files.user.and_then(entry_at) == Some(true);
    // An old install: its choice is each repository. riff asks nothing.
    let kept = flag.is_none() && earlier.is_none() && global();
    let answer = match (flag, earlier) {
        (Some(flag), _) => Some(flag),
        (None, Some(_)) => None,
        (None, None) if kept => Some(Scope::Global),
        (None, None) => ask()?,
    };
    let take_out = matches!(answer, Some(Scope::Repo | Scope::None));
    let mut moved = None;
    if take_out && global() {
        if let Some(user) = files.user {
            set(user, None)?;
        }
        let text = files.claude.and_then(|p| std::fs::read_to_string(p).ok());
        moved = Some(used(&text.unwrap_or_default()));
    }
    match answer {
        Some(Scope::Global) => {
            set(&file(Place::Global, None, files.user)?, Some(true))?;
        }
        Some(Scope::Repo) if Repo::of(dir).is_some() => {
            enable(dir, Place::Local, files.user)?;
        }
        _ => {}
    }
    if let Some(answer) = answer {
        crate::settings::set_connect_scope(files.riff, answer)?;
    }
    Ok(Scoped {
        answer,
        moved,
        state: State::of(dir, files.user, false),
        global: global(),
        kept,
    })
}

/// The state file of Claude Code: `$CLAUDE_CONFIG_DIR/.claude.json`, or
/// `$HOME/.claude.json`.
///
/// ```
/// use std::path::Path;
///
/// let p = riff::enable::claude_state_from(None, Some("/home/mike".into()));
/// assert_eq!(p.as_deref(), Some(Path::new("/home/mike/.claude.json")));
/// let p = riff::enable::claude_state_from(Some("/cfg".into()), Some("/home/mike".into()));
/// assert_eq!(p.as_deref(), Some(Path::new("/cfg/.claude.json")));
/// ```
pub fn claude_state_from(
    config_dir: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let dir = config_dir.filter(|d| !d.is_empty()).or(home)?;
    Some(PathBuf::from(dir).join(".claude.json"))
}

/// [`claude_state_from`] with the values from the environment.
pub fn claude_state() -> Option<PathBuf> {
    claude_state_from(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("HOME"),
    )
}

/// The projects of the Claude Code state text `text`: each path with
/// its record.
fn projects(text: &str) -> Vec<(PathBuf, Value)> {
    let value: Value = serde_json::from_str(text).unwrap_or_default();
    let projects = value.get("projects").and_then(Value::as_object);
    projects
        .into_iter()
        .flatten()
        .map(|(path, record)| (PathBuf::from(path), record.clone()))
        .collect()
}

/// True when a person turned the riff server off for a project of
/// `dirs` in the `/mcp` dialog of Claude Code: the record of the
/// project in the state text `text` has [`MCP_SERVER`] in
/// `disabledMcpServers` (01M3XY2T0R2Q39XYX8AYV7T0RK).
///
/// ```
/// use std::path::Path;
///
/// let text = r#"{"projects": {"/r": {"disabledMcpServers": ["plugin:riff:riff"]},
///                              "/s": {"disabledMcpServers": ["github"]}}}"#;
/// assert!(riff::enable::mcp_off(text, &[Path::new("/r/sub"), Path::new("/r")]));
/// assert!(!riff::enable::mcp_off(text, &[Path::new("/s")]));
/// assert!(!riff::enable::mcp_off("", &[Path::new("/r")]));
/// ```
pub fn mcp_off(text: &str, dirs: &[&Path]) -> bool {
    projects(text).iter().any(|(path, record)| {
        let off = record.get("disabledMcpServers").and_then(Value::as_array);
        dirs.contains(&path.as_path()) && off.is_some_and(|off| off.iter().any(|s| s == MCP_SERVER))
    })
}

/// [`mcp_off`] for the working directory `dir` and its repository, with
/// the state file of Claude Code at `state`.
pub fn mcp_off_in(state: Option<&Path>, dir: &Path, repo: Option<&Repo>) -> bool {
    let Some(text) = state.and_then(|path| std::fs::read_to_string(path).ok()) else {
        return false;
    };
    let tops = repo
        .into_iter()
        .flat_map(|r| [Some(&r.top), r.main.as_ref()]);
    let dirs: Vec<&Path> = std::iter::once(dir)
        .chain(tops.flatten().map(PathBuf::as_path))
        .collect();
    mcp_off(&text, &dirs)
}

/// The repositories of the Claude Code state text `text` that used
/// riff: each project that is a main clone and whose project settings
/// or local settings name riff. `riff connect claude` names them when
/// a person takes the entry of the user settings out
/// (01M3XY2SR3VJZAKEPC6CBCS292).
pub fn used(text: &str) -> Vec<PathBuf> {
    let names_riff = |path: PathBuf| {
        std::fs::read_to_string(path).is_ok_and(|text| text.contains("mcp__plugin_riff_riff"))
    };
    projects(text)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| Repo::of(path).is_some_and(|r| r.top == *path && r.main.is_none()))
        .filter(|path| {
            names_riff(Repo::file(path, Place::Shared))
                || names_riff(Repo::file(path, Place::Local))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &Path) -> PathBuf {
        let main = dir.join("app");
        std::fs::create_dir_all(main.join(".git/worktrees/issue-12")).unwrap();
        main
    }

    fn linked(main: &Path) -> PathBuf {
        let tree = main.join(".claude/worktrees/issue-12");
        std::fs::create_dir_all(&tree).unwrap();
        let gitdir = main.join(".git/worktrees/issue-12");
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        tree
    }

    /// A main clone and a linked worktree of it, both made by git.
    fn git_worktree(dir: &Path) -> (PathBuf, PathBuf) {
        std::fs::create_dir_all(dir).unwrap();
        let main = dir.canonicalize().unwrap().join("app");
        let tree = main.join(".claude/worktrees/issue-12");
        std::fs::create_dir(&main).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&main)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        git(&["init", "-q"]);
        git(&["commit", "-q", "--allow-empty", "-m", "x"]);
        git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "issue-12",
            tree.to_str().unwrap(),
        ]);
        (main, tree)
    }

    /// 01M3XY2T2YEV7GT7DKJHSMMHYR.
    #[test]
    fn a_linked_worktree_reads_the_local_settings_of_the_main_clone() {
        let dir = tempfile::tempdir().unwrap();
        let (main, tree) = git_worktree(dir.path());
        assert!(!State::of(&tree, None, false).on);
        let done = enable(&tree, Place::Local, None).unwrap();
        assert_eq!(done.file, main.join(".claude/settings.local.json"));
        assert!(done.state.on);
        assert!(State::of(&main, None, false).on);
    }

    /// 01M3YCGKGP3VC93S8FA1G4K3QK: the text of a `.git` file names a
    /// main clone, and git does not confirm it. riff writes nothing
    /// there.
    #[test]
    fn enable_writes_no_settings_of_a_main_clone_that_git_does_not_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let main = repo(dir.path());
        let tree = linked(&main);
        for change in [enable, disable] {
            let error = change(&tree, Place::Local, None).unwrap_err();
            let error = format!("{error:#}");
            assert!(error.contains("git does not confirm"), "{error}");
            assert!(error.contains("--shared"), "{error}");
        }
        let settings = main.join(".claude/settings.local.json");
        assert!(!settings.exists(), "riff wrote in the main clone");
        // The shared settings are in the tree itself: no main clone.
        let done = enable(&tree, Place::Shared, None).unwrap();
        assert_eq!(done.file, real(&tree.join(".claude/settings.json")));

        // A `.git` file that names the entry of a real worktree of
        // another repository: git gives the common directory of that
        // repository, but the entry names its own worktree.
        let (victim, worktree) = git_worktree(&dir.path().join("v"));
        let copy = dir.path().join("copy");
        std::fs::create_dir(&copy).unwrap();
        let entry = victim.join(".git/worktrees/issue-12");
        std::fs::write(copy.join(".git"), format!("gitdir: {}\n", entry.display())).unwrap();
        assert_eq!(Repo::of(&copy).unwrap().main, Some(victim.clone()));
        let error = enable(&copy, Place::Local, None).unwrap_err();
        assert!(format!("{error:#}").contains("git does not confirm"));
        assert!(!victim.join(".claude/settings.local.json").exists());
        // The worktree that git made passes.
        assert!(enable(&worktree, Place::Local, None).unwrap().state.on);
    }

    /// 01M3ZGT8ST7HCK6J7VZJ09XE0M: a tree whose `.git` is a symbolic
    /// link to the `.git` file of a worktree of another repository gets
    /// no write. git and the entry both confirm the link target, so only
    /// the link check stops it.
    #[test]
    fn a_dot_git_that_is_a_symbolic_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (victim, worktree) = git_worktree(&dir.path().join("v"));
        let tree = dir.path().canonicalize().unwrap().join("tree");
        std::fs::create_dir(&tree).unwrap();
        std::os::unix::fs::symlink(worktree.join(".git"), tree.join(".git")).unwrap();
        assert_eq!(Repo::of(&tree).unwrap().main, Some(victim.clone()));
        // The worktree is in `victim/.claude`, so count its entries.
        let entries = |at: &Path| std::fs::read_dir(at).unwrap().count();
        let before = (entries(&victim), entries(&victim.join(".claude")));
        for change in [enable, disable] {
            let error = format!("{:#}", change(&tree, Place::Local, None).unwrap_err());
            assert!(error.contains("is a symbolic link"), "{error}");
            assert!(error.contains(&victim.display().to_string()), "{error}");
            assert!(error.contains("--shared"), "{error}");
        }
        let settings = victim.join(".claude/settings.local.json");
        assert!(!settings.exists(), "riff wrote in the victim");
        assert!(
            !worktree.join(".claude").exists(),
            "riff wrote in the worktree"
        );
        let after = (entries(&victim), entries(&victim.join(".claude")));
        assert_eq!(after, before);
        // The worktree that git made still passes.
        assert!(enable(&worktree, Place::Local, None).unwrap().state.on);
    }

    /// 01M3YCGKGP3VC93S8FA1G4K3QK: a worktree that git made passes in
    /// each form: from a subdirectory, after `git worktree move`, and
    /// through a main clone behind a symbolic link.
    #[test]
    fn a_real_worktree_passes_in_each_form() {
        let dir = tempfile::tempdir().unwrap();
        let (main, tree) = git_worktree(dir.path());
        let settings = main.join(".claude/settings.local.json");
        let on = |at: &Path| {
            let done = enable(at, Place::Local, None).unwrap();
            assert_eq!(done.file, settings, "{}", at.display());
            assert!(done.state.on, "{}", at.display());
            disable(at, Place::Local, None).unwrap();
            assert_eq!(entry_at(&settings), None, "{}", at.display());
        };
        // A subdirectory.
        let sub = tree.join("src/deep");
        std::fs::create_dir_all(&sub).unwrap();
        on(&sub);
        // The main clone behind a symbolic link.
        let link = dir.path().canonicalize().unwrap().join("link");
        std::os::unix::fs::symlink(&main, &link).unwrap();
        on(&link.join(".claude/worktrees/issue-12"));
        // A moved worktree.
        let moved = dir.path().canonicalize().unwrap().join("moved");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(["worktree", "move"])
            .args([&tree, &moved])
            .output()
            .unwrap();
        assert!(out.status.success(), "git worktree move: {out:?}");
        on(&moved);
    }

    /// 01M3YCGKGP3VC93S8FA1G4K3QK: riff writes through a symbolic link,
    /// and names the real path of the file. The link stays.
    #[test]
    fn enable_writes_through_a_symbolic_link_and_names_the_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().canonicalize().unwrap();
        let repo = dir.join("app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join(".claude")).unwrap();
        let target = dir.join("dotfiles/settings.json");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "{\"keep\": 1}\n").unwrap();
        let link = repo.join(".claude/settings.local.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let done = enable(&repo, Place::Local, None).unwrap();
        assert_eq!(done.file, target);
        assert!(done.changed && done.state.on);
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        let text = std::fs::read_to_string(&target).unwrap();
        assert_eq!(
            text,
            "{\"keep\": 1, \"enabledPlugins\": {\"riff@riff\": true}}\n"
        );
        let done = disable(&repo, Place::Local, None).unwrap();
        assert_eq!(done.file, target);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"keep\": 1}\n");
    }

    /// 01M3XY2SKQ27K3TE4NV28FHTVV: each byte of the rest of the file
    /// stays, for each form of a settings file.
    #[test]
    fn a_change_of_the_entry_keeps_each_other_byte_of_the_file() {
        let forms = [
            "{\n    \"model\": \"opus\",\n    \"permissions\": {\"allow\": [\"Bash(ls)\", \"Bash(cat:*)\"]}\n}\n",
            "{\n\t\"a\": \"x, y } \\\" {\",\n\t\"enabledPlugins\": {\n\t\t\"a@b\": true\n\t},\n\t\"z\": [1, 2]\n}",
            "{\"enabledPlugins\":{\"x@y\":false},\"b\":null}",
            "  {\"a\":1}  \n",
            "{\n  \"enabledPlugins\": {\n    \"riff@riff\": false,\n    \"a@b\": true\n  }\n}\n",
        ];
        for form in forms {
            let had = entry(form);
            let on = with_entry(form, Some(true)).unwrap().unwrap();
            assert_eq!(entry(&on), Some(true), "{on}");
            // The text with no entry of riff is the same before and after.
            let none = |text: &str| with_entry(text, None).unwrap().unwrap_or(text.to_owned());
            assert_eq!(none(&on), none(form), "{form}");
            if had.is_none() {
                assert_eq!(none(&on), form, "enable, then disable: {form}");
            }
            let no = with_entry(&on, Some(false)).unwrap().unwrap();
            assert_eq!(
                no,
                on.replace("\"riff@riff\": true", "\"riff@riff\": false")
            );
            // Each line of the file that has no part of the entry stays.
            let lines = form.lines().filter(|_| form.lines().count() > 1);
            for line in lines.filter(|l| !l.contains("enabledPlugins") && !l.contains(KEY)) {
                let line = line.trim_end_matches(',');
                assert!(on.contains(line), "{line:?} is not in {on:?}");
            }
        }
        // A key two times: the text edit would change the wrong one, so
        // riff writes the plain form, with the right values.
        let twice =
            "{\"enabledPlugins\": {\"riff@riff\": false}, \"enabledPlugins\": {\"a@b\": true}}";
        let on = with_entry(twice, Some(true)).unwrap().unwrap();
        assert_eq!(entry(&on), Some(true));
    }

    #[test]
    fn a_no_in_the_main_clone_wins_over_the_shared_settings_of_a_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let main = repo(dir.path());
        let tree = linked(&main);
        set(&tree.join(".claude/settings.json"), Some(true)).unwrap();
        assert!(State::of(&tree, None, false).on);
        set(&main.join(".claude/settings.local.json"), Some(false)).unwrap();
        let state = State::of(&tree, None, false);
        assert!(!state.on);
        assert_eq!(state.by.unwrap().0, Place::Local);
    }

    #[test]
    fn a_submodule_is_a_repository_with_no_main_clone() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(Repo::of(&sub).unwrap().main, None);
    }

    /// 01M3XY2SKQ27K3TE4NV28FHTVV.
    #[test]
    fn disable_in_one_repository_changes_nothing_in_another() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user.json");
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for repo in [&a, &b] {
            std::fs::create_dir_all(repo.join(".git")).unwrap();
        }
        enable(&a, Place::Global, Some(&user)).unwrap();
        let before = std::fs::read_to_string(&user).unwrap();
        let done = disable(&a, Place::Local, Some(&user)).unwrap();
        assert!(!done.state.on);
        assert!(State::of(&b, Some(&user), false).on);
        assert_eq!(std::fs::read_to_string(&user).unwrap(), before);
        // `riff enable` takes the no away again.
        assert!(enable(&a, Place::Local, Some(&user)).unwrap().state.on);
    }

    #[test]
    fn disable_with_no_file_makes_no_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let done = disable(dir.path(), Place::Local, None).unwrap();
        assert!(!done.changed && !done.file.exists());
    }

    #[test]
    fn a_place_in_a_repository_needs_a_repository() {
        let dir = isolated::outside_git();
        let error = enable(dir.path(), Place::Local, None).unwrap_err();
        assert!(format!("{error:#}").contains("--global"), "{error:#}");
    }

    #[test]
    fn a_test_with_its_temp_dir_in_a_repository_writes_nothing_there() {
        // The TMPDIR of a worker, under a home that is a repository.
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".git")).unwrap();
        let tmp = home.path().join("tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        let mut roots = vec![tmp];
        roots.extend(isolated::temp_roots());
        let dir = isolated::outside_git_from(&roots);
        assert!(enable(dir.path(), Place::Local, None).is_err());
        assert!(!home.path().join(".claude").exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn used_names_the_main_clones_with_the_riff_rules() {
        let dir = tempfile::tempdir().unwrap();
        let main = repo(dir.path());
        let tree = linked(&main);
        let other = dir.path().join("other");
        std::fs::create_dir_all(other.join(".git")).unwrap();
        std::fs::create_dir_all(main.join(".claude")).unwrap();
        std::fs::write(
            main.join(".claude/settings.json"),
            r#"{"permissions": {"allow": ["mcp__plugin_riff_riff"]}}"#,
        )
        .unwrap();
        let text = serde_json::json!({"projects": {
            main.to_str().unwrap(): {}, tree.to_str().unwrap(): {}, other.to_str().unwrap(): {},
        }})
        .to_string();
        assert_eq!(used(&text), [main]);
    }
}
