//! `riff cloud` (01M4262DQ9RNFNJ07CRTSGEAM1): the settings of each
//! instance, and the `gcloud` calls of each command (R6, R46,
//! R134-R136, 01M3NJAZAQ3AKMAM0EGM7R3S89).
//!
//! Each test runs `riff cloud` in a clone of its own, with a copy of
//! `deploy/cloud`, and a fake `gcloud` on `PATH` that writes each call
//! to a log. No test reaches the cloud. The fake is a checked-in file:
//! a test never runs a file that it wrote (`ETXTBSY`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use isolated::Isolated;

/// The folder of the settings files of the repository.
fn settings_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/cloud")
}

/// The settings of the file `file` in `deploy/cloud`: each `KEY=VALUE`
/// line.
fn settings(file: &str) -> Vec<(String, String)> {
    fs::read_to_string(settings_dir().join(file))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// Each settings file in `deploy/cloud`.
fn settings_files() -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(settings_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".env"))
        .collect();
    files.sort();
    files
}

/// A clone with a copy of `deploy/cloud`, and the fake `gcloud`.
struct Cloud {
    env: Isolated,
    top: tempfile::TempDir,
    /// The describe calls that succeed: each line is the start of a call.
    found: Vec<String>,
    /// Each call fails as when the sign-in of `gcloud` ended.
    signin_ended: bool,
    /// Each call fails as when the account has no permission.
    denied: bool,
    /// The reply of each `get-iam-policy` call.
    iam: String,
}

/// The result of one `riff cloud` run.
struct Ran {
    out: Output,
    /// Each `gcloud` call, on one line.
    calls: String,
    /// Each argument of each call, on one line.
    args: String,
}

impl Ran {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.out.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.out.stderr).into_owned()
    }

    /// The first call that starts with `start`.
    fn line(&self, start: &str) -> &str {
        self.calls
            .lines()
            .find(|l| l.starts_with(start))
            .unwrap_or_else(|| panic!("no call {start}:\n{}", self.calls))
    }

    /// Asserts that the run passed, and gives it back.
    fn ok(self) -> Ran {
        assert!(self.out.status.success(), "{}", self.stderr());
        self
    }
}

impl Cloud {
    fn new() -> Cloud {
        let top = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(top.path())
            .status()
            .unwrap();
        assert!(status.success());
        let dir = top.path().join("deploy/cloud");
        fs::create_dir_all(&dir).unwrap();
        for file in settings_files() {
            fs::copy(settings_dir().join(&file), dir.join(&file)).unwrap();
        }
        Cloud {
            env: Isolated::new(),
            top,
            found: Vec::new(),
            signin_ended: false,
            denied: false,
            iam: String::new(),
        }
    }

    /// The reply of each `get-iam-policy` call.
    fn iam(mut self, policy: &str) -> Cloud {
        self.iam = policy.to_owned();
        self
    }

    /// The describe calls that succeed.
    fn found(mut self, found: &[&str]) -> Cloud {
        self.found = found.iter().map(|f| (*f).to_owned()).collect();
        self
    }

    /// Each call of `gcloud` fails as when its sign-in ended.
    fn signin_ended(mut self) -> Cloud {
        self.signin_ended = true;
        self
    }

    /// Each call of `gcloud` fails as when the account has no
    /// permission.
    fn denied(mut self) -> Cloud {
        self.denied = true;
        self
    }

    /// Sets `key` to `value` in the settings file `file`.
    fn set(self, file: &str, key: &str, value: &str) -> Cloud {
        let path = self.top.path().join("deploy/cloud").join(file);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, riff::cloud::set_line(&text, key, value)).unwrap();
        self
    }

    /// The text of the settings file `file` of the clone.
    fn text(&self, file: &str) -> String {
        fs::read_to_string(self.top.path().join("deploy/cloud").join(file)).unwrap()
    }

    /// `riff cloud ARGS` with the owner `owner@example.com`.
    fn run(&self, args: &[&str]) -> Ran {
        self.run_with(args, Some("owner@example.com"), "")
    }

    /// `riff cloud ARGS` with `RIFF_OWNER` set to `owner` or not set,
    /// and `input` on stdin.
    fn run_with(&self, args: &[&str], owner: Option<&str>, input: &str) -> Ran {
        let logs = tempfile::tempdir().unwrap();
        let log = logs.path().join("calls");
        let each = logs.path().join("args");
        let found = logs.path().join("found");
        fs::write(&found, self.found.join("\n") + "\n").unwrap();
        let iam = logs.path().join("iam");
        fs::write(&iam, &self.iam).unwrap();
        let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake");
        let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
        let mut cmd = self.env.riff();
        cmd.arg("cloud")
            .args(args)
            .current_dir(self.top.path())
            .env("PATH", path)
            .env("RIFF_USER", "mike")
            .env("FAKE_GCLOUD_LOG", &log)
            .env("FAKE_GCLOUD_ARGS", &each)
            .env("FAKE_GCLOUD_FOUND", &found)
            .env("FAKE_GCLOUD_IAM", &iam)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(owner) = owner {
            cmd.env("RIFF_OWNER", owner);
        }
        if self.signin_ended {
            cmd.env("FAKE_GCLOUD_SIGNIN_ENDED", "1");
        }
        if self.denied {
            cmd.env("FAKE_GCLOUD_DENIED", "1");
        }
        let mut child = cmd.spawn().unwrap();
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(input.as_bytes()).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        Ran {
            out,
            calls: fs::read_to_string(&log).unwrap_or_default(),
            args: fs::read_to_string(&each).unwrap_or_default(),
        }
    }

    /// Makes the clone a tree that Cloud Build can build: a
    /// `Dockerfile`, and a commit that changed the code.
    fn tree(self) -> Cloud {
        let top = self.top.path();
        fs::create_dir_all(top.join("crates")).unwrap();
        fs::write(top.join("Dockerfile"), "FROM scratch\n").unwrap();
        fs::write(top.join("crates/x"), "x\n").unwrap();
        for args in [
            &["add", "-A"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "x",
            ],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(top)
                .status()
                .unwrap();
            assert!(status.success());
        }
        self
    }
}

