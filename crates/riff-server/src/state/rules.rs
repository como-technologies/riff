//! The rules of [`permits`] and of [`Command::handle`], one test for
//! each rule, in the form given, when, then:
//!
//! - `given` applies records to an empty state.
//! - `live` makes sessions live, as a call does. It makes no record.
//! - `when` runs [`State::check`] with one command. The caller is live.
//!   A test names a command in a short form with no `me` ([`Ask`]):
//!   `when` puts the caller in it.
//! - `then` compares the changes, or `then_refused` the error.
//! - `apply` writes more records to the state of `given`, and gives the
//!   state, so that a test can look at it.
//!
//! The tests do no I/O.

use riff_core::record::{By, Claimed, Forgotten, Member, RiffStateSet, SettingChanged};
use riff_core::wire;

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

fn post(me: &SessionUri, thread: Option<&ThreadName>, to: &[&str], body: &str) -> Post {
    draft(me, thread, to, body)
}

/// A command in a short form with no `me`. `when` makes the command of
/// the caller from it.
trait Ask {
    type Command: Command;
    fn of(self, me: &SessionUri) -> Self::Command;
}

/// The short form of a command that has only a `me`.
macro_rules! asks {
    ($($ask:ident => $command:ty, $make:expr;)*) => {
        $(struct $ask;

        impl Ask for $ask {
            type Command = $command;
            fn of(self, me: &SessionUri) -> $command {
                let make: fn(SessionUri) -> $command = $make;
                make(me.clone())
            }
        })*
    };
}

asks! {
    Register => wire::Register, |me| wire::Register { me, worker: false };
    Start => wire::Start, |me| wire::Start { me };
    End => wire::End, |me| wire::End { me };
    Lead => wire::Lead, |me| wire::Lead { me };
    Pause => wire::Pause, |me| wire::Pause { me };
    Resume => wire::Resume, |me| wire::Resume { me };
}

struct Join(ThreadName);

impl Ask for Join {
    type Command = wire::Join;
    fn of(self, me: &SessionUri) -> wire::Join {
        wire::Join {
            me: me.clone(),
            thread: self.0,
        }
    }
}

struct Leave(ThreadName);

impl Ask for Leave {
    type Command = wire::Leave;
    fn of(self, me: &SessionUri) -> wire::Leave {
        wire::Leave {
            me: me.clone(),
            thread: self.0,
        }
    }
}

struct Claim {
    thread: ThreadName,
    item: String,
}

impl Ask for Claim {
    type Command = wire::Claim;
    fn of(self, me: &SessionUri) -> wire::Claim {
        wire::Claim {
            me: me.clone(),
            thread: self.thread,
            item: self.item,
        }
    }
}

struct Release {
    thread: ThreadName,
    item: String,
}

impl Ask for Release {
    type Command = wire::Release;
    fn of(self, me: &SessionUri) -> wire::Release {
        wire::Release {
            me: me.clone(),
            thread: self.thread,
            item: self.item,
        }
    }
}

struct ReleaseFor {
    thread: ThreadName,
    item: String,
    holder: String,
}

impl Ask for ReleaseFor {
    type Command = wire::ReleaseFor;
    fn of(self, me: &SessionUri) -> wire::ReleaseFor {
        wire::ReleaseFor {
            me: me.clone(),
            thread: self.thread,
            item: self.item,
            session: self.holder,
        }
    }
}

struct SetIdle {
    per_host: Option<u16>,
    after_secs: Option<u64>,
}

impl Ask for SetIdle {
    type Command = wire::SetIdle;
    fn of(self, me: &SessionUri) -> wire::SetIdle {
        wire::SetIdle {
            me: me.clone(),
            per_host: self.per_host,
            after_secs: self.after_secs,
        }
    }
}

/// A command with its `me`, or a command of the server, is its own
/// short form.
macro_rules! whole {
    ($($command:ty),*) => {
        $(impl Ask for $command {
            type Command = $command;
            fn of(self, _me: &SessionUri) -> $command {
                self
            }
        })*
    };
}

