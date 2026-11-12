//! `riff cloud`: make and run riff-server instances on Cloud Run.
//!
//! # Design
//!
//! Each command is a thin wrapper around the `gcloud` of the machine,
//! as [`crate::pr`] is around `gh` (01M4262DQ9RNFNJ07CRTSGEAM1). A
//! person signs in to `gcloud` first. riff keeps no cloud credential.
//!
//! | Command | Does |
//! |---|---|
//! | `riff cloud create NAME --project P --region R` | Writes the settings of a new instance, then makes each resource that is missing ([`create`]) |
//! | `riff cloud signin NAME` | Shows the console steps of the sign-in client, then stores the secret in Secret Manager and the client ID in the settings ([`signin`]) |
//! | `riff cloud deploy NAME [TAG]` | Deploys the image of a release tag, or builds this tree with Cloud Build ([`deploy`]) |
//! | `riff cloud list` | Each instance: URL, release, ready, paused |
//! | `riff cloud status NAME` | The same for one instance, with its revision and memory, and each secret with the roles of the run account on it ([`secret_states`]) |
//! | `riff cloud log NAME [--errors]` | The log lines of the service |
//! | `riff cloud delete NAME [--with-state]` | Deletes the service, and the bucket with `--with-state` ([`delete`]) |
//!
//! ## Who runs it
//!
//! The writes ([`WRITES`]) refuse before each call of `gcloud` in a
//! worker and in an agent session that is not the lead, with the check
//! `lead_only` (01M558C4VZ2PSMEYPAZBRYW1VV). This check is
//! the guard. A person in a plain terminal and the lead run them. Each
//! session runs the reads: `list`, `status` and `log`.
//!
//! ## Settings
//!
//! Each instance has one settings file `NAME.env` of `KEY=VALUE` lines
//! ([`Settings`]). It names its own service, bucket, accounts, secret
//! and sign-in client, so one instance holds no data of another, and
//! each bucket has its own riff ID (01M4262DSKVWP4064FKSMAQACZ). The
//! files are in `deploy/cloud/` of the repository of this directory,
//! when it has that folder. Else they are in `cloud/` beside the riff
//! settings of the machine ([`dir`]). The riff repository holds two:
//! `shared.env`, the shared riff, and `stage.env`, the stage.
//!
//! `CLOUD_GITHUB_SECRET` names the secret of the keys of the GitHub
//! Apps. It has no default: settings with no value for it are an error
//! that names the line to add (01M6YC5XGQR7V3KC68WW8NE1PD). `riff cloud create`
//! writes `NAME-github-apps` in the file of a new instance.
//!
//! ## A clone that is behind
//!
//! `riff cloud create NAME` reads or writes the settings of a clone only
//! when the branch of the clone is not behind its upstream ([`behind`],
//! 01M6YC5XGQR7V3KC68WW8NE1PD). Else it stops before each call of `gcloud`, makes
//! nothing, and names the fix `git pull`. A branch with no upstream, a
//! detached `HEAD`, and a folder outside a clone have no check.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff cloud create NAME"] --> U{"branch has an upstream?"}
//!     U -- no --> R["read or write NAME.env"]
//!     U -- yes --> F["git fetch, quiet, best effort"]
//!     F --> B{"HEAD..upstream has a commit?"}
//!     B -- no --> R
//!     B -- yes --> X["stop: run git pull<br/>no call of gcloud"]
//!     R --> G{"CLOUD_GITHUB_SECRET?"}
//!     G -- empty or none --> Y["stop: add the line"]
//!     G -- yes --> M["make each missing resource"]
//! ```
//!
//! The shared riff and the stage take the same code path. Only the
//! setting `CLOUD_CONFIRM=true` differs in the flow: then `deploy` asks
//! for the name, as `delete` always does ([`confirm`],
//! 01M4262DVY8QCSZS61VDQ61SB3). With no terminal, the flag
//! `--confirm NAME` gives the name.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff cloud create"] --> F["NAME.env"]
//!     F --> S["riff cloud signin"]
//!     S --> D["riff cloud deploy"]
//!     D --> R["Cloud Run service<br/>bucket, secret, accounts"]
//!     L["list, status, log"] --> R
//!     X["riff cloud delete"] --> R
//! ```
//!
//! ```
//! use riff::cloud::{Settings, set_line};
//!
//! let text = "# The stage\nCLOUD_PROJECT=p\nRIFF_OIDC_CLIENT_ID=\n";
//! let s = Settings::parse("stage", text).unwrap_err();
//! assert!(s.to_string().contains("have no CLOUD_PROJECT_NUMBER"));
//!
//! // No default for the secret of the GitHub Apps (01M6YC5XGQR7V3KC68WW8NE1PD).
//! let mut new = Settings::new("stage", "p", "1", "r").text();
//! assert!(Settings::parse("stage", &new).is_ok());
//! new = new.replace("CLOUD_GITHUB_SECRET=stage-github-apps\n", "");
//! let e = Settings::parse("stage", &new).unwrap_err().to_string();
//! assert!(e.contains("Add the line CLOUD_GITHUB_SECRET=SECRET to stage.env"), "{e}");
//! assert_eq!(
//!     set_line(text, "RIFF_OIDC_CLIENT_ID", "x.apps.googleusercontent.com"),
//!     "# The stage\nCLOUD_PROJECT=p\nRIFF_OIDC_CLIENT_ID=x.apps.googleusercontent.com\n",
//! );
//! ```

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::text;

/// The lifecycle rules of a bucket: delete an older version after 7
/// days, and each thread object 30 days after its last change.
pub const LIFECYCLE: &str = include_str!("../../../deploy/lifecycle.json");

/// The alert on each error of the service, before [`alert`] names the
/// service in it.
pub const ALERT: &str = include_str!("../../../deploy/alert.json");

/// The subcommands of `riff cloud` that change an instance: on Cloud
/// Run, or on its riff, as `smoke` does with its sign-in, its session and
/// its note (01M558C4VZ2PSMEYPAZBRYW1VV). Only a person in a plain
/// terminal and the lead run them.
///
/// ```
/// assert_eq!(riff::cloud::WRITES, ["create", "signin", "deploy", "smoke", "delete"]);
/// ```
pub const WRITES: [&str; 5] = ["create", "signin", "deploy", "smoke", "delete"];

/// The APIs that an instance uses.
const APIS: [&str; 10] = [
    "secretmanager.googleapis.com",
    "storage.googleapis.com",
    "iam.googleapis.com",
    "run.googleapis.com",
    "cloudbuild.googleapis.com",
    "artifactregistry.googleapis.com",
    "iamcredentials.googleapis.com",
    "sts.googleapis.com",
    "logging.googleapis.com",
    "monitoring.googleapis.com",
];

/// The `gcloud` of the machine.
pub struct Gcloud {
    program: PathBuf,
    /// The wait between two tries of a role ([`Gcloud::bind`]).
    wait: Duration,
}

impl Default for Gcloud {
    fn default() -> Self {
        Self {
            program: "gcloud".into(),
            wait: Duration::from_secs(10),
        }
    }
}

