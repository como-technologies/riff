//! The runtime of a session: the trait [`Runtime`] and the plain data of
//! a session, [`SessionSpec`] (#715, 01M4Z60B7GTV4HHF9R3TMZRS7J).
//!
//! # Design
//!
//! A session is a [`SessionSpec`] of plain data: the name, the image, the
//! binds, the environment, the limits and the command. A [`Runtime`]
//! makes a running session from it, and stops it
//! (01M4ZBGK3A977KZW50AZFR95E9). riff runs on Linux only. Its
//! runtime there is rootless podman, [`crate::podman::Podman`], behind
//! `cfg(target_os = "linux")`. A pod backend can take the same spec
//! later.
//!
//! The code that drives a session takes its runtime as a parameter: the
//! daemon ([`crate::daemon::Daemon`]). So its tests take [`Fake`], a
//! runtime with no container (01M4ZBGK5G3EFXWT61DR2NHMDD). A spec is
//! plain data: `riff run` sends it to the daemon as JSON.
//!
//! ```mermaid
//! flowchart LR
//!     F[the facts of the host] -->|run::spec| S[SessionSpec]
//!     S -->|start, with the values of the host| R[Running]
//!     R -->|input| C[the command of the session]
//!     C -->|events, errors| R
//!     R -->|stop| E[End: the exit]
//!     T[a terminal] -->|attach| C
//! ```
//!
//! | Step | The trait | podman |
//! |---|---|---|
//! | Find the image | [`Runtime::has_image`] | `podman image exists IMAGE` |
//! | Get the image | [`Runtime::pull`] | `podman pull IMAGE` |
//! | Start | [`Runtime::start`] | `podman run --rm -i ...` ([`crate::podman::run_args`]) |
//! | The events | [`Running::events`] | the stdout of `podman run` |
//! | Stop | [`Runtime::stop`] | `podman stop NAME` |
//! | Find a live session | [`Runtime::running`] | `podman container inspect --format {{.State.Running}} NAME` |
//! | Attach a terminal | [`Runtime::attach`] | `podman attach NAME` |
//! | Draw the terminal again at the next attach | [`Runtime::redraw`] | `podman exec NAME stty -F /dev/console rows ROWS cols COLS` |
//! | Start with a terminal | [`Runtime::start_terminal`] | `podman run --rm -d -i -t ...` ([`crate::podman::terminal_args`]), then `podman wait NAME` |
//! | Run a command with a terminal | [`Runtime::exec`] | `podman exec -it NAME COMMAND` |
//! | Find out if a live session reaches the network | [`Runtime::reaches`] | `podman exec NAME getent hosts api.anthropic.com` |
//! | Find the image digest of a live session | [`Runtime::digest`] | `podman container inspect --format {{.ImageDigest}} NAME` |
//!
//! A session with a terminal is the lead (01M4Z60B81RTZT71PN5HV0GZHX).
//! The runtime holds its terminal: conmon on podman. A person attaches to
//! it, and detaches with Ctrl-P Ctrl-Q: the session runs on. A takeover
//! runs `claude --resume ID` with the terminal of the person in the
//! container of a worker ([`Runtime::exec`], 01M4Z60B83ZH10HYKW7X1C0SSS).
//!
//! A spec holds no secret. Each [`Env::Host`] of a spec names a variable
//! only. Its value goes to [`Runtime::start`] beside the spec, and the
//! runtime gives it to the session with no command line
//! (01M4Z60B89BHXQVJ2MGRRQ2CKJ).

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, ready};

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, oneshot};

/// Where a bind comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// A path on the host.
    Path(PathBuf),
    /// A named volume of the runtime.
    Volume(String),
}

/// One bind of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bind {
    /// The path on the host, or the volume.
    pub source: Source,
    /// The path in the session.
    pub target: PathBuf,
    /// True when the session can write it.
    pub write: bool,
}

/// One variable of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Env {
    /// The name and its value: the value is in the spec.
    Value(String, String),
    /// Only the name. The value goes to [`Runtime::start`] beside the
    /// spec, so it is in no spec and in no command line
    /// (01M4Z60B89BHXQVJ2MGRRQ2CKJ).
    Host(String),
}

