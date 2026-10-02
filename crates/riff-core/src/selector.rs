//! Selectors: the address of a post (R51).
//!
//! # Rules
//!
//! - A post has a `to` list of selectors. Text in the body never wakes
//!   a session.
//! - A selector names one or more fields. A field that it leaves out
//!   matches each session.
//! - A session matches a selector when each named field matches. A post
//!   wakes each session that matches one or more selectors.
//!
//! | Field | Matches |
//! |---|---|
//! | `user` | The user. |
//! | `session` | The session ID. |
//! | `host` | The host. |
//! | `repo` | `OWNER/REPO`, or `-` outside git. |
//! | `worktree` | The worktree. The main worktree has none. |
//! | `claim` | One of the claims that the session holds. |
//! | `lead` | `true`: the lead of its user in its repository. `false`: each other session. |
//!
//! On the command line, a selector is `FIELD=VALUE` pairs with commas
//! between them.
//!
//! # A selector of a later build (01M3XSF90E9JYYTC13D9THY4WE)
//!
//! The set of fields can grow, and so can the form of a selector. A
//! selector that this build does not know is a selector `other`: an
//! object with a field or a value that the build does not know, and
//! each other form of JSON (a text, a list, a number, `null`). It keeps
//! its JSON as it came ([`Selector::other`]), and writes it again. So a
//! reader of a later build gets the selector as it was, and the check
//! of a signed message covers it. A selector `other` matches no
//! session, because a field that the build cannot check never makes a
//! selector wider. A `post` call with such a selector is refused.
//!
//! ```
//! use riff_core::name::SessionUri;
//! use riff_core::selector::Selector;
//!
//! let uri: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
//! for json in [r#"{"user":"mike","wave":"17"}"#, r#"{"user":null,"wave":"17"}"#, r#""all""#, "null"] {
//!     let later: Selector = serde_json::from_str(json).unwrap();
//!     assert!(later.is_other() && !later.matches(&uri), "{json}");
//!     assert_eq!(later.to_string(), json);
//!     assert_eq!(serde_json::to_string(&later).unwrap(), json);
//! }
//! // The same selector with only the field that the build knows.
//! assert!("user=mike".parse::<Selector>()?.matches(&uri));
//! # Ok::<(), riff_core::name::NameError>(())
//! ```
//!
//! # Example
//!
//! ```
//! use riff_core::name::SessionUri;
//! use riff_core::selector::Selector;
//!
//! let uri: SessionUri =
//!     "riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#api".parse()?;
//!
//! let mikes_on_pangolin: Selector = "user=mike,host=pangolin".parse()?;
//! assert!(mikes_on_pangolin.matches(&uri));
//!
//! let holder: Selector = "claim=issue-6".parse()?;
//! assert!(holder.matches(&uri));
//!
//! let brett: Selector = "user=brett".parse()?;
//! assert!(!brett.matches(&uri));
//!
//! let mikes_lead: Selector = "user=mike,repo=como-technologies/riff,lead=true".parse()?;
//! assert!(!mikes_lead.matches(&uri));
//! assert!(mikes_lead.matches(&uri.with_lead(true)));
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::{NameError, SessionUri};

/// Picks sessions by who they are, where they work, and what they hold.
/// Each field that is set must match. Set one or more fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Selector {
    /// The user, for example `mike`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The session ID from the `session` part of a session URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The host, for example `pangolin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The repository as `OWNER/REPO`, for example `como-technologies/riff`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The worktree, for example `issue-6`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// A claimed work item, for example `issue-6`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    /// True picks the lead of its user in its repository. Use it with
    /// `user` and `repo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<bool>,
    /// The JSON of a selector that this build does not know, as it
    /// came. Each field above is then empty, and the selector matches
    /// no session.
    #[schemars(skip)]
    pub other: Option<serde_json::Value>,
}

/// The selector of this build, as its JSON has it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Known {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lead: Option<bool>,
}

/// A selector `other` writes its JSON as it came.
impl Serialize for Selector {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(other) = &self.other {
            return other.serialize(serializer);
        }
        let selector = self.clone();
        Known {
            user: selector.user,
            session: selector.session,
            host: selector.host,
            repo: selector.repo,
            worktree: selector.worktree,
            claim: selector.claim,
            lead: selector.lead,
        }
        .serialize(serializer)
    }
}

