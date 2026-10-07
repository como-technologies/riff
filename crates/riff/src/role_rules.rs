//! The Claude Code permission rules of each role, from its profile.
//!
//! # Design
//!
//! The permission rules of a role come from its [`Profile`], so the
//! profile and the rules say the same thing (01M4BT33R71HXAVQGHFD4ZFGR5).
//! riff passes them in the flag settings (`--settings`) of `claude` at
//! each start of the role: `riff workers run` for a worker and a
//! verifier ([`flag`]). The flag settings add their rules to the rules
//! of the person and of the project.
//!
//! - **Allow**: `Read` for each path that the role reads, and `Read` and
//!   `Edit` for each path that it writes.
//! - **Deny the home**: Claude Code tries the deny rules first, and an
//!   allow rule cannot open a path in a denied folder. So no rule can
//!   deny the home as a whole: the paths of the profile are in it.
//!   [`of`] walks the home on the disk. It denies each file and each
//!   folder that holds no path of the profile, and goes into each
//!   folder that holds one (01M4BT33TPSXJVB6JZDZ3F1GGX).
//! - **Deny the secrets**: each place of [`Session::secrets`]: the
//!   keyring, the D-Bus socket, the keys of SSH and GnuPG, the sign-in
//!   of `gh`.
//! - **Deny the settings**: no role edits a `settings.json` or a
//!   `settings.local.json` of Claude Code. They hold the rules, so a
//!   session that edits them can widen its own rules
//!   (01M4BT33X0WVVJH7Y6AXSWZEYC).
//! - A role that riff cannot give a profile, for example with a home
//!   that is a tool path, starts with no rules of a profile, and says
//!   why in one line (01M4BT33Z914GBHCGCAXFVQ2X7).
//!
//! The rules cover the file tools of Claude Code and the file commands
//! that it knows in Bash. The sandbox of #607 holds each other process.
//!
//! ```mermaid
//! flowchart LR
//!     S["Session"] --> P["Profile::of(role, session)"]
//!     P --> A["allow: Read and Edit of each path"]
//!     H["the home on the disk"] --> D["deny: each path of the home<br/>outside the profile"]
//!     P --> D
//!     P --> X["deny: secrets, settings files"]
//!     A & D & X --> F["--settings of claude"]
//! ```
//!
//! ```
//! use riff::profile::{Endpoint, Profile, Role, Session};
//! use riff::role_rules::{Entry, of};
//! use std::path::Path;
//!
//! let session = Session {
//!     home: "/home/ada".into(),
//!     runtime: "/run/user/1000".into(),
//!     clone: "/home/ada/app".into(),
//!     worktree: "/home/ada/app/.claude/worktrees".into(),
//!     target: "/home/ada/app/.claude/worktrees".into(),
//!     temp: "/var/tmp/s1".into(),
//!     claude: "/home/ada/.claude".into(),
//!     state: "/run/user/1000/riff".into(),
//!     tools: vec![],
//!     server: Endpoint::of_url("https://riff.example.com").unwrap(),
//! };
//! let disk = |dir: &Path| match dir.to_str() {
//!     Some("/home/ada") => vec![Entry::file("/home/ada/.bashrc"), Entry::dir("/home/ada/app")],
//!     Some("/home/ada/app") => vec![Entry::file("/home/ada/app/README.md"), Entry::dir("/home/ada/app/.claude")],
//!     _ => vec![],
//! };
//! let profile = Profile::of(Role::Worker, &session)?;
//! let rules = of(&profile, &session, &disk);
//! assert!(rules.allow.contains(&"Edit(//home/ada/app/.claude/worktrees/**)".to_owned()));
//! assert!(rules.deny.contains(&"Read(//home/ada/.bashrc)".to_owned()));
//! assert!(rules.deny.contains(&"Edit(//home/ada/app/README.md)".to_owned()));
//! assert!(rules.deny.contains(&"Read(//run/user/1000/bus)".to_owned()));
//! assert!(rules.deny.contains(&"Edit(//home/ada/.claude/settings.json)".to_owned()));
//! # Ok::<(), riff::profile::Refused>(())
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::permissions::Rules;
use crate::profile::{Endpoint, Profile, Role, Session};

/// A file or a folder that a walk of the home finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path.
    pub path: PathBuf,
    /// True for a folder. A link counts as a file: the walk does not
    /// follow it, and Claude Code applies a deny rule to the target of a
    /// link too.
    pub dir: bool,
}

impl Entry {
    /// A file at `path`.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            dir: false,
        }
    }

    /// A folder at `path`.
    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            dir: true,
        }
    }
}