/// A session as plain data: the image, the binds, the environment and
/// the limits (01M4Z60B7GTV4HHF9R3TMZRS7J).
///
/// ```
/// use riff::runtime::{Bind, Env, SessionSpec, Source};
///
/// let spec = SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "localhost/riff-session:latest".into(),
///     home: "/home/mike".into(),
///     workdir: "/data/clone".into(),
///     binds: vec![Bind {
///         source: Source::Path("/data/clone".into()),
///         target: "/data/clone".into(),
///         write: true,
///     }],
///     env: vec![Env::Value("USER".into(), "mike".into()), Env::Host("GH_TOKEN".into())],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec!["claude".into(), "-p".into()],
/// };
/// // The token of gh goes beside the spec, by its name.
/// let values = [("GH_TOKEN".to_owned(), "gho_x".to_owned())];
/// assert!(spec.check_values(&values).is_ok());
/// assert!(spec.check_values(&[]).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSpec {
    /// The name of the session: the name of its container.
    pub name: String,
    /// The image.
    pub image: String,
    /// The home of the person: a tmpfs in the session.
    pub home: PathBuf,
    /// The working folder in the session.
    pub workdir: PathBuf,
    /// The binds, in order.
    pub binds: Vec<Bind>,
    /// The variables, in order.
    pub env: Vec<Env>,
    /// The memory limit, for example `8g`.
    pub memory: String,
    /// The CPU limit.
    pub cpus: u32,
    /// The command and its arguments.
    pub command: Vec<String>,
}

impl SessionSpec {
    /// Checks the values of the host for this spec: one value for each
    /// [`Env::Host`], and no value for another name. Each runtime calls
    /// it in [`Runtime::start`].
    ///
    /// ```
    /// use riff::runtime::{Env, SessionSpec};
    ///
    /// let spec = SessionSpec {
    ///     name: "s".into(),
    ///     image: "i".into(),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![Env::Host("GH_TOKEN".into())],
    ///     memory: "8g".into(),
    ///     cpus: 4,
    ///     command: vec![],
    /// };
    /// let value = |n: &str| (n.to_owned(), "v".to_owned());
    /// assert!(spec.check_values(&[value("GH_TOKEN")]).is_ok());
    /// assert!(spec.check_values(&[value("GH_TOKEN"), value("GH_TOKEN")]).is_err());
    /// assert!(spec.check_values(&[value("GH_TOKEN"), value("OTHER")]).is_err());
    /// ```
    pub fn check_values(&self, values: &[(String, String)]) -> Result<()> {
        let hosts: BTreeSet<&str> = self
            .env
            .iter()
            .filter_map(|e| match e {
                Env::Host(name) => Some(name.as_str()),
                Env::Value(..) => None,
            })
            .collect();
        let mut given = BTreeSet::new();
        for (name, _) in values {
            if !hosts.contains(name.as_str()) {
                bail!("the session {} takes no value for {name}", self.name);
            }
            if !given.insert(name.as_str()) {
                bail!("the session {} got two values for {name}", self.name);
            }
        }
        if let Some(missing) = hosts.difference(&given).next() {
            bail!("the session {} needs a value for {missing}", self.name);
        }
        Ok(())
    }
}

/// How the command of a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// The exit code, or `None` when a signal ended it.
    pub code: Option<i32>,
}

impl Exit {
    /// The exit 0.
    pub const SUCCESS: Exit = Exit { code: Some(0) };

    /// True for the exit 0.
    ///
    /// ```
    /// use riff::runtime::Exit;
    ///
    /// assert!(Exit::SUCCESS.success());
    /// assert!(!Exit { code: Some(1) }.success());
    /// assert!(!Exit { code: None }.success());
    /// ```
    pub fn success(self) -> bool {
        self.code == Some(0)
    }
}

impl From<std::process::ExitStatus> for Exit {
    fn from(status: std::process::ExitStatus) -> Self {
        Exit {
            code: status.code(),
        }
    }
}

/// The end of a running session: a future of its [`Exit`].
pub struct End(Pin<Box<dyn Future<Output = Result<Exit>> + Send>>);

impl End {
    /// The end that `exit` gives.
    pub fn new(exit: impl Future<Output = Result<Exit>> + Send + 'static) -> Self {
        End(Box::pin(exit))
    }

    /// Waits for the end of the session.
    pub async fn wait(self) -> Result<Exit> {
        self.0.await
    }
}

/// A session that runs: its input, its events, its errors and its end.
pub struct Running {
    /// The name of the session.
    pub name: String,
    /// The stdin of the command. riff writes one message on each line.
    /// A drop closes it.
    pub input: Box<dyn AsyncWrite + Send + Unpin>,
    /// The stdout of the command: one event on each line.
    pub events: Box<dyn AsyncRead + Send + Unpin>,
    /// The stderr of the command and of the runtime.
    pub errors: Box<dyn AsyncRead + Send + Unpin>,
    /// The end of the session.
    pub end: End,
}

