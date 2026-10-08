//! `riff forge create`: an admin makes the GitHub App of riff with one
//! command, by the GitHub App manifest flow (#627).
//!
//! # Design
//!
//! One App of riff serves each account. It is public: an organization
//! or a person installs it with `riff forge install OWNER`
//! ([`Forge::install`]). GitHub gives the private key to the server,
//! never to a machine (01M4CTAYRSC27Q6AAGTH9CBD7Q).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff forge create
//!     participant B as browser
//!     participant S as riff-server
//!     participant G as GitHub
//!     R->>S: POST /v1/forge/create (an admin)
//!     S-->>R: the start page, with a new state (10 minutes)
//!     R->>B: open the start page
//!     B->>S: GET /forge/new?state
//!     S-->>B: a form with the manifest
//!     B->>G: POST the manifest to the settings of the org
//!     Note over B,G: the admin clicks "Create GitHub App"
//!     G-->>B: go to /forge/created?code&state
//!     B->>S: GET /forge/created?code&state
//!     S->>G: POST /app-manifests/{code}/conversions
//!     G-->>S: the App ID and the private key
//!     S->>S: the store keeps them; the server uses the App at once
//!     S-->>B: go to the install page of the App on the org
//!     R->>S: POST /v1/forge/created (each 2 seconds)
//!     S->>G: GET /app/installations (JWT of the App)
//!     S-->>R: the App, and installed
//!     R->>S: riff forge allow, then riff forge check
//! ```
//!
//! - [`manifest`] gives the manifest: the [`app_permissions`] only, no
//!   webhook, public (01M4CTAYV4P2M8Q85ESWS0B59C).
//! - A `state` is good for [`STATE_LIFE`], for one return of GitHub, and
//!   only for the admin that made it ([`Starts`]). A wrong, old or used
//!   `state` is refused (01M4CTAYXHW701XPW2X10H8HH8).
//! - The key is in no reply and in no log line. The server keeps it in
//!   its [`super::store::Store`] and in memory.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use riff_core::forge::app_permissions;
use riff_core::wire::{ForgeCreatedReply, ForgeInstallReply};
use serde::Deserialize;
use serde_json::{Value, json};

use super::store::Stored;
use super::{App, Forge, Refusal, TARGET, now_secs};

/// A `state` of `riff forge create` is good this long.
pub const STATE_LIFE: Duration = Duration::from_secs(10 * 60);

/// The start page: it posts the manifest to GitHub.
pub const NEW_PATH: &str = "/forge/new";

/// GitHub sends the browser here after the admin made the App.
pub const CREATED_PATH: &str = "/forge/created";

/// The web site of GitHub.
pub const GITHUB_WEB: &str = "https://github.com";

/// The manifest of the GitHub App of riff, for the server at
/// `public_url` and the organization `org`.
///
/// ```
/// use riff_server::forge::manifest::manifest;
///
/// let m = manifest("https://riff.example.com", "acme");
/// assert_eq!(m["public"], true);
/// assert_eq!(m["hook_attributes"]["active"], false);
/// assert_eq!(m["redirect_url"], "https://riff.example.com/forge/created");
/// assert_eq!(m["default_permissions"]["contents"], "write");
/// assert!(m["default_permissions"].get("administration").is_none());
/// assert_eq!(m["default_events"], serde_json::json!([]));
/// ```
pub fn manifest(public_url: &str, org: &str) -> Value {
    json!({
        "name": format!("riff-{org}"),
        "url": public_url,
        "redirect_url": format!("{public_url}{CREATED_PATH}"),
        "description": "riff gives each agent session a token with only the rights of its role.",
        "public": true,
        "default_permissions": app_permissions(),
        "default_events": [],
        "hook_attributes": { "url": public_url, "active": false },
    })
}

/// True when `name` can be a GitHub account: letters, digits and `-`.
///
/// ```
/// use riff_server::forge::manifest::account_name;
///
/// assert!(account_name("como-technologies"));
/// assert!(!account_name("acme/app"));
/// assert!(!account_name(""));
/// assert!(!account_name("a\"><script>"));
/// ```
pub fn account_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The text of `text` in HTML.
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The start page of `state`: a form that posts `manifest` to the new
/// App page of `org` on GitHub, and posts itself.
pub fn start_page(org: &str, state: &str, manifest: &Value) -> String {
    let action = format!("{GITHUB_WEB}/organizations/{org}/settings/apps/new?state={state}");
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>riff: make the GitHub \
         App</title></head>\n<body>\n<form id=\"manifest\" method=\"post\" action=\"{}\">\n\
         <input type=\"hidden\" name=\"manifest\" value=\"{}\">\n<p>riff makes its GitHub App \
         on {}. On GitHub, click \"Create GitHub App\".</p>\n<button type=\"submit\">Go on to \
         GitHub</button>\n</form>\n<script>document.getElementById(\"manifest\").submit()\
         </script>\n</body></html>\n",
        html(&action),
        html(&manifest.to_string()),
        html(org)
    )
}

