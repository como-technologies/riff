//! The keyring store of riff opens plain sessions of the Secret Service,
//! as `gh` does (01M556QPPRSQ91HDED128G2X7G, #1008).
//!
//! Each test runs its check in a child process on a private keyring: a
//! private D-Bus session bus of `dbus-run-session`, with its own
//! `gnome-keyring-daemon`, a test password and a temp home. The child
//! never uses the bus or the keyring of the machine. It stops before its
//! first call to the keyring when the bus already has a Secret Service,
//! or when the owner of `org.freedesktop.secrets` is not the daemon that
//! it started.
//!
//! The child sets the keyring store of its process, so this file is a
//! test binary of its own.

#![cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keyring_core::api::CredentialStoreApi;
use riff::device;
use riff::keyring_store;
use riff::login::{self, SignIn};
use riff::secrets::{self, SERVICE};
use riff_core::dpop::Key;

/// The variable that names the check of the child.
const CHILD: &str = "RIFF_TEST_KEYRING_CHILD";

/// The name of the Secret Service on the bus.
const SECRETS: &str = "org.freedesktop.secrets";

/// The algorithm of a DH session: the store of the `keyring` crate.
const DH: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";

/// The programs of a private keyring.
const TOOLS: [&str; 4] = [
    "dbus-run-session",
    "dbus-send",
    "dbus-monitor",
    "gnome-keyring-daemon",
];

/// The longest time of one child.
const CHILD_WAIT: Duration = Duration::from_secs(120);

/// The criterion 1 of #1008: the store of riff opens each session as
/// `plain`. The control: the store of the `keyring` crate opens it as
/// DH.
#[test]
fn the_store_of_riff_opens_each_session_as_plain() {
    on_a_private_keyring("plain");
}

/// The criterion 2 of #1008: 300 sessions of the store of riff add no
/// locked memory to GNOME Keyring. The control: 300 sessions of the
/// store of the `keyring` crate add locked memory.
#[test]
fn three_hundred_sessions_of_the_store_of_riff_add_no_locked_memory() {
    on_a_private_keyring("memory");
}

/// The criterion 3 of #1008: riff reads and writes the sign-in and the
/// device key that the store of the `keyring` crate wrote, and that
/// store reads what riff writes.
#[test]
fn riff_reads_and_writes_the_secrets_of_the_old_store() {
    on_a_private_keyring("old");
}

/// Runs the check `check` of [`child`] in a child on a private keyring,
/// and fails when the child fails.
fn on_a_private_keyring(check: &str) {
    for tool in TOOLS {
        assert!(
            on_path(tool),
            "the test needs {tool}: install the packages dbus-daemon, dbus-bin and gnome-keyring"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);
    for name in ["home", "run", "data", "config", "cache"] {
        std::fs::create_dir(path(name)).unwrap();
    }
    let private = std::os::unix::fs::PermissionsExt::from_mode(0o700);
    std::fs::set_permissions(path("run"), private).unwrap();
    let out = std::fs::File::create(path("out")).unwrap();
    let mut child = Command::new("dbus-run-session")
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "child", "--nocapture"])
        .env(CHILD, check)
        .env("HOME", path("home"))
        .env("XDG_RUNTIME_DIR", path("run"))
        .env("XDG_DATA_HOME", path("data"))
        .env("XDG_CONFIG_HOME", path("config"))
        .env("XDG_CACHE_HOME", path("cache"))
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("GNOME_KEYRING_CONTROL")
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("RIFF_HOME")
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > CHILD_WAIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the check {check} did not end in {CHILD_WAIT:?}:\n{}",
                std::fs::read_to_string(path("out")).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let text = std::fs::read_to_string(path("out")).unwrap();
    println!("{text}");
    assert!(status.success(), "the check {check} failed:\n{text}");
    assert!(
        text.contains("1 passed"),
        "the check {check} did not run:\n{text}"
    );
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file()))
}

/// The check of a child on a private keyring. In each other run, it
/// does nothing.
#[test]
fn child() {
    let Ok(check) = std::env::var(CHILD) else {
        return;
    };
    let keyring = Keyring::start();
    match check.as_str() {
        "plain" => plain(),
        "memory" => memory(&keyring),
        "old" => old(),
        other => panic!("no check {other}"),
    }
}

