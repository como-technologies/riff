//! The [`Runtime`] of riff on Linux: rootless podman (#715,
//! 01M4Z60B7QDWEFQ96H448RE9N2). riff builds it only for
//! `target_os = "linux"` (01M4Z60B7GTV4HHF9R3TMZRS7J).
//!
//! # Design
//!
//! Each step of the trait is one `podman` command. [`run_args`] makes the
//! arguments of `podman run` from a [`SessionSpec`]. riff builds no shell
//! string: each value is one argument (01M4Z60B8KDXD190RWC7D184Q3).
//!
//! | Part of the spec | The argument of `podman run` |
//! |---|---|
//! | (the daemon in a unit) | the start is `systemd-run --user --scope --collect --quiet -- podman run ...`: pasta runs in a scope of its own ([`start_command`], 01M5RG2AS2CYJPWGKTV7N2HNYP) |
//! | (each spec) | `--sig-proxy=false` |
//! | `name` | `--name NAME` |
//! | The user | `--userns=keep-id` |
//! | (each spec) | `--init`: the init of podman is PID 1, and reaps each orphan process (01M51117K3WXK35CE67KPRFMA8) |
//! | `memory`, `cpus` | `--memory 8g`, `--cpus 4` |
//! | `home` | `--mount type=tmpfs,destination=HOME,tmpfs-mode=0700,U=true` |
//! | A [`Bind`] | `-v SOURCE:TARGET`, with `:ro` when it has no write |
//! | An [`Env::Value`] | `-e NAME=VALUE` |
//! | An [`Env::Host`] | `-e NAME`: podman takes the value from its own environment |
//! | `workdir` | `-w WORKDIR` |
//! | `image`, `command` | the last arguments |
//!
//! `podman run --rm -i` keeps stdin open: it is [`Running::input`]. The
//! stdout of the command is [`Running::events`]. `podman stop` ends the
//! container and each of its processes (01M4Z60B7XT7462XMV49GJTVSJ).
//!
//! A session with a terminal, the lead, starts with `podman run --rm -d
//! -i -t` ([`terminal_args`]): conmon holds its terminal, and `podman
//! run` ends when the container runs. `podman wait NAME` gives its end.
//! `podman attach NAME` shows the terminal to a person, until Ctrl-P
//! Ctrl-Q. Before it, `podman exec NAME stty` sets the terminal of the
//! container to another size ([`redraw_args`]). The attach sets the size
//! of the person again, so `claude` draws its screen
//! (01M51PD7M04HZR2PVVJA4WCBK5). A takeover runs `podman exec -it NAME
//! COMMAND` with the terminal of the person (01M4Z60B81RTZT71PN5HV0GZHX,
//! 01M4Z60B83ZH10HYKW7X1C0SSS).
//!
//! conmon, the monitor of podman, holds the container. A stop of the
//! daemon stops no container, and leaves its network
//! (01M4Z60B7Z24WBD6HV6ER0WN9K, 01M4ZDTW00B7N6T9P3MK066KA7,
//! 01M5RG2AS2CYJPWGKTV7N2HNYP):
//!
//! - riff starts each `podman` with no `INVOCATION_ID`. systemd sets it
//!   in each process of a unit, and with it podman keeps conmon in the
//!   cgroup of the unit of the daemon. With no `INVOCATION_ID`, podman
//!   puts conmon in a scope of its own, so a stop of the unit does not
//!   end it.
//! - pasta, the network process of podman, stays in the cgroup of the
//!   process that ran `podman run`. So when the daemon runs in a unit
//!   (it has an `INVOCATION_ID`), [`start_command`] runs each `podman
//!   run` in a scope of its own, with `systemd-run --user --scope`. A
//!   stop of the unit does not end pasta, and the container keeps its
//!   network. The scope ends with the container.
//! - `--sig-proxy=false`: the `podman run` of riff passes no signal to
//!   the container. In a scope, a stop of the unit sends no SIGTERM to
//!   that `podman run`: it ends when the daemon closes its pipes, or
//!   with the container. riff stops a container only with
//!   [`Runtime::stop`].
//!
//! ```mermaid
//! flowchart TD
//!     U["riff.service"] --> D["riff daemon"]
//!     D -- "systemd-run --user --scope" --> S["scope run-pNNN.scope"]
//!     S --> P["podman run"]
//!     S --> N["pasta: the network"]
//!     P -- "conmon, in a scope of its own" --> C["the container"]
//! ```

