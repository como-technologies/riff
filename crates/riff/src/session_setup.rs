//! The model, the env and the volumes of a session, from the riff config
//! of the repository (#1423, #1691).
//!
//! # Design
//!
//! riff sets in a session only what riff needs itself: the home, the
//! user, the sign-ins, the git config, the Claude folder, the clone, the
//! folder of the socket, and the caps `--memory` and `--cpus`
//! (01M5SMW28KT5D5HPRH7KAZ8TVC). It sets no variable and no volume of a
//! language or a tool. The repository gives them in its `riff.toml`
//! (01M5SMVVC4GQS310CVK3H9CMQ7), and the model of its sessions too
//! (01M6VAQWECJ6N6VJF66KCMNYD1):
//!
//! ```toml
//! [session]
//! model = "opus"
//!
//! [session.env]
//! RUSTC_WRAPPER = "sccache"
//! CARGO_HOME = "$HOME/.cargo"
//!
//! [session.volumes]
//! riff-cargo = "$HOME/.cargo"
//! ```
//!
//! The key `model` of `[session]` names the model of each session of the
//! repository: the lead, each work item and each verify start with
//! `claude --model MODEL` ([`Setup::claude`]). With no key, the model is
//! [`MODEL`]. A session that runs keeps its model, and the terminal of a
//! takeover resumes it on the same model ([`model_of`],
//! [`crate::takeover::command`]). riff takes `ANTHROPIC_MODEL` in
//! `[session.env]` as any other variable, and `claude --model` wins over
//! it (01M6VAQWMKDX4VY8QRPA8EYFN9).
//!
//! The key of `[session.volumes]` is the name of a podman volume, and
//! the value is its path in the container. A value of `[session.env]`
//! and a path may have `$HOME`: riff puts the home of the person there,
//! the same path on the host and in the container. A volume is bound
//! with write. A `riff.toml` with neither table gives a session
//! neither.
//!
//! ```mermaid
//! flowchart LR
//!     T["riff.toml on origin/HEAD:<br/>model, [session.env], [session.volumes]"] -- "session_image::merged_config" --> S["Setup"]
//!     S -- "run::spec: $HOME becomes the home" --> P["the env and the binds of the SessionSpec"]
//!     S -- "Setup::claude" --> M["claude --model MODEL"]
//!     R["what riff needs itself:<br/>HOME, USER, GH_TOKEN, the clone, ..."] --> P
//!     P --> C["podman run -e NAME=VALUE -v VOLUME:PATH"]
//!     M --> C
//! ```
//!
//! Only the merged `riff.toml` counts, as for the image
//! ([`crate::session_image::merged_config`]). The value of a variable
//! is in the spec and in the command line of podman: a secret does not
//! belong in `riff.toml`.
//!
//! riff refuses what it could not keep (01M5SMW2B4GMJ3V6BCX1KGC1ZY,
//! 01M6VAQWJ5RH7NCHAX0KCQ4BWY): a variable that riff sets itself, a
//! model that `claude` could read as a flag, a volume name that podman
//! could read as a path, and a path that is not absolute. The error
//! names the key and the file.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::runtime::{Bind, Env, Source};

/// The model of a session when `riff.toml` names none
/// (01M6VAQWECJ6N6VJF66KCMNYD1): the alias of `claude --model` for the
/// latest Opus.
pub const MODEL: &str = "opus";

/// The names of the variables that riff sets itself.
const RESERVED: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "GH_TOKEN",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

/// The starts of the names of the variables that riff sets itself.
const RESERVED_STARTS: &[&str] = &["GIT_CONFIG_", "RIFF_"];

/// The text that stands for the home of the person in a value and in a
/// path.
pub const HOME: &str = "$HOME";

