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
//! | (each spec) | the start is `nice -n 19 podman run ...`: each process of the container gets nice 19 from podman ([`start_command`], [`NICE`]) |
//! | (the daemon in a unit) | the start is `systemd-run --user --scope --slice riff.slice --collect --quiet -- env -u INVOCATION_ID nice -n 19 podman run ...`: pasta runs in a scope of its own, in [`SLICE`] ([`start_command`], [`NO_INVOCATION_ID`], 01M5RG2AS2CYJPWGKTV7N2HNYP) |
//! | (each spec) | `--sig-proxy=false` |
//! | `name` | `--name NAME` |
//! | The user | `--userns=keep-id` |
//! | (each spec) | `--init`: the init of podman is PID 1, and reaps each orphan process (01M51117K3WXK35CE67KPRFMA8) |
//! | `memory`, `cpus` | `--memory 8g`, `--cpus 4` |
//! | (each spec) | `--cgroup-conf cpu.weight=1`: the lowest CPU weight of cgroup v2 ([`CPU_WEIGHT`]) |
//! | (each spec) | `--cgroup-parent riff.slice`: the container and its conmon run in the slice of riff ([`SLICE`]) |
//! | `home` | `--mount type=tmpfs,destination=HOME,tmpfs-mode=0700,U=true` |
//! | Each folder of [`SessionSpec::home_folders`] | `--mount type=tmpfs,destination=FOLDER,tmpfs-mode=0755,U=true`: a folder of the session user, not of root (01M748WY71EFVZR6SG5CCEQYE0) |
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
//! nice -n 19 COMMAND` with the terminal of the person
//! (01M4Z60B81RTZT71PN5HV0GZHX, 01M4Z60B83ZH10HYKW7X1C0SSS,
//! [`exec_args`]).
//!
//! # The priority
//!
//! Each process of a session container runs at the lowest CPU priority,
//! so a build in a session yields the CPU to the programs of the person
//! (#1990, #2001). A nice value counts only against the processes of its
//! own cgroup, and a CPU weight only against the other cgroups of its
//! level. So riff sets each of them:
//!
//! - The daemon starts each `podman run` of a worker, a verify and the
//!   lead with `nice -n 19` ([`NICE`], 01M7755V17SGY2NW231WJ12WXN).
//!   podman, conmon and the runtime keep the nice value, and each process
//!   of the container gets it from them: the init of podman, the relay,
//!   `claude` and each build.
//! - Each `podman exec` of riff starts its command in the container with
//!   `nice -n 19`: a takeover, and the checks `stty`, `getent` and `stat`
//!   of the daemon ([`exec_of`], 01M778VZCT30WV6VBP2GY1HNBD). So `podman
//!   exec` itself keeps the nice value of its caller.
//! - `--cgroup-conf cpu.weight=1` ([`CPU_WEIGHT`]): the runtime gives the
//!   scope of the container the systemd property `CPUWeight=1`, and
//!   writes `cpu.weight` in the cgroup of the container. It counts against
//!   the other cgroups of the slice: the scopes of conmon and of pasta.
//! - Each container runs in [`SLICE`], a slice of the manager of the user
//!   next to `app.slice`, where the manager runs the programs of the
//!   person (01M778VSP6Z29FCPGX5S76ME1T). `--cgroup-parent` puts the
//!   container and its conmon there, each in a scope of its own, for a
//!   start in a unit and out of it. `systemd-run --slice` puts the scope
//!   of `podman run` and pasta there. Before each start,
//!   [`set_cpu_weight`] gives the slice the CPU weight [`SLICE_WEIGHT`].
//!   The default weight of a cgroup is 100.
//!
//! riff writes none of them in a file of the person. A login over SSH
//! runs in a scope of the system manager, beside the manager of the user:
//! the weight of the slice does not count against it.
//!
//! ```mermaid
//! flowchart TD
//!     M["user@UID.service: the manager of the user"] --> A["app.slice: weight 100<br/>riff.service and the programs of the person"]
//!     M --> R["riff.slice: weight 1"]
//!     R --> S["run-pNNN.scope: nice -n 19 podman run, and pasta"]
//!     R --> O["libpod-conmon-ID.scope: conmon"]
//!     R --> C["libpod-ID.scope: the container,<br/>nice 19, cpu.weight 1"]
//! ```
//!
//! conmon, the monitor of podman, holds the container. A stop of the
//! daemon stops no container, and leaves its network
//! (01M4Z60B7Z24WBD6HV6ER0WN9K, 01M4ZDTW00B7N6T9P3MK066KA7,
//! 01M5RG2AS2CYJPWGKTV7N2HNYP):
//!
//! - riff starts each `podman` with no `INVOCATION_ID`. systemd sets it
//!   in each process of a unit, and with it podman keeps conmon in the
//!   cgroup of its caller. With no `INVOCATION_ID`, podman puts conmon in
//!   a scope of its own, `libpod-conmon-ID.scope` in [`SLICE`], so a stop
//!   of the unit does not end it. `systemd-run --scope` sets
//!   `INVOCATION_ID` in its command too, so in a scope riff starts podman
//!   with `env -u INVOCATION_ID` ([`NO_INVOCATION_ID`]).
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
//!     D -- "systemd-run --user --scope --slice riff.slice" --> S["scope run-pNNN.scope"]
//!     S --> P["env -u INVOCATION_ID nice -n 19 podman run"]
//!     S --> N["pasta: the network"]
//!     P -- "conmon, in libpod-conmon-ID.scope, at nice 19" --> C["the container:<br/>nice 19, cpu.weight 1"]
//! ```