/// Each resource that `riff cloud create shared` makes, as its describe
/// call.
const SETUP: [&str; 7] = [
    "storage buckets describe gs://como-riff-state",
    "iam service-accounts describe riff-server@",
    "iam service-accounts describe riff-build@",
    "iam service-accounts describe riff-deploy@",
    "artifacts repositories describe riff ",
    "iam workload-identity-pools describe github ",
    "iam workload-identity-pools providers describe github ",
];

/// 01M3NJAZAQ3AKMAM0EGM7R3S89: only `main` and the tags `v*` of the
/// repository sign in.
const CONDITION: &str = "--attribute-condition assertion.repository == 'como-technologies/riff' && (assertion.ref == 'refs/heads/main' || assertion.ref.startsWith('refs/tags/v'))";

#[test]
fn create_makes_the_bucket_and_sets_the_rules() {
    let ran = Cloud::new().run(&["create", "shared"]).ok();
    let make = ran.line("storage buckets create gs://como-riff-state ");
    assert!(make.contains("--public-access-prevention"), "{make}");
    assert!(make.contains("--uniform-bucket-level-access"), "{make}");
    assert!(make.contains("--default-storage-class standard"), "{make}");
    let update = ran.line("storage buckets update gs://como-riff-state --lifecycle-file ");
    assert!(update.contains("--versioning"), "{update}");
}

#[test]
fn create_makes_the_service_accounts() {
    let ran = Cloud::new().run(&["create", "shared"]).ok();
    ran.line("iam service-accounts create riff-server ");
    ran.line("iam service-accounts create riff-build ");
    ran.line("iam service-accounts create riff-deploy ");
}

#[test]
fn the_server_account_gets_only_its_bucket_and_its_secret() {
    let ran = Cloud::new().found(&SETUP).run(&["create", "shared"]).ok();
    let account = "serviceAccount:riff-server@como-riff.iam.gserviceaccount.com";
    let bucket = ran.line("storage buckets add-iam-policy-binding gs://como-riff-state ");
    assert!(bucket.contains(&format!(
        "--member {account} --role roles/storage.objectUser"
    )));
    let binding = ran.line("secrets add-iam-policy-binding riff-oidc-client-secret ");
    assert!(binding.contains(&format!(
        "--member {account} --role roles/secretmanager.secretAccessor"
    )));
    let project = ran.line("projects add-iam-policy-binding como-riff ");
    assert!(project.contains("riff-build@"), "{project}");
    assert!(project.contains("--role roles/run.builder"), "{project}");
}

#[test]
fn create_again_keeps_each_resource() {
    let ran = Cloud::new().found(&SETUP).run(&["create", "shared"]).ok();
    for made in [
        "storage buckets create",
        "service-accounts create",
        "repositories create",
        "workload-identity-pools create",
        "providers create-oidc",
    ] {
        assert!(!ran.calls.contains(made), "{made} in:\n{}", ran.calls);
    }
    ran.line("storage buckets update gs://como-riff-state --lifecycle-file ");
    // A provider that exists gets the mapping and the condition again.
    let update = ran.line("iam workload-identity-pools providers update-oidc github ");
    assert!(update.contains(CONDITION), "{update}");
    assert!(update.contains(&format!("--attribute-mapping {MAPPING} ")), "{update}");
}

/// R46, and 01M3TJWJEPTSF1S3S5PJD25Z7Y: an older version of an object
/// goes after 7 days. riff carries the rules in its binary.
#[test]
fn the_rules_delete_thread_objects_after_30_days_and_older_versions_after_7() {
    let rules: serde_json::Value = serde_json::from_str(riff::cloud::LIFECYCLE).unwrap();
    assert_eq!(
        rules,
        serde_json::json!({"rule": [
            {
                "action": {"type": "Delete"},
                "condition": {"age": 30, "matchesPrefix": ["threads/"]},
            },
            {
                "action": {"type": "Delete"},
                "condition": {"daysSinceNoncurrentTime": 7},
            },
        ]})
    );
}

/// 01M3TJWJEPTSF1S3S5PJD25Z7Y: the service has 1 GiB of memory. The
/// deploy sets it, and a create sets it on a service that runs.
#[test]
fn the_service_gets_1_gib_of_memory() {
    let cloud = Cloud::new();
    let ran = cloud
        .run(&["deploy", "shared", "v1.0.0", "--confirm", "shared"])
        .ok();
    let deploy = ran.line("run deploy riff-server ");
    assert!(deploy.contains("--memory 1Gi"), "{deploy}");

    let ran = cloud.run(&["create", "shared"]).ok();
    assert!(!ran.calls.contains("run services update"), "{}", ran.calls);
    let ran = Cloud::new()
        .found(&["run services describe riff-server "])
        .run(&["create", "shared"])
        .ok();
    let update = ran.line("run services update riff-server ");
    assert!(update.contains("--memory 1Gi"), "{update}");
    assert!(update.contains("--region us-central1"), "{update}");
}

