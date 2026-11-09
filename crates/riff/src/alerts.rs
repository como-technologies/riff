//! The alert gate of `riff release cut` (#1841, 01M6ZM68Y1QCTA2M8DHYRMKBPN):
//! riff cuts no release while an alert of the repository has no review.
//!
//! # Design
//!
//! Before it makes the release issue and the release pull request,
//! [`crate::cycle::cut`] reads the alerts of the repository with
//! [`crate::cycle::Forge::alerts`]: the code scanning alerts, of security
//! and of quality, the secret scanning alerts and the Dependabot alerts
//! ([`read`]). An alert has a review when it is fixed, or dismissed with
//! a comment that gives the reason ([`Alert::reviewed`]). The cut refuses
//! while an alert is open, or dismissed with no comment, and lists each
//! one ([`refusal`]). It also refuses when riff cannot read the alerts
//! of a kind ([`unreadable`]): a gate that cannot look lets nothing
//! through. No flag skips the gate.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff release cut X.Y.Z"] --> R["read the alerts:<br/>code scanning, secret scanning, Dependabot"]
//!     R -- "a read fails" --> U["refuse: the error and the fix"]
//!     R --> Q{"an open alert, or a dismissed<br/>alert with no comment?"}
//!     Q -- yes --> F["refuse: each alert with its kind,<br/>rule, file and line, and link"]
//!     Q -- no --> P["the release issue<br/>and the release pull request"]
//! ```
//!
//! | Kind | The read | Open | Dismissed |
//! |---|---|---|---|
//! | code scanning, security and quality | `code-scanning/alerts?state=open`, and `?state=dismissed` | `open` | `dismissed`, with `dismissed_comment` |
//! | secret scanning | `secret-scanning/alerts`, with no secret | `open` | `resolved`, with `resolution_comment` |
//! | Dependabot | `dependabot/alerts?state=open,dismissed,auto_dismissed` | `open` | `dismissed` and `auto_dismissed`, with `dismissed_comment` |
//!
//! A code scanning alert is of security when its rule has a security
//! severity or the tag `security`. Else it is of quality.
//!
//! Only the lead App and a person can read the alerts
//! (01M6ZEJ4NEZGQFV3Q4BER5AW3S). The session App of a worker has no
//! right on them, so the cut of a worker refuses.
//!
//! ```
//! use riff::alerts::{Alert, Kind, State, unreviewed};
//!
//! let alert = |state| Alert {
//!     kind: Kind::Dependabot,
//!     number: 2,
//!     rule: "GHSA-1234 in tokio".into(),
//!     path: Some("Cargo.lock".into()),
//!     line: None,
//!     url: "https://github.com/o/r/security/dependabot/2".into(),
//!     state,
//! };
//! let reason = State::Dismissed { comment: "Only the tests use it.".into() };
//! let none = State::Dismissed { comment: " ".into() };
//! let alerts = [alert(State::Open), alert(reason), alert(none)];
//! let left = unreviewed(&alerts);
//! assert_eq!(left, [&alerts[0], &alerts[2]]);
//! assert!(alerts[1].reviewed());
//! ```

use anyhow::{Result, anyhow};
use riff_core::build::Semver;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::pr::Gh;
use crate::text;

/// The kind of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A code scanning alert of security, for example of CodeQL.
    Security,
    /// A code scanning alert of quality.
    Quality,
    /// A secret scanning alert.
    Secret,
    /// A Dependabot alert.
    Dependabot,
}

impl Kind {
    /// The words of the kind in the refusal.
    ///
    /// ```
    /// assert_eq!(riff::alerts::Kind::Quality.word(), "code scanning (quality)");
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            Kind::Security => "code scanning (security)",
            Kind::Quality => "code scanning (quality)",
            Kind::Secret => "secret scanning",
            Kind::Dependabot => "Dependabot",
        }
    }
}

/// The state of an alert for the gate. A fixed alert is in no list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The alert is open.
    Open,
    /// A person or a rule dismissed the alert. `comment` is the comment
    /// of the dismiss: empty when it has none.
    Dismissed {
        /// The comment of the dismiss.
        comment: String,
    },
}

/// An alert of the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Its kind.
    pub kind: Kind,
    /// Its number on GitHub, in its kind.
    pub number: u64,
    /// Its rule: the rule of the scan, the type of the secret, or the
    /// advisory and its package.
    pub rule: String,
    /// Its file, when it has one.
    pub path: Option<String>,
    /// Its line in the file, when it has one.
    pub line: Option<u64>,
    /// Its page on GitHub.
    pub url: String,
    /// Open, or dismissed with its comment.
    pub state: State,
}