use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::runtime::{Bind, End, Env, Exit, Running, Runtime, SessionSpec, Source};
use crate::text;

/// The program of the container runtime.
pub const PODMAN: &str = "podman";

/// Rootless podman, from `PATH`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Podman;

/// The arguments of `podman` for `spec`: `run`, with stdin open. It
/// refuses a path that podman cannot take in an option: a path with a
/// comma or a colon.
///
/// ```
/// use riff::podman::run_args;
/// use riff::runtime::{Bind, Env, SessionSpec, Source};
///
/// let spec = SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "localhost/riff-session:latest".into(),
///     home: "/home/mike".into(),
///     workdir: "/data/clone".into(),
///     binds: vec![
///         Bind { source: Source::Path("/data/clone".into()), target: "/data/clone".into(), write: true },
///         Bind { source: Source::Volume("riff-cargo".into()), target: "/home/mike/.cargo".into(), write: true },
///         Bind { source: Source::Path("/home/mike/.gitconfig".into()), target: "/home/mike/.gitconfig".into(), write: false },
///     ],
///     env: vec![Env::Value("USER".into(), "mike".into()), Env::Host("GH_TOKEN".into())],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec!["claude".into(), "-p".into()],
/// };
/// let args: Vec<String> = run_args(&spec)?
///     .into_iter()
///     .map(|a| a.into_string().unwrap())
///     .collect();
/// assert_eq!(
///     args.join(" "),
///     "run --rm -i --sig-proxy=false --name riff-issue-12-1a2b3c4d --userns=keep-id \
///      --init --memory 8g --cpus 4 \
///      --mount type=tmpfs,destination=/home/mike,tmpfs-mode=0700,U=true \
///      -v /data/clone:/data/clone -v riff-cargo:/home/mike/.cargo \
///      -v /home/mike/.gitconfig:/home/mike/.gitconfig:ro \
///      -e USER=mike -e GH_TOKEN -w /data/clone localhost/riff-session:latest claude -p"
/// );
///
/// // A path with a comma or a colon breaks the options of podman.
/// let bad = SessionSpec { workdir: "/data/a:b".into(), ..spec };
/// assert!(run_args(&bad).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run_args(spec: &SessionSpec) -> Result<Vec<OsString>> {
    args_of(spec, &["run", "--rm", "-i", "--sig-proxy=false"])
}

/// The arguments of `podman` for `spec` with a terminal: `run`, with no
/// wait for the end. conmon holds the terminal. The rest is as
/// [`run_args`].
///
/// ```
/// use riff::podman::terminal_args;
/// use riff::runtime::SessionSpec;
///
/// let spec = SessionSpec {
///     name: "riff-lead-o-r".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/data/clone".into(),
///     binds: vec![],
///     env: vec![],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec!["claude".into()],
/// };
/// let args: Vec<String> = terminal_args(&spec)?
///     .into_iter()
///     .map(|a| a.into_string().unwrap())
///     .collect();
/// assert_eq!(
///     args.join(" "),
///     "run --rm -d -i -t --name riff-lead-o-r --userns=keep-id --init --memory 8g --cpus 4 \
///      --mount type=tmpfs,destination=/home/mike,tmpfs-mode=0700,U=true \
///      -w /data/clone img claude"
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn terminal_args(spec: &SessionSpec) -> Result<Vec<OsString>> {
    args_of(spec, &["run", "--rm", "-d", "-i", "-t"])
}