impl Gcloud {
    /// The `gcloud` at `program`, with no wait between tries, for tests.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            wait: Duration::ZERO,
        }
    }

    /// Runs `gcloud ARGS` with `input` on stdin, and returns its stdout.
    /// It fails with the stderr of `gcloud` when `gcloud` fails.
    pub fn run(&self, args: &[String], input: Option<&str>) -> Result<String> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("cannot run {}", self.program.display()))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(input.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(Failed {
                args: args.to_vec(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
            .into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The stdout of `gcloud ARGS`, a describe call, or `None` when
    /// `gcloud` replies that the resource does not exist. Each other
    /// failure is an error, for example an ended sign-in
    /// (01M4382RKERWAPKBRY9W8F2GSA).
    pub fn find(&self, args: &[String]) -> Result<Option<String>> {
        match self.run(args, None) {
            Ok(out) => Ok(Some(out)),
            Err(e) => match e.downcast_ref::<Failed>() {
                Some(failed) if failed.not_found() => Ok(None),
                _ => Err(e),
            },
        }
    }

    /// True when `gcloud ARGS`, a describe call, finds the resource
    /// ([`Gcloud::find`]).
    pub fn exists(&self, args: &[String]) -> Result<bool> {
        Ok(self.find(args)?.is_some())
    }

    /// Runs `gcloud ARGS` with the terminal of riff: the person sees its
    /// output and answers its questions.
    pub fn show(&self, args: &[String]) -> Result<()> {
        let status = Command::new(&self.program)
            .args(args)
            .status()
            .with_context(|| format!("cannot run {}", self.program.display()))?;
        if !status.success() {
            bail!("gcloud {} failed", args.join(" "));
        }
        Ok(())
    }

    /// Runs `gcloud ARGS` up to 6 times, with a wait between two tries.
    /// IAM needs some seconds before it knows a new service account.
    pub fn bind(&self, args: &[String]) -> Result<()> {
        for _ in 0..5 {
            if self.run(args, None).is_ok() {
                return Ok(());
            }
            std::thread::sleep(self.wait);
        }
        self.run(args, None).map(drop)
    }
}

/// A call of `gcloud` that failed: its arguments and its stderr. It
/// shows as one line ([`text::gcloud_failed`]).
///
/// ```
/// use riff::cloud::Failed;
///
/// let args = ["run", "services", "describe", "riff-stage"].map(String::from).to_vec();
/// let gone = Failed::new(args.clone(), "ERROR: (gcloud.run.services.describe) Cannot find service [riff-stage].\n");
/// assert!(gone.not_found());
/// assert_eq!(gone.to_string(), "gcloud run services describe: Cannot find service [riff-stage].");
///
/// let ended = Failed::new(
///     args,
///     "ERROR: (gcloud.run.services.describe) There was a problem refreshing your current auth tokens: \
///      Reauthentication failed. cannot prompt during non-interactive execution.\n\
///      Please run:\n\n  $ gcloud auth login\n\nto obtain new credentials.\n",
/// );
/// assert!(!ended.not_found());
/// assert_eq!(ended.to_string(), "gcloud: the sign-in ended: run gcloud auth login");
///
/// // Cloud Run says "may not exist" when the account has no permission.
/// let denied = Failed::new(
///     ["run", "services", "describe", "riff-server"].map(String::from).to_vec(),
///     "ERROR: (gcloud.run.services.describe) PERMISSION_DENIED: Permission 'run.services.get' \
///      denied on resource 'namespaces/p/services/riff-server' (or resource may not exist).\n",
/// );
/// assert!(denied.denied());
/// assert!(!denied.not_found());
/// assert!(denied.to_string().starts_with("gcloud run services describe: PERMISSION_DENIED: "));
/// ```
#[derive(Debug)]
pub struct Failed {
    args: Vec<String>,
    stderr: String,
}

impl Failed {
    /// The failure of `gcloud ARGS` with `stderr`.
    pub fn new(args: Vec<String>, stderr: &str) -> Self {
        Self {
            args,
            stderr: stderr.to_owned(),
        }
    }

    /// True when `gcloud` needs a new sign-in.
    pub fn signin_ended(&self) -> bool {
        let e = &self.stderr;
        e.contains("gcloud auth login")
            || e.contains("Reauthentication failed")
            || e.contains("refreshing your current auth tokens")
            || e.contains("do not currently have an active account")
    }

    /// True when the account of `gcloud` has no permission for the
    /// call.
    pub fn denied(&self) -> bool {
        let e = self.stderr.to_lowercase();
        e.contains("permission_denied") || e.contains("permission denied")
    }

    /// True when `gcloud` replies that the resource does not exist. An
    /// ended sign-in and a refused permission are never this, also when
    /// the reply says that the resource "may not exist".
    pub fn not_found(&self) -> bool {
        if self.signin_ended() || self.denied() {
            return false;
        }
        let e = self.stderr.to_lowercase();
        ["cannot find", "not_found", "not found", "does not exist"]
            .iter()
            .any(|word| e.contains(word))
    }

    /// The command of the call: the arguments before the first flag.
    fn command(&self) -> String {
        self.args
            .iter()
            .take_while(|a| !a.starts_with('-'))
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.signin_ended() {
            return f.write_str(text::GCLOUD_SIGNIN_ENDED);
        }
        f.write_str(&text::gcloud_failed(&self.command(), &self.stderr))
    }
}

impl std::error::Error for Failed {}

/// `ARGS` as owned strings.
fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

/// The settings of one instance: the `KEY=VALUE` lines of `NAME.env`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The name of the instance: the file name with no `.env`.
    pub name: String,
    pub project: String,
    pub project_number: String,
    pub region: String,
    /// The name of the secret of the sign-in client in Secret Manager.
    pub secret_name: String,
    /// The name of the secret of the keys of the GitHub Apps in Secret
    /// Manager (01M6WG52ZEVRSQ463FTDS94M54): `CLOUD_GITHUB_SECRET` of the
    /// settings. It has no default ([`Settings::parse`],
    /// 01M6YC5XGQR7V3KC68WW8NE1PD).
    pub github_secret: String,
    /// The bucket that holds the state of riff-server.
    pub bucket: String,
    /// The Cloud Run service.
    pub service: String,
    /// The domain of the service, or empty.
    pub domain: String,
    /// The memory of the service, for example `1Gi`.
    pub memory: String,
    /// The least number of instances: 1, or 0 for a riff that scales
    /// to zero between calls (01M496JT648QS9WTE1QVHAJEE5).
    pub min_instances: u32,
    /// The alert on each error in the log, or empty for no alert.
    pub alert: String,
    /// The channel that sends the alert to the email of `RIFF_OWNER`.
    pub alert_channel: String,
    /// The public URL of the service.
    pub url: String,
    /// The account that riff-server runs as.
    pub run_account: String,
    /// The account that Cloud Build builds the image as.
    pub build_account: String,
    /// The account that CI deploys as, or empty for no CI deploy.
    pub deploy_account: String,
    /// The account that the CI job `Check the deploy` reads the log as,
    /// or empty for none (01M76SSFX0QGPA4MTXV39JV5P3).
    pub check_account: String,
    /// The GitHub repository whose CI deploys, or empty.
    pub github_repo: String,
    /// The GitHub environment of the CI job that deploys, for example
    /// `production`, or empty for no CI deploy. Only a job in this
    /// environment signs in as the deploy account
    /// (01M3NJAZAQ3AKMAM0EGM7R3S89).
    pub github_environment: String,
    /// The image repository of the project.
    pub repository: String,
    /// The client ID of the sign-in client, or empty before `signin`.
    pub client_id: String,
    /// True when `deploy` asks for the name ([`confirm`]).
    pub confirm: bool,
}

/// The roles of the run account on the secret of the sign-in client: it
/// only reads it (R134).
pub const SIGNIN_SECRET_ROLES: &[&str] = &["roles/secretmanager.secretAccessor"];

/// The roles of the run account on the secret of the GitHub Apps: it
/// reads it and adds its versions (01M6WG52ZEVRSQ463FTDS94M54).
pub const GITHUB_SECRET_ROLES: &[&str] = &[
    "roles/secretmanager.secretAccessor",
    "roles/secretmanager.secretVersionAdder",
];

/// A secret of an instance in Secret Manager ([`Settings::secrets`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Secret<'a> {
    /// What the secret holds, as `riff cloud status` names it.
    pub label: &'static str,
    /// The name of the secret.
    pub name: &'a str,
    /// The roles that the run account needs on the secret.
    pub roles: &'static [&'static str],
}

/// The keys that must have a value. `CLOUD_GITHUB_SECRET` must have one
/// too: its error names the line to add ([`Settings::parse`]).
const REQUIRED: [&str; 10] = [
    "CLOUD_PROJECT",
    "CLOUD_PROJECT_NUMBER",
    "CLOUD_REGION",
    "CLOUD_SECRET",
    "CLOUD_BUCKET",
    "CLOUD_SERVICE",
    "CLOUD_MEMORY",
    "CLOUD_URL",
    "CLOUD_RUN_ACCOUNT",
    "CLOUD_BUILD_ACCOUNT",
];

impl Settings {
    /// The settings of the instance `name` from the text of its file
    /// `NAME.env`. It fails when a required key has no value. With no
    /// value of `CLOUD_GITHUB_SECRET`, the error names the line to add
    /// (01M6YC5XGQR7V3KC68WW8NE1PD).
    ///
    /// ```
    /// use riff::cloud::Settings;
    ///
    /// let text = Settings::new("team", "acme", "1", "r").text();
    /// assert_eq!(Settings::parse("team", &text)?.github_secret, "team-github-apps");
    /// for gone in ["", "CLOUD_GITHUB_SECRET=\n"] {
    ///     let text = text.replace("CLOUD_GITHUB_SECRET=team-github-apps\n", gone);
    ///     let e = Settings::parse("team", &text).unwrap_err().to_string();
    ///     assert_eq!(e, riff::text::cloud_no_github_secret("team", "team.env".as_ref()));
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(name: &str, text: &str) -> Result<Self> {
        Self::parse_at(name, text, Path::new(&format!("{name}.env")))
    }

    /// [`Settings::parse`] of the text of the file `path`: an error
    /// names that file.
    fn parse_at(name: &str, text: &str, path: &Path) -> Result<Self> {
        let values = values(text);
        for key in REQUIRED {
            if values.get(key).is_none_or(|v| v.is_empty()) {
                bail!("the cloud settings {name} have no {key}");
            }
        }
        let get = |key: &str| values.get(key).cloned().unwrap_or_default();
        let github_secret = get("CLOUD_GITHUB_SECRET");
        if github_secret.is_empty() {
            bail!(text::cloud_no_github_secret(name, path));
        }
        Ok(Self {
            name: name.to_owned(),
            project: get("CLOUD_PROJECT"),
            project_number: get("CLOUD_PROJECT_NUMBER"),
            region: get("CLOUD_REGION"),
            secret_name: get("CLOUD_SECRET"),
            github_secret,
            bucket: get("CLOUD_BUCKET"),
            service: get("CLOUD_SERVICE"),
            domain: get("CLOUD_DOMAIN"),
            memory: get("CLOUD_MEMORY"),
            min_instances: match get("CLOUD_MIN_INSTANCES").as_str() {
                "" => 1,
                n => n.parse().ok().filter(|n| *n <= 1).with_context(|| {
                    format!("the cloud settings {name}: CLOUD_MIN_INSTANCES is 0 or 1, not {n}")
                })?,
            },
            alert: get("CLOUD_ALERT"),
            alert_channel: get("CLOUD_ALERT_CHANNEL"),
            url: get("CLOUD_URL"),
            run_account: get("CLOUD_RUN_ACCOUNT"),
            build_account: get("CLOUD_BUILD_ACCOUNT"),
            deploy_account: get("CLOUD_DEPLOY_ACCOUNT"),
            check_account: get("CLOUD_CHECK_ACCOUNT"),
            github_repo: get("CLOUD_GITHUB_REPO"),
            github_environment: get("CLOUD_GITHUB_ENVIRONMENT"),
            repository: get("CLOUD_REPOSITORY"),
            client_id: get("RIFF_OIDC_CLIENT_ID"),
            confirm: get("CLOUD_CONFIRM") == "true",
        })
    }

