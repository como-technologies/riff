//! `.github/dependabot.yml` keeps the actions of the pipeline on a
//! version that GitHub supports.

use std::fs;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn dependabot_updates_the_github_actions_each_week() {
    let text = fs::read_to_string(root().join(".github/dependabot.yml")).unwrap();
    assert!(text.contains("package-ecosystem: github-actions"), "{text}");
    assert!(text.contains("interval: weekly"), "{text}");
}

#[test]
fn no_workflow_uses_an_action_that_runs_on_node_20() {
    let old = [
        "actions/checkout@v6",
        "actions/cache@v4",
        "actions/upload-artifact@v4",
        "actions/download-artifact@v4",
        "actions/upload-pages-artifact@v4",
        "actions/deploy-pages@v4",
    ];
    for entry in fs::read_dir(root().join(".github/workflows")).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        for action in old {
            assert!(!text.contains(action), "{} uses {action}", path.display());
        }
    }
}