/// The arguments of `podman` that set the terminal of the container
/// `name` to a size that is not `rows` and `cols`, the size of the
/// terminal of the person: one row less, or one more for a terminal of
/// one row. The next attach sets the size of the person, so the command
/// draws again (01M51PD7M04HZR2PVVJA4WCBK5). `/dev/console` is the
/// terminal that conmon holds.
///
/// ```
/// use riff::podman::redraw_args;
///
/// assert_eq!(
///     redraw_args("riff-lead-o-r", 30, 100).join(" "),
///     "exec riff-lead-o-r stty -F /dev/console rows 29 cols 100"
/// );
/// assert_eq!(redraw_args("n", 1, 80)[6], "2");
/// ```
pub fn redraw_args(name: &str, rows: u16, cols: u16) -> Vec<String> {
    let rows = if rows > 1 { rows - 1 } else { rows + 1 };
    [
        "exec",
        name,
        "stty",
        "-F",
        "/dev/console",
        "rows",
        &rows.to_string(),
        "cols",
        &cols.to_string(),
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The arguments of `podman` for `spec`, after the arguments `first`.
fn args_of(spec: &SessionSpec, first: &[&str]) -> Result<Vec<OsString>> {
    let mut args: Vec<OsString> = first.iter().map(OsString::from).collect();
    args.push("--name".into());
    args.push(spec.name.clone().into());
    args.push("--userns=keep-id".into());
    args.push("--init".into());
    args.push("--memory".into());
    args.push(spec.memory.clone().into());
    args.push("--cpus".into());
    args.push(spec.cpus.to_string().into());
    args.push("--mount".into());
    args.push(
        format!(
            "type=tmpfs,destination={},tmpfs-mode=0700,U=true",
            plain(&spec.home)?
        )
        .into(),
    );
    for bind in &spec.binds {
        args.push("-v".into());
        args.push(volume(bind)?.into());
    }
    for env in &spec.env {
        args.push("-e".into());
        args.push(match env {
            Env::Value(name, value) => format!("{name}={value}").into(),
            Env::Host(name) => name.into(),
        });
    }
    args.push("-w".into());
    args.push(plain(&spec.workdir)?.into());
    args.push(spec.image.clone().into());
    args.extend(spec.command.iter().map(OsString::from));
    Ok(args)
}

/// The value of `-v` for `bind`.
fn volume(bind: &Bind) -> Result<String> {
    let source = match &bind.source {
        Source::Path(path) => plain(path)?,
        Source::Volume(name) => name.as_str(),
    };
    let mode = if bind.write { "" } else { ":ro" };
    Ok(format!("{source}:{}{mode}", plain(&bind.target)?))
}

/// `path` as text that podman can take in an option.
fn plain(path: &Path) -> Result<&str> {
    let text = path
        .to_str()
        .ok_or_else(|| anyhow!("the path {} is not UTF-8", path.display()))?;
    if text.contains([',', ':']) {
        bail!("{}", text::podman_bad_path(text));
    }
    Ok(text)
}

/// The variable that systemd sets in each process of a unit. podman
/// keeps conmon in the cgroup of the unit when it is set, so riff starts
/// each `podman` with no such variable (01M4ZDTW00B7N6T9P3MK066KA7).
pub const INVOCATION_ID: &str = "INVOCATION_ID";

/// A `podman` command with no [`INVOCATION_ID`].
fn podman() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(PODMAN);
    command.env_remove(INVOCATION_ID);
    command
}

/// The program that puts a command in a systemd scope.
pub const SYSTEMD_RUN: &str = "systemd-run";

/// The arguments of [`SYSTEMD_RUN`] before the command: a scope in the
/// manager of the user that systemd removes when it ends, and says
/// nothing.
pub const SCOPE: [&str; 5] = ["--user", "--scope", "--collect", "--quiet", "--"];

/// True when this process runs in a systemd unit: systemd sets
/// [`INVOCATION_ID`] in each process of a unit.
fn in_unit() -> bool {
    std::env::var_os(INVOCATION_ID).is_some()
}

/// The program that [`start_command`] runs.
fn starter(in_unit: bool) -> &'static str {
    if in_unit { SYSTEMD_RUN } else { PODMAN }
}

/// The command that starts `podman` with `args`, the arguments of `run`,
/// and with no [`INVOCATION_ID`]. In a unit (`in_unit`), it starts in a
/// systemd scope of its own: pasta, the network process of podman, is
/// then not in the cgroup of the unit, and a stop of the unit leaves the
/// network of the container (01M5RG2AS2CYJPWGKTV7N2HNYP).
///
/// ```
/// use riff::podman::{run_args, start_command, terminal_args};
/// use riff::runtime::SessionSpec;
///
/// let spec = SessionSpec {
///     name: "riff-lead-o-r".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/data/clone".into(),
///     binds: vec![],
///     env: vec![],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec!["claude".into()],
/// };
/// let words = |command: &tokio::process::Command| -> String {
///     let command = command.as_std();
///     std::iter::once(command.get_program())
///         .chain(command.get_args())
///         .map(|a| a.to_string_lossy().into_owned())
///         .collect::<Vec<_>>()
///         .join(" ")
/// };
///
/// // A session of a worker, in a unit.
/// let worker = start_command(&run_args(&spec)?, true);
/// assert!(
///     words(&worker).starts_with(
///         "systemd-run --user --scope --collect --quiet -- podman run --rm -i --sig-proxy=false "
///     ),
///     "{}",
///     words(&worker)
/// );
/// // The lead, in a unit.
/// let lead = start_command(&terminal_args(&spec)?, true);
/// assert!(
///     words(&lead).starts_with(
///         "systemd-run --user --scope --collect --quiet -- podman run --rm -d -i -t "
///     ),
///     "{}",
///     words(&lead)
/// );
/// // Out of a unit, podman starts as it is.
/// let plain = start_command(&terminal_args(&spec)?, false);
/// assert!(words(&plain).starts_with("podman run --rm -d -i -t "));
///
/// // None of them has an INVOCATION_ID.
/// for command in [&worker, &lead, &plain] {
///     let envs: Vec<_> = command.as_std().get_envs().collect();
///     assert!(envs.contains(&("INVOCATION_ID".as_ref(), None)));
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn start_command(args: &[OsString], in_unit: bool) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(starter(in_unit));
    if in_unit {
        command.args(SCOPE).arg(PODMAN);
    }
    command.args(args).env_remove(INVOCATION_ID);
    command
}

