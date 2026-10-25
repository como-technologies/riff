//! The end of a session with no pull request (01M5A5HXKZTDP5M5HY2FHCEBZA),
//! or of a verify with no PASS or FAIL (01M5SMP2D4MGME90EHVKAQHQ26): the
//! record in the thread [`THREAD`] and `riff session why`.
//!
//! # Design
//!
//! The daemon, the socket and the clones are on the host, so the lead in
//! its container cannot find why a session of a wave ended with no work.
//! After the end of a build, the daemon looks for the open pull request
//! of its branch. When there is none, it posts one note in the thread
//! [`THREAD`], as the session ([`post`]), and the session leaves the
//! thread. A verify has a pull request, so the daemon looks for its
//! PASS or FAIL: when there is none, it posts the same note for the item
//! `verify-issue-N`. The lead reads it with `riff session why ITEM`
//! ([`read`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as session
//!     participant D as riff daemon
//!     participant R as riff-server
//!     participant L as lead
//!     S->>D: the end (code, or signal)
//!     D->>D: the pull request of the branch? none (a verify: PASS or FAIL? none)
//!     D->>R: a note in "ends": the reason, the last 50 lines
//!     L->>R: riff session why ITEM
//!     R->>L: the last record of ITEM
//! ```
//!
//! # The record
//!
//! The first line has the tags, then a colon and the reason. The last
//! lines of the output follow:
//!
//! ```text
//! ended item=issue-1202 session=1a2b3c4d code=1 failed=yes denied=2: exit code 1, 2 denied tool calls (Bash, Write)
//! {"type":"result","subtype":"error_during_execution"}
//! ```
//!
//! ```
//! use riff::ends::{End, Record};
//!
//! let end = End {
//!     item: "issue-1202".into(),
//!     session: "1a2b3c4d".into(),
//!     code: Some(1),
//!     failed: true,
//!     denied: vec!["Bash".into(), "Write".into(), "Bash".into()],
//!     tail: vec!["last line".into()],
//! };
//! assert_eq!(end.reason(), "exit code 1, 3 denied tool calls (Bash, Write)");
//! let record = Record::parse(&end.body(), 9).unwrap();
//! assert_eq!(record.tag("item"), "issue-1202");
//! assert_eq!(record.reason, end.reason());
//! assert_eq!(record.tail, ["last line"]);
//!
//! let verify = End { item: "verify-issue-1202".into(), code: Some(0), failed: false, denied: vec![], ..end };
//! assert_eq!(verify.reason(), "exit code 0, and no PASS or FAIL");
//! ```

use std::collections::BTreeMap;

use anyhow::Result;
use riff_client::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::Kind;
use serde::{Deserialize, Serialize};

use crate::daemon::{EVENTS, Ended};
use crate::room::Role;
use crate::run::Scrub;

/// The thread of the records.
pub const THREAD: &str = "ends";

/// The most lines of the output in a record.
pub const TAIL: usize = 50;

/// The most characters of a line of the output in a record.
pub const LINE_MAX: usize = 300;

/// The thread [`THREAD`].
pub fn thread() -> ThreadName {
    THREAD.parse().expect("ends is a thread name")
}

/// What a session of `item` lacks when its record is in the thread
/// [`THREAD`]: a verify has no PASS or FAIL, any other item has no pull
/// request.
///
/// ```
/// assert_eq!(riff::ends::lacks("issue-7"), "pull request");
/// assert_eq!(riff::ends::lacks("verify-issue-7"), "PASS or FAIL");
/// ```
pub fn lacks(item: &str) -> &'static str {
    match Role::of(item) {
        Role::Verify => "PASS or FAIL",
        Role::Lead | Role::Work => "pull request",
    }
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct End {
    /// The item.
    pub item: String,
    /// The start of the session ID.
    pub session: String,
    /// The exit code of `claude`, or none after a signal.
    pub code: Option<i32>,
    /// True when a turn ended with an error, or a stop ended it.
    pub failed: bool,
    /// The tool of each call that auto mode denied in the last turn.
    pub denied: Vec<String>,
    /// The last [`TAIL`] lines of its output.
    pub tail: Vec<String>,
}

impl End {
    /// The end of the session of `record`, as it `ended`. It reads the
    /// events of the session, and replaces each secret of `scrub`.
    pub fn of(record: &crate::daemon::Record, ended: Ended, scrub: &Scrub) -> End {
        let events = std::fs::read_to_string(record.folder.join(EVENTS)).unwrap_or_default();
        End::from_events(
            &record.live.item,
            &record.live.session,
            ended,
            &events,
            scrub,
        )
    }