impl Alert {
    /// True when the alert has a review: dismissed with a comment that
    /// gives the reason. An open alert has none.
    pub fn reviewed(&self) -> bool {
        matches!(&self.state, State::Dismissed { comment } if !comment.trim().is_empty())
    }

    /// The line of the alert in the refusal: the kind, the number, the
    /// rule, the file and line, the state, and the link.
    ///
    /// ```
    /// use riff::alerts::{Alert, Kind, State};
    ///
    /// let alert = Alert {
    ///     kind: Kind::Security,
    ///     number: 4,
    ///     rule: "rust/cleartext-logging".into(),
    ///     path: Some("crates/riff/src/login.rs".into()),
    ///     line: Some(88),
    ///     url: "https://github.com/o/r/security/code-scanning/4".into(),
    ///     state: State::Open,
    /// };
    /// assert_eq!(
    ///     alert.line(),
    ///     "- code scanning (security) #4 rust/cleartext-logging, crates/riff/src/login.rs:88, \
    ///      open: https://github.com/o/r/security/code-scanning/4"
    /// );
    /// let lock = Alert { path: Some("Cargo.lock".into()), line: None, ..alert.clone() };
    /// assert!(lock.line().contains(", Cargo.lock, open: "));
    /// let gone = Alert { path: None, state: State::Dismissed { comment: String::new() }, ..alert };
    /// assert!(gone.line().contains(", no file, dismissed with no reason: "));
    /// ```
    pub fn line(&self) -> String {
        let place = match (&self.path, self.line) {
            (Some(path), Some(line)) => format!("{path}:{line}"),
            (Some(path), None) => path.clone(),
            (None, _) => "no file".to_owned(),
        };
        let state = match self.state {
            State::Open => "open",
            State::Dismissed { .. } => "dismissed with no reason",
        };
        format!(
            "- {} #{} {}, {place}, {state}: {}",
            self.kind.word(),
            self.number,
            self.rule,
            self.url
        )
    }
}

/// The alerts of `alerts` with no review, in their order: each open
/// alert, and each dismissed alert with no comment.
pub fn unreviewed(alerts: &[Alert]) -> Vec<&Alert> {
    alerts.iter().filter(|a| !a.reviewed()).collect()
}

/// The refusal of the cut of `new` for the alerts `left` with no review.
///
/// ```
/// use riff::alerts::{Alert, Kind, State, refusal};
///
/// let alert = Alert {
///     kind: Kind::Secret,
///     number: 2,
///     rule: "GitHub Personal Access Token".into(),
///     path: Some("docs/x.md".into()),
///     line: Some(1),
///     url: "https://github.com/o/r/security/secret-scanning/2".into(),
///     state: State::Open,
/// };
/// let text = refusal(riff::cycle::version("3.3.0")?, &[&alert]);
/// assert!(text.starts_with("riff release cut: 1 alert of the repository has no review."));
/// assert!(text.contains("\n- secret scanning #2 GitHub Personal Access Token, docs/x.md:1, open: "));
/// assert!(text.ends_with("the reason in the comment, then riff release cut 3.3.0"));
/// let two = refusal(riff::cycle::version("3.3.0")?, &[&alert, &alert]);
/// assert!(two.starts_with("riff release cut: 2 alerts of the repository have no review."));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn refusal(new: Semver, left: &[&Alert]) -> String {
    let count = match left.len() {
        1 => "1 alert of the repository has".to_owned(),
        n => format!("{n} alerts of the repository have"),
    };
    let lines: Vec<String> = left.iter().map(|a| a.line()).collect();
    format!(
        "riff release cut: {count} no review. An alert has a review when it is fixed, or \
         dismissed with a comment that gives the reason:\n{}\nnext: fix each alert in the code, \
         or dismiss it on GitHub with the reason in the comment, then riff release cut {new}",
        lines.join("\n")
    )
}

