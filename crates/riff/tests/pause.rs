//! Pause and resume (01M3JCFTWCR72HQB8CBTQKXJNF to
//! 01M3JCG4AV80MHFP73CWDY5E3M), over real HTTP: a new riff is paused, a
//! pause wakes the sessions and keeps their claims, and a session with
//! work runs the WIP steps of the skill. They push its branch and
//! nothing to the default branch.
//!
//! The two pauses (01M3XAHZBGSSJB3YX23K88W01K to
//! 01M3XAHZSJ5914BRQBZ2G4ZBSA): a pause of one repository stops and
//! wakes only the sessions of that repository.

use riff::text;
use riff_client::api::PauseScope;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::record::By;
use riff_core::wire::{Posted, RiffState};

use crate::common;

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The lead of mike, a session of mike with work, and the person mike.
fn sessions() -> (SessionUri, SessionUri, SessionUri) {
    (
        uri("riff://mike@pangolin/como-technologies/riff?session=l1"),
        uri("riff://mike@pangolin/como-technologies/riff?session=w1#issue-12"),
        uri("riff://mike@pangolin/como-technologies/riff"),
    )
}

#[tokio::test]
async fn a_new_riff_is_paused_until_the_lead_resumes_it() {
    let server = common::start_signed().await;
    let (lead, worker, _) = sessions();
    common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    common::api_of(&server, &worker)
        .register(&worker)
        .await
        .unwrap();
    common::name_lead(&common::api_of(&server, &common::person_of(&lead)), &lead).await;

    assert_eq!(
        common::api_of(&server, &worker)
            .riff(&worker)
            .await
            .unwrap(),
        RiffState::Paused
    );
    let error = common::api_of(&server, &worker)
        .claim(&worker, &repo(), "issue-12")
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("the riff is paused"));
    let error = common::api_of(&server, &worker)
        .set_riff(&worker, RiffState::Running)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("403"), "{error:#}");

    let (reply, posted) = common::api_of(&server, &lead)
        .set_riff(&lead, RiffState::Running)
        .await
        .unwrap();
    assert!(reply.changed);
    let woken: Vec<_> = posted.iter().flat_map(|p| &p.woken).collect();
    assert_eq!(woken.len(), 1);
    assert_eq!(woken[0].who(), worker.who());
    // The waiting session reads the news, then claims a free item.
    let news = common::api_of(&server, &worker)
        .read(&worker, &repo(), false)
        .await
        .unwrap();
    assert!(news[0].message.body.contains("running again"), "{news:?}");
    assert!(
        common::api_of(&server, &worker)
            .claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );
}

#[tokio::test]
async fn a_pause_wakes_each_session_and_keeps_its_claims() {
    let server = common::start_signed().await;
    let (lead, worker, mike) = sessions();
    let other = uri("riff://brett@heron/other/repo?session=b1");
    for me in [&lead, &worker, &other] {
        common::api_of(&server, me).register(me).await.unwrap();
    }
    common::name_lead(&common::api_of(&server, &common::person_of(&lead)), &lead).await;
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    assert!(
        common::api_of(&server, &worker)
            .claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );

    let (reply, posted) = common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed);
    let mut woken: Vec<_> = posted
        .iter()
        .flat_map(|p| &p.woken)
        .map(|uri| uri.who().session().unwrap().to_owned())
        .collect();
    woken.sort();
    assert_eq!(woken, ["b1", "l1", "w1"], "each repository gets the news");

    let who = common::api_of(&server, &lead)
        .who(&lead, false)
        .await
        .unwrap();
    let held = who.iter().find(|s| s.uri.who() == worker.who()).unwrap();
    assert_eq!(held.uri.claims(), ["issue-12"], "the claim stays");
    assert!(
        common::api_of(&server, &lead)
            .claim(&lead, &repo(), "issue-13")
            .await
            .is_err()
    );
    // Messages still flow while the riff is paused.
    common::api_of(&server, &worker)
        .tell(&worker, "lead", "paused at: tests of issue-12")
        .await
        .unwrap();

    let (again, posted) = common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Paused)
        .await
        .unwrap();
    assert!(!again.changed);
    assert!(posted.is_empty(), "no change wakes nobody");
    common::api_of(&server, &worker)
        .release(&worker, &repo(), "issue-12")
        .await
        .unwrap();
}

fn strata() -> ThreadName {
    "como-technologies/strata".parse().unwrap()
}

/// The lead of brett in the strata repository, and a session of brett
/// with work there.
fn strata_sessions() -> (SessionUri, SessionUri) {
    (
        uri("riff://brett@kadomony/como-technologies/strata?session=b1"),
        uri("riff://brett@kadomony/como-technologies/strata?session=b2#issue-7"),
    )
}

/// The session ID of each agent session that `posted` woke, in order.
/// The person of a user has no session ID: it is not in the list.
fn woken(posted: &[Posted]) -> Vec<String> {
    let mut woken: Vec<_> = posted
        .iter()
        .flat_map(|p| &p.woken)
        .filter_map(|uri| uri.who().session().map(str::to_owned))
        .collect();
    woken.sort();
    woken
}