/// The files and folders in `dir` on the disk. An error gives none.
pub fn list(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<Entry> = read
        .flatten()
        .map(|e| Entry {
            path: e.path(),
            dir: e.file_type().is_ok_and(|t| t.is_dir()),
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The names of the settings files of Claude Code that hold rules.
pub const SETTINGS: [&str; 2] = ["settings.json", "settings.local.json"];

/// The permission rules of `profile`, for its `session`. `list` gives
/// the entries of a folder of the home (see [`list`]).
pub fn of(profile: &Profile, session: &Session, list: &dyn Fn(&Path) -> Vec<Entry>) -> Rules {
    let mut rules = Set::default();
    let writes: Vec<&Path> = profile.write_paths().collect();
    for path in &writes {
        rules.allow(format!("Read({}/**)", pattern(path)));
        rules.allow(format!("Edit({}/**)", pattern(path)));
    }
    for path in profile.read_paths() {
        rules.allow(format!("Read({}/**)", pattern(path)));
    }
    let granted: Vec<&Path> = writes
        .iter()
        .copied()
        .chain(profile.read_paths())
        .filter(|p| p.starts_with(&session.home))
        .collect();
    walk(&Entry::dir(&session.home), &granted, list, &mut rules);
    for secret in session.secrets() {
        rules.deny_all(&secret, true);
    }
    for name in SETTINGS {
        rules.deny(format!("Edit({})", pattern(&session.claude.join(name))));
        rules.deny(format!("Edit(//**/.claude/{name})"));
    }
    rules.done()
}

/// Denies `entry` when it holds no granted path, and else goes into it.
fn walk(entry: &Entry, granted: &[&Path], list: &dyn Fn(&Path) -> Vec<Entry>, rules: &mut Set) {
    let path = entry.path.as_path();
    if granted.iter().any(|g| path.starts_with(g)) {
        return;
    }
    if !granted.iter().any(|g| g.starts_with(path)) {
        rules.deny_all(path, entry.dir);
        return;
    }
    for e in list(path) {
        walk(&e, granted, list, rules);
    }
}

/// The rules in their order, each once.
#[derive(Default)]
struct Set {
    rules: Rules,
    seen: HashSet<String>,
}

impl Set {
    fn allow(&mut self, rule: String) {
        if self.seen.insert(rule.clone()) {
            self.rules.allow.push(rule);
        }
    }

    fn deny(&mut self, rule: String) {
        if self.seen.insert(rule.clone()) {
            self.rules.deny.push(rule);
        }
    }

    /// Denies the read and the edit of `path`, and of each path in it
    /// when it is (or can be) a folder.
    fn deny_all(&mut self, path: &Path, dir: bool) {
        let p = pattern(path);
        for tool in ["Read", "Edit"] {
            self.deny(format!("{tool}({p})"));
            if dir {
                self.deny(format!("{tool}({p}/**)"));
            }
        }
    }

    fn done(self) -> Rules {
        self.rules
    }
}

/// The pattern of an absolute path in a rule of Claude Code: `//`, then
/// the path, with each character of a gitignore pattern escaped.
///
/// ```
/// use riff::role_rules::pattern;
/// assert_eq!(pattern("/home/ada/src".as_ref()), "//home/ada/src");
/// assert_eq!(pattern("/home/ada/a*b[1]?".as_ref()), r"//home/ada/a\*b\[1\]\?");
/// ```
pub fn pattern(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::from("/");
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The session of a worker in the main clone `clone`, with the temp
/// folder `temp`, from the variables of this process. A worker starts
/// with no item, so its worktree is the folder of the worktrees of the
/// clone, and its target is in it (01M4BT341H1M1N1MT947HXNXDR). The
/// tools are the toolchain of Rust, the binaries of riff and of
/// `claude`, the plugin of riff and the settings of riff. `None` with no
/// `HOME`, or with a server URL that has no host.
pub fn worker_session(clone: &Path, temp: &Path, claude: &Path, server: &str) -> Option<Session> {
    let home = PathBuf::from(std::env::var_os("HOME").filter(|h| !h.is_empty())?);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|r| !r.is_empty())
        .map_or_else(
            || PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())),
            PathBuf::from,
        );
    let worktrees = clone.join(".claude/worktrees");
    let mut tools = vec![
        home.join(".cargo"),
        home.join(".rustup"),
        home.join(".local/share/riff"),
    ];
    let bins = [crate::binary::this_on_disk().ok(), on_path(claude)];
    for bin in bins.into_iter().flatten() {
        for path in [Some(bin.clone()), bin.canonicalize().ok()]
            .into_iter()
            .flatten()
        {
            tools.extend(path.parent().map(Path::to_path_buf));
        }
    }
    if let Some(dir) = crate::settings::path()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        tools.push(dir);
    }
    tools.retain(|t| !home.starts_with(t));
    let mut seen = HashSet::new();
    tools.retain(|t| seen.insert(t.clone()));
    Some(Session {
        claude: crate::worker_lsp::claude_dir().unwrap_or_else(|| home.join(".claude")),
        state: crate::local::dir().unwrap_or_else(|| runtime.join("riff")),
        home,
        runtime,
        clone: clone.to_path_buf(),
        worktree: worktrees.clone(),
        target: worktrees,
        temp: temp.to_path_buf(),
        tools,
        server: Endpoint::of_url(server)?,
    })
}