    /// The settings of a new instance `name` in `project` (with the
    /// number `number`) and `region`. Each resource takes its name from
    /// the instance, so two instances share no resource. The secret of
    /// the GitHub Apps is `NAME-github-apps`: [`Settings::text`] writes
    /// it in the file.
    ///
    /// ```
    /// let s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
    /// assert_eq!(s.service, "mine");
    /// assert_eq!(s.bucket, "acme-mine-state");
    /// assert_eq!(s.run_account, "mine-server");
    /// assert_eq!(s.url, "https://mine-123.europe-west1.run.app");
    /// let text = s.text();
    /// assert!(text.contains("\nCLOUD_GITHUB_SECRET=mine-github-apps\n"), "{text}");
    /// assert_eq!(riff::cloud::Settings::parse("mine", &text).unwrap(), s);
    /// ```
    pub fn new(name: &str, project: &str, number: &str, region: &str) -> Self {
        Self {
            name: name.to_owned(),
            project: project.to_owned(),
            project_number: number.to_owned(),
            region: region.to_owned(),
            secret_name: format!("{name}-oidc-client-secret"),
            github_secret: format!("{name}-github-apps"),
            bucket: format!("{project}-{name}-state"),
            service: name.to_owned(),
            domain: String::new(),
            memory: "1Gi".into(),
            min_instances: 1,
            alert: String::new(),
            alert_channel: String::new(),
            url: format!("https://{name}-{number}.{region}.run.app"),
            run_account: format!("{name}-server"),
            build_account: format!("{name}-build"),
            deploy_account: String::new(),
            check_account: String::new(),
            github_repo: String::new(),
            github_environment: String::new(),
            repository: "riff".into(),
            client_id: String::new(),
            confirm: false,
        }
    }

    /// The text of the settings file.
    pub fn text(&self) -> String {
        let mut text = format!(
            "# The cloud settings of the riff instance {name}. `riff cloud`\n\
             # reads them. Put no account data here.\n",
            name = self.name
        );
        for (key, value) in self.pairs() {
            text.push_str(&format!("{key}={value}\n"));
        }
        text
    }

    /// Each setting as its key and its value.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("CLOUD_PROJECT", self.project.clone()),
            ("CLOUD_PROJECT_NUMBER", self.project_number.clone()),
            ("CLOUD_REGION", self.region.clone()),
            ("CLOUD_SECRET", self.secret_name.clone()),
            ("CLOUD_GITHUB_SECRET", self.github_secret.clone()),
            ("CLOUD_BUCKET", self.bucket.clone()),
            ("CLOUD_SERVICE", self.service.clone()),
            ("CLOUD_DOMAIN", self.domain.clone()),
            ("CLOUD_MEMORY", self.memory.clone()),
            ("CLOUD_MIN_INSTANCES", self.min_instances.to_string()),
            ("CLOUD_ALERT", self.alert.clone()),
            ("CLOUD_ALERT_CHANNEL", self.alert_channel.clone()),
            ("CLOUD_URL", self.url.clone()),
            ("CLOUD_RUN_ACCOUNT", self.run_account.clone()),
            ("CLOUD_BUILD_ACCOUNT", self.build_account.clone()),
            ("CLOUD_DEPLOY_ACCOUNT", self.deploy_account.clone()),
            ("CLOUD_CHECK_ACCOUNT", self.check_account.clone()),
            ("CLOUD_GITHUB_REPO", self.github_repo.clone()),
            ("CLOUD_GITHUB_ENVIRONMENT", self.github_environment.clone()),
            ("CLOUD_REPOSITORY", self.repository.clone()),
            ("CLOUD_CONFIRM", self.confirm.to_string()),
            ("RIFF_OIDC_CLIENT_ID", self.client_id.clone()),
        ]
    }

    /// The email of the service account `name` of the project.
    pub fn account(&self, name: &str) -> String {
        format!("{name}@{}.iam.gserviceaccount.com", self.project)
    }

    /// The two secrets of the instance in Secret Manager: of the sign-in
    /// client, and of the keys of the GitHub Apps.
    ///
    /// ```
    /// use riff::cloud::{GITHUB_SECRET_ROLES, SIGNIN_SECRET_ROLES, Settings};
    ///
    /// let s = Settings::new("mine", "acme", "123", "europe-west1");
    /// let [signin, github] = s.secrets();
    /// assert_eq!((signin.label, signin.name), ("sign-in secret", "mine-oidc-client-secret"));
    /// assert_eq!(signin.roles, SIGNIN_SECRET_ROLES);
    /// assert_eq!((github.label, github.name), ("github secret", "mine-github-apps"));
    /// assert_eq!(github.roles, GITHUB_SECRET_ROLES);
    /// ```
    pub fn secrets(&self) -> [Secret<'_>; 2] {
        [
            Secret {
                label: "sign-in secret",
                name: &self.secret_name,
                roles: SIGNIN_SECRET_ROLES,
            },
            Secret {
                label: "github secret",
                name: &self.github_secret,
                roles: GITHUB_SECRET_ROLES,
            },
        ]
    }

    /// The image of the release `tag` in the image repository.
    ///
    /// ```
    /// let s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
    /// assert_eq!(s.image("v1.0.0"), "europe-west1-docker.pkg.dev/acme/riff/riff-server:v1.0.0");
    /// ```
    pub fn image(&self, tag: &str) -> String {
        format!(
            "{}-docker.pkg.dev/{}/{}/riff-server:{tag}",
            self.region, self.project, self.repository
        )
    }

    /// `--project PROJECT`.
    fn project(&self) -> Vec<String> {
        args(&["--project", &self.project])
    }

    /// `--project PROJECT --region REGION`.
    fn place(&self) -> Vec<String> {
        args(&["--project", &self.project, "--region", &self.region])
    }
}

/// The values of the `KEY=VALUE` lines of `text`. A line that starts
/// with `#` is a comment.
pub fn values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
        .collect()
}

/// `text` with the line of `key` set to `value`. It keeps each other
/// line, and adds the line at the end when `text` has no line of `key`.
pub fn set_line(text: &str, key: &str, value: &str) -> String {
    let start = format!("{key}=");
    let mut found = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with(&start) {
            found = true;
            out.push_str(&format!("{key}={value}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(&format!("{key}={value}\n"));
    }
    out
}

/// True when `name` can name an instance: a lowercase letter, then
/// lowercase letters, digits and dashes, 20 characters at most. So the
/// account `NAME-server` fits the limit of 30 characters.
///
/// ```
/// use riff::cloud::valid_name;
/// assert!(valid_name("stage") && valid_name("team-2"));
/// assert!(!valid_name("v1.0.0") && !valid_name("Stage") && !valid_name("") && !valid_name("-x"));
/// assert!(!valid_name("a-name-that-is-too-long"));
/// ```
pub fn valid_name(name: &str) -> bool {
    name.len() <= 20
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The folder of the settings files: `deploy/cloud` of the repository
/// of `here`, when it has that folder, else `cloud` beside the riff
/// settings of the machine.
pub fn dir(here: &Path) -> Result<PathBuf> {
    if let Some(dir) = top(here).map(|t| t.join("deploy").join("cloud"))
        && dir.is_dir()
    {
        return Ok(dir);
    }
    let settings = crate::settings::path()?;
    Ok(settings.parent().unwrap_or(Path::new(".")).join("cloud"))
}

/// The top of the git work tree of `here`, or `None` outside one.
pub fn top(here: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(here)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().into())
}

/// The settings file of the instance `name` in `dir`.
pub fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.env"))
}

/// The settings of the instance `name` in `dir`.
pub fn load(dir: &Path, name: &str) -> Result<Settings> {
    if !valid_name(name) {
        bail!(text::cloud_bad_name(name));
    }
    let path = file(dir, name);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!(text::cloud_no_settings(name, &path)))?;
    Settings::parse_at(name, &text, &path)
}

/// The branch of a clone that is behind its upstream ([`behind`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Behind {
    /// The top of the work tree of the clone.
    pub top: PathBuf,
    /// The upstream of the branch, for example `origin/main`.
    pub upstream: String,
    /// The number of commits of the upstream that the branch does not
    /// have.
    pub commits: u64,
}

/// The longest wait for the fetch of [`behind`].
const FETCH_WAIT: Duration = Duration::from_secs(30);