/// 01M3TJWJ6J3M6JRXJTAETZ5M6F: an alert on each error in the log of
/// the service goes to the owner by email.
#[test]
fn create_makes_the_alert_for_the_owner() {
    let ran = Cloud::new().run(&["create", "shared"]).ok();
    let channel = ran.line("beta monitoring channels create ");
    assert!(channel.contains("--type email"), "{channel}");
    assert!(
        channel.contains("--channel-labels email_address=owner@example.com"),
        "{channel}"
    );
    let policy = ran.line("alpha monitoring policies create ");
    assert!(policy.contains("--policy-from-file "), "{policy}");
    assert!(policy.contains("--notification-channels"), "{policy}");
    assert!(
        ran.calls.contains("monitoring.googleapis.com"),
        "{}",
        ran.calls
    );

    // A create again keeps the channel and the alert.
    let found = [
        "beta monitoring channels list ",
        "alpha monitoring policies list ",
    ];
    let ran = Cloud::new().found(&found).run(&["create", "shared"]).ok();
    assert!(
        !ran.calls.contains("monitoring channels create"),
        "{}",
        ran.calls
    );
    assert!(
        !ran.calls.contains("monitoring policies create"),
        "{}",
        ran.calls
    );
}

#[test]
fn create_with_no_owner_makes_no_alert_and_says_how() {
    let ran = Cloud::new().run_with(&["create", "shared"], None, "").ok();
    assert!(!ran.calls.contains("monitoring channels"), "{}", ran.calls);
    assert!(!ran.calls.contains("monitoring policies"), "{}", ran.calls);
    assert!(
        ran.stdout()
            .contains("RIFF_OWNER=YOUR_EMAIL riff cloud create shared"),
        "{}",
        ran.stdout()
    );
}

/// The alert of an instance fires on a log line of its own service with
/// the severity ERROR or more, and has the name that `create` looks
/// for.
#[test]
fn the_alert_matches_each_error_of_the_service() {
    let values: std::collections::BTreeMap<String, String> =
        settings("shared.env").into_iter().collect();
    let text = fs::read_to_string(settings_dir().join("shared.env")).unwrap();
    let s = riff::cloud::Settings::parse("shared", &text).unwrap();
    let alert: serde_json::Value = serde_json::from_str(&riff::cloud::alert(&s)).unwrap();
    assert_eq!(alert["displayName"], values["CLOUD_ALERT"].as_str());
    let filter = alert["conditions"][0]["conditionMatchedLog"]["filter"]
        .as_str()
        .unwrap();
    assert!(filter.contains("severity>=ERROR"), "{filter}");
    let service = format!(
        "resource.labels.service_name=\"{}\"",
        values["CLOUD_SERVICE"]
    );
    assert!(filter.contains(&service), "{filter}");
    // A log alert needs a rate limit.
    assert!(alert["alertStrategy"]["notificationRateLimit"]["period"].is_string());
}

#[test]
fn only_main_and_the_release_tags_sign_in_as_the_deploy_account() {
    let ran = Cloud::new().run(&["create", "shared"]).ok();
    let provider = ran.line("iam workload-identity-pools providers create-oidc github ");
    assert!(
        provider.contains("--issuer-uri https://token.actions.githubusercontent.com"),
        "{provider}"
    );
    assert!(provider.contains(CONDITION), "{provider}");
    assert!(provider.contains(&format!("--attribute-mapping {MAPPING} ")), "{provider}");
    assert!(
        !ran.calls.contains("providers update-oidc"),
        "{}",
        ran.calls
    );
    let user = ran.line("iam service-accounts add-iam-policy-binding riff-deploy@");
    assert!(user.contains(&format!("--member {PRODUCTION} ")), "{user}");
    assert!(
        user.contains("--role roles/iam.workloadIdentityUser"),
        "{user}"
    );
}

/// The member of the pool for each job of the repository in the GitHub
/// environment `production`. It is not the subject: this repository
/// has the immutable subject of GitHub, with the IDs of the owner and
/// the repository.
const PRODUCTION: &str = "principalSet://iam.googleapis.com/projects/816917641970/locations/global/workloadIdentityPools/github/attribute.environment/production";

/// The attribute mapping of the provider: it maps the claim
/// `environment` of a job.
const MAPPING: &str = "google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref,attribute.environment=assertion.environment";

/// The member of the pool for each job of the repository.
const REPOSITORY: &str = "principalSet://iam.googleapis.com/projects/816917641970/locations/global/workloadIdentityPools/github/attribute.repository/como-technologies/riff";

/// 01M49M8W30M2084QN4HX1FJFKS: the shared deploy account takes only a
/// job in the environment `production`, and the stage deploy account
/// only a job in the environment `stage`. No deploy account takes each
/// job of the repository.
#[test]
fn only_a_job_in_its_github_environment_signs_in_as_a_deploy_account() {
    for (name, account, environment) in [
        ("shared", "riff-deploy@", "production"),
        ("stage", "riff-stage-deploy@", "stage"),
    ] {
        let ran = Cloud::new().run(&["create", name]).ok();
        let users: Vec<&str> = ran
            .calls
            .lines()
            .filter(|l| l.contains("--role roles/iam.workloadIdentityUser"))
            .collect();
        assert_eq!(users.len(), 1, "{users:#?}");
        let member = format!(
            "--member principalSet://iam.googleapis.com/projects/816917641970/locations/global/workloadIdentityPools/github/attribute.environment/{environment} "
        );
        assert!(
            users[0].starts_with(&format!(
                "iam service-accounts add-iam-policy-binding {account}"
            )) && users[0].contains(&member),
            "{}",
            users[0]
        );
        assert!(
            !ran.calls.contains("attribute.repository/"),
            "{}",
            ran.calls
        );
        assert!(
            ran.stdout().contains(&format!(
                "CI deploy: set, for a job in the GitHub environment {environment}."
            )),
            "{}",
            ran.stdout()
        );
    }
}