/// A page with one line of text.
pub fn text_page(text: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>riff</title></head>\n\
         <body><p>{}</p></body></html>\n",
        html(text)
    )
}

/// The end of one start.
#[derive(Clone, Debug, PartialEq, Eq)]
enum End {
    /// The server has the App.
    Made { app: u64, slug: String },
    /// The start failed.
    Failed(String),
}

#[derive(Clone, Debug)]
struct Start {
    org: String,
    /// The user of the admin that made the start.
    by: String,
    until: Instant,
    /// True after GitHub sent the browser back.
    used: bool,
    end: Option<End>,
}

/// The starts of `riff forge create`, by their `state`.
#[derive(Debug, Default)]
pub struct Starts(Mutex<HashMap<String, Start>>);

/// Why a `state` is refused.
pub const BAD_STATE: &str =
    "this link of riff forge create is wrong, old or used. Run riff forge create again";

impl Starts {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Start>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A new `state` for `org`, made by the user `by`. It forgets each
    /// start that ended its life.
    pub fn begin(&self, org: &str, by: &str, now: Instant) -> String {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the OS gives random bytes");
        let state: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let mut map = self.map();
        map.retain(|_, s| now < s.until + STATE_LIFE);
        map.insert(
            state.clone(),
            Start {
                org: org.to_owned(),
                by: by.to_owned(),
                until: now + STATE_LIFE,
                used: false,
                end: None,
            },
        );
        state
    }

    /// The org of `state`, while it is good and not used.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_server::forge::manifest::{STATE_LIFE, Starts};
    ///
    /// let starts = Starts::default();
    /// let now = Instant::now();
    /// let state = starts.begin("acme", "mike", now);
    /// assert_eq!(starts.org(&state, now).as_deref(), Some("acme"));
    /// assert_eq!(starts.org("other", now), None);
    /// assert_eq!(starts.org(&state, now + STATE_LIFE + Duration::from_secs(1)), None);
    /// // GitHub sends the browser back one time.
    /// assert_eq!(starts.take(&state, now).as_deref(), Ok("acme"));
    /// assert!(starts.take(&state, now).is_err());
    /// assert_eq!(starts.org(&state, now), None);
    /// ```
    pub fn org(&self, state: &str, now: Instant) -> Option<String> {
        self.map()
            .get(state)
            .filter(|s| !s.used && now < s.until)
            .map(|s| s.org.clone())
    }

    /// The org of `state`, for the one return of GitHub: after it, the
    /// `state` is used.
    pub fn take(&self, state: &str, now: Instant) -> Result<String, &'static str> {
        let mut map = self.map();
        match map.get_mut(state) {
            Some(s) if !s.used && now < s.until => {
                s.used = true;
                Ok(s.org.clone())
            }
            _ => Err(BAD_STATE),
        }
    }

    fn end(&self, state: &str, end: End) {
        if let Some(s) = self.map().get_mut(state) {
            s.end = Some(end);
        }
    }

    /// The org and the end of `state`, for the user `by`. A `state` of
    /// another user is refused like a wrong one.
    fn of(&self, state: &str, by: &str) -> Result<(String, Option<End>), &'static str> {
        self.map()
            .get(state)
            .filter(|s| s.by == by)
            .map(|s| (s.org.clone(), s.end.clone()))
            .ok_or(BAD_STATE)
    }
}

/// What GitHub gives for the code of a manifest.
#[derive(Deserialize)]
struct Conversion {
    id: u64,
    slug: String,
    pem: String,
    owner: Account,
}

#[derive(Deserialize)]
struct Account {
    login: String,
    id: u64,
}

#[derive(Deserialize)]
struct Installed {
    account: Option<Login>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
struct AppReply {
    slug: String,
}

/// The install page of the App `slug` for the account with the ID
/// `target`.
///
/// ```
/// use riff_server::forge::manifest::install_url;
///
/// assert_eq!(
///     install_url("riff-acme", 42),
///     "https://github.com/apps/riff-acme/installations/new/permissions?target_id=42"
/// );
/// ```
pub fn install_url(slug: &str, target: u64) -> String {
    format!("{GITHUB_WEB}/apps/{slug}/installations/new/permissions?target_id={target}")
}

impl Forge {
    /// The starts of `riff forge create`.
    pub fn starts(&self) -> &Starts {
        &self.starts
    }

    /// True when the server can keep a new App.
    pub fn has_store(&self) -> bool {
        self.store.is_some()
    }