/// The store of riff opens each session as `plain`, and the store of
/// the `keyring` crate as DH.
fn plain() {
    let monitor = Monitor::start();
    let name = login::secret_name("https://riff.example");
    // The secrets of riff go through the store of the OS: one session.
    secrets::set(&name, "not a real secret").unwrap();
    assert_eq!(
        secrets::get(&name).unwrap().as_deref(),
        Some("not a real secret")
    );
    assert!(!secrets::own_store());
    // Each short riff process opens a store of its own.
    for _ in 0..3 {
        let store = keyring_store::Store::new().unwrap();
        let entry = store.build(SERVICE, &name, None).unwrap();
        assert_eq!(entry.get_password().unwrap(), "not a real secret");
    }
    assert_eq!(monitor.wait_for(4), ["plain"; 4]);

    // The control: the store of the keyring crate.
    let old = zbus_secret_service_keyring_store::Store::new().unwrap();
    let entry = old.build(SERVICE, &name, None).unwrap();
    assert_eq!(entry.get_password().unwrap(), "not a real secret");
    assert_eq!(
        monitor.wait_for(5),
        ["plain", "plain", "plain", "plain", DH]
    );
}

/// 300 sessions of the store of riff add no locked memory, and 300 of
/// the store of the `keyring` crate do.
fn memory(keyring: &Keyring) {
    let name = device::secret_name("https://riff.example");
    secrets::set(&name, "not a real secret").unwrap();
    // As a short process: a new D-Bus connection and a new session, a
    // read of the secret, then the connection closes.
    let read = |store: Arc<keyring_core::api::CredentialStore>| {
        let entry = store.build(SERVICE, &name, None).unwrap();
        assert_eq!(entry.get_password().unwrap(), "not a real secret");
    };
    let start = keyring.locked();
    for _ in 0..300 {
        read(keyring_store::Store::new().unwrap());
    }
    let after_riff = keyring.locked();
    for _ in 0..300 {
        read(zbus_secret_service_keyring_store::Store::new().unwrap());
    }
    let after_old = keyring.locked();
    println!(
        "VmLck: {start} kB, {after_riff} kB after riff, {after_old} kB after the keyring crate"
    );
    assert_eq!(after_riff, start, "300 plain sessions add locked memory");
    assert!(
        after_old > after_riff,
        "the control: 300 DH sessions add no locked memory, so the test cannot see it"
    );
}

/// riff reads and writes the secrets that the store of the `keyring`
/// crate wrote, and that store reads what riff writes.
fn old() {
    let server = "https://riff.example";
    let sign_in_name = login::secret_name(server);
    let key_name = device::secret_name(server);
    let old = zbus_secret_service_keyring_store::Store::new().unwrap();
    let entry = |name: &str| old.build(SERVICE, name, None).unwrap();
    let items = |name: &str| {
        old.search(&HashMap::from([("service", SERVICE), ("username", name)]))
            .unwrap()
    };

    // An older riff kept a sign-in and a device key.
    let sign_in = SignIn {
        user: "mike".into(),
        access_token: "a-1".into(),
        refresh_token: "r-1".into(),
        expires_at: 1,
        riff_id: Some("riff-1".into()),
    };
    let key = Key::generate();
    entry(&sign_in_name)
        .set_password(&serde_json::to_string(&sign_in).unwrap())
        .unwrap();
    entry(&key_name).set_password(&key.to_secret()).unwrap();

    // This riff reads them.
    assert_eq!(login::stored(server).unwrap(), Some(sign_in.clone()));
    assert_eq!(device::key(server).unwrap().thumbprint(), key.thumbprint());

    // This riff writes them. Each stays one item, and the older riff
    // reads the new values.
    let new = SignIn {
        access_token: "a-2".into(),
        refresh_token: "r-2".into(),
        ..sign_in.clone()
    };
    login::store(server, &new).unwrap();
    let new_key = Key::generate();
    secrets::set(&key_name, &new_key.to_secret()).unwrap();
    for name in [&sign_in_name, &key_name] {
        assert_eq!(items(name).len(), 1, "{name}");
    }
    let read: SignIn = serde_json::from_str(&entry(&sign_in_name).get_password().unwrap()).unwrap();
    assert_eq!(read, new);
    let read = Key::from_secret(&entry(&key_name).get_password().unwrap()).unwrap();
    assert_eq!(read.thumbprint(), new_key.thumbprint());

    // A new secret of this riff: one item with the label of the old
    // store, and the older riff reads it.
    let other = "https://other.example";
    login::store(other, &sign_in).unwrap();
    let found = items(&login::secret_name(other));
    assert_eq!(found.len(), 1);
    let wrapper: &zbus_secret_service_keyring_store::cred::Wrapper =
        found[0].as_any().downcast_ref().unwrap();
    assert_eq!(
        wrapper.get_label().unwrap(),
        keyring_store::label(SERVICE, &login::secret_name(other))
    );
    let read: SignIn =
        serde_json::from_str(&entry(&login::secret_name(other)).get_password().unwrap()).unwrap();
    assert_eq!(read, sign_in);

    // riff removes a secret, and the older riff finds none.
    assert!(login::logout(server).unwrap());
    assert!(matches!(
        entry(&sign_in_name).get_password(),
        Err(keyring_core::Error::NoEntry)
    ));
    assert!(items(&sign_in_name).is_empty());
}