/// The runtime of the sessions (01M4Z60B7GTV4HHF9R3TMZRS7J,
/// 01M4ZBGK3A977KZW50AZFR95E9). On Linux, it is
/// [`crate::podman::Podman`]. The tests take [`Fake`].
///
/// The code that drives a session is generic over it:
///
/// ```
/// use riff::runtime::{Env, Fake, Runtime, SessionSpec};
/// use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
///
/// /// Gives the session one message, and gives its first event.
/// async fn ask(runtime: &impl Runtime, spec: &SessionSpec) -> anyhow::Result<String> {
///     if !runtime.has_image(&spec.image).await? {
///         runtime.pull(&spec.image).await?;
///     }
///     let values = [("GH_TOKEN".to_owned(), "gho_x".to_owned())];
///     let mut running = runtime.start(spec, &values).await?;
///     running.input.write_all(b"{\"type\":\"user\"}\n").await?;
///     let first = BufReader::new(running.events).lines().next_line().await?;
///     drop(running.input);
///     assert!(running.end.wait().await?.success());
///     Ok(first.unwrap_or_default())
/// }
///
/// let spec = SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "ghcr.io/o/i@sha256:0".into(),
///     home: "/home/mike".into(),
///     workdir: "/data/clone".into(),
///     binds: vec![],
///     env: vec![Env::Host("GH_TOKEN".into())],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec!["claude".into(), "-p".into()],
/// };
/// let fake = Fake::default()
///     .with_registry(&spec.image)
///     .with_events([r#"{"type":"result"}"#]);
/// let first = tokio::runtime::Builder::new_current_thread()
///     .build()?
///     .block_on(ask(&fake, &spec))?;
/// assert_eq!(first, r#"{"type":"result"}"#);
/// assert_eq!(fake.calls(), ["pull ghcr.io/o/i@sha256:0", "start riff-issue-12-1a2b3c4d"]);
/// assert_eq!(fake.input("riff-issue-12-1a2b3c4d"), "{\"type\":\"user\"}\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub trait Runtime: Send + Sync {
    /// True when this machine has the image `image`.
    fn has_image(&self, image: &str) -> impl Future<Output = Result<bool>> + Send;

    /// Gets the image `image` from its registry.
    fn pull(&self, image: &str) -> impl Future<Output = Result<()>> + Send;

    /// Runs `command` in a container of the image `image`, with no
    /// network, and gives what it prints. An error when the command
    /// fails, or the image has no such program
    /// ([`crate::image_tools`], 01M5SNTPHHXP54YBTV09JNRY3F).
    fn output(&self, image: &str, command: &[&str]) -> impl Future<Output = Result<String>> + Send;

    /// Starts the session of `spec`. `values` gives the value of each
    /// [`Env::Host`] of the spec ([`SessionSpec::check_values`]).
    fn start(
        &self,
        spec: &SessionSpec,
        values: &[(String, String)],
    ) -> impl Future<Output = Result<Running>> + Send;

    /// Stops the session `name`, and each of its processes. Its end
    /// comes with no exit code.
    fn stop(&self, name: &str) -> impl Future<Output = Result<()>> + Send;

    /// True when the session `name` runs. A new daemon takes up each
    /// session that runs (01M4ZHT01HH88KPA8628M7VBEY).
    fn running(&self, name: &str) -> impl Future<Output = Result<bool>> + Send;

    /// Attaches the terminal of this process to the session `name`,
    /// until a detach or the end of the session.
    fn attach(&self, name: &str) -> impl Future<Output = Result<Exit>> + Send;

    /// Makes the command of the session `name` draw its terminal again
    /// at the next [`Runtime::attach`] (01M51PD7M04HZR2PVVJA4WCBK5). A
    /// command such as `claude` draws only when the size of its terminal
    /// changes, and an attach with the same size changes nothing.
    fn redraw(&self, name: &str) -> impl Future<Output = Result<()>> + Send;

    /// Starts the session of `spec` with a terminal that the runtime
    /// holds: [`Runtime::attach`] shows it. It gives the end of the
    /// session. `values` is as for [`Runtime::start`].
    fn start_terminal(
        &self,
        spec: &SessionSpec,
        values: &[(String, String)],
    ) -> impl Future<Output = Result<End>> + Send;

    /// Runs `command` in the live session `name` with the terminal of
    /// this process, until the command ends.
    fn exec(&self, name: &str, command: &[String]) -> impl Future<Output = Result<Exit>> + Send;

    /// The count of the links of the file `path` in the live session
    /// `name`. A file of the host that the session binds has 0 links when
    /// the host replaced it: the session holds the old file
    /// (01M5NJ87YEHHGJBANM7CRYV56Z).
    fn links(&self, name: &str, path: &Path) -> impl Future<Output = Result<u64>> + Send;

    /// True when the live session `name` reaches the network: it looks up
    /// the host of the API of `claude` ([`REACH_HOST`]). A container that
    /// lost its network process runs on with no network
    /// (01M5SMQBTA3HQXPKKK01ZGFB4H). Fails when the runtime cannot say.
    fn reaches(&self, name: &str) -> impl Future<Output = Result<bool>> + Send;

    /// The digest of the image that the container `name` runs, as
    /// `sha256:` and 64 hex digits. It is the digest of the image that
    /// the runtime has, not the digest that the spec named
    /// (01M5TKPG5WZ1T58FNAMZNP6CJ5). Fails when the runtime cannot say.
    fn digest(&self, name: &str) -> impl Future<Output = Result<String>> + Send;
}