use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::platform::systemd::set_cpu_weight;
use crate::ps::Proc;
use crate::runtime::{Bind, End, Env, Exit, REACH_HOST, Running, Runtime, SessionSpec, Source};
use crate::text;

/// The program of the container runtime.
pub const PODMAN: &str = "podman";

/// The words before a program that start it at nice 19, the lowest CPU
/// priority: each process of a session container runs at it (see "The
/// priority" above).
pub const NICE: [&str; 3] = ["nice", "-n", "19"];

/// The value of `--cgroup-conf` of each session container: `cpu.weight`
/// 1, the lowest CPU weight of cgroup v2. The default is 100.
pub const CPU_WEIGHT: &str = "cpu.weight=1";

/// The slice of the manager of the user that holds each session
/// container, next to `app.slice` (see "The priority" above).
pub const SLICE: &str = "riff.slice";

/// The CPU weight of [`SLICE`]: 1, the lowest of cgroup v2.
pub const SLICE_WEIGHT: u16 = 1;

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
///      --init --memory 8g --cpus 4 --cgroup-conf cpu.weight=1 \
///      --cgroup-parent riff.slice \
///      --mount type=tmpfs,destination=/home/mike,tmpfs-mode=0700,U=true \
///      --mount type=tmpfs,destination=/home/mike/.cache,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local/share,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local/state,tmpfs-mode=0755,U=true \
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
///      --cgroup-conf cpu.weight=1 --cgroup-parent riff.slice \
///      --mount type=tmpfs,destination=/home/mike,tmpfs-mode=0700,U=true \
///      --mount type=tmpfs,destination=/home/mike/.cache,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local/share,tmpfs-mode=0755,U=true \
///      --mount type=tmpfs,destination=/home/mike/.local/state,tmpfs-mode=0755,U=true \
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
///     "exec riff-lead-o-r nice -n 19 stty -F /dev/console rows 29 cols 100"
/// );
/// assert_eq!(redraw_args("n", 1, 80)[9], "2");
/// ```
pub fn redraw_args(name: &str, rows: u16, cols: u16) -> Vec<String> {
    let rows = if rows > 1 { rows - 1 } else { rows + 1 };
    exec_of(
        &[],
        name,
        [
            "stty",
            "-F",
            "/dev/console",
            "rows",
            &rows.to_string(),
            "cols",
            &cols.to_string(),
        ],
    )
}