/// Two repositories in one riff. The lead of strata pauses its
/// repository. Only the sessions of strata stop and wake
/// (01M3XAHZBGSSJB3YX23K88W01K, 01M3XAHZSJ5914BRQBZ2G4ZBSA): a claim in
/// the other repository works.
#[tokio::test]
async fn a_pause_of_one_repository_stops_and_wakes_only_its_sessions() {
    let server = common::start_signed().await;
    let (lead, worker, mike) = sessions();
    let (strata_lead, strata_worker) = strata_sessions();
    for me in [&lead, &worker, &strata_lead, &strata_worker] {
        common::api_of(&server, me).register(me).await.unwrap();
    }
    for me in [&lead, &strata_lead] {
        common::name_lead(&common::api_of(&server, &common::person_of(me)), me).await;
    }
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    for me in [&worker, &strata_worker] {
        common::api_of(&server, me)
            .read(me, &me.default_thread().unwrap(), false)
            .await
            .unwrap();
    }

    let (reply, posted) = common::api_of(&server, &strata_lead)
        .set_pause(&strata_lead, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed);
    assert_eq!(reply.state, RiffState::Paused, "paused for the caller");
    assert!(reply.riff.is_none(), "the whole riff runs");
    let by = reply.repository(&strata()).unwrap().by.clone();
    assert_eq!(by, Some(By::Session(strata_lead.who().clone())));
    assert_eq!(reply.repositories.len(), 1);
    assert_eq!(woken(&posted), ["b2"], "only the sessions of strata wake");
    assert_eq!(posted[0].thread, strata());

    // The other repository goes on, and no news comes to it.
    assert_eq!(
        common::api_of(&server, &worker)
            .riff(&worker)
            .await
            .unwrap(),
        RiffState::Running
    );
    assert!(
        common::api_of(&server, &worker)
            .read(&worker, &repo(), false)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        common::api_of(&server, &worker)
            .claim(&worker, &repo(), "issue-12")
            .await
            .unwrap()
            .granted
    );
    // It sees the pause of strata, and who set it.
    let seen = common::api_of(&server, &worker)
        .pauses(&worker)
        .await
        .unwrap();
    assert_eq!(
        text::riff_state(&seen, Some(&repo())),
        "The riff is running.\n\
         The repository como-technologies/strata is paused by the session brett/b1."
    );

    // The sessions of strata are paused, and the refusal says by which
    // pause, and who ends it.
    assert_eq!(
        common::api_of(&server, &strata_worker)
            .riff(&strata_worker)
            .await
            .unwrap(),
        RiffState::Paused
    );
    let error = common::api_of(&server, &strata_worker)
        .claim(&strata_worker, &strata(), "issue-7")
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains(
            "the repository como-technologies/strata is paused by the session brett/b1, so \
             nobody claims issue-7. Wait until your user or the lead resumes it."
        ),
        "{error}"
    );
    let news = common::api_of(&server, &strata_worker)
        .read(&strata_worker, &strata(), false)
        .await
        .unwrap();
    assert_eq!(
        news[0].message.body,
        riff_client::text::riff_news(Some(&strata()), RiffState::Paused)
    );

    // The resume wakes the same sessions, and the claim works.
    let (reply, posted) = common::api_of(&server, &strata_lead)
        .set_pause(&strata_lead, &PauseScope::Here, RiffState::Running)
        .await
        .unwrap();
    assert!(reply.changed && reply.repositories.is_empty());
    assert_eq!(reply.state, RiffState::Running);
    assert_eq!(woken(&posted), ["b2"]);
    assert!(
        common::api_of(&server, &strata_worker)
            .claim(&strata_worker, &strata(), "issue-7")
            .await
            .unwrap()
            .granted
    );
}

/// A session that is not the lead cannot pause its repository, and a
/// person can (01M3XAHZDSQR263QZVB41CK0MX).
#[tokio::test]
async fn a_worker_cannot_pause_its_repository_and_a_person_can() {
    let server = common::start_signed().await;
    let (lead, worker, mike) = sessions();
    common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    common::api_of(&server, &worker)
        .register(&worker)
        .await
        .unwrap();
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();

    let error = common::api_of(&server, &worker)
        .set_pause(&worker, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("403"), "{error:#}");
    assert_eq!(
        common::api_of(&server, &worker)
            .riff(&worker)
            .await
            .unwrap(),
        RiffState::Running
    );

    let (reply, posted) = common::api_of(&server, &mike)
        .set_pause(&mike, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed);
    assert_eq!(woken(&posted), ["l1", "w1"]);
    let by = reply.repository(&repo()).unwrap().by.clone();
    assert_eq!(by, Some(By::Person("mike".into())));
}

