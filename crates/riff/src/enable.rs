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
    /// let dir = tempfile::tempdir()?;
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

/// The top of the main clone from the text of the `.git` file of the
/// linked worktree at `top`: `gitdir: MAIN/.git/worktrees/NAME`.
fn main_of(top: &Path, text: &str) -> Option<PathBuf> {
    let gitdir = top.join(text.strip_prefix("gitdir:")?.trim());
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
    /// let dir = tempfile::tempdir()?;
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

/// The settings text with the entry of riff set to `on`, or with no
/// entry for `None`. `None` when the text has that state already. It
/// keeps each other key in its order. An `enabledPlugins` that is empty
/// after the change goes.
///
/// ```
/// use riff::enable::with_entry;
///
/// let text = r#"{"model": "opus", "enabledPlugins": {"a@b": true}}"#;
/// let on = with_entry(text, Some(true))?.unwrap();
/// let value: serde_json::Value = serde_json::from_str(&on)?;
/// assert_eq!(value["enabledPlugins"]["riff@riff"], true);
/// assert_eq!(value["enabledPlugins"]["a@b"], true);
/// assert_eq!(value["model"], "opus");
/// assert_eq!(with_entry(&on, Some(true))?, None);
/// let off = with_entry(&on, None)?.unwrap();
/// assert_eq!(serde_json::from_str::<serde_json::Value>(&off)?, serde_json::from_str::<serde_json::Value>(text)?);
/// assert_eq!(with_entry(r#"{"enabledPlugins": {"riff@riff": true}}"#, None)?.unwrap(), "{}\n");
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
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(Some(out))
}

/// Sets the entry of riff in the settings file at `path` to `on`, or
/// removes it for `None`. It makes the file when it is not there and
/// `on` is a value. It writes the file only when it changes. True when
/// it changed the file.
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

/// What `riff enable` or `riff disable` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The file of the command.
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
    let file = file(place, Repo::of(dir).as_ref(), user)?;
    let changed = set(&file, Some(true))?;
    Ok(Changed {
        file,
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
    let file = file(place, Repo::of(dir).as_ref(), user)?;
    let mut changed = set(&file, None)?;
    let mut denied = None;
    if place == Place::Local && State::of(dir, user, false).on {
        changed |= set(&file, Some(false))?;
        denied = Some(file.clone());
    }
    Ok(Changed {
        file,
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
    /// out of the user settings. `None` when it took none out.
    pub moved: Option<Vec<PathBuf>>,
    /// The state of the working directory after the change.
    pub state: State,
    /// True when the user settings turn riff on after the change.
    pub global: bool,
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
///     R -- no --> T{a terminal?}
///     T -- yes --> Q[ask] --> A
///     T -- no --> O{"user settings<br/>turn riff on?"}
///     O -- "yes: an old install" --> M[take the entry out, name the repositories]
///     O -- no --> K
/// ```
///
/// - It asks one time: an answer in the settings of riff stops the
///   question.
/// - Only the answer `global` writes `true` to the user settings. So an
///   update, which has no terminal, never turns the global scope on,
///   and keeps an earlier choice (01M3XY2SR3VJZAKEPC6CBCS292).
/// - An old install has `true` in the user settings and no answer.
///   With no terminal, riff takes the entry out: riff is installed and
///   off. It names each repository that used riff ([`used`]).
///
/// ```
/// use riff::enable::{Files, Scope, scope, set};
///
/// let dir = tempfile::tempdir()?;
/// let user = dir.path().join("user.json");
/// let files = Files { user: Some(&user), riff: &dir.path().join("config.toml"), claude: None };
/// // An old install, and nobody to ask: riff is off after it.
/// set(&user, Some(true))?;
/// let done = scope(dir.path(), files, None, || Ok(None))?;
/// assert_eq!(done.moved, Some(vec![]));
/// assert!(!done.global);
/// // The person answers: each repository.
/// let done = scope(dir.path(), files, None, || Ok(Some(Scope::Global)))?;
/// assert!(done.global);
/// // The answer is kept, so riff asks no more, and an update changes nothing.
/// let done = scope(dir.path(), files, None, || unreachable!())?;
/// assert_eq!((done.answer, done.moved, done.global), (None, None, true));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn scope(
    dir: &Path,
    files: Files,
    flag: Option<Scope>,
    ask: impl FnOnce() -> Result<Option<Scope>>,
) -> Result<Scoped> {
    let earlier = crate::settings::connect_scope(files.riff)?;
    let answer = match (flag, earlier) {
        (Some(flag), _) => Some(flag),
        (None, Some(_)) => None,
        (None, None) => ask()?,
    };
    let global = || files.user.and_then(entry_at) == Some(true);
    let take_out = match answer {
        Some(Scope::Global) => false,
        Some(_) => true,
        None => earlier.is_none(),
    };
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
    let tops = repo.into_iter().flat_map(|r| [Some(&r.top), r.main.as_ref()]);
    let dirs: Vec<&Path> = std::iter::once(dir)
        .chain(tops.flatten().map(PathBuf::as_path))
        .collect();
    mcp_off(&text, &dirs)
}

/// The repositories of the Claude Code state text `text` that used
/// riff: each project that is a main clone and whose project settings
/// or local settings name riff. `riff connect claude` names them when
/// it moves an old install (01M3XY2SR3VJZAKEPC6CBCS292).
pub fn used(text: &str) -> Vec<PathBuf> {
    let names_riff = |path: PathBuf| {
        std::fs::read_to_string(path).is_ok_and(|text| text.contains("mcp__plugin_riff_riff"))
    };
    projects(text)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| Repo::of(path).is_some_and(|r| r.top == *path && r.main.is_none()))
        .filter(|path| {
            names_riff(Repo::file(path, Place::Shared)) || names_riff(Repo::file(path, Place::Local))
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

    /// 01M3XY2T2YEV7GT7DKJHSMMHYR.
    #[test]
    fn a_linked_worktree_reads_the_local_settings_of_the_main_clone() {
        let dir = tempfile::tempdir().unwrap();
        let main = repo(dir.path());
        let tree = linked(&main);
        assert!(!State::of(&tree, None, false).on);
        let done = enable(&tree, Place::Local, None).unwrap();
        assert_eq!(done.file, main.join(".claude/settings.local.json"));
        assert!(done.state.on);
        assert!(State::of(&main, None, false).on);
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
        let dir = tempfile::tempdir().unwrap();
        let error = enable(dir.path(), Place::Local, None).unwrap_err();
        assert!(format!("{error:#}").contains("--global"), "{error:#}");
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
