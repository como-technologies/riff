//! The rules of [`State::handle`], one test for each rule, in the form
//! given, when, then:
//!
//! - `given` applies records to an empty state.
//! - `live` makes sessions live, as a call does. It makes no record.
//! - `when` runs `handle` with one command. The caller is live.
//! - `then` compares the changes, or `then_refused` the error.
//! - `apply` writes more records to the state of `given`, and gives the
//!   state, so that a test can look at it.
//!
//! The tests do no I/O.

use super::*;

fn ann() -> SessionUri {
    "riff://ann@heron/acme/app?session=a1".parse().unwrap()
}

fn ann2() -> SessionUri {
    "riff://ann@heron/acme/app?session=a2#api".parse().unwrap()
}

fn bob() -> SessionUri {
    "riff://bob@kite/acme/app?session=b1".parse().unwrap()
}

/// A person on the command line: no session, no repository.
fn person() -> SessionUri {
    "riff://ann@heron".parse().unwrap()
}

fn repo() -> ThreadName {
    "acme/app".parse().unwrap()
}

fn design() -> ThreadName {
    "design".parse().unwrap()
}

fn joined(me: &SessionUri, thread: &ThreadName) -> Change {
    Change::JoinedThread(Member {
        session: me.clone(),
        thread: thread.clone(),
    })
}

fn left(me: &SessionUri, thread: &ThreadName) -> Change {
    Change::LeftThread(Member {
        session: me.clone(),
        thread: thread.clone(),
    })
}

fn lead_set(me: &SessionUri) -> Change {
    Change::LeadSet(Member {
        session: me.clone(),
        thread: repo(),
    })
}

fn claimed(me: &SessionUri, item: &str) -> Change {
    Change::Claimed(Claimed {
        session: me.clone(),
        thread: repo(),
        item: item.into(),
    })
}

fn released(me: &SessionUri, item: &str) -> Change {
    Change::Released(Claimed {
        session: me.clone(),
        thread: repo(),
        item: item.into(),
    })
}

fn riff_set(state: RiffState) -> Change {
    Change::RiffStateSet(RiffStateSet { state })
}

/// Ann and bob in the repository thread of a running riff, each the lead
/// of its user.
fn team() -> Vec<Change> {
    vec![
        joined(&ann(), &repo()),
        lead_set(&ann()),
        joined(&bob(), &repo()),
        lead_set(&bob()),
        riff_set(RiffState::Running),
    ]
}

fn message(from: SessionUri, seq: u64, to: &[&str], body: &str) -> Message {
    Message {
        seq,
        from,
        to: to.iter().map(|s| s.parse().unwrap()).collect(),
        body: body.into(),
        at_ms: 5,
        kind: Kind::Message,
        sig: None,
        payload: None,
    }
}

fn posted(thread: &ThreadName, message: Message, woken: &[&SessionUri]) -> Change {
    Change::Posted(Box::new(Posted {
        thread: thread.clone(),
        message,
        woken: woken.iter().map(|u| u.who().clone()).collect(),
    }))
}

/// A post at the time 5.
fn draft(me: &SessionUri, thread: Option<&ThreadName>, to: &[&str], body: &str) -> Post {
    let to = to.iter().map(|s| s.parse().unwrap()).collect();
    Post {
        at_ms: Some(5),
        ..Post::new(me, thread.cloned(), to, body)
    }
}

fn post(me: &SessionUri, thread: Option<&ThreadName>, to: &[&str], body: &str) -> Command {
    Command::Post(Box::new(draft(me, thread, to, body)))
}

fn claim(item: &str) -> Command {
    Command::Claim {
        thread: repo(),
        item: item.into(),
    }
}

struct Given {
    state: State,
    now: Instant,
}

fn given(changes: &[Change]) -> Given {
    let now = Instant::now();
    let records = changes.iter().enumerate().map(|(n, change)| Record {
        position: u64::try_from(n).unwrap() + 1,
        written_at_ms: 0,
        change: change.clone(),
    });
    Given {
        state: State::replay(records, now, 0),
        now,
    }
}

impl Given {
    /// Makes each session live and known in its place, as a call does,
    /// with no record.
    fn live(mut self, sessions: &[SessionUri]) -> Given {
        for me in sessions {
            let session = self
                .state
                .sessions
                .entry(me.who().clone())
                .or_insert_with(|| Session::new(me.place().clone(), self.now));
            session.place = me.place().clone();
            session.live(self.now);
        }
        self
    }