/// The clone of the folder `dir` when its branch is behind its upstream
/// (01M6YC5XGQR7V3KC68WW8NE1PD), else `None`. It fetches the remote of the
/// upstream first, with no output and no prompt, for at most 30
/// seconds. When the fetch fails, it compares the branch with the
/// upstream ref that the clone has. A folder outside a clone, a detached
/// `HEAD` and a branch with no upstream give `None`.
///
/// ```
/// use riff::cloud::behind;
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = std::process::Command::new("git")
///         .args(["-c", "user.name=a", "-c", "user.email=a@b.c", "-c", "commit.gpgsign=false", "-C"])
///         .arg(dir)
///         .args(args)
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let (origin, clone) = (dir.path().join("origin"), dir.path().join("clone"));
/// git(dir.path(), &["init", "-q", "-b", "main", "origin"]);
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "first"]);
/// git(dir.path(), &["clone", "-q", "origin", "clone"]);
/// assert_eq!(behind(&clone), None);
///
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
/// let late = behind(&clone).unwrap();
/// assert_eq!((late.upstream.as_str(), late.commits), ("origin/main", 1));
/// assert_eq!(late.top.canonicalize()?, clone.canonicalize()?);
///
/// git(&clone, &["pull", "-q", "--ff-only"]);
/// assert_eq!(behind(&clone), None);
/// // The origin is the clone of nothing: it has no upstream.
/// assert_eq!(behind(&origin), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn behind(dir: &Path) -> Option<Behind> {
    use crate::session_image::git;
    if !dir.is_dir() {
        return None;
    }
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    // This fails on a detached HEAD, and on a branch with no upstream.
    let upstream = git(
        dir,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()?;
    // With no remote named, git fetches the remote of the upstream.
    fetch(dir);
    let commits = git(dir, &["rev-list", "--count", "HEAD..@{upstream}"])
        .ok()?
        .parse()
        .ok()?;
    (commits > 0).then(|| Behind {
        top: top.into(),
        upstream,
        commits,
    })
}

/// Runs `git fetch --quiet` in `dir` with no output and no prompt, for
/// at most [`FETCH_WAIT`]. A fetch that fails changes nothing.
fn fetch(dir: &Path) {
    use std::os::unix::process::CommandExt;
    let child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["fetch", "--quiet"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // A group of its own: the stop at the end of the wait also stops
        // each process that git started, for example ssh.
        .process_group(0)
        .spawn();
    let Ok(mut child) = child else {
        return;
    };
    let end = std::time::Instant::now() + FETCH_WAIT;
    while matches!(child.try_wait(), Ok(None)) {
        if std::time::Instant::now() >= end {
            if let Ok(group) = i32::try_from(child.id()) {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(group),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The settings of each instance in `dir`, by name.
pub fn all(dir: &Path) -> Result<Vec<Settings>> {
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".env"))
                    .map(str::to_owned)
            })
            .filter(|n| valid_name(n))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names.iter().map(|n| load(dir, n)).collect()
}

/// Checks the name that a person gives before a change that cannot be
/// undone: `given` from `--confirm`, or a line that the person types.
/// With no terminal and no `--confirm`, it refuses.
pub fn confirm(name: &str, given: Option<&str>, what: &str) -> Result<()> {
    let typed = match given {
        Some(given) => given.to_owned(),
        None if std::io::stdin().is_terminal() => {
            eprint!("{}", text::cloud_type_name(name, what));
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            line.trim().to_owned()
        }
        None => bail!(text::cloud_confirm_flag(name, what)),
    };
    if typed != name {
        bail!(text::cloud_wrong_name(name, &typed));
    }
    Ok(())
}

/// Writes the settings of a new instance to `dir`, or reads the
/// settings that are there. `project` and `region` must match the
/// settings that are there. When `dir` is in a clone whose branch is
/// behind its upstream ([`behind`]), it stops before each call of
/// `gcloud` and writes nothing: the error names the fix `git pull`
/// (01M6YC5XGQR7V3KC68WW8NE1PD).
pub fn settings_for_create(
    gcloud: &Gcloud,
    dir: &Path,
    name: &str,
    project: Option<&str>,
    region: Option<&str>,
) -> Result<Settings> {
    if !valid_name(name) {
        bail!(text::cloud_bad_name(name));
    }
    if let Some(late) = behind(dir) {
        bail!(text::cloud_clone_behind(name, &late));
    }
    let path = file(dir, name);
    if path.exists() {
        let s = load(dir, name)?;
        for (flag, given, have) in [
            ("--project", project, &s.project),
            ("--region", region, &s.region),
        ] {
            if let Some(given) = given
                && given != have
            {
                bail!(text::cloud_settings_differ(name, flag, given, have));
            }
        }
        println!("{}", text::cloud_settings_read(&path));
        return Ok(s);
    }
    let (Some(project), Some(region)) = (project, region) else {
        bail!(text::cloud_create_needs(name));
    };
    let number = gcloud
        .run(
            &args(&[
                "projects",
                "describe",
                project,
                "--format",
                "value(projectNumber)",
            ]),
            None,
        )
        .map_err(|_| anyhow::anyhow!(text::cloud_no_project(project)))?;
    let s = Settings::new(name, project, number.trim(), region);
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    std::fs::write(&path, s.text()).with_context(|| format!("cannot write {}", path.display()))?;
    println!("{}", text::cloud_settings_written(&path));
    Ok(s)
}

/// The alert of `s`: [`ALERT`] with the name of the alert and the
/// service of `s`.
///
/// ```
/// let mut s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
/// s.alert = "mine-errors".into();
/// let alert: serde_json::Value = serde_json::from_str(&riff::cloud::alert(&s)).unwrap();
/// assert_eq!(alert["displayName"], "mine-errors");
/// let filter = alert["conditions"][0]["conditionMatchedLog"]["filter"].as_str().unwrap();
/// assert!(filter.contains(r#"service_name="mine""#), "{filter}");
/// ```
pub fn alert(s: &Settings) -> String {
    let mut alert: serde_json::Value =
        serde_json::from_str(ALERT).expect("deploy/alert.json is JSON");
    alert["displayName"] = s.alert.clone().into();
    let filter = format!(
        "resource.type=\"cloud_run_revision\" AND resource.labels.service_name=\"{}\" AND severity>=ERROR",
        s.service
    );
    alert["conditions"][0]["conditionMatchedLog"]["filter"] = filter.into();
    alert.to_string()
}

/// Writes `text` to a new temporary file for a `--...-file` flag of
/// `gcloud`.
fn temp_file(text: &str) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(text.as_bytes())?;
    Ok(file)
}

