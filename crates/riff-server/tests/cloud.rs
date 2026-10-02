//! The cloud scripts in `deploy/`, the image and the CI deploy (R6,
//! R46, R134-R136, 01M3NJAZ6BYH7TWKDYTVEK78PG, 01M3NJAZAQ3AKMAM0EGM7R3S89).
//! The tests run each script with a fake `gcloud` that writes each call
//! to a log.
//!
//! The tests never run a file that they wrote. A test that writes a
//! file holds it open for a short time, and a parallel test can fork
//! then. Linux does not run a file that a process holds open for
//! writing (`ETXTBSY`).

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

fn deploy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy")
}

/// Runs a script with a fake `gcloud`. A `describe` call succeeds only
/// when it starts with one of `found`. Returns the calls, one on each
/// line.
fn run(script: &str, found: &[&str]) -> String {
    run_with(script, &[], found)
}

/// Runs a script with arguments. See [`run`].
fn run_with(script: &str, args: &[&str], found: &[&str]) -> String {
    run_in(&deploy(), script, args, found)
}

/// A copy of `deploy/` whose `cloud.env` sets `CLOUD_URL` to `url`.
/// Each other file is a link to the file in `deploy/`.
fn deploy_with_url(url: &str) -> tempfile::TempDir {
    deploy_with("cloud.env", "CLOUD_URL", url)
}

/// A copy of `deploy/` whose settings file `file` sets `key` to
/// `value`. Each other file is a link to the file in `deploy/`.
fn deploy_with(file: &str, key: &str, value: &str) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let dir = copy.path().join("deploy");
    fs::create_dir(&dir).unwrap();
    for entry in fs::read_dir(deploy()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        if name != file {
            symlink(&path, dir.join(name)).unwrap();
        }
    }
    let env = fs::read_to_string(deploy().join(file)).unwrap();
    let start = format!("{key}=");
    let env: Vec<String> = env
        .lines()
        .map(|l| {
            if l.starts_with(&start) {
                format!("{key}={value}")
            } else {
                l.to_owned()
            }
        })
        .collect();
    fs::write(dir.join(file), env.join("\n") + "\n").unwrap();
    copy
}

/// The settings of the file `file` in `deploy/`: each `KEY=VALUE` line.
fn settings(file: &str) -> Vec<(String, String)> {
    fs::read_to_string(deploy().join(file))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// Each settings file of a riff in `deploy/`: `cloud.env` and the
/// others, for example `stage.env`.
fn settings_files() -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(deploy())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".env"))
        .collect();
    files.sort();
    files
}

/// Runs a script from the directory `scripts`. See [`run`]. The deploy
/// gets the owner `owner@example.com`.
fn run_in(scripts: &Path, script: &str, args: &[&str], found: &[&str]) -> String {
    let out = command(scripts, script, args, found, Some("owner@example.com"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.log
}

/// The result of a script: its exit status, its output and the calls.
struct Ran {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    log: String,
}

/// Runs a script with a fake `gcloud`, and with `RIFF_OWNER` set to
/// `owner` or not set.
fn command(
    scripts: &Path,
    script: &str,
    args: &[&str],
    found: &[&str],
    owner: Option<&str>,
) -> Ran {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls");
    let found_file = dir.path().join("found");
    fs::write(&found_file, found.join("\n") + "\n").unwrap();
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake");
    let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
    let mut command = Command::new(scripts.join(script));
    command
        .args(args)
        .env("PATH", path)
        .env("FAKE_GCLOUD_LOG", &log)
        .env("FAKE_GCLOUD_FOUND", &found_file)
        .env_remove("RIFF_OWNER");
    if let Some(owner) = owner {
        command.env("RIFF_OWNER", owner);
    }
    let out = command.output().unwrap();
    Ran {
        status: out.status,
        stdout: out.stdout,
        stderr: out.stderr,
        log: fs::read_to_string(log).unwrap_or_default(),
    }
}

/// Each resource that `cloud-setup.sh` makes, as its describe call.
const SETUP: [&str; 7] = [
    "storage buckets describe gs://como-riff-state",
    "iam service-accounts describe riff-server@",
    "iam service-accounts describe riff-build@",
    "iam service-accounts describe riff-deploy@",
    "artifacts repositories describe riff ",
    "iam workload-identity-pools describe github ",
    "iam workload-identity-pools providers describe github ",
];

fn line<'a>(calls: &'a str, start: &str) -> &'a str {
    calls
        .lines()
        .find(|l| l.starts_with(start))
        .unwrap_or_else(|| panic!("no call {start}:\n{calls}"))
}

#[test]
fn setup_makes_the_bucket_and_sets_the_rule() {
    let calls = run("cloud-setup.sh", &[]);
    let make = line(&calls, "storage buckets create gs://como-riff-state ");
    assert!(make.contains("--public-access-prevention"), "{make}");
    assert!(make.contains("--uniform-bucket-level-access"), "{make}");
    assert!(
        calls.contains(
            "storage buckets update gs://como-riff-state --lifecycle-file lifecycle.json"
        )
    );
}

#[test]
fn setup_makes_the_service_accounts() {
    let calls = run("cloud-setup.sh", &[]);
    line(&calls, "iam service-accounts create riff-server ");
    line(&calls, "iam service-accounts create riff-build ");
    line(&calls, "iam service-accounts create riff-deploy ");
}

