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
//! | `riff cloud status NAME` | The same for one instance, with its revision and memory |
//! | `riff cloud log NAME [--errors]` | The log lines of the service |
//! | `riff cloud delete NAME [--with-state]` | Deletes the service, and the bucket with `--with-state` ([`delete`]) |
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
//! assert!(s.to_string().contains("CLOUD_REGION"));
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
            bail!(
                "gcloud {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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

    /// True when `gcloud ARGS` exits with status 0: for a check that a
    /// resource exists.
    pub fn ok(&self, args: &[String]) -> bool {
        self.run(args, None).is_ok()
    }

    /// Runs `gcloud ARGS` up to 6 times, with a wait between two tries.
    /// IAM needs some seconds before it knows a new service account.
    pub fn bind(&self, args: &[String]) -> Result<()> {
        for _ in 0..5 {
            if self.ok(args) {
                return Ok(());
            }
            std::thread::sleep(self.wait);
        }
        self.run(args, None).map(drop)
    }
}

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
    /// The secret of the sign-in client in Secret Manager.
    pub secret: String,
    /// The bucket that holds the state of riff-server.
    pub bucket: String,
    /// The Cloud Run service.
    pub service: String,
    /// The domain of the service, or empty.
    pub domain: String,
    /// The memory of the service, for example `1Gi`.
    pub memory: String,
    /// The alert on each error in the log, or empty for no alert.
    pub alert: String,
    /// The channel that sends the alert to the owner.
    pub alert_channel: String,
    /// The public URL of the service.
    pub url: String,
    /// The account that riff-server runs as.
    pub run_account: String,
    /// The account that Cloud Build builds the image as.
    pub build_account: String,
    /// The account that CI deploys as, or empty for no CI deploy.
    pub deploy_account: String,
    /// The GitHub repository whose CI deploys, or empty.
    pub github_repo: String,
    /// The image repository of the project.
    pub repository: String,
    /// The client ID of the sign-in client, or empty before `signin`.
    pub client_id: String,
    /// True when `deploy` asks for the name ([`confirm`]).
    pub confirm: bool,
}