/// The host that [`Runtime::reaches`] looks up: the API of `claude`.
pub const REACH_HOST: &str = "api.anthropic.com";

/// A runtime with no container, for tests (01M4ZBGK5G3EFXWT61DR2NHMDD).
///
/// Each session of it plays a `claude` of one turn: it prints the lines
/// of [`Fake::with_events`], keeps its input, and ends with the exit 0
/// when its input closes. A stop ends it with no exit code, as a signal
/// does. A start needs the image on this machine: [`Fake::with_image`],
/// or a pull of an image of [`Fake::with_registry`].
///
/// ```
/// use riff::runtime::{Exit, Fake, Runtime, SessionSpec};
///
/// let spec = SessionSpec {
///     name: "s1".into(),
///     image: "i".into(),
///     home: "/h".into(),
///     workdir: "/w".into(),
///     binds: vec![],
///     env: vec![],
///     memory: "8g".into(),
///     cpus: 4,
///     command: vec![],
/// };
/// let fake = Fake::default().with_image("i");
/// tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
///     let running = fake.start(&spec, &[]).await?;
///     // A second session with the same name does not start.
///     assert!(fake.start(&spec, &[]).await.is_err());
///     fake.stop("s1").await?;
///     assert_eq!(running.end.wait().await?, Exit { code: None });
///     assert!(fake.stop("s1").await.is_err());
///     Ok::<(), anyhow::Error>(())
/// })?;
/// assert_eq!(fake.calls(), ["start s1", "start s1", "stop s1", "stop s1"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct Fake {
    book: Arc<Mutex<Book>>,
}

/// The state of a [`Fake`].
#[derive(Debug, Default)]
struct Book {
    /// The images on this machine.
    local: BTreeSet<String>,
    /// The images that a pull finds.
    registry: BTreeSet<String>,
    /// What a command prints in an image, by image and program.
    tools: BTreeMap<(String, String), String>,
    /// The lines that each new session prints.
    events: Vec<String>,
    /// Each pull, start, stop, redraw, attach and exec, in order.
    calls: Vec<String>,
    /// The sessions, by name.
    sessions: BTreeMap<String, Played>,
    /// True when [`Runtime::running`] fails: podman cannot say.
    no_answer: bool,
    /// True when each attach ends the session: the person ends the
    /// command in the terminal.
    exit_in_attach: bool,
    /// The sessions with no network ([`Runtime::reaches`]).
    offline: BTreeSet<String>,
    /// The digest that the container of a session runs, by name
    /// ([`Fake::with_digest`]). A session with none runs the digest of
    /// its image.
    digests: BTreeMap<String, String>,
    /// The sessions that do not start ([`Fake::refuse_start`]).
    refused: BTreeSet<String>,
}

/// One session of a [`Fake`].
#[derive(Debug)]
struct Played {
    spec: SessionSpec,
    values: Vec<(String, String)>,
    input: Vec<u8>,
    /// The writer of the events. The end drops it: the events end.
    events: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// The sender of the end. The first end takes it.
    end: Option<oneshot::Sender<Exit>>,
    /// The exit, after the end.
    exit: Option<Exit>,
    /// Each file of the host that the session binds, open from its
    /// start, by its path in the session: [`Runtime::links`] reads it as
    /// a container does.
    files: BTreeMap<PathBuf, std::fs::File>,
}

impl Fake {
    /// This machine has the image `image`.
    pub fn with_image(self, image: &str) -> Self {
        self.lock().local.insert(image.to_owned());
        self
    }

    /// A pull finds the image `image`.
    pub fn with_registry(self, image: &str) -> Self {
        self.lock().registry.insert(image.to_owned());
        self
    }

    /// The program `program` prints `printed` in the image `image`
    /// ([`Runtime::output`]). An image has no other program.
    pub fn with_tool(self, image: &str, program: &str, printed: &str) -> Self {
        self.lock()
            .tools
            .insert((image.to_owned(), program.to_owned()), printed.to_owned());
        self
    }

    /// Each new session prints `lines`, one on each line.
    pub fn with_events<S: Into<String>>(self, lines: impl IntoIterator<Item = S>) -> Self {
        self.lock().events = lines.into_iter().map(Into::into).collect();
        self
    }

    /// [`Runtime::running`] fails, as when podman cannot say whether a
    /// container runs.
    pub fn with_no_answer(self) -> Self {
        self.lock().no_answer = true;
        self
    }

    /// Each attach ends the session with the exit 0, as when the person
    /// types `/exit` in the `claude` of the lead.
    pub fn with_exit_in_attach(self) -> Self {
        self.lock().exit_in_attach = true;
        self
    }