#[test]
fn the_server_account_gets_only_its_bucket_and_its_secret() {
    let calls = run("cloud-setup.sh", &SETUP);
    let account = "serviceAccount:riff-server@como-riff.iam.gserviceaccount.com";
    let bucket = line(
        &calls,
        "storage buckets add-iam-policy-binding gs://como-riff-state ",
    );
    assert!(bucket.contains(&format!(
        "--member {account} --role roles/storage.objectUser"
    )));
    let secret = line(
        &calls,
        "secrets add-iam-policy-binding riff-oidc-client-secret ",
    );
    assert!(secret.contains(&format!(
        "--member {account} --role roles/secretmanager.secretAccessor"
    )));
    let project = line(&calls, "projects add-iam-policy-binding como-riff ");
    assert!(project.contains("riff-build@"), "{project}");
    assert!(project.contains("--role roles/run.builder"), "{project}");
}

#[test]
fn setup_again_keeps_each_resource() {
    let calls = run("cloud-setup.sh", &SETUP);
    assert!(!calls.contains("storage buckets create"), "{calls}");
    assert!(!calls.contains("service-accounts create"), "{calls}");
    assert!(!calls.contains("repositories create"), "{calls}");
    assert!(!calls.contains("workload-identity-pools create"), "{calls}");
    assert!(!calls.contains("providers create-oidc"), "{calls}");
    assert!(calls.contains("storage buckets update gs://como-riff-state --lifecycle-file"));
}

/// A setup again gives the provider that exists the condition of
/// 01M3NJAZAQ3AKMAM0EGM7R3S89.
#[test]
fn setup_again_sets_the_condition_of_the_provider() {
    let calls = run("cloud-setup.sh", &SETUP);
    let update = line(
        &calls,
        "iam workload-identity-pools providers update-oidc github ",
    );
    assert!(update.contains(CONDITION), "{update}");
}

/// R46, and 01M3TJWJEPTSF1S3S5PJD25Z7Y: an older version of an object
/// goes after 7 days.
#[test]
fn the_rules_delete_thread_objects_after_30_days_and_older_versions_after_7() {
    let text = fs::read_to_string(deploy().join("lifecycle.json")).unwrap();
    let rules: serde_json::Value = serde_json::from_str(&text).unwrap();
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

/// 01M3TJWJEPTSF1S3S5PJD25Z7Y: a standard bucket with object versioning.
#[test]
fn setup_makes_a_standard_bucket_with_versioning() {
    let calls = run("cloud-setup.sh", &[]);
    let make = line(&calls, "storage buckets create gs://como-riff-state ");
    assert!(make.contains("--default-storage-class standard"), "{make}");
    let update = line(&calls, "storage buckets update gs://como-riff-state ");
    assert!(update.contains("--versioning"), "{update}");
    // A setup again sets the same.
    let again = run("cloud-setup.sh", &SETUP);
    let update = line(&again, "storage buckets update gs://como-riff-state ");
    assert!(update.contains("--versioning"), "{update}");
}

/// 01M3TJWJEPTSF1S3S5PJD25Z7Y: the service has 1 GiB of memory. The
/// deploy sets it, and a setup sets it on a service that runs.
#[test]
fn the_service_gets_1_gib_of_memory() {
    let calls = run("deploy.sh", &[]);
    let deploy = line(&calls, "run deploy riff-server ");
    assert!(deploy.contains("--memory 1Gi"), "{deploy}");

    let calls = run("cloud-setup.sh", &[]);
    assert!(!calls.contains("run services update"), "{calls}");
    let calls = run("cloud-setup.sh", &["run services describe riff-server "]);
    let update = line(&calls, "run services update riff-server ");
    assert!(update.contains("--memory 1Gi"), "{update}");
    assert!(update.contains("--region us-central1"), "{update}");
}

/// 01M3TJWJ6J3M6JRXJTAETZ5M6F: an alert on each error in the log of
/// riff-server goes to the owner by email.
#[test]
fn setup_makes_the_alert_for_the_owner() {
    let calls = run("cloud-setup.sh", &[]);
    let channel = line(&calls, "beta monitoring channels create ");
    assert!(channel.contains("--type email"), "{channel}");
    assert!(
        channel.contains("--channel-labels email_address=owner@example.com"),
        "{channel}"
    );
    let policy = line(&calls, "alpha monitoring policies create ");
    assert!(policy.contains("--policy-from-file alert.json"), "{policy}");
    assert!(policy.contains("--notification-channels"), "{policy}");
    assert!(calls.contains("monitoring.googleapis.com"), "{calls}");

    // A setup again keeps the channel and the alert.
    let found = [
        "beta monitoring channels list ",
        "alpha monitoring policies list ",
    ];
    let calls = run("cloud-setup.sh", &found);
    assert!(!calls.contains("monitoring channels create"), "{calls}");
    assert!(!calls.contains("monitoring policies create"), "{calls}");
}

#[test]
fn the_errors_recipe_reads_only_the_lines_with_the_severity_error() {
    let text = fs::read_to_string(deploy().join("cloud.just")).unwrap();
    let recipe = text.split("\nerrors ").nth(1).expect("an errors recipe");
    assert!(recipe.contains("gcloud run services logs read"), "{recipe}");
    assert!(
        recipe.contains("--log-filter 'severity>=ERROR'"),
        "{recipe}"
    );
}

#[test]
fn setup_with_no_owner_makes_no_alert_and_says_how() {
    let out = command(&deploy(), "cloud-setup.sh", &[], &[], None);
    assert!(out.status.success());
    assert!(!out.log.contains("monitoring channels"), "{}", out.log);
    assert!(!out.log.contains("monitoring policies"), "{}", out.log);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RIFF_OWNER=YOUR_EMAIL just cloud setup"),
        "{stdout}"
    );
}