/// `create` removes the old binding that let each job of the
/// repository sign in as the deploy account.
#[test]
fn create_removes_the_sign_in_of_each_job_of_the_repository() {
    let policy = format!(
        r#"{{"bindings":[{{"members":["{REPOSITORY}"],"role":"roles/iam.workloadIdentityUser"}}]}}"#
    );
    let ran = Cloud::new()
        .found(&SETUP)
        .iam(&policy)
        .run(&["create", "shared"])
        .ok();
    let get = ran.line("iam service-accounts get-iam-policy riff-deploy@");
    assert!(get.contains("--format json"), "{get}");
    let remove = ran.line("iam service-accounts remove-iam-policy-binding riff-deploy@");
    assert!(
        remove.contains(&format!(
            "--member {REPOSITORY} --role roles/iam.workloadIdentityUser"
        )),
        "{remove}"
    );
    // The new binding comes first, so the deploy never has no member.
    let add = ran.calls.find(&format!("--member {PRODUCTION} ")).unwrap();
    assert!(add < ran.calls.find("remove-iam-policy-binding").unwrap());

    // With no old binding, nothing is removed.
    let ran = Cloud::new().found(&SETUP).run(&["create", "shared"]).ok();
    assert!(
        !ran.calls.contains("remove-iam-policy-binding"),
        "{}",
        ran.calls
    );
}

