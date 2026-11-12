//! The rules of the projects of the riff (01M76SG2E9RPV4B7CYSZPGGZ3P,
//! 01M76SG8GBXFYXKWMQHPCNEXBN, 01M76SG8JVHTKY6N62FQG7XAE6,
//! 01M76SG8NCSAP69Y5AX455BJQM): one test for each rule of
//! [`projects`](crate::state::projects), in the form given, when, then.

use riff_core::record::{NotesSet, ProjectEnrolled, ProjectRemoved};

use super::*;

impl Ask for EnrollProject {
    type Command = EnrollProject;
    fn of(self, _me: &SessionUri) -> EnrollProject {
        self
    }
}

impl Ask for RemoveProject {
    type Command = RemoveProject;
    fn of(self, _me: &SessionUri) -> RemoveProject {
        self
    }
}

impl Ask for FindProjects {
    type Command = FindProjects;
    fn of(self, _me: &SessionUri) -> FindProjects {
        self
    }
}

/// The enrollment of `acme/app` with `hosts`, after a check on GitHub
/// that passed.
fn enroll(hosts: &[&str]) -> EnrollProject {
    EnrollProject {
        repo: repo(),
        hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
        github: Ok(()),
    }
}

fn remove() -> RemoveProject {
    RemoveProject {
        repo: repo(),
        github: Ok(()),
    }
}

fn enrolled(hosts: &[&str]) -> Change {
    Change::ProjectEnrolled(ProjectEnrolled {
        repo: repo(),
        hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
    })
}

fn removed() -> Change {
    Change::ProjectRemoved(ProjectRemoved { repo: repo() })
}

/// A person who is an admin of the riff.
fn admin() -> Caller {
    Caller::of(&person()).with_role(Role::Admin)
}

/// An admin who is an admin of the repository on GitHub enrolls it, with
/// the hosts sorted and each one time. The same hosts make no record,
/// and other hosts make a new one.
#[test]
fn an_admin_of_the_riff_and_of_the_repository_enrolls_it_and_sets_its_hosts() {
    given(&team())
        .when_as(&admin(), enroll(&["thelio", "heron", "thelio"]))
        .then(&[enrolled(&["heron", "thelio"])]);
    given(&team())
        .when_as(&admin(), enroll(&[]))
        .then(&[enrolled(&[])]);
    let written = [team(), vec![enrolled(&["heron"])]].concat();
    given(&written)
        .when_as(&admin(), enroll(&["heron"]))
        .then(&[]);
    given(&written)
        .when_as(&admin(), enroll(&[]))
        .then(&[enrolled(&[])]);
    // The owner is an admin too.
    let owner = Caller::of(&person()).with_role(Role::Owner);
    given(&team())
        .when_as(&owner, enroll(&[]))
        .then(&[enrolled(&[])]);
}

/// A member, and a session with each role, cannot enroll or remove a
/// project.
#[test]
fn a_member_and_a_session_cannot_enroll_or_remove_a_project() {
    let member = Caller::of(&person());
    given(&team())
        .when_as(&member, enroll(&[]))
        .then_refused_as(Code::NotAllowed, "ann is not an admin; only an admin enrolls");
    let written = [team(), vec![enrolled(&[])]].concat();
    given(&written)
        .when_as(&member, remove())
        .then_refused_as(Code::NotAllowed, "only an admin removes a project");
    for role in Role::ALL {
        let session = Caller::of(&ann()).with_role(role);
        given(&written)
            .when_as(&session, enroll(&[]))
            .then_refused_as(Code::NotAllowed, "a session cannot send the command");
        given(&written)
            .when_as(&session, remove())
            .then_refused_as(Code::NotAllowed, "a session cannot send the command");
    }
}

/// An admin who is not an admin of the repository on GitHub is refused,
/// with the reason of riff-server.
#[test]
fn an_admin_who_is_not_an_admin_of_the_repository_is_refused() {
    let no = || Err("GitHub says that ann-gh is no admin of acme/app".to_owned());
    let ask = EnrollProject {
        github: no(),
        ..enroll(&[])
    };
    given(&team())
        .when_as(&admin(), ask)
        .then_refused_as(Code::NotAllowed, "only an admin of acme/app on GitHub");
    let written = [team(), vec![enrolled(&[])]].concat();
    let ask = RemoveProject {
        github: no(),
        ..remove()
    };
    given(&written)
        .when_as(&admin(), ask)
        .then_refused_as(Code::NotAllowed, "ann-gh is no admin of acme/app");
}

/// A host must be the host part of a session URI, and the repository a
/// repository thread.
#[test]
fn a_bad_host_or_a_thread_that_is_no_repository_is_refused() {
    given(&team())
        .when_as(&admin(), enroll(&["the lio"]))
        .then_refused_as(Code::BadRequest, "the host part has a character");
    given(&team())
        .when_as(&admin(), enroll(&[""]))
        .then_refused_as(Code::BadRequest, "the host part is empty");
    let ask = EnrollProject {
        repo: design(),
        ..enroll(&[])
    };
    given(&team())
        .when_as(&admin(), ask)
        .then_refused_as(Code::BadRequest, "design is not a repository thread");
}