/// The alert fires on a log line of the service of `cloud.env` with the
/// severity ERROR or more, and has the name that the setup looks for.
#[test]
fn the_alert_matches_each_error_of_the_service() {
    let text = fs::read_to_string(deploy().join("alert.json")).unwrap();
    let alert: serde_json::Value = serde_json::from_str(&text).unwrap();
    let env = fs::read_to_string(deploy().join("cloud.env")).unwrap();
    let setting = |name: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("no {name} in cloud.env"))
            .to_owned()
    };
    assert_eq!(alert["displayName"], setting("CLOUD_ALERT"));
    let filter = alert["conditions"][0]["conditionMatchedLog"]["filter"]
        .as_str()
        .unwrap();
    assert!(filter.contains("severity>=ERROR"), "{filter}");
    let service = format!(
        "resource.labels.service_name=\"{}\"",
        setting("CLOUD_SERVICE")
    );
    assert!(filter.contains(&service), "{filter}");
    // A log alert needs a rate limit.
    assert!(alert["alertStrategy"]["notificationRateLimit"]["period"].is_string());
}

#[test]
fn deploy_runs_one_instance_with_sign_in() {
    let calls = run("deploy.sh", &[]);
    let deploy = line(&calls, "run deploy riff-server --source . ");
    for flag in [
        "--region us-central1",
        "--service-account riff-server@como-riff.iam.gserviceaccount.com",
        "--build-service-account projects/como-riff/serviceAccounts/riff-build@",
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
}

/// The cloud riff needs an owner (01M3JN3ASSV9SA0QZKXXJ0RTEV). With
/// no `RIFF_OWNER`, the deploy stops before it calls gcloud.
#[test]
fn deploy_stops_with_no_owner() {
    let out = command(&deploy(), "deploy.sh", &[], &[], None);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("gh variable set RIFF_OWNER"), "{stderr}");
    assert!(!out.log.contains("run deploy"), "{}", out.log);
}

#[test]
fn deploy_maps_the_domain_once_when_the_url_is_the_domain() {
    let copy = deploy_with_url("https://riff.comotechnologies.io");
    let scripts = copy.path().join("deploy");
    let calls = run_in(&scripts, "deploy.sh", &[], &[]);
    let deploy = line(&calls, "run deploy riff-server ");
    assert!(deploy.contains("RIFF_PUBLIC_URL=https://riff.comotechnologies.io,"));
    let map = line(
        &calls,
        "beta run domain-mappings create --service riff-server ",
    );
    assert!(map.contains("--domain riff.comotechnologies.io"), "{map}");
    assert!(map.contains("--region us-central1"), "{map}");

    let found = ["beta run domain-mappings describe"];
    let calls = run_in(&scripts, "deploy.sh", &[], &found);
    assert!(!calls.contains("domain-mappings create"), "{calls}");
}

#[test]
fn deploy_maps_no_domain_while_the_url_is_the_cloud_run_url() {
    let calls = run("deploy.sh", &[]);
    assert!(!calls.contains("domain-mappings"), "{calls}");
}

#[test]
fn deploy_with_an_image_deploys_only_that_image() {
    let image = "us-central1-docker.pkg.dev/como-riff/riff/riff-server:abc";
    let calls = run_with("deploy.sh", &["--image", image], &[]);
    let deploy = line(&calls, "run deploy riff-server --image ");
    assert!(deploy.contains(image), "{deploy}");
    assert!(!deploy.contains("--source"), "{deploy}");
    assert!(!deploy.contains("--build-service-account"), "{deploy}");
    assert!(
        deploy.contains("--min-instances 1 --max-instances 1"),
        "{deploy}"
    );
    assert!(!calls.contains("domain-mappings"), "{calls}");
}

/// 01M3NJAZAQ3AKMAM0EGM7R3S89: only `main` and the tags `v*` of the
/// repository sign in.
const CONDITION: &str = "--attribute-condition assertion.repository == 'como-technologies/riff' && (assertion.ref == 'refs/heads/main' || assertion.ref.startsWith('refs/tags/v'))";

#[test]
fn only_main_and_the_release_tags_sign_in_as_the_deploy_account() {
    let calls = run("cloud-setup.sh", &[]);
    let provider = line(
        &calls,
        "iam workload-identity-pools providers create-oidc github ",
    );
    assert!(
        provider.contains("--issuer-uri https://token.actions.githubusercontent.com"),
        "{provider}"
    );
    assert!(provider.contains(CONDITION), "{provider}");
    assert!(!calls.contains("providers update-oidc"), "{calls}");
    let user = line(
        &calls,
        "iam service-accounts add-iam-policy-binding riff-deploy@",
    );
    assert!(
        user.contains(
            "--member principalSet://iam.googleapis.com/projects/816917641970/locations/global/workloadIdentityPools/github/attribute.repository/como-technologies/riff"
        ),
        "{user}"
    );
    assert!(
        user.contains("--role roles/iam.workloadIdentityUser"),
        "{user}"
    );
}