/// Runs `podman ARGS` with no stdin and no stdout, and gives its exit.
/// podman shows its progress and its errors on stderr.
async fn quiet(args: &[&str]) -> Result<ExitStatus> {
    podman()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .await
        .with_context(|| text::no_program(PODMAN))
}

/// Fails when `status` is not a success of `podman ARGS`.
fn succeeded(status: ExitStatus, args: &[&str]) -> Result<()> {
    if !status.success() {
        bail!("podman {}: {status}", args.join(" "));
    }
    Ok(())
}

impl Runtime for Podman {
    async fn has_image(&self, image: &str) -> Result<bool> {
        Ok(quiet(&["image", "exists", image]).await?.success())
    }

    async fn pull(&self, image: &str) -> Result<()> {
        let args = ["pull", image];
        succeeded(quiet(&args).await?, &args)
    }

    /// Only the `podman` child gets the values of the host, in its
    /// environment (01M4Z60B89BHXQVJ2MGRRQ2CKJ).
    async fn start(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<Running> {
        spec.check_values(values)?;
        let in_unit = in_unit();
        let mut child = start_command(&run_args(spec)?, in_unit)
            .envs(values.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| text::no_program(starter(in_unit)))?;
        let input = child.stdin.take().context("the stdin of podman")?;
        let events = child.stdout.take().context("the stdout of podman")?;
        let errors = child.stderr.take().context("the stderr of podman")?;
        Ok(Running {
            name: spec.name.clone(),
            input: Box::new(input),
            events: Box::new(events),
            errors: Box::new(errors),
            end: End::new(async move { Ok(Exit::from(child.wait().await?)) }),
        })
    }

    async fn stop(&self, name: &str) -> Result<()> {
        let args = ["stop", name];
        succeeded(quiet(&args).await?, &args)
    }

    /// A container that podman does not know does not run.
    async fn running(&self, name: &str) -> Result<bool> {
        let out = podman()
            .args([
                "container",
                "inspect",
                "--format",
                "{{.State.Running}}",
                name,
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        Ok(out.status.success() && out.stdout.trim_ascii() == b"true")
    }

    async fn attach(&self, name: &str) -> Result<Exit> {
        let status = podman()
            .args(["attach", name])
            .status()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        Ok(status.into())
    }

    /// Sets the terminal of the container to the size of the terminal of
    /// this process with one row less (one more for a terminal of one
    /// row). `podman attach` then sets the size of this terminal, and the
    /// kernel tells the command with SIGWINCH: it draws again. A plain
    /// SIGWINCH does not do it: `claude` draws only when the size changes
    /// (01M51PD7M04HZR2PVVJA4WCBK5). With no terminal here, it does
    /// nothing: `podman attach` then sets no size.
    async fn redraw(&self, name: &str) -> Result<()> {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            return Ok(());
        }
        let size = ratatui::crossterm::terminal::window_size()
            .context("cannot read the size of this terminal")?;
        let out = podman()
            .args(redraw_args(name, size.rows, size.columns))
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        if !out.status.success() {
            bail!(
                "podman exec {name} stty: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// Only the `podman` child gets the values of the host. The end is
    /// `podman wait`: no exit code when it cannot say, for example when
    /// the container went before the wait.
    async fn start_terminal(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<End> {
        spec.check_values(values)?;
        let in_unit = in_unit();
        let out = start_command(&terminal_args(spec)?, in_unit)
            .envs(values.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(starter(in_unit)))?;
        if !out.status.success() {
            bail!(
                "podman run {}: {}",
                spec.name,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let name = spec.name.clone();
        Ok(End::new(async move {
            let out = podman()
                .args(["wait", &name])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .await
                .with_context(|| text::no_program(PODMAN))?;
            let code = String::from_utf8_lossy(&out.stdout).trim().parse().ok();
            Ok(Exit {
                code: code.filter(|_| out.status.success()),
            })
        }))
    }

    async fn exec(&self, name: &str, command: &[String]) -> Result<Exit> {
        let status = podman()
            .args(["exec", "-it", name])
            .args(command)
            .status()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        Ok(status.into())
    }

    /// `podman exec NAME stat -c %h PATH`.
    async fn links(&self, name: &str, path: &Path) -> Result<u64> {
        let out = podman()
            .args(["exec", name, "stat", "-c", "%h"])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        if !out.status.success() {
            bail!(
                "podman exec {name} stat: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let shown = String::from_utf8_lossy(&out.stdout);
        shown
            .trim()
            .parse()
            .with_context(|| format!("podman exec {name} stat: {}", shown.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SessionSpec {
        SessionSpec {
            name: "riff-issue-12-1a2b3c4d".into(),
            image: "img".into(),
            home: "/home/mike".into(),
            workdir: "/r/clone".into(),
            binds: vec![
                Bind {
                    source: Source::Path("/r/clone".into()),
                    target: "/r/clone".into(),
                    write: true,
                },
                Bind {
                    source: Source::Volume("riff-cargo".into()),
                    target: "/home/mike/.cargo".into(),
                    write: true,
                },
            ],
            env: vec![Env::Host("GH_TOKEN".into())],
            memory: "8g".into(),
            cpus: 4,
            command: vec!["claude".into()],
        }
    }

    #[test]
    fn a_path_with_a_comma_or_a_colon_gives_no_arguments() {
        let bad = ["/r/a,b", "/r/a:b"];
        for path in bad {
            let mut specs = vec![
                SessionSpec {
                    home: path.into(),
                    ..spec()
                },
                SessionSpec {
                    workdir: path.into(),
                    ..spec()
                },
            ];
            let mut source = spec();
            source.binds[0].source = Source::Path(path.into());
            let mut target = spec();
            target.binds[1].target = path.into();
            specs.extend([source, target]);
            for spec in specs {
                let error = run_args(&spec).unwrap_err().to_string();
                assert!(error.contains("comma or a colon"), "{error}");
            }
        }
    }

    /// The program and the arguments of `command`, as words.
    fn words(command: &tokio::process::Command) -> Vec<String> {
        let command = command.as_std();
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|word| word.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn in_a_unit_podman_run_starts_in_a_scope_of_its_own() {
        for args in [run_args(&spec()).unwrap(), terminal_args(&spec()).unwrap()] {
            let words = words(&start_command(&args, true));
            let scope = [
                "systemd-run",
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "--",
            ];
            assert_eq!(words[..scope.len()], scope, "{words:?}");
            assert_eq!(words[scope.len()], "podman", "{words:?}");
            let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
            assert_eq!(words[scope.len() + 1..], args, "{words:?}");
        }
    }

    #[test]
    fn out_of_a_unit_podman_run_starts_as_it_is() {
        let args = run_args(&spec()).unwrap();
        let words = words(&start_command(&args, false));
        assert_eq!(words[0], "podman");
        assert_eq!(words[1], "run");
        assert!(!words.iter().any(|w| w == "systemd-run" || w == "--scope"));
    }

    #[test]
    fn podman_starts_with_no_invocation_id_in_a_unit_or_out_of_it() {
        let args = run_args(&spec()).unwrap();
        for in_unit in [true, false] {
            let command = start_command(&args, in_unit);
            let envs: Vec<_> = command.as_std().get_envs().collect();
            assert!(envs.contains(&("INVOCATION_ID".as_ref(), None)), "{envs:?}");
        }
    }

    #[test]
    fn a_value_of_the_host_is_in_no_argument() {
        let args: Vec<String> = run_args(&spec())
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let at = args.iter().position(|a| a == "GH_TOKEN").unwrap();
        assert_eq!(args[at - 1], "-e");
        assert!(!args.iter().any(|a| a.starts_with("GH_TOKEN=")), "{args:?}");
    }

    #[tokio::test]
    async fn podman_starts_no_session_with_a_value_that_the_spec_does_not_name() {
        let error = Podman
            .start(&spec(), &[("OTHER".into(), "v".into())])
            .await
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("takes no value for OTHER"),
            "{error}"
        );
    }
}
