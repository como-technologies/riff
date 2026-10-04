# Review link-00: the crates for the client link

Issue: #420. Base: origin/main a67f384.

Checked on 2026-10-03. Sources: the crates.io API (crate, versions,
`reverse_dependencies`, `dependencies`), the `.crate` files on
static.crates.io, the GitHub API, and the RustSec advisory database
(`rustsec/advisory-db`, `crates/<name>`). "Reverse deps" is the
`meta.total` of crates.io: it counts each crate that names the crate,
with each kind of dependency.

## Summary

riff needs almost no new crate for the link. `reqwest` 0.13, `tokio`,
`tower` and `axum` are in the build already, and they give the time
limits, the TCP keep-alive, the timers and the watch channel of a
state. No crate gives the rule of riff: which fault gets a new try,
which call is safe to send two times, and what a person sees. That
rule stays in riff. One crate is worth its cost: `sse-stream` parses
server-sent events to the full specification, with `id:`, and adds only
itself to the build. Do not use `backoff` (unmaintained, RUSTSEC-2025-0012)
or `reqwest-eventsource` (it pulls a second `reqwest`, 0.12).

## What riff has today

- `crates/riff/src/api.rs`: `busy_waits` (250 ms, double, at most 5 s,
  60 s in all, no jitter), the loops for 503 and refused connects,
  `follow` (opens a stream again when it ends), and one client of its
  own for streams (`pool_max_idle_per_host(0)`). No client sets
  `connect_timeout`, `read_timeout` or `tcp_keepalive`.
- `crates/riff-server/src/lib.rs`: `watch` and `tail_thread` send
  server-sent events with axum `Sse`, `KeepAlive::default()` (a comment
  each 15 s) and a first comment `: ready`. No event has an `id:`. The
  server does not read `Last-Event-ID`.
- The wire is SSE, not JSON lines. The client parses only the `data:`
  lines, with a parser of about 20 lines.
- `watch` has a cursor on the server: at a new connect it sends the
  newest unread wake (`State::missed`, from the read cursor). `tail`
  has no cursor: it loses each message that comes during a break.
- No call carries an ID of the call. The engine `key` is the thumbprint
  of the device key.

## The parts

1. A new try with a wait that grows.
2. Time limits: to connect, for a call, between two reads of a stream.
3. A call that is safe to send two times.
4. A stream that connects again and goes on from a cursor.
5. Heartbeats: a sign of life in each direction.
6. The state of the connection that a person sees.

## Crates

Column key. "New crates" counts the crates that are not in `Cargo.lock`
today, the crate itself included. `Cargo.lock` lists the crates of each
target, so a crate only for `wasm32` adds a line to `Cargo.lock` (and
to the audit) but not to the build of riff. "Unsafe" counts the code
lines with the word `unsafe` in `src/` of the `.crate` file. Build time
was not measured: the lines of `src/` give the size.