whole!(Post, Announce, Forget, MakeRiff);

fn claim(item: &str) -> Claim {
    Claim {
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
        by: None,
        command: None,
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
                .presence
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
            .presence
            .cursors
            .insert((me.who().clone(), thread.clone()), seq);
        self
    }

    /// Commits `changes` as records, writes them, and gives the state.
    fn apply(mut self, changes: &[Change]) -> State {
        let cause = Cause::of(&Caller::server(), CommandKind::Forget);
        let records = self.state.queue(&cause, changes, self.now);
        self.state.written(&records);
        self.state
    }

    /// The time passes by `by`.
    fn after(mut self, by: Duration) -> Given {
        self.now += by;
        self
    }

    /// The caller that acts as `me`, with the role of an admin, as in a
    /// riff with no sign-in.
    fn caller(me: &SessionUri) -> Caller {
        if me == &crate::owner::server_uri() {
            Caller::server()
        } else {
            Caller::of(me).with_role(Role::Admin)
        }
    }

    fn when<A: Ask>(self, me: &SessionUri, ask: A) -> When {
        self.when_as(&Given::caller(me), ask)
    }

    /// As `when`, for a caller with a class, a mark or a role of its
    /// own.
    fn when_as<A: Ask>(self, caller: &Caller, ask: A) -> When {
        let now = self.now;
        let me = caller.me().clone();
        let mut this = if caller.class() == Class::Server {
            self
        } else {
            self.live(std::slice::from_ref(&me))
        };
        let command = ask.of(&me);
        let check = this.state.check(caller, &command, now);
        assert!(check.registered.is_none(), "the caller is live");
        When(check.result.map(|(changes, _)| changes))
    }

    /// As `when`, for a caller that the state does not know: it is not
    /// made live first.
    fn when_new<A: Ask>(mut self, me: &SessionUri, ask: A) -> When {
        let command = ask.of(me);
        let check = self.state.check(&Given::caller(me), &command, self.now);
        When(check.result.map(|(changes, _)| changes))
    }
}

struct When(Result<Vec<Change>, Refused>);

impl When {
    fn then(self, changes: &[Change]) {
        assert_eq!(self.0.as_deref(), Ok(changes));
    }

    fn then_refused(self, part: &str) {
        match self.0 {
            Err(refused) => assert!(refused.reason.contains(part), "{refused}"),
            Ok(changes) => panic!("not refused: {changes:?}"),
        }
    }

    /// As `then_refused`, and it compares the code of the refusal.
    fn then_refused_as(self, code: Code, part: &str) {
        match self.0 {
            Err(refused) => {
                assert_eq!(refused.code, code, "{refused}");
                assert!(refused.reason.contains(part), "{refused}");
            }
            Ok(changes) => panic!("not refused: {changes:?}"),
        }
    }
}

#[test]
fn a_new_session_joins_its_repository_and_is_the_first_lead() {
    given(&[])
        .when(&ann(), Register)
        .then(&[joined(&ann(), &repo()), lead_set(&ann())]);
}

#[test]
fn a_second_session_of_a_user_is_not_the_lead_while_the_first_holds() {
    given(&[joined(&ann(), &repo()), lead_set(&ann())])
        .live(&[ann()])
        .when(&ann2(), Register)
        .then(&[joined(&ann2(), &repo())]);
}

#[test]
fn a_second_session_is_the_lead_when_the_first_stopped_long_ago() {
    given(&[joined(&ann(), &repo()), lead_set(&ann())])
        .after(CLAIM_GRACE)
        .when(&ann2(), Register)
        .then(&[joined(&ann2(), &repo()), lead_set(&ann2())]);
}

#[test]
fn a_register_of_a_member_and_lead_changes_nothing() {
    given(&team()).when(&ann(), Register).then(&[]);
}

#[test]
fn a_person_joins_no_thread_on_register() {
    given(&[]).when(&person(), Register).then(&[]);
}