/// The path of `bin`: itself when it has a folder, else the first match
/// in `PATH`.
fn on_path(bin: &Path) -> Option<PathBuf> {
    if bin.components().count() > 1 {
        return Some(bin.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(bin))
        .find(|p| p.is_file())
}

/// The rules of `role` for `session`, from the disk, or the line that
/// says why there are none.
pub fn here(role: Role, session: &Session) -> Result<Rules, String> {
    let profile =
        Profile::of(role, session).map_err(|e| crate::text::no_role_rules(role, &e.to_string()))?;
    Ok(of(&profile, session, &list))
}

/// `args` of `claude` with `rules` in the permissions of its flag
/// settings (see [`crate::terminal::with_settings`]).
///
/// ```
/// use riff::permissions::Rules;
/// use riff::role_rules::flag;
///
/// let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
/// let rules = Rules { allow: vec!["Read(//w/**)".into()], deny: vec!["Read(//h/.bashrc)".into()] };
/// assert_eq!(
///     flag(&args(&["--settings", r#"{"permissions":{"deny":["X"]}}"#, "Join the riff."]), &rules),
///     args(&["--settings", r#"{"permissions":{"deny":["X","Read(//h/.bashrc)"],"allow":["Read(//w/**)"]}}"#, "Join the riff."]),
/// );
/// ```
pub fn flag(args: &[String], rules: &Rules) -> Vec<String> {
    crate::terminal::with_settings(args, |settings| {
        let permissions = settings
            .entry("permissions")
            .or_insert_with(|| serde_json::json!({}));
        let Some(permissions) = permissions.as_object_mut() else {
            return;
        };
        for (list, want) in [("deny", &rules.deny), ("allow", &rules.allow)] {
            if want.is_empty() {
                continue;
            }
            let have = permissions
                .entry(list)
                .or_insert_with(|| serde_json::json!([]));
            if let Some(have) = have.as_array_mut() {
                for rule in want {
                    if !have.iter().any(|r| r == rule.as_str()) {
                        have.push(rule.as_str().into());
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            home: "/home/ada".into(),
            runtime: "/run/user/1000".into(),
            clone: "/home/ada/src/app".into(),
            worktree: "/home/ada/src/app/.claude/worktrees/issue-12".into(),
            target: "/home/ada/src/app/.claude/worktrees/issue-12/target".into(),
            temp: "/home/ada/.cache/riff/tmp/s1".into(),
            claude: "/home/ada/.claude".into(),
            state: "/run/user/1000/riff".into(),
            tools: vec!["/home/ada/.cargo".into(), "/home/ada/.rustup".into()],
            server: Endpoint::of_url("https://riff.example.com").unwrap(),
        }
    }

    /// A home with a clone, a second clone, dot files, a cache, the
    /// toolchain and the secrets.
    fn disk(dir: &Path) -> Vec<Entry> {
        let d = |p: &str| Entry::dir(format!("{}/{p}", dir.display()));
        let f = |p: &str| Entry::file(format!("{}/{p}", dir.display()));
        match dir.to_str().unwrap() {
            "/home/ada" => vec![
                f(".bashrc"),
                f(".gitconfig"),
                d(".cache"),
                d(".cargo"),
                d(".claude"),
                d(".config"),
                d(".local"),
                d(".rustup"),
                d(".ssh"),
                d("Documents"),
                d("src"),
            ],
            "/home/ada/.cache" => vec![d("riff"), d("mozilla")],
            "/home/ada/.cache/riff" => vec![d("tmp")],
            "/home/ada/.cache/riff/tmp" => vec![d("s1"), d("s2")],
            "/home/ada/.local" => vec![d("share")],
            "/home/ada/.local/share" => vec![d("keyrings")],
            "/home/ada/src" => vec![d("app"), d("other")],
            "/home/ada/src/app" => vec![d(".git"), d(".claude"), f("README.md")],
            "/home/ada/src/app/.claude" => vec![d("worktrees"), f("settings.json")],
            "/home/ada/src/app/.claude/worktrees" => vec![d("issue-12"), d("issue-7")],
            _ => vec![],
        }
    }

    fn rules(role: Role) -> Rules {
        of(&Profile::of(role, &session()).unwrap(), &session(), &disk)
    }

    /// The path that a rule of `tool` names, with `/**` for a folder.
    fn target<'a>(rule: &'a str, tool: &str) -> Option<(&'a str, bool)> {
        let inner = rule
            .strip_prefix(tool)?
            .strip_prefix("(/")?
            .strip_suffix(')')?;
        Some(match inner.strip_suffix("/**") {
            Some(dir) => (dir, true),
            None => (inner, false),
        })
    }

    /// True when a rule of `tool` in `list` covers `path`.
    fn covers(list: &[String], tool: &str, path: &Path) -> bool {
        list.iter().filter_map(|r| target(r, tool)).any(|(p, dir)| {
            if dir {
                path.starts_with(p) && path != Path::new(p)
            } else {
                path.starts_with(p)
            }
        })
    }

    #[test]
    fn each_path_of_the_profile_has_an_allow_rule() {
        for role in Role::ALL {
            let profile = Profile::of(role, &session()).unwrap();
            let rules = rules(role);
            for path in profile.write_paths() {
                let inside = path.join("x");
                assert!(covers(&rules.allow, "Read", &inside), "{role}: {path:?}");
                assert!(covers(&rules.allow, "Edit", &inside), "{role}: {path:?}");
            }
            for path in profile.read_paths() {
                assert!(
                    covers(&rules.allow, "Read", &path.join("x")),
                    "{role}: {path:?}"
                );
                assert!(
                    !covers(&rules.allow, "Edit", &path.join("x")),
                    "{role}: {path:?}"
                );
            }
        }
    }

    #[test]
    fn the_home_and_the_secrets_have_a_deny_rule() {
        let s = session();
        let places = [
            s.home.join(".bashrc"),
            s.home.join(".gitconfig"),
            s.home.join("Documents/tax.pdf"),
            s.home.join(".config/gh/hosts.yml"),
            s.home.join(".ssh/id_ed25519"),
            s.home.join(".local/share/keyrings/login.keyring"),
            s.home.join(".cache/mozilla/x"),
            s.home.join(".cache/riff/tmp/s2/x"),
            s.home.join("src/other/main.rs"),
            s.runtime.join("bus"),
            s.runtime.join("keyring/control"),
            s.runtime.join("gnupg/S.gpg-agent"),
        ];
        for role in Role::ALL {
            let rules = rules(role);
            for place in &places {
                assert!(covers(&rules.deny, "Read", place), "{role}: {place:?}");
                assert!(covers(&rules.deny, "Edit", place), "{role}: {place:?}");
            }
        }
        // A worker does not read the files of the main clone.
        let worker = rules(Role::Worker);
        let readme = s.clone.join("README.md");
        assert!(covers(&worker.deny, "Read", &readme));
        assert!(!covers(&rules(Role::Lead).deny, "Read", &readme));
    }

    #[test]
    fn no_deny_rule_closes_a_path_of_the_profile() {
        for role in Role::ALL {
            let profile = Profile::of(role, &session()).unwrap();
            let rules = rules(role);
            for path in profile.write_paths().chain(profile.read_paths()) {
                let inside = path.join("src/lib.rs");
                assert!(!covers(&rules.deny, "Read", &inside), "{role}: {path:?}");
            }
            for path in profile.write_paths() {
                let inside = path.join("src/lib.rs");
                assert!(!covers(&rules.deny, "Edit", &inside), "{role}: {path:?}");
            }
        }
    }

    #[test]
    fn no_role_edits_a_settings_file_of_claude_code() {
        for role in Role::ALL {
            let rules = rules(role);
            for name in SETTINGS {
                let mine = format!("Edit(//home/ada/.claude/{name})");
                assert!(rules.deny.contains(&mine), "{role}: {mine}");
                let any = format!("Edit(//**/.claude/{name})");
                assert!(rules.deny.contains(&any), "{role}: {any}");
            }
        }
    }

    #[test]
    fn a_home_with_no_path_of_the_profile_is_denied_as_a_whole() {
        let s = Session {
            clone: "/srv/app".into(),
            worktree: "/srv/app/.claude/worktrees".into(),
            target: "/srv/app/.claude/worktrees".into(),
            temp: "/var/tmp/s1".into(),
            claude: "/srv/claude".into(),
            tools: vec!["/opt/rust".into()],
            ..session()
        };
        let r = of(&Profile::of(Role::Worker, &s).unwrap(), &s, &disk);
        assert!(r.deny.contains(&"Read(//home/ada/**)".to_owned()));
        assert!(r.deny.contains(&"Edit(//home/ada/**)".to_owned()));
    }

    #[test]
    fn each_rule_comes_once() {
        let s = Session {
            target: "/home/ada/src/app/.claude/worktrees/issue-12".into(),
            ..session()
        };
        let r = of(&Profile::of(Role::Worker, &s).unwrap(), &s, &disk);
        let all: Vec<&String> = r.allow.iter().chain(&r.deny).collect();
        let set: HashSet<&String> = all.iter().copied().collect();
        assert_eq!(all.len(), set.len());
    }
}
