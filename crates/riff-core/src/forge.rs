//! The forge token of a role: what `riff` and `riff-server` agree on.
//!
//! riff-server makes the forge token of each session with the GitHub
//! App of riff (#628). The server picks the role of the token from its
//! own facts ([`role_of`]). The token has the [`permissions`] of that
//! role on the repository of the session only, and no more
//! ([`check_given`]).
//!
//! | Role | GitHub permissions |
//! |---|---|
//! | lead | metadata, actions, checks, statuses: read; contents, issues, pull requests: write |
//! | worker | the same as the lead |
//! | verifier | metadata, actions, checks, contents: read; issues, pull requests, statuses: write |
//!
//! A test run gets no token: it has no [`TokenRole`].
//!
//! ```
//! use riff_core::forge::{Access, TokenRole, permissions};
//!
//! let worker = permissions(TokenRole::Worker);
//! assert_eq!(worker["contents"], Access::Write);
//! assert!(!worker.contains_key("administration"));
//! assert!(!worker.contains_key("workflows"));
//! assert_eq!(permissions(TokenRole::Verifier)["statuses"], Access::Write);
//! assert_eq!(permissions(TokenRole::Verifier)["contents"], Access::Read);
//! assert_eq!(permissions(TokenRole::Lead), worker);
//! ```

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A level of a GitHub permission.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Read only.
    Read,
    /// Read and write.
    Write,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Access::Read => "read",
            Access::Write => "write",
        })
    }
}

/// The role of a forge token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TokenRole {
    /// The lead of a person in a repository.
    Lead,
    /// A worker.
    Worker,
    /// A session with a `verify-` claim.
    Verifier,
}

impl TokenRole {
    /// Each role, in the order of the table of the module.
    pub const ALL: [TokenRole; 3] = [TokenRole::Lead, TokenRole::Worker, TokenRole::Verifier];
}

impl fmt::Display for TokenRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TokenRole::Lead => "lead",
            TokenRole::Worker => "worker",
            TokenRole::Verifier => "verifier",
        })
    }
}

/// The GitHub permissions of `role`. No role gets `administration`,
/// `deployments`, `environments`, `secrets` or `workflows`. Only the
/// verifier sets a commit status, and the verifier writes no code.
pub fn permissions(role: TokenRole) -> BTreeMap<&'static str, Access> {
    use Access::{Read, Write};
    let mut all = BTreeMap::from([
        ("metadata", Read),
        ("actions", Read),
        ("checks", Read),
        ("issues", Write),
        ("pull_requests", Write),
    ]);
    let (contents, statuses) = match role {
        TokenRole::Lead | TokenRole::Worker => (Write, Read),
        TokenRole::Verifier => (Read, Write),
    };
    all.insert("contents", contents);
    all.insert("statuses", statuses);
    all
}

/// The permissions of the GitHub App itself: each permission of a role,
/// at the highest level that a role needs, and no more. `riff forge
/// create` puts them in the manifest of the App (#627). Each token then
/// asks for the [`permissions`] of its role only.
///
/// ```
/// use riff_core::forge::{Access, TokenRole, app_permissions, permissions};
///
/// let app = app_permissions();
/// assert_eq!(app["contents"], Access::Write);
/// assert_eq!(app["statuses"], Access::Write);
/// assert_eq!(app["actions"], Access::Read);
/// assert!(!app.contains_key("administration"));
/// for role in TokenRole::ALL {
///     assert!(permissions(role).iter().all(|(name, level)| app[name] >= *level));
/// }
/// assert_eq!(app.len(), permissions(TokenRole::Worker).len());
/// ```
pub fn app_permissions() -> BTreeMap<&'static str, Access> {
    let mut all = BTreeMap::new();
    for role in TokenRole::ALL {
        for (name, level) in permissions(role) {
            let at = all.entry(name).or_insert(level);
            *at = (*at).max(level);
        }
    }
    all
}

