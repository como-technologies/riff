//! The commands of the spike.
//!
//! - `listen` binds an endpoint with the defaults of n0 and answers
//!   each dial: it checks the record of the dialer, and sends a record
//!   of its own.
//! - `dial` connects to a listener by its endpoint ID only, sends a
//!   record, checks the answer, and measures the connect.
//! - `hold` keeps one connection, sends a ping each 250 ms, and can
//!   drop the link of its network namespace to measure a reconnect.

use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    ms,
    record::{self, Kind, Record},
    spread,
};
use iroh::{
    Endpoint, EndpointId, SecretKey,
    endpoint::{Connection, Incoming, presets},
};
use tokio::time::{MissedTickBehavior, sleep, sleep_until, timeout};

const ALPN: &[u8] = b"riff/spike/iroh/0";

/// The wait for the home relay after a bind.
const ONLINE_WAIT: Duration = Duration::from_secs(15);
/// The wait for one connect.
const CONNECT_WAIT: Duration = Duration::from_secs(20);
/// The wait for a direct path after the start of a connect.
const DIRECT_WAIT: Duration = Duration::from_secs(10);
/// The wait for the answer to one ping.
const PING_WAIT: Duration = Duration::from_secs(1);
/// The wait for one connect of `hold` after its connection closed.
const REDIAL_WAIT: Duration = Duration::from_secs(5);
/// The time between two pings of `hold`.
const PING_EVERY: Duration = Duration::from_millis(250);

type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

const USAGE: &str = "\
usage:
  iroh-spike listen [--key FILE]
  iroh-spike dial ENDPOINT_ID [--count N] [--warm] [--relay-only]
  iroh-spike hold ENDPOINT_ID [--seconds N] [--drop-after N --drop-for N]

listen      Print the endpoint ID, then answer each dial until Ctrl-C.
            --key FILE keeps the secret key in FILE, so the ID stays.
dial        Connect N times (default 5). Each connect has a new endpoint,
            or one endpoint with --warm. --relay-only uses no IP path.
hold        Keep one connection for N seconds (default 120), with a
            ping each 250 ms. --drop-after and --drop-for take the link
            down and up again. Run it in a namespace of its own: see
            drop.sh.";

#[derive(Debug, Default)]
struct Options {
    key: Option<PathBuf>,
    count: usize,
    warm: bool,
    relay_only: bool,
    seconds: u64,
    drop_after: Option<u64>,
    drop_for: Option<u64>,
}

enum Run {
    Listen(Options),
    Dial(EndpointId, Options),
    Hold(EndpointId, Options),
}

fn parse(args: &[String]) -> Result<Run, String> {
    let (command, rest) = args.split_first().ok_or("no command")?;
    let mut options = Options {
        count: 5,
        seconds: 120,
        ..Options::default()
    };
    let mut peer = None;
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        let mut value = |name: &str| {
            words
                .next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        let number = |name: &str, text: String| {
            text.parse::<u64>()
                .map_err(|_| format!("{name} needs a number, not {text:?}"))
        };
        match word.as_str() {
            "--key" => options.key = Some(PathBuf::from(value("--key")?)),
            "--count" => options.count = number("--count", value("--count")?)? as usize,
            "--warm" => options.warm = true,
            "--relay-only" => options.relay_only = true,
            "--seconds" => options.seconds = number("--seconds", value("--seconds")?)?,
            "--drop-after" => {
                options.drop_after = Some(number("--drop-after", value("--drop-after")?)?)
            }
            "--drop-for" => options.drop_for = Some(number("--drop-for", value("--drop-for")?)?),
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            id if peer.is_none() => {
                peer = Some(
                    id.parse::<EndpointId>()
                        .map_err(|e| format!("{id:?} is not an endpoint ID: {e}"))?,
                )
            }
            extra => return Err(format!("one endpoint ID only, not also {extra:?}")),
        }
    }
    if options.drop_after.is_some() != options.drop_for.is_some() {
        return Err("give --drop-after and --drop-for together".into());
    }
    let peer = || peer.ok_or_else(|| format!("{command} needs an ENDPOINT_ID"));
    match command.as_str() {
        "listen" => Ok(Run::Listen(options)),
        "dial" => Ok(Run::Dial(peer()?, options)),
        "hold" => Ok(Run::Hold(peer()?, options)),
        other => Err(format!("unknown command {other:?}")),
    }
}