#[test]
fn the_deploy_account_pushes_images_and_deploys_the_service() {
    let calls = run("cloud-setup.sh", &SETUP);
    let account = "serviceAccount:riff-deploy@como-riff.iam.gserviceaccount.com";
    let repository = line(
        &calls,
        "artifacts repositories add-iam-policy-binding riff ",
    );
    assert!(repository.contains(&format!(
        "--member {account} --role roles/artifactregistry.writer"
    )));
    let roles: Vec<&str> = calls
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

#[test]
fn ci_deploys_after_the_gate_or_the_release_check_with_no_key() {
    let (_, job) = ci_parts();
    for part in [
        "needs: [gate, release]",
        "github.ref == 'refs/heads/main'",
        "id-token: write",
        "google-github-actions/auth@",
        "workload_identity_provider:",
        "deploy/deploy.sh --image \"$IMAGE\"",
        "RIFF_OWNER: ${{ vars.RIFF_OWNER }}",
    ] {
        assert!(job.contains(part), "{part} is not in the deploy job");
    }
    assert!(
        !job.contains("credentials_json"),
        "the job must not use a key"
    );
}

/// The deploy job of the CI workflow, and the triggers of the workflow.
fn ci_parts() -> (String, String) {
    let text = fs::read_to_string(deploy().join("../.github/workflows/ci.yml")).unwrap();
    let (on, rest) = text.split_once("\njobs:\n").unwrap();
    let job = rest
        .split_once("\n  deploy:\n")
        .unwrap()
        .1
        .split_once("\n  audit:\n")
        .unwrap()
        .0;
    (on.to_owned(), job.to_owned())
}

/// The `if:` of the deploy job, as one line.
fn deploy_if() -> String {
    let (_, job) = ci_parts();
    let when = job.split_once("\n    if: >-\n").unwrap().1;
    let when = when.split_once("\n    runs-on:").unwrap().0;
    when.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A value of a GitHub Actions expression.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str(String),
    Bool(bool),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty(),
            Value::Bool(b) => *b,
        }
    }
}

/// Evaluates the part of the GitHub Actions expression language that
/// the deploy job uses: strings, context paths, `==`, `!=`, `&&`, `||`,
/// `!`, parentheses, `startsWith` and `cancelled`. A context path that
/// `ctx` does not name is the empty string. It panics on each other
/// part, so a new part of the `if:` needs a new part here.
struct Eval<'a> {
    rest: &'a str,
    ctx: &'a [(&'a str, &'a str)],
}

impl Eval<'_> {
    fn eat(&mut self, token: &str) -> bool {
        self.rest = self.rest.trim_start();
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn or(&mut self) -> Value {
        let mut value = self.and();
        while self.eat("||") {
            let right = self.and();
            value = Value::Bool(value.truthy() || right.truthy());
        }
        value
    }

    fn and(&mut self) -> Value {
        let mut value = self.unary();
        while self.eat("&&") {
            let right = self.unary();
            value = Value::Bool(value.truthy() && right.truthy());
        }
        value
    }

    fn unary(&mut self) -> Value {
        if self.eat("!") {
            return Value::Bool(!self.unary().truthy());
        }
        let left = self.primary();
        if self.eat("==") {
            Value::Bool(left == self.primary())
        } else if self.eat("!=") {
            Value::Bool(left != self.primary())
        } else {
            left
        }
    }

    fn primary(&mut self) -> Value {
        if self.eat("(") {
            let value = self.or();
            assert!(self.eat(")"), "no ): {}", self.rest);
            return value;
        }
        if self.eat("'") {
            let (text, rest) = self.rest.split_once('\'').unwrap();
            self.rest = rest;
            return Value::Str(text.to_owned());
        }
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
            .unwrap_or(self.rest.len());
        let (name, rest) = self.rest.split_at(end);
        assert!(!name.is_empty(), "no value at: {}", self.rest);
        self.rest = rest;
        if self.eat("(") {
            let mut args = Vec::new();
            while !self.eat(")") {
                args.push(self.or());
                self.eat(",");
            }
            return match (name, args.as_slice()) {
                ("cancelled", []) => Value::Bool(false),
                ("startsWith", [Value::Str(text), Value::Str(start)]) => {
                    Value::Bool(text.starts_with(start.as_str()))
                }
                _ => panic!("no function {name}({args:?})"),
            };
        }
        let value = self.ctx.iter().find(|(key, _)| *key == name);
        Value::Str(value.map_or("", |(_, value)| value).to_owned())
    }
}

/// Does the deploy job run in the context `ctx`?
fn deploys(ctx: &[(&str, &str)]) -> bool {
    let when = deploy_if();
    let mut eval = Eval { rest: &when, ctx };
    let value = eval.or();
    assert!(eval.rest.trim().is_empty(), "left: {}", eval.rest);
    value.truthy()
}

type Ctx = Vec<(&'static str, &'static str)>;

