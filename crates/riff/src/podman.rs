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
//! stdout of the command is [`Running::events`]. conmon, the monitor of
//! podman, holds the container. `podman stop` ends the container and
//! each of its processes (01M4Z60B7XT7462XMV49GJTVSJ).

use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::runtime::{Bind, End, Env, Exit, Runtime, Running, SessionSpec, Source};
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
///     "run --rm -i --name riff-issue-12-1a2b3c4d --userns=keep-id --memory 8g --cpus 4 \
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
    let mut args: Vec<OsString> = ["run", "--rm", "-i", "--name"]
        .into_iter()
        .map(OsString::from)
        .collect();
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

/// Runs `podman ARGS` with no stdin and no stdout, and gives its exit.
/// podman shows its progress and its errors on stderr.
async fn quiet(args: &[&str]) -> Result<ExitStatus> {
    tokio::process::Command::new(PODMAN)
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
        let mut child = tokio::process::Command::new(PODMAN)
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

    async fn attach(&self, name: &str) -> Result<Exit> {
        let status = tokio::process::Command::new(PODMAN)
            .args(["attach", name])
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
        assert!(error.to_string().contains("takes no value for OTHER"), "{error}");
    }
}