/// Makes each resource of the instance `s` that is missing. It checks
/// each resource first, so it can run again. `owner` is the email that
/// gets the alert.
pub fn create(gcloud: &Gcloud, s: &Settings, owner: Option<&str>) -> Result<()> {
    let project = s.project();
    let with = |parts: &[&str], tail: &[String]| {
        let mut a = args(parts);
        a.extend_from_slice(tail);
        a
    };
    if !gcloud.exists(&args(&["projects", "describe", &s.project]))? {
        bail!(text::cloud_no_project(&s.project));
    }
    let billing = gcloud.run(
        &args(&[
            "billing",
            "projects",
            "describe",
            &s.project,
            "--format",
            "value(billingEnabled)",
        ]),
        None,
    )?;
    if billing.trim() != "True" {
        bail!(text::cloud_no_billing(&s.project));
    }
    println!("Project {}: exists, with billing.", s.project);

    let mut enable = args(&["services", "enable"]);
    enable.extend(APIS.iter().map(|a| (*a).to_owned()));
    enable.extend_from_slice(&project);
    gcloud.run(&enable, None)?;
    println!("APIs: on.");

    // The secret of the sign-in client, and the secret of the keys of the
    // GitHub Apps (01M6WG52ZEVRSQ463FTDS94M54). riff-server adds each
    // version of the second one itself.
    for Secret { name, .. } in s.secrets() {
        if gcloud.exists(&with(&["secrets", "describe", name], &project))? {
            println!("Secret {name}: exists.");
        } else {
            println!("Secret {name}: making it.");
            gcloud.run(
                &with(
                    &[
                        "secrets",
                        "create",
                        name,
                        "--replication-policy",
                        "automatic",
                    ],
                    &project,
                ),
                None,
            )?;
        }
    }

    let bucket = format!("gs://{}", s.bucket);
    if gcloud.exists(&with(
        &["storage", "buckets", "describe", &bucket],
        &project,
    ))? {
        println!("Bucket {}: exists.", s.bucket);
    } else {
        println!("Bucket {}: making it.", s.bucket);
        gcloud.run(
            &with(
                &[
                    "storage",
                    "buckets",
                    "create",
                    &bucket,
                    "--location",
                    &s.region,
                    "--default-storage-class",
                    "standard",
                    "--uniform-bucket-level-access",
                    "--public-access-prevention",
                ],
                &project,
            ),
            None,
        )?;
    }
    // Object versioning keeps each older version of an object. One rule
    // deletes an older version after 7 days (01M3TJWJEPTSF1S3S5PJD25Z7Y).
    // The other deletes each thread object 30 days after its last change
    // (R46).
    let lifecycle = temp_file(LIFECYCLE)?;
    let lifecycle_path = lifecycle.path().display().to_string();
    gcloud.run(
        &with(
            &[
                "storage",
                "buckets",
                "update",
                &bucket,
                "--lifecycle-file",
                &lifecycle_path,
                "--versioning",
            ],
            &project,
        ),
        None,
    )?;
    println!("Bucket {}: versioning on, lifecycle rules set.", s.bucket);

    for name in [
        &s.run_account,
        &s.build_account,
        &s.deploy_account,
        &s.check_account,
    ] {
        if name.is_empty() {
            continue;
        }
        let email = s.account(name);
        if gcloud.exists(&with(
            &["iam", "service-accounts", "describe", &email],
            &project,
        ))? {
            println!("Service account {name}: exists.");
        } else {
            println!("Service account {name}: making it.");
            gcloud.run(
                &with(
                    &[
                        "iam",
                        "service-accounts",
                        "create",
                        name,
                        "--display-name",
                        name,
                    ],
                    &project,
                ),
                None,
            )?;
        }
    }
    // riff-server reads and writes only its bucket, and reads only its
    // secret (R134). It reads the secret of the GitHub Apps, and adds its
    // versions (01M6WG52ZEVRSQ463FTDS94M54). The build account may only
    // build and store images.
    let run = format!("serviceAccount:{}", s.account(&s.run_account));
    gcloud.bind(&with(
        &[
            "storage",
            "buckets",
            "add-iam-policy-binding",
            &bucket,
            "--member",
            &run,
            "--role",
            "roles/storage.objectUser",
        ],
        &project,
    ))?;
    for secret in s.secrets() {
        for role in secret.roles {
            gcloud.bind(&with(
                &[
                    "secrets",
                    "add-iam-policy-binding",
                    secret.name,
                    "--member",
                    &run,
                    "--role",
                    role,
                ],
                &project,
            ))?;
        }
    }
    gcloud.bind(&with(
        &[
            "projects",
            "add-iam-policy-binding",
            &s.project,
            "--member",
            &format!("serviceAccount:{}", s.account(&s.build_account)),
            "--role",
            "roles/run.builder",
            "--condition",
            "None",
        ],
        &project,
    ))?;
    println!("Service accounts: roles set.");

    let mut place = args(&["--location", &s.region]);
    place.extend_from_slice(&project);
    if gcloud.exists(&with(
        &["artifacts", "repositories", "describe", &s.repository],
        &place,
    ))? {
        println!("Image repository {}: exists.", s.repository);
    } else {
        println!("Image repository {}: making it.", s.repository);
        gcloud.run(
            &with(
                &[
                    "artifacts",
                    "repositories",
                    "create",
                    &s.repository,
                    "--repository-format",
                    "docker",
                ],
                &place,
            ),
            None,
        )?;
    }

    if s.deploy_account.is_empty() {
        println!(
            "CI deploy: none, because {}.env has no deploy account.",
            s.name
        );
    } else {
        ci_deploy(gcloud, s, &place)?;
    }

    // riff-server keeps the state in memory (01M3TJWJEPTSF1S3S5PJD25Z7Y).
    // A service that runs with another limit gets the limit now.
    let service = with(&["run", "services", "describe", &s.service], &s.place());
    if gcloud.exists(&service)? {
        let mut memory = service.clone();
        memory.extend(args(&[
            "--format",
            "value(spec.template.spec.containers[0].resources.limits.memory)",
        ]));
        if gcloud.run(&memory, None)?.trim() == s.memory {
            println!("Service {}: {} of memory.", s.service, s.memory);
        } else {
            println!("Service {}: setting {} of memory.", s.service, s.memory);
            gcloud.run(
                &with(
                    &[
                        "run", "services", "update", &s.service, "--memory", &s.memory,
                    ],
                    &s.place(),
                ),
                None,
            )?;
        }
    } else {
        println!(
            "Service {}: none yet. The deploy gives it {} of memory.",
            s.service, s.memory
        );
    }

    make_alert(gcloud, s, owner)?;

    let versions = gcloud
        .run(
            &with(
                &[
                    "secrets",
                    "versions",
                    "list",
                    &s.secret_name,
                    "--filter=state=ENABLED",
                    "--limit",
                    "1",
                    "--format=value(name)",
                ],
                &project,
            ),
            None,
        )
        .unwrap_or_default();
    if s.client_id.is_empty() || versions.trim().is_empty() {
        println!();
        println!("{}", text::cloud_next_signin(&s.name));
    }
    Ok(())
}

/// The sign-in of GitHub Actions for the deploy account of `s`: only
/// from the main branch and the tags `v*` of the repository
/// (01M3NJAZAQ3AKMAM0EGM7R3S89), and only a job in the GitHub
/// environment of `s` (01M49M8W30M2084QN4HX1FJFKS). No key exists.
///
/// ```mermaid
/// flowchart LR
///     job[job of the repository] --> pool{provider github:\nrepository, main or v*}
///     pool -- no --> refused[no sign-in]
///     pool -- yes --> env{attribute.environment}
///     env -- E of the account --> account[deploy account]
///     env -- other, or none --> refused
/// ```
fn ci_deploy(gcloud: &Gcloud, s: &Settings, place: &[String]) -> Result<()> {
    if s.github_environment.is_empty() {
        bail!("{}", text::cloud_no_github_environment(&s.name));
    }
    let project = s.project();
    let with = |parts: &[&str], tail: &[String]| {
        let mut a = args(parts);
        a.extend_from_slice(tail);
        a
    };
    let mut pool = args(&["--workload-identity-pool", "github", "--location", "global"]);
    pool.extend_from_slice(&project);
    let condition = format!(
        "assertion.repository == '{}' && (assertion.ref == 'refs/heads/main' || assertion.ref.startsWith('refs/tags/v'))",
        s.github_repo
    );
    if gcloud.exists(&with(
        &[
            "iam",
            "workload-identity-pools",
            "describe",
            "github",
            "--location",
            "global",
        ],
        &project,
    ))? {
        println!("Identity pool github: exists.");
    } else {
        println!("Identity pool github: making it.");
        gcloud.run(
            &with(
                &[
                    "iam",
                    "workload-identity-pools",
                    "create",
                    "github",
                    "--location",
                    "global",
                    "--display-name",
                    "GitHub Actions",
                ],
                &project,
            ),
            None,
        )?;
    }
    let provider = |verb: &str| {
        with(
            &[
                "iam",
                "workload-identity-pools",
                "providers",
                verb,
                "github",
            ],
            &pool,
        )
    };
    if gcloud.exists(&provider("describe"))? {
        println!("Identity provider github: exists. Setting its mapping and condition.");
        let mut update = provider("update-oidc");
        update.extend(args(&[
            "--attribute-mapping",
            GITHUB_MAPPING,
            "--attribute-condition",
            &condition,
        ]));
        gcloud.run(&update, None)?;
    } else {
        println!("Identity provider github: making it.");
        let mut make = provider("create-oidc");
        make.extend(args(&[
            "--issuer-uri",
            "https://token.actions.githubusercontent.com",
            "--attribute-mapping",
            GITHUB_MAPPING,
            "--attribute-condition",
            &condition,
        ]));
        gcloud.run(&make, None)?;
    }
    // The deploy account pushes images, deploys the service, and runs it
    // as riff-server. Only the repository may use the account.
    let deploy = format!("serviceAccount:{}", s.account(&s.deploy_account));
    gcloud.bind(&with(
        &[
            "artifacts",
            "repositories",
            "add-iam-policy-binding",
            &s.repository,
            "--member",
            &deploy,
            "--role",
            "roles/artifactregistry.writer",
        ],
        place,
    ))?;
    gcloud.bind(&with(
        &[
            "projects",
            "add-iam-policy-binding",
            &s.project,
            "--member",
            &deploy,
            "--role",
            "roles/run.admin",
            "--condition",
            "None",
        ],
        &project,
    ))?;
    gcloud.bind(&with(
        &[
            "iam",
            "service-accounts",
            "add-iam-policy-binding",
            &s.account(&s.run_account),
            "--member",
            &deploy,
            "--role",
            "roles/iam.serviceAccountUser",
        ],
        &project,
    ))?;
    let account = s.account(&s.deploy_account);
    let user = |verb: &str, member: &str| {
        with(
            &[
                "iam",
                "service-accounts",
                verb,
                &account,
                "--member",
                member,
                "--role",
                "roles/iam.workloadIdentityUser",
            ],
            &project,
        )
    };
    gcloud.bind(&user("add-iam-policy-binding", &github_job(s)))?;
    // Before 01M49M8W30M2084QN4HX1FJFKS, each job of the repository
    // could sign in as the account.
    let repository = github_repository(s);
    let policy = gcloud.run(
        &with(
            &[
                "iam",
                "service-accounts",
                "get-iam-policy",
                &account,
                "--format",
                "json",
            ],
            &project,
        ),
        None,
    )?;
    if policy.contains(&repository) {
        println!("CI deploy: each job of the repository signs in. Removing it.");
        gcloud.run(&user("remove-iam-policy-binding", &repository), None)?;
    }
    println!(
        "CI deploy: set, for a job in the GitHub environment {}.",
        s.github_environment
    );
    ci_check(gcloud, s)
}

/// The only role of the check account on the project: it reads the log
/// (01M76SSFX0QGPA4MTXV39JV5P3).
pub const CHECK_ROLE: &str = "roles/logging.viewer";