### Part 1: a new try with a wait that grows

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `backon` 1.6.0 | Retry for a closure or future. Exponential, constant, Fibonacci waits, jitter, `when` (classify), `notify` (a hook at each try), total limit. | 2022-04-12 / 2025-10-18 | Xuanwo (one person); repo active (push 2026-06-19), 1,060 stars | 243 | Apache-2.0 | 6 lines (pin projection by hand) | 1 with `default-features = false, features = ["std", "tokio-sleep"]`; the default features add `gloo-timers` to `Cargo.lock` (wasm only). 3,196 lines | Good. It replaces about 20 lines of riff. |
| `tokio-retry` 0.3.2 | Retry a future with a strategy iterator: exponential, fixed, Fibonacci, jitter. | 2017-03-05 / 2026-06-09 (no release 2021 to 2026) | djc (Dirkjan Ochtman) took it over in 2026 | 118 | MIT | 0 | 1 (`rand` 0.10 is in the lock). 548 lines | Good, small. Revived only this year. |
| `tokio-retry2` 0.9.1 | A fork of `tokio-retry` with a classify of errors. | 2024-09-04 / 2026-01-11 | naomijub (one person) | 20 | MIT | 0 | 2 (adds `pin-project`). 1,897 lines | Fits; few users. |
| `exponential-backoff` 2.1.1 | An iterator of waits with jitter. No loop. | 2018-07-11 / 2026-09-14 | yoshuawuyts | 25 | MIT OR Apache-2.0 | 0 | 1 (`fastrand` is in the lock). 417 lines | Fits; it gives only what `busy_waits` gives. |
| `tower` 0.5.3, features `retry`, `timeout` | `Retry` layer with a `Policy`, `ExponentialBackoff`, retry budget; `Timeout` layer. | 2016-12-23 / 2026-01-12 | tower-rs organization | 5,408 | MIT | 0, `forbid(unsafe_code)` | 0 (in the lock through `reqwest`; the features need only `tokio/time`) | Poor. A `Service` shape for each call; a stream body cannot go again. |
| `reqwest` 0.13.5 `retry` module | `ClientBuilder::retry`: a classify, a budget of 20 % more load, at most N tries. | in the build | seanmonstar, hyperium | (reqwest) | MIT OR Apache-2.0 | not checked | 0 | Poor. No wait between tries (`// TODO? backoff` in `src/retry.rs`). Its default (a new try on an h2 refusal that is safe) is on already. |
| `reqwest-middleware` 0.5.2 + `reqwest-retry` 0.9.1 + `retry-policies` 0.5.2 | A middleware chain for `reqwest`; a retry middleware with exponential waits and a classify of transient errors. | 2021-08-12 / 2026-05-19, 2026-02-05, 2026-05-09 | TrueLayer organization | 543 / 213 / 26 | MIT OR Apache-2.0 | 0 | 3, plus `wasmtimer` in `Cargo.lock` (wasm only). 2,231 lines | Poor. It changes the client type to `ClientWithMiddleware`, it tries again only up to the headers, and it cannot show a state line. |
| `backoff` 0.4.0 | Exponential waits, sync and async. | 2017-05-28 / 2021-12-14 | ihrwein; no push since 2024-02 | 287 | MIT/Apache-2.0 | 0 | 4 or more (itself, `instant`, the `rand` 0.8 family) | No. RUSTSEC-2025-0012 (unmaintained); `instant` is RUSTSEC-2024-0384. |
| `again` 0.1.2, `futures-retry` 0.6.0, `tryhard` 0.5.2 | Small retry helpers. | last 2020-05, 2021-01, 2025-06 | softprops; mexus (GitLab); EmbarkStudios | 4 / 15 / 14 | MIT; MIT/Apache-2.0; MIT OR Apache-2.0 | not checked | not checked | No: old or few users. |

### Part 2: time limits

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `reqwest` 0.13.5 `ClientBuilder` | `connect_timeout`, `timeout` (whole call), `read_timeout` (each read: fits a stream), `tcp_keepalive`, `tcp_keepalive_interval`, `tcp_keepalive_retries`, `tcp_user_timeout`, `http2_keep_alive_*`. | in the build | seanmonstar, hyperium | (reqwest) | MIT OR Apache-2.0 | not checked | 0 | Good. riff sets none of them today. |
| `tokio` `time::timeout` | A limit on any future. | in the build | tokio-rs | (tokio) | MIT | (tokio) | 0 | Good, for a limit over many tries. |
| `tower` `timeout` | A `Timeout` layer. | see part 1 | tower-rs | 5,408 | MIT | 0 | 0 | Not needed: `reqwest` and `tokio` do it. |

### Part 3: a call that is safe to send two times

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `axum-idempotent` 0.4.0 | An axum layer: it keeps the reply of each request in a session store and sends it again for the same request. | 2025-02-07 / 2026-09-23 | jimmielovell (one person) | 0 (254 downloads in 90 days) | MIT | not checked | at least 3 (`blake3`, `ruts`, `serde_bytes`) | No. Few users. Its store is not the engine log of riff, so a restart of the server forgets each key. |
| (no client crate) | The client part is one header with a random ID. `getrandom` is in the build. | | | | | | 0 | |