    /// `me` read `thread` up to `seq`, with no record.
    fn read_to(mut self, me: &SessionUri, thread: &ThreadName, seq: u64) -> Given {
        self.state
            .cursors
            .insert((me.who().clone(), thread.clone()), seq);
        self
    }

    /// Commits `changes` as records, writes them, and gives the state.
    fn apply(mut self, changes: &[Change]) -> State {
        self.state.commit(changes, self.now);
        let records = self.state.take_queue();
        self.state.written(&records);
        self.state
    }

    /// The time passes by `by`.
    fn after(mut self, by: Duration) -> Given {
        self.now += by;
        self
    }

    fn when(self, me: &SessionUri, command: Command) -> When {
        let now = self.now;
        let this = if me == &crate::owner::server_uri() {
            self
        } else {
            self.live(std::slice::from_ref(me))
        };
        When(this.state.handle(me, &command, now))
    }
}

struct When(Result<Vec<Change>, String>);

impl When {
    fn then(self, changes: &[Change]) {
        assert_eq!(self.0.as_deref(), Ok(changes));
    }

    fn then_refused(self, part: &str) {
        match self.0 {
            Err(error) => assert!(error.contains(part), "{error}"),
            Ok(changes) => panic!("not refused: {changes:?}"),
        }
    }
}

#[test]
fn a_new_session_joins_its_repository_and_is_the_first_lead() {
    given(&[])
        .when(&ann(), Command::Register)
        .then(&[joined(&ann(), &repo()), lead_set(&ann())]);
}

#[test]
fn a_second_session_of_a_user_is_not_the_lead_while_the_first_holds() {
    given(&[joined(&ann(), &repo()), lead_set(&ann())])
        .live(&[ann()])
        .when(&ann2(), Command::Register)
        .then(&[joined(&ann2(), &repo())]);
}

#[test]
fn a_second_session_is_the_lead_when_the_first_stopped_long_ago() {
    given(&[joined(&ann(), &repo()), lead_set(&ann())])
        .after(CLAIM_GRACE)
        .when(&ann2(), Command::Register)
        .then(&[joined(&ann2(), &repo()), lead_set(&ann2())]);
}

#[test]
fn a_register_of_a_member_and_lead_changes_nothing() {
    given(&team()).when(&ann(), Command::Register).then(&[]);
}

#[test]
fn a_person_joins_no_thread_on_register() {
    given(&[]).when(&person(), Command::Register).then(&[]);
}

#[test]
fn a_join_adds_a_member_once() {
    given(&[])
        .live(&[ann()])
        .when(&ann(), Command::Join(design()))
        .then(&[joined(&ann(), &design())]);
    given(&[joined(&ann(), &design())])
        .when(&ann(), Command::Join(design()))
        .then(&[]);
}

#[test]
fn a_leave_removes_a_member_or_a_lead() {
    given(&team())
        .when(&ann(), Command::Leave(repo()))
        .then(&[left(&ann(), &repo())]);
    given(&[lead_set(&ann())])
        .when(&ann(), Command::Leave(repo()))
        .then(&[left(&ann(), &repo())]);
    given(&team())
        .when(&ann(), Command::Leave(design()))
        .then(&[]);
}

#[test]
fn a_post_wakes_each_selected_session() {
    let sender = ann().with_lead(true);
    given(&team())
        .live(&[bob()])
        .when(&ann(), post(&ann(), Some(&repo()), &["user=bob"], "hi"))
        .then(&[posted(
            &repo(),
            message(sender, 1, &["user=bob"], "hi"),
            &[&bob()],
        )]);
}

#[test]
fn a_post_joins_its_sender_and_each_woken_session() {
    let sender = ann().with_lead(true);
    given(&team())
        .live(&[bob()])
        .when(&ann(), post(&ann(), Some(&design()), &["user=bob"], "look"))
        .then(&[
            joined(&ann(), &design()),
            joined(&bob(), &design()),
            posted(
                &design(),
                message(sender, 1, &["user=bob"], "look"),
                &[&bob()],
            ),
        ]);
}

#[test]
fn a_post_gets_the_next_seq_of_its_thread() {
    let first = posted(&repo(), message(ann(), 1, &[], "one"), &[]);
    let sender = ann().with_lead(true);
    let mut records = team();
    records.push(first);
    given(&records)
        .when(&ann(), post(&ann(), Some(&repo()), &[], "two"))
        .then(&[posted(&repo(), message(sender, 2, &[], "two"), &[])]);
}

#[test]
fn a_gone_session_does_not_wake() {
    let sender = ann().with_lead(true);
    given(&team())
        .when(&ann(), post(&ann(), Some(&repo()), &["user=bob"], "hi"))
        .then(&[posted(
            &repo(),
            message(sender, 1, &["user=bob"], "hi"),
            &[],
        )]);
}