/// A `gnome-keyring-daemon` of the test on the private bus of the
/// child, unlocked with a test password.
struct Keyring {
    daemon: Child,
}

impl Keyring {
    /// Starts the daemon. Stops the test when the bus is not private:
    /// it already has a Secret Service, or the owner of the Secret
    /// Service is another process.
    fn start() -> Keyring {
        let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .expect("the child runs on the bus of dbus-run-session");
        assert!(
            !has_owner(),
            "the bus {bus} already has a Secret Service: it is not a private bus. Stop."
        );
        let mut daemon = Command::new("gnome-keyring-daemon")
            .args(["--foreground", "--unlock", "--components=secrets"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // The daemon reads the password up to the end of its input.
        daemon
            .stdin
            .take()
            .unwrap()
            .write_all(b"test-password")
            .unwrap();
        let keyring = Keyring { daemon };
        let start = Instant::now();
        while !has_owner() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the test keyring did not start"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let owner = bus_call("GetConnectionUnixProcessID");
        assert!(
            owner.contains(&format!("uint32 {}", keyring.daemon.id())),
            "the owner of {SECRETS} is not the test keyring: {owner}. Stop."
        );
        keyring
    }

    /// The locked memory of the daemon, in kB, after it handled each
    /// closed connection.
    fn locked(&self) -> u64 {
        std::thread::sleep(Duration::from_secs(1));
        let status = Path::new("/proc")
            .join(self.daemon.id().to_string())
            .join("status");
        let status = std::fs::read_to_string(status).unwrap();
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmLck:"))
            .and_then(|kb| kb.trim().trim_end_matches("kB").trim().parse().ok())
            .unwrap_or_else(|| panic!("no VmLck in {status}"))
    }
}

impl Drop for Keyring {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// True when the bus has an owner of the Secret Service.
fn has_owner() -> bool {
    bus_call("NameHasOwner").contains("boolean true")
}

/// The reply of the bus to the call `method` with the name of the
/// Secret Service.
fn bus_call(method: &str) -> String {
    let out = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            &format!("org.freedesktop.DBus.{method}"),
            &format!("string:{SECRETS}"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A `dbus-monitor` of each `OpenSession` call on the bus of the child.
struct Monitor {
    child: Child,
    lines: Arc<Mutex<Vec<String>>>,
}

impl Monitor {
    /// Starts the monitor, and waits until the bus sends it the calls.
    fn start() -> Monitor {
        let mut child = Command::new("dbus-monitor")
            .args([
                "--session",
                "type='method_call',interface='org.freedesktop.Secret.Service',member='OpenSession'",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let stdout = child.stdout.take().unwrap();
        let kept = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                kept.lock().unwrap().push(line);
            }
        });
        let monitor = Monitor { child, lines };
        // A new monitor loses its name when it starts to monitor.
        monitor.wait(|lines| lines.iter().any(|l| l.contains("member=NameLost")));
        monitor
    }

    /// The algorithm of each `OpenSession` call, in order, when there
    /// are `count` calls.
    fn wait_for(&self, count: usize) -> Vec<String> {
        self.wait(|lines| algorithms(lines).len() >= count);
        // No later call comes with the calls that the test waits for.
        std::thread::sleep(Duration::from_millis(500));
        algorithms(&self.lines.lock().unwrap())
    }

    fn wait(&self, done: impl Fn(&[String]) -> bool) {
        let start = Instant::now();
        while !done(&self.lines.lock().unwrap()) {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "dbus-monitor: {:?}",
                self.lines.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The algorithm of each `OpenSession` call in the lines of
/// `dbus-monitor`: the first argument of the call.
fn algorithms(lines: &[String]) -> Vec<String> {
    lines
        .windows(2)
        .filter(|pair| pair[0].starts_with("method call") && pair[0].contains("member=OpenSession"))
        .filter_map(|pair| {
            let arg = pair[1].trim().strip_prefix("string \"")?;
            arg.strip_suffix('"').map(str::to_owned)
        })
        .collect()
}

#[test]
fn the_algorithm_is_the_first_argument_of_each_open_session() {
    let lines: Vec<String> = [
        "signal time=1.0 sender=org.freedesktop.DBus -> destination=:1.1 serial=4 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=NameLost",
        "   string \":1.1\"",
        "method call time=2.0 sender=:1.6 -> destination=org.freedesktop.secrets serial=3 path=/org/freedesktop/secrets; interface=org.freedesktop.Secret.Service; member=OpenSession",
        "   string \"plain\"",
        "   variant       string \"\"",
        "method call time=3.0 sender=:1.7 -> destination=org.freedesktop.secrets serial=3 path=/org/freedesktop/secrets; interface=org.freedesktop.Secret.Service; member=OpenSession",
        "   string \"dh-ietf1024-sha256-aes128-cbc-pkcs7\"",
        "   variant       array of bytes [",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(algorithms(&lines), ["plain", DH]);
}