    /// The container of the session `name` runs the image with the digest
    /// `digest`, as `sha256:` and 64 hex digits ([`Runtime::digest`]).
    /// A session with none runs the digest of the image of its spec.
    ///
    /// ```
    /// use riff::runtime::{Fake, Runtime, SessionSpec};
    ///
    /// let spec = |name: &str| SessionSpec {
    ///     name: name.into(),
    ///     image: format!("ghcr.io/o/i@sha256:{}", "0".repeat(64)),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![],
    ///     memory: "8g".into(),
    ///     cpus: 4,
    ///     command: vec![],
    /// };
    /// let new = format!("sha256:{}", "1".repeat(64));
    /// let fake = Fake::default()
    ///     .with_image(&spec("s1").image)
    ///     .with_digest("s2", &new);
    /// tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
    ///     let _s1 = fake.start(&spec("s1"), &[]).await?;
    ///     let _s2 = fake.start(&spec("s2"), &[]).await?;
    ///     assert_eq!(fake.digest("s1").await?, format!("sha256:{}", "0".repeat(64)));
    ///     assert_eq!(fake.digest("s2").await?, new);
    ///     assert!(fake.digest("s3").await.is_err());
    ///     Ok::<(), anyhow::Error>(())
    /// })?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_digest(self, name: &str, digest: &str) -> Self {
        self.lock()
            .digests
            .insert(name.to_owned(), digest.to_owned());
        self
    }

    /// The session `name` has no network: [`Runtime::reaches`] says false
    /// for it, as for a container that lost its network process.
    ///
    /// ```
    /// use riff::runtime::{Fake, Runtime, SessionSpec};
    ///
    /// let spec = |name: &str| SessionSpec {
    ///     name: name.into(),
    ///     image: "i".into(),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![],
    ///     memory: "8g".into(),
    ///     cpus: 4,
    ///     command: vec![],
    /// };
    /// let fake = Fake::default().with_image("i").with_no_network("s2");
    /// tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
    ///     let _s1 = fake.start(&spec("s1"), &[]).await?;
    ///     let _s2 = fake.start(&spec("s2"), &[]).await?;
    ///     assert!(fake.reaches("s1").await?);
    ///     assert!(!fake.reaches("s2").await?);
    ///     // A session that does not run cannot say.
    ///     assert!(fake.reaches("s3").await.is_err());
    ///     Ok::<(), anyhow::Error>(())
    /// })?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_no_network(self, name: &str) -> Self {
        self.lock().offline.insert(name.to_owned());
        self
    }

    /// The start of the session `name` fails from now on, as when podman
    /// cannot run the container: the call stays in [`Fake::calls`].
    ///
    /// ```
    /// use riff::runtime::{Fake, Runtime, SessionSpec};
    ///
    /// let spec = SessionSpec {
    ///     name: "s1".into(),
    ///     image: "i".into(),
    ///     home: "/h".into(),
    ///     workdir: "/w".into(),
    ///     binds: vec![],
    ///     env: vec![],
    ///     memory: "8g".into(),
    ///     cpus: 4,
    ///     command: vec![],
    /// };
    /// let fake = Fake::default().with_image("i");
    /// fake.refuse_start("s1");
    /// tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
    ///     assert!(fake.start_terminal(&spec, &[]).await.is_err());
    ///     assert!(!fake.running("s1").await?);
    ///     Ok::<(), anyhow::Error>(())
    /// })?;
    /// assert_eq!(fake.calls(), ["terminal s1"]);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn refuse_start(&self, name: &str) {
        self.lock().refused.insert(name.to_owned());
    }

    /// Each pull, start, stop, redraw, attach and exec, in order: for
    /// example `start riff-issue-12-1a2b3c4d`, `terminal riff-lead-o-r`
    /// or `exec riff-issue-12-1a2b3c4d claude --resume ID`.
    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    /// The spec of the session `name`, when it started.
    pub fn spec(&self, name: &str) -> Option<SessionSpec> {
        self.lock().sessions.get(name).map(|p| p.spec.clone())
    }

    /// The values of the host that the session `name` got.
    pub fn values(&self, name: &str) -> Vec<(String, String)> {
        self.lock()
            .sessions
            .get(name)
            .map(|p| p.values.clone())
            .unwrap_or_default()
    }

    /// The input that the session `name` got.
    pub fn input(&self, name: &str) -> String {
        self.lock()
            .sessions
            .get(name)
            .map(|p| String::from_utf8_lossy(&p.input).into_owned())
            .unwrap_or_default()
    }

    /// The exit of the session `name`, after its end.
    pub fn exit(&self, name: &str) -> Option<Exit> {
        self.lock().sessions.get(name).and_then(|p| p.exit)
    }

    fn lock(&self) -> MutexGuard<'_, Book> {
        lock(&self.book)
    }
}

fn lock(book: &Mutex<Book>) -> MutexGuard<'_, Book> {
    book.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Ends the live session `name` of `book` with `exit`. False when no