#[test]
fn a_note_wakes_nobody_but_its_selected_sessions_join() {
    let sender = ann().with_lead(true);
    let note = draft(&ann(), Some(&design()), &["user=bob"], "fyi");
    let note = Post {
        kind: Kind::Note,
        ..note
    };
    let expected = Message {
        kind: Kind::Note,
        ..message(sender, 1, &["user=bob"], "fyi")
    };
    given(&team())
        .live(&[bob()])
        .when(&ann(), Command::Post(Box::new(note)))
        .then(&[
            joined(&ann(), &design()),
            joined(&bob(), &design()),
            posted(&design(), expected, &[]),
        ]);
}

#[test]
fn a_direct_message_goes_to_the_direct_thread_of_the_two_sessions() {
    let direct = ThreadName::direct(ann().who(), bob().who());
    let sender = ann().with_lead(true);
    given(&team())
        .live(&[bob()])
        .when(&ann(), post(&ann(), None, &["session=b1"], "psst"))
        .then(&[
            joined(&ann(), &direct),
            joined(&bob(), &direct),
            posted(
                &direct,
                message(sender, 1, &["session=b1"], "psst"),
                &[&bob()],
            ),
        ]);
}

#[test]
fn a_direct_message_to_a_gone_session_is_refused() {
    given(&team())
        .when(&ann(), post(&ann(), None, &["session=b1"], "psst"))
        .then_refused("is gone");
}

#[test]
fn a_direct_message_needs_one_selector_with_a_session_or_the_lead() {
    let refused = |to: &[&str], part: &str| {
        given(&team())
            .live(&[bob()])
            .when(&ann(), post(&ann(), None, to, "psst"))
            .then_refused(part);
    };
    refused(&["session=b1", "user=bob"], "exactly one selector");
    refused(&["user=bob"], "a session or lead=true");
    refused(&["user=cy,lead=true"], "Ask your own user");
    refused(&["session=zz"], "no session matches");
}

#[test]
fn a_post_to_a_named_direct_thread_is_refused() {
    let direct = ThreadName::direct(ann().who(), bob().who());
    given(&team())
        .live(&[bob()])
        .when(&ann(), post(&ann(), Some(&direct), &["session=b1"], "psst"))
        .then_refused("leave out the thread");
}

#[test]
fn a_signed_post_with_the_lead_mark_of_a_session_that_is_not_the_lead_is_refused() {
    let signed = draft(&ann2().with_lead(true), Some(&repo()), &[], "merge");
    let signed = Post {
        sig: Some("sig".into()),
        payload: Some("payload".into()),
        ..signed
    };
    given(&team())
        .when(&ann2().with_lead(true), Command::Post(Box::new(signed)))
        .then_refused("not the lead");
}

#[test]
fn a_copy_of_a_signed_payload_is_refused() {
    let mut kept = message(ann(), 1, &[], "request");
    kept.sig = Some("first".into());
    kept.payload = Some("payload".into());
    let mut records = team();
    records.push(posted(&repo(), kept, &[]));
    let copy = draft(&ann(), Some(&repo()), &[], "request");
    let copy = Post {
        sig: Some("second".into()),
        payload: Some("payload".into()),
        ..copy
    };
    given(&records)
        .when(&ann(), Command::Post(Box::new(copy)))
        .then_refused("a copy of message 1");
}

#[test]
fn the_server_announces_as_itself_and_joins_no_thread() {
    let server = crate::owner::server_uri();
    let announce = Command::Announce {
        thread: Some(repo()),
        to: vec!["user=bob".parse().unwrap()],
        body: "news".into(),
        kind: Kind::Note,
        at_ms: 5,
    };
    let expected = Message {
        kind: Kind::Note,
        ..message(server.clone(), 1, &["user=bob"], "news")
    };
    given(&team())
        .live(&[bob()])
        .when(&server, announce)
        .then(&[posted(&repo(), expected, &[])]);
}

#[test]
fn a_claim_of_a_free_item_is_granted() {
    given(&team())
        .when(&ann(), claim("issue-7"))
        .then(&[claimed(&ann(), "issue-7")]);
}

#[test]
fn a_claim_of_an_item_that_a_live_session_holds_changes_nothing() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    given(&records)
        .live(&[bob()])
        .when(&ann(), claim("issue-7"))
        .then(&[]);
}