    /// The return of GitHub with `code` for `state`: the server swaps
    /// the code for the App, keeps it in its store, uses it at once, and
    /// gives the install page of the App on the org. The error is the
    /// text for the admin.
    pub async fn created(&self, code: &str, state: &str) -> Result<String, String> {
        let org = self.starts.take(state, Instant::now())?;
        match self.convert(code).await {
            Ok((app, slug, target)) => {
                self.starts.end(state, End::Made { app, slug: slug.clone() });
                tracing::info!(target: TARGET, result = "app", app, org, "the new GitHub App");
                Ok(install_url(&slug, target))
            }
            Err(why) => {
                tracing::warn!(target: TARGET, org, "riff forge create failed: {why}");
                self.starts.end(state, End::Failed(why.clone()));
                Err(why)
            }
        }
    }

    /// Swaps `code` for the App: its ID, its slug, and the ID of its
    /// owner. It keeps the App in the store, then uses it.
    async fn convert(&self, code: &str) -> Result<(u64, String, u64), String> {
        let store = self
            .store
            .as_ref()
            .ok_or("this riff-server has no store for the App: give it RIFF_FORGE_SECRET")?;
        if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("GitHub gave no valid code".into());
        }
        let url = format!("{}/app-manifests/{code}/conversions", self.api);
        let what = "the new GitHub App";
        let conversion = match self
            .send::<Conversion>(self.http.post(url), what)
            .await
            .map_err(|r| r.to_string())?
        {
            (_, Some(c)) => c,
            (status, None) => return Err(format!("the GitHub API refused {what}: {status}")),
        };
        let app = App::new(conversion.id, conversion.pem.as_bytes())?;
        store
            .write(&Stored {
                app: conversion.id,
                key: conversion.pem,
            })
            .await?;
        self.set_app(app);
        tracing::info!(
            target: TARGET,
            app = conversion.id,
            owner = conversion.owner.login,
            "the store keeps the new GitHub App"
        );
        Ok((conversion.id, conversion.slug, conversion.owner.id))
    }

    /// How far the start `state` of the user `by` is.
    pub async fn progress(&self, state: &str, by: &str) -> Result<ForgeCreatedReply, String> {
        let (org, end) = self.starts.of(state, by)?;
        Ok(match end {
            None => ForgeCreatedReply::default(),
            Some(End::Failed(why)) => ForgeCreatedReply {
                error: Some(why),
                ..ForgeCreatedReply::default()
            },
            Some(End::Made { app, slug }) => ForgeCreatedReply {
                app: Some(app),
                slug: Some(slug),
                installed: self.installed(&org).await.map_err(|r| r.to_string())?,
                error: None,
            },
        })
    }

    /// True when the App is installed on the GitHub account `owner`.
    pub async fn installed(&self, owner: &str) -> Result<bool, Refusal> {
        let s = self.settings()?;
        let url = format!("{}/app/installations?per_page=100", s.api);
        let jwt = s.app.jwt(now_secs()).map_err(Refusal::GitHub)?;
        let what = "the installations of the App";
        match self
            .send::<Vec<Installed>>(self.http.get(url).bearer_auth(jwt), what)
            .await?
        {
            (_, Some(all)) => Ok(all
                .iter()
                .filter_map(|i| i.account.as_ref())
                .any(|a| a.login.eq_ignore_ascii_case(owner))),
            (status, None) => Err(Refusal::GitHub(format!(
                "the GitHub API refused {what}: {status}"
            ))),
        }
    }

    /// `riff forge install OWNER`: the install page of the App with the
    /// account `owner` as the target, and whether the App is installed
    /// there now.
    pub async fn install(&self, owner: &str) -> Result<ForgeInstallReply, Refusal> {
        let s = self.settings()?;
        let jwt = s.app.jwt(now_secs()).map_err(Refusal::GitHub)?;
        let what = "the GitHub App";
        let slug = match self
            .send::<AppReply>(self.http.get(format!("{}/app", s.api)).bearer_auth(jwt), what)
            .await?
        {
            (_, Some(app)) => app.slug,
            (status, None) => {
                return Err(Refusal::GitHub(format!(
                    "the GitHub API refused {what}: {status}"
                )));
            }
        };
        let what = format!("the GitHub account {owner}");
        let target = match self
            .send::<Account>(self.http.get(format!("{}/users/{owner}", s.api)), &what)
            .await?
        {
            (_, Some(account)) => account.id,
            (status, None) => {
                return Err(Refusal::GitHub(format!(
                    "GitHub has no account {owner}: {status}"
                )));
            }
        };
        Ok(ForgeInstallReply {
            url: install_url(&slug, target),
            installed: self.installed(owner).await?,
        })
    }
}