/// The refusal of the cut of `new` when riff cannot read the alerts:
/// `why` is the failed read.
///
/// ```
/// let why = "the read of the Dependabot alerts failed: gh: Resource not accessible by integration (HTTP 403)";
/// let text = riff::alerts::unreadable(riff::cycle::version("3.3.0")?, why);
/// assert!(text.starts_with(&format!("riff release cut: {why}. riff cuts no release")));
/// assert!(text.contains("the lead or a person cuts the release"));
/// assert!(text.contains("riff github check"));
/// assert!(text.ends_with("next: fix it, then riff release cut 3.3.0"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn unreadable(new: Semver, why: &str) -> String {
    format!(
        "riff release cut: {why}. riff cuts no release while it cannot check the alerts of the \
         repository. A worker has no right on the alerts: the lead or a person cuts the release. \
         For the lead, riff github check names each right that the lead App misses. When a scan \
         is off, turn it on in the settings of the repository. next: fix it, then riff release \
         cut {new}"
    )
}

/// The place of an alert in a file.
#[derive(Deserialize)]
struct Location {
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    path: Option<String>,
    #[serde(default)]
    start_line: Option<u64>,
}

#[derive(Deserialize)]
struct CodeRule {
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    id: Option<String>,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    name: Option<String>,
    #[serde(default)]
    security_severity_level: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CodeInstance {
    #[serde(default)]
    location: Option<Location>,
}

/// A code scanning alert of the REST API.
#[derive(Deserialize)]
struct CodeAlert {
    number: u64,
    #[serde(default)]
    state: String,
    rule: CodeRule,
    #[serde(default)]
    most_recent_instance: Option<CodeInstance>,
    #[serde(default, deserialize_with = "text::forge_de")]
    html_url: String,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    dismissed_comment: Option<String>,
}

/// A secret scanning alert of the REST API. riff reads no secret.
#[derive(Deserialize)]
struct SecretAlert {
    number: u64,
    #[serde(default)]
    state: String,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    secret_type_display_name: Option<String>,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    secret_type: Option<String>,
    #[serde(default, deserialize_with = "text::forge_de")]
    html_url: String,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    resolution_comment: Option<String>,
    #[serde(default)]
    first_location_detected: Option<Location>,
}

#[derive(Deserialize)]
struct Package {
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    name: Option<String>,
}

#[derive(Deserialize)]
struct Dependency {
    #[serde(default)]
    package: Option<Package>,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    manifest_path: Option<String>,
}

#[derive(Deserialize)]
struct Advisory {
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    ghsa_id: Option<String>,
}

/// A Dependabot alert of the REST API.
#[derive(Deserialize)]
struct DependabotAlert {
    number: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    dependency: Option<Dependency>,
    #[serde(default)]
    security_advisory: Option<Advisory>,
    #[serde(default, deserialize_with = "text::forge_de")]
    html_url: String,
    #[serde(default, deserialize_with = "text::forge_de_opt")]
    dismissed_comment: Option<String>,
}

/// The state of an alert of the API in `open`, or in one of the states
/// `dismissed`. `None` for another state, for example `fixed`.
fn state_of(state: &str, dismissed: &[&str], comment: Option<String>) -> Option<State> {
    if state == "open" {
        Some(State::Open)
    } else if dismissed.contains(&state) {
        Some(State::Dismissed {
            comment: comment.unwrap_or_default(),
        })
    } else {
        None
    }
}

/// The items of `out`, the output of `gh api --paginate PATH --jq .[]`:
/// one JSON value after the other.
fn items<T: DeserializeOwned>(out: &str, what: &str) -> Result<Vec<T>> {
    serde_json::Deserializer::from_str(out)
        .into_iter::<T>()
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow!("riff cannot read the {what} alerts that gh gave: {e}"))
}