/// The arguments of `podman` that run `command` in the container `name`
/// at nice 19 ([`NICE`]), with the options `options` of `exec`. The
/// nice value is in the container: `podman exec` itself keeps the nice
/// value of its caller (01M778VZCT30WV6VBP2GY1HNBD).
///
/// ```
/// use riff::podman::exec_of;
///
/// assert_eq!(
///     exec_of(&[], "riff-issue-12-1a2b3c4d", ["stat", "-c", "%h"]).join(" "),
///     "exec riff-issue-12-1a2b3c4d nice -n 19 stat -c %h"
/// );
/// assert_eq!(
///     exec_of(&["-it"], "riff-lead-o-r", ["claude"]).join(" "),
///     "exec -it riff-lead-o-r nice -n 19 claude"
/// );
/// ```
pub fn exec_of<'a>(
    options: &[&str],
    name: &str,
    command: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut args: Vec<String> = ["exec"]
        .into_iter()
        .chain(options.iter().copied())
        .chain([name])
        .chain(NICE)
        .map(str::to_owned)
        .collect();
    args.extend(command.into_iter().map(str::to_owned));
    args
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
    args.push("--cgroup-conf".into());
    args.push(CPU_WEIGHT.into());
    args.push("--cgroup-parent".into());
    args.push(SLICE.into());
    args.push("--mount".into());
    args.push(
        format!(
            "type=tmpfs,destination={},tmpfs-mode=0700,U=true",
            plain(&spec.home)?
        )
        .into(),
    );
    for folder in spec.home_folders() {
        args.push("--mount".into());
        args.push(
            format!(
                "type=tmpfs,destination={},tmpfs-mode=0755,U=true",
                plain(&folder)?
            )
            .into(),
        );
    }
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

/// The longest time that [`Runtime::reaches`] waits for the look-up of a
/// host. A container with no network waits for its resolver for longer.
pub const REACH_LIMIT: Duration = Duration::from_secs(15);

/// The arguments of the look-up of [`Runtime::reaches`] in the container
/// `name`, at nice 19 ([`exec_of`]).
///
/// ```
/// let args = riff::podman::reach_args("riff-lead-o-r");
/// assert_eq!(
///     args.join(" "),
///     "exec riff-lead-o-r nice -n 19 getent hosts api.anthropic.com"
/// );
/// ```
pub fn reach_args(name: &str) -> Vec<String> {
    exec_of(&[], name, ["getent", "hosts", REACH_HOST])
}

/// The arguments of `podman` for [`Runtime::links`] in the container
/// `name`, at nice 19 ([`exec_of`]): the path comes after them.
///
/// ```
/// let args = riff::podman::links_args("riff-issue-12-1a2b3c4d");
/// assert_eq!(args.join(" "), "exec riff-issue-12-1a2b3c4d nice -n 19 stat -c %h");
/// ```
pub fn links_args(name: &str) -> Vec<String> {
    exec_of(&[], name, ["stat", "-c", "%h"])
}

/// What the exit code of `getent hosts` says: the container reaches the
/// network (0), or it does not (2: the host has no address, also when the
/// resolver gets no answer). Another code is a fault of `podman exec`,
/// or of `getent`: it says nothing.
///
/// ```
/// assert_eq!(riff::podman::reached(Some(0)), Some(true));
/// assert_eq!(riff::podman::reached(Some(2)), Some(false));
/// assert_eq!(riff::podman::reached(Some(125)), None);
/// assert_eq!(riff::podman::reached(Some(127)), None);
/// assert_eq!(riff::podman::reached(None), None);
/// ```
pub fn reached(code: Option<i32>) -> Option<bool> {
    match code {
        Some(0) => Some(true),
        Some(2) => Some(false),
        _ => None,
    }
}

/// The arguments of the look-up of [`Runtime::digest`] for the container
/// `name`.
///
/// ```
/// let args = riff::podman::digest_args("riff-issue-12-1a2b3c4d");
/// assert_eq!(args, ["container", "inspect", "--format", "{{.ImageDigest}}", "riff-issue-12-1a2b3c4d"]);
/// ```
pub fn digest_args(name: &str) -> [&str; 5] {
    ["container", "inspect", "--format", "{{.ImageDigest}}", name]
}

