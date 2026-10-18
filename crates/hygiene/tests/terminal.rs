//! `riff-client` holds no terminal code: it depends on no crate of the
//! terminal, also not through another crate (01M54216PBE319RASGNMY9F0MR).

use std::path::Path;

use hygiene::terminal;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn riff_client_has_no_terminal_crate() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let manifest = read(&crates.join("riff-client/Cargo.toml"));
    let lock = read(&crates.parent().unwrap().join("Cargo.lock"));
    // The check reaches the crates of riff-client through the lock file.
    let roots = terminal::dependencies(&manifest);
    assert!(roots.iter().any(|name| name == "riff-core"), "{roots:?}");
    assert!(lock.contains("name = \"riff-client\""));
    assert_eq!(terminal::found(&manifest, &lock), Vec::<String>::new());
}

/// The control: the check fails on a sample `Cargo.toml` with ratatui,
/// with the lock file of the workspace.
#[test]
fn the_check_fails_on_a_manifest_with_ratatui() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let lock = read(&crates.parent().unwrap().join("Cargo.lock"));
    let sample = "[package]\nname = \"sample\"\n\n[dependencies]\n\
                  riff-core.workspace = true\nratatui.workspace = true\n";
    // ratatui brings crossterm.
    assert_eq!(terminal::found(sample, &lock), ["crossterm", "ratatui"]);
    // riff depends on clap, ratatui and rustyline itself, and on
    // crossterm through ratatui.
    let riff = read(&crates.join("riff/Cargo.toml"));
    assert_eq!(
        terminal::found(&riff, &lock),
        ["clap", "crossterm", "ratatui", "rustyline"]
    );
}