/// An instance with a deploy account and no GitHub environment makes
/// no CI deploy, and says what to set.
#[test]
fn a_deploy_account_needs_a_github_environment() {
    let ran = Cloud::new()
        .set("stage.env", "CLOUD_GITHUB_ENVIRONMENT", "")
        .run(&["create", "stage"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr()
            .contains("stage.env has a CLOUD_DEPLOY_ACCOUNT and no CLOUD_GITHUB_ENVIRONMENT"),
        "{}",
        ran.stderr()
    );
    assert!(!ran.calls.contains("workloadIdentityUser"), "{}", ran.calls);
}

#[test]
fn the_deploy_account_pushes_images_and_deploys_the_service() {
    let ran = Cloud::new().found(&SETUP).run(&["create", "shared"]).ok();
    let account = "serviceAccount:riff-deploy@como-riff.iam.gserviceaccount.com";
    let repository = ran.line("artifacts repositories add-iam-policy-binding riff ");
    assert!(repository.contains(&format!(
        "--member {account} --role roles/artifactregistry.writer"
    )));
    let roles: Vec<&str> = ran
        .calls
        .lines()
        .filter(|l| l.contains(&format!("--member {account} ")))
        .collect();
    assert_eq!(roles.len(), 3, "{roles:#?}");
    assert!(roles.iter().any(
        |l| l.starts_with("projects add-iam-policy-binding como-riff ")
            && l.contains("--role roles/run.admin")
    ));
    assert!(roles.iter().any(|l| {
        l.starts_with("iam service-accounts add-iam-policy-binding riff-server@")
            && l.contains("--role roles/iam.serviceAccountUser")
    }));
}

/// A new instance gets a settings file whose resources take the name
/// of the instance, and riff makes them (01M4262DSKVWP4064FKSMAQACZ).
#[test]
fn create_of_a_new_instance_writes_its_settings_and_makes_its_resources() {
    let cloud = Cloud::new();
    let ran = cloud
        .run(&[
            "create",
            "team",
            "--project",
            "acme",
            "--region",
            "europe-west1",
        ])
        .ok();
    ran.line("projects describe acme --format value(projectNumber)");
    let text = cloud.text("team.env");
    for line in [
        "CLOUD_PROJECT=acme\n",
        "CLOUD_PROJECT_NUMBER=123456\n",
        "CLOUD_SERVICE=team\n",
        "CLOUD_BUCKET=acme-team-state\n",
        "CLOUD_URL=https://team-123456.europe-west1.run.app\n",
        "CLOUD_DEPLOY_ACCOUNT=\n",
        "RIFF_OIDC_CLIENT_ID=\n",
    ] {
        assert!(text.contains(line), "{line:?} is not in:\n{text}");
    }
    ran.line("storage buckets create gs://acme-team-state --location europe-west1 ");
    ran.line("iam service-accounts create team-server ");
    assert!(!ran.calls.contains("workload-identity"), "{}", ran.calls);
    assert!(!ran.calls.contains("monitoring channels"), "{}", ran.calls);
    assert!(
        ran.stdout().contains("Run: riff cloud signin team"),
        "{}",
        ran.stdout()
    );

    // Again: the settings stay, and another project is refused.
    let again = cloud.run(&["create", "team"]).ok();
    assert!(
        !again.calls.contains("value(projectNumber)"),
        "{}",
        again.calls
    );
    assert_eq!(cloud.text("team.env"), text);
    let other = cloud.run(&["create", "team", "--project", "other"]);
    assert!(!other.out.status.success());
    assert!(
        other.stderr().contains("have acme, not other"),
        "{}",
        other.stderr()
    );
    assert!(other.calls.is_empty(), "{}", other.calls);
}

#[test]
fn create_of_a_new_instance_needs_its_project_and_region() {
    let cloud = Cloud::new();
    let ran = cloud.run(&["create", "team", "--project", "acme"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("--project PROJECT --region REGION"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
    for name in ["Team", "v1.0.0", "a-name-that-is-too-long"] {
        let ran = cloud.run(&["create", name, "--project", "acme", "--region", "r"]);
        assert!(!ran.out.status.success(), "{name}");
        assert!(
            ran.stderr().contains("is no name of a riff instance"),
            "{}",
            ran.stderr()
        );
    }
}

#[test]
fn deploy_runs_one_instance_with_sign_in() {
    let ran = Cloud::new()
        .run(&["deploy", "shared", "v1.0.0", "--confirm", "shared"])
        .ok();
    let deploy = ran.line("run deploy riff-server --image ");
    for flag in [
        "--image us-central1-docker.pkg.dev/como-riff/riff/riff-server:v1.0.0 ",
        "--region us-central1",
        "--service-account riff-server@como-riff.iam.gserviceaccount.com",
        "--min-instances 1 --max-instances 1 --no-cpu-throttling",
        "--concurrency 1000 --timeout 3600",
        "RIFF_PUBLIC_URL=https://riff-server-816917641970.us-central1.run.app,",
        "RIFF_REQUIRE_SIGN_IN=true,",
        "RIFF_BUCKET=como-riff-state,",
        "RIFF_OWNER=owner@example.com",
        "--set-secrets RIFF_OIDC_CLIENT_SECRET=riff-oidc-client-secret:latest",
    ] {
        assert!(deploy.contains(flag), "{flag} is not in: {deploy}");
    }
    assert!(!deploy.contains("--source"), "{deploy}");
    assert!(!ran.calls.contains("domain-mappings"), "{}", ran.calls);
}

/// With no tag, Cloud Build builds the tree of this directory, with the
/// build in `build-id.env`; riff removes that file after the deploy.
#[test]
fn deploy_with_no_tag_builds_this_tree() {
    // No Dockerfile: no tree to build.
    let ran = Cloud::new().run(&["deploy", "shared", "--confirm", "shared"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("has no Dockerfile"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);

    let cloud = Cloud::new().tree();
    let ran = cloud.run(&["deploy", "shared", "--confirm", "shared"]).ok();
    let deploy = ran.line("run deploy riff-server --source ");
    assert!(
        deploy.contains("--build-service-account projects/como-riff/serviceAccounts/riff-build@"),
        "{deploy}"
    );
    assert!(!cloud.top.path().join("build-id.env").exists());
}

/// The cloud riff needs an owner (01M3JN3ASSV9SA0QZKXXJ0RTEV). With
/// no `RIFF_OWNER`, the deploy stops before it calls gcloud.
#[test]
fn deploy_stops_with_no_owner() {
    let ran = Cloud::new().run_with(&["deploy", "stage", "v1.0.0"], None, "");
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("export RIFF_OWNER=YOUR_EMAIL"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
}

#[test]
fn deploy_maps_the_domain_once_when_the_url_is_the_domain() {
    let cloud = Cloud::new()
        .set(
            "shared.env",
            "CLOUD_URL",
            "https://riff.comotechnologies.io",
        )
        .set("shared.env", "CLOUD_CONFIRM", "false")
        .tree();
    let ran = cloud.run(&["deploy", "shared"]).ok();
    let deploy = ran.line("run deploy riff-server ");
    assert!(deploy.contains("RIFF_PUBLIC_URL=https://riff.comotechnologies.io,"));
    let map = ran.line("beta run domain-mappings create --service riff-server ");
    assert!(map.contains("--domain riff.comotechnologies.io"), "{map}");
    assert!(map.contains("--region us-central1"), "{map}");

    let cloud = cloud.found(&["beta run domain-mappings describe"]);
    let ran = cloud.run(&["deploy", "shared"]).ok();
    assert!(
        !ran.calls.contains("domain-mappings create"),
        "{}",
        ran.calls
    );
    // A tag deploys an image, and maps nothing.
    let ran = cloud.run(&["deploy", "shared", "v1.0.0"]).ok();
    assert!(!ran.calls.contains("domain-mappings"), "{}", ran.calls);
}

#[test]
fn deploy_refuses_a_tag_that_is_not_a_release_tag() {
    let cloud = Cloud::new().set(
        "stage.env",
        "RIFF_OIDC_CLIENT_ID",
        "1-a.apps.googleusercontent.com",
    );
    for tag in ["0.8.0", "v0.8", "V0.8.0", "v1.0.0-rc1"] {
        let ran = cloud.run(&["deploy", "stage", tag]);
        assert!(!ran.out.status.success(), "{tag}");
        assert!(
            ran.stderr().contains("is not a release tag"),
            "{tag}: {}",
            ran.stderr()
        );
        assert!(ran.calls.is_empty(), "{}", ran.calls);
    }
}

/// The stage has no client ID until `riff cloud signin stage`.
#[test]
fn deploy_with_no_client_says_how_to_store_it() {
    let ran = Cloud::new().run(&["deploy", "stage", "v1.0.0"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr()
            .contains("The settings stage have no client ID. Run: riff cloud signin stage"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
}

/// The shared riff asks for its name before a deploy; the stage does
/// not. With no terminal, only `--confirm NAME` gives it
/// (01M4262DVY8QCSZS61VDQ61SB3).
#[test]
fn deploy_to_the_shared_riff_asks_for_the_name() {
    let cloud = Cloud::new();
    let ran = cloud.run(&["deploy", "shared", "v1.0.0"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("add: --confirm shared"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
    let ran = cloud.run(&["deploy", "shared", "v1.0.0", "--confirm", "stage"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("riff changed nothing"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);

    let stage = Cloud::new().set(
        "stage.env",
        "RIFF_OIDC_CLIENT_ID",
        "1-a.apps.googleusercontent.com",
    );
    stage.run(&["deploy", "stage", "v1.0.0"]).ok();
}

/// 01M496JTN962N0AX378MA1MBPM: no setting of one riff names the bucket,
/// the service, the secret or another resource of a different riff. So
/// the stage cannot touch the data of the shared riff.
#[test]
fn no_settings_of_one_riff_name_a_resource_of_another() {
    const RESOURCES: [&str; 11] = [
        "CLOUD_SECRET",
        "CLOUD_BUCKET",
        "CLOUD_SERVICE",
        "CLOUD_URL",
        "CLOUD_DOMAIN",
        "CLOUD_ALERT",
        "CLOUD_RUN_ACCOUNT",
        "CLOUD_BUILD_ACCOUNT",
        "CLOUD_DEPLOY_ACCOUNT",
        "CLOUD_ALERT_CHANNEL",
        "RIFF_OIDC_CLIENT_ID",
    ];
    let files = settings_files();
    assert_eq!(files, ["shared.env", "stage.env"]);
    for a in &files {
        for b in files.iter().filter(|b| *b != a) {
            let theirs = settings(b);
            for (key, value) in settings(a) {
                if !RESOURCES.contains(&key.as_str()) || value.is_empty() {
                    continue;
                }
                for (other, text) in &theirs {
                    assert!(
                        !text.contains(&value),
                        "{other}={text} in {b} names {key}={value} of {a}"
                    );
                }
            }
        }
    }
}

/// Each settings file has each key, in the same order, also when its
/// value is empty.
#[test]
fn each_settings_file_has_each_key() {
    let keys = |file: &str| -> Vec<String> { settings(file).into_iter().map(|(k, _)| k).collect() };
    let s = riff::cloud::Settings::new("x", "p", "1", "r");
    let all: Vec<String> = s.pairs().iter().map(|(k, _)| (*k).to_owned()).collect();
    for file in settings_files() {
        assert_eq!(keys(&file), all, "{file}");
    }
}

/// The create of the stage makes only the resources of the stage, and
/// the CI deploy of the stage as its own account
/// (01M496JT94NQ686GVSY5CCGZK7). It makes no alert.
#[test]
fn create_of_the_stage_makes_only_the_resources_of_the_stage() {
    let ran = Cloud::new().run(&["create", "stage"]).ok();
    ran.line("storage buckets create gs://como-riff-stage-state ");
    ran.line("iam service-accounts create riff-stage-server ");
    ran.line("iam service-accounts create riff-stage-build ");
    let binding = ran.line("secrets add-iam-policy-binding riff-stage-oidc-client-secret ");
    assert!(binding.contains("riff-stage-server@"), "{binding}");
    for shared in [
        "como-riff-state",
        "riff-oidc-client-secret",
        "riff-server@",
        "riff-build@",
        "riff-deploy@",
        "monitoring channels",
        "monitoring policies",
        "run services describe riff-server ",
    ] {
        assert!(!ran.calls.contains(shared), "{shared} in:\n{}", ran.calls);
    }
    let user = ran.line("iam service-accounts add-iam-policy-binding riff-stage-deploy@");
    assert!(
        user.contains("--role roles/iam.workloadIdentityUser"),
        "{user}"
    );
    // The stage needs a client of its own.
    assert!(
        ran.stdout().contains("riff cloud signin stage"),
        "{}",
        ran.stdout()
    );
}

/// The deploy of the stage runs on the service, bucket and secret of
/// the stage only.
#[test]
fn deploy_of_the_stage_uses_only_the_resources_of_the_stage() {
    let cloud = Cloud::new().set(
        "stage.env",
        "RIFF_OIDC_CLIENT_ID",
        "1-a.apps.googleusercontent.com",
    );
    let ran = cloud.run(&["deploy", "stage", "v0.8.0"]).ok();
    let deploy = ran.line("run deploy riff-stage --image ");
    for flag in [
        "--image us-central1-docker.pkg.dev/como-riff/riff/riff-server:v0.8.0 ",
        "--service-account riff-stage-server@como-riff.iam.gserviceaccount.com",
        "RIFF_PUBLIC_URL=https://riff-stage-816917641970.us-central1.run.app,",
        "RIFF_BUCKET=como-riff-stage-state,",
        "RIFF_OIDC_CLIENT_ID=1-a.apps.googleusercontent.com,",
        "--set-secrets RIFF_OIDC_CLIENT_SECRET=riff-stage-oidc-client-secret:latest",
    ] {
        assert!(deploy.contains(flag), "{flag} is not in: {deploy}");
    }
    for shared in ["como-riff-state", "riff-server ", "riff-oidc-client-secret"] {
        assert!(!ran.calls.contains(shared), "{shared} in:\n{}", ran.calls);
    }
}

/// 01M496JTDB16G52G22CJZRA8J0, 01M496JT648QS9WTE1QVHAJEE5: the stage
/// takes the image of a commit and scales to zero. The shared riff takes
/// only a release tag, and keeps one instance.
#[test]
fn the_stage_takes_the_image_of_a_commit_and_scales_to_zero() {
    let commit = "183456a0c4e2b1f3d5a6978877665544332211ff";
    let cloud = Cloud::new().set(
        "stage.env",
        "RIFF_OIDC_CLIENT_ID",
        "1-a.apps.googleusercontent.com",
    );
    let ran = cloud.run(&["deploy", "stage", commit]).ok();
    let deploy = ran.line("run deploy riff-stage --image ");
    for flag in [
        format!("--image us-central1-docker.pkg.dev/como-riff/riff/riff-server:{commit} "),
        "--min-instances 0 --max-instances 1 ".to_owned(),
    ] {
        assert!(deploy.contains(&flag), "{flag} is not in: {deploy}");
    }

    let ran = cloud.run(&["deploy", "shared", commit, "--confirm", "shared"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr()
            .contains("shared takes only a release tag vX.Y.Z"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);

    let ran = cloud
        .run(&["deploy", "shared", "v1.0.0", "--confirm", "shared"])
        .ok();
    let deploy = ran.line("run deploy riff-server --image ");
    assert!(
        deploy.contains("--min-instances 1 --max-instances 1 "),
        "{deploy}"
    );

    // A short commit ID is no tag.
    let ran = cloud.run(&["deploy", "stage", &commit[..12]]);
    assert!(!ran.out.status.success());
    assert!(ran.calls.is_empty(), "{}", ran.calls);
}

/// `riff cloud smoke` needs the refresh token of the test account in
/// its variable, and calls no `gcloud` (01M496JTHN19BZ7YN94993R35X).
#[test]
fn the_smoke_test_with_no_token_says_what_it_needs() {
    let ran = Cloud::new().run(&["smoke", "stage"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains(riff::text::SMOKE_NO_TOKEN),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
}

#[test]
fn a_name_with_no_settings_file_stops_each_command() {
    let cloud = Cloud::new();
    for args in [
        &["create", "nosuch"][..],
        &["signin", "nosuch"],
        &["deploy", "nosuch", "v1.0.0"],
        &["status", "nosuch"],
        &["log", "nosuch"],
        &["delete", "nosuch", "--confirm", "nosuch"],
    ] {
        let ran = cloud.run(args);
        assert!(!ran.out.status.success(), "{args:?}");
        let stderr = ran.stderr();
        assert!(
            stderr.contains("nosuch.env is not there") || stderr.contains("--project PROJECT"),
            "{args:?}: {stderr}"
        );
        assert!(ran.calls.is_empty(), "{args:?}: {}", ran.calls);
    }
}

/// `riff cloud signin` stores the secret in Secret Manager through
/// stdin, and the client ID in the settings file. The secret goes to no
/// file.
#[test]
fn signin_stores_the_secret_and_writes_the_client_id() {
    let cloud = Cloud::new();
    let id = "1-a.apps.googleusercontent.com";
    let ran = cloud
        .run_with(&["signin", "stage"], None, &format!("{id}\nthe-secret\n"))
        .ok();
    assert!(
        ran.stdout()
            .contains("console.cloud.google.com/auth/overview?project=como-riff")
    );
    let add = ran.line("secrets versions add riff-stage-oidc-client-secret --data-file=- ");
    assert!(!add.contains("the-secret"), "{add}");
    assert!(
        cloud
            .text("stage.env")
            .contains(&format!("\nRIFF_OIDC_CLIENT_ID={id}\n"))
    );
    assert!(!cloud.text("stage.env").contains("the-secret"));

    let ran = cloud.run_with(&["signin", "stage"], None, "not-an-id\nx\n");
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains(".apps.googleusercontent.com"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);
}

#[test]
fn log_reads_the_lines_of_the_service() {
    let cloud = Cloud::new();
    let filter = r#"jsonPayload.message:"imported the objects""#;
    let ran = cloud.run(&["log", "shared", "--filter", filter]).ok();
    let args: Vec<&str> = ran.args.lines().collect();
    assert_eq!(
        &args[..5],
        ["run", "services", "logs", "read", "riff-server"]
    );
    assert!(args.contains(&filter), "{args:?}");
    assert!(args.windows(2).any(|w| w == ["--limit", "50"]), "{args:?}");

    let ran = cloud
        .run(&["log", "stage", "--errors", "--limit", "20"])
        .ok();
    let args: Vec<&str> = ran.args.lines().collect();
    assert_eq!(args[4], "riff-stage", "{args:?}");
    assert!(args.windows(2).any(|w| w == ["--limit", "20"]), "{args:?}");
    assert!(args.contains(&"severity>=ERROR"), "{args:?}");
}

#[test]
fn list_and_status_show_each_instance() {
    // A dead URL: the test never reaches a riff.
    let cloud = Cloud::new()
        .set("shared.env", "CLOUD_URL", "http://127.0.0.1:9")
        .set("stage.env", "CLOUD_URL", "http://127.0.0.1:9")
        .found(&["run services describe riff-stage "]);
    let ran = cloud.run(&["list"]).ok();
    let out = ran.stdout();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "shared  http://127.0.0.1:9  -  no service  paused ?",
            "stage  http://127.0.0.1:9  v1.0.0  ready  paused ?",
        ],
        "{out}"
    );
    let ran = cloud.run(&["status", "stage"]).ok();
    let out = ran.stdout();
    assert!(out.contains("revision: rev-1\n"), "{out}");
    assert!(out.contains("bucket: como-riff-stage-state\n"), "{out}");
    assert!(
        out.contains(
            "CI deploys: each merge to main, when the GitHub variable STAGE_DEPLOY is true"
        ),
        "{out}"
    );
}

/// A failed call of `gcloud` gives its error in one line and a code
/// that is not 0. It never gives "no service" (01M4382RKERWAPKBRY9W8F2GSA).
fn a_failed_gcloud_is_an_error_and_no_service_is_not_said(cloud: Cloud, line: &str) {
    let cloud = cloud
        .set("shared.env", "CLOUD_URL", "http://127.0.0.1:9")
        .set("stage.env", "CLOUD_URL", "http://127.0.0.1:9")
        .found(&["run services describe riff-stage "]);
    for args in [
        &["list"][..],
        &["status", "stage"],
        &["delete", "stage", "--with-state", "--confirm", "stage"],
        &["create", "stage"],
    ] {
        let ran = cloud.run(args);
        assert!(!ran.out.status.success(), "{args:?}");
        let stderr = ran.stderr();
        let lines: Vec<&str> = stderr.lines().filter(|l| l.contains("gcloud")).collect();
        assert_eq!(lines.len(), 1, "{args:?}: one line names gcloud:\n{stderr}");
        assert!(lines[0].contains(line), "{args:?}: {stderr}");
        let stdout = ran.stdout();
        assert!(!stdout.contains("no service"), "{args:?}: {stdout}");
        assert!(!stdout.contains(": none"), "{args:?}: {stdout}");
        assert!(!stdout.contains("making it"), "{args:?}: {stdout}");
        assert!(!ran.calls.contains(" delete "), "{args:?}: {}", ran.calls);
        assert!(!ran.calls.contains(" create "), "{args:?}: {}", ran.calls);
    }
}

#[test]
fn an_ended_sign_in_of_gcloud_is_an_error_and_no_service_is_not_said() {
    a_failed_gcloud_is_an_error_and_no_service_is_not_said(
        Cloud::new().signin_ended(),
        "gcloud: the sign-in ended: run gcloud auth login",
    );
}

/// Cloud Run says "or resource may not exist" when the account has no
/// permission. riff does not take it as "no service".
#[test]
fn a_refused_permission_is_an_error_and_no_service_is_not_said() {
    a_failed_gcloud_is_an_error_and_no_service_is_not_said(
        Cloud::new().denied(),
        ": PERMISSION_DENIED: Permission 'run.services.get' denied",
    );
}

/// "no service" comes only from a reply of `gcloud` that the service
/// does not exist (01M4382RKERWAPKBRY9W8F2GSA).
#[test]
fn no_service_comes_only_from_a_reply_that_the_service_does_not_exist() {
    let cloud = Cloud::new().set("shared.env", "CLOUD_URL", "http://127.0.0.1:9");
    let ran = cloud.run(&["status", "shared"]).ok();
    assert!(
        ran.stdout()
            .starts_with("shared  http://127.0.0.1:9  -  no service  ")
    );
    assert!(ran.stderr().is_empty(), "{}", ran.stderr());
}

#[test]
fn list_with_no_settings_says_how_to_make_one() {
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    let out = env
        .riff()
        .args(["cloud", "list"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("riff cloud create NAME --project PROJECT --region REGION"),
        "{stdout}"
    );
    // Outside a repository with deploy/cloud, the settings are beside
    // the riff settings of the machine.
    assert!(
        stdout.contains(&env.riff_home().join("cloud").display().to_string()),
        "{stdout}"
    );
}

/// `riff cloud delete` asks for the name, deletes the service, and
/// keeps the bucket unless `--with-state`.
#[test]
fn delete_asks_for_the_name_and_keeps_the_state() {
    let cloud = Cloud::new().found(&[
        "run services describe riff-stage ",
        "storage buckets describe gs://como-riff-stage-state",
    ]);
    let ran = cloud.run(&["delete", "stage"]);
    assert!(!ran.out.status.success());
    assert!(
        ran.stderr().contains("add: --confirm stage"),
        "{}",
        ran.stderr()
    );
    assert!(ran.calls.is_empty(), "{}", ran.calls);

    let ran = cloud.run(&["delete", "stage", "--confirm", "stage"]).ok();
    ran.line("run services delete riff-stage --quiet ");
    assert!(!ran.calls.contains("storage rm"), "{}", ran.calls);
    assert!(
        ran.stdout().contains("add --with-state"),
        "{}",
        ran.stdout()
    );

    let ran = cloud
        .run(&["delete", "stage", "--with-state", "--confirm", "stage"])
        .ok();
    ran.line("storage rm --recursive gs://como-riff-stage-state ");
    assert!(cloud.text("stage.env").contains("CLOUD_SERVICE=riff-stage"));
}

/// A worker never runs `riff cloud` (01M4262DY8NN30SC4REYX2G9DV).
#[test]
fn a_worker_never_runs_riff_cloud() {
    let cloud = Cloud::new();
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("calls");
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake");
    let out = cloud
        .env
        .riff()
        .args(["cloud", "list"])
        .current_dir(cloud.top.path())
        .env(
            "PATH",
            format!("{}:{}", fake.display(), std::env::var("PATH").unwrap()),
        )
        .env("FAKE_GCLOUD_LOG", &log)
        .env("RIFF_WORKER", "1")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("a worker never runs riff cloud"));
    assert!(!log.exists());
}
