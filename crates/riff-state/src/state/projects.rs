//! The group "projects": the commands [`EnrollProject`],
//! [`RemoveProject`] and [`FindProjects`], and the projects of the riff
//! (01M76SG2E9RPV4B7CYSZPGGZ3P).
//!
//! A project is a repository `OWNER/REPO` on GitHub that a person
//! enrolled. It keeps who enrolled it and when, from the envelope of its
//! first `project_enrolled` record, and the hosts that may run its wave:
//! an empty list is each host. A later `project_enrolled` sets the hosts
//! only. A `project_removed` forgets the project. The checkpoint keeps
//! the projects, and the mark that the log has a record of a project.
//!
//! | Who | Enrolls, sets the hosts, removes (01M76SG8GBXFYXKWMQHPCNEXBN) | Reads (01M76SG8JVHTKY6N62FQG7XAE6) |
//! |---|---|---|
//! | a person who is an admin of the riff and an admin of the repository on GitHub | yes | yes |
//! | an admin who is not an admin of the repository | no | yes |
//! | a member, and each session | no | yes |
//! | the server, at a start with no record of a project (01M76SG8NCSAP69Y5AX455BJQM) | enrolls each repository with a lead or notes | no |
//!
//! The state does no I/O. So riff-server asks GitHub first, with the
//! token of `gh` of the person, and gives the answer to the command in
//! [`OnGitHub`]. [`permits`](super::permits) refuses a session and a
//! member before `handle` reads the answer.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff project init"] --> S["riff-server: GET /repos/OWNER/REPO<br/>with the token of the person"]
//!     S --> P{"permits:<br/>a person, an admin"}
//!     P -->|no| N[not_allowed]
//!     P -->|yes| G{"admin of the<br/>repository?"}
//!     G -->|no| N
//!     G -->|yes| H{"same hosts?"}
//!     H -->|yes| X[no record]
//!     H -->|no| R[(project_enrolled)]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use riff_core::name::ThreadName;
use riff_core::record::{By, Change, Envelope, ProjectEnrolled, ProjectRemoved, Record};
use riff_core::wire::{ProjectFacts, ProjectInitReply, ProjectsReply};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::plan::repository;
use super::view::View;

/// The projects of the riff, by their repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projects {
    projects: BTreeMap<ThreadName, Project>,
    /// True after the first record of a project: the server then looks
    /// for no project at its start (01M76SG8NCSAP69Y5AX455BJQM).
    known: bool,
}

/// One project: who enrolled it and when, and its hosts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct Project {
    /// The caller of the first `project_enrolled`. `None` when the
    /// record has no cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    by: Option<By>,
    /// The time of that record.
    at_ms: u64,
    /// The hosts that may run the wave, sorted. Empty: each host may.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hosts: Vec<String>,
}

impl Projects {
    /// True when the log has a record of a project.
    pub fn known(&self) -> bool {
        self.known
    }

    /// True when `repo` is a project of the riff.
    pub fn has(&self, repo: &ThreadName) -> bool {
        self.projects.contains_key(repo)
    }

    /// The hosts that may run the wave of the project `repo`, sorted:
    /// empty for each host. `None` when `repo` is no project.
    pub fn hosts(&self, repo: &ThreadName) -> Option<&[String]> {
        self.projects.get(repo).map(|p| p.hosts.as_slice())
    }

    /// The record enrolls its repository, or sets the hosts of the
    /// project. A project keeps the `by` and the time of its first
    /// record.
    pub(super) fn enrolled(
        &mut self,
        enrolled: &ProjectEnrolled,
        record: &Record,
    ) -> Result<(), &'static str> {
        let ProjectEnrolled { repo, hosts } = enrolled;
        let Envelope {
            position: _,
            written_at_ms,
            by,
            command: _,
            call: _,
        } = &record.envelope;
        self.known = true;
        self.projects
            .entry(repo.clone())
            .and_modify(|project| project.hosts.clone_from(hosts))
            .or_insert_with(|| Project {
                by: by.clone(),
                at_ms: *written_at_ms,
                hosts: hosts.clone(),
            });
        Ok(())
    }

    /// The record forgets its project.
    pub(super) fn removed(&mut self, removed: &ProjectRemoved) -> Result<(), &'static str> {
        let ProjectRemoved { repo } = removed;
        self.known = true;
        match self.projects.remove(repo) {
            Some(_) => Ok(()),
            None => Err("the repository is no project"),
        }
    }

    /// The projects, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        let Projects { projects, known } = self;
        Saved {
            projects: projects
                .iter()
                .map(|(repo, project)| SavedProject {
                    repo: repo.clone(),
                    project: project.clone(),
                })
                .collect(),
            projects_known: *known,
        }
    }
}