    /// [`End::of`] for the text `events` of the events file.
    pub fn from_events(
        item: &str,
        session: &str,
        ended: Ended,
        events: &str,
        scrub: &Scrub,
    ) -> End {
        let lines: Vec<&str> = events.lines().collect();
        let denied = lines
            .iter()
            .rev()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|event| event["type"] == "result")
            .and_then(|event| {
                event["permission_denials"].as_array().map(|all| {
                    all.iter()
                        .map(|d| d["tool_name"].as_str().unwrap_or("?").to_owned())
                        .collect()
                })
            })
            .unwrap_or_default();
        let tail = lines[lines.len().saturating_sub(TAIL)..]
            .iter()
            .map(|line| {
                let line = scrub.clean(line);
                match line.char_indices().nth(LINE_MAX) {
                    Some((at, _)) => format!("{}...", &line[..at]),
                    None => line.into_owned(),
                }
            })
            .collect();
        End {
            item: item.to_owned(),
            session: crate::run::short(session).to_owned(),
            code: ended.code,
            failed: ended.failed,
            denied,
            tail,
        }
    }

    /// The reason in words: the exit code, or the signal, and the denied
    /// tool calls.
    pub fn reason(&self) -> String {
        let mut reason = match (self.code, self.failed) {
            (Some(0), false) => format!("exit code 0, and no {}", lacks(&self.item)),
            (Some(0), true) => {
                "exit code 0, but a turn ended with an error or a stop ended it".to_owned()
            }
            (Some(code), _) => format!("exit code {code}"),
            (None, _) => {
                "no exit code: a signal, or the container ended before its relay".to_owned()
            }
        };
        if !self.denied.is_empty() {
            let mut tools: Vec<&str> = self.denied.iter().map(String::as_str).collect();
            tools.sort_unstable();
            tools.dedup();
            reason += &format!(
                ", {} denied tool calls ({})",
                self.denied.len(),
                tools.join(", ")
            );
        }
        reason
    }

    /// The body of the post.
    pub fn body(&self) -> String {
        let code = self.code.map_or("-".to_owned(), |c| c.to_string());
        let mut body = format!(
            "ended item={} session={} code={code} failed={} denied={}: {}",
            self.item,
            self.session,
            if self.failed { "yes" } else { "no" },
            self.denied.len(),
            self.reason()
        );
        for line in &self.tail {
            body.push('\n');
            body.push_str(line);
        }
        body
    }
}

/// Posts `end` as a note in the thread [`THREAD`], as the session `me`,
/// and leaves the thread.
pub async fn post(api: &Api, me: &SessionUri, end: &End) -> Result<()> {
    let thread = thread();
    api.post(me, Some(&thread), &[], &end.body(), Kind::Note)
        .await?;
    let _ = api.leave(me, &thread).await;
    Ok(())
}

/// One record of the thread [`THREAD`]. The daemon gives the records to a
/// session in a container as JSON (#1338).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The time of the post, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// Each tag, by name.
    pub tags: BTreeMap<String, String>,
    /// The reason in words.
    pub reason: String,
    /// The last lines of the output.
    pub tail: Vec<String>,
}

impl Record {
    /// The record of the body `body` of a post at `at_ms`. `None` when
    /// the body is no record.
    pub fn parse(body: &str, at_ms: u64) -> Option<Record> {
        let mut lines = body.lines();
        let (tags, reason) = lines.next()?.strip_prefix("ended ")?.split_once(": ")?;
        let tags = tags
            .split(' ')
            .map(|tag| tag.split_once('='))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Some(Record {
            at_ms,
            tags,
            reason: reason.to_owned(),
            tail: lines.map(str::to_owned).collect(),
        })
    }

    /// The value of the tag `name`, or `-`.
    pub fn tag(&self, name: &str) -> &str {
        self.tags.get(name).map_or("-", String::as_str)
    }
}

