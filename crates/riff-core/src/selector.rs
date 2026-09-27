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
use serde::{Deserialize, Serialize};

use crate::name::{NameError, SessionUri};

/// Picks sessions by who they are, where they work, and what they hold.
/// Each field that is set must match. Set one or more fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
}

impl Selector {
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
        self.lead.is_none() && self.fields().iter().all(|(_, v)| v.is_none())
    }

    /// True when each named field matches the session.
    pub fn matches(&self, uri: &SessionUri) -> bool {
        let place = uri.place();
        let who = uri.who();
        let eq = |want: &Option<String>, have: Option<&str>| {
            want.as_deref().is_none_or(|w| Some(w) == have)
        };
        !self.is_empty()
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        assert!(serde_json::from_str::<Selector>(r#"{"colour":"blue"}"#).is_err());
    }
}