/// The sign-in of the CI job `Check the deploy` as the check account of
/// `s` (01M76SSFX0QGPA4MTXV39JV5P3). The account has only [`CHECK_ROLE`]
/// on the project. Only a job of a tag signs in as it ([`github_tag`]),
/// and the provider lets in only `main` and the tags `v*`: so only a job
/// of a tag `v*` does. No key exists.
///
/// ```mermaid
/// flowchart LR
///     job[job of the repository] --> pool{provider github:\nrepository, main or v*}
///     pool -- no --> refused[no sign-in]
///     pool -- yes --> kind{attribute.ref_type}
///     kind -- tag --> account["check account:\nroles/logging.viewer"]
///     kind -- branch --> refused
/// ```
fn ci_check(gcloud: &Gcloud, s: &Settings) -> Result<()> {
    if s.check_account.is_empty() {
        println!(
            "CI check: none, because {}.env has no check account.",
            s.name
        );
        return Ok(());
    }
    let project = s.project();
    let with = |parts: &[&str]| {
        let mut a = args(parts);
        a.extend_from_slice(&project);
        a
    };
    let account = s.account(&s.check_account);
    gcloud.bind(&with(&[
        "projects",
        "add-iam-policy-binding",
        &s.project,
        "--member",
        &format!("serviceAccount:{account}"),
        "--role",
        CHECK_ROLE,
        "--condition",
        "None",
    ]))?;
    gcloud.bind(&with(&[
        "iam",
        "service-accounts",
        "add-iam-policy-binding",
        &account,
        "--member",
        &github_tag(s),
        "--role",
        "roles/iam.workloadIdentityUser",
    ]))?;
    println!(
        "CI check: set, for a job of a tag v*. {} only reads the log.",
        s.check_account
    );
    Ok(())
}

/// The attribute mapping of the provider `github`. The deploy accounts
/// bind `attribute.environment` (01M49M8W30M2084QN4HX1FJFKS): the
/// subject of a job can have the immutable form of GitHub, with the IDs
/// of the owner and the repository, but the claim `environment` is the
/// plain name. The check account binds `attribute.ref_type`
/// (01M76SSFX0QGPA4MTXV39JV5P3).
pub const GITHUB_MAPPING: &str = "google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref,attribute.environment=assertion.environment,attribute.ref_type=assertion.ref_type";

/// The member of the pool `github` for each job of a tag
/// (01M76SSFX0QGPA4MTXV39JV5P3). The condition of the provider lets in
/// only the repository of `s`, its `main` and its tags `v*`.
///
/// ```
/// let s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
/// assert_eq!(
///     riff::cloud::github_tag(&s),
///     "principalSet://iam.googleapis.com/projects/123/locations/global/workloadIdentityPools/github/attribute.ref_type/tag"
/// );
/// assert!(riff::cloud::GITHUB_MAPPING.contains("attribute.ref_type=assertion.ref_type"));
/// ```
pub fn github_tag(s: &Settings) -> String {
    format!(
        "principalSet://iam.googleapis.com/projects/{}/locations/global/workloadIdentityPools/github/attribute.ref_type/tag",
        s.project_number
    )
}

/// The member of the pool `github` for each job in the GitHub
/// environment of `s` (01M49M8W30M2084QN4HX1FJFKS). The condition of
/// the provider lets in only the repository of `s`.
///
/// ```
/// let mut s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
/// s.github_repo = "acme/app".into();
/// s.github_environment = "production".into();
/// assert_eq!(
///     riff::cloud::github_job(&s),
///     "principalSet://iam.googleapis.com/projects/123/locations/global/workloadIdentityPools/github/attribute.environment/production"
/// );
/// assert!(riff::cloud::GITHUB_MAPPING.contains("attribute.environment=assertion.environment"));
/// ```
pub fn github_job(s: &Settings) -> String {
    format!(
        "principalSet://iam.googleapis.com/projects/{}/locations/global/workloadIdentityPools/github/attribute.environment/{}",
        s.project_number, s.github_environment
    )
}

/// The member of the pool `github` for each job of the repository of
/// `s`. No deploy account has it (01M49M8W30M2084QN4HX1FJFKS):
/// `create` removes it.
///
/// ```
/// let mut s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
/// s.github_repo = "acme/app".into();
/// assert!(riff::cloud::github_repository(&s).ends_with("/attribute.repository/acme/app"));
/// ```
pub fn github_repository(s: &Settings) -> String {
    format!(
        "principalSet://iam.googleapis.com/projects/{}/locations/global/workloadIdentityPools/github/attribute.repository/{}",
        s.project_number, s.github_repo
    )
}

/// The alert on each log line of the service with the severity ERROR
/// or more, by email to the address of `RIFF_OWNER`
/// (01M3TJWJ6J3M6JRXJTAETZ5M6F). The repository is public, so the address
/// is not in it.
fn make_alert(gcloud: &Gcloud, s: &Settings, owner: Option<&str>) -> Result<()> {
    if s.alert.is_empty() {
        println!("Alert: none, because {}.env has no alert.", s.name);
        return Ok(());
    }
    let Some(owner) = owner else {
        println!("{}", text::cloud_alert_no_owner(&s.name));
        return Ok(());
    };
    let project = s.project();
    let list = |what: &str, name: &str| {
        let mut a = args(&[
            what,
            "monitoring",
            if what == "beta" {
                "channels"
            } else {
                "policies"
            },
            "list",
        ]);
        a.extend_from_slice(&project);
        a.extend(args(&[
            "--filter",
            &format!("displayName=\"{name}\""),
            "--format",
            "value(name)",
        ]));
        a
    };
    let mut channel = gcloud
        .run(&list("beta", &s.alert_channel), None)?
        .trim()
        .to_owned();
    if channel.is_empty() {
        println!("Alert channel {}: making it.", s.alert_channel);
        let mut make = args(&["beta", "monitoring", "channels", "create"]);
        make.extend_from_slice(&project);
        make.extend(args(&[
            "--display-name",
            &s.alert_channel,
            "--type",
            "email",
            "--channel-labels",
            &format!("email_address={owner}"),
            "--format",
            "value(name)",
        ]));
        channel = gcloud.run(&make, None)?.trim().to_owned();
    } else {
        println!("Alert channel {}: exists.", s.alert_channel);
    }
    if gcloud
        .run(&list("alpha", &s.alert), None)?
        .trim()
        .is_empty()
    {
        println!("Alert {}: making it.", s.alert);
        let policy = temp_file(&alert(s))?;
        let mut make = args(&["alpha", "monitoring", "policies", "create"]);
        make.extend_from_slice(&project);
        make.extend(args(&[
            "--policy-from-file",
            &policy.path().display().to_string(),
            "--notification-channels",
            &channel,
        ]));
        gcloud.run(&make, None)?;
    } else {
        println!("Alert {}: exists.", s.alert);
    }
    Ok(())
}

/// Stores the sign-in client of the instance in `path`: the secret in
/// Secret Manager, and the ID in the settings file. The secret goes to
/// no file.
pub fn signin(gcloud: &Gcloud, s: &Settings, path: &Path, id: &str, secret: &str) -> Result<()> {
    if !id.ends_with(".apps.googleusercontent.com")
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
    {
        bail!(text::CLOUD_BAD_CLIENT_ID);
    }
    if secret.is_empty() {
        bail!(text::CLOUD_EMPTY_SECRET);
    }
    let mut add = args(&[
        "secrets",
        "versions",
        "add",
        &s.secret_name,
        "--data-file=-",
    ]);
    add.extend_from_slice(&s.project());
    gcloud.run(&add, Some(secret))?;
    println!("Secret {}: stored.", s.secret_name);
    let text = std::fs::read_to_string(path)?;
    std::fs::write(path, set_line(&text, "RIFF_OIDC_CLIENT_ID", id))?;
    println!("{}", text::cloud_client_written(path));
    Ok(())
}

/// Reads a line from stdin with no echo on a terminal: for a secret.
pub fn read_hidden(prompt: &str) -> Result<String> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
    let stdin = std::io::stdin();
    eprint!("{prompt}");
    let old = tcgetattr(&stdin).ok();
    if let Some(old) = &old {
        let mut quiet = old.clone();
        quiet.local_flags.remove(LocalFlags::ECHO);
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, &quiet);
    }
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if let Some(old) = &old {
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, old);
        eprintln!();
    }
    read?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Reads a line from stdin, after `prompt`.
pub fn read_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// What `deploy` deploys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The image that CI built for a release tag.
    Tag(String),
    /// The tree at this path, which Cloud Build builds.
    Tree(PathBuf),
}