/// such session runs.
fn end(book: &mut Book, name: &str, exit: Exit) -> bool {
    let Some(played) = book.sessions.get_mut(name).filter(|p| p.exit.is_none()) else {
        return false;
    };
    played.exit = Some(exit);
    played.events = None;
    if let Some(end) = played.end.take() {
        let _ = end.send(exit);
    }
    true
}

impl Runtime for Fake {
    async fn has_image(&self, image: &str) -> Result<bool> {
        Ok(self.lock().local.contains(image))
    }

    async fn pull(&self, image: &str) -> Result<()> {
        let mut book = self.lock();
        book.calls.push(format!("pull {image}"));
        if !book.registry.contains(image) {
            bail!("the registry has no image {image}");
        }
        book.local.insert(image.to_owned());
        Ok(())
    }

    async fn output(&self, image: &str, command: &[&str]) -> Result<String> {
        let program = command.first().copied().unwrap_or_default();
        self.lock()
            .tools
            .get(&(image.to_owned(), program.to_owned()))
            .cloned()
            .ok_or_else(|| anyhow!("the image {image} has no program {program}"))
    }

    async fn start(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<Running> {
        let (read, end) = self.play(spec, values, "start")?;
        let name = spec.name.clone();
        Ok(Running {
            name: name.clone(),
            input: Box::new(FakeInput {
                book: self.book.clone(),
                name,
            }),
            events: Box::new(FakeEvents {
                read,
                left: Vec::new(),
            }),
            errors: Box::new(tokio::io::empty()),
            end,
        })
    }

    async fn stop(&self, name: &str) -> Result<()> {
        let mut book = self.lock();
        book.calls.push(format!("stop {name}"));
        if !end(&mut book, name, Exit { code: None }) {
            bail!("no session {name} runs");
        }
        Ok(())
    }

    async fn running(&self, name: &str) -> Result<bool> {
        let book = self.lock();
        if book.no_answer {
            bail!("podman does not answer");
        }
        Ok(book.sessions.get(name).is_some_and(|p| p.exit.is_none()))
    }

    async fn attach(&self, name: &str) -> Result<Exit> {
        let mut book = self.lock();
        book.calls.push(format!("attach {name}"));
        match book.sessions.get(name) {
            Some(played) if played.exit.is_none() => {
                if book.exit_in_attach {
                    end(&mut book, name, Exit::SUCCESS);
                }
                Ok(Exit::SUCCESS)
            }
            _ => bail!("no session {name} runs"),
        }
    }

    async fn redraw(&self, name: &str) -> Result<()> {
        let mut book = self.lock();
        book.calls.push(format!("redraw {name}"));
        match book.sessions.get(name) {
            Some(played) if played.exit.is_none() => Ok(()),
            _ => bail!("no session {name} runs"),
        }
    }

    /// A session with a terminal prints no event: a stop ends it.
    async fn start_terminal(&self, spec: &SessionSpec, values: &[(String, String)]) -> Result<End> {
        let (_, end) = self.play(spec, values, "terminal")?;
        Ok(end)
    }

    async fn exec(&self, name: &str, command: &[String]) -> Result<Exit> {
        let mut book = self.lock();
        book.calls
            .push(format!("exec {name} {}", command.join(" ")));
        match book.sessions.get(name) {
            Some(played) if played.exit.is_none() => Ok(Exit::SUCCESS),
            _ => bail!("no session {name} runs"),
        }
    }

    async fn links(&self, name: &str, path: &Path) -> Result<u64> {
        use std::os::unix::fs::MetadataExt;
        let book = self.lock();
        let Some(played) = book.sessions.get(name).filter(|p| p.exit.is_none()) else {
            bail!("no session {name} runs");
        };
        match played.files.get(path) {
            Some(file) => Ok(file.metadata()?.nlink()),
            None => bail!("the session {name} has no file {}", path.display()),
        }
    }

    async fn reaches(&self, name: &str) -> Result<bool> {
        let book = self.lock();
        if book.no_answer {
            bail!("podman does not answer");
        }
        if book.sessions.get(name).is_none_or(|p| p.exit.is_some()) {
            bail!("no session {name} runs");
        }
        Ok(!book.offline.contains(name))
    }

    async fn digest(&self, name: &str) -> Result<String> {
        let book = self.lock();
        if book.no_answer {
            bail!("podman does not answer");
        }
        let Some(played) = book.sessions.get(name).filter(|p| p.exit.is_none()) else {
            bail!("no session {name} runs");
        };
        match book.digests.get(name) {
            Some(digest) => Ok(digest.clone()),
            None => played
                .spec
                .image
                .split_once('@')
                .map(|(_, digest)| digest.to_owned())
                .ok_or_else(|| anyhow!("the image {} has no digest", played.spec.image)),
        }
    }
}

impl Fake {
    /// Starts the session of `spec` as the call `call`: its events and its
    /// end.
    fn play(
        &self,
        spec: &SessionSpec,
        values: &[(String, String)],
        call: &str,
    ) -> Result<(mpsc::UnboundedReceiver<Vec<u8>>, End)> {
        spec.check_values(values)?;
        let mut book = self.lock();
        book.calls.push(format!("{call} {}", spec.name));
        if book.refused.contains(&spec.name) {
            bail!("the fake refuses to start the session {}", spec.name);
        }
        if !book.local.contains(&spec.image) {
            bail!("this machine has no image {}", spec.image);
        }
        if book
            .sessions
            .get(&spec.name)
            .is_some_and(|p| p.exit.is_none())
        {
            bail!("the session {} runs already", spec.name);
        }
        let (events, read) = mpsc::unbounded_channel();
        if call == "start" {
            for line in &book.events {
                let _ = events.send(format!("{line}\n").into_bytes());
            }
        }
        let (ended, exit) = oneshot::channel();
        book.sessions.insert(
            spec.name.clone(),
            Played {
                spec: spec.clone(),
                values: values.to_vec(),
                input: Vec::new(),
                events: Some(events),
                end: Some(ended),
                exit: None,
                files: open_files(spec),
            },
        );
        let name = spec.name.clone();
        let end = End::new(async move {
            exit.await
                .map_err(|_| anyhow!("the fake session {name} has no end"))
        });
        Ok((read, end))
    }
}

/// Each file of the host that `spec` binds, open, by its path in the
/// session.
fn open_files(spec: &SessionSpec) -> BTreeMap<PathBuf, std::fs::File> {
    spec.binds
        .iter()
        .filter_map(|bind| match &bind.source {
            Source::Path(source) if source.is_file() => std::fs::File::open(source)
                .ok()
                .map(|file| (bind.target.clone(), file)),
            _ => None,
        })
        .collect()
}

/// The input of a session of a [`Fake`]. Its drop or its shutdown
/// closes it: the session ends with the exit 0.
struct FakeInput {
    book: Arc<Mutex<Book>>,
    name: String,
}

impl AsyncWrite for FakeInput {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut book = lock(&self.book);
        Poll::Ready(
            match book
                .sessions
                .get_mut(&self.name)
                .filter(|p| p.exit.is_none())
            {
                Some(played) => {
                    played.input.extend_from_slice(buf);
                    Ok(buf.len())
                }
                None => Err(io::ErrorKind::BrokenPipe.into()),
            },
        )
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        end(&mut lock(&self.book), &self.name, Exit::SUCCESS);
        Poll::Ready(Ok(()))
    }
}

