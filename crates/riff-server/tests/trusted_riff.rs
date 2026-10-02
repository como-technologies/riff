//! `riff-server` with no sign-in trusts its network (R211). It marks
//! each `read` reply as trusted, also when it listens on the network.
//! It listens on the network only with `--insecure`
//! (01M3JCE4ZD4DZCQ21FA69RT52D). With OIDC settings, it requires
//! sign-in.

mod common;

use isolated::Isolated;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Post, Read, ReadReply, Register};

/// Kills the server when the test ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `riff-server` with these arguments and no settings from the
/// environment.
fn server(args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff_server();
    cmd.args(args).env("NO_COLOR", "1");
    cmd
}

/// Starts `riff-server` with these arguments. Returns the server, the
/// port that it listens on, and the log up to that line.
fn serve(args: &[&str]) -> (Server, u16, String) {
    serve_cmd(server(args))
}

/// Starts this `riff-server` command. See [`serve`].
fn serve_cmd(mut cmd: Command) -> (Server, u16, String) {
    let mut child = cmd.stdout(Stdio::piped()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
    let mut log = String::new();
    let line = loop {
        let line = lines.next().expect("the server says that it listens");
        log.push_str(&line);
        log.push('\n');
        if line.contains("riff-server listens on") {
            break line;
        }
    };
    // Read the rest of the log, so that the server can write it.
    std::thread::spawn(move || lines.for_each(drop));
    // Each log line is JSON (01M3TJWJ3VK671T9NM95F3ES82).
    let line: serde_json::Value = serde_json::from_str(&line).unwrap();
    let message = line["message"].as_str().unwrap();
    let port = message.rsplit(':').next().unwrap().trim().parse().unwrap();
    (server, port, log)
}

/// Posts one message to the repository thread and reads it, through
/// the loopback address.
async fn read_reply(port: u16) -> ReadReply {
    let url = format!("http://127.0.0.1:{port}/v1");
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1"
        .parse()
        .unwrap();
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let http = common::client();
    let call =
        |op: &str, body: serde_json::Value| http.post(format!("{url}/{op}")).json(&body).send();
    let register = Register {
        me: me.clone(),
        worker: false,
    };
    call("register", serde_json::to_value(register).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let post = Post::new(&me, Some(thread.clone()), vec![], "ready");
    call("post", serde_json::to_value(post).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let read = Read {
        me,
        thread,
        all: true,
        after: None,
    };
    call("read", serde_json::to_value(read).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_server_with_no_sign_in_trusts_its_callers() {
    for args in [
        &["--listen", "127.0.0.1:0"][..],
        &["--listen", "0.0.0.0:0", "--insecure"][..],
    ] {
        let (_server, port, _) = serve(args);
        let reply = read_reply(port).await;
        assert_eq!(reply.messages.len(), 1);
        assert!(reply.trusted, "{args:?}");
    }
}

/// Only the read of the log takes a value of a later build
/// (01M3XSF90E9JYYTC13D9THY4WE): a `post` call with a kind or a field
/// of a selector that the server does not know is refused.
#[tokio::test]
async fn a_post_with_a_kind_or_a_selector_field_of_a_later_build_is_a_bad_request() {
    let (_server, port, _) = serve(&["--listen", "127.0.0.1:0"]);
    let url = format!("http://127.0.0.1:{port}/v1");
    let me = "riff://mike@pangolin/como-technologies/riff?session=a1";
    let thread = "como-technologies/riff";
    let http = common::client();
    let call =
        |op: &str, body: serde_json::Value| http.post(format!("{url}/{op}")).json(&body).send();
    call("register", serde_json::json!({ "me": me }))
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let post = |kind: &str, to: serde_json::Value| serde_json::json!({ "me": me, "thread": thread, "to": [to], "body": "hi", "kind": kind });
    let known = serde_json::json!({ "user": "mike" });
    let later = serde_json::json!({ "user": "mike", "wave": "17" });
    for (body, text) in [
        (post("poll", known.clone()), "no such kind"),
        (post("other", known.clone()), "no such kind"),
        (
            post("note", later),
            r#"this server does not know the selector {"user":"mike","wave":"17"}"#,
        ),
        (
            post("note", serde_json::json!("all")),
            r#"this server does not know the selector "all""#,
        ),
        (
            post(
                "note",
                serde_json::json!({ "user": "mike", "lead": "maybe" }),
            ),
            "this server does not know the selector",
        ),
    ] {
        let reply = call("post", body.clone()).await.unwrap();
        assert_eq!(reply.status(), 400, "{body}");
        let code = reply.headers()[riff_core::wire::REFUSED_HEADER].clone();
        assert_eq!(code, "bad_request", "{body}");
        let why = reply.text().await.unwrap();
        assert!(why.contains(text), "{why}");
    }
    // A caller whose URI has a query part that the server does not
    // know is refused in each call: a command, a signal and a query.
    let later = format!("{me}&wave=17");
    for (op, body) in [
        ("register", serde_json::json!({ "me": later })),
        ("alive", serde_json::json!({ "me": later })),
        (
            "post",
            serde_json::json!({ "me": later, "thread": thread, "body": "hi" }),
        ),
        (
            "read",
            serde_json::json!({ "me": later, "thread": thread, "all": true }),
        ),
        ("who", serde_json::json!({ "me": later })),
    ] {
        let reply = call(op, body.clone()).await.unwrap();
        assert_eq!(reply.status(), 400, "{op}");
        // A command names the code of its refusal. A signal and a
        // query give only the status and the text.
        let code = reply
            .headers()
            .get(riff_core::wire::REFUSED_HEADER)
            .cloned();
        if matches!(op, "register" | "post") {
            assert_eq!(code.unwrap(), "bad_request", "{op}");
        }
        let why = reply.text().await.unwrap();
        assert!(
            why.contains("a part that this server does not know: wave=17"),
            "{op}: {why}"
        );
    }
    call("post", post("note", known))
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let read = serde_json::json!({ "me": me, "thread": thread, "all": true });
    let reply: ReadReply = call("read", read)
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply.messages.len(), 1, "a refused post leaves no message");
}

#[test]
fn with_no_sign_in_a_network_address_needs_insecure() {
    let out = server(&["--listen", "0.0.0.0:0"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--insecure"), "{stderr}");
    assert!(stderr.contains("0.0.0.0:0"), "{stderr}");
    // It names the settings of sign-in (01M3JZN1VEF73EPFE2FJY36EY4).
    assert!(stderr.contains("RIFF_OIDC_CLIENT_ID"), "{stderr}");
    assert!(stderr.contains("RIFF_OIDC_CLIENT_SECRET"), "{stderr}");
}

/// With both OIDC settings in the environment, the server starts and
/// requires sign-in (01M3JZN1XQVVNVD0MJVM8J91HC). The issuer does not
/// answer, so the test calls no provider; the server serves (R153).
#[tokio::test]
async fn oidc_settings_in_the_environment_require_sign_in() {
    let mut cmd = server(&["--listen", "127.0.0.1:0"]);
    cmd.env("RIFF_OIDC_ISSUER", "http://127.0.0.1:1")
        .env("RIFF_OIDC_CLIENT_ID", "my-app")
        .env("RIFF_OIDC_CLIENT_SECRET", "my-secret");
    let (_server, port, _) = serve_cmd(cmd);
    let url = format!("http://127.0.0.1:{port}/v1");
    let http = common::client();
    let config: serde_json::Value = http
        .get(format!("{url}/sign-in"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(config["client_id"], "my-app");
    let register = Register {
        me: "riff://mike@pangolin".parse().unwrap(),
        worker: false,
    };
    let reply = http
        .post(format!("{url}/register"))
        .json(&register)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

#[test]
fn insecure_warns_at_start() {
    let (_server, _, log) = serve(&["--listen", "0.0.0.0:0", "--insecure"]);
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("answer as any person"), "{log}");
}

#[test]
fn loopback_needs_no_flag_and_gives_no_warning() {
    let (_server, _, log) = serve(&["--listen", "127.0.0.1:0"]);
    assert!(!log.contains("any person"), "{log}");
}

#[test]
fn a_server_with_sign_in_and_an_owner_listens_anywhere_with_no_flag() {
    let (_server, _, log) = serve(&[
        "--listen",
        "0.0.0.0:0",
        "--require-sign-in",
        "--owner",
        "ada@gmail.com",
    ]);
    assert!(!log.contains("any person"), "{log}");
}

/// A riff with sign-in and no owner listens only on loopback, so that
/// the first sign-in comes from its own machine
/// (01M3JN3AQMHZHT6JP3P6GM9PWZ). --insecure does not change that.
#[test]
fn a_server_with_sign_in_and_no_owner_refuses_a_network_address() {
    for args in [
        &["--listen", "0.0.0.0:0", "--require-sign-in"][..],
        &["--listen", "0.0.0.0:0", "--require-sign-in", "--insecure"][..],
    ] {
        let out = server(args).output().unwrap();
        assert!(!out.status.success(), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("no owner"), "{stderr}");
        assert!(stderr.contains("--owner"), "{stderr}");
    }
    let (_server, _, _) = serve(&["--listen", "127.0.0.1:0", "--require-sign-in"]);
}

#[test]
fn an_owner_that_is_not_an_email_is_refused() {
    let out = server(&[
        "--listen",
        "127.0.0.1:0",
        "--require-sign-in",
        "--owner",
        "ada",
    ])
    .output()
    .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not an email"), "{stderr}");
}