/// Runs the spike with the words of its command line, with no program
/// name. The binary `iroh-spike` calls it, and so does the probe build
/// of riff that `riff-size.sh` makes.
pub async fn main(args: Vec<String>) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let run = match parse(&args) {
        Ok(run) => run,
        Err(why) => {
            eprintln!("iroh-spike: {why}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let done = match run {
        Run::Listen(options) => listen(options).await,
        Run::Dial(peer, options) => dial(peer, options).await,
        Run::Hold(peer, options) => hold(peer, options).await,
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("iroh-spike: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Binds an endpoint with the defaults of n0: the DNS discovery of n0
/// (publish and resolve) and the public relays of n0. Then it waits
/// for the home relay.
async fn bind(key: SecretKey, relay_only: bool) -> Result<(Endpoint, Duration)> {
    let start = Instant::now();
    let mut builder = Endpoint::builder(presets::N0)
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()]);
    if relay_only {
        builder = builder.clear_ip_transports();
    }
    let endpoint = builder.bind().await?;
    if timeout(ONLINE_WAIT, endpoint.online()).await.is_err() {
        println!(
            "not online after {} s: no home relay yet",
            ONLINE_WAIT.as_secs()
        );
    }
    Ok((endpoint, start.elapsed()))
}

async fn listen(options: Options) -> Result<()> {
    let key = match &options.key {
        Some(path) => load_or_make_key(path)?,
        None => SecretKey::generate(),
    };
    let (endpoint, up) = bind(key.clone(), false).await?;
    println!("endpoint {}", endpoint.id());
    println!("online in {}", ms(up));
    for relay in endpoint.addr().relay_urls() {
        println!("home relay {relay}");
    }
    println!("ready: iroh-spike dial {}", endpoint.id());
    let seq = Arc::new(AtomicU64::new(1));
    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let (key, seq) = (key.clone(), seq.clone());
                tokio::spawn(async move {
                    if let Err(why) = serve(incoming, key, seq).await {
                        println!("a connection failed: {why}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    endpoint.close().await;
    Ok(())
}

/// Answers one connection: the first stream holds the record of the
/// dialer, and each next stream is a ping.
async fn serve(incoming: Incoming, key: SecretKey, seq: Arc<AtomicU64>) -> Result<()> {
    let conn = incoming.await?;
    let peer = conn.remote_id();
    let who = peer.fmt_short();
    let (mut send, mut recv) = conn.accept_bi().await?;
    // The size check comes first: the stream gives no more than MAX.
    let got = match recv.read_to_end(record::MAX).await {
        Ok(bytes) => record::check(&bytes, peer).map_err(|why| why.to_string()),
        Err(why) => Err(why.to_string()),
    };
    let got = match got {
        Ok(got) => got,
        Err(why) => {
            println!("{who}: refused a record: {why}");
            conn.close(1u32.into(), b"refused");
            return Ok(());
        }
    };
    println!(
        "{who}: accepted a {:?} record, seq {}: {}",
        got.kind, got.seq, got.body
    );
    let body = format!("{} accepted seq {}", host(), got.seq);
    let answer = record::sign(
        &key,
        Kind::Status,
        seq.fetch_add(1, Ordering::SeqCst),
        &body,
    )?;
    send.write_all(&answer).await?;
    send.finish()?;
    // Each ping has a task of its own: a dialer drops a ping that it
    // waited for too long, and the next pings go on.
    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
        tokio::spawn(async move {
            let ping = recv.read_to_end(8).await?;
            send.write_all(&ping).await?;
            send.finish()?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
    }
    println!("{who}: closed: {}", conn.closed().await);
    Ok(())
}

/// The path that a connection uses now.
struct Selected {
    direct: bool,
    addr: String,
    rtt: Duration,
}

impl Selected {
    fn of(conn: &Connection) -> Option<Selected> {
        conn.paths()
            .iter()
            .find(|path| path.is_selected())
            .map(|path| Selected {
                direct: path.is_ip(),
                addr: path.remote_addr().to_string(),
                rtt: path.rtt(),
            })
    }

    fn describe(path: &Option<Selected>) -> String {
        match path {
            Some(p) => format!("{} (rtt {})", Selected::name(path), ms(p.rtt)),
            None => Selected::name(path),
        }
    }

    /// The kind and the address of a path, with no time.
    fn name(path: &Option<Selected>) -> String {
        match path {
            Some(p) if p.direct => format!("direct {}", p.addr),
            Some(p) => format!("relay {}", p.addr),
            None => "no path".into(),
        }
    }
}

/// Sends a record of this endpoint, and checks the record of the
/// answer.
async fn exchange(conn: &Connection, key: &SecretKey, seq: u64) -> Result<Record> {
    let (mut send, mut recv) = conn.open_bi().await?;
    let body = format!("{} sends {seq}", host());
    send.write_all(&record::sign(key, Kind::Message, seq, &body)?)
        .await?;
    send.finish()?;
    let answer = recv.read_to_end(record::MAX).await?;
    Ok(record::check(&answer, conn.remote_id())?)
}

struct Attempt {
    up: Duration,
    connect: Duration,
    exchange: Duration,
    first_direct: bool,
    direct: Option<Duration>,
    path: Option<Selected>,
}

async fn attempt(endpoint: &Endpoint, up: Duration, peer: EndpointId, seq: u64) -> Result<Attempt> {
    let start = Instant::now();
    let conn = timeout(CONNECT_WAIT, endpoint.connect(peer, ALPN))
        .await
        .map_err(|_| format!("no connect in {} s", CONNECT_WAIT.as_secs()))??;
    let connect = start.elapsed();
    let first_direct = Selected::of(&conn).is_some_and(|p| p.direct);
    // Look for the first direct path while the records go.
    let watch = {
        let conn = conn.clone();
        tokio::spawn(async move {
            while start.elapsed() < DIRECT_WAIT {
                if Selected::of(&conn).is_some_and(|p| p.direct) {
                    return Some(start.elapsed());
                }
                sleep(Duration::from_millis(5)).await;
            }
            None
        })
    };
    let sent = Instant::now();
    let answer = exchange(&conn, endpoint.secret_key(), seq).await?;
    let exchange = sent.elapsed();
    println!("  the answer passed each check: {}", answer.body);
    let direct = match first_direct {
        true => Some(connect),
        false => watch.await?,
    };
    let path = Selected::of(&conn);
    conn.close(0u32.into(), b"done");
    Ok(Attempt {
        up,
        connect,
        exchange,
        first_direct,
        direct,
        path,
    })
}

async fn dial(peer: EndpointId, options: Options) -> Result<()> {
    let warm = match options.warm {
        true => Some(bind(SecretKey::generate(), options.relay_only).await?),
        false => None,
    };
    let mut connects = Vec::new();
    let mut directs = Vec::new();
    let mut exchanges = Vec::new();
    let mut failed = 0;
    for n in 1..=options.count as u64 {
        let fresh = match &warm {
            Some(_) => None,
            None => Some(bind(SecretKey::generate(), options.relay_only).await?),
        };
        let (endpoint, up) = warm.as_ref().or(fresh.as_ref()).expect("an endpoint");
        match attempt(endpoint, *up, peer, n).await {
            Ok(a) => {
                println!(
                    "attempt {n}: online in {}, connect {} on {}, records {}, direct {}, now {}",
                    ms(a.up),
                    ms(a.connect),
                    if a.first_direct { "direct" } else { "relay" },
                    ms(a.exchange),
                    a.direct.map_or("never".into(), ms),
                    Selected::describe(&a.path),
                );
                connects.push(a.connect);
                exchanges.push(a.exchange);
                directs.extend(a.direct);
            }
            Err(why) => {
                failed += 1;
                println!("attempt {n}: failed: {why}");
            }
        }
        if let Some((endpoint, _)) = fresh {
            endpoint.close().await;
        }
    }
    if let Some((endpoint, _)) = warm {
        endpoint.close().await;
    }
    let kind = match (options.warm, options.relay_only) {
        (false, false) => "cold",
        (true, false) => "warm",
        (false, true) => "cold, relay only",
        (true, true) => "warm, relay only",
    };
    println!(
        "summary ({kind}): {} of {} connects",
        connects.len(),
        options.count
    );
    for (name, times) in [
        ("connect", &connects),
        ("records", &exchanges),
        ("direct", &directs),
    ] {
        if let Some((least, median, most)) = spread(times) {
            println!(
                "  {name}: least {}, median {}, most {} ({} of {})",
                ms(least),
                ms(median),
                ms(most),
                times.len(),
                options.count
            );
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} of {} connects failed", options.count).into()),
    }
}

/// Connects and swaps records, again and again until it works or the
/// time ends. Each try waits `wait` at most.
async fn connect(
    endpoint: &Endpoint,
    peer: EndpointId,
    seq: u64,
    wait: Duration,
    end: Instant,
) -> Option<Connection> {
    while Instant::now() < end {
        match timeout(wait, endpoint.connect(peer, ALPN)).await {
            Ok(Ok(conn)) => match exchange(&conn, endpoint.secret_key(), seq).await {
                Ok(_) => return Some(conn),
                Err(why) => println!("{}: the records failed: {why}", stamp()),
            },
            Ok(Err(why)) => println!("{}: no connect: {why}", stamp()),
            Err(_) => println!("{}: no connect in {}", stamp(), ms(wait)),
        }
        sleep(Duration::from_secs(1)).await;
    }
    None
}

async fn ping(conn: &Connection, n: u64) -> Result<()> {
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(&n.to_be_bytes()).await?;
    send.finish()?;
    let echo = recv.read_to_end(8).await?;
    match echo == n.to_be_bytes() {
        true => Ok(()),
        false => Err("the echo is not the ping".into()),
    }
}

/// `drop.sh` sets this variable. `hold` takes a link down only with
/// it, so that it never takes down the link of a host.
const OWN_NETNS: &str = "IROH_SPIKE_OWN_NETNS";

async fn hold(peer: EndpointId, options: Options) -> Result<()> {
    let link = match options.drop_after.zip(options.drop_for) {
        Some(_) if std::env::var_os(OWN_NETNS).is_none() => {
            return Err("--drop-after takes a link down: run it with drop.sh".into());
        }
        Some(times) => Some((Link::find()?, times)),
        None => None,
    };
    let (endpoint, up) = bind(SecretKey::generate(), false).await?;
    println!("{}: online in {}", stamp(), ms(up));
    let start = Instant::now();
    let end = start + Duration::from_secs(options.seconds);
    let mut seq = 1;
    let mut conn = connect(&endpoint, peer, seq, CONNECT_WAIT, end)
        .await
        .ok_or("no connect before the end")?;
    println!("{}: connected in {}", stamp(), ms(start.elapsed()));
    // The link goes down and up in a task of its own, so that a
    // connect that waits does not stop it.
    let link_up = Arc::new(Mutex::new(None::<Instant>));
    let link_task = link.map(|(mut link, (after, down))| {
        let link_up = link_up.clone();
        tokio::spawn(async move {
            sleep_until((start + Duration::from_secs(after)).into()).await;
            link.take_down()?;
            println!("{}: the link is down", stamp());
            sleep_until((start + Duration::from_secs(after + down)).into()).await;
            link.bring_up()?;
            *link_up.lock().expect("the lock") = Some(Instant::now());
            println!("{}: the link is up", stamp());
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
    });
    let first = Selected::of(&conn);
    let mut path = Selected::name(&first);
    println!("{}: path {}", stamp(), Selected::describe(&first));
    let mut lost: Option<Instant> = None;
    let mut gaps = Vec::new();
    let mut after_up = Vec::new();
    let (mut sent, mut answered) = (0, 0);
    let mut tick = tokio::time::interval(PING_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    while Instant::now() < end {
        tick.tick().await;
        let now = Selected::of(&conn);
        if Selected::name(&now) != path {
            path = Selected::name(&now);
            println!("{}: path {}", stamp(), Selected::describe(&now));
        }
        if let Some(why) = conn.close_reason() {
            println!("{}: the connection closed: {why}", stamp());
            seq += 1;
            let Some(again) = connect(&endpoint, peer, seq, REDIAL_WAIT, end).await else {
                break;
            };
            conn = again;
            println!("{}: a new connection", stamp());
        }
        sent += 1;
        let at = Instant::now();
        let ok = matches!(timeout(PING_WAIT, ping(&conn, sent)).await, Ok(Ok(())));
        match (ok, lost) {
            (true, Some(since)) => {
                answered += 1;
                lost = None;
                gaps.push(since.elapsed());
                let after = link_up.lock().expect("the lock").map(|t| t.elapsed());
                after_up.extend(after);
                println!(
                    "{}: ping {sent} answered again, {} after the first lost ping{}",
                    stamp(),
                    ms(since.elapsed()),
                    after.map_or(String::new(), |t| format!(
                        ", {} after the link came up",
                        ms(t)
                    )),
                );
            }
            (true, None) => answered += 1,
            (false, None) => {
                lost = Some(at);
                println!("{}: ping {sent} got no answer", stamp());
            }
            (false, Some(_)) => {}
        }
    }
    if let Some(task) = link_task {
        if !task.is_finished() {
            println!("{}: the end came before the link came up", stamp());
        }
        task.abort();
        if let Ok(Err(why)) = task.await {
            println!("the link failed: {why}");
        }
    }
    conn.close(0u32.into(), b"done");
    let _ = timeout(Duration::from_secs(3), endpoint.close()).await;
    println!(
        "summary: {answered} of {sent} pings answered, {} gaps",
        gaps.len()
    );
    if let Some((least, median, most)) = spread(&gaps) {
        println!(
            "  gap: least {}, median {}, most {}",
            ms(least),
            ms(median),
            ms(most)
        );
    }
    if let Some((least, _, most)) = spread(&after_up) {
        println!(
            "  after the link came up: least {}, most {}",
            ms(least),
            ms(most)
        );
    }
    match lost {
        None => Ok(()),
        Some(since) => Err(format!("no answer for the last {}", ms(since.elapsed())).into()),
    }
}

/// The link of the default route of this network namespace. `hold`
/// takes it down and brings it up again with `ip`. Only a namespace of
/// its own allows it: see `drop.sh`.
struct Link {
    dev: String,
    /// Each route of the link, as the words of `ip route show`. A link
    /// that goes down loses its routes, and an address with
    /// `noprefixroute` gets no route back at the next up.
    routes: Vec<(&'static str, Vec<String>)>,
}

/// The flags of `ip route show` that `ip route replace` does not take.
const STATE_FLAGS: &[&str] = &[
    "linkdown",
    "dead",
    "offload",
    "trap",
    "rt_offload",
    "rt_trap",
];

impl Link {
    fn find() -> Result<Link> {
        let dev = ip(&["-o", "route", "show", "default"])?
            .lines()
            .find_map(|line| route_dev(line).map(str::to_owned))
            .ok_or("no default route")?;
        let mut routes = Vec::new();
        for family in ["-4", "-6"] {
            for line in ip(&[family, "-o", "route", "show", "dev", &dev])?.lines() {
                routes.push((family, route_words(line, &dev)));
            }
        }
        // The default routes go last: each needs the route to its gateway.
        routes.sort_by_key(|(_, words)| words.first().is_some_and(|w| w == "default"));
        // Keep the IPv6 addresses when the link goes down.
        let keep = format!("/proc/sys/net/ipv6/conf/{dev}/keep_addr_on_down");
        if let Err(why) = fs::write(&keep, "1") {
            println!("cannot write {keep}: {why}");
        }
        Ok(Link { dev, routes })
    }

    fn take_down(&mut self) -> Result<()> {
        ip(&["link", "set", "dev", &self.dev, "down"])?;
        Ok(())
    }

    fn bring_up(&mut self) -> Result<()> {
        ip(&["link", "set", "dev", &self.dev, "up"])?;
        for (family, words) in &self.routes {
            let mut args = vec![*family, "route", "replace"];
            args.extend(words.iter().map(String::as_str));
            ip(&args)?;
        }
        Ok(())
    }
}

/// The device of a line of `ip route show`.
fn route_dev(line: &str) -> Option<&str> {
    let mut words = line.split_whitespace();
    words.find(|w| *w == "dev")?;
    words.next()
}

/// The words of a line of `ip -o route show dev DEV` for
/// `ip route replace`: with the device, with no state flag.
fn route_words(line: &str, dev: &str) -> Vec<String> {
    let mut words: Vec<String> = line
        .split_whitespace()
        .filter(|w| !STATE_FLAGS.contains(w))
        .map(str::to_owned)
        .collect();
    if !words.iter().any(|w| w == "dev") {
        words.extend(["dev".to_owned(), dev.to_owned()]);
    }
    words
}

fn ip(args: &[&str]) -> Result<String> {
    let out = Command::new("ip").args(args).output()?;
    if !out.status.success() {
        return Err(format!(
            "ip {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?)
}

/// Reads the secret key from `path`, or makes one and writes it there
/// with mode 0600.
fn load_or_make_key(path: &Path) -> Result<SecretKey> {
    match fs::read(path) {
        Ok(bytes) => {
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| format!("{} does not hold 32 bytes", path.display()))?;
            Ok(SecretKey::from_bytes(&bytes))
        }
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?
                .write_all(&key.to_bytes())?;
            Ok(key)
        }
        Err(why) => Err(why.into()),
    }
}

fn host() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|_| "a host".into())
}

/// The wall clock in seconds, so that the lines of two hosts line up.
fn stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:.3}", now.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn dial_takes_an_endpoint_id_and_its_flags() {
        let id = SecretKey::from_bytes(&[1; 32]).public().to_string();
        let Ok(Run::Dial(peer, options)) =
            parse(&words(&format!("dial {id} --count 3 --warm --relay-only")))
        else {
            panic!("not a dial");
        };
        assert_eq!(peer.to_string(), id);
        assert_eq!(options.count, 3);
        assert!(options.warm && options.relay_only);
    }

    #[test]
    fn a_bad_command_line_names_its_fault() {
        let id = SecretKey::from_bytes(&[1; 32]).public().to_string();
        let fault = |line: &str| parse(&words(line)).err().expect("a fault");
        assert!(fault("dial").contains("needs an ENDPOINT_ID"));
        assert!(fault("dial nope").contains("is not an endpoint ID"));
        assert!(fault(&format!("dial {id} --count x")).contains("needs a number"));
        assert!(fault(&format!("hold {id} --drop-after 5")).contains("together"));
        assert!(fault("listen --nope").contains("unknown flag"));
        assert!(fault("talk").contains("unknown command"));
    }

    #[test]
    fn a_route_line_gives_the_words_of_a_replace() {
        let line = "default via 192.168.1.1 proto dhcp metric 600 linkdown";
        assert_eq!(
            route_words(line, "wlp10s0"),
            words("default via 192.168.1.1 proto dhcp metric 600 dev wlp10s0")
        );
        let line = "default via 192.168.1.1 dev wlp10s0 proto dhcp metric 600";
        assert_eq!(route_dev(line), Some("wlp10s0"));
        assert_eq!(route_words(line, "wlp10s0"), words(line));
        assert_eq!(route_dev("192.168.1.0/24 proto kernel"), None);
    }

    #[test]
    fn a_key_file_keeps_the_endpoint_id() {
        let dir = std::env::temp_dir().join(format!("iroh-spike-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key");
        let first = load_or_make_key(&path).unwrap();
        let again = load_or_make_key(&path).unwrap();
        assert_eq!(first.public(), again.public());
        let mode = fs::metadata(&path).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        fs::write(&path, b"short").unwrap();
        assert!(load_or_make_key(&path).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