/// The records of the thread [`THREAD`], oldest first, as `me` reads
/// them. A thread that the server does not know has none.
pub async fn read(api: &Api, me: &SessionUri) -> Result<Vec<Record>> {
    let messages = match api.read(me, &thread(), true).await {
        Ok(messages) => messages,
        Err(e)
            if e.downcast_ref::<riff_client::api::Refusal>()
                .is_some_and(|r| r.status == reqwest::StatusCode::NOT_FOUND) =>
        {
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    Ok(messages
        .iter()
        .filter_map(|m| Record::parse(&m.message.body, m.message.at_ms))
        .collect())
}

/// The text of `riff session why` for `item` and the `records` of the
/// thread: the last record of the item, and how many it has.
pub fn why(item: &str, records: &[Record]) -> String {
    let mine: Vec<&Record> = records.iter().filter(|r| r.tag("item") == item).collect();
    let Some(last) = mine.last() else {
        return crate::text::session_why_none(item);
    };
    crate::text::session_why(item, mine.len(), last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ended(code: Option<i32>, failed: bool) -> Ended {
        Ended { code, failed }
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA.
    #[test]
    fn the_record_has_the_last_50_lines_with_no_secret_and_the_denials() {
        let mut events: String = (0..80).map(|n| format!("line {n}\n")).collect();
        events.push_str(&format!("secret-token-1 {}\n", "x".repeat(400)));
        events.push_str(
            r#"{"type":"result","permission_denials":[{"tool_name":"Bash"},{"tool_name":"Write"}]}"#,
        );
        let scrub = Scrub::new(["secret-token-1".to_owned()]);
        let end = End::from_events(
            "issue-9",
            "0123456789",
            ended(Some(1), true),
            &events,
            &scrub,
        );
        assert_eq!(end.session, "01234567");
        assert_eq!(end.tail.len(), TAIL);
        assert!(end.tail[0].starts_with("line 32"), "{:?}", end.tail[0]);
        let long = &end.tail[TAIL - 2];
        assert!(
            long.starts_with("[secret]") && long.ends_with("..."),
            "{long}"
        );
        assert_eq!(long.chars().count(), LINE_MAX + 3);
        assert_eq!(end.denied, ["Bash", "Write"]);
        assert_eq!(
            end.reason(),
            "exit code 1, 2 denied tool calls (Bash, Write)"
        );
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA: a signal and a clean exit have words.
    #[test]
    fn the_reason_names_a_signal_and_a_clean_exit() {
        let of =
            |code, failed| End::from_events("i", "s", ended(code, failed), "", &Scrub::new([]));
        assert!(of(None, true).reason().starts_with("no exit code"));
        assert_eq!(
            of(Some(0), false).reason(),
            "exit code 0, and no pull request"
        );
        assert!(
            of(Some(0), true)
                .reason()
                .contains("a turn ended with an error")
        );
        assert!(of(Some(137), true).reason().starts_with("exit code 137"));
    }

    #[test]
    fn a_body_that_is_no_record_is_none() {
        assert!(Record::parse("hello", 1).is_none());
        assert!(Record::parse("ended no tags", 1).is_none());
        assert_eq!(
            why("issue-1", &[]),
            crate::text::session_why_none("issue-1")
        );
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA, 01M5A5HXP9X3AXTS6N3A57DMQM: a session
    /// posts its end, and leaves the thread. Another session reads the
    /// last record of the item, and the count.
    #[tokio::test]
    async fn the_end_of_a_session_reaches_the_lead_that_reads_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let api = Api::new(&format!("http://{addr}"));
        let reader: SessionUri = "riff://ann@heron/acme/app?session=r1".parse().unwrap();
        let worker: SessionUri = "riff://ann@heron/acme/app?session=w1".parse().unwrap();
        api.register(&reader).await.unwrap();
        api.register(&worker).await.unwrap();
        assert!(read(&api, &reader).await.unwrap().is_empty());
        for (n, code) in [(1, 1), (2, 2)] {
            let events = format!("out {n}\n");
            let end = End::from_events(
                "issue-5",
                "w1",
                ended(Some(code), true),
                &events,
                &Scrub::new([]),
            );
            post(&api, &worker, &end).await.unwrap();
        }
        let other = End::from_events("issue-6", "w1", ended(None, true), "", &Scrub::new([]));
        post(&api, &worker, &other).await.unwrap();
        let records = read(&api, &reader).await.unwrap();
        assert_eq!(records.len(), 3);
        let text = why("issue-5", &records);
        assert!(
            text.starts_with("issue-5: 2 sessions ended with no pull request."),
            "{text}"
        );
        assert!(text.contains("ended: exit code 2."), "{text}");
        assert!(text.ends_with("out 2"), "{text}");
        let threads = api.threads(&worker).await.unwrap();
        assert!(
            threads
                .iter()
                .filter(|t| t.thread.to_string() == THREAD)
                .all(|t| t.members.iter().all(|m| m.who() != worker.who())),
            "the worker left the thread"
        );
    }
}
