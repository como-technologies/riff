//! The names of the chat: `@person` and `@project` (#1771).
//!
//! # Design
//!
//! riff-server keeps a name for each person and each project of the
//! riff. A person has the name of its user: `brett`. A project has the
//! name of its repository after the slash: `riff`, `sens-o-matic`. Each
//! name is unique: when two projects, or a person and a project, have
//! the same name, each of those projects has `OWNER/REPO` as its name.
//! `OWNER/REPO` also names a project with no clash. A name matches with
//! no regard to case (01M6XZ27XV8RAC8B72DCM937ST).
//!
//! - The persons: the user of each member of the riff, and the user of
//!   each known session. A riff with no sign-in has no members. The
//!   user of the riff server ([`SERVER_USER`]) is no person.
//! - The projects: the repository of each known session, also a gone one
//!   (`View::repositories`).
//!
//! A post carries the names of its line as selectors with the field
//! `name` only ([`Selector::name`]). `View::named` resolves each one
//! when the message is posted (01M6XZ280AMTAMPD6R4212YC2T):
//!
//! | The name is | It wakes | As the selector |
//! |---|---|---|
//! | a person | the terminals of the person | `user=USER,person=true` |
//! | a project | the lead of the sender in the project | `user=SENDER,repo=OWNER/REPO,lead=true` |
//! | neither | no session: the line still goes | none |
//!
//! A project with no live lead of the sender wakes no session: the line
//! waits in the log. A name never wakes a free session in the place of
//! a lead, as a `lead=true` selector of a thread does
//! (01M3JY1TBPQHH6WPPBTF42T64H). A post with no selector wakes no
//! session.
//!
//! ```mermaid
//! flowchart LR
//!     P["a post: to name=N"] --> R{"N is"}
//!     R -- "a person" --> T["the terminals of the person"]
//!     R -- "a project" --> L["the lead of the sender there"]
//!     R -- "neither" --> N["no session: the line goes"]
//!     L -- "no live lead" --> W["no session: the line waits in the log"]
//! ```
//!
//! [`State::names`](super::State::names) gives the list for the
//! type-ahead of `@`, with the kind of each name and whether its role is
//! online: a person that `who` shows as live, or a project with a live
//! lead of the caller.
//!
//! ```
//! use std::time::Instant;
//! use riff_core::name::SessionUri;
//! use riff_core::selector::Selector;
//! use riff_core::wire::{NameKind, Post};
//! use riff_state::state::State;
//!
//! let mike: SessionUri = "riff://mike@thelio".parse()?;
//! let lead: SessionUri = "riff://mike@thelio/como-technologies/riff?session=m1".parse()?;
//! let brett: SessionUri = "riff://brett@heron".parse()?;
//! let now = Instant::now();
//! let mut state = State::default();
//! for uri in [&mike, &lead, &brett] {
//!     state.register(uri, now);
//! }
//! state.lead(&lead, now).unwrap();
//!
//! let names: Vec<(String, NameKind)> =
//!     state.names(mike.who(), now).into_iter().map(|n| (n.name, n.kind)).collect();
//! assert_eq!(names, [
//!     ("brett".into(), NameKind::Person),
//!     ("mike".into(), NameKind::Person),
//!     ("riff".into(), NameKind::Project),
//! ]);
//!
//! // @brett @riff wakes the terminals of brett and the lead of mike in riff.
//! let to = vec![Selector::name("brett"), Selector::name("riff")];
//! let chat = Some("chat".parse()?);
//! let delivery = state.post(Post::new(&mike, chat, to, "@brett @riff is #12 done?"), now, 0).unwrap();
//! let woken: Vec<_> = delivery.wakes.iter().map(|(who, _)| who.clone()).collect();
//! assert_eq!(woken, [brett.who().clone(), lead.who().clone()]);
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use riff_core::name::{ThreadName, Who};
use riff_core::selector::Selector;
use riff_core::wire::{ChatName, NameKind};

use riff_core::name::SERVER_USER;

use super::view::View;

/// What a name of the chat stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Named {
    /// A person, by its user.
    Person(String),
    /// A project, by the thread of its repository.
    Project(ThreadName),
}

/// The name of `repo` after the slash.
fn short(repo: &ThreadName) -> String {
    let text = repo.to_string();
    text.rsplit('/').next().unwrap_or(&text).to_owned()
}