/// The part of the checkpoint of this group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projects: Vec<SavedProject>,
    /// True when the log has a record of a project.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    projects_known: bool,
}

/// A project: the map entry of [`Projects`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedProject {
    repo: ThreadName,
    #[serde(flatten)]
    project: Project,
}

impl Saved {
    pub(super) fn restore(self) -> Projects {
        let Saved {
            projects,
            projects_known,
        } = self;
        Projects {
            projects: projects
                .into_iter()
                .map(|SavedProject { repo, project }| (repo, project))
                .collect(),
            known: projects_known,
        }
    }
}

/// The answer of GitHub to the check of riff-server before a command of
/// a project (01M76SG8GBXFYXKWMQHPCNEXBN): `Ok` when the person is an
/// admin of the repository, else why not.
pub type OnGitHub = Result<(), String>;

/// Enrolls the repository `repo` as a project of the riff, or sets the
/// hosts of the project (01M76SG8GBXFYXKWMQHPCNEXBN). riff-server sends
/// it after its check on GitHub. The same hosts make no record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnrollProject {
    pub repo: ThreadName,
    /// The hosts that may run the wave. Empty: each host may.
    pub hosts: Vec<String>,
    pub github: OnGitHub,
}

/// What an [`EnrollProject`] changed, for its reply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Enrolled {
    /// The project had these hosts: no record.
    #[default]
    Same,
    /// The repository was no project.
    New,
    /// The project had other hosts.
    Hosts,
}

impl Command for EnrollProject {
    const KIND: CommandKind = CommandKind::EnrollProject;
    type Reply = ProjectInitReply;
    type Note = Enrolled;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        _: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, Enrolled), Refused> {
        let EnrollProject {
            repo,
            hosts,
            github,
        } = self;
        repository(repo)?;
        let hosts = checked_hosts(hosts)?;
        admin_on_github(repo, github)?;
        let made = match view.riff.projects().hosts(repo) {
            Some(now) if now == hosts.as_slice() => return Ok((Vec::new(), Enrolled::Same)),
            Some(_) => Enrolled::Hosts,
            None => Enrolled::New,
        };
        let repo = repo.clone();
        Ok((vec![Change::ProjectEnrolled(ProjectEnrolled { repo, hosts })], made))
    }

    fn reply(
        &self,
        _: &Caller,
        view: &View<'_>,
        _: &Done,
        made: Enrolled,
        now: Now,
    ) -> ProjectInitReply {
        let project = view.project(&self.repo, now.at).unwrap_or_else(|| ProjectFacts {
            repo: self.repo.clone(),
            by: None,
            at_ms: 0,
            hosts: Vec::new(),
            leads: Vec::new(),
        });
        ProjectInitReply {
            project,
            enrolled: made == Enrolled::New,
            hosts_changed: made == Enrolled::Hosts,
        }
    }
}

/// The riff forgets the project `repo` (01M76SG8GBXFYXKWMQHPCNEXBN).
/// GitHub keeps the repository. riff-server sends it after its check on
/// GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveProject {
    pub repo: ThreadName,
    pub github: OnGitHub,
}

impl Command for RemoveProject {
    const KIND: CommandKind = CommandKind::RemoveProject;
    type Reply = ProjectFacts;
    /// The project before the removal: the written copy has it no more.
    type Note = Option<ProjectFacts>;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        _: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Option<ProjectFacts>), Refused> {
        let RemoveProject { repo, github } = self;
        repository(repo)?;
        admin_on_github(repo, github)?;
        let Some(project) = view.project(repo, now.at) else {
            let reason = format!(
                "{repo} is no project of the riff. riff project list shows the projects."
            );
            return Err(Refused::new(Code::BadRequest, reason));
        };
        let repo = repo.clone();
        Ok((
            vec![Change::ProjectRemoved(ProjectRemoved { repo })],
            Some(project),
        ))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        project: Option<ProjectFacts>,
        _: Now,
    ) -> ProjectFacts {
        project.unwrap_or_else(|| ProjectFacts {
            repo: self.repo.clone(),
            by: None,
            at_ms: 0,
            hosts: Vec::new(),
            leads: Vec::new(),
        })
    }
}

