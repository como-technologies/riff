//! The [`Runtime`] of riff 3.0 on Linux: rootless podman (#715,
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
//! | (each spec) | `--sig-proxy=false` |
//! | `name` | `--name NAME` |
//! | The user | `--userns=keep-id` |
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
//! Ctrl-Q. A takeover runs `podman exec -it NAME COMMAND` with the
//! terminal of the person (01M4Z60B81RTZT71PN5HV0GZHX,
//! 01M4Z60B83ZH10HYKW7X1C0SSS).
//!
//! conmon, the monitor of podman, holds the container. A stop of the
//! daemon stops no container (01M4Z60B7Z24WBD6HV6ER0WN9K,
//! 01M4ZDTW00B7N6T9P3MK066KA7):
//!
//! - riff starts each `podman` with no `INVOCATION_ID`. systemd sets it
//!   in each process of a unit, and with it podman keeps conmon in the
//!   cgroup of the unit of the daemon. With no `INVOCATION_ID`, podman
//!   puts conmon in a scope of its own, so a stop of the unit does not
//!   end it.
//! - `--sig-proxy=false`: the `podman run` of riff passes no signal to
//!   the container. So the SIGTERM of a stop of the unit ends that
//!   `podman run`, not the container. riff stops a container only with
//!   [`Runtime::stop`].

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
///      --memory 8g --cpus 4 \
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
///     "run --rm -d -i -t --name riff-lead-o-r --userns=keep-id --memory 8g --cpus 4 \
///      --mount type=tmpfs,destination=/home/mike,tmpfs-mode=0700,U=true \
///      -w /data/clone img claude"
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn terminal_args(spec: &SessionSpec) -> Result<Vec<OsString>> {
    args_of(spec, &["run", "--rm", "-d", "-i", "-t"])
}

/// The arguments of `podman` for `spec`, after the arguments `first`.
fn args_of(spec: &SessionSpec, first: &[&str]) -> Result<Vec<OsString>> {
    let mut args: Vec<OsString> = first.iter().map(OsString::from).collect();
    args.push("--name".into());
    args.push(spec.name.clone().into());
    args.push("--userns=keep-id".into());
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
        let mut child = podman()
            .args(run_args(spec)?)
            .envs(values.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| text::no_program(PODMAN))?;
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

    /// Only the `podman` child gets the values of the host. The end is
    /// `podman wait`: no exit code when it cannot say, for example when
    /// the container went before the wait.
    async fn start_terminal(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<End> {
        spec.check_values(values)?;
        let out = podman()
            .args(terminal_args(spec)?)
            .envs(values.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
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