#[test]
fn a_claim_of_an_item_whose_holder_stopped_long_ago_is_granted() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    given(&records)
        .after(CLAIM_GRACE)
        .when(&ann(), claim("issue-7"))
        .then(&[claimed(&ann(), "issue-7")]);
}

#[test]
fn a_claim_of_an_own_item_changes_nothing() {
    let mut records = team();
    records.push(claimed(&ann(), "issue-7"));
    given(&records).when(&ann(), claim("issue-7")).then(&[]);
}

#[test]
fn a_claim_in_a_paused_riff_is_refused() {
    given(&[joined(&ann(), &repo())])
        .when(&ann(), claim("issue-7"))
        .then_refused("paused");
}

#[test]
fn a_claim_that_does_not_fit_in_a_uri_is_refused() {
    given(&team())
        .when(&ann(), claim("issue 7"))
        .then_refused("claim");
}

#[test]
fn only_the_holder_releases_a_claim() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    given(&records)
        .live(&[bob()])
        .when(
            &bob(),
            Command::Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then(&[released(&bob(), "issue-7")]);
    given(&records)
        .live(&[bob()])
        .when(
            &ann(),
            Command::Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then_refused("is held by bob");
    given(&team())
        .when(
            &ann(),
            Command::Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then_refused("nobody holds issue-7");
}

fn release_for(holder: &str) -> Command {
    Command::ReleaseFor {
        thread: repo(),
        item: "issue-7".into(),
        holder: holder.into(),
    }
}

/// The lead of a user frees the claim of another session of that user,
/// live or gone (01M3WG243BW7P6E1ME0DFNQF8C). The record names the
/// holder.
#[test]
fn the_lead_releases_the_claim_of_a_session_of_its_user() {
    let mut records = team();
    records.push(claimed(&ann2(), "issue-7"));
    // The holder is gone: it made no call since the replay.
    given(&records)
        .when(&ann(), release_for("a2"))
        .then(&[released(&ann2(), "issue-7")]);
    given(&records)
        .live(&[ann2()])
        .when(&ann(), release_for("a2"))
        .then(&[released(&ann2(), "issue-7")]);
}

#[test]
fn a_session_that_is_not_the_lead_releases_no_claim_of_another_session() {
    let third: SessionUri = "riff://ann@heron/acme/app?session=a3".parse().unwrap();
    let mut records = team();
    records.push(joined(&third, &repo()));
    records.push(claimed(&ann2(), "issue-7"));
    given(&records)
        .when(&third, release_for("a2"))
        .then_refused("Only the lead of your user");
    // The lead of another user is refused too.
    given(&records)
        .when(&bob(), release_for("a2"))
        .then_refused("Only the lead of its user");
    // A person is no lead.
    given(&records)
        .when(&person(), release_for("a2"))
        .then_refused("Only the lead of its user");
}

#[test]
fn a_release_for_a_session_names_the_holder() {
    let mut records = team();
    records.push(claimed(&ann2(), "issue-7"));
    given(&records)
        .when(&ann(), release_for("b1"))
        .then_refused("not by the session b1");
    given(&team())
        .when(&ann(), release_for("a2"))
        .then_refused("nobody holds issue-7");
}

#[test]
fn an_end_and_a_new_start_free_each_claim() {
    let mut records = team();
    records.extend([claimed(&ann(), "issue-7"), claimed(&ann(), "issue-8")]);
    records.push(claimed(&bob(), "issue-9"));
    for command in [Command::End, Command::Start] {
        given(&records)
            .when(&ann(), command)
            .then(&[released(&ann(), "issue-7"), released(&ann(), "issue-8")]);
    }
}

#[test]
fn a_lead_call_makes_the_session_the_lead() {
    given(&team())
        .live(&[ann()])
        .when(&ann2(), Command::Lead)
        .then(&[lead_set(&ann2())]);
    given(&team()).when(&ann(), Command::Lead).then(&[]);
}

#[test]
fn a_person_or_a_session_outside_git_is_never_the_lead() {
    given(&[])
        .when(&person(), Command::Lead)
        .then_refused("only an agent session");
    let outside: SessionUri = "riff://ann@heron/-?session=a9".parse().unwrap();
    given(&[])
        .when(&outside, Command::Lead)
        .then_refused("needs a git repository");
}

#[test]
fn only_a_person_or_the_lead_sets_the_riff_state() {
    given(&team())
        .when(&ann(), Command::SetRiff(RiffState::Paused))
        .then(&[riff_set(RiffState::Paused)]);
    given(&team())
        .when(&person(), Command::SetRiff(RiffState::Paused))
        .then(&[riff_set(RiffState::Paused)]);
    given(&team())
        .live(&[ann()])
        .when(&ann2(), Command::SetRiff(RiffState::Paused))
        .then_refused("only your user or the lead");
    given(&team())
        .when(&ann(), Command::SetRiff(RiffState::Running))
        .then(&[]);
}

#[test]
fn a_change_of_the_idle_settings_is_a_record() {
    let idle = Idle {
        per_host: 2,
        after_secs: 60,
    };
    let set = |per_host, after_secs| Command::SetIdle {
        per_host,
        after_secs,
    };
    given(&team())
        .when(&person(), set(Some(2), None))
        .then(&[Change::SettingChanged(SettingChanged { idle })]);
    given(&team())
        .when(&person(), set(Some(1), Some(60)))
        .then(&[]);
}

fn carol() -> SessionUri {
    "riff://carol@wren/acme/app?session=c1".parse().unwrap()
}

fn forgotten(me: &SessionUri) -> Change {
    Change::SessionForgotten(Forgotten {
        session: me.clone(),
    })
}

fn direct(a: &SessionUri, b: &SessionUri) -> ThreadName {
    ThreadName::direct(a.who(), b.who())
}

/// Ann, bob and carol in the repository thread, with a direct message
/// from ann to bob and one from ann to carol.
fn three_with_direct_threads() -> Vec<Change> {
    let mut changes = vec![
        joined(&ann(), &repo()),
        joined(&bob(), &repo()),
        joined(&carol(), &repo()),
        joined(&bob(), &design()),
        riff_set(RiffState::Running),
    ];
    for peer in [bob(), carol()] {
        let thread = direct(&ann(), &peer);
        changes.push(joined(&ann(), &thread));
        changes.push(joined(&peer, &thread));
        changes.push(posted(&thread, message(ann(), 1, &[], "hi"), &[&peer]));
    }
    changes
}

#[test]
fn a_session_with_no_sign_of_life_for_the_expiry_is_forgotten() {
    given(&three_with_direct_threads())
        .after(SESSION_EXPIRY)
        .live(&[carol()])
        .when(&crate::owner::server_uri(), Command::Forget)
        .then(&[forgotten(&ann()), forgotten(&bob())]);
}

#[test]
fn a_session_with_a_sign_of_life_in_the_expiry_is_not_forgotten() {
    given(&three_with_direct_threads())
        .after(SESSION_EXPIRY - Duration::from_secs(60))
        .when(&crate::owner::server_uri(), Command::Forget)
        .then(&[]);
}

#[test]
fn session_forgotten_drops_the_cursors_the_memberships_and_the_direct_thread_of_two_gone_sessions()
{
    let ann_bob = direct(&ann(), &bob());
    let ann_carol = direct(&ann(), &carol());
    let state = given(&three_with_direct_threads())
        .read_to(&ann(), &repo(), 1)
        .read_to(&bob(), &ann_bob, 1)
        .read_to(&bob(), &design(), 1)
        .read_to(&carol(), &ann_carol, 1)
        .apply(&[forgotten(&ann()), forgotten(&bob())]);
    let riff = &state.written;

    // The cursors of ann and bob are gone. Carol keeps hers.
    let readers: Vec<&Who> = state.cursors.keys().map(|(who, _)| who).collect();
    assert_eq!(readers, [carol().who()]);
    // Ann and bob are in no thread.
    for thread in riff.threads.values() {
        assert!(!thread.members.contains(ann().who()));
        assert!(!thread.members.contains(bob().who()));
    }
    // The direct thread of ann and bob is gone: both sessions are gone.
    assert!(!riff.threads.contains_key(&ann_bob));
    // The direct thread of ann and carol stays while carol is known.
    assert!(riff.threads.contains_key(&ann_carol));
    assert!(!state.sessions.contains_key(ann().who()));
    assert!(state.sessions.contains_key(carol().who()));

    // When carol is forgotten too, her direct thread with ann goes.
    let state = Given {
        state,
        now: Instant::now(),
    }
    .apply(&[forgotten(&carol())]);
    assert!(!state.written.threads.contains_key(&ann_carol));
    assert!(state.cursors.is_empty());
    assert_eq!(state.written.threads[&repo()].members.len(), 0);
}

#[test]
fn a_forgotten_session_loses_its_claims_and_its_lead() {
    let state = given(&team()).apply(&[claimed(&ann(), "issue-7"), forgotten(&ann())]);
    assert!(state.written.claims.is_empty());
    assert_eq!(
        state.written.leads.values().collect::<Vec<_>>(),
        [bob().who()]
    );
}