/// The model, the env and the volumes that a riff config gives to a
/// session. The env and the volumes are in the order of the file.
///
/// ```
/// use riff::session_setup::Setup;
///
/// let config = r#"
/// [session]
/// image = "ghcr.io/o/i@sha256:00"
/// model = "sonnet"
///
/// [session.env]
/// RUSTC_WRAPPER = "sccache"
/// CARGO_HOME = "$HOME/.cargo"
///
/// [session.volumes]
/// riff-cargo = "$HOME/.cargo"
/// "#;
/// let setup = Setup::of_config(config, "riff.toml")?;
/// assert_eq!(setup.model, "sonnet");
/// assert_eq!(setup.env[0], ("RUSTC_WRAPPER".to_owned(), "sccache".to_owned()));
/// assert_eq!(setup.volumes, [("riff-cargo".to_owned(), "$HOME/.cargo".to_owned())]);
///
/// // No table, no env and no volume. No model: opus.
/// let none = Setup::of_config("[session]\nimage = \"x\"\n", "riff.toml")?;
/// assert_eq!(none, Setup::default());
/// assert_eq!(none.model, "opus");
///
/// // riff sets HOME itself.
/// let error = Setup::of_config("[session.env]\nHOME = \"/x\"\n", "riff.toml").unwrap_err();
/// assert!(format!("{error:#}").contains("riff sets HOME itself"));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    /// The model of `claude --model` (01M6VAQWECJ6N6VJF66KCMNYD1):
    /// `model` in `[session]`, else [`MODEL`].
    pub model: String,
    /// The name and the value of each variable. A value may have
    /// [`HOME`].
    pub env: Vec<(String, String)>,
    /// The name of each volume and its path in the container. A path
    /// may start with [`HOME`].
    pub volumes: Vec<(String, String)>,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            model: MODEL.to_owned(),
            env: Vec::new(),
            volumes: Vec::new(),
        }
    }
}

impl Setup {
    /// The model, the env and the volumes of the riff config `text`.
    /// `place` names the config in an error.
    pub fn of_config(text: &str, place: &str) -> Result<Setup> {
        let config: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("{place} is not valid TOML"))?;
        let Some(session) = config.get("session") else {
            return Ok(Setup::default());
        };
        let model = model(session, place)?;
        let env = pairs(session, "env", place)?;
        for (name, value) in &env {
            check_name(name, place)?;
            if value.contains('\0') {
                bail!("{place}: the value of [session.env] {name} has a NUL character");
            }
        }
        let volumes = pairs(session, "volumes", place)?;
        for (name, path) in &volumes {
            check_volume(name, path, place)?;
        }
        Ok(Setup {
            model,
            env,
            volumes,
        })
    }

    /// The start of the command of each session: `claude --model MODEL`
    /// (01M6VAQWECJ6N6VJF66KCMNYD1). The spec of a worker
    /// ([`crate::run::spec`]) and of the lead ([`crate::lead::spec`]) add
    /// their flags after it, and the daemon adds `--effort LEVEL` after
    /// `claude` ([`crate::room::Limit::effort`]).
    ///
    /// ```
    /// use riff::session_setup::Setup;
    ///
    /// assert_eq!(Setup::default().claude(), ["claude", "--model", "opus"]);
    /// let setup = Setup::of_config("[session]\nmodel = \"claude-opus-5-5\"\n", "riff.toml")?;
    /// assert_eq!(setup.claude(), ["claude", "--model", "claude-opus-5-5"]);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn claude(&self) -> Vec<String> {
        vec!["claude".into(), "--model".into(), self.model.clone()]
    }

    /// The binds of the volumes, with write, for the person whose home is
    /// `home`.
    ///
    /// ```
    /// use riff::runtime::Source;
    /// use riff::session_setup::Setup;
    ///
    /// let setup = Setup::of_config("[session.volumes]\nv = \"$HOME/.cache/v\"\n", "riff.toml")?;
    /// let binds = setup.binds("/home/mike".as_ref());
    /// assert_eq!(binds[0].source, Source::Volume("v".into()));
    /// assert_eq!(binds[0].target, std::path::Path::new("/home/mike/.cache/v"));
    /// assert!(binds[0].write);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn binds(&self, home: &Path) -> Vec<Bind> {
        self.volumes
            .iter()
            .map(|(name, path)| Bind {
                source: Source::Volume(name.clone()),
                target: expand(path, home).into(),
                write: true,
            })
            .collect()
    }

    /// The variables, for the person whose home is `home`.
    ///
    /// ```
    /// use riff::runtime::Env;
    /// use riff::session_setup::Setup;
    ///
    /// let setup = Setup::of_config("[session.env]\nCARGO_HOME = \"$HOME/.cargo\"\n", "riff.toml")?;
    /// assert_eq!(
    ///     setup.env("/home/mike".as_ref()),
    ///     [Env::Value("CARGO_HOME".into(), "/home/mike/.cargo".into())]
    /// );
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn env(&self, home: &Path) -> Vec<Env> {
        self.env
            .iter()
            .map(|(name, value)| Env::Value(name.clone(), expand(value, home)))
            .collect()
    }
}