/// The code scanning alerts in `out`, the output of `gh api --paginate
/// PATH --jq .[]`.
///
/// ```
/// use riff::alerts::{Kind, State, code_scanning};
///
/// let out = concat!(
///     r#"{"number":4,"state":"open","rule":{"id":"rust/cleartext-logging","security_severity_level":"high"},"#,
///     r#""most_recent_instance":{"location":{"path":"src/a.rs","start_line":12}},"html_url":"https://github.com/o/r/security/code-scanning/4"}"#,
///     "\n",
///     r#"{"number":5,"state":"dismissed","rule":{"id":"rust/unused","tags":["maintainability"]},"#,
///     r#""html_url":"https://github.com/o/r/security/code-scanning/5","dismissed_comment":"A test."}"#,
///     "\n",
/// );
/// let alerts = code_scanning(out)?;
/// assert_eq!((alerts[0].kind, alerts[0].line), (Kind::Security, Some(12)));
/// assert_eq!(alerts[1].kind, Kind::Quality);
/// assert_eq!(alerts[1].state, State::Dismissed { comment: "A test.".into() });
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn code_scanning(out: &str) -> Result<Vec<Alert>> {
    let list: Vec<CodeAlert> = items(out, "code scanning")?;
    Ok(list
        .into_iter()
        .filter_map(|a| {
            let state = state_of(&a.state, &["dismissed"], a.dismissed_comment)?;
            let security = a.rule.security_severity_level.is_some()
                || a.rule.tags.iter().flatten().any(|t| t == "security");
            let location = a.most_recent_instance.and_then(|i| i.location);
            Some(Alert {
                kind: if security {
                    Kind::Security
                } else {
                    Kind::Quality
                },
                number: a.number,
                rule: a.rule.id.or(a.rule.name).unwrap_or_default(),
                path: location.as_ref().and_then(|l| l.path.clone()),
                line: location.and_then(|l| l.start_line),
                url: a.html_url,
                state,
            })
        })
        .collect())
}

/// The secret scanning alerts in `out`, the output of `gh api --paginate
/// PATH --jq .[]`. A resolved alert counts as dismissed.
///
/// ```
/// use riff::alerts::{Kind, State, secret_scanning};
///
/// let out = r#"{"number":2,"state":"resolved","resolution":"false_positive","secret_type":"github_pat","secret_type_display_name":"GitHub Personal Access Token","html_url":"https://github.com/o/r/security/secret-scanning/2","first_location_detected":{"path":"docs/x.md","start_line":1}}"#;
/// let alerts = secret_scanning(out)?;
/// assert_eq!(alerts[0].kind, Kind::Secret);
/// assert_eq!(alerts[0].rule, "GitHub Personal Access Token");
/// assert_eq!((alerts[0].path.as_deref(), alerts[0].line), (Some("docs/x.md"), Some(1)));
/// assert_eq!(alerts[0].state, State::Dismissed { comment: String::new() });
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn secret_scanning(out: &str) -> Result<Vec<Alert>> {
    let list: Vec<SecretAlert> = items(out, "secret scanning")?;
    Ok(list
        .into_iter()
        .filter_map(|a| {
            let state = state_of(&a.state, &["resolved"], a.resolution_comment)?;
            let location = a.first_location_detected;
            Some(Alert {
                kind: Kind::Secret,
                number: a.number,
                rule: a
                    .secret_type_display_name
                    .or(a.secret_type)
                    .unwrap_or_default(),
                path: location.as_ref().and_then(|l| l.path.clone()),
                line: location.and_then(|l| l.start_line),
                url: a.html_url,
                state,
            })
        })
        .collect())
}

/// The Dependabot alerts in `out`, the output of `gh api --paginate PATH
/// --jq .[]`. An alert that a rule dismissed counts as dismissed.
///
/// ```
/// use riff::alerts::{Kind, State, dependabot};
///
/// let out = r#"{"number":7,"state":"auto_dismissed","dependency":{"package":{"ecosystem":"rust","name":"tokio"},"manifest_path":"Cargo.lock"},"security_advisory":{"ghsa_id":"GHSA-1234"},"html_url":"https://github.com/o/r/security/dependabot/7"}"#;
/// let alerts = dependabot(out)?;
/// assert_eq!(alerts[0].kind, Kind::Dependabot);
/// assert_eq!(alerts[0].rule, "GHSA-1234 in tokio");
/// assert_eq!((alerts[0].path.as_deref(), alerts[0].line), (Some("Cargo.lock"), None));
/// assert_eq!(alerts[0].state, State::Dismissed { comment: String::new() });
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn dependabot(out: &str) -> Result<Vec<Alert>> {
    let list: Vec<DependabotAlert> = items(out, "Dependabot")?;
    Ok(list
        .into_iter()
        .filter_map(|a| {
            let state = state_of(
                &a.state,
                &["dismissed", "auto_dismissed"],
                a.dismissed_comment,
            )?;
            let advisory = a
                .security_advisory
                .and_then(|s| s.ghsa_id)
                .unwrap_or_default();
            let (package, path) = match a.dependency {
                Some(d) => (d.package.and_then(|p| p.name), d.manifest_path),
                None => (None, None),
            };
            let rule = match package {
                Some(package) => format!("{advisory} in {package}"),
                None => advisory,
            };
            Some(Alert {
                kind: Kind::Dependabot,
                number: a.number,
                rule,
                path,
                line: None,
                url: a.html_url,
                state,
            })
        })
        .collect())
}