/// A build reads each selector that it does not know as a selector
/// `other`: an object with a field or a value that it does not know, and
/// each other form of JSON (01M3XSF90E9JYYTC13D9THY4WE).
impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        // Only an object is a selector of this build: a list of values
        // in the order of the fields is not.
        let known = value
            .is_object()
            .then(|| Known::deserialize(&value).ok())
            .flatten();
        Ok(match known {
            Some(known) => Selector {
                user: known.user,
                session: known.session,
                host: known.host,
                repo: known.repo,
                worktree: known.worktree,
                claim: known.claim,
                lead: known.lead,
                other: None,
            },
            None => Selector {
                other: Some(value),
                ..Selector::default()
            },
        })
    }
}

impl Selector {
    /// True for a selector `other`: a selector that this build does not
    /// know.
    pub fn is_other(&self) -> bool {
        self.other.is_some()
    }

    /// A selector for one session.
    pub fn session(id: &str) -> Self {
        Self {
            session: Some(id.to_owned()),
            ..Self::default()
        }
    }

    /// A selector for the lead of `user` in the repository `repo`.
    pub fn lead(user: &str, repo: &str) -> Self {
        Self {
            user: Some(user.to_owned()),
            repo: Some(repo.to_owned()),
            lead: Some(true),
            ..Self::default()
        }
    }

    /// True when the selector names no field.
    pub fn is_empty(&self) -> bool {
        self.lead.is_none() && !self.is_other() && self.fields().iter().all(|(_, v)| v.is_none())
    }

    /// True when each named field matches the session. A selector
    /// `other` matches no session.
    pub fn matches(&self, uri: &SessionUri) -> bool {
        let place = uri.place();
        let who = uri.who();
        let eq = |want: &Option<String>, have: Option<&str>| {
            want.as_deref().is_none_or(|w| Some(w) == have)
        };
        !self.is_empty()
            && !self.is_other()
            && eq(&self.user, Some(who.user()))
            && eq(&self.session, who.session())
            && eq(&self.host, Some(place.host()))
            && eq(&self.repo, Some(&place.repo_text()))
            && eq(&self.worktree, place.worktree())
            && self.claim.as_ref().is_none_or(|c| uri.claims().contains(c))
            && self.lead.is_none_or(|l| l == uri.lead())
    }

    fn fields(&self) -> [(&'static str, &Option<String>); 6] {
        [
            ("user", &self.user),
            ("session", &self.session),
            ("host", &self.host),
            ("repo", &self.repo),
            ("worktree", &self.worktree),
            ("claim", &self.claim),
        ]
    }
}

impl fmt::Display for Selector {
    /// The `FIELD=VALUE` pairs. A selector `other` shows its JSON.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(other) = &self.other {
            return write!(f, "{other}");
        }
        let pairs: Vec<String> = self
            .fields()
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={v}")))
            .chain(self.lead.map(|l| format!("lead={l}")))
            .collect();
        f.write_str(&pairs.join(","))
    }
}

impl FromStr for Selector {
    type Err = NameError;

