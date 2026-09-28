//! A change to a wire type starts a new line of the version
//! (01M3MX1E3R5WESVHA8RZXFQR1J). `wire.json` keeps the last release and
//! the JSON schema of each wire type in it. The test fails when the
//! schema changes and the crate version stays on the line of that
//! release.
//!
//! At each release, record the release and its schema:
//!
//! ```sh
//! RIFF_BLESS=1 cargo test -p riff-core --test wire
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use riff_core::build::Semver;
use riff_core::wire::*;
use schemars::schema_for;
use serde_json::{Value, json};

/// Each wire type by name. `each_wire_type_is_in_the_schema` checks
/// that the list names each type of `wire.rs`.
macro_rules! schemas {
    ($($t:ty),* $(,)?) => {{
        let mut map = BTreeMap::new();
        $(map.insert(stringify!($t).to_owned(), schema_for!($t).to_value());)*
        map
    }};
}

fn current() -> Value {
    let mut map: BTreeMap<String, Value> = schemas!(
        Register,
        Alive,
        End,
        WhoRequest,
        WhoReply,
        SessionInfo,
        Status,
        StatusInfo,
        SetStatus,
        Threads,
        ThreadsReply,
        ThreadInfo,
        Membership,
        Post,
        Kind,
        Posted,
        Read,
        ReadReply,
        Message,
        Claim,
        ClaimReply,
        Lead,
        LeadReply,
        Start,
        Started,
        Freed,
        Riff,
        RiffReply,
        RiffState,
        Wake,
        Tailed,
        TokenRequest,
        TokenReply,
        SignInConfig,
        Discovery,
        Revoke,
        Revoked,
        Invite,
        Invited,
        Remove,
        Removed,
        SetAdmin,
        AdminSet,
        PassOwner,
        OwnerPassed,
        Members,
        MembersReply,
        TokenError,
        ResourceMetadata,
        ServerMetadata,
    );
    map.insert(
        "Content".into(),
        schema_for!(riff_core::signed::Content<'static>).to_value(),
    );
    map.insert("Jwk".into(), schema_for!(riff_core::dpop::Jwk).to_value());
    let mut value = serde_json::to_value(map).unwrap();
    strip_descriptions(&mut value);
    value
}

/// A change to a doc comment is not a change to the wire.
fn strip_descriptions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("description");
            map.values_mut().for_each(strip_descriptions);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_descriptions),
        _ => {}
    }
}

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wire.json")
}

/// The version of the crates.
fn version() -> Semver {
    env!("CARGO_PKG_VERSION").parse().unwrap()
}

/// The check: `Ok` when the schema is the schema of the release in the
/// snapshot, or `version` starts a new line after that release. Else
/// the step that fixes it.
fn check(snapshot: &Value, schema: &Value, version: Semver) -> Result<(), String> {
    let Some(release) = snapshot["release"]
        .as_str()
        .and_then(|r| r.parse::<Semver>().ok())
    else {
        return Err(
            "wire.json names no release. Record it: RIFF_BLESS=1 cargo test -p \
                    riff-core --test wire"
                .into(),
        );
    };
    if version.same_line(release) {
        if &snapshot["schema"] == schema {
            return Ok(());
        }
        return Err(format!(
            "the wire types changed since the release {release}, and the version {version} \
             stays on its line {}. Set the version {}: see \"Change a wire type\" in \
             development.md",
            release.line(),
            release.line_after()
        ));
    }
    if version > release {
        return Ok(());
    }
    Err(format!(
        "wire.json names the release {release}, after the version {version}. Record the \
         release: RIFF_BLESS=1 cargo test -p riff-core --test wire"
    ))
}

#[test]
fn the_wire_types_match_the_line_of_the_version() {
    let schema = current();
    let text = std::fs::read_to_string(path()).unwrap_or_else(|_| "{}".into());
    let snapshot: Value = serde_json::from_str(&text).unwrap();
    let result = check(&snapshot, &schema, version());
    if std::env::var_os("RIFF_BLESS").is_some() {
        // A bless never hides a change with no new line.
        if let Err(step) = result
            && step.starts_with("the wire types changed")
        {
            panic!("{step}");
        }
        let release = version().to_string();
        let text = serde_json::to_string_pretty(&json!({ "release": release, "schema": schema }));
        std::fs::write(path(), text.unwrap() + "\n").unwrap();
        return;
    }
    if let Err(step) = result {
        panic!("{step}");
    }
}

#[test]
fn a_change_to_a_wire_type_with_no_bump_of_the_minor_fails() {
    let v = |s: &str| s.parse::<Semver>().unwrap();
    let schema = current();
    let snapshot = json!({ "release": "0.4.0", "schema": schema });
    assert_eq!(check(&snapshot, &schema, v("0.4.0")), Ok(()));
    assert_eq!(check(&snapshot, &schema, v("0.4.3")), Ok(()));

    // A new field in Post, and the version stays on the line 0.4.
    let mut changed = schema.clone();
    changed["Post"]["properties"]["new"] = json!({ "type": "string" });
    for version in ["0.4.0", "0.4.1"] {
        let err = check(&snapshot, &changed, v(version)).unwrap_err();
        assert!(
            err.contains("since the release 0.4.0") && err.contains("Set the version 0.5.0"),
            "{err}"
        );
    }

    // A bump of the minor starts a new line.
    assert_eq!(check(&snapshot, &changed, v("0.5.0")), Ok(()));
    // After 1.0, the major.
    let snapshot = json!({ "release": "1.2.0", "schema": schema });
    let err = check(&snapshot, &changed, v("1.3.0")).unwrap_err();
    assert!(err.contains("Set the version 2.0.0"), "{err}");
    assert_eq!(check(&snapshot, &changed, v("2.0.0")), Ok(()));

    // A release after the version, or no release, needs a new record.
    let err = check(&snapshot, &schema, v("0.9.0")).unwrap_err();
    assert!(err.contains("RIFF_BLESS=1"), "{err}");
    let err = check(&json!({ "schema": schema }), &schema, v("0.4.0")).unwrap_err();
    assert!(err.contains("names no release"), "{err}");

    // A new doc comment is no change.
    let snapshot = json!({ "release": "0.4.0", "schema": schema });
    let mut documented = schema;
    documented["Post"]["description"] = json!("new words");
    strip_descriptions(&mut documented);
    assert_eq!(check(&snapshot, &documented, v("0.4.0")), Ok(()));
}

#[test]
fn each_wire_type_is_in_the_schema() {
    let source = include_str!("../src/wire.rs");
    let schema = current();
    for line in source.lines() {
        let rest = line
            .strip_prefix("pub struct ")
            .or_else(|| line.strip_prefix("pub enum "));
        let Some(rest) = rest else { continue };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
        assert!(
            schema.get(&name).is_some(),
            "{name} is a wire type: add it to the list in tests/wire.rs"
        );
    }
}