A search of crates.io for "idempotency" and "idempotent" found no
other crate for HTTP.

### Part 4: a stream that connects again and goes on from a cursor

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `sse-stream` 0.3.0 | Parses SSE from an `http_body::Body` or a byte stream (`SseStream::from_bytes_stream`). Gives `event`, `data`, `id`, `retry`. Also a body for a server. No reconnect. | 2025-03-21 / 2026-09-18 | 4t145 (one person; `rmcp` uses it for its HTTP transport) | 33 (12 M downloads in 90 days) | MIT OR Apache-2.0 | 1 line (`from_utf8_unchecked`, after a check) | 1. Each dependency is in the lock. 1,517 lines | Good parser. riff keeps `follow` for the reconnect. |
| `eventsource-stream` 0.2.3 | Parses SSE from a byte stream. No reconnect. | 2020-06-27 / 2022-02-17 | jpopesculian (one person); no push since 2024-08 | 317 | MIT OR Apache-2.0 | 1 line | 3 (`nom` 7, `minimal-lexical`, itself). 936 lines | Fits, but no release for 4 years. |
| `reqwest-eventsource` 0.6.0 | An `EventSource` on `reqwest`: reconnect with waits, `Last-Event-ID`. | 2020-06-28 / 2024-03-29 | jpopesculian; no push since 2024-06 | 163 | MIT OR Apache-2.0 | 0 | many: it needs `reqwest` 0.12, so a second `reqwest`, `hyper-rustls`, and so on; also `thiserror` 1, `futures-timer`, `nom` 7 | No. |
| `eventsource-client` 0.18.0 | An SSE client with reconnect, waits and `Last-Event-ID`, on its own transport. | 2019-07-18 / 2026-08-10 | LaunchDarkly (organization, release bot) | 35 | Apache-2.0 | 0 | at least 3 (`launchdarkly-sdk-transport`, `pin-project`, itself); its default `hyper` feature adds more (not counted). Needs Rust 1.95. | No: a second HTTP stack beside `reqwest`. |
| `reqwest-sse` 0.2.0 | SSE from a `reqwest::Response`. | 2025-07-23 / 2026-05-08 | vvvinceocam (one person) | 4 | MIT | 0 | 3 (`async-stream`, its macro crate, itself) | No: young, few users. |
| `tokio-util` 0.7.19 `codec::LinesCodec` | Lines from a byte stream. | in the build | tokio-rs | 7,586 | MIT | 29 lines (whole crate) | 0 | Only for JSON lines. riff sends SSE. |

### Part 5: heartbeats

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `axum` 0.8.9 `sse::KeepAlive` | The server sends a comment on a quiet stream; default each 15 s. | in the build | tokio-rs | (axum) | MIT | not checked | 0 | In use. The client must also time out on it. |
| `reqwest` `read_timeout`, `tcp_keepalive` | The client ends a stream with no byte for N seconds; the OS probes a quiet TCP connection. | in the build | | | | | 0 | Good: `read_timeout` of about 3 times 15 s finds a dead stream after a sleep. |
| `tokio::time::interval` | A timer for a heartbeat call. | in the build | tokio-rs | | MIT | | 0 | Good. |

No crate gives a heartbeat of an application over HTTP.

### Part 6: the state of the connection that a person sees

| Crate | What it does | First release / last release | Maintainer | Reverse deps | License | Unsafe | New crates in our build | Fit for riff |
|---|---|---|---|---|---|---|---|---|
| `tokio::sync::watch` | One value, many readers, each reader sees the newest value. | in the build | tokio-rs | | MIT | | 0 | Good: the link writes `Up`, `Waiting { since }`, `Refused`; each command and the status line read it. |
| `failsafe` 1.3.0 | A circuit breaker: it refuses calls after many faults. | 2018-09-03 / 2024-07-05 | dmexe (one person) | 19 | MIT | 1 line | about 5 (`rand` 0.8 family, `pin-project`, itself) | No. riff must try again, not refuse, and it must show the state. |
| `recloser` 1.4.0 | A circuit breaker. | 2019-07-03 / 2026-06-20 | lerouxrgd (one person) | 8 | MIT | 10 lines | not counted | No, for the same reason. |
| `tower-resilience` 0.13.0 | Many tower layers: retry, circuit breaker, health check, reconnect. | 2025-10-08 / 2026-08-22 | joshrotenberg (one person) | 7 | MIT OR Apache-2.0 | not checked | 2 or more (one crate for each layer) | No: young, few users, `Service` shape. |