    /// Parses `FIELD=VALUE` pairs with commas between them.
    ///
    /// ```
    /// use riff_core::selector::Selector;
    ///
    /// let s: Selector = "user=mike,claim=issue-6".parse()?;
    /// assert_eq!(s.to_string(), "user=mike,claim=issue-6");
    /// let lead: Selector = "user=mike,lead=true".parse()?;
    /// assert_eq!(lead.lead, Some(true));
    /// assert!("lead=yes".parse::<Selector>().is_err());
    /// assert!("".parse::<Selector>().is_err());
    /// assert!("colour=blue".parse::<Selector>().is_err());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = Self::default();
        for pair in s.split(',').filter(|p| !p.is_empty()) {
            let (key, value) = pair
                .split_once('=')
                .filter(|(_, v)| !v.is_empty())
                .ok_or_else(|| NameError::new(format!("not FIELD=VALUE: {pair}")))?;
            if key == "lead" {
                let lead = value
                    .parse()
                    .map_err(|_| NameError::new(format!("lead is true or false, not {value}")))?;
                out.lead = Some(lead);
                continue;
            }
            let field = match key {
                "user" => &mut out.user,
                "session" => &mut out.session,
                "host" => &mut out.host,
                "repo" => &mut out.repo,
                "worktree" => &mut out.worktree,
                "claim" => &mut out.claim,
                _ => {
                    return Err(NameError::new(format!(
                        "no field named {key}. Use user, session, host, repo, worktree, claim or lead."
                    )));
                }
            };
            *field = Some(value.to_owned());
        }
        if out.is_empty() {
            return Err(NameError::new("a selector needs one or more fields"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    fn sel(text: &str) -> Selector {
        text.parse().unwrap()
    }

    #[test]
    fn each_field_selects_on_its_own() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#api");
        for s in [
            "user=mike",
            "session=a6cf",
            "host=pangolin",
            "repo=como-technologies/riff",
            "worktree=api",
            "claim=issue-6",
        ] {
            assert!(sel(s).matches(&u), "{s}");
        }
        for s in [
            "user=brett",
            "session=77e0",
            "host=heron",
            "repo=como-technologies/other",
            "worktree=docs",
            "claim=issue-7",
        ] {
            assert!(!sel(s).matches(&u), "{s}");
        }
    }

    #[test]
    fn lead_selects_the_lead_or_the_others() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf");
        let lead = u.clone().with_lead(true);
        assert!(sel("lead=true").matches(&lead));
        assert!(!sel("lead=true").matches(&u));
        assert!(sel("lead=false").matches(&u));
        assert!(!sel("lead=false").matches(&lead));
        let s = Selector::lead("mike", "como-technologies/riff");
        assert_eq!(
            s.to_string(),
            "user=mike,repo=como-technologies/riff,lead=true"
        );
        assert!(s.matches(&lead));
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"user":"mike","repo":"como-technologies/riff","lead":true}"#
        );
    }

    #[test]
    fn all_named_fields_must_match() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf");
        assert!(sel("user=mike,host=pangolin").matches(&u));
        assert!(!sel("user=mike,host=heron").matches(&u));
    }

    #[test]
    fn the_main_worktree_matches_no_worktree_selector() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf");
        assert!(!sel("worktree=api").matches(&u));
    }

    #[test]
    fn an_empty_selector_matches_nothing() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf");
        assert!(!Selector::default().matches(&u));
    }

    #[test]
    fn json_leaves_out_unset_fields() {
        let s = Selector::session("a6cf");
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"session":"a6cf"}"#);
        assert!(!s.is_other());
    }

    #[test]
    fn a_selector_of_a_later_build_matches_no_session() {
        let u = uri("riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#api")
            .with_lead(true);
        // The selector of a later build, and the same selector with
        // only the fields that this build knows.
        for (json, known) in [
            (r#"{"colour":"blue"}"#, "{}"),
            (r#"{"user":"mike","wave":"17"}"#, r#"{"user":"mike"}"#),
            (r#"{"session":"a6cf","wave":17}"#, r#"{"session":"a6cf"}"#),
            (r#"{"lead":true,"wave":{"n":17}}"#, r#"{"lead":true}"#),
            (
                r#"{"claim":"issue-6","wave":null}"#,
                r#"{"claim":"issue-6"}"#,
            ),
            (r#"{"user":null,"wave":"17"}"#, "{}"),
            // A value of a field that the build does not know.
            (r#"{"lead":"maybe","user":"mike"}"#, r#"{"user":"mike"}"#),
            (r#"{"user":["mike","brett"]}"#, "{}"),
            // Each other form of JSON.
            (r#""all""#, "{}"),
            (r#"["user","mike"]"#, "{}"),
            ("17", "{}"),
            ("true", "{}"),
            ("null", "{}"),
        ] {
            let later: Selector = serde_json::from_str(json).unwrap();
            assert!(later.is_other() && !later.is_empty(), "{json}");
            assert!(!later.matches(&u), "{json}");
            // The selector is written again as it came.
            assert_eq!(serde_json::to_string(&later).unwrap(), json);
            assert_eq!(later.to_string(), json);
            // Two selectors of a later build are the same only when
            // their JSON is the same.
            let same: Selector = serde_json::from_str(json).unwrap();
            assert_eq!(later, same);
            assert_ne!(later, serde_json::from_str(r#"{"wave":"18"}"#).unwrap());
            // The selector with only the known fields is wider.
            let known: Selector = serde_json::from_str(known).unwrap();
            assert!(!known.is_other(), "{json}");
            assert!(known.is_empty() || known.matches(&u), "{json}");
        }
        // The command line still takes only the fields of this build.
        assert!("user=mike,wave=17".parse::<Selector>().is_err());
    }

    /// A field that the build knows can be `null`: it is not set. An
    /// agent tool sends such a selector.
    #[test]
    fn a_null_of_a_field_that_the_build_knows_is_a_field_that_is_not_set() {
        let s: Selector = serde_json::from_str(r#"{"user":"mike","session":null}"#).unwrap();
        assert_eq!(s, sel("user=mike"));
        assert!(!s.is_other());
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"user":"mike"}"#);
    }

    #[test]
    fn the_schema_of_a_selector_has_only_the_fields_of_this_build() {
        let schema = serde_json::to_value(schemars::schema_for!(Selector)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        let mut fields: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        fields.sort();
        assert_eq!(
            fields,
            [
                "claim", "host", "lead", "repo", "session", "user", "worktree"
            ]
        );
    }
}
