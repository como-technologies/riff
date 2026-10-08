//! The round trip of each type of the log and the wire
//! (01M49W18ETF4KZJN848M91VY35).
//!
//! Each sample is written to JSON, read back and written again. The
//! two texts must be the same: a field that the reader drops, or that
//! the writer does not write, makes them differ. The schema of the type
//! then shows that the sample sets each field: each property is in the
//! JSON, no list and no map is empty, and each variant of each enum
//! shows once in the samples.
//!
//! A test of this module reads the source of each module of the log
//! and the wire, and fails for a serde type that no round trip covers.
//! A new type of the log or the wire so needs a sample here. The round
//! trip of the log through the store is in `riff-server/tests/calls.rs`,
//! and the round trip of the checkpoint in `riff-server`. Both use
//! [`Covered`] too: the feature `test-support` gives it.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The types that each round trip covered, and the variants of each
/// enum that a sample showed.
#[derive(Default)]
pub struct Covered {
    /// The name of each type that a sample covered.
    pub types: BTreeSet<String>,
    /// For each enum, by its name: the index of each variant that a
    /// sample showed.
    variants: BTreeMap<String, BTreeSet<usize>>,
    /// For each enum: its number of variants.
    counts: BTreeMap<String, usize>,
    /// Each field that a sample left out or left empty.
    missing: Vec<String>,
}

impl Covered {
    /// Writes `value`, reads it back, and writes it again. Then walks
    /// the schema of `T` over the JSON.
    pub fn trip<T: Serialize + DeserializeOwned + JsonSchema>(&mut self, value: &T) {
        let json = round_trip(value);
        let schema = schemars::schema_for!(T);
        let root = schema.as_value().clone();
        let name = T::schema_name().into_owned();
        self.walk(&root, &root, &json, &name, Some(&name));
    }

    fn walk(&mut self, root: &Value, schema: &Value, json: &Value, path: &str, name: Option<&str>) {
        if let Some(name) = name {
            self.types.insert(name.to_owned());
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let def = reference.trim_start_matches("#/$defs/");
            let target = &root["$defs"][def];
            return self.walk(root, target, json, path, Some(def));
        }
        if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
            for part in parts {
                self.walk(root, part, json, path, None);
            }
        }
        for key in ["oneOf", "anyOf"] {
            let Some(branches) = schema.get(key).and_then(Value::as_array) else {
                continue;
            };
            let real: Vec<&Value> = branches.iter().filter(|b| !is_null(b)).collect();
            let Some(index) = real.iter().position(|b| matches(root, b, json)) else {
                self.missing
                    .push(format!("{path}: no variant reads {json}"));
                return;
            };
            if real.len() > 1
                && let Some(name) = name
            {
                self.counts.insert(name.to_owned(), real.len());
                self.variants
                    .entry(name.to_owned())
                    .or_default()
                    .insert(index);
            }
            return self.walk(root, real[index], json, path, None);
        }
        if let Some(values) = schema.get("enum").and_then(Value::as_array)
            && let Some(name) = name
        {
            let index = values.iter().position(|v| v == json);
            self.counts.insert(name.to_owned(), values.len());
            if let Some(index) = index {
                self.variants
                    .entry(name.to_owned())
                    .or_default()
                    .insert(index);
            }
        }
        match json {
            Value::Object(fields) => {
                if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                    for (key, property) in properties {
                        let path = format!("{path}.{key}");
                        match fields.get(key) {
                            None | Some(Value::Null) => self.missing.push(path),
                            Some(value) => self.walk(root, property, value, &path, None),
                        }
                    }
                }
                if let Some(values) = schema.get("additionalProperties").filter(|v| v.is_object()) {
                    if fields.is_empty() {
                        self.missing.push(format!("{path}: an empty map"));
                    }
                    for (key, value) in fields {
                        self.walk(root, values, value, &format!("{path}[{key}]"), None);
                    }
                }
            }
            Value::Array(items) => {
                if items.is_empty() {
                    self.missing.push(format!("{path}: an empty list"));
                }
                if let Some(item) = schema.get("items") {
                    for (i, value) in items.iter().enumerate() {
                        self.walk(root, item, value, &format!("{path}[{i}]"), None);
                    }
                }
            }
            _ => {}
        }
    }

    /// Each field that a sample left out, and each variant that no
    /// sample showed.
    pub fn gaps(&self) -> Vec<String> {
        let mut gaps = self.missing.clone();
        for (name, count) in &self.counts {
            let shown = self.variants.get(name).map_or(0, BTreeSet::len);
            if shown < *count {
                gaps.push(format!("{name}: {shown} of {count} variants"));
            }
        }
        gaps
    }
}

