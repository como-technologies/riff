//! The cloud scripts in `deploy/`, the image and the CI deploy (R6,
//! R46, R134-R136, R160, R161).
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
    let copy = tempfile::tempdir().unwrap();
    let dir = copy.path().join("deploy");
    fs::create_dir(&dir).unwrap();
    for entry in fs::read_dir(deploy()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        if name != "cloud.env" {
            symlink(&path, dir.join(name)).unwrap();
        }
    }
    let env = fs::read_to_string(deploy().join("cloud.env")).unwrap();
    let env: Vec<String> = env
        .lines()
        .map(|l| {
            if l.starts_with("CLOUD_URL=") {
                format!("CLOUD_URL={url}")
            } else {
                l.to_owned()
            }
        })
        .collect();
    fs::write(dir.join("cloud.env"), env.join("\n") + "\n").unwrap();
    copy
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

/// The result of a script: its exit status, its stderr and the calls.
struct Ran {
    status: std::process::ExitStatus,
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

#[test]
fn the_rule_deletes_only_thread_objects_after_30_days() {
    let text = fs::read_to_string(deploy().join("lifecycle.json")).unwrap();
    let rules: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        rules,
        serde_json::json!({"rule": [{
            "action": {"type": "Delete"},
            "condition": {"age": 30, "matchesPrefix": ["threads/"]},
        }]})
    );
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

#[test]
fn only_main_of_the_repository_signs_in_as_the_deploy_account() {
    let calls = run("cloud-setup.sh", &[]);
    let provider = line(
        &calls,
        "iam workload-identity-pools providers create-oidc github ",
    );
    assert!(
        provider.contains("--issuer-uri https://token.actions.githubusercontent.com"),
        "{provider}"
    );
    assert!(
        provider.contains(
            "assertion.repository == 'como-technologies/riff' && assertion.ref == 'refs/heads/main'"
        ),
        "{provider}"
    );
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
fn ci_deploys_after_the_gate_with_no_key() {
    let (_, job) = ci_parts();
    for part in [
        "needs: gate",
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

/// 01M3MMZQ3KTF5Z3GXNR7DRQ65Z: a push to main never deploys. Only a
/// run by hand with the input `tag` does (R160).
#[test]
fn only_a_run_by_hand_deploys() {
    let (on, job) = ci_parts();
    assert!(on.contains("\n  workflow_dispatch:\n"), "{on}");
    assert!(on.contains("\n      tag:\n"), "{on}");
    assert!(on.contains("type: string"), "{on}");
    let when = job.lines().find(|l| l.trim().starts_with("if:")).unwrap();
    assert!(
        when.contains("github.event_name == 'workflow_dispatch'"),
        "{when}"
    );
    assert!(when.contains("inputs.tag != ''"), "{when}");
    assert!(!when.contains("'push'"), "{when}");
    assert!(!job.contains("github.event.before"), "{job}");
}

/// The book how-to runs the workflow with the real input name.
#[test]
fn the_book_deploys_at_the_end_of_a_wave() {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page
        .find("### Deploy the shared server at the end of a wave\n")
        .unwrap()..];
    let part = &part[..part[4..].find("\n### ").unwrap()];
    assert!(part.contains("```sh\n"), "{part}");
    assert!(
        part.contains("gh workflow run CI --ref main -f tag=v0.2.0"),
        "{part}"
    );
    assert!(part.contains("riff workers start"), "{part}");
}

/// The book how-to makes a release: bump, merge, tag, with the real
/// commands (01M3MRMASMP59PKHAV92XSV7XE).
#[test]
fn the_book_makes_a_release() {
    let page = fs::read_to_string(deploy().join("../docs/src/development.md")).unwrap();
    let part = &page[page.find("### Make a release\n").unwrap()..];
    let part = &part[..part[4..].find("\n### ").unwrap()];
    for text in [
        "An admin makes the release",
        "riff workers stop\n",
        "sed -i 's/^version = \".*\"/version = \"0.2.0\"/' Cargo.toml\n",
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

/// 01M3MRMAY3P1K151RGAP9K6GSH: the deploy checks out the release tag of
/// its input and checks it before it builds. So it refuses an input
/// that is not a tag vX.Y.Z.
#[test]
fn the_deploy_takes_only_a_release_tag() {
    let (_, job) = ci_parts();
    let checkout = job.find("ref: refs/tags/${{ inputs.tag }}").unwrap();
    let check = job.find("run: deploy/release-check.sh \"$TAG\"").unwrap();
    let build = job.find("docker/build-push-action@").unwrap();
    assert!(checkout < check && check < build, "{job}");
    assert!(job.contains("TAG: ${{ inputs.tag }}"), "{job}");
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

/// 01M3MRMASMP59PKHAV92XSV7XE: CI checks each pushed tag v*. The check
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