/// The keys that must have a value.
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
    /// The settings of the instance `name` from the text of its file.
    /// It fails when a key of [`REQUIRED`] has no value.
    pub fn parse(name: &str, text: &str) -> Result<Self> {
        let values = values(text);
        for key in REQUIRED {
            if values.get(key).is_none_or(|v| v.is_empty()) {
                bail!("the cloud settings {name} have no {key}");
            }
        }
        let get = |key: &str| values.get(key).cloned().unwrap_or_default();
        Ok(Self {
            name: name.to_owned(),
            project: get("CLOUD_PROJECT"),
            project_number: get("CLOUD_PROJECT_NUMBER"),
            region: get("CLOUD_REGION"),
            secret: get("CLOUD_SECRET"),
            bucket: get("CLOUD_BUCKET"),
            service: get("CLOUD_SERVICE"),
            domain: get("CLOUD_DOMAIN"),
            memory: get("CLOUD_MEMORY"),
            alert: get("CLOUD_ALERT"),
            alert_channel: get("CLOUD_ALERT_CHANNEL"),
            url: get("CLOUD_URL"),
            run_account: get("CLOUD_RUN_ACCOUNT"),
            build_account: get("CLOUD_BUILD_ACCOUNT"),
            deploy_account: get("CLOUD_DEPLOY_ACCOUNT"),
            github_repo: get("CLOUD_GITHUB_REPO"),
            repository: get("CLOUD_REPOSITORY"),
            client_id: get("RIFF_OIDC_CLIENT_ID"),
            confirm: get("CLOUD_CONFIRM") == "true",
        })
    }

    /// The settings of a new instance `name` in `project` (with the
    /// number `number`) and `region`. Each resource takes its name from
    /// the instance, so two instances share no resource.
    ///
    /// ```
    /// let s = riff::cloud::Settings::new("mine", "acme", "123", "europe-west1");
    /// assert_eq!(s.service, "mine");
    /// assert_eq!(s.bucket, "acme-mine-state");
    /// assert_eq!(s.run_account, "mine-server");
    /// assert_eq!(s.url, "https://mine-123.europe-west1.run.app");
    /// let text = s.text();
    /// assert_eq!(riff::cloud::Settings::parse("mine", &text).unwrap(), s);
    /// ```
    pub fn new(name: &str, project: &str, number: &str, region: &str) -> Self {
        Self {
            name: name.to_owned(),
            project: project.to_owned(),
            project_number: number.to_owned(),
            region: region.to_owned(),
            secret: format!("{name}-oidc-client-secret"),
            bucket: format!("{project}-{name}-state"),
            service: name.to_owned(),
            domain: String::new(),
            memory: "1Gi".into(),
            alert: String::new(),
            alert_channel: String::new(),
            url: format!("https://{name}-{number}.{region}.run.app"),
            run_account: format!("{name}-server"),
            build_account: format!("{name}-build"),
            deploy_account: String::new(),
            github_repo: String::new(),
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
            ("CLOUD_SECRET", self.secret.clone()),
            ("CLOUD_BUCKET", self.bucket.clone()),
            ("CLOUD_SERVICE", self.service.clone()),
            ("CLOUD_DOMAIN", self.domain.clone()),
            ("CLOUD_MEMORY", self.memory.clone()),
            ("CLOUD_ALERT", self.alert.clone()),
            ("CLOUD_ALERT_CHANNEL", self.alert_channel.clone()),
            ("CLOUD_URL", self.url.clone()),
            ("CLOUD_RUN_ACCOUNT", self.run_account.clone()),
            ("CLOUD_BUILD_ACCOUNT", self.build_account.clone()),
            ("CLOUD_DEPLOY_ACCOUNT", self.deploy_account.clone()),
            ("CLOUD_GITHUB_REPO", self.github_repo.clone()),
            ("CLOUD_REPOSITORY", self.repository.clone()),
            ("CLOUD_CONFIRM", self.confirm.to_string()),
            ("RIFF_OIDC_CLIENT_ID", self.client_id.clone()),
        ]
    }

    /// The email of the service account `name` of the project.
    pub fn account(&self, name: &str) -> String {
        format!("{name}@{}.iam.gserviceaccount.com", self.project)
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
    Settings::parse(name, &text)
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
/// settings that are there.
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
    if !gcloud.ok(&args(&["projects", "describe", &s.project])) {
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

    if gcloud.ok(&with(&["secrets", "describe", &s.secret], &project)) {
        println!("Secret {}: exists.", s.secret);
    } else {
        println!("Secret {}: making it.", s.secret);
        gcloud.run(
            &with(
                &[
                    "secrets",
                    "create",
                    &s.secret,
                    "--replication-policy",
                    "automatic",
                ],
                &project,
            ),
            None,
        )?;
    }

    let bucket = format!("gs://{}", s.bucket);
    if gcloud.ok(&with(
        &["storage", "buckets", "describe", &bucket],
        &project,
    )) {
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

    for name in [&s.run_account, &s.build_account, &s.deploy_account] {
        if name.is_empty() {
            continue;
        }
        let email = s.account(name);
        if gcloud.ok(&with(
            &["iam", "service-accounts", "describe", &email],
            &project,
        )) {
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
    // secret (R134). The build account may only build and store images.
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
    gcloud.bind(&with(
        &[
            "secrets",
            "add-iam-policy-binding",
            &s.secret,
            "--member",
            &run,
            "--role",
            "roles/secretmanager.secretAccessor",
        ],
        &project,
    ))?;
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
    if gcloud.ok(&with(
        &["artifacts", "repositories", "describe", &s.repository],
        &place,
    )) {
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
    if gcloud.ok(&service) {
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
                    &s.secret,
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
/// (01M3NJAZAQ3AKMAM0EGM7R3S89). No key exists.
fn ci_deploy(gcloud: &Gcloud, s: &Settings, place: &[String]) -> Result<()> {
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
    if gcloud.ok(&with(
        &[
            "iam",
            "workload-identity-pools",
            "describe",
            "github",
            "--location",
            "global",
        ],
        &project,
    )) {
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
    if gcloud.ok(&provider("describe")) {
        println!("Identity provider github: exists. Setting its condition.");
        let mut update = provider("update-oidc");
        update.extend(args(&["--attribute-condition", &condition]));
        gcloud.run(&update, None)?;
    } else {
        println!("Identity provider github: making it.");
        let mut make = provider("create-oidc");
        make.extend(args(&[
            "--issuer-uri",
            "https://token.actions.githubusercontent.com",
            "--attribute-mapping",
            "google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref",
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
    let github = format!(
        "principalSet://iam.googleapis.com/projects/{}/locations/global/workloadIdentityPools/github/attribute.repository/{}",
        s.project_number, s.github_repo
    );
    gcloud.bind(&with(
        &[
            "iam",
            "service-accounts",
            "add-iam-policy-binding",
            &s.account(&s.deploy_account),
            "--member",
            &github,
            "--role",
            "roles/iam.workloadIdentityUser",
        ],
        &project,
    ))?;
    println!("CI deploy: set.");
    Ok(())
}

/// The alert on each log line of the service with the severity ERROR
/// or more, by email to the owner (01M3TJWJ6J3M6JRXJTAETZ5M6F). The
/// repository is public, so the email comes from `RIFF_OWNER`.
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
    let mut add = args(&["secrets", "versions", "add", &s.secret, "--data-file=-"]);
    add.extend_from_slice(&s.project());
    gcloud.run(&add, Some(secret))?;
    println!("Secret {}: stored.", s.secret);
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

/// The build of the tree at `top`: the last commit that changed the
/// code, and its UTC time, as `RIFF_COMMIT` and `RIFF_COMMIT_TIME`
/// lines. Cloud Build gets the source with no git, so the image reads
/// the build from this file (01M3JEE7YXQPWS65FBVTASAEBX).
pub fn build_id(top: &Path) -> Result<String> {
    let out = Command::new("git")
        .args([
            "log",
            "-1",
            "--abbrev=12",
            "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
            "--format=%h %cd",
            "--",
            "crates",
            "Cargo.toml",
            "Cargo.lock",
        ])
        .env("TZ", "UTC")
        .current_dir(top)
        .output()
        .context("cannot run git")?;
    let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let Some((commit, time)) = line.split_once(' ') else {
        bail!("git log of {} gave no commit", top.display());
    };
    Ok(format!("RIFF_COMMIT={commit}\nRIFF_COMMIT_TIME={time}\n"))
}

/// Deploys riff-server of `source` to the service of `s` (R5, R6, R29,
/// R32, R130, R131, R134). `owner` is the owner of the riff
/// (01M3JN3ASSV9SA0QZKXXJ0RTEV).
pub fn deploy(gcloud: &Gcloud, s: &Settings, source: &Source, owner: &str) -> Result<()> {
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
        "1",
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
            "RIFF_PUBLIC_URL={},RIFF_REQUIRE_SIGN_IN=true,RIFF_OIDC_CLIENT_ID={},RIFF_BUCKET={},RIFF_OWNER={owner}",
            s.url, s.client_id, s.bucket
        ),
        "--set-secrets",
        &format!("RIFF_OIDC_CLIENT_SECRET={}:latest", s.secret),
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
    if gcloud.ok(&describe) {
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

/// The facts of the service of `s`.
pub fn facts(gcloud: &Gcloud, s: &Settings) -> Facts {
    let mut describe = args(&["run", "services", "describe", &s.service]);
    describe.extend(s.place());
    describe.extend(args(&["--format", "json"]));
    let Ok(out) = gcloud.run(&describe, None) else {
        return Facts::default();
    };
    let json: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let ready = json["status"]["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["type"] == "Ready" && c["status"] == "True");
    let container = &json["spec"]["template"]["spec"]["containers"][0];
    Facts {
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
    }
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
    if gcloud.ok(&describe) {
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
    if gcloud.ok(&describe) {
        let mut remove = args(&["storage", "rm", "--recursive", &bucket]);
        remove.extend(s.project());
        gcloud.run(&remove, None)?;
        println!("Bucket {}: deleted, with its state.", s.bucket);
    } else {
        println!("Bucket {}: none.", s.bucket);
    }
    Ok(())
}