## How other tools do it

| Tool | How | Lesson for riff |
|---|---|---|
| mosh ([mosh.org](https://mosh.org/), [paper](https://mosh.org/mosh-paper.pdf)) | The state sync protocol over UDP. The client roams to a new address. A heartbeat goes each 3 s. The top line says "Last contact N seconds ago". | Show the time of the last contact. Do not hide a gap. |
| OpenSSH ([ssh_config(5)](https://man.openbsd.org/ssh_config)) | `ServerAliveInterval` sends a message inside the encrypted channel. After `ServerAliveCountMax` (default 3) with no reply, ssh ends the connection. `TCPKeepAlive` alone can be spoofed and is slow. | A heartbeat of the application, with a count of misses, finds a dead link. TCP keep-alive does not. |
| gRPC ([connectivity states](https://grpc.github.io/grpc/core/md_doc_connectivity-semantics-and-api.html), [retry](https://grpc.io/docs/guides/retry/), [keepalive](https://grpc.io/docs/guides/keepalive/)) | A channel has five states: IDLE, CONNECTING, READY, TRANSIENT_FAILURE, SHUTDOWN. A retry policy names the status codes that get a new try, with exponential waits and jitter, and a limit of tries. A call that never left the client gets a "transparent" new try. Keep-alive pings have a minimum interval that the server enforces. | One small state set, readable by each user of the link. A fixed list of faults that get a new try. |
| Kubernetes client-go ([API concepts: efficient detection of changes](https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes)) | A reflector lists, then watches from the `resourceVersion` of the list. When the watch ends, it watches again from the last version. On `410 Gone` it lists again. Waits grow with jitter, up to about 30 s. | Each stream has a cursor. When the cursor is too old, the server says so, and the client loads the full state. |
| HTML server-sent events ([WHATWG spec](https://html.spec.whatwg.org/multipage/server-sent-events.html)) | Each event can have an `id:`. At a reconnect, the client sends `Last-Event-ID`. The server can set the wait with `retry:`. A 204 reply stops the reconnect. | riff has SSE already. Add `id:` on the server and `Last-Event-ID` in `follow`: the cursor needs no new wire. |
| Stripe ([idempotent requests](https://docs.stripe.com/api/idempotent_requests)) and the IETF draft ([draft-ietf-httpapi-idempotency-key-header-07](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/)) | The client sends an `Idempotency-Key` header with a random value. The server keeps the first result for at least 24 hours and sends it again for the same key. The same key with other parameters is an error. The IETF draft is at -07 and expired on 2026-04-18; it is not an RFC. | Use the header name `Idempotency-Key`. The server keeps the result in its log, with a time limit. |
| NATS clients ([reconnect](https://docs.nats.io/using-nats/developer/connecting/reconnect)) | Reconnect with a wait and jitter. Outgoing messages go to a buffer while the link is down. A ping each 2 minutes; 2 missed pongs end the link. Events tell the program: disconnected, reconnected, closed. | Tell the program each change of the state as an event. |
| Tailscale ([how Tailscale works](https://tailscale.com/blog/how-tailscale-works)) | The client watches the network of the host (the `netmon` package in the source) and makes new connections at once after a change of the network. Its control link is a long poll. | Do not wait for a time-out after a sleep or a new Wi-Fi. When the clock jumps or a read fails, connect again at once. |

## Recommendations

| Part | Use | Why |
|---|---|---|
| 1. A new try with a wait that grows | Write it in riff. Keep `busy_waits` and add jitter with `rand`, which is in the build. | The waits are about 20 lines with a doc test today. The hard part is the rule of riff: which fault gets a new try, the line `WAITING`, the state of part 6, and the limit of a short command. No crate knows it. `backon` is the crate to take if the loop grows: 1 new crate, Apache-2.0, 243 reverse deps. Not `backoff`: it is unmaintained. |
| 2. Time limits | Use `reqwest` (in the build): `connect_timeout` and `timeout` for calls, `read_timeout` for streams, `tcp_keepalive`. Use `tokio::time::timeout` for the limit over all tries. | 0 new crates. riff sets none of these today, so a dead connection after a sleep can hang a stream until the OS ends it. |
| 3. A call that is safe to send two times | Write it in riff: an `Idempotency-Key` header on each call that changes state, and a memory of the last keys in the engine. | No crate fits. `axum-idempotent` keeps the reply in a store that is not the engine log and has no users. The header name follows Stripe and the IETF draft. |
| 4. A stream that goes on from a cursor | Use `sse-stream` to parse the events. Keep `follow` in riff for the reconnect, and send `Last-Event-ID` from it. | The wire is SSE already. `sse-stream` reads `id:` and `retry:`, adds 1 crate and no other, is current, and `rmcp` uses it. The SSE crates with a reconnect (`reqwest-eventsource`, `eventsource-client`) bring a second HTTP stack. |
| 5. Heartbeats | Write it in riff: one heartbeat call with `tokio::time::interval`, and `read_timeout` on each stream against the 15 s keep-alive of axum. | No crate gives a heartbeat of an application over HTTP. Each needed part is in `tokio`, `reqwest` and `axum`. |
| 6. The state that a person sees | Write it in riff: an enum in a `tokio::sync::watch` channel, read by each command and the status line. | 0 new crates. The circuit-breaker crates (`failsafe`, `recloser`) refuse calls; riff must try again and show the state. |

## Checks

- `cargo audit`: run on 2026-10-03 on this tree (cargo-audit 0.22.2).
  It passes with one allowed warning (`yoke-derive` 0.8.3, yanked).
  `.cargo/audit.toml` ignores no advisory (R123). CI runs
  `just crate-audit` as its own job and each Monday.
- RustSec advisories for the crates of this review, checked on
  2026-10-03: `sse-stream`, `backon`, `tokio-retry`, `tokio-retry2`,
  `exponential-backoff`, `reqwest-middleware`, `reqwest-retry`,
  `retry-policies`, `eventsource-stream`, `reqwest-eventsource`,
  `eventsource-client`, `tower`, `failsafe` and `nom` have none.
  `backoff` has RUSTSEC-2025-0012 and its dependency `instant` has
  RUSTSEC-2024-0384, both "unmaintained". Plain `cargo audit` shows
  them as warnings, not as failures, but R123 forbids an ignore, so
  riff must not take `backoff`.
- `sse-stream` (the one recommended new crate) and its dependencies
  have no advisory. Its dependencies are in the lock today, so the
  audit result does not change. Not verified by a run with the crate
  added: this review changes no `Cargo.toml`.
- `cargo deny`: the repository has no `deny.toml`, and no job runs
  `cargo deny`. `cargo-deny` is not installed on this machine, so no
  run was made. The build item that adds `sse-stream` must also add
  `deny.toml` with these sections, and a `just` recipe and a CI step
  that run `cargo deny check`:
  - `[advisories]`: no ignore, the same rule as R123.
  - `[licenses]`: an allow list for the whole tree. It must include
    `MIT`, `Apache-2.0` and `MIT OR Apache-2.0` (the crates above),
    and each other license that `cargo deny list` shows for the tree
    today (not checked here).
  - `[bans]`: deny a second major version of `reqwest` and `hyper`, so
    that no crate brings a second HTTP stack.
  - `[sources]`: crates.io only.
- Build time: not measured. `sse-stream` has 1,517 lines of `src/` and
  no new dependency, so its cost is small.