/// The digest that `podman container inspect` printed: `sha256:` and 64
/// hex digits on one line. Anything else says nothing.
///
/// ```
/// let digest = format!("sha256:{}", "ab".repeat(32));
/// assert_eq!(riff::podman::digest_of(format!("{digest}\n").as_bytes()), Some(digest));
/// assert_eq!(riff::podman::digest_of(b""), None);
/// assert_eq!(riff::podman::digest_of(b"sha256:abc\n"), None);
/// assert_eq!(riff::podman::digest_of(b"<no value>\n"), None);
/// ```
pub fn digest_of(printed: &[u8]) -> Option<String> {
    let digest = std::str::from_utf8(printed).ok()?.trim();
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then(|| digest.to_owned())
}

/// The arguments of `podman` for [`Runtime::processes`] of the container
/// `name`: the process ID in the container, the ID of the parent, the run
/// time and the command line (#1610). `podman top` reads them on the host,
/// so the image needs no `ps`.
///
/// ```
/// let args = riff::podman::top_args("riff-issue-12-1a2b3c4d");
/// assert_eq!(args, ["top", "riff-issue-12-1a2b3c4d", "pid", "ppid", "etime", "args"]);
/// ```
pub fn top_args(name: &str) -> [&str; 6] {
    ["top", name, "pid", "ppid", "etime", "args"]
}

/// The arguments of `podman` for [`Runtime::output`]: `run` of the image
/// with the program as the entrypoint, with no network and no pull.
///
/// ```
/// use riff::podman::output_args;
///
/// assert_eq!(
///     output_args("ghcr.io/o/i@sha256:0", &["gh", "--version"]).join(" "),
///     "run --rm --network=none --pull=never --entrypoint gh ghcr.io/o/i@sha256:0 --version"
/// );
/// ```
pub fn output_args(image: &str, command: &[&str]) -> Vec<String> {
    let (program, args) = command.split_first().unwrap_or((&"", &[]));
    let mut out: Vec<String> = [
        "run",
        "--rm",
        "--network=none",
        "--pull=never",
        "--entrypoint",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.extend([program, image].map(str::to_owned));
    out.extend(args.iter().map(|a| (*a).to_owned()));
    out
}

/// How long [`Runtime::output`] waits for a program in an image.
pub const OUTPUT_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// A `podman` command with no [`INVOCATION_ID`].
fn podman() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(PODMAN);
    command.env_remove(INVOCATION_ID);
    command
}

/// The program that puts a command in a systemd scope.
pub const SYSTEMD_RUN: &str = "systemd-run";

/// The arguments of [`SYSTEMD_RUN`] before the command: a scope in
/// [`SLICE`] of the manager of the user that systemd removes when it
/// ends, and says nothing.
pub const SCOPE: [&str; 7] = [
    "--user",
    "--scope",
    "--slice",
    SLICE,
    "--collect",
    "--quiet",
    "--",
];

/// The words after [`SCOPE`] that start a program with no
/// [`INVOCATION_ID`]. `systemd-run --scope` sets it in its command, and
/// with it podman keeps conmon in the scope of `podman run`, not in a
/// scope of its own (01M4ZDTW00B7N6T9P3MK066KA7).
pub const NO_INVOCATION_ID: [&str; 3] = ["env", "-u", INVOCATION_ID];

/// True when this process runs in a systemd unit: systemd sets
/// [`INVOCATION_ID`] in each process of a unit.
fn in_unit() -> bool {
    std::env::var_os(INVOCATION_ID).is_some()
}

/// The program that [`start_command`] runs.
fn starter(in_unit: bool) -> &'static str {
    if in_unit { SYSTEMD_RUN } else { NICE[0] }
}