/// An admin removes a project. A repository that is no project is
/// refused, and names `riff project list`.
#[test]
fn an_admin_removes_a_project_and_a_repository_that_is_no_project_is_refused() {
    let written = [team(), vec![enrolled(&["heron"])]].concat();
    given(&written).when_as(&admin(), remove()).then(&[removed()]);
    given(&team())
        .when_as(&admin(), remove())
        .then_refused_as(Code::BadRequest, "riff project list shows the projects");
    let gone = [written, vec![removed()]].concat();
    given(&gone)
        .when_as(&admin(), remove())
        .then_refused_as(Code::BadRequest, "acme/app is no project of the riff");
}

/// A project keeps who enrolled it and when. A later enrollment sets its
/// hosts only. The query gives each project with the user of each lead
/// that holds, and the reply of a removal is the project that went.
#[test]
fn a_project_keeps_who_enrolled_it_and_when_and_the_query_names_its_leads() {
    let Given { mut state, now } = given(&team()).live(&[ann(), bob()]);
    let mike = Caller::of(&"riff://mike@pangolin".parse().unwrap()).with_role(Role::Admin);
    let (made, note) = written(&mut state, &mike, &enroll(&["thelio"]), now);
    assert_eq!(note, Enrolled::New);
    let at_ms = made[0].envelope.written_at_ms;
    let (_, note) = written(&mut state, &admin(), &enroll(&["heron"]), now);
    assert_eq!(note, Enrolled::Hosts);
    let (none, note) = written(&mut state, &admin(), &enroll(&["heron"]), now);
    assert!(none.is_empty());
    assert_eq!(note, Enrolled::Same);
    let projects = state.projects(now).projects;
    assert_eq!(projects.len(), 1);
    let project = &projects[0];
    assert_eq!(project.repo, repo());
    assert_eq!(project.by, Some(By::Person("mike".into())));
    assert_eq!(project.at_ms, at_ms);
    assert_eq!(project.hosts, ["heron"]);
    assert_eq!(project.leads, ["ann", "bob"]);
    assert!(state.projects_known());
    // The removal gives the project that went.
    let (_, went) = written(&mut state, &admin(), &remove(), now);
    assert_eq!(went.as_ref().map(|p| p.hosts.clone()), Some(vec!["heron".into()]));
    assert!(state.projects(now).projects.is_empty());
    // The mark stays after the last project went.
    assert!(state.projects_known());
}

/// With no record of a project, the server enrolls each repository
/// thread with a lead or the notes of a lead, for each host. An empty
/// text is no notes.
#[test]
fn the_server_enrolls_each_repository_with_a_lead_or_notes() {
    let notes = |thread: &str, text: &str| {
        Change::NotesSet(NotesSet {
            thread: thread.parse().unwrap(),
            user: "carol".into(),
            text: text.into(),
        })
    };
    let records = [
        team(),
        vec![notes("acme/web", "Wave 3 runs."), notes("acme/old", "")],
    ]
    .concat();
    let web = Change::ProjectEnrolled(ProjectEnrolled {
        repo: "acme/web".parse().unwrap(),
        hosts: Vec::new(),
    });
    given(&records)
        .when_as(&Caller::server(), FindProjects)
        .then(&[enrolled(&[]), web]);
    // No lead and no notes: no record.
    given(&[]).when_as(&Caller::server(), FindProjects).then(&[]);
}

/// A log with a record of a project gets no search, also after the last
/// project went.
#[test]
fn a_log_with_a_record_of_a_project_gets_no_search() {
    let enrolled_once = [team(), vec![enrolled(&[])]].concat();
    given(&enrolled_once)
        .when_as(&Caller::server(), FindProjects)
        .then(&[]);
    let all_gone = [enrolled_once, vec![removed()]].concat();
    given(&all_gone)
        .when_as(&Caller::server(), FindProjects)
        .then(&[]);
}

/// Only the server looks for the projects.
#[test]
fn only_the_server_looks_for_the_projects() {
    for caller in [admin(), Caller::of(&ann()).with_role(Role::Owner)] {
        given(&team())
            .when_as(&caller, FindProjects)
            .then_refused_as(Code::NotAllowed, "cannot send the command find_projects");
    }
}

/// The checkpoint keeps the projects and the mark.
#[test]
fn the_checkpoint_keeps_the_projects_and_the_mark() {
    let Given { mut state, now } = given(&team()).live(&[ann()]);
    written(&mut state, &admin(), &enroll(&["heron"]), now);
    let snapshot = state.snapshot(now, 0);
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["projects"][0]["repo"], "acme/app");
    assert_eq!(json["projects"][0]["hosts"][0], "heron");
    assert_eq!(json["projects_known"], true);
    let back: Snapshot = serde_json::from_value(json).unwrap();
    assert_eq!(back, snapshot);
    // A checkpoint from before the projects has none, and no mark.
    let empty = serde_json::to_value(given(&team()).state.snapshot(now, 0)).unwrap();
    assert!(empty.get("projects").is_none() && empty.get("projects_known").is_none());
}

/// Runs `command` of `caller` on `state`, and writes its records to the
/// written copy, as the writer does.
fn written<C: Command>(
    state: &mut State,
    caller: &Caller,
    command: &C,
    now: Instant,
) -> (Vec<Record>, C::Note) {
    let (made, note) = state.run(caller, command, now).unwrap();
    state.written(&made);
    (made, note)
}