impl View<'_> {
    /// Each name of the chat with what it stands for, sorted by name with
    /// no regard to case.
    fn name_table(&self) -> Vec<(String, Named)> {
        let mut users: BTreeSet<String> =
            self.persons().into_iter().map(|(user, _)| user).collect();
        users.extend(
            self.presence
                .sessions
                .keys()
                .map(|who| who.user().to_owned()),
        );
        users.remove(SERVER_USER);
        let repos = self.repositories();
        let mut uses: BTreeMap<String, usize> = BTreeMap::new();
        for repo in &repos {
            *uses.entry(short(repo).to_lowercase()).or_default() += 1;
        }
        let people: BTreeSet<String> = users.iter().map(|u| u.to_lowercase()).collect();
        let mut table: Vec<(String, Named)> = users
            .into_iter()
            .map(|user| (user.clone(), Named::Person(user)))
            .collect();
        for repo in repos {
            let name = short(&repo);
            let key = name.to_lowercase();
            let clash = uses.get(&key).is_some_and(|n| *n > 1) || people.contains(&key);
            let name = if clash { repo.to_string() } else { name };
            table.push((name, Named::Project(repo)));
        }
        table.sort_by_key(|(name, _)| name.to_lowercase());
        table
    }

    /// The names of the chat for the caller `me`, sorted by name: the
    /// list for the type-ahead of `@`. A person is online when `who`
    /// shows it as live: a session of the person has an open watch. A
    /// project is online when `me` has a live lead there.
    pub(super) fn names(&self, me: &Who, now: Instant) -> Vec<ChatName> {
        self.name_table()
            .into_iter()
            .map(|(name, named)| {
                let (kind, online) = match &named {
                    Named::Person(user) => (
                        NameKind::Person,
                        self.presence
                            .sessions
                            .iter()
                            .any(|(who, s)| who.user() == user && s.watching(now)),
                    ),
                    Named::Project(repo) => (
                        NameKind::Project,
                        self.lead_of(&(me.user().to_owned(), repo.clone()), now)
                            .is_some_and(|lead| !self.gone(lead, now)),
                    ),
                };
                ChatName { name, kind, online }
            })
            .collect()
    }

    /// The selector that the name `name` in a post of `from` stands for:
    /// the terminals of a person, or the lead of `from` in a project.
    /// `None` for a name that is no person and no project.
    pub(super) fn named(&self, from: &Who, name: &str) -> Option<Selector> {
        let (_, named) = self.name_table().into_iter().find(|(known, named)| {
            known.eq_ignore_ascii_case(name)
                || matches!(named, Named::Project(repo) if repo.to_string().eq_ignore_ascii_case(name))
        })?;
        Some(match named {
            Named::Person(user) => Selector {
                user: Some(user),
                person: Some(true),
                ..Selector::default()
            },
            Named::Project(repo) => Selector::lead(from.user(), &repo.to_string()),
        })
    }

    /// Each live session other than `from` that the name `name` in a post
    /// of `from` wakes, and true when the name stands for `from` itself.
    /// A name with no live session wakes none: no free session takes the
    /// place of a lead.
    pub(super) fn woken_by_name(&self, from: &Who, name: &str, now: Instant) -> (Vec<Who>, bool) {
        let Some(target) = self.named(from, name) else {
            return (Vec::new(), false);
        };
        let woken = self
            .presence
            .sessions
            .keys()
            .filter(|who| *who != from && !self.gone(who, now))
            .filter(|who| target.matches(&self.uri(who, now)))
            .cloned()
            .collect();
        (woken, target.matches(&self.uri(from, now)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use riff_core::name::SessionUri;
    use riff_core::selector::Selector;
    use riff_core::wire::{NameKind, Post};

    use crate::state::{GONE, State};

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    /// The names of `state` for `me`: the name, the kind and online.
    fn names(state: &State, me: &SessionUri, now: Instant) -> Vec<(String, NameKind, bool)> {
        state
            .names(me.who(), now)
            .into_iter()
            .map(|n| (n.name, n.kind, n.online))
            .collect()
    }

    /// Two projects with the name `riff`, and a project with the name of
    /// the person `brett`: each of them has `OWNER/REPO`. The user of
    /// the server is no person.
    #[test]
    fn a_clash_gives_each_project_of_the_clash_its_owner() {
        let now = Instant::now();
        let mut state = State::default();
        for text in [
            "riff://mike@thelio/como-technologies/riff?session=m1",
            "riff://mike@thelio/acme/riff?session=m2",
            "riff://mike@thelio/como-technologies/strata?session=m3",
            "riff://brett@heron/como-technologies/brett?session=b1",
            "riff://riff@server",
        ] {
            state.register(&uri(text), now);
        }
        let me = uri("riff://mike@thelio");
        let listed: Vec<(String, NameKind)> = names(&state, &me, now)
            .into_iter()
            .map(|(name, kind, _)| (name, kind))
            .collect();
        let expected = [
            ("acme/riff", NameKind::Project),
            ("brett", NameKind::Person),
            ("como-technologies/brett", NameKind::Project),
            ("como-technologies/riff", NameKind::Project),
            ("mike", NameKind::Person),
            ("strata", NameKind::Project),
        ];
        let expected: Vec<(String, NameKind)> =
            expected.iter().map(|(n, k)| (n.to_string(), *k)).collect();
        assert_eq!(listed, expected);
    }

    /// A project is online while the caller has a live lead there.
    #[test]
    fn a_project_is_online_with_a_live_lead_of_the_caller() {
        let now = Instant::now();
        let mut state = State::default();
        let mike = uri("riff://mike@thelio");
        let lead = uri("riff://mike@thelio/como-technologies/riff?session=m1");
        let brett = uri("riff://brett@heron/como-technologies/strata?session=b1");
        for uri in [&mike, &lead, &brett] {
            state.register(uri, now);
        }
        state.lead(&lead, now).unwrap();
        state.lead(&brett, now).unwrap();
        let online = |state: &State, at| {
            names(state, &mike, at)
                .into_iter()
                .filter(|(_, kind, online)| *kind == NameKind::Project && *online)
                .map(|(name, _, _)| name)
                .collect::<Vec<_>>()
        };
        assert_eq!(online(&state, now), ["riff"], "brett's lead is not mike's");
        assert!(online(&state, now + GONE).is_empty(), "the lead is gone");
    }

    /// `OWNER/REPO` and a name in another case resolve too. A name with
    /// other fields is refused.
    #[test]
    fn a_name_resolves_with_no_regard_to_case_and_stands_alone() {
        let now = Instant::now();
        let mut state = State::default();
        let mike = uri("riff://mike@thelio");
        let lead = uri("riff://mike@thelio/como-technologies/riff?session=m1");
        state.register(&mike, now);
        state.register(&lead, now);
        state.lead(&lead, now).unwrap();
        let chat = Some("chat".parse().unwrap());
        for name in ["RIFF", "como-technologies/riff"] {
            let post = Post::new(&mike, chat.clone(), vec![Selector::name(name)], "hi");
            let delivery = state.post(post, now, 0).unwrap();
            assert_eq!(delivery.wakes.len(), 1, "{name}");
            assert!(delivery.unmatched.is_empty(), "{name}");
        }
        let wide = "name=riff,host=thelio".parse().unwrap();
        let refused = state.post(Post::new(&mike, chat, vec![wide], "hi"), now, 0);
        assert!(refused.unwrap_err().contains("a name stands alone"));
    }

    /// A project with no live lead of the sender wakes no session, not
    /// even a free session of the sender there: the line waits in the
    /// log. An unknown name wakes no session, and the line goes. Both
    /// names are in the unmatched selectors.
    #[test]
    fn a_name_with_no_live_role_wakes_no_one_and_the_line_goes() {
        let now = Instant::now();
        let mut state = State::default();
        let mike = uri("riff://mike@thelio");
        let free = uri("riff://mike@thelio/como-technologies/riff?session=m1");
        state.register(&mike, now);
        state.register(&free, now);
        let chat: riff_core::name::ThreadName = "chat".parse().unwrap();
        let to = vec![Selector::name("riff"), Selector::name("nobody")];
        let post = Post::new(&mike, Some(chat.clone()), to.clone(), "is #12 done?");
        let delivery = state.post(post, now, 0).unwrap();
        assert!(delivery.wakes.is_empty());
        assert_eq!(delivery.unmatched, to);
        let all = state.read(&mike, &chat, true, now).unwrap();
        assert_eq!(all.last().unwrap().body, "is #12 done?");
    }

    /// A name that stands for the sender wakes no session, and is no
    /// unmatched selector: the lead posts to its own project, and a
    /// person names itself.
    #[test]
    fn a_name_of_the_sender_is_matched_by_the_sender() {
        let now = Instant::now();
        let mut state = State::default();
        let lead = uri("riff://mike@thelio/como-technologies/riff?session=m1");
        let mike = uri("riff://mike@thelio");
        state.register(&lead, now);
        state.register(&mike, now);
        state.lead(&lead, now).unwrap();
        let chat = Some("chat".parse().unwrap());
        let me = state.uri(lead.who(), now);
        let post = Post::new(&me, chat.clone(), vec![Selector::name("riff")], "done");
        let delivery = state.post(post, now, 0).unwrap();
        assert!(delivery.wakes.is_empty() && delivery.unmatched.is_empty());
        let post = Post::new(&mike, chat, vec![Selector::name("mike")], "a note");
        let delivery = state.post(post, now, 0).unwrap();
        assert!(delivery.wakes.is_empty() && delivery.unmatched.is_empty());
    }
}