/// The command that starts `podman` with `args`, the arguments of `run`,
/// at nice 19 ([`NICE`]), and with no [`INVOCATION_ID`]. Each process of
/// the container gets the nice value from podman. In a unit (`in_unit`),
/// it starts in a systemd scope of its own, in [`SLICE`]: pasta, the
/// network process of podman, is then not in the cgroup of the unit, and
/// a stop of the unit leaves the network of the container
/// (01M5RG2AS2CYJPWGKTV7N2HNYP, 01M778VSP6Z29FCPGX5S76ME1T). In the
/// scope, `env` removes the [`INVOCATION_ID`] of `systemd-run`
/// ([`NO_INVOCATION_ID`]). So podman puts conmon in a scope of its own
/// in [`SLICE`], in a unit and out of it.
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
/// // A session of a worker or of a verify, in a unit.
/// let worker = start_command(&run_args(&spec)?, true);
/// assert!(
///     words(&worker).starts_with(
///         "systemd-run --user --scope --slice riff.slice --collect --quiet -- \
///          env -u INVOCATION_ID nice -n 19 podman run --rm -i --sig-proxy=false "
///     ),
///     "{}",
///     words(&worker)
/// );
/// // The lead, in a unit.
/// let lead = start_command(&terminal_args(&spec)?, true);
/// assert!(
///     words(&lead).starts_with(
///         "systemd-run --user --scope --slice riff.slice --collect --quiet -- \
///          env -u INVOCATION_ID nice -n 19 podman run --rm -d -i -t "
///     ),
///     "{}",
///     words(&lead)
/// );
/// // Out of a unit, podman starts with no scope, at nice 19 too.
/// let plain = start_command(&terminal_args(&spec)?, false);
/// assert!(words(&plain).starts_with("nice -n 19 podman run --rm -d -i -t "));
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
        command.args(SCOPE).args(NO_INVOCATION_ID).arg(NICE[0]);
    }
    command
        .args(&NICE[1..])
        .arg(PODMAN)
        .args(args)
        .env_remove(INVOCATION_ID);
    command
}

