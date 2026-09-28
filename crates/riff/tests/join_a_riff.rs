//! "Join a Riff" (01M3MEHCGZ4AG4C2A77J5HA3P7): one set of steps joins a
//! riff with sign-in, for a person that the owner invites and for a
//! second machine of the owner. The page test follows the steps on
//! three machines, against a real riff and a fake provider: a session
//! of each machine sees the others in `riff who`.
//!
//! Each machine has its own keyring: a mock store of `keyring-core`.
//! The default store is global, so only one test of this file uses a
//! keyring.

mod book;
mod common;

use std::sync::Arc;

use book::{commands_of_part, each_is_real, page, riff_commands_of};
use common::{CLIENT, FakeProvider, browser};
use keyring_core::mock;
use riff::api::{self, Api};
use riff::{login, text};
use riff_core::name::SessionUri;
use riff_core::wire::Invited;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;

const PAGE: &str = "join-a-riff.md";

#[test]
fn a_person_joins_with_three_commands() {
    let steps = commands_of_part(PAGE, "Join the riff");
    let install = format!(
        "cargo install --locked --git {} {}",
        env!("CARGO_PKG_REPOSITORY"),
        env!("CARGO_PKG_NAME")
    );
    assert_eq!(
        steps,
        [
            install.as_str(),
            "echo 'export RIFF_SERVER=ADDRESS' >> ~/.bashrc",
            "riff connect claude",
        ]
    );
}

/// `riff invite` prints the steps of "Join the riff", with the address
/// of the riff.
#[test]
fn the_invite_prints_the_steps_of_the_page() {
    let address = "https://riff.example.com";
    let invited = Invited {
        email: "bob@example.org".into(),
        address: address.into(),
    };
    let steps = commands_of_part(PAGE, "Join the riff")
        .join("\n")
        .replace("ADDRESS", address);
    let shown = text::invited(&invited);
    assert!(shown.ends_with(&format!("\n\n{steps}")), "{shown}");
}

#[test]
fn each_riff_command_of_the_page_is_real() {
    let riff = riff_commands_of(PAGE);
    assert_eq!(
        riff,
        [
            "riff connect claude",
            "riff server",
            "riff who",
            "riff update",
            "riff connect claude",
        ]
    );
    each_is_real(&riff);
}

/// A person who joins runs no riff of their own. The page names no
/// path that has no sign-in.
#[test]
fn the_page_names_no_riff_server_and_no_path_without_sign_in() {
    let text = page(PAGE);
    for word in ["riff-server", "--insecure", "systemd"] {
        assert!(!text.contains(word), "{PAGE} names {word}");
    }
}

/// "Add a Machine" is gone. "Join a Riff" takes its place in the book.
#[test]
fn no_page_links_to_add_a_machine() {
    assert!(!book::dir().join("add-a-machine.md").exists());
    for entry in std::fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("add-a-machine"), "{}", path.display());
    }
    assert!(page("SUMMARY.md").contains("- [Join a Riff](join-a-riff.md)\n"));
}

/// A machine, with a keyring of its own.
struct Machine {
    keyring: Arc<mock::Store>,
}

impl Machine {
    fn new() -> Machine {
        Machine {
            keyring: mock::Store::new().unwrap(),
        }
    }

    /// Makes the keyring of this machine the keyring of riff.
    fn use_keyring(&self) {
        keyring_core::set_default_store(self.keyring.clone());
    }

    /// Does the steps of "Join the riff" on this machine, with `server`
    /// in `RIFF_SERVER`: `riff connect claude` signs in
    /// (01M3JZN1ZZED3FXQEFNJ4KVCN5). The test leaves out the install
    /// and the plugin.
    async fn join(&self, server: &str) -> anyhow::Result<()> {
        self.use_keyring();
        login::ensure(&Api::new(server), browser).await?;
        Ok(())
    }

    /// Starts the session `me` on this machine. Returns its client.
    async fn start(&self, server: &str, me: &SessionUri) -> Api {
        self.use_keyring();
        let session = Api::new(server).signed_in(me.who().session()).unwrap();
        session.register(me).await.unwrap();
        session
    }

    /// The hosts that `riff who` shows to the session `me` of this
    /// machine.
    async fn who(&self, session: &Api, me: &SessionUri) -> Vec<String> {
        self.use_keyring();
        let sessions = session.who(me, false).await.unwrap();
        let shown = text::who(&sessions, me);
        let mut hosts: Vec<String> = sessions
            .iter()
            .map(|s| s.uri.place().host().to_owned())
            .collect();
        hosts.sort();
        for host in &hosts {
            assert!(shown.contains(&format!("@{host}:riff")), "{shown}");
        }
        hosts
    }
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

#[tokio::test]
async fn a_session_of_each_machine_sees_the_others_in_riff_who() {
    // The owner starts a riff with sign-in, with the owner named.
    let provider = FakeProvider::start("ada@example.com", None).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let service = Service::new(Config {
        owner: Some("ada@example.com".into()),
        provider: Some(Provider {
            issuer: provider.issuer.clone(),
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: Vec::new(),
        }),
        ..Config::new(&format!("http://{address}"))
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    // `RIFF_SERVER` takes the address as the owner gives it.
    let server = api::server_url(&address).unwrap();

    // The owner joins from a first machine.
    let first = Machine::new();
    first.join(&server).await.unwrap();
    let ada = uri("riff://ada@pangolin/como-technologies/riff?session=a1");
    let ada_session = first.start(&server, &ada).await;

    // Bob joins from a second machine. Bob needs an invite first.
    let second = Machine::new();
    provider.sign_in_as("bob@example.org", None);
    let refused = second.join(&server).await.unwrap_err();
    assert!(
        format!("{refused:#}").contains("bob@example.org is not a member of this riff"),
        "{refused:#}"
    );
    first.use_keyring();
    Api::new(&server)
        .signed_in(None)
        .unwrap()
        .invite("bob@example.org")
        .await
        .unwrap();
    second.join(&server).await.unwrap();
    let bob = uri("riff://bob@thelio/como-technologies/riff?session=b2");
    let bob_session = second.start(&server, &bob).await;

    // The owner adds a third machine. The owner needs no invite.
    let third = Machine::new();
    provider.sign_in_as("ada@example.com", None);
    third.join(&server).await.unwrap();
    let ada_too = uri("riff://ada@kadomony/como-technologies/riff?session=c3");
    let ada_too_session = third.start(&server, &ada_too).await;

    let all = ["kadomony", "pangolin", "thelio"];
    assert_eq!(first.who(&ada_session, &ada).await, all);
    assert_eq!(second.who(&bob_session, &bob).await, all);
    assert_eq!(third.who(&ada_too_session, &ada_too).await, all);
}