/// Writes `value`, reads it back, writes it again, and checks that the
/// two texts are the same. Gives the JSON.
pub fn round_trip<T: Serialize + DeserializeOwned>(value: &T) -> Value {
    let json = serde_json::to_value(value).unwrap();
    let read: T = serde_json::from_value(json.clone()).unwrap();
    let again = serde_json::to_value(&read).unwrap();
    assert_eq!(again, json, "the round trip changed the JSON");
    json
}

fn is_null(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("null")
}

/// True when `json` can be a value of `schema`: its const, its type,
/// each required field, no field that it does not know, and the const of
/// each tag.
fn matches(root: &Value, schema: &Value, json: &Value) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let def = reference.trim_start_matches("#/$defs/");
        return matches(root, &root["$defs"][def], json);
    }
    if let Some(constant) = schema.get("const") {
        return constant == json;
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.contains(json);
    }
    let kind = match json {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let typed = match schema.get("type") {
        Some(Value::String(t)) => t == kind || (t == "number" && kind == "integer"),
        Some(Value::Array(ts)) => ts.iter().any(|t| t == kind),
        _ => true,
    };
    if !typed {
        return false;
    }
    let Value::Object(fields) = json else {
        return true;
    };
    let required = schema.get("required").and_then(Value::as_array);
    if required.is_some_and(|r| r.iter().any(|k| !fields.contains_key(k.as_str().unwrap()))) {
        return false;
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties") == Some(&Value::Bool(false))
        && let Some(properties) = properties
        && fields.keys().any(|k| !properties.contains_key(k))
    {
        return false;
    }
    properties.is_none_or(|properties| {
        properties.iter().all(|(key, property)| {
            property
                .get("const")
                .is_none_or(|constant| fields.get(key) == Some(constant))
        })
    })
}

/// The names of the serde types of `source`: each type with a derive of
/// `Serialize` or `Deserialize`, and each type with an impl of one.
pub fn serde_types(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut derive = String::new();
    let mut in_derive = false;
    for line in source.lines() {
        if line.starts_with("#[derive(") {
            in_derive = true;
            derive.clear();
        }
        if in_derive {
            derive.push_str(line);
            let item = [
                "pub struct ",
                "pub enum ",
                "pub(crate) struct ",
                "struct ",
                "enum ",
            ]
            .iter()
            .find_map(|start| line.strip_prefix(start));
            if let Some(item) = item {
                in_derive = false;
                if derive.contains("Serialize") || derive.contains("Deserialize") {
                    names.insert(type_name(item));
                }
            }
        }
        for start in ["impl Serialize for ", "impl<'de> Deserialize<'de> for "] {
            if let Some(item) = line.strip_prefix(start) {
                names.insert(type_name(item));
            }
        }
    }
    names
}

fn type_name(item: &str) -> String {
    item.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::{SessionUri, ThreadName, Who};
    use crate::record::{By, Envelope, Plan, PlanItem, PlanSet, Record, Wave, one_of_each};
    use crate::selector::Selector;
    use crate::signed::{Content, Signed};
    use crate::wire::*;

    fn uri() -> SessionUri {
        "riff://ann@heron/acme/app?session=s1&lead=true&claim=issue-7#issue-7"
            .parse()
            .unwrap()
    }

    fn thread() -> ThreadName {
        "acme/app".parse().unwrap()
    }

    fn who() -> Who {
        uri().who().clone()
    }

    fn text(s: &str) -> String {
        s.to_owned()
    }

    fn selector() -> Selector {
        Selector {
            user: Some(text("ann")),
            session: Some(text("s1")),
            host: Some(text("heron")),
            repo: Some(text("acme/app")),
            worktree: Some(text("issue-7")),
            claim: Some(text("issue-7")),
            lead: Some(true),
            other: None,
        }
    }

    fn keys() -> Keys {
        Keys::from([(text("ann"), vec![text("jkt")])])
    }

    fn message() -> Message {
        Message {
            seq: 1,
            from: uri(),
            to: vec![selector()],
            body: text("hi"),
            at_ms: 5,
            kind: Kind::Status,
            sig: Some(text("h.p.s")),
            payload: Some(text("cA")),
        }
    }

    fn activity() -> Activity {
        Activity {
            tool: Some(text("Bash")),
            turn: true,
            secs: 3,
        }
    }

    fn status() -> Status {
        Status {
            step: text("tests"),
        }
    }

    fn pause() -> PauseInfo {
        PauseInfo {
            by: Some(By::Session(who())),
            at_ms: 9,
        }
    }

    fn session_info(state: SessionState, waits: Waits) -> SessionInfo {
        SessionInfo {
            uri: uri(),
            live: true,
            idle_secs: 4,
            status: Some(StatusInfo {
                status: status(),
                age_secs: 2,
                stale: true,
            }),
            worker: true,
            stopping: true,
            claims_secs: 6,
            must_clear: true,
            fresh_secs: Some(7),
            state: Some(state),
            work: Some(activity()),
            waits: Some(waits),
            blocked: Some(BlockedInfo {
                reason: text("a question"),
                secs: 8,
                answered: true,
                woken_again: true,
                unanswered: true,
            }),
            step: Some(StepInfo {
                name: text("live window"),
                secs: 9,
                failed: Some(text("502")),
            }),
        }
    }

    fn person(role: PersonRole) -> Person {
        Person {
            user: text("ann"),
            role,
            live: true,
            seen_secs: Some(3),
        }
    }

    fn full_envelope(position: u64) -> Envelope {
        Envelope {
            position,
            written_at_ms: 1_790_000_000_000 + position,
            by: Some(By::Session(who())),
            command: Some(text("claim")),
            call: Some(format!("call-{position}")),
        }
    }

    /// One sample of each type of the wire, with each variant of each enum.
    fn wire(c: &mut Covered) {
        let me = uri;
        c.trip(&Register {
            me: me(),
            worker: true,
        });
        c.trip(&Alive {
            me: me(),
            activity: Some(activity()),
            prompt_secs: Some(4),
        });
        c.trip(&AliveReply {
            stop: true,
            clear: true,
        });
        c.trip(&End { me: me() });
        c.trip(&WhoRequest {
            me: me(),
            all: true,
        });
        let waits = [
            Waits::Verify { pull: 1 },
            Waits::Merge { pull: 2 },
            Waits::Needs { issues: vec![3] },
        ];
        let states = [
            SessionState::Offline,
            SessionState::Paused,
            SessionState::Blocked,
            SessionState::MustClear,
            SessionState::Waiting,
            SessionState::Busy,
            SessionState::Idle,
        ];
        let sessions: Vec<SessionInfo> = states
            .iter()
            .zip(waits.iter().cycle())
            .map(|(state, waits)| session_info(*state, waits.clone()))
            .collect();
        for owner in [
            RiffOwner::NoSignIn,
            RiffOwner::Nobody,
            RiffOwner::Owner {
                user: text("ann"),
                email: text("ann@acme.io"),
            },
        ] {
            c.trip(&WhoReply {
                sessions: sessions.clone(),
                owner,
                people: vec![
                    person(PersonRole::Owner),
                    person(PersonRole::Admin),
                    person(PersonRole::Member),
                ],
            });
        }
        c.trip(&MeReply {
            session: Some(sessions[0].clone()),
            build: text("1.2.0"),
        });
        c.trip(&ServerFacts {
            not_serving: Some(text("a later format")),
            last_error: Some(FactError {
                message: text("a 503"),
                at_ms: 1,
            }),
            position: 2,
            chunk_written_at_ms: Some(3),
            chunk_write_ms: Some(4),
            write_errors: 5,
            skipped_records: 6,
            checkpoint: Some(CheckpointFacts {
                position: 7,
                written_at_ms: 8,
                build: text("1.2.0"),
            }),
            no_checkpoint: Some(text("none yet")),
            chunks: 9,
            sessions: 10,
            cursors: 11,
            threads: 12,
            sign_ins: 13,
            memory_bytes: Some(14),
            started_at_ms: 15,
            replay_ms: 16,
            now_ms: 17,
        });
        c.trip(&SetStatus {
            me: me(),
            status: status(),
        });
        c.trip(&SetBlocked {
            me: me(),
            reason: text("a question"),
        });
        for change in [
            StepChange::Start {
                name: text("live window"),
            },
            StepChange::Done,
            StepChange::Fail {
                reason: text("502"),
            },
        ] {
            c.trip(&SetStep { me: me(), change });
        }
        c.trip(&BlockedLook {
            me: me(),
            after_secs: 60,
        });
        c.trip(&BlockedLookReply {
            unanswered: vec![Unanswered {
                session: me(),
                reason: text("a question"),
            }],
        });
        c.trip(&ItemFacts {
            me: me(),
            items: [
                PullState::Asked,
                PullState::Passed,
                PullState::Failed,
                PullState::Merged,
            ]
            .into_iter()
            .map(|state| ItemFact {
                item: text("issue-7"),
                pull: Some(PullFact { number: 40, state }),
                needs: vec![12],
            })
            .collect(),
            all: true,
        });
        c.trip(&Threads { me: me() });
        c.trip(&ThreadsReply {
            threads: vec![ThreadInfo {
                thread: thread(),
                members: vec![me()],
                unread: 2,
            }],
        });
        c.trip(&Join {
            me: me(),
            thread: thread(),
        });
        c.trip(&Leave {
            me: me(),
            thread: thread(),
        });
        // A post, a message and a wake write no kind `message`.
        for kind in [Kind::Message, Kind::Status, Kind::Note] {
            c.trip(&kind);
        }
        for kind in [Kind::Status, Kind::Note] {
            c.trip(&Post {
                me: me(),
                thread: Some(thread()),
                to: vec![selector()],
                body: text("hi"),
                kind,
                at_ms: Some(5),
                sig: Some(text("h.p.s")),
                payload: Some(text("cA")),
            });
        }
        c.trip(&Posted {
            thread: thread(),
            seq: 1,
            woken: vec![me()],
            unmatched: vec![selector()],
        });
        c.trip(&Read {
            me: me(),
            thread: thread(),
            all: true,
            after: Some(3),
        });
        c.trip(&ReadReply {
            messages: vec![message()],
            next: Some(1),
            keys: keys(),
            trusted: true,
        });
        c.trip(&Claim {
            me: me(),
            thread: thread(),
            item: text("issue-7"),
        });
        c.trip(&ClaimReply {
            holder: me(),
            warning: Some(text("held by a worker")),
        });
        c.trip(&Hold {
            me: me(),
            thread: thread(),
            item: text("issue-7"),
            reason: text("waits for ann"),
        });
        c.trip(&HoldReply {
            changed: true,
            holder: Some(me()),
        });
        c.trip(&Free {
            me: me(),
            thread: thread(),
            item: text("issue-7"),
        });
        c.trip(&FreeReply { freed: true });
        let plan = Plan {
            wave: Some(Wave {
                number: 3,
                title: text("Wave 3"),
            }),
            items: vec![PlanItem {
                item: text("issue-7"),
                needs: vec![text("issue-6")],
            }],
            done: vec![text("issue-6")],
        };
        // A flattened type has no name in the schema of its holder.
        c.trip(&plan);
        c.trip(&SetPlan {
            me: me(),
            base: Some(3),
            plan: PlanSet {
                thread: thread(),
                plan: plan.clone(),
            },
        });
        c.trip(&PlanOff {
            me: me(),
            thread: thread(),
        });
        c.trip(&PlanOffReply { ended: true });
        c.trip(&PlanSeen {
            me: me(),
            thread: thread(),
            position: 3,
        });
        c.trip(&PlanShow {
            me: me(),
            thread: thread(),
        });
        c.trip(&PlanReply {
            plan: Some(PlanShown {
                plan,
                position: 3,
                set_ms: 5,
                seen_ms: Some(6),
                stale: true,
                holders: BTreeMap::from([(text("issue-7"), me())]),
            }),
            holds: BTreeMap::from([(
                text("issue-8"),
                HoldInfo {
                    reason: text("waits for ann"),
                    by: Some(By::Session(who())),
                    at_ms: 5,
                },
            )]),
        });
        c.trip(&Release {
            me: me(),
            thread: thread(),
            item: text("issue-7"),
        });
        c.trip(&ReleaseReply { must_clear: true });
        c.trip(&ReleaseFor {
            me: me(),
            thread: thread(),
            item: text("issue-7"),
            session: text("s2"),
        });
        c.trip(&Lead { me: me() });
        c.trip(&LeadReply {
            lead: me(),
            replaced: Some(me()),
        });
        for reason in [
            StartReason::Process,
            StartReason::Resume,
            StartReason::Clear,
            StartReason::Join,
            StartReason::Other,
        ] {
            c.trip(&Start {
                me: me(),
                reason,
                worker: true,
            });
        }
        c.trip(&Started {
            freed: vec![Freed {
                thread: thread(),
                item: text("issue-7"),
            }],
        });
        c.trip(&RiffQuery { me: me() });
        c.trip(&Pause {
            me: me(),
            riff: true,
            repository: Some(thread()),
        });
        c.trip(&Resume {
            me: me(),
            riff: true,
            repository: Some(thread()),
        });
        for state in [RiffState::Paused, RiffState::Running] {
            c.trip(&RiffReply {
                state,
                changed: true,
                riff: Some(pause()),
                repositories: vec![RepositoryPause {
                    repository: thread(),
                    pause: pause(),
                }],
            });
        }
        c.trip(&IdleQuery { me: me() });
        {
            use crate::forge::{Access, TokenRole};
            use crate::wire::{
                ForgeAccounts, ForgeAllow, ForgeCheck, ForgeCheckReply, ForgeCreate,
                ForgeCreateReply, ForgeCreated, ForgeCreatedReply, ForgeInstall, ForgeInstallReply,
                ForgeToken, ForgeTokenReply, RoleCheck,
            };
            let permissions = BTreeMap::from([
                ("contents".to_owned(), Access::Write),
                ("metadata".to_owned(), Access::Read),
            ]);
            c.trip(&ForgeToken { me: me() });
            c.trip(&ForgeTokenReply {
                role: TokenRole::Worker,
                repo: "acme/app".into(),
                token: "ghs_x".into(),
                ends_ms: 1,
                permissions: permissions.clone(),
            });
            c.trip(&TokenRole::Lead);
            c.trip(&ForgeCheck { me: me() });
            c.trip(&ForgeCheckReply {
                repo: "acme/app".into(),
                app: 7,
                roles: vec![RoleCheck {
                    role: TokenRole::Verifier,
                    permissions,
                    error: Some("no".into()),
                }],
            });
            c.trip(&ForgeAllow {
                me: me(),
                owner: Some("acme".into()),
                allowed: true,
            });
            c.trip(&ForgeAccounts {
                accounts: vec!["acme".into()],
            });
            c.trip(&ForgeCreate {
                me: me(),
                org: "acme".into(),
            });
            c.trip(&ForgeCreateReply {
                url: "https://riff.example/forge/new?state=s".into(),
                state: "s".into(),
            });
            c.trip(&ForgeCreated {
                me: me(),
                state: "s".into(),
            });
            c.trip(&ForgeCreatedReply {
                app: Some(7),
                slug: Some("riff-acme".into()),
                installed: true,
                error: Some("no".into()),
            });
            c.trip(&ForgeInstall {
                me: me(),
                owner: "acme".into(),
            });
            c.trip(&ForgeInstallReply {
                url: "https://github.com/apps/riff-acme/installations/new".into(),
                installed: false,
            });
        }
        c.trip(&SetIdle {
            me: me(),
            per_host: Some(1),
            after_secs: Some(300),
        });
        c.trip(&Idle {
            per_host: 1,
            after_secs: 300,
        });
        c.trip(&Wake {
            thread: thread(),
            seq: 1,
            from: me(),
            kind: Kind::Note,
        });
        c.trip(&Tailed {
            thread: thread(),
            message: message(),
            keys: keys(),
            trusted: true,
        });
        c.trip(&TokenRequest {
            grant_type: text("refresh_token"),
            refresh_token: Some(text("r")),
            subject_token: Some(text("s")),
            subject_token_type: Some(text("t")),
            session: Some(text("s1")),
            requested_token_type: Some(text("g")),
            session_proof: Some(text("p")),
            resource: Some(text("https://riff.example")),
        });
        c.trip(&GrantEnd { token: text("g") });
        c.trip(&TokenReply {
            access_token: text("a"),
            token_type: text("DPoP"),
            expires_in: 3600,
            refresh_token: text("r"),
            user: text("ann"),
        });
        c.trip(&SignInConfig {
            issuer: text("https://accounts.example"),
            client_id: text("id"),
            client_secret: Some(text("secret")),
            riff_id: text("r1"),
        });
        c.trip(&Discovery {
            issuer: text("https://accounts.example"),
            authorization_endpoint: text("https://accounts.example/auth"),
            token_endpoint: text("https://accounts.example/token"),
            jwks_uri: text("https://accounts.example/jwks"),
        });
        c.trip(&Revoke {
            user: Some(text("ann")),
        });
        c.trip(&Revoked {
            user: text("ann"),
            sign_ins: 2,
        });
        c.trip(&Invite {
            email: text("bob@acme.io"),
        });
        c.trip(&Invited {
            email: text("bob@acme.io"),
            address: text("https://riff.example"),
        });
        c.trip(&Remove {
            email: text("bob@acme.io"),
        });
        c.trip(&Removed {
            email: text("bob@acme.io"),
            sign_ins: 1,
        });
        c.trip(&SetAdmin {
            email: text("bob@acme.io"),
            admin: true,
        });
        c.trip(&AdminSet {
            email: text("bob@acme.io"),
            admin: true,
        });
        c.trip(&PassOwner {
            email: text("bob@acme.io"),
        });
        c.trip(&OwnerPassed {
            owner: text("bob@acme.io"),
            admin: text("ann@acme.io"),
        });
        c.trip(&TakeOwner {});
        c.trip(&OwnerAsked {
            admin: text("bob@acme.io"),
            owner: Some(text("ann@acme.io")),
            answer_secs: 600,
        });
        c.trip(&DenyOwner {});
        c.trip(&OwnerDenied {
            owner: text("ann@acme.io"),
            admin: text("bob@acme.io"),
        });
        c.trip(&LogQuery { repo: thread() });
        c.trip(&Members {});
        c.trip(&MembersReply {
            owner: Some(text("ann@acme.io")),
            admins: vec![text("bob@acme.io")],
            members: vec![text("cy@acme.io")],
            allowed_domains: vec![text("acme.io")],
        });
        c.trip(&TokenError {
            error: text("invalid_grant"),
            error_description: Some(text("the refresh token ended")),
        });
        c.trip(&ResourceMetadata {
            resource: text("https://riff.example"),
            authorization_servers: vec![text("https://riff.example")],
            bearer_methods_supported: vec![text("header")],
            dpop_signing_alg_values_supported: vec![text("ES256")],
            dpop_bound_access_tokens_required: true,
        });
        c.trip(&ServerMetadata {
            issuer: text("https://riff.example"),
            token_endpoint: text("https://riff.example/token"),
            grant_types_supported: vec![text("refresh_token")],
            response_types_supported: vec![text("none")],
            code_challenge_methods_supported: vec![text("S256")],
            token_endpoint_auth_methods_supported: vec![text("none")],
            dpop_signing_alg_values_supported: vec![text("ES256")],
        });
    }

    /// Each record of [`one_of_each`] with each field of its envelope set.
    fn records() -> Vec<Record> {
        one_of_each()
            .into_iter()
            .zip(1..)
            .map(|(change, position)| Record {
                envelope: full_envelope(position),
                change,
            })
            .collect()
    }

    /// The types with no schema: a round trip of their own covers each.
    fn with_no_schema(c: &mut Covered) {
        // The reply of the log holds the records as they are.
        let reply = LogReply { records: records() };
        round_trip(&reply);
        let read: LogReply = serde_json::from_value(serde_json::to_value(&reply).unwrap()).unwrap();
        assert_eq!(read, reply);
        c.types.insert(text("LogReply"));

        // The sender writes a content; a reader reads it as `Signed`.
        let (who, thread, to) = (who(), thread(), [selector()]);
        let content = Content {
            from: &who,
            lead: true,
            thread: Some(&thread),
            to: &to,
            body: "hi",
            kind: Kind::Note,
            at_ms: 5,
        };
        let json = serde_json::to_value(content).unwrap();
        let signed: Signed = serde_json::from_value(json.clone()).unwrap();
        assert!(signed.covers(&content));
        assert_eq!(serde_json::to_value(&signed).unwrap(), json);
        c.types.insert(text("Payload"));
    }

    #[test]
    fn each_type_of_the_log_and_the_wire_reads_back_with_each_field() {
        let mut c = Covered::default();
        wire(&mut c);
        for record in records() {
            c.trip(&record);
        }
        c.trip(&full_envelope(1));
        with_no_schema(&mut c);
        assert_eq!(c.gaps(), Vec::<String>::new());
    }

    /// A new serde type in a module of the log or the wire fails here until
    /// a sample covers it.
    #[test]
    fn each_serde_type_has_a_round_trip() {
        let mut c = Covered::default();
        wire(&mut c);
        for record in records() {
            c.trip(&record);
        }
        c.trip(&full_envelope(1));
        with_no_schema(&mut c);
        // Each private part of a proof or a signature has its own tests in
        // `dpop` and `signed`.
        let by_their_module = ["Header", "Claims", "Jwk"];
        let mut not_covered = Vec::new();
        for (module, source) in [
            ("record", include_str!("record.rs")),
            ("wire", include_str!("wire.rs")),
            ("selector", include_str!("selector.rs")),
            ("signed", include_str!("signed.rs")),
            ("dpop", include_str!("dpop.rs")),
        ] {
            for name in serde_types(source) {
                if !c.types.contains(&name) && !by_their_module.contains(&name.as_str()) {
                    not_covered.push(format!("{module}::{name}"));
                }
            }
        }
        assert_eq!(not_covered, Vec::<String>::new());
    }

    /// The scan finds each serde type: by a derive on one or more lines,
    /// and by a hand impl. A type with no serde is not in it.
    #[test]
    fn the_scan_finds_each_serde_type() {
        let source = "#[derive(Clone, Serialize)]\npub struct A {\n}\n\
            #[derive(\n    Clone,\n    Deserialize,\n)]\npub enum B {}\n\
            #[derive(Clone)]\nstruct C;\n\
            impl Serialize for D {\n}\nimpl<'de> Deserialize<'de> for E {\n}\n";
        let names: Vec<String> = serde_types(source).into_iter().collect();
        assert_eq!(names, ["A", "B", "D", "E"]);
    }

    /// The check of the samples finds a field that a sample leaves out.
    #[test]
    fn a_sample_with_no_optional_field_shows_a_gap() {
        let mut c = Covered::default();
        c.trip(&ClaimReply {
            holder: uri(),
            warning: None,
        });
        assert_eq!(c.gaps(), ["ClaimReply.warning"]);
    }
}