/// The context of a push of `git_ref`, with the results of the gate
/// and the Release check.
fn push(git_ref: &'static str, gate: &'static str, release: &'static str) -> Ctx {
    vec![
        ("github.event_name", "push"),
        ("github.ref", git_ref),
        ("vars.CLOUD_DEPLOY", "true"),
        ("needs.gate.result", gate),
        ("needs.release.result", release),
    ]
}

/// The context of a run by hand on `git_ref` with the input `tag`.
fn dispatch(git_ref: &'static str, tag: &'static str, gate: &'static str) -> Ctx {
    vec![
        ("github.event_name", "workflow_dispatch"),
        ("github.ref", git_ref),
        ("inputs.tag", tag),
        ("vars.CLOUD_DEPLOY", "true"),
        ("needs.gate.result", gate),
        ("needs.release.result", "skipped"),
    ]
}

/// The same context with no `CLOUD_DEPLOY`.
fn off(mut ctx: Ctx) -> Ctx {
    ctx.retain(|(key, _)| *key != "vars.CLOUD_DEPLOY");
    ctx
}

/// 01M3NJAZ6BYH7TWKDYTVEK78PG, 01M3NJAZ8HSE87H0GZ8SNGWN31: a release
/// tag deploys after its Release check, and a run by hand with the
/// input `tag` deploys after the gate. A push to main never deploys.
#[test]
fn a_release_tag_or_a_run_by_hand_deploys() {
    let (on, job) = ci_parts();
    assert!(on.contains("\n  workflow_dispatch:\n"), "{on}");
    assert!(on.contains("\n      tag:\n"), "{on}");
    assert!(on.contains("type: string"), "{on}");
    assert!(on.contains("tags: [\"v*\"]"), "{on}");
    assert!(!job.contains("github.event.before"), "{job}");

    assert!(deploys(&push("refs/tags/v0.6.0", "skipped", "success")));
    assert!(deploys(&dispatch("refs/heads/main", "v0.5.0", "success")));

    for gate in ["success", "skipped", "failure"] {
        for release in ["success", "skipped", "failure"] {
            let ctx = push("refs/heads/main", gate, release);
            assert!(!deploys(&ctx), "a push to main deploys: {ctx:?}");
        }
    }
    // A tag that is not vX.Y.Z fails its Release check.
    assert!(!deploys(&push("refs/tags/v0.6", "skipped", "failure")));
    assert!(!deploys(&push("refs/tags/nightly", "skipped", "success")));
    assert!(!deploys(&push("refs/tags/v0.6.0", "skipped", "cancelled")));
    assert!(!deploys(&dispatch("refs/heads/main", "v0.5.0", "failure")));
    assert!(!deploys(&dispatch("refs/heads/main", "", "success")));
    assert!(!deploys(&dispatch("refs/heads/other", "v0.5.0", "success")));
    assert!(!deploys(&off(push(
        "refs/tags/v0.6.0",
        "skipped",
        "success"
    ))));
    assert!(!deploys(&off(dispatch(
        "refs/heads/main",
        "v0.5.0",
        "success"
    ))));
}

/// The part of the Development page under `heading`, up to the next
/// heading of its level.
fn book_part(heading: &str) -> String {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page.find(heading).unwrap()..];
    part[..part[4..].find("\n### ").unwrap()].to_owned()
}

/// The book says that the release tag deploys, and that each machine
/// updates after it (01M3NJAZ8HSE87H0GZ8SNGWN31).
#[test]
fn the_book_deploys_at_the_end_of_a_wave() {
    let part = book_part("### Deploy the shared server at the end of a wave\n");
    assert!(part.contains("The release tag deploys itself."), "{part}");
    assert!(part.contains("`riff update`"), "{part}");
    assert!(part.contains("`riff update --auto on`"), "{part}");
    assert!(part.contains("riff workers start"), "{part}");
    assert!(!part.contains("gh workflow run"), "{part}");
}

/// The book how-to deploys again or rolls back with the real input
/// name of the workflow.
#[test]
fn the_book_deploys_a_release_again() {
    let part = book_part("### Deploy a release again, or roll back\n");
    assert!(part.contains("```sh\n"), "{part}");
    assert!(
        part.contains("gh workflow run CI --ref main -f tag=v0.2.0"),
        "{part}"
    );
}