/// True when `tag` is a release tag `vX.Y.Z`.
///
/// ```
/// use riff::cloud::release_tag;
/// assert!(release_tag("v1.0.0") && release_tag("v10.20.30"));
/// assert!(!release_tag("1.0.0") && !release_tag("v1.0") && !release_tag("v1.0.0-rc1"));
/// ```
pub fn release_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// True when `tag` is the full ID of a commit: 40 hex digits in lower
/// case. CI tags the image of each merge to main with it
/// (01M496JTDB16G52G22CJZRA8J0).
///
/// ```
/// use riff::cloud::commit_tag;
/// assert!(commit_tag("183456a0c4e2b1f3d5a6978877665544332211ff"));
/// assert!(!commit_tag("183456a") && !commit_tag("v1.0.0"));
/// assert!(!commit_tag("183456A0C4E2B1F3D5A6978877665544332211FF"));
/// ```
pub fn commit_tag(tag: &str) -> bool {
    tag.len() == 40 && tag.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// The build of the tree at `top`: the hash of the trees of the code,
/// and the UTC time of the last commit that changed it, as `RIFF_BUILD`
/// and `RIFF_BUILD_TIME` lines. Cloud Build gets the source with no
/// git, so the image reads the build from this file
/// (01M3JEE7YXQPWS65FBVTASAEBX, 01M6QV4SRYYAH06WSYMF6KHY7S). It runs the
/// git commands of `deploy/build-id.sh`.
pub fn build_id(top: &Path) -> Result<String> {
    let git = |args: &[&str], input: Option<&str>| -> Result<String> {
        let mut child = Command::new("git")
            .args(args)
            .env("TZ", "UTC")
            .current_dir(top)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("cannot run git")?;
        if let (Some(input), Some(stdin)) = (input, child.stdin.as_mut()) {
            stdin.write_all(input.as_bytes())?;
        }
        let out = child.wait_with_output().context("cannot run git")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    let code = ["crates", "Cargo.toml", "Cargo.lock"];
    let trees = git(&[&["ls-tree", "HEAD", "--"][..], &code].concat(), None)?;
    // `git` trims the output: the script hashes the lines with their end.
    let id = git(&["hash-object", "--stdin"], Some(&format!("{trees}\n")))?;
    let log = [
        &[
            "log",
            "-1",
            "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
            "--format=%cd",
            "--",
        ][..],
        &code,
    ]
    .concat();
    let time = git(&log, None)?;
    if trees.is_empty() || id.is_empty() || time.is_empty() {
        bail!("git log of {} gave no commit of the code", top.display());
    }
    Ok(format!(
        "RIFF_BUILD={}\nRIFF_BUILD_TIME={time}\n",
        id.chars().take(12).collect::<String>()
    ))
}

/// Deploys riff-server of `source` to the service of `s` (R5, R6, R29,
/// R32, R130, R131, R134). `admin` is the email of `RIFF_OWNER`: riff-server
/// names it an admin at each start, as `RIFF_ADMINS`
/// (01M7767SVH8AKK7144QTJGGGRY).
pub fn deploy(gcloud: &Gcloud, s: &Settings, source: &Source, admin: &str) -> Result<()> {
    if s.client_id.is_empty() {
        bail!(text::cloud_no_client(&s.name));
    }
    let mut deploy = args(&["run", "deploy", &s.service]);
    let _build_id = match source {
        Source::Tag(tag) => {
            deploy.extend(args(&["--image", &s.image(tag)]));
            None
        }
        Source::Tree(top) => {
            let path = top.join("build-id.env");
            std::fs::write(&path, build_id(top)?)?;
            deploy.extend(args(&[
                "--source",
                &top.display().to_string(),
                "--build-service-account",
                &format!(
                    "projects/{}/serviceAccounts/{}",
                    s.project,
                    s.account(&s.build_account)
                ),
            ]));
            Some(RemoveOnDrop(path))
        }
    };
    deploy.push("--quiet".into());
    deploy.extend(s.place());
    // One instance, with its CPU on also between calls, and the memory
    // for the state (01M3TJWJEPTSF1S3S5PJD25Z7Y). 1000 calls at a time,
    // each for up to 60 minutes. riff checks each token itself, so Cloud
    // Run lets each caller in.
    deploy.extend(args(&[
        "--service-account",
        &s.account(&s.run_account),
        "--port",
        "8080",
        "--min-instances",
        &s.min_instances.to_string(),
        "--max-instances",
        "1",
        "--no-cpu-throttling",
        "--memory",
        &s.memory,
        "--concurrency",
        "1000",
        "--timeout",
        "3600",
        "--no-invoker-iam-check",
        "--set-env-vars",
        &format!(
            "RIFF_PUBLIC_URL={},RIFF_REQUIRE_SIGN_IN=true,RIFF_OIDC_CLIENT_ID={},RIFF_BUCKET={},RIFF_ADMINS={admin},RIFF_GITHUB_SECRET=projects/{}/secrets/{}",
            s.url, s.client_id, s.bucket, s.project, s.github_secret
        ),
        "--set-secrets",
        &format!("RIFF_OIDC_CLIENT_SECRET={}:latest", s.secret_name),
    ]));
    gcloud.show(&deploy)?;

    // A riff with no domain, or with the Cloud Run URL, maps no domain.
    if matches!(source, Source::Tag(_))
        || s.domain.is_empty()
        || s.url != format!("https://{}", s.domain)
    {
        return Ok(());
    }
    // Only the beta commands take --region.
    let mut describe = args(&[
        "beta",
        "run",
        "domain-mappings",
        "describe",
        "--domain",
        &s.domain,
    ]);
    describe.extend(s.place());
    if gcloud.exists(&describe)? {
        println!("Domain {}: mapped.", s.domain);
    } else {
        // gcloud shows the DNS records to add.
        println!("Domain {}: mapping it.", s.domain);
        let mut map = args(&[
            "beta",
            "run",
            "domain-mappings",
            "create",
            "--service",
            &s.service,
            "--domain",
            &s.domain,
        ]);
        map.extend(s.place());
        gcloud.show(&map)?;
    }
    Ok(())
}

/// Removes a file when it drops.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The facts of a service on Cloud Run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// False when no service runs.
    pub exists: bool,
    /// The condition `Ready` of the service is `True`.
    pub ready: bool,
    /// The image of the last ready revision.
    pub image: String,
    /// The last ready revision.
    pub revision: String,
    /// The memory of the service.
    pub memory: String,
}

impl Facts {
    /// The release of the image: its tag, or `a build` for an image
    /// that Cloud Build built from a tree.
    ///
    /// ```
    /// let mut f = riff::cloud::Facts::default();
    /// f.image = "r-docker.pkg.dev/p/riff/riff-server:v1.0.0".into();
    /// assert_eq!(f.release(), "v1.0.0");
    /// f.image = "r-docker.pkg.dev/p/cloud-run-source-deploy/riff@sha256:ab".into();
    /// assert_eq!(f.release(), "a build");
    /// assert_eq!(riff::cloud::Facts::default().release(), "-");
    /// ```
    pub fn release(&self) -> String {
        if self.image.is_empty() {
            return "-".into();
        }
        match self.image.rsplit_once(':') {
            Some((name, tag)) if name.ends_with("/riff-server") && release_tag(tag) => {
                tag.to_owned()
            }
            _ => "a build".into(),
        }
    }
}

/// The facts of the service of `s`. A service that `gcloud` does not
/// find has no facts. Each other failure of `gcloud` is an error, so
/// riff never says "no service" for a failed call
/// (01M4382RKERWAPKBRY9W8F2GSA).
pub fn facts(gcloud: &Gcloud, s: &Settings) -> Result<Facts> {
    let mut describe = args(&["run", "services", "describe", &s.service]);
    describe.extend(s.place());
    describe.extend(args(&["--format", "json"]));
    let Some(out) = gcloud.find(&describe)? else {
        return Ok(Facts::default());
    };
    let json: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let ready = json["status"]["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["type"] == "Ready" && c["status"] == "True");
    let container = &json["spec"]["template"]["spec"]["containers"][0];
    Ok(Facts {
        exists: true,
        ready,
        image: container["image"].as_str().unwrap_or_default().to_owned(),
        revision: json["status"]["latestReadyRevisionName"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        memory: container["resources"]["limits"]["memory"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The state of a secret of an instance in Secret Manager, for
/// `riff cloud status` (01M6YC5XGQR7V3KC68WW8NE1PD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretState {
    /// What the secret holds ([`Secret::label`]).
    pub label: &'static str,
    /// The name of the secret.
    pub name: String,
    /// False when Secret Manager has no secret of this name.
    pub exists: bool,
    /// The roles that the run account needs on the secret and does not
    /// have. Empty for a secret that does not exist.
    pub missing: Vec<&'static str>,
}

/// The state of each secret of `s` ([`Settings::secrets`]): whether it
/// exists, and each role that the run account needs on it and does not
/// have (01M6YC5XGQR7V3KC68WW8NE1PD). Each failure of `gcloud` other than "not
/// found" is an error, so riff never says "missing" for a failed call
/// (01M4382RKERWAPKBRY9W8F2GSA).
pub fn secret_states(gcloud: &Gcloud, s: &Settings) -> Result<Vec<SecretState>> {
    let member = format!("serviceAccount:{}", s.account(&s.run_account));
    let mut states = Vec::new();
    for secret in s.secrets() {
        let mut describe = args(&["secrets", "describe", secret.name]);
        describe.extend(s.project());
        let exists = gcloud.find(&describe)?.is_some();
        let missing = if exists {
            let mut policy = args(&["secrets", "get-iam-policy", secret.name]);
            policy.extend(s.project());
            policy.extend(args(&["--format", "json"]));
            missing_roles(&gcloud.run(&policy, None)?, &member, secret.roles)
        } else {
            Vec::new()
        };
        states.push(SecretState {
            label: secret.label,
            name: secret.name.to_owned(),
            exists,
            missing,
        });
    }
    Ok(states)
}

/// Each role of `roles` that `member` has in no binding of `policy`, the
/// JSON of `gcloud ... get-iam-policy`. A policy with no bindings gives
/// each role.
///
/// ```
/// use riff::cloud::{GITHUB_SECRET_ROLES, SIGNIN_SECRET_ROLES, missing_roles};
///
/// let member = "serviceAccount:a@p.iam.gserviceaccount.com";
/// let policy = format!(
///     r#"{{"bindings":[{{"role":"roles/secretmanager.secretAccessor","members":["{member}"]}}]}}"#
/// );
/// assert_eq!(missing_roles(&policy, member, GITHUB_SECRET_ROLES), ["roles/secretmanager.secretVersionAdder"]);
/// assert!(missing_roles(&policy, member, SIGNIN_SECRET_ROLES).is_empty());
/// assert_eq!(missing_roles(&policy, "serviceAccount:b@p", SIGNIN_SECRET_ROLES), SIGNIN_SECRET_ROLES);
/// assert_eq!(missing_roles(r#"{"etag":"ACAB"}"#, member, GITHUB_SECRET_ROLES), GITHUB_SECRET_ROLES);
/// ```
pub fn missing_roles(policy: &str, member: &str, roles: &[&'static str]) -> Vec<&'static str> {
    let policy: serde_json::Value = serde_json::from_str(policy).unwrap_or_default();
    let has = |role: &str| {
        policy["bindings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["role"] == role)
            .flat_map(|b| b["members"].as_array().into_iter().flatten())
            .any(|m| m == member)
    };
    roles.iter().copied().filter(|role| !has(role)).collect()
}

/// The `gcloud` arguments that read the log of the service of `s`:
/// the last `limit` lines, only the errors with `errors`, and only the
/// lines of `filter`.
///
/// ```
/// let s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
/// let a = riff::cloud::log_args(&s, true, 20, Some(r#"jsonPayload.result="refused""#));
/// assert_eq!(a[..5], ["run", "services", "logs", "read", "mine"]);
/// assert!(a.contains(&r#"severity>=ERROR AND (jsonPayload.result="refused")"#.to_owned()));
/// assert!(a.windows(2).any(|w| w == ["--limit", "20"]));
/// ```
pub fn log_args(s: &Settings, errors: bool, limit: u32, filter: Option<&str>) -> Vec<String> {
    let mut a = args(&["run", "services", "logs", "read", &s.service]);
    a.extend(s.place());
    a.extend(args(&["--limit", &limit.to_string()]));
    let filter = match (errors, filter) {
        (true, Some(f)) => Some(format!("severity>=ERROR AND ({f})")),
        (true, None) => Some("severity>=ERROR".into()),
        (false, Some(f)) => Some(f.to_owned()),
        (false, None) => None,
    };
    if let Some(filter) = filter {
        a.extend(args(&["--log-filter", &filter]));
    }
    a
}

/// Deletes the service of `s`, and its bucket with all its objects
/// when `with_state`. The secret, the accounts and the settings stay.
pub fn delete(gcloud: &Gcloud, s: &Settings, with_state: bool) -> Result<()> {
    let mut describe = args(&["run", "services", "describe", &s.service]);
    describe.extend(s.place());
    if gcloud.exists(&describe)? {
        let mut delete = args(&["run", "services", "delete", &s.service, "--quiet"]);
        delete.extend(s.place());
        gcloud.run(&delete, None)?;
        println!("Service {}: deleted.", s.service);
    } else {
        println!("Service {}: none.", s.service);
    }
    let bucket = format!("gs://{}", s.bucket);
    if !with_state {
        println!("{}", text::cloud_state_stays(&s.bucket));
        return Ok(());
    }
    let mut describe = args(&["storage", "buckets", "describe", &bucket]);
    describe.extend(s.project());
    if gcloud.exists(&describe)? {
        let mut remove = args(&["storage", "rm", "--recursive", &bucket]);
        remove.extend(s.project());
        gcloud.run(&remove, None)?;
        println!("Bucket {}: deleted, with its state.", s.bucket);
    } else {
        println!("Bucket {}: none.", s.bucket);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `git ARGS` in `dir`, as a person with a name and no signing.
    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b.c"])
            .args(["-c", "commit.gpgsign=false", "-C"])
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// A folder with a repository `origin` and its clone `clone`, with
    /// one commit, and a folder `deploy/cloud` in the clone.
    fn clones() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let (origin, clone) = (dir.path().join("origin"), dir.path().join("clone"));
        git(dir.path(), &["init", "-q", "-b", "main", "origin"]);
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(dir.path(), &["clone", "-q", "origin", "clone"]);
        std::fs::create_dir_all(clone.join("deploy/cloud")).unwrap();
        (dir, origin, clone)
    }

    #[test]
    fn settings_with_no_github_secret_name_the_line_to_add() {
        let text = Settings::new("team", "acme", "1", "r").text();
        let line = "CLOUD_GITHUB_SECRET=team-github-apps\n";
        assert!(text.contains(line), "{text}");
        for gone in ["", "CLOUD_GITHUB_SECRET=\n", "CLOUD_GITHUB_SECRET=  \n"] {
            let text = text.replace(line, gone);
            let e = Settings::parse("team", &text).unwrap_err().to_string();
            assert!(
                e.starts_with("the cloud settings team have no CLOUD_GITHUB_SECRET. "),
                "{e}"
            );
            assert!(
                e.contains("Add the line CLOUD_GITHUB_SECRET=SECRET to team.env. "),
                "{e}"
            );
        }
    }

    #[test]
    fn load_names_the_path_of_the_file_with_no_github_secret() {
        let dir = tempfile::tempdir().unwrap();
        let text = Settings::new("team", "acme", "1", "r").text();
        let path = file(dir.path(), "team");
        std::fs::write(
            &path,
            text.replace("CLOUD_GITHUB_SECRET=team-github-apps\n", ""),
        )
        .unwrap();
        let e = load(dir.path(), "team").unwrap_err().to_string();
        assert_eq!(e, text::cloud_no_github_secret("team", &path));
        assert!(e.contains(&format!("to {}. ", path.display())), "{e}");
    }

    #[test]
    fn a_clone_is_behind_only_after_a_new_commit_of_its_upstream() {
        let (_dir, origin, clone) = clones();
        assert_eq!(behind(&clone.join("deploy/cloud")), None);
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "third"]);
        let late = behind(&clone.join("deploy/cloud")).unwrap();
        assert_eq!(late.upstream, "origin/main");
        assert_eq!(late.commits, 2);
        assert_eq!(
            late.top.canonicalize().unwrap(),
            clone.canonicalize().unwrap()
        );
        // A commit of the clone only: the branch is ahead, not behind.
        git(&clone, &["pull", "-q", "--ff-only"]);
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "mine"]);
        assert_eq!(behind(&clone), None);
    }

    #[test]
    fn a_failed_fetch_compares_with_the_upstream_ref_of_the_clone() {
        let (_dir, origin, clone) = clones();
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
        git(&clone, &["fetch", "-q"]);
        std::fs::remove_dir_all(&origin).unwrap();
        let late = behind(&clone).unwrap();
        assert_eq!(late.commits, 1);
    }

    #[test]
    fn no_upstream_a_detached_head_and_no_clone_have_no_check() {
        let (dir, origin, clone) = clones();
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
        git(&clone, &["fetch", "-q"]);
        assert!(behind(&clone).is_some());
        git(&clone, &["checkout", "-q", "--detach"]);
        assert_eq!(behind(&clone), None);
        git(&clone, &["checkout", "-q", "-b", "local"]);
        assert_eq!(behind(&clone), None);
        // The folder of the clones is in no clone.
        assert_eq!(behind(dir.path()), None);
        assert_eq!(behind(&dir.path().join("nosuch")), None);
    }

    /// 01M6YC5XGQR7V3KC68WW8NE1PD: in a clone that is behind, `settings_for_create`
    /// stops before each call of `gcloud`, and writes nothing.
    #[test]
    fn create_in_a_clone_that_is_behind_stops_before_gcloud() {
        let (_dir, origin, clone) = clones();
        let cloud = clone.join("deploy/cloud");
        let text = Settings::new("team", "acme", "1", "r").text();
        std::fs::write(file(&cloud, "team"), &text).unwrap();
        git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
        // No gcloud is there: a call would fail with another error.
        let gcloud = Gcloud::at("/nonexistent/gcloud");
        for (name, project, region) in [
            ("team", None, None),
            ("new", Some("acme"), Some("europe-west1")),
        ] {
            let e = settings_for_create(&gcloud, &cloud, name, project, region)
                .unwrap_err()
                .to_string();
            assert!(e.starts_with("The clone "), "{e}");
            assert!(e.contains("Run git pull in "), "{e}");
            assert!(!file(&cloud, "new").exists());
        }
        git(&clone, &["pull", "-q", "--ff-only"]);
        let s = settings_for_create(&gcloud, &cloud, "team", None, None).unwrap();
        assert_eq!(s.github_secret, "team-github-apps");
    }

    #[test]
    fn each_secret_names_the_roles_of_the_run_account_on_it() {
        let s = Settings::new("team", "acme", "1", "r");
        let [signin, github] = s.secrets();
        assert_eq!(signin.name, s.secret_name);
        assert_eq!(github.name, s.github_secret);
        assert_eq!(signin.roles, ["roles/secretmanager.secretAccessor"]);
        assert_eq!(
            github.roles,
            [
                "roles/secretmanager.secretAccessor",
                "roles/secretmanager.secretVersionAdder"
            ]
        );
    }

    #[test]
    fn missing_roles_reads_only_the_bindings_of_the_member() {
        let member = "serviceAccount:run@p.iam.gserviceaccount.com";
        let policy = serde_json::json!({"bindings": [
            {"role": "roles/secretmanager.secretVersionAdder", "members": ["user:x@y.z"]},
            {"role": "roles/secretmanager.secretAccessor", "members": ["user:x@y.z", member]},
        ]})
        .to_string();
        assert_eq!(
            missing_roles(&policy, member, GITHUB_SECRET_ROLES),
            ["roles/secretmanager.secretVersionAdder"]
        );
        assert!(missing_roles(&policy, member, SIGNIN_SECRET_ROLES).is_empty());
        assert_eq!(
            missing_roles("", member, GITHUB_SECRET_ROLES),
            GITHUB_SECRET_ROLES
        );
    }
}