/// The reads of [`read`]: what each one reads, its path, and the parse of
/// its output.
type Parse = fn(&str) -> Result<Vec<Alert>>;
const READS: [(&str, &str, Parse); 4] = [
    (
        "code scanning",
        "repos/{owner}/{repo}/code-scanning/alerts?state=open&per_page=100",
        code_scanning,
    ),
    (
        "code scanning",
        "repos/{owner}/{repo}/code-scanning/alerts?state=dismissed&per_page=100",
        code_scanning,
    ),
    (
        "secret scanning",
        "repos/{owner}/{repo}/secret-scanning/alerts?per_page=100&hide_secret=true",
        secret_scanning,
    ),
    (
        "Dependabot",
        "repos/{owner}/{repo}/dependabot/alerts?state=open,dismissed,auto_dismissed&per_page=100",
        dependabot,
    ),
];

/// The open and the dismissed alerts of the repository of `gh`, of each
/// kind, with `gh api --paginate`. A failed read fails with what it read,
/// for example "the read of the Dependabot alerts failed: …". A limit of
/// GitHub fails with the error of the limit.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let script = "#!/bin/sh\ncase \"$*\" in\n\
///     *dependabot*) echo 'gh: Dependabot alerts are disabled for this repository. (HTTP 403)' >&2; exit 1 ;;\n\
///     esac\nexit 0\n";
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let error = riff::alerts::read(&gh).unwrap_err().to_string();
/// assert_eq!(
///     error,
///     "the read of the Dependabot alerts failed: gh: Dependabot alerts are disabled for this repository. (HTTP 403)"
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(gh: &Gh) -> Result<Vec<Alert>> {
    let mut alerts = Vec::new();
    for (what, path, parse) in READS {
        let out = gh.run_as(
            &format!("the read of the {what} alerts"),
            &["api", "--paginate", path, "--jq", ".[]"],
        )?;
        alerts.extend(parse(&out)?);
    }
    Ok(alerts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bad part of a text of the forge: it sets the title of the
    /// terminal, and breaks the line.
    const BAD: &str = r"\u001b]0;owned\u0007\r\n";

    /// Each text of the forge in the line of an alert goes through
    /// [`text::forge`] (01M3ZRQY6YQ8QAKZPGWH1XD6WW).
    #[test]
    fn the_line_of_an_alert_has_no_control_character() {
        let code = format!(
            r#"{{"number":1,"state":"open","rule":{{"id":"r{BAD}"}},"most_recent_instance":{{"location":{{"path":"a{BAD}.rs","start_line":2}}}},"html_url":"https://e.test/1{BAD}"}}"#
        );
        let secret = format!(
            r#"{{"number":2,"state":"open","secret_type_display_name":"s{BAD}","html_url":"https://e.test/2{BAD}","first_location_detected":{{"path":"b{BAD}.md"}}}}"#
        );
        let advisory = format!(
            r#"{{"number":3,"state":"open","dependency":{{"package":{{"name":"p{BAD}"}},"manifest_path":"Cargo{BAD}.lock"}},"security_advisory":{{"ghsa_id":"GHSA{BAD}"}},"html_url":"https://e.test/3{BAD}"}}"#
        );
        let alerts = [
            code_scanning(&code).unwrap(),
            secret_scanning(&secret).unwrap(),
            dependabot(&advisory).unwrap(),
        ]
        .concat();
        assert_eq!(alerts.len(), 3);
        for alert in &alerts {
            let line = alert.line();
            assert!(!line.chars().any(char::is_control), "{line:?}");
            assert!(!line.contains("]0;"), "{line:?}");
        }
    }

    /// A fixed alert is in no list, and a reply that is no JSON of the
    /// alerts fails with the kind.
    #[test]
    fn a_fixed_alert_is_left_out_and_a_bad_reply_names_the_kind() {
        let fixed = r#"{"number":1,"state":"fixed","rule":{"id":"r"},"html_url":"u"}"#;
        assert!(code_scanning(fixed).unwrap().is_empty());
        assert!(code_scanning("").unwrap().is_empty());
        let error = dependabot("not json").unwrap_err().to_string();
        assert!(
            error.starts_with("riff cannot read the Dependabot alerts that gh gave"),
            "{error}"
        );
    }
}
