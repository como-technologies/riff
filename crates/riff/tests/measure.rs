//! The measure of the one wait rule (01M3WRD933ESXF33WDEDFCRFB8): the
//! time of a `status` call and of a `claim` call while 8 sessions post.
//!
//! The test is ignored: it measures, and it checks nothing. Run it on
//! a quiet machine:
//!
//! ```sh
//! cargo test -p riff --test measure -- --ignored --nocapture
//! ```
//!
//! The store is in memory, and each write of a chunk of the log takes
//! [`WRITE`], as a write to a bucket does. A `status` is a signal: it
//! does not wait for the writer. A `claim` is a command: it waits until
//! the writer is done with its entry, so it takes about one or two
//! writes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, RiffState, Status};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::{Loaded, Memory, Store, StoreError, Version};

/// The time of one write of a chunk of the log.
const WRITE: Duration = Duration::from_millis(30);

/// The number of sessions that post.
const POSTERS: usize = 8;

/// The number of `status` calls and of `claim` calls that the test
/// measures.
const CALLS: usize = 200;

/// A store in memory whose chunk writes take [`WRITE`].
#[derive(Default)]
struct Slow(Memory);

impl Store for Slow {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.0.load(name)
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.0.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        Box::pin(async move {
            if name.starts_with("log/") {
                tokio::time::sleep(WRITE).await;
            }
            self.0.save(name, bytes, known).await
        })
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        self.0.delete(name)
    }
}

fn session(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// The middle, the 95th of 100 and the longest of `times`, in
/// milliseconds.
fn summary(mut times: Vec<Duration>) -> String {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |part: usize| ms(times[(times.len() - 1) * part / 100]);
    format!(
        "p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        at(50),
        at(95),
        at(100)
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measure: run it by hand, see the module docs"]
async fn a_status_and_a_claim_while_8_sessions_post() {
    let mut config = Config::default();
    config.lease.wait = Duration::from_millis(10);
    let service = Service::load(config, Arc::new(Slow::default()))
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = service.router();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let api = Api::new(&format!("http://{addr}"));

    let probe = session("probe");
    let thread: ThreadName = probe.default_thread().unwrap();
    api.register(&probe).await.unwrap();
    // The first session of the user is the lead: it resumes the riff.
    api.set_riff(&probe, RiffState::Running).await.unwrap();

    let mut posters = Vec::new();
    for n in 0..POSTERS {
        let (api, thread) = (api.clone(), thread.clone());
        let me = session(&format!("p{n}"));
        api.register(&me).await.unwrap();
        posters.push(tokio::spawn(async move {
            loop {
                api.post(&me, Some(&thread), &[], "a post", Kind::Message)
                    .await
                    .unwrap();
            }
        }));
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut statuses = Vec::new();
    let mut claims = Vec::new();
    for n in 0..CALLS {
        let status = Status {
            step: format!("step {n}"),
        };
        let began = Instant::now();
        api.status(&probe, &status).await.unwrap();
        statuses.push(began.elapsed());

        let item = format!("issue-{n}");
        let began = Instant::now();
        api.claim(&probe, &thread, &item).await.unwrap();
        claims.push(began.elapsed());
        api.release(&probe, &thread, &item).await.unwrap();
    }
    for poster in posters {
        poster.abort();
    }
    println!(
        "{POSTERS} sessions post, a chunk write takes {} ms, {CALLS} calls of each kind",
        WRITE.as_millis()
    );
    println!("status: {}", summary(statuses));
    println!("claim:  {}", summary(claims));
}