/// The arguments of `podman` for [`Runtime::exec`]: `command` in the
/// container `name` with the terminal of the person, at nice 19
/// ([`exec_of`]). `podman exec` itself keeps the nice value of the
/// person.
///
/// ```
/// let command = ["claude".to_owned(), "--resume".to_owned(), "s1".to_owned()];
/// assert_eq!(
///     riff::podman::exec_args("riff-issue-12-1a2b3c4d", &command).join(" "),
///     "exec -it riff-issue-12-1a2b3c4d nice -n 19 claude --resume s1"
/// );
/// ```
pub fn exec_args(name: &str, command: &[String]) -> Vec<String> {
    exec_of(&["-it"], name, command.iter().map(String::as_str))
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

/// The reason of a failed `podman pull` in its stderr: the last line that
/// starts with `Error:`, else the last line. podman shows its progress
/// on stderr too.
///
/// ```
/// use riff::podman::pull_reason;
///
/// let stderr = "Trying to pull ghcr.io/o/i@sha256:0...\n\
///               Error: initializing source docker://ghcr.io/o/i@sha256:0: reading manifest: unauthorized\n";
/// assert_eq!(
///     pull_reason(stderr),
///     "Error: initializing source docker://ghcr.io/o/i@sha256:0: reading manifest: unauthorized"
/// );
/// assert_eq!(pull_reason("one\ntwo\n\n"), "two");
/// assert_eq!(pull_reason(""), "podman said nothing");
/// ```
pub fn pull_reason(stderr: &str) -> String {
    let lines = || {
        stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .rfind(|line| line.starts_with("Error:"))
        .or_else(|| lines().next_back())
        .unwrap_or("podman said nothing")
        .to_owned()
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

    /// A pull that fails gives the reason of podman: its last error line
    /// ([`pull_reason`], 01M5SMSB3H1E9NXMBDN8ZDJ6CD).
    async fn pull(&self, image: &str) -> Result<()> {
        let args = ["pull", image];
        let out = podman()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprint!("{stderr}");
            bail!(
                "podman {}: {}: {}",
                args.join(" "),
                out.status,
                pull_reason(&stderr)
            );
        }
        Ok(())
    }

    /// `podman run` of the image with the program as the entrypoint, no
    /// network and no pull. It stops after [`OUTPUT_LIMIT`].
    async fn output(&self, image: &str, command: &[&str]) -> Result<String> {
        let program = command.first().context("no command to run")?;
        let run = podman()
            .args(output_args(image, command))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output();
        let out = tokio::time::timeout(OUTPUT_LIMIT, run)
            .await
            .map_err(|_| {
                anyhow!(
                    "podman run {image} {program} took more than {} s",
                    OUTPUT_LIMIT.as_secs()
                )
            })?
            .with_context(|| text::no_program(PODMAN))?;
        if !out.status.success() {
            bail!("podman run {image} {program}: {}", out.status);
        }
        String::from_utf8(out.stdout)
            .with_context(|| format!("{program} in {image} prints no UTF-8"))
    }

    /// [`Podman::start_as`], in a unit when this process runs in one.
    async fn start(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<Running> {
        self.start_as(spec, values, in_unit()).await
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

    /// [`Podman::start_terminal_as`], in a unit when this process runs in
    /// one.
    async fn start_terminal(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<End> {
        self.start_terminal_as(spec, values, in_unit()).await
    }

    /// `podman exec -it NAME nice -n 19 COMMAND` ([`exec_args`]).
    async fn exec(&self, name: &str, command: &[String]) -> Result<Exit> {
        let status = podman()
            .args(exec_args(name, command))
            .status()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        Ok(status.into())
    }

    /// `podman exec NAME nice -n 19 stat -c %h PATH` ([`links_args`]).
    async fn links(&self, name: &str, path: &Path) -> Result<u64> {
        let out = podman()
            .args(links_args(name))
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

    /// `podman exec NAME nice -n 19 getent hosts api.anthropic.com`
    /// ([`reach_args`]).
    /// A look-up that gets no answer in [`REACH_LIMIT`] says no network:
    /// `podman exec` ends with it.
    async fn reaches(&self, name: &str) -> Result<bool> {
        let mut child = podman()
            .args(reach_args(name))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| text::no_program(PODMAN))?;
        let Ok(status) = tokio::time::timeout(REACH_LIMIT, child.wait()).await else {
            return Ok(false);
        };
        let status = status?;
        reached(status.code()).ok_or_else(|| anyhow!("podman exec {name} getent: {status}"))
    }

    /// `podman container inspect --format {{.ImageDigest}} NAME`
    /// ([`digest_args`]).
    async fn digest(&self, name: &str) -> Result<String> {
        let out = podman()
            .args(digest_args(name))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        out.status
            .success()
            .then(|| digest_of(&out.stdout))
            .flatten()
            .ok_or_else(|| anyhow!("podman does not say the image digest of {name}"))
    }

    /// `podman top NAME pid ppid etime args` ([`top_args`]).
    async fn processes(&self, name: &str) -> Result<Vec<Proc>> {
        let out = podman()
            .args(top_args(name))
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| text::no_program(PODMAN))?;
        if !out.status.success() {
            bail!(
                "podman top {name}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(crate::ps::parse_top(&String::from_utf8_lossy(&out.stdout)))
    }
}

impl Podman {
    /// [`Runtime::start`], with the start of a unit (`in_unit`) or not,
    /// whatever the environment of this process says ([`start_command`]).
    /// Only the `podman` child gets the values of the host, in its
    /// environment (01M58B7H2HMYT6VTY0B8T2G0KW). Before it, [`SLICE`]
    /// gets the CPU weight [`SLICE_WEIGHT`]: a start fails when systemd
    /// does not set it (01M778VSP6Z29FCPGX5S76ME1T).
    pub async fn start_as(
        &self,
        spec: &SessionSpec,
        values: &[(String, String)],
        in_unit: bool,
    ) -> Result<Running> {
        spec.check_values(values)?;
        set_cpu_weight(SLICE, SLICE_WEIGHT).await?;
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

    /// [`Runtime::start_terminal`], with the start of a unit (`in_unit`)
    /// or not, as [`Podman::start_as`]. Only the `podman` child gets the
    /// values of the host. The end is `podman wait`: no exit code when it
    /// cannot say, for example when the container went before the wait.
    /// As [`Podman::start_as`], it first gives [`SLICE`] its weight.
    pub async fn start_terminal_as(
        &self,
        spec: &SessionSpec,
        values: &[(String, String)],
        in_unit: bool,
    ) -> Result<End> {
        spec.check_values(values)?;
        set_cpu_weight(SLICE, SLICE_WEIGHT).await?;
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
    fn each_folder_above_a_bind_in_the_home_is_a_tmpfs_of_the_session_user() {
        let h = "/home/mike";
        let folder = format!("{h}/.local/share/riff/sessions/issue-12-1a2b3c4d");
        let path = |target: String, write| Bind {
            source: Source::Path(target.clone().into()),
            target: target.into(),
            write,
        };
        let spec = SessionSpec {
            binds: vec![
                path(format!("{folder}/clone"), true),
                path(format!("{h}/src/o/r/.git/objects"), false),
                path(format!("{h}/.gitconfig"), false),
                path(format!("{h}/.claude"), true),
                path(format!("{h}/.claude/.credentials.json"), false),
                Bind {
                    source: Source::Volume("riff-sccache".into()),
                    target: format!("{h}/.cache/sccache").into(),
                    write: true,
                },
                path(format!("{folder}/gh"), false),
                path("/run/riff".into(), true),
            ],
            ..spec()
        };
        let args: Vec<String> = run_args(&spec)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let tmpfs: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--mount")
            .map(|w| w[1].as_str())
            .collect();
        let mine = |f: &str| format!("type=tmpfs,destination={f},tmpfs-mode=0755,U=true");
        let mut want = vec![format!("type=tmpfs,destination={h},tmpfs-mode=0700,U=true")];
        want.extend(
            [
                ".cache",
                ".local",
                ".local/share",
                ".local/share/riff",
                ".local/share/riff/sessions",
                ".local/share/riff/sessions/issue-12-1a2b3c4d",
                ".local/state",
                "src",
                "src/o",
                "src/o/r",
                "src/o/r/.git",
            ]
            .map(|f| mine(&format!("{h}/{f}"))),
        );
        assert_eq!(tmpfs, want);
        // The folders come before the binds.
        let last = args.iter().rposition(|a| a == "--mount").unwrap();
        let first = args.iter().position(|a| a == "-v").unwrap();
        assert!(last < first, "{args:?}");
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
                "--slice",
                "riff.slice",
                "--collect",
                "--quiet",
                "--",
            ];
            assert_eq!(words[..scope.len()], scope, "{words:?}");
            let nice = scope.len() + NO_INVOCATION_ID.len();
            assert_eq!(
                words[scope.len()..nice],
                ["env", "-u", "INVOCATION_ID"],
                "{words:?}"
            );
            let podman = nice + NICE.len();
            assert_eq!(words[nice..podman], NICE, "{words:?}");
            assert_eq!(words[podman], "podman", "{words:?}");
            let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
            assert_eq!(words[podman + 1..], args, "{words:?}");
        }
    }

    #[test]
    fn out_of_a_unit_podman_run_starts_with_no_scope() {
        let args = run_args(&spec()).unwrap();
        let words = words(&start_command(&args, false));
        assert_eq!(words[..5], ["nice", "-n", "19", "podman", "run"]);
        assert!(!words.iter().any(|w| w == "systemd-run" || w == "--scope"));
    }

    /// The worker and the verify start with [`run_args`], the lead with
    /// [`terminal_args`]. In a unit or out of it, podman starts at nice
    /// 19, and the container gets the lowest CPU weight.
    #[test]
    fn each_kind_of_session_starts_at_nice_19_with_the_lowest_cpu_weight() {
        let kinds = [
            ("worker", run_args(&spec()).unwrap()),
            ("verify", run_args(&spec()).unwrap()),
            ("lead", terminal_args(&spec()).unwrap()),
        ];
        for (kind, args) in kinds {
            for in_unit in [true, false] {
                let words = words(&start_command(&args, in_unit));
                let podman = words.iter().position(|w| w == "podman").unwrap();
                assert_eq!(
                    words[podman - 3..podman],
                    ["nice", "-n", "19"],
                    "{kind}: {words:?}"
                );
                assert!(
                    words[podman..]
                        .windows(2)
                        .any(|w| w == ["--cgroup-conf", "cpu.weight=1"]),
                    "{kind}: {words:?}"
                );
                // The options come before the image.
                let weight = words.iter().position(|w| w == "cpu.weight=1").unwrap();
                let image = words.iter().position(|w| w == "img").unwrap();
                assert!(weight < image, "{kind}: {words:?}");
            }
        }
    }

    /// 01M778VSP6Z29FCPGX5S76ME1T: each container of a worker, a verify
    /// and the lead starts in the slice of riff, in a unit and out of it.
    /// In a unit, the scope of `podman run` and pasta is in it too.
    #[test]
    fn each_container_starts_in_the_slice_of_riff() {
        assert_eq!(SLICE, "riff.slice");
        assert_eq!(CPU_WEIGHT, format!("cpu.weight={SLICE_WEIGHT}"));
        let kinds = [
            ("worker", run_args(&spec()).unwrap()),
            ("verify", run_args(&spec()).unwrap()),
            ("lead", terminal_args(&spec()).unwrap()),
        ];
        for (kind, args) in kinds {
            for in_unit in [true, false] {
                let words = words(&start_command(&args, in_unit));
                let podman = words.iter().position(|w| w == "podman").unwrap();
                let parent = words[podman..]
                    .windows(2)
                    .position(|w| w == ["--cgroup-parent", "riff.slice"])
                    .map(|at| podman + at);
                let image = words.iter().position(|w| w == "img").unwrap();
                assert!(parent.is_some_and(|at| at < image), "{kind}: {words:?}");
                let scope = words[..podman]
                    .windows(2)
                    .any(|w| w == ["--slice", "riff.slice"]);
                assert_eq!(scope, in_unit, "{kind}: {words:?}");
            }
        }
    }

    /// 01M778VZCT30WV6VBP2GY1HNBD: each `podman exec` of riff starts its
    /// command at nice 19 in the container, right after the name.
    #[test]
    fn each_exec_of_riff_runs_its_command_at_nice_19_in_the_container() {
        let name = "riff-issue-12-1a2b3c4d";
        let execs = [
            ("takeover", exec_args(name, &["claude".to_owned()])),
            ("redraw", redraw_args(name, 30, 100)),
            ("reach", reach_args(name)),
            ("links", links_args(name)),
        ];
        for (what, args) in execs {
            assert_eq!(args[0], "exec", "{what}: {args:?}");
            let at = args.iter().position(|a| a == name).unwrap();
            assert_eq!(args[at + 1..at + 4], NICE, "{what}: {args:?}");
            assert!(args.len() > at + 4, "{what}: {args:?}");
        }
    }

    #[test]
    fn a_takeover_runs_its_command_at_nice_19_in_the_container() {
        let command = ["claude".to_owned(), "--resume".to_owned()];
        let args = exec_args("riff-issue-12-1a2b3c4d", &command);
        assert_eq!(
            args,
            [
                "exec",
                "-it",
                "riff-issue-12-1a2b3c4d",
                "nice",
                "-n",
                "19",
                "claude",
                "--resume"
            ]
        );
    }

    /// 01M78TQ48B5JQXGRSDBGWMZDS7: `systemd-run --scope` sets an
    /// `INVOCATION_ID` in its command, so in a unit `env` removes it again
    /// right before podman.
    #[test]
    fn podman_starts_with_no_invocation_id_in_a_unit_or_out_of_it() {
        for args in [run_args(&spec()).unwrap(), terminal_args(&spec()).unwrap()] {
            for in_unit in [true, false] {
                let command = start_command(&args, in_unit);
                let envs: Vec<_> = command.as_std().get_envs().collect();
                assert!(envs.contains(&("INVOCATION_ID".as_ref(), None)), "{envs:?}");
                let words = words(&command);
                let nice = words.iter().position(|w| w == "nice").unwrap();
                let unset = words[..nice].ends_with(&NO_INVOCATION_ID.map(str::to_owned));
                assert_eq!(unset, in_unit, "{words:?}");
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