/// The book how-to makes a release: bump, merge, tag, with the real
/// commands (01M3N73AW2J3TVSZWFJ88A91PG). The pull request states the
/// level and the reason, and the notes list what each person runs
/// (01M3N73EGY4NQDQQP8Y185E9VZ). GitHub generates the rest of the
/// notes (01M3NB3EWE2V9PCTMNTZAKEXMA).
#[test]
fn the_book_makes_a_release() {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page.find("### Make a release\n").unwrap()..];
    let part = &part[..part[4..].find("\n### ").unwrap()];
    for text in [
        "An admin makes the release",
        "(how-it-works.md#the-test-for-each-release)",
        "states the level and the\nreason in one line",
        "```text\nLevel: minor.",
        "the commands that each person\nruns",
        "`riff update --tag v0.2.0`",
        "gh pr create --title \"Release v0.2.0\" --label release --body-file pr.md\n",
        "the level line\nof the pull request",
        "gh release create v0.2.0 --verify-tag --title v0.2.0 --notes-file notes.md --generate-notes\n",
        "`--notes-start-tag v0.1.0`",
        "riff workers stop\n",
        "sed -i 's/^version = \".*\"/version = \"0.2.0\"/' Cargo.toml\n",
        "sed -i 's/\"version\": \".*\"/\"version\": \"0.2.0\"/' crates/riff/claude-plugin/riff/.claude-plugin/plugin.json\n",
        "cargo update --workspace\n",
        "git tag v0.2.0 origin/main\n",
        "git push origin v0.2.0\n",
        "`Release check`",
    ] {
        assert!(part.contains(text), "{text:?} is not in: {part}");
    }
    let root = tempfile::tempdir().unwrap();
    fs::copy(
        deploy().join("../Cargo.toml"),
        root.path().join("Cargo.toml"),
    )
    .unwrap();
    let bump = Command::new("sh")
        .arg("-c")
        .arg("sed -i 's/^version = \".*\"/version = \"0.2.0\"/' Cargo.toml")
        .current_dir(root.path())
        .status()
        .unwrap();
    assert!(bump.success());
    let bumped = fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
    assert!(bumped.contains("\nversion = \"0.2.0\"\n"), "{bumped}");
    assert!(bumped.contains("tokio = { version = \"1."), "{bumped}");
    let plugin = root.path().join("plugin.json");
    fs::copy(
        deploy().join("../crates/riff/claude-plugin/riff/.claude-plugin/plugin.json"),
        &plugin,
    )
    .unwrap();
    let bump = Command::new("sh")
        .arg("-c")
        .arg("sed -i 's/\"version\": \".*\"/\"version\": \"0.2.0\"/' plugin.json")
        .current_dir(root.path())
        .status()
        .unwrap();
    assert!(bump.success());
    let bumped = fs::read_to_string(&plugin).unwrap();
    assert!(bumped.contains("\"version\": \"0.2.0\""), "{bumped}");
}

/// Runs `deploy/release-check.sh` with `args`.
fn release_check(args: &[&str]) -> std::process::Output {
    Command::new(deploy().join("release-check.sh"))
        .args(args)
        .output()
        .unwrap()
}

/// A checkout whose crates have the version `crates` in Cargo.toml and
/// `locked` in Cargo.lock.
fn checkout(crates: &str, locked: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        format!("[workspace.package]\nedition = \"2024\"\nversion = \"{crates}\"\n"),
    )
    .unwrap();
    let lock: String = ["riff", "riff-core", "riff-server"]
        .iter()
        .map(|name| format!("[[package]]\nname = \"{name}\"\nversion = \"{locked}\"\n\n"))
        .collect();
    fs::write(root.path().join("Cargo.lock"), lock).unwrap();
    root
}

/// 01M3MRMAY3P1K151RGAP9K6GSH: the deploy checks out the release tag,
/// the pushed tag or its input, and checks it before it builds. So it
/// refuses a tag that is not vX.Y.Z.
#[test]
fn the_deploy_takes_only_a_release_tag() {
    let (_, job) = ci_parts();
    let checkout = job.find("ref: refs/tags/${{ env.TAG }}").unwrap();
    let check = job.find("run: deploy/release-check.sh \"$TAG\"").unwrap();
    let build = job.find("docker/build-push-action@").unwrap();
    assert!(checkout < check && check < build, "{job}");
    assert!(
        job.contains("TAG: ${{ github.event_name == 'push' && github.ref_name || inputs.tag }}"),
        "{job}"
    );
    assert!(job.contains("riff-server:$TAG"), "{job}");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.to_str().unwrap();
    for bad in ["", "main", "v0.2", "0.2.0", "v0.2.0-rc1", "refs/heads/main"] {
        let out = release_check(&[bad, root]);
        assert!(!out.status.success(), "{bad} passed");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("\"{bad}\" is not a release tag. Give vX.Y.Z")),
            "{stderr}"
        );
    }
    let this = format!("v{}", env!("CARGO_PKG_VERSION"));
    let out = release_check(&[&this, root]);
    assert!(out.status.success(), "{out:?}");
}