impl Drop for FakeInput {
    fn drop(&mut self) {
        end(&mut lock(&self.book), &self.name, Exit::SUCCESS);
    }
}

/// The events of a session of a [`Fake`]: they end at the end of the
/// session.
struct FakeEvents {
    read: mpsc::UnboundedReceiver<Vec<u8>>,
    /// The part of the last line that the reader did not take yet.
    left: Vec<u8>,
}

impl AsyncRead for FakeEvents {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.left.is_empty() {
            match ready!(self.read.poll_recv(cx)) {
                Some(bytes) => self.left = bytes,
                None => return Poll::Ready(Ok(())),
            }
        }
        let n = buf.remaining().min(self.left.len());
        buf.put_slice(&self.left[..n]);
        self.left.drain(..n);
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    fn spec(name: &str) -> SessionSpec {
        SessionSpec {
            name: name.into(),
            image: "img".into(),
            home: "/home/mike".into(),
            workdir: "/data/clone".into(),
            binds: vec![],
            env: vec![Env::Host("GH_TOKEN".into())],
            memory: "8g".into(),
            cpus: 4,
            command: vec!["claude".into()],
        }
    }

    fn token() -> Vec<(String, String)> {
        vec![("GH_TOKEN".into(), "gho_x".into())]
    }

    #[tokio::test]
    async fn a_fake_session_prints_its_events_and_ends_when_its_input_closes() {
        let fake = Fake::default()
            .with_image("img")
            .with_events(["one", "two"]);
        let running = fake.start(&spec("s1"), &token()).await.unwrap();
        let Running {
            mut input,
            events,
            end,
            ..
        } = running;
        input.write_all(b"hello\n").await.unwrap();
        let mut lines = BufReader::new(events).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("one"));
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("two"));
        assert_eq!(fake.exit("s1"), None);
        input.shutdown().await.unwrap();
        // The end of the session ends its events.
        assert_eq!(lines.next_line().await.unwrap(), None);
        assert_eq!(end.wait().await.unwrap(), Exit::SUCCESS);
        assert_eq!(fake.input("s1"), "hello\n");
        assert_eq!(fake.values("s1"), token());
        assert_eq!(fake.spec("s1"), Some(spec("s1")));
        // The input of a session that ended takes no more.
        assert!(input.write_all(b"late\n").await.is_err());
    }

    #[tokio::test]
    async fn a_stop_ends_a_fake_session_with_no_exit_code_and_ends_its_events() {
        let fake = Fake::default().with_image("img").with_events(["one"]);
        let running = fake.start(&spec("s1"), &token()).await.unwrap();
        let mut events = String::new();
        fake.stop("s1").await.unwrap();
        // The line before the stop is still there.
        let mut read = running.events;
        read.read_to_string(&mut events).await.unwrap();
        assert_eq!(events, "one\n");
        assert_eq!(running.end.wait().await.unwrap(), Exit { code: None });
        // The drop of the input after the stop changes no exit.
        drop(running.input);
        assert_eq!(fake.exit("s1"), Some(Exit { code: None }));
        assert!(fake.attach("s1").await.is_err());
        assert_eq!(fake.calls(), ["start s1", "stop s1", "attach s1"]);
    }

    #[tokio::test]
    async fn a_fake_starts_no_session_with_no_image_or_with_a_wrong_value() {
        let fake = Fake::default().with_registry("img");
        let error = fake.start(&spec("s1"), &token()).await.err().unwrap();
        assert!(error.to_string().contains("no image img"), "{error}");
        assert!(!fake.has_image("img").await.unwrap());
        fake.pull("img").await.unwrap();
        assert!(fake.has_image("img").await.unwrap());
        assert!(fake.pull("other").await.is_err());
        let error = fake.start(&spec("s1"), &[]).await.err().unwrap();
        assert!(
            error.to_string().contains("needs a value for GH_TOKEN"),
            "{error}"
        );
        assert_eq!(fake.calls(), ["start s1", "pull img", "pull other"]);
    }

    #[tokio::test]
    async fn a_fake_attaches_to_a_live_session_and_starts_a_name_again_after_its_end() {
        let fake = Fake::default().with_image("img");
        let running = fake.start(&spec("s1"), &token()).await.unwrap();
        assert_eq!(fake.attach("s1").await.unwrap(), Exit::SUCCESS);
        drop(running.input);
        assert_eq!(running.end.wait().await.unwrap(), Exit::SUCCESS);
        let again = fake.start(&spec("s1"), &token()).await.unwrap();
        assert_eq!(again.name, "s1");
        assert_eq!(fake.exit("s1"), None);
    }

    #[tokio::test]
    async fn a_fake_session_with_a_terminal_takes_an_attach_and_an_exec_until_a_stop() {
        let fake = Fake::default()
            .with_image("img")
            .with_events(["not for a terminal"]);
        let end = fake.start_terminal(&spec("lead"), &token()).await.unwrap();
        fake.redraw("lead").await.unwrap();
        assert_eq!(fake.attach("lead").await.unwrap(), Exit::SUCCESS);
        let command = ["claude".to_owned(), "--resume".to_owned(), "s1".to_owned()];
        assert_eq!(fake.exec("lead", &command).await.unwrap(), Exit::SUCCESS);
        assert!(fake.start_terminal(&spec("lead"), &token()).await.is_err());
        fake.stop("lead").await.unwrap();
        assert_eq!(end.wait().await.unwrap(), Exit { code: None });
        assert!(fake.exec("lead", &command).await.is_err());
        assert!(fake.redraw("lead").await.is_err());
        assert_eq!(
            fake.calls(),
            [
                "terminal lead",
                "redraw lead",
                "attach lead",
                "exec lead claude --resume s1",
                "terminal lead",
                "stop lead",
                "exec lead claude --resume s1",
                "redraw lead",
            ]
        );
    }

    #[tokio::test]
    async fn a_fake_with_an_exit_in_attach_ends_the_session_in_the_attach() {
        let fake = Fake::default().with_image("img").with_exit_in_attach();
        let end = fake.start_terminal(&spec("lead"), &token()).await.unwrap();
        assert!(fake.running("lead").await.unwrap());
        assert_eq!(fake.attach("lead").await.unwrap(), Exit::SUCCESS);
        assert!(!fake.running("lead").await.unwrap());
        assert_eq!(end.wait().await.unwrap(), Exit::SUCCESS);
        assert!(fake.attach("lead").await.is_err());
    }

    #[test]
    fn a_spec_takes_only_the_values_that_it_names_by_host() {
        let mut spec = spec("s1");
        spec.env.push(Env::Value("USER".into(), "mike".into()));
        let value = |n: &str| (n.to_owned(), "v".to_owned());
        assert!(spec.check_values(&[value("GH_TOKEN")]).is_ok());
        // USER has its value in the spec: the host gives none.
        let error = spec
            .check_values(&[value("GH_TOKEN"), value("USER")])
            .unwrap_err();
        assert!(
            error.to_string().contains("takes no value for USER"),
            "{error}"
        );
    }
}
