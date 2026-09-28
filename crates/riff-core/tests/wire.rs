//! A change to a wire type bumps the wire version
//! (01M3MNVT7G701SDP1Z1THMRDQ2). `wire.json` keeps the wire version and
//! the JSON schema of each wire type. The test fails when the schema
//! changes and `WIRE` stays the same.
//!
//! After a bump of `WIRE`, record the new schema:
//!
//! ```sh
//! RIFF_BLESS=1 cargo test -p riff-core --test wire
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use riff_core::build::WIRE;
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

/// The check: `Ok` when the snapshot holds this schema and this wire
/// version. Else the step that fixes it.
fn check(snapshot: &Value, schema: &Value, wire: u32) -> Result<(), String> {
    let same_schema = &snapshot["schema"] == schema;
    let same_wire = snapshot["wire"] == json!(wire);
    match (same_schema, same_wire) {
        (true, true) => Ok(()),
        (false, true) => Err(format!(
            "the wire types changed, and WIRE stays {wire}. Bump WIRE in \
             crates/riff-core/src/build.rs, then run: RIFF_BLESS=1 cargo test -p riff-core \
             --test wire"
        )),
        _ => Err(format!(
            "WIRE is {wire}, and wire.json has another schema or wire version. Record them: \
             RIFF_BLESS=1 cargo test -p riff-core --test wire"
        )),
    }
}

#[test]
fn the_wire_types_match_the_wire_version() {
    let schema = current();
    let text = std::fs::read_to_string(path()).unwrap_or_else(|_| "{}".into());
    let snapshot: Value = serde_json::from_str(&text).unwrap();
    let result = check(&snapshot, &schema, WIRE);
    if std::env::var_os("RIFF_BLESS").is_some() {
        // A bless never hides a change with no bump.
        if snapshot["wire"] == json!(WIRE) && snapshot["schema"] != schema {
            panic!("{}", result.unwrap_err());
        }
        let text = serde_json::to_string_pretty(&json!({ "wire": WIRE, "schema": schema }));
        std::fs::write(path(), text.unwrap() + "\n").unwrap();
        return;
    }
    if let Err(step) = result {
        panic!("{step}");
    }
}

#[test]
fn a_change_to_a_wire_type_with_no_bump_fails() {
    let schema = current();
    let snapshot = json!({ "wire": WIRE, "schema": schema });
    assert_eq!(check(&snapshot, &schema, WIRE), Ok(()));

    // A new field in Post, and WIRE stays the same.
    let mut changed = schema.clone();
    changed["Post"]["properties"]["new"] = json!({ "type": "string" });
    let err = check(&snapshot, &changed, WIRE).unwrap_err();
    assert!(err.contains("Bump WIRE"), "{err}");

    // With a bump, the step is to record the new schema.
    let err = check(&snapshot, &changed, WIRE + 1).unwrap_err();
    assert!(err.contains("RIFF_BLESS=1"), "{err}");

    // A new doc comment is no change.
    let mut documented = schema;
    documented["Post"]["description"] = json!("new words");
    strip_descriptions(&mut documented);
    assert_eq!(check(&snapshot, &documented, WIRE), Ok(()));
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