/// The whole riff is paused, and the repository too. A resume of the
/// repository changes only its pause: the riff still stops its
/// sessions, so nobody wakes, and the answer says that the riff is
/// still paused (01M3XAHZSJ5914BRQBZ2G4ZBSA).
#[tokio::test]
async fn a_resume_of_a_repository_in_a_paused_riff_wakes_nobody() {
    let server = common::start_signed().await;
    let (lead, worker, mike) = sessions();
    let (strata_lead, strata_worker) = strata_sessions();
    for me in [&lead, &worker, &strata_lead, &strata_worker] {
        common::api_of(&server, me).register(me).await.unwrap();
    }
    for me in [&lead, &strata_lead] {
        common::name_lead(&common::api_of(&server, &common::person_of(me)), me).await;
    }
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    let (_, posted) = common::api_of(&server, &lead)
        .set_pause(&lead, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap();
    assert_eq!(woken(&posted), ["w1"]);

    // The pause of the riff wakes only the sessions that it stops: the
    // repository of mike is paused already.
    let (reply, posted) = common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed && reply.riff.is_some());
    assert_eq!(woken(&posted), ["b1", "b2"]);

    let (reply, posted) = common::api_of(&server, &lead)
        .set_pause(&lead, &PauseScope::Here, RiffState::Running)
        .await
        .unwrap();
    assert!(reply.changed, "the pause of the repository ended");
    assert_eq!(reply.state, RiffState::Paused, "the riff still stops it");
    assert!(reply.riff.is_some() && reply.repositories.is_empty());
    assert!(posted.is_empty(), "nobody wakes: {posted:?}");
    assert_eq!(
        text::riff_set(Some(&repo()), RiffState::Running, &reply, &posted),
        "The repository como-technologies/riff is running now. No other session woke. The \
         whole riff is still paused: the owner or an admin resumes it with `riff resume --riff`."
    );
    let error = common::api_of(&server, &worker)
        .claim(&worker, &repo(), "issue-12")
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("the riff is paused by the person mike"),
        "{error}"
    );

    // The resume of the riff wakes each session: no repository has a
    // pause of its own now.
    let (_, posted) = common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    assert_eq!(woken(&posted), ["b1", "b2", "l1", "w1"]);
}

/// A resume of the whole riff does not wake the sessions of a
/// repository that has a pause of its own, and the answer names it.
#[tokio::test]
async fn a_resume_of_the_riff_leaves_a_paused_repository_paused() {
    let server = common::start_signed().await;
    let (lead, worker, mike) = sessions();
    let (strata_lead, strata_worker) = strata_sessions();
    for me in [&lead, &worker, &strata_lead, &strata_worker] {
        common::api_of(&server, me).register(me).await.unwrap();
    }
    for me in [&lead, &strata_lead] {
        common::name_lead(&common::api_of(&server, &common::person_of(me)), me).await;
    }
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    common::api_of(&server, &strata_lead)
        .set_pause(&strata_lead, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap();
    common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Paused)
        .await
        .unwrap();

    let (reply, posted) = common::api_of(&server, &mike)
        .set_riff(&mike, RiffState::Running)
        .await
        .unwrap();
    assert_eq!(woken(&posted), ["l1", "w1"], "strata stays paused");
    let answer = text::riff_set(None, RiffState::Running, &reply, &posted);
    assert!(
        answer.ends_with(
            "Still paused: como-technologies/strata. Its user or its lead resumes it with \
             `riff resume`."
        ),
        "{answer}"
    );
    assert_eq!(
        common::api_of(&server, &strata_worker)
            .riff(&strata_worker)
            .await
            .unwrap(),
        RiffState::Paused
    );
    assert_eq!(
        common::api_of(&server, &worker)
            .riff(&worker)
            .await
            .unwrap(),
        RiffState::Running
    );
}

/// mike has live sessions on hosts `a` and `b`. A resume on `b` gives
/// the person of mike the host `b`. A pause on `a` then posts a note
/// from `mike@a` (01M3MWW8KYJ3ZV91X22RBSAF33).
#[tokio::test]
async fn a_pause_names_the_host_where_it_ran() {
    let server = common::start_signed().await;
    let on_a = uri("riff://mike@a/como-technologies/riff?session=s1");
    let on_b = uri("riff://mike@b/como-technologies/riff?session=s2");
    for me in [&on_a, &on_b] {
        common::api_of(&server, me).register(me).await.unwrap();
    }
    let person_on_b = uri("riff://mike@b/como-technologies/riff");
    common::api_of(&server, &person_on_b)
        .set_riff(&person_on_b, RiffState::Running)
        .await
        .unwrap();

    let person_on_a = uri("riff://mike@a/como-technologies/riff");
    let (reply, _) = common::api_of(&server, &person_on_a)
        .set_riff(&person_on_a, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed);

    let news = common::api_of(&server, &on_b)
        .read(&on_b, &repo(), false)
        .await
        .unwrap();
    let senders: Vec<String> = news.iter().map(|m| m.message.from.short()).collect();
    assert_eq!(senders, ["mike@b:riff", "mike@a:riff"], "{news:?}");
    assert!(news[1].message.body.contains("paused"), "{news:?}");
}