/// The search of the projects at a start of the server
/// (01M76SG8NCSAP69Y5AX455BJQM): with no record of a project in the log,
/// it enrolls each repository thread that has a lead or the notes of a
/// lead, for each host. A log with a record of a project gets no record.
/// The reply is each repository that it enrolled.
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_state::state::{Caller, FindProjects, State};
///
/// let mike: SessionUri = "riff://mike@pangolin/acme/app?session=a1".parse()?;
/// let now = Instant::now();
/// let mut state = State::default();
/// state.register(&mike, now);
/// state.lead(&mike, now).unwrap();
/// let (made, ()) = state.run(&Caller::server(), &FindProjects, now).unwrap();
/// assert_eq!(made.len(), 1);
/// let projects = state.projects(now).projects;
/// assert_eq!(projects[0].repo.to_string(), "acme/app");
/// assert_eq!(projects[0].leads, ["mike"]);
/// // The search runs one time.
/// assert!(state.run(&Caller::server(), &FindProjects, now).unwrap().0.is_empty());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindProjects;

impl Command for FindProjects {
    const KIND: CommandKind = CommandKind::FindProjects;
    type Reply = Vec<ThreadName>;
    type Note = ();

    fn handle(&self, _: &Caller, view: &View<'_>, _: Now) -> Result<(Vec<Change>, ()), Refused> {
        let riff = view.riff;
        if riff.projects().known() {
            return Ok((Vec::new(), ()));
        }
        let leads = riff.work().leads.keys().map(|(_, thread)| thread);
        let found: BTreeSet<&ThreadName> = leads
            .chain(riff.plans().noted())
            .filter(|thread| repository(thread).is_ok())
            .collect();
        let changes = found
            .into_iter()
            .map(|repo| {
                Change::ProjectEnrolled(ProjectEnrolled {
                    repo: repo.clone(),
                    hosts: Vec::new(),
                })
            })
            .collect();
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> Vec<ThreadName> {
        done.made
            .iter()
            .filter_map(|record| match &record.change {
                Change::ProjectEnrolled(enrolled) => Some(enrolled.repo.clone()),
                _ => None,
            })
            .collect()
    }
}

/// The hosts of `hosts`, sorted and each one time. Each is the host part
/// of a session URI: not empty, and only ASCII letters, digits, `-`,
/// `_`, `.` and `~`.
fn checked_hosts(hosts: &[String]) -> Result<Vec<String>, Refused> {
    let mut checked = BTreeSet::new();
    for host in hosts {
        riff_core::name::check("host", host).map_err(|e| e.to_string())?;
        checked.insert(host.clone());
    }
    Ok(checked.into_iter().collect())
}

/// The refusal when GitHub said that the person is no admin of `repo`,
/// or riff-server could not ask.
fn admin_on_github(repo: &ThreadName, github: &OnGitHub) -> Result<(), Refused> {
    github.as_ref().map_err(|why| {
        let reason = format!(
            "only an admin of {repo} on GitHub enrolls or removes the project {repo}: {why}"
        );
        Refused::new(Code::NotAllowed, reason)
    })?;
    Ok(())
}

impl View<'_> {
    /// The project `repo` with the user of each lead of its thread now,
    /// or `None` when `repo` is no project.
    pub(super) fn project(&self, repo: &ThreadName, now: Instant) -> Option<ProjectFacts> {
        let project = self.riff.projects().projects.get(repo)?;
        let leads: BTreeSet<String> = self
            .riff
            .work()
            .leads
            .keys()
            .filter(|key| &key.1 == repo)
            .filter(|key| self.lead_of(key, now).is_some())
            .map(|(user, _)| user.clone())
            .collect();
        Some(ProjectFacts {
            repo: repo.clone(),
            by: project.by.clone(),
            at_ms: project.at_ms,
            hosts: project.hosts.clone(),
            leads: leads.into_iter().collect(),
        })
    }

    /// Each project of the riff, in the order of the repository
    /// (01M76SG8JVHTKY6N62FQG7XAE6).
    pub(super) fn projects(&self, now: Instant) -> ProjectsReply {
        let projects = self
            .riff
            .projects()
            .projects
            .keys()
            .filter_map(|repo| self.project(repo, now))
            .collect();
        ProjectsReply { projects }
    }
}