#[test]
fn a_join_adds_a_member_once() {
    given(&[])
        .live(&[ann()])
        .when(&ann(), Join(design()))
        .then(&[joined(&ann(), &design())]);
    given(&[joined(&ann(), &design())])
        .when(&ann(), Join(design()))
        .then(&[]);
}

#[test]
fn a_leave_removes_a_member_or_a_lead() {
    given(&team())
        .when(&ann(), Leave(repo()))
        .then(&[left(&ann(), &repo())]);
    given(&[lead_set(&ann())])
        .when(&ann(), Leave(repo()))
        .then(&[left(&ann(), &repo())]);
    given(&team()).when(&ann(), Leave(design())).then(&[]);
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
    given(&team()).live(&[bob()]).when(&ann(), note).then(&[
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
        .when(&ann2().with_lead(true), signed)
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
        .when(&ann(), copy)
        .then_refused("a copy of message 1");
}

#[test]
fn the_server_announces_as_itself_and_joins_no_thread() {
    let server = crate::owner::server_uri();
    let announce = Announce {
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

/// The refusal names the holder (01M3WRD9JBQMNN96TXJH8EAJ3W).
#[test]
fn a_claim_of_an_item_that_a_live_session_holds_is_refused_as_held() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    given(&records)
        .live(&[bob()])
        .when(&ann(), claim("issue-7"))
        .then_refused_as(Code::Held, "bob@kite:app (b1) holds issue-7 in acme/app.");
}

#[test]
fn a_claim_of_an_item_whose_holder_stopped_long_ago_is_granted() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    given(&records)
        .after(CLAIM_GRACE)
        .when(&ann(), claim("issue-7"))
        .then(&[released(&bob(), "issue-7"), claimed(&ann(), "issue-7")]);
}

/// The old holder gets its `released` record first, in the chunk of the
/// claim, with the cause of the claim (RID_TAKEN).
#[test]
fn a_claim_that_takes_an_item_gives_the_old_holder_a_released_record_in_one_chunk() {
    let mut records = team();
    records.push(claimed(&bob(), "issue-7"));
    let Given { mut state, now } = given(&records).after(CLAIM_GRACE);
    let command = claim("issue-7").of(&ann());
    let (made, ()) = state.run(&Caller::of(&ann()), &command, now).unwrap();
    let kinds: Vec<&Change> = made.iter().map(|record| &record.change).collect();
    assert_eq!(
        kinds,
        [&released(&bob(), "issue-7"), &claimed(&ann(), "issue-7")]
    );
    assert_eq!(made[1].position, made[0].position + 1);
    for record in &made {
        assert_eq!(record.by, Some(By::Session(ann().who().clone())));
        assert_eq!(record.command.as_deref(), Some("claim"));
    }
    // The item has one holder, and the old holder has no claim.
    state.written(&made);
    assert_eq!(state.uri(ann().who(), now).claims(), ["issue-7"]);
    assert!(state.uri(bob().who(), now).claims().is_empty());
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
        .then_refused_as(Code::Paused, "paused");
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
            Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then(&[released(&bob(), "issue-7")]);
    given(&records)
        .live(&[bob()])
        .when(
            &ann(),
            Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then_refused("is held by bob");
    given(&team())
        .when(
            &ann(),
            Release {
                thread: repo(),
                item: "issue-7".into(),
            },
        )
        .then_refused("nobody holds issue-7");
}

fn release_for(holder: &str) -> ReleaseFor {
    ReleaseFor {
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
    let note = |holder: &str| Message {
        kind: Kind::Note,
        at_ms: 0,
        ..message(
            crate::owner::server_uri(),
            1,
            &[],
            &format!(
                "claims: the lead ann@heron:app (a1) released issue-7 for the session \
                 {holder} (a2). issue-7 is free."
            ),
        )
    };
    // The holder is gone: it made no call since the replay.
    given(&records).when(&ann(), release_for("a2")).then(&[
        released(&ann2(), "issue-7"),
        posted(&repo(), note("ann@heron:app#api"), &[]),
    ]);
    given(&records)
        .live(&[ann2()])
        .when(&ann(), release_for("a2"))
        .then(&[
            released(&ann2(), "issue-7"),
            posted(&repo(), note("ann@heron:app#api"), &[]),
        ]);
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
    // A person is no lead: `permits` refuses it.
    given(&records)
        .when(&person(), release_for("a2"))
        .then_refused_as(
            Code::NotAllowed,
            "a person cannot send the command release_for",
        );
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
    given(&records)
        .when(&ann(), End)
        .then(&[released(&ann(), "issue-7"), released(&ann(), "issue-8")]);
    given(&records)
        .when(&ann(), Start)
        .then(&[released(&ann(), "issue-7"), released(&ann(), "issue-8")]);
}

#[test]
fn a_lead_call_makes_the_session_the_lead() {
    given(&team())
        .live(&[ann()])
        .when(&ann2(), Lead)
        .then(&[lead_set(&ann2())]);
    given(&team()).when(&ann(), Lead).then(&[]);
}

#[test]
fn a_person_or_a_session_outside_git_is_never_the_lead() {
    given(&[])
        .when(&person(), Lead)
        .then_refused("only an agent session");
    let outside: SessionUri = "riff://ann@heron/-?session=a9".parse().unwrap();
    given(&[])
        .when(&outside, Lead)
        .then_refused("needs a git repository");
}

#[test]
fn only_a_person_or_the_lead_sets_the_riff_state() {
    given(&team())
        .when(&ann(), Pause)
        .then(&[riff_set(RiffState::Paused)]);
    given(&team())
        .when(&person(), Pause)
        .then(&[riff_set(RiffState::Paused)]);
    given(&team())
        .live(&[ann()])
        .when(&ann2(), Pause)
        .then_refused_as(Code::NotAllowed, "only your user or the lead");
    given(&team()).when(&ann(), Resume).then(&[]);
}

#[test]
fn a_change_of_the_idle_settings_is_a_record() {
    let idle = Idle {
        per_host: 2,
        after_secs: 60,
    };
    let set = |per_host, after_secs| SetIdle {
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
        .when(&crate::owner::server_uri(), Forget)
        .then(&[forgotten(&ann()), forgotten(&bob())]);
}

#[test]
fn a_session_with_a_sign_of_life_in_the_expiry_is_not_forgotten() {
    given(&three_with_direct_threads())
        .after(SESSION_EXPIRY - Duration::from_secs(60))
        .when(&crate::owner::server_uri(), Forget)
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
    let threads = &state.written.threads().by_name;

    // The cursors of ann and bob are gone. Carol keeps hers.
    let readers: Vec<&Who> = state.presence.cursors.keys().map(|(who, _)| who).collect();
    assert_eq!(readers, [carol().who()]);
    // Ann and bob are in no thread.
    for thread in threads.values() {
        assert!(!thread.members.contains(ann().who()));
        assert!(!thread.members.contains(bob().who()));
    }
    // The direct thread of ann and bob is gone: both sessions are gone.
    assert!(!threads.contains_key(&ann_bob));
    // The direct thread of ann and carol stays while carol is known.
    assert!(threads.contains_key(&ann_carol));
    assert!(!state.presence.sessions.contains_key(ann().who()));
    assert!(state.presence.sessions.contains_key(carol().who()));

    // When carol is forgotten too, her direct thread with ann goes.
    let state = Given {
        state,
        now: Instant::now(),
    }
    .apply(&[forgotten(&carol())]);
    assert!(!state.written.threads().by_name.contains_key(&ann_carol));
    assert!(state.presence.cursors.is_empty());
    assert_eq!(state.written.threads().by_name[&repo()].members.len(), 0);
}

#[test]
fn a_forgotten_session_loses_its_claims_and_its_lead() {
    let state = given(&team()).apply(&[claimed(&ann(), "issue-7"), forgotten(&ann())]);
    assert!(state.written.work().claims.is_empty());
    assert_eq!(
        state.written.work().leads.values().collect::<Vec<_>>(),
        [bob().who()]
    );
}

#[test]
fn make_riff_pauses_only_a_riff_with_no_record() {
    let server = crate::owner::server_uri();
    given(&[])
        .when(&server, MakeRiff)
        .then(&[riff_set(RiffState::Paused)]);
    given(&team()).when(&server, MakeRiff).then(&[]);
}

/// Only the server sends a command of the server
/// (01M3WRD959DYNZHDKP5ZT9Q1C7).
#[test]
fn a_session_cannot_send_a_command_of_the_server() {
    given(&team())
        .when(&ann(), Forget)
        .then_refused_as(Code::NotAllowed, "a session cannot send the command forget");
    given(&team())
        .when(&person(), MakeRiff)
        .then_refused_as(Code::NotAllowed, "a person cannot send");
}

/// A change of the idle settings needs an admin
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74).
#[test]
fn a_member_cannot_change_the_idle_settings() {
    let set = SetIdle {
        per_host: Some(2),
        after_secs: None,
    };
    given(&team())
        .when_as(&Caller::of(&person()), set)
        .then_refused_as(Code::NotAllowed, "ann is not an admin");
    let zero = SetIdle {
        per_host: None,
        after_secs: Some(0),
    };
    given(&team())
        .when(&person(), zero)
        .then_refused_as(Code::BadRequest, "at least 1 second");
}

/// A worker is never the lead (01M3WRD959DYNZHDKP5ZT9Q1C7). The state
/// adds the worker mark of the caller before `permits`.
#[test]
fn a_worker_cannot_send_lead() {
    let mut given = given(&team()).live(&[ann(), ann2()]);
    let sessions = &mut given.state.presence.sessions;
    sessions.get_mut(ann2().who()).unwrap().worker = true;
    given
        .when(&ann2(), Lead)
        .then_refused_as(Code::NotAllowed, "a worker cannot be the lead");
}

/// A command that is not refused sets its signal. A refused command
/// sets none (01M3WRD97EZJK3AABXECXEY133).
#[test]
fn a_refused_command_sets_no_signal() {
    let Given { mut state, now } = given(&team()).live(&[ann(), person()]);
    // `permits` refuses the end of a person.
    let end = wire::End { me: person() };
    let check = state.check(&Caller::of(&person()), &end, now);
    assert_eq!(check.result.unwrap_err().code, Code::NotAllowed);
    assert!(!state.presence.sessions[person().who()].ended);

    let end = wire::End { me: ann() };
    assert!(state.check(&Caller::of(&ann()), &end, now).result.is_ok());
    assert!(state.presence.sessions[ann().who()].ended);
}

/// A call of a session that the state does not know runs `register`
/// first, as a command of its own. An `end` registers nothing.
#[test]
fn a_call_of_a_new_session_registers_it_first_but_an_end_does_not() {
    let now = Instant::now();
    let mut state = State::default();
    let end = wire::End { me: ann() };
    let check = state.check(&Caller::of(&ann()), &end, now);
    assert!(check.registered.is_none());
    assert_eq!(check.result.unwrap().0, []);
    assert!(!state.knows(ann().who()));

    let join = wire::Join {
        me: ann(),
        thread: design(),
    };
    let check = state.check(&Caller::of(&ann()), &join, now);
    let registered: Vec<Change> = check
        .registered
        .unwrap()
        .into_iter()
        .map(|record| record.change)
        .collect();
    assert_eq!(registered, [joined(&ann(), &repo()), lead_set(&ann())]);
    assert_eq!(check.result.unwrap().0, [joined(&ann(), &design())]);
    assert!(state.knows(ann().who()));
}

/// A session ID that the state knows under another user is refused
/// with the code `other_user` (R159).
#[test]
fn a_known_session_id_under_another_user_is_refused_as_other_user() {
    let other: SessionUri = "riff://bob@kite/acme/app?session=a1".parse().unwrap();
    given(&team())
        .live(&[ann()])
        .when_new(&other, Register)
        .then_refused_as(Code::OtherUser, "known as user ann");
}