/// 01M3N73AW2J3TVSZWFJ88A91PG: CI checks each pushed tag v*. The check
/// fails a tag whose version is not the version of the crates.
#[test]
fn the_tag_check_fails_a_tag_of_another_version() {
    let text = fs::read_to_string(deploy().join("../.github/workflows/ci.yml")).unwrap();
    let (on, jobs) = text.split_once("\njobs:\n").unwrap();
    assert!(on.contains("tags: [\"v*\"]"), "{on}");
    let job = jobs
        .split_once("\n  release:\n")
        .unwrap()
        .1
        .split_once("\n  deploy:\n")
        .unwrap()
        .0;
    assert!(job.contains("if: github.ref_type == 'tag'"), "{job}");
    assert!(
        job.contains("run: deploy/release-check.sh \"$GITHUB_REF_NAME\""),
        "{job}"
    );

    let root = checkout("0.2.0", "0.2.0");
    let path = root.path().to_str().unwrap();
    assert!(release_check(&["v0.2.0", path]).status.success());
    let out = release_check(&["v0.3.0", path]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("the tag v0.3.0 is not the version of the crates in Cargo.toml: 0.2.0."),
        "{out:?}"
    );
    let root = checkout("0.2.0", "0.1.0");
    let out = release_check(&["v0.2.0", root.path().to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("the tag v0.2.0 is not the version of riff in Cargo.lock: 0.1.0."),
        "{out:?}"
    );
}

#[test]
fn the_image_holds_only_the_binary_and_the_certificates() {
    let text = fs::read_to_string(deploy().join("../Dockerfile")).unwrap();
    let last = text.rsplit_once("\nFROM ").unwrap().1;
    assert!(last.starts_with("scratch\n"), "{last}");
    let copies: Vec<&str> = last.lines().filter(|l| l.starts_with("COPY")).collect();
    assert_eq!(copies.len(), 2, "{copies:?}");
    assert!(copies.iter().any(|c| c.ends_with(" /riff-server")));
    assert!(copies.iter().any(|c| c.contains("ca-certificates.crt")));
    let user = last.lines().find(|l| l.starts_with("USER ")).unwrap();
    assert!(!user.contains(" 0") && !user.contains("root"), "{user}");
}

/// The generated notes of the GitHub releases are the changelog: they
/// leave out the release pull requests, no changelog file exists, and
/// the book says where to read them (01M3NB3EWE2V9PCTMNTZAKEXMA).
#[test]
fn the_release_notes_are_the_changelog() {
    let root = deploy().join("..");
    let config = fs::read_to_string(root.join(".github/release.yml")).unwrap();
    assert!(
        config.contains("changelog:\n  exclude:\n    labels:\n      - release\n"),
        "{config}"
    );
    for name in ["CHANGELOG.md", "CHANGELOG", "docs/src/changelog.md"] {
        assert!(!root.join(name).exists(), "{name} exists");
    }
    let page = fs::read_to_string(root.join("docs/src/how-it-works.md")).unwrap();
    let part = &page[page
        .find("### See what changed in a release\n")
        .expect("no how-to")..];
    let part = &part[..part[4..].find("\n### ").unwrap() + 4];
    assert!(
        part.contains(
            "```sh\ngh release list --repo como-technologies/riff\n\
             gh release view v0.4.0 --repo como-technologies/riff\n```"
        ),
        "{part}"
    );
}

/// The settings that name a resource of one riff: its bucket, service,
/// secret, accounts, URL, domain, alert and sign-in client.
const RESOURCES: [&str; 10] = [
    "CLOUD_BUCKET",
    "CLOUD_SERVICE",
    "CLOUD_SECRET",
    "CLOUD_URL",
    "CLOUD_DOMAIN",
    "CLOUD_ALERT",
    "CLOUD_RUN_ACCOUNT",
    "CLOUD_BUILD_ACCOUNT",
    "CLOUD_DEPLOY_ACCOUNT",
    "RIFF_OIDC_CLIENT_ID",
];

/// 01M3ZE3Z26N1CG090D5D5FZ3NW: no setting of one riff names the bucket,
/// the service, the secret or another resource of a different riff. So
/// the stage cannot touch the data of the shared riff.
#[test]
fn no_settings_of_one_riff_name_a_resource_of_another() {
    let files = settings_files();
    assert!(files.contains(&"cloud.env".to_owned()), "{files:?}");
    assert!(files.contains(&"stage.env".to_owned()), "{files:?}");
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

/// Each script sets `-u`, so each settings file has each key of the
/// shared riff, also when its value is empty.
#[test]
fn each_settings_file_has_each_key_of_the_shared_riff() {
    let keys = |file: &str| -> Vec<String> { settings(file).into_iter().map(|(k, _)| k).collect() };
    let shared = keys("cloud.env");
    for file in settings_files() {
        assert_eq!(keys(&file), shared, "{file}");
    }
}

/// 01M3ZE3Z580RB5AYAJX6321DFW: the setup of the stage makes only the
/// resources of the stage. It makes no CI deploy and no alert.
#[test]
fn setup_of_the_stage_makes_only_the_resources_of_the_stage() {
    let calls = run_with("cloud-setup.sh", &["stage"], &[]);
    line(&calls, "storage buckets create gs://como-riff-stage-state ");
    line(&calls, "iam service-accounts create riff-stage-server ");
    line(&calls, "iam service-accounts create riff-stage-build ");
    let secret = line(
        &calls,
        "secrets add-iam-policy-binding riff-stage-oidc-client-secret ",
    );
    assert!(secret.contains("riff-stage-server@"), "{secret}");
    for shared in [
        "como-riff-state",
        "riff-oidc-client-secret",
        "riff-server@",
        "riff-build@",
        "riff-deploy",
        "workload-identity",
        "monitoring channels",
        "monitoring policies",
        "run services describe riff-server ",
    ] {
        assert!(!calls.contains(shared), "{shared} in:\n{calls}");
    }
    // The stage needs a client of its own.
    let out = command(&deploy(), "cloud-setup.sh", &["stage"], &SETUP, None);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("just cloud oauth-client stage"), "{stdout}");
}

/// The name `cloud` is the shared riff, as no name.
#[test]
fn the_name_cloud_is_the_shared_riff() {
    let calls = run_with("cloud-setup.sh", &["cloud"], &SETUP);
    assert_eq!(calls, run("cloud-setup.sh", &SETUP));
}

#[test]
fn a_name_with_no_settings_file_stops_the_script() {
    for script in ["cloud-setup.sh", "deploy.sh", "oauth-client.sh"] {
        let out = command(&deploy(), script, &["nosuch"], &[], Some("o@x"));
        assert!(!out.status.success(), "{script}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("deploy/nosuch.env is not there"),
            "{stderr}"
        );
        assert!(out.log.is_empty(), "{}", out.log);
    }
}

/// The stage, with a client ID.
fn stage() -> tempfile::TempDir {
    deploy_with(
        "stage.env",
        "RIFF_OIDC_CLIENT_ID",
        "1-a.apps.googleusercontent.com",
    )
}

/// 01M3ZE3Z580RB5AYAJX6321DFW: the deploy of the stage builds the image
/// and runs it on the service, bucket and secret of the stage.
#[test]
fn deploy_of_the_stage_uses_only_the_resources_of_the_stage() {
    let copy = stage();
    let scripts = copy.path().join("deploy");
    let calls = run_in(&scripts, "deploy.sh", &["stage"], &[]);
    let deploy = line(&calls, "run deploy riff-stage --source . ");
    for flag in [
        "--service-account riff-stage-server@como-riff.iam.gserviceaccount.com",
        "--build-service-account projects/como-riff/serviceAccounts/riff-stage-build@",
        "RIFF_PUBLIC_URL=https://riff-stage-816917641970.us-central1.run.app,",
        "RIFF_BUCKET=como-riff-stage-state,",
        "RIFF_OIDC_CLIENT_ID=1-a.apps.googleusercontent.com,",
        "--set-secrets RIFF_OIDC_CLIENT_SECRET=riff-stage-oidc-client-secret:latest",
    ] {
        assert!(deploy.contains(flag), "{flag} is not in: {deploy}");
    }
    for shared in ["como-riff-state", "riff-server", "riff-oidc-client-secret"] {
        assert!(!calls.contains(shared), "{shared} in:\n{calls}");
    }
    assert!(!calls.contains("domain-mappings"), "{calls}");
}

/// 01M3ZE3Z580RB5AYAJX6321DFW: a deploy with a release tag deploys the
/// image that CI built for that tag, and builds nothing.
#[test]
fn deploy_with_a_tag_deploys_the_image_of_the_tag() {
    let copy = stage();
    let scripts = copy.path().join("deploy");
    let calls = run_in(&scripts, "deploy.sh", &["stage", "v0.8.0"], &[]);
    let deploy = line(&calls, "run deploy riff-stage --image ");
    let image = "us-central1-docker.pkg.dev/como-riff/riff/riff-server:v0.8.0";
    assert!(deploy.contains(&format!("--image {image} ")), "{deploy}");
    assert!(!deploy.contains("--source"), "{deploy}");
    assert!(
        deploy.contains("RIFF_BUCKET=como-riff-stage-state,"),
        "{deploy}"
    );
    assert!(!calls.contains("domain-mappings"), "{calls}");

    // The shared riff takes a tag too.
    let calls = run_with("deploy.sh", &["v0.8.0"], &[]);
    let deploy = line(&calls, "run deploy riff-server --image ");
    assert!(deploy.contains(&format!("--image {image} ")), "{deploy}");
}

#[test]
fn deploy_refuses_a_tag_that_is_not_a_release_tag() {
    let copy = stage();
    let scripts = copy.path().join("deploy");
    for tag in ["0.8.0", "v0.8", "V0.8.0", "--source"] {
        let out = command(&scripts, "deploy.sh", &["stage", tag], &[], Some("o@x"));
        assert!(!out.status.success(), "{tag}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("is not a release tag"), "{tag}: {stderr}");
        assert!(out.log.is_empty(), "{}", out.log);
    }
}

/// The stage has no client ID until `just cloud oauth-client stage`.
#[test]
fn deploy_of_the_stage_with_no_client_says_how_to_store_it() {
    let out = command(&deploy(), "deploy.sh", &["stage"], &[], Some("o@x"));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("deploy/stage.env has no client ID. Run: just cloud oauth-client stage"),
        "{stderr}"
    );
    assert!(!out.log.contains("run deploy"), "{}", out.log);
}

/// Each recipe of `just cloud` that acts on one riff takes its name and
/// reads its settings with `settings.sh`.
#[test]
fn the_recipes_take_the_name_of_the_settings() {
    let text = fs::read_to_string(deploy().join("cloud.just")).unwrap();
    for (recipe, call) in [
        ("setup NAME='':", "./cloud-setup.sh {{NAME}}"),
        ("oauth-client NAME='':", "./oauth-client.sh {{NAME}}"),
        ("deploy NAME='' TAG='':", "./deploy.sh {{NAME}} {{TAG}}"),
        ("down NAME='':", ". ./settings.sh"),
        ("status NAME='':", ". ./settings.sh"),
        ("log *ARGS:", ". ./settings.sh"),
        ("errors *ARGS:", ". ./settings.sh"),
    ] {
        let body = text
            .split(&format!("\n{recipe}\n"))
            .nth(1)
            .unwrap_or_else(|| panic!("no recipe {recipe}"));
        let body = body.split("\n\n").next().unwrap();
        assert!(body.contains(call), "{recipe}: {body}");
    }
    // Only the shared riff has the CI variable.
    let down = text.split("\ndown NAME='':\n").nth(1).unwrap();
    assert!(
        down.contains("if [ -z \"$CLOUD_NAME\" ] && [ -n \"$(gh variable get CLOUD_DEPLOY"),
        "{down}"
    );
}