/// The role of the token of a session: the lead, else a session with a
/// `verify-` claim is the verifier, else the worker. The server gives
/// `lead` and `claims` from its own facts, so a session cannot ask for
/// more rights.
///
/// ```
/// use riff_core::forge::{TokenRole, role_of};
///
/// assert_eq!(role_of(true, &[]), TokenRole::Lead);
/// assert_eq!(role_of(false, &["verify-issue-12".into()]), TokenRole::Verifier);
/// assert_eq!(role_of(false, &["issue-12".into()]), TokenRole::Worker);
/// assert_eq!(role_of(false, &[]), TokenRole::Worker);
/// ```
pub fn role_of(lead: bool, claims: &[String]) -> TokenRole {
    if lead {
        TokenRole::Lead
    } else if claims.iter().any(|c| c.starts_with("verify-")) {
        TokenRole::Verifier
    } else {
        TokenRole::Worker
    }
}

/// Checks the permissions that GitHub gave against the permissions of
/// `role`: each one that the role needs, and none more. The error is the
/// text for a person.
///
/// ```
/// use riff_core::forge::{TokenRole, check_given, permissions};
///
/// let given = |r| permissions(r).into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
/// assert!(check_given(TokenRole::Worker, &given(TokenRole::Worker)).is_ok());
/// // The worker writes the code: more than the verifier asked for.
/// assert!(check_given(TokenRole::Verifier, &given(TokenRole::Worker)).is_err());
/// // The App lacks a permission of the role.
/// assert!(check_given(TokenRole::Worker, &given(TokenRole::Verifier)).is_err());
/// ```
pub fn check_given(role: TokenRole, given: &BTreeMap<String, Access>) -> Result<(), String> {
    let asked = permissions(role);
    let more: Vec<String> = given
        .iter()
        .filter(|(name, level)| asked.get(name.as_str()).is_none_or(|a| *level > a))
        .map(|(name, level)| format!("{name}: {level}"))
        .collect();
    if !more.is_empty() {
        return Err(format!(
            "GitHub gave the {role} token rights that it did not ask for: {}",
            more.join(", ")
        ));
    }
    let less: Vec<String> = asked
        .iter()
        .filter(|(name, level)| given.get(**name).is_none_or(|g| g < level))
        .map(|(name, level)| format!("{name}: {level}"))
        .collect();
    if !less.is_empty() {
        return Err(format!(
            "the GitHub App does not have these permissions of the {role}: {}. Add them in the \
             settings of the App",
            less.join(", ")
        ));
    }
    Ok(())
}

/// The permissions of a token, as one line: `contents: write, ...`.
///
/// ```
/// use riff_core::forge::{TokenRole, line, permissions};
///
/// let p = permissions(TokenRole::Verifier).into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
/// assert!(line(&p).contains("statuses: write"));
/// ```
pub fn line(permissions: &BTreeMap<String, Access>) -> String {
    permissions
        .iter()
        .map(|(name, level)| format!("{name}: {level}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_role_gets_the_rights_of_an_admin() {
        for role in TokenRole::ALL {
            let p = permissions(role);
            for name in [
                "administration",
                "deployments",
                "environments",
                "workflows",
                "secrets",
            ] {
                assert!(!p.contains_key(name), "{role} has {name}");
            }
        }
    }

    #[test]
    fn no_role_both_sets_a_status_and_writes_the_code() {
        for role in TokenRole::ALL {
            let p = permissions(role);
            assert!(
                !(p["statuses"] == Access::Write && p["contents"] == Access::Write),
                "{role}"
            );
        }
    }

    #[test]
    fn the_role_serializes_as_its_name() {
        let text = serde_json::to_string(&TokenRole::Verifier).unwrap();
        assert_eq!(text, r#""verifier""#);
        assert_eq!(TokenRole::Verifier.to_string(), "verifier");
    }
}
