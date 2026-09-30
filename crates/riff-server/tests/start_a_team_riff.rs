//! "Start a Team Riff" (01M3MEFG6F102T1H8DFJ38EJ4A): the page starts a
//! real `riff-server` with the OIDC app in the environment. The test
//! gives it a fake provider, signs in the owner and invites a person.
//! The `riff` commands of the page are checked in
//! `crates/riff/tests/start_a_riff.rs`.

mod common;

use isolated::Isolated;
use std::fs;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::Duration;

use riff_core::dpop::Key;
use riff_core::wire::{ID_TOKEN_TYPE, Invite, Invited, TOKEN_EXCHANGE, TokenReply, TokenRequest};

/// The text of the page.
fn page() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/start-a-team-riff.md");
    fs::read_to_string(path).unwrap()
}

/// The commands in the `sh` blocks of the page, in order.
fn commands() -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page().lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && !line.is_empty() {
            commands.push(line.to_owned());
        }
    }
    commands
}

/// A `riff-server` process that stops when the test ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A free port on the loopback address.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Runs the `riff-server` command of the page. `URL` is `url` and
/// `EMAIL` is `owner`. The environment holds the OIDC app of the page,
/// with the fake `issuer`, and the listen address of the test. The TLS
/// proxy of the page is not in the test.
async fn start(command: &str, issuer: &str, listen: &str, url: &str, owner: &str) -> Server {
    let args: Vec<String> = command
        .split_whitespace()
        .skip(1)
        .map(|word| match word {
            "URL" => url.to_owned(),
            "EMAIL" => owner.to_owned(),
            other => other.to_owned(),
        })
        .collect();
    let child = Isolated::shared()
        .riff_server()
        .args(args)
        .env_clear()
        .env("RIFF_OIDC_CLIENT_ID", "riff-client")
        .env("RIFF_OIDC_CLIENT_SECRET", "not-secret")
        .env("RIFF_OIDC_ISSUER", issuer)
        .env("RIFF_LISTEN", listen)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server(child);
    for _ in 0..200 {
        if tokio::net::TcpStream::connect(listen).await.is_ok() {
            return server;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("riff-server does not listen on {listen}");
}

/// Signs in `email` with a personal account, as `riff login` does.
async fn sign_in(url: &str, issuer: &str, email: &str) -> (Key, TokenReply) {
    let key = Key::generate();
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(common::id_token(issuer, email, None)),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let reply = common::post(&format!("{url}/v1/token"), &key, None)
        .form(&form)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{}", reply.text().await.unwrap());
    (key, reply.json().await.unwrap())
}

#[tokio::test]
async fn the_page_starts_a_team_riff_signs_in_the_owner_and_invites_a_person() {
    let commands = commands();
    // The OIDC app comes from the environment: no secret on the page.
    assert!(
        commands
            .contains(&"export RIFF_OIDC_CLIENT_ID=ID RIFF_OIDC_CLIENT_SECRET=SECRET".to_owned()),
        "{commands:?}"
    );
    let servers: Vec<&String> = commands
        .iter()
        .filter(|c| c.starts_with("riff-server "))
        .collect();
    // The second command is the example of "Change the times"
    // (01M3Q5460YESBSQHTV3M15PE53). The third keeps the state over a
    // restart (R30).
    assert_eq!(
        servers,
        [
            "riff-server --public-url URL --owner EMAIL",
            "riff-server --public-url URL --owner EMAIL --owner-take-minutes 30",
            "riff-server --public-url URL --owner EMAIL --dir ~/.local/state/riff-server",
        ]
    );

    let issuer = common::fake_provider().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let url = format!("http://{listen}");
    let _server = start(servers[0], &issuer, &listen, &url, "Ada@gmail.com").await;

    // The owner signs in first.
    let (key, owner) = sign_in(&url, &issuer, "ada@gmail.com").await;
    assert_eq!(owner.user, "ada");

    // The owner invites a person. The reply names the public address.
    let invite = common::post(&format!("{url}/v1/invite"), &key, Some(&owner.access_token))
        .json(&Invite {
            email: "bob@gmail.com".into(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(invite.status(), 200);
    let invited: Invited = invite.json().await.unwrap();
    assert_eq!(invited.email, "bob@gmail.com");
    assert_eq!(invited.address, url);

    // The person signs in.
    let (_, bob) = sign_in(&url, &issuer, "bob@gmail.com").await;
    assert_eq!(bob.user, "bob");
}

#[test]
fn the_page_has_no_secret_and_no_path_for_developers() {
    let page = page();
    for word in [
        "--insecure",
        "systemd",
        "riff-server install",
        "GOCSPX-",
        ".apps.googleusercontent.com",
    ] {
        assert!(!page.contains(word), "the page names {word}");
    }
}

#[test]
fn each_riff_server_command_of_the_page_is_real() {
    for command in commands().iter().filter(|c| c.starts_with("riff-server ")) {
        Isolated::shared()
            .assert_riff_server()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