/// The name and the string value of each key of the table `key` in the
/// table `[session]`, in the order of the file.
fn pairs(session: &toml_edit::Item, key: &str, place: &str) -> Result<Vec<(String, String)>> {
    let Some(item) = session.get(key) else {
        return Ok(Vec::new());
    };
    let table = item
        .as_table_like()
        .with_context(|| format!("{place}: [session] {key} is not a table"))?;
    table
        .iter()
        .map(|(name, value)| {
            let value = value
                .as_str()
                .with_context(|| format!("{place}: [session.{key}] {name} is not a string"))?;
            Ok((name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The model of the command of a session: the word after `--model`, or
/// none for a command with no `--model`. The daemon keeps it for the
/// terminal of a takeover (01M6VD1NSHTJQGR31VZ4689S8J).
///
/// ```
/// use riff::session_setup::{Setup, model_of};
///
/// let mut command = Setup::default().claude();
/// command.extend(["-p".to_owned(), "--verbose".to_owned()]);
/// assert_eq!(model_of(&command), Some("opus"));
/// assert_eq!(model_of(&["claude".to_owned(), "-p".to_owned()]), None);
/// ```
pub fn model_of(command: &[String]) -> Option<&str> {
    let at = command.iter().position(|arg| arg == "--model")?;
    command.get(at + 1).map(String::as_str)
}

/// The key `model` of the table `[session]`, or [`MODEL`] with no key
/// (01M6VAQWJ5RH7NCHAX0KCQ4BWY). A model is one argument of `claude`:
/// riff refuses an empty one, one with a space or a control character,
/// and one that starts with `-`, which `claude` reads as a flag.
fn model(session: &toml_edit::Item, place: &str) -> Result<String> {
    let Some(item) = session.get("model") else {
        return Ok(MODEL.to_owned());
    };
    let model = item
        .as_str()
        .with_context(|| format!("{place}: [session] model is not a string"))?;
    if model.is_empty()
        || model.starts_with('-')
        || model.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!(
            "{place}: [session] model {model:?} is not the name of a model: give one word that \
             does not start with -, for example \"{MODEL}\""
        );
    }
    Ok(model.to_owned())
}

/// Checks the name of a variable: letters, digits and `_`, not a digit
/// first, and not a name that riff sets itself.
fn check_name(name: &str, place: &str) -> Result<()> {
    let first = name.chars().next();
    if !first.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        bail!(
            "{place}: [session.env] {name} is not the name of a variable: use letters, digits and _"
        );
    }
    if RESERVED.contains(&name) || RESERVED_STARTS.iter().any(|start| name.starts_with(start)) {
        bail!("{place}: [session.env] {name}: riff sets {name} itself");
    }
    Ok(())
}

/// Checks a volume: a name that podman cannot read as a path, and an
/// absolute path that podman can take in an option.
fn check_volume(name: &str, path: &str, place: &str) -> Result<()> {
    let first = name.chars().next();
    if !first.is_some_and(|c| c.is_ascii_alphanumeric())
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!(
            "{place}: [session.volumes] {name} is not the name of a volume: use letters, digits, \
             _ . and -, and start with a letter or a digit"
        );
    }
    let absolute = path.starts_with('/')
        || path == HOME
        || path
            .strip_prefix(HOME)
            .is_some_and(|rest| rest.starts_with('/'));
    if !absolute {
        bail!(
            "{place}: [session.volumes] {name}: the path {path} is not absolute. Start it with / or {HOME}/"
        );
    }
    if path.contains([':', ',', '\0', '\n']) {
        bail!(
            "{place}: [session.volumes] {name}: the path {path} has a colon, a comma or a line break"
        );
    }
    Ok(())
}

/// `text` with each [`HOME`] in place of `home`. `$HOMEWORK` is not
/// [`HOME`]: the name ends at a character that is not a letter, a digit
/// or `_`.
fn expand(text: &str, home: &Path) -> String {
    let home = home.to_string_lossy();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(HOME) {
        let (before, after) = rest.split_at(at);
        let after = &after[HOME.len()..];
        out.push_str(before);
        if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            out.push_str(HOME);
        } else {
            out.push_str(&home);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(config: &str) -> Result<Setup> {
        Setup::of_config(config, "riff.toml")
    }

    fn refused(config: &str, why: &str) {
        let error = format!("{:#}", setup(config).unwrap_err());
        assert!(error.contains(why), "{config}: {error}");
        assert!(error.starts_with("riff.toml"), "{error}");
    }

    /// 01M5SMVVC4GQS310CVK3H9CMQ7: the order of the file stays, and both
    /// kinds of table work.
    #[test]
    fn the_order_of_the_file_stays_in_a_table_and_in_an_inline_table() {
        let one = setup("[session.env]\nB = \"1\"\nA = \"2\"\nC = \"3\"\n").unwrap();
        let names: Vec<_> = one.env.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["B", "A", "C"]);
        let two =
            setup("[session]\nenv = { B = \"1\", A = \"2\" }\nvolumes = { v = \"/v\" }\n").unwrap();
        assert_eq!(two.env.len(), 2);
        assert_eq!(two.volumes, [("v".to_owned(), "/v".to_owned())]);
    }

    /// 01M5SMVVC4GQS310CVK3H9CMQ7: `$HOME` is the home, also in the
    /// middle of a value, and `$HOMEWORK` is a name of its own.
    #[test]
    fn home_is_the_home_and_a_longer_name_is_not() {
        let home = Path::new("/home/mike");
        assert_eq!(expand("$HOME", home), "/home/mike");
        assert_eq!(expand("$HOME/a:$HOME/b", home), "/home/mike/a:/home/mike/b");
        assert_eq!(expand("$HOMEWORK/$HOME_X", home), "$HOMEWORK/$HOME_X");
        assert_eq!(expand("a$HOME", home), "a/home/mike");
        assert_eq!(expand("none", home), "none");
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a variable that riff sets itself.
    #[test]
    fn a_variable_that_riff_sets_is_refused() {
        for name in [
            "HOME",
            "USER",
            "LOGNAME",
            "GH_TOKEN",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "RIFF_SOCKET",
        ] {
            refused(
                &format!("[session.env]\n{name} = \"x\"\n"),
                &format!("riff sets {name} itself"),
            );
        }
        // A name that only looks like one is fine.
        assert!(setup("[session.env]\nHOMEBREW = \"x\"\nGIT_DIR = \"x\"\n").is_ok());
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a bad name, value or table.
    #[test]
    fn a_bad_variable_or_table_is_refused() {
        refused(
            "[session.env]\n\"1A\" = \"x\"\n",
            "not the name of a variable",
        );
        refused(
            "[session.env]\n\"A=B\" = \"x\"\n",
            "not the name of a variable",
        );
        refused(
            "[session.env]\n\"\" = \"x\"\n",
            "not the name of a variable",
        );
        refused("[session.env]\nA = 1\n", "[session.env] A is not a string");
        refused("[session]\nenv = \"x\"\n", "[session] env is not a table");
        refused(
            "[session]\nvolumes = [\"x\"]\n",
            "[session] volumes is not a table",
        );
        refused(
            "[session.volumes]\nv = true\n",
            "[session.volumes] v is not a string",
        );
        refused("[session.env]\nA = \"a\\u0000b\"\n", "NUL");
        refused("[session.env\n", "is not valid TOML");
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a volume name that podman could read
    /// as a path binds a folder of the host, so riff refuses it.
    #[test]
    fn a_volume_name_that_podman_reads_as_a_path_is_refused() {
        for name in ["/home/mike/.ssh", ".", "..", "./v", "-v", "a/b", "a:b", ""] {
            refused(
                &format!("[session.volumes]\n\"{name}\" = \"/v\"\n"),
                "not the name of a volume",
            );
        }
        assert!(setup("[session.volumes]\nriff-cargo = \"/v\"\n\"a_b.c\" = \"/w\"\n").is_ok());
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a path that is not absolute, or that
    /// podman cannot take in an option.
    #[test]
    fn a_path_that_is_not_absolute_or_that_podman_cannot_take_is_refused() {
        refused("[session.volumes]\nv = \"cache\"\n", "is not absolute");
        refused("[session.volumes]\nv = \"~/cache\"\n", "is not absolute");
        refused(
            "[session.volumes]\nv = \"$HOMEX/cache\"\n",
            "is not absolute",
        );
        refused("[session.volumes]\nv = \"/a:ro\"\n", "has a colon");
        refused("[session.volumes]\nv = \"/a,b\"\n", "has a colon");
        assert!(setup("[session.volumes]\nv = \"$HOME\"\nw = \"$HOME/c\"\nx = \"/c\"\n").is_ok());
    }

    #[test]
    fn a_config_with_no_session_table_gives_nothing() {
        assert_eq!(setup("").unwrap(), Setup::default());
        assert_eq!(setup("[release]\nx = 1\n").unwrap(), Setup::default());
    }

    /// 01M6VAQWECJ6N6VJF66KCMNYD1: `model` in `[session]` is the model,
    /// and with no key the model is opus.
    #[test]
    fn the_model_comes_from_the_session_table_and_the_default_is_opus() {
        for config in [
            "",
            "[session]\nimage = \"x\"\n",
            "[session.env]\nA = \"1\"\n",
        ] {
            assert_eq!(setup(config).unwrap().model, "opus", "{config:?}");
        }
        for model in ["sonnet", "claude-opus-5-5", "sonnet[1m]", "opus"] {
            let config = format!("[session]\nimage = \"x\"\nmodel = \"{model}\"\n");
            assert_eq!(setup(&config).unwrap().model, model);
        }
        assert_eq!(
            setup("[session]\nmodel = \"haiku\"\n").unwrap().claude(),
            ["claude", "--model", "haiku"]
        );
    }

    /// 01M6VAQWMKDX4VY8QRPA8EYFN9: riff takes `ANTHROPIC_MODEL` in
    /// `[session.env]` beside `model` in `[session]`, and the command
    /// has the model of the key.
    #[test]
    fn anthropic_model_in_the_env_is_taken_beside_the_model() {
        let setup =
            setup("[session]\nmodel = \"sonnet\"\n\n[session.env]\nANTHROPIC_MODEL = \"opus\"\n")
                .unwrap();
        assert_eq!(setup.claude(), ["claude", "--model", "sonnet"]);
        assert_eq!(
            setup.env,
            [("ANTHROPIC_MODEL".to_owned(), "opus".to_owned())]
        );
    }

    /// 01M6VD1NSHTJQGR31VZ4689S8J: the model of a command is the word
    /// after `--model`, also after the `--effort` of the daemon.
    #[test]
    fn the_model_of_a_command_is_the_word_after_model() {
        let command = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            model_of(&command(&[
                "claude", "--effort", "xhigh", "--model", "sonnet", "-p"
            ])),
            Some("sonnet")
        );
        assert_eq!(model_of(&command(&["claude", "-p"])), None);
        assert_eq!(model_of(&command(&["claude", "--model"])), None);
    }

    /// 01M6VAQWJ5RH7NCHAX0KCQ4BWY: a model that `claude` could read as a
    /// flag or as two arguments.
    #[test]
    fn a_model_that_is_not_one_word_is_refused() {
        refused("[session]\nmodel = 4\n", "[session] model is not a string");
        for model in ["", "-p", "--model", "opus sonnet", "opus\\n", "a\\tb"] {
            refused(
                &format!("[session]\nmodel = \"{model}\"\n"),
                "is not the name of a model",
            );
        }
    }
}
