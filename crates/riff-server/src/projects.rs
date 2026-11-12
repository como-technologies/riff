//! The projects of the riff on riff-server (01M76SG2E9RPV4B7CYSZPGGZ3P):
//! the enrollment and the removal of a project after a check on GitHub,
//! the list, and the search at a start.
//!
//! # Design
//!
//! The state does no I/O, so the check on GitHub comes first, in the
//! handler (01M76SG8GBXFYXKWMQHPCNEXBN):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff project init
//!     participant S as riff-server
//!     participant G as GitHub
//!     R->>S: POST /v1/project/init: me, repo, hosts, the token of gh
//!     S->>S: admit the caller of me
//!     S->>G: GET /repos/OWNER/REPO with the token (only for a person who is an admin)
//!     G-->>S: permissions.admin
//!     S->>S: the command enroll_project with the answer of GitHub
//!     S-->>R: the project, enrolled or with its new hosts
//! ```
//!
//! - riff-server asks GitHub only for a person who is an admin of the
//!   riff: `permits` refuses each other caller first, and GitHub gets no
//!   call for it. The token goes in that one call, and in no record, no
//!   log line and no reply.
//! - A riff with no sign-in takes the call with no token: each person
//!   there is an admin of the riff, and the check on GitHub stays.
//! - The list (`list`) is a query: each person and each session reads
//!   it (01M76SG8JVHTKY6N62FQG7XAE6).
//! - At a start with no record of a project in the log, and with a
//!   repository thread that has a lead or the notes of a lead, the
//!   server sends the command `find_projects`, and posts one line of the
//!   kind `riff` in the chat to each admin (`find`,
//!   01M76SG8NCSAP69Y5AX455BJQM, 01M77FVW07HC05Q69HV6971F20).

use std::time::Instant;

use axum::Json;
use axum::extract::State as AxumState;
use axum::response::{IntoResponse, Response};
use riff_core::name::ThreadName;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, ProjectInit, ProjectRemove, ProjectsQuery, ProjectsReply};
use riff_state::state::{EnrollProject, OnGitHub, RemoveProject, Role};

use crate::engine::{Admitted, Failed};
use crate::{Proof, Reply, Server, Shared, admit};

/// `POST /v1/project/init`: enrolls the repository, or sets the hosts of
/// the project, after the check on GitHub (01M76SG8GBXFYXKWMQHPCNEXBN).
pub(crate) async fn init(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ProjectInit>,
) -> Result<Response, Failed> {
    let admitted = admit(&s, &proof, &r.me)?;
    let ProjectInit {
        me: _,
        repo,
        hosts,
        github_token,
    } = r;
    let github = on_github(&s, &admitted, &repo, &github_token).await;
    let enroll = EnrollProject {
        repo,
        hosts,
        github,
    };
    let reply = s.engine.enroll_project(admitted, enroll).await?;
    Ok(Json(reply).into_response())
}

/// `POST /v1/project/remove`: the riff forgets the project, after the
/// check on GitHub (01M76SG8GBXFYXKWMQHPCNEXBN). GitHub keeps the
/// repository.
pub(crate) async fn remove(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ProjectRemove>,
) -> Result<Response, Failed> {
    let admitted = admit(&s, &proof, &r.me)?;
    let ProjectRemove {
        me: _,
        repo,
        github_token,
    } = r;
    let github = on_github(&s, &admitted, &repo, &github_token).await;
    let reply = s
        .engine
        .remove_project(admitted, RemoveProject { repo, github })
        .await?;
    Ok(Json(reply).into_response())
}

/// `POST /v1/projects`: each project of the riff, for each person and
/// each session (01M76SG8JVHTKY6N62FQG7XAE6).
pub(crate) async fn list(
    AxumState(s): AxumState<Shared>,
    proof: Proof,
    Json(r): Json<ProjectsQuery>,
) -> Reply<ProjectsReply> {
    let caller = admit(&s, &proof, &r.me)?;
    let reply = s
        .engine
        .query(&caller, |state| state.projects(Instant::now()))
        .await?;
    Ok(Json(reply))
}

/// The answer of GitHub for the call of `admitted` on `repo`. Only a
/// person who is an admin of the riff gets a call to GitHub: the check
/// of the command refuses each other caller before it reads the answer.
async fn on_github(s: &Server, admitted: &Admitted, repo: &ThreadName, token: &str) -> OnGitHub {
    let person = admitted.who().session().is_none();
    if !person || s.engine.role(admitted) < Role::Admin {
        return Err("riff-server did not ask GitHub".into());
    }
    let repo = repo.to_string();
    if !crate::github::valid_repo(&repo) {
        return Err(format!("{repo:?} is no repository OWNER/NAME of GitHub"));
    }
    let checked = s.github.admin_of(&repo, token).await;
    match &checked {
        Ok(()) => tracing::info!(
            repo,
            "GitHub says that the person is an admin of the repository"
        ),
        Err(why) => tracing::info!(repo, "the check of a project on GitHub: {why}"),
    }
    checked
}

/// The search of the projects at a start (01M76SG8NCSAP69Y5AX455BJQM):
/// the command `find_projects`, then one line of the kind `riff` in the
/// chat to each admin, with each repository that it enrolled. A riff with
/// no admin that has a USER gets the line with no selector.
pub(crate) async fn find(s: &Server) {
    let found = match s.engine.find_projects().await {
        Ok(found) => found,
        Err(failed) => {
            tracing::warn!("the search of the projects did not go: {}", failed.text());
            return;
        }
    };
    if found.is_empty() {
        return;
    }
    tracing::info!(
        projects = found.len(),
        "enrolled each repository with a lead or the notes of a lead as a project"
    );
    let admins = s.engine.read(|state| state.people().admin_users());
    let to = admins
        .into_iter()
        .map(|user| Selector {
            user: Some(user),
            ..Selector::default()
        })
        .collect();
    let body = found_line(&found);
    let post = Server::news(Some(ThreadName::chat()), to, &body, Kind::Riff);
    if let Err(failed) = s.engine.announce(post).await {
        tracing::warn!("the line of the projects did not go: {}", failed.text());
    }
}

/// The line to each admin after the search of the projects
/// (01M76SG8NCSAP69Y5AX455BJQM).
///
/// ```
/// let repos = ["acme/app".parse()?, "acme/web".parse()?];
/// let line = riff_server::projects::found_line(&repos);
/// assert!(line.starts_with("riff enrolled 2 repositories with a lead or the notes of a lead as projects: acme/app, acme/web."), "{line}");
/// assert!(line.contains("riff project list"), "{line}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn found_line(found: &[ThreadName]) -> String {
    let names: Vec<String> = found.iter().map(ToString::to_string).collect();
    let count = match found.len() {
        1 => "1 repository".to_owned(),
        n => format!("{n} repositories"),
    };
    format!(
        "riff enrolled {count} with a lead or the notes of a lead as projects: {}. Review the \
         list with riff project list, and remove a project with riff project remove OWNER/REPO.",
        names.join(", ")
    )
}
