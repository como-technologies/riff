//! The cloud scripts in `deploy/`, the image and the CI deploy (R6,
//! R46, R134-R136, R160, R161).
//! The tests run each script with a fake `gcloud` that writes each call
//! to a log.

use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls");
    let found_file = dir.path().join("found");
    fs::write(&found_file, found.join("\n") + "\n").unwrap();
    let fake = dir.path().join("gcloud");
    fs::write(
        &fake,
        format!(
            r#"#!/bin/sh
echo "$*" >> {log}
case "$*" in
    "billing projects describe"*) echo True; exit 0 ;;
    "projects describe"*|"secrets describe"*) exit 0 ;;
    "secrets versions list"*) echo 1; exit 0 ;;
    *" describe "*)
        while IFS= read -r f; do
            [ -n "$f" ] || continue
            case "$*" in "$f"*) exit 0 ;; esac
        done < {found}
        exit 1 ;;
esac
"#,
            log = log.display(),
            found = found_file.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap()
    );
    let out = Command::new(deploy().join(script))
        .args(args)
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::read_to_string(log).unwrap()
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
        "RIFF_PUBLIC_URL=https://riff.comotechnologies.io,",
        "RIFF_REQUIRE_SIGN_IN=true,",
        "RIFF_BUCKET=como-riff-state",
        "--set-secrets RIFF_OIDC_CLIENT_SECRET=riff-oidc-client-secret:latest",
    ] {
        assert!(deploy.contains(flag), "{flag} is not in: {deploy}");
    }
}

#[test]
fn deploy_maps_the_domain_once() {
    let calls = run("deploy.sh", &[]);
    let map = line(
        &calls,
        "beta run domain-mappings create --service riff-server ",
    );
    assert!(map.contains("--domain riff.comotechnologies.io"), "{map}");
    assert!(map.contains("--region us-central1"), "{map}");

    let calls = run("deploy.sh", &["beta run domain-mappings describe"]);
    assert!(!calls.contains("domain-mappings create"), "{calls}");
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
    let text = fs::read_to_string(deploy().join("../.github/workflows/ci.yml")).unwrap();
    let job = text
        .split_once("\n  deploy:\n")
        .unwrap()
        .1
        .split_once("\n  audit:\n")
        .unwrap()
        .0;
    for part in [
        "needs: gate",
        "github.ref == 'refs/heads/main'",
        "id-token: write",
        "google-github-actions/auth@",
        "workload_identity_provider:",
        "deploy/deploy.sh --image \"$IMAGE\"",
    ] {
        assert!(job.contains(part), "{part} is not in the deploy job");
    }
    assert!(
        !job.contains("credentials_json"),
        "the job must not use a key"
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
