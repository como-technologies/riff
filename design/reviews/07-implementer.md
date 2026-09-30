# Review 07: the implementer

Issue: #308. Design at commit bf325d9.

## Summary

We can build the design as written, with the crates that it names, in
the static musl image that we have. Two things in the text do not work
at go-live: an older riff never sees the new build, and the sign-in
routes that the OAuth and MCP specs fix as HTTP go away. The spike on a
zonal bucket costs more than it gives: it needs a gRPC-only storage
client that the Rust SDK marks unstable, and it loses versioning and
compose. Item 6, the gRPC API, is the riskiest item: it changes the
client, the server and 20 hand-written test files at once. Split it,
and let both transports run side by side for one item.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 07-1 | An older riff never calls `GET /v1/build`. It reads the `riff-build` header of the reply to each of its own calls: `check_build` in `crates/riff/src/api.rs` starts `auto_update::begin` on any reply with the header, on any status. At go-live a 0.8 riff sends `POST /v1/who`. A gRPC server with one JSON route replies 404 with no header. The riff shows "no riff build in the reply: status 404" and never updates. | must-fix | gRPC for the calls; Go live; build item 6 | Keep a fallback JSON handler for each `/v1/*` path. It replies 409 and the `riff-build` header, as the layer `check_build` in `crates/riff-server/src/lib.rs` does today. Keep it until the line before 1.0 is gone. `GET /v1/build` can stay for people and `curl`. Add a test: a call of the old JSON form gets the header. |
| 07-2 | The design keeps one JSON route, and each call moves to gRPC. But `POST /v1/token` is an OAuth 2.1 token endpoint (a form), `GET /v1/sign-in` names the provider, and the two `/.well-known` documents follow the MCP authorization spec (R22, RFC 9728, RFC 8414). These are HTTP by spec, and today they stay open to a client of each version (01M3MX4V43SF2XFCZWANHD19WV). `riff login` and each token refresh use them. Without them, no one signs in. | must-fix | gRPC for the calls; build item 6 | Name the HTTP routes that stay: `/v1/token`, `/v1/sign-in`, the two `/.well-known` documents and `/v1/build`. Each other call is gRPC. `curl` stays for these routes; `grpcurl` is for the rest. |
| 07-3 | The zonal bucket costs more than it gives at the target. The JSON API cannot write to a zonal bucket; only gRPC `BidiWriteObject` in appendable mode can. The released Rust SDK `google-cloud-storage` 1.19 shows no appendable write in its docs; in its repository, appendable uploads sit behind `--cfg google_cloud_unstable_storage_bidi`, a rustc flag for the whole build. A zonal bucket has no object versioning, no compose, no soft delete, one writer for each object, and needs the hierarchical namespace. So "Backup and restore" and "Compaction" do not work on it. Rapid storage costs about $0.11 for each GB and month, against $0.02 for Standard. The gain is fewer objects. The chunks cost about $5 each month at the target, by the table of the design. | should-fix | Decisions of the review, 3; build item 2 | Take the spike out of Wave 15. Keep the chunks on the regional bucket. Put "append on a zonal bucket" in the backlog, with two conditions: the Rust SDK marks bidi writes stable, and the log has a copy on a regional bucket. When the lead keeps the spike, it proves the list in question 2. |
| 07-4 | A chunk write is a fence, and the design does not use it. Two instances that both think they serve write the same chunk name, the first position of the chunk. The write sends `ifGenerationMatch=0`, so the second gets 412. Today a save that finds another version stops the instance for good (R141). The design drops that rule without a word, because chunks are new objects. | should-fix | The lease; Chunks | Keep R141: a 412 on a chunk write means that another instance serves, and the instance stops for good. The lease stays for the start gap. The records in the queue of the loser are not lost: each caller waits for the write, gets an error, and sends its call again. |
| 07-5 | `MessageContent` drops two signed fields of today. `Content` in `crates/riff-core/src/signed.rs` signs the user and the session ID of the sender, the lead mark, the thread, the selectors, the body, the kind and the time (R196, R198). `MessageContent` has `sender_session`, but no user and no lead mark. A reader can then not check "from the lead of my user" against the signature. The fields are cheap now. Later, the check "compare the decoded fields with the call" must learn them, and older messages have them empty. | should-fix | Signed messages; build item 1 | Add `sender_user` and `as_lead` to `MessageContent`, or change R196 and R198 by decision. Say which in the design. |
| 07-6 | Sessions live in memory, but claims and leads are in the log. A replay applies `ClaimItem` and `SetLead` records of sessions that the server does not know. Today `State::load` marks each loaded session as stopped at the time of the load, so its claims end after `CLAIM_GRACE` (5 minutes) unless it comes back (R125). The design does not say what `apply` does with a claim of an unknown session. Without a rule, a claim lives for ever after a start, or ends at once. | should-fix | The classes of data; The start and the replay; build item 3 | `apply` makes a session for the holder, stopped at the start of the instance. The claim frees after `CLAIM_GRACE` unless the session calls. Put the rule in the design, and in a replay test. |
| 07-7 | A read of old messages from the chunks is slow when it reads one chunk at a time. Memory keeps the last N (200) messages of a thread. A session that is further behind reads from the chunks. A chunk holds a few records, so 200 messages can sit in up to 200 chunks. A GET takes about 50 ms; one after the other, that is 10 s. The start routine of the skill reads the whole history: the repository thread has about 400 messages today, and one `read` with `all` gave this session 338,645 characters. | should-fix | Reads; build item 4 | Read the chunks in parallel, at most 16 at a time, and keep read chunk bytes in memory for a short time. Give `read` with `all` a cursor (a position), so that `riff read`, the MCP `read` tool and step 1 of the skill read the history page by page. Measure: the time of a `read` of 200 old messages after a cold start; the target is under 2 s. |
| 07-8 | 20 test files speak `/v1/` JSON to the server by hand: `dpop`, `token`, `sign_in`, `session`, `auth`, `lease`, `gcs`, `stream`, `saved`, `signed` and others in `crates/riff-server/tests`, and a few in `crates/riff/tests`. Item 6 rewrites each of them in one step, with the client and the server. Nothing passes until all of it passes. | should-fix | Build items; item 6 | Split item 6. 6a: the server serves gRPC beside JSON, from the same handlers; the tests of the server move to the generated tonic client; the CLI still runs on JSON. 6b: the client moves to gRPC; the JSON handlers become the fallback of 07-1. The about 55 CLI tests in `crates/riff/tests` follow the client and need no change of their own. |
| 07-9 | The design does not say which replies wait for the chunk write. "A reply that needs its record in the log (a post, a claim, a pause) waits" gives three examples of about twelve record kinds. | note | Chunks | One rule: each call that makes a record waits for its chunk. Register, alive, status, read and who make no record and wait for nothing. The cost is one write, 50 to 100 ms, for each call that changes the riff. |
| 07-10 | The people (members, admins, owner) and the sign-in chains live together in the token store today (`token.rs`, 2,142 lines; `owner.rs`, 372 lines). The design puts the people in the log (`ChangePerson`) and the chains in `signins.pb`. So item 3 already needs `ChangePerson` and the people state; item 5 is the chains and their snapshot only. | note | Build items 3 and 5 | Say the split in the items: item 3 takes the people out of the token store; item 5 takes the chains. |
| 07-11 | Each error reply must carry the build. Today each reply, also a 409, has the `riff-build` header, and the client checks it (01M3MX1E65XGWDZ062PQ9YXQ5T). In tonic, an error is a `Status`; its metadata comes from `Status::with_metadata`. A refusal made without it has no build, and the client reads it as a version error. | note | gRPC for the calls | One tower layer puts the build in the metadata of each reply, also of each `Status`. A test checks an `UNAUTHENTICATED` and a `FAILED_PRECONDITION` reply for it. |
| 07-12 | tonic maps a front-end reply with no `grpc-status` to a code: 429, 502, 503 and 504 give `UNAVAILABLE`; 404 gives `UNIMPLEMENTED`; 401 gives `UNAUTHENTICATED`; 200 with no trailer gives `UNKNOWN` (`infer_grpc_status` in `tonic/src/status.rs`). So the outage rule of #290 maps: `UNAVAILABLE` with no build in the metadata is an outage; `UNAVAILABLE` with the build is the busy reply of today. | note | gRPC for the calls | Port `busy_waits` and `outage` of `api.rs` on that rule. Keep the tests of `crates/riff/tests/build.rs` with a fake front end that answers 502 with no metadata. |
| 07-13 | `wire.rs` (1,357 lines) carries `serde` and `schemars` derives. The MCP tools (`rmcp`) take their schemas from them, and `text.rs`, `view.rs` and `top.rs` print from them. The prost types have no `schemars` derive, and their enums are `i32`. | note | The wire; build item 6 | Keep `wire.rs` as the JSON and schema types of the client, and map to the prost types in `api.rs`. The map is about the size of `wire.rs`. `type_attribute` in `prost-build` can add derives, but not the enum form. |
| 07-14 | `grpcurl` needs the `.proto` files (`-proto`) or server reflection. `tonic-reflection` 0.14.6 exists. | note | gRPC for the calls; Tools | Add reflection: one crate and a few lines. Then an operator needs no files. |
| 07-15 | `buf breaking` against the last release tag needs the `buf` CLI in CI (`bufbuild/buf-action`), a `buf.yaml`, and the tag in the checkout (a full fetch, or a fetch of the tag). The first release with `.proto` files has no earlier tag with them, so the check starts one release later. | note | Protobuf for each message | Write the CI step with `--against '.git#tag=vX.Y.Z'`, and let the first release pass with no compare. |
| 07-16 | The fixtures rule needs a recipe. "Each release adds some real signed messages to the test fixtures" names no command and no step of the release. | note | Protobuf for each message; Go live | A `just fixtures` recipe signs a set of messages with a test key and writes them to `crates/riff-core/testdata/vX.Y.Z/`. Add the step to "Make a release" in the book. |
| 07-17 | `riff update` builds riff on each machine with `cargo install --git`. After item 1, each machine compiles `protox`, `prost` and `tonic` too. Nobody installs `protoc`, which is the point of `protox`. The build takes longer; on the image build, with `lto` and one codegen unit, expect about a minute more. | note | Protobuf for each message; Go live | Measure the install time of a machine before and after item 1. Say the number in the release notes. |

## The questions of this point of view

### 1. Can we build it as written?

Yes, with the corrections of 07-1, 07-2 and 07-11.

- The crates: `tonic` 0.14.6, `tonic-prost-build` 0.14.6, `prost`
  0.14.4 and `protox` 0.9.1. `protox::compile` gives a
  `FileDescriptorSet`, and `tonic_prost_build::configure().compile_fds`
  takes it. No `protoc`, in CI, in the image build, or on a machine.
- The static musl build: `tonic`, `prost`, `protox`, `hyper` and `h2`
  are pure Rust. The server needs no TLS: Cloud Run ends TLS at its
  front end and sends h2c to the container. The client needs TLS for
  `https://`: `tonic` with `tls-aws-lc` and `tls-native-roots`.
  `aws-lc-sys` already builds in the Alpine image (the Dockerfile has
  `cmake` and `perl`) and on each machine (`jsonwebtoken` uses
  `aws_lc_rs` today). No new build tool.
- gRPC and JSON on one port: `tonic` 0.14 depends on `axum` 0.8, the
  version of riff. `Routes::into_axum_router` (feature `router`) merges
  the gRPC routes into the `axum::Router` of today, and the hyper
  server of `axum::serve` speaks HTTP/1.1 and h2c on one listener.
- DPoP in gRPC metadata: the proof binds `htm` `POST` and `htu` the
  public URL plus the path, for example
  `https://riff.comotechnologies.io/riff.v1.Riff/Post`. A tower layer
  sees the path and the metadata of each call, as the axum layer does
  today. A stream checks its proof at open, as today.
- HTTP/2 on Cloud Run: `--use-http2` in `deploy/deploy.sh`. The
  container must serve h2c; hyper does. Cloud Run carries the requests
  of HTTP/1 clients to the container over h2c, so `curl` and a 0.8 riff
  still reach `/v1/build` and the fallback of 07-1. The deploy already
  has `--timeout 3600`, the limit of a stream. The first deploy checks
  this with `curl --http1.1` through Cloud Run.
- The GCS side of the chunks is the JSON API of today with `reqwest`:
  a `POST` upload with `ifGenerationMatch=0` for a chunk, a list with
  `startOffset` at start, and a `compose` call later. The lifecycle
  rules are `Delete` actions: `age: 90` with `matchesPrefix: ["log/"]`,
  and `daysSinceNoncurrentTime: 7` with `isLive: false`. `signins.pb`
  at one write each second matches the quota of one write each second
  to one object name.

### 2. Can a zonal bucket append to an object in our region or another one?

Yes. Rapid Bucket is generally available (the Next 26 storage post,
April 22, 2026). `us-central1` has the zones `a`, `b`, `c` and `f` for
zonal buckets. "Workloads in other zones and regions can also access
the bucket, with performance relative to the network distance." Cloud
Run picks the zone of the instance, so the append crosses a zone in
most cases.

The costs: see 07-3. The JSON API cannot write to a zonal bucket. The
Rust SDK marks appendable uploads unstable. A zonal bucket has no
versioning, no compose and no soft delete, and needs the hierarchical
namespace. Our bucket is regional and cannot change, so the log gets a
second bucket, and the checkpoint and the sign-ins stay on the first.

When the lead keeps the spike, it must prove:

1. The musl image builds with `google-cloud-storage` and
   `--cfg google_cloud_unstable_storage_bidi` in `RUSTFLAGS`, with
   `--locked`, and runs on Cloud Run.
2. The latency of an append and a flush from Cloud Run to the bucket
   zone: p50 and p99 at 100 records each second.
3. What a reader sees after a flush, and what is left after a crash
   before a flush.
4. A new instance can open the object of a dead writer and append to
   it: one writer for each object, so the takeover, and how long it
   takes.
5. A restore with no versioning: how an operator brings back a bad
   object.
6. A lifecycle `Delete` rule with `age` on an object that is not
   final.
7. `just cloud setup` with `gcloud` 553 or later, and the fake `gcloud`
   tests in `crates/riff-server/tests/cloud.rs`.
8. The cost: about $0.11 for each GB and month, plus the operations
   and the network between zones.

### 3. For each build item: the effort, the risk, and its needs

Effort: S is under a day of one session, M is a few days, L is a week,
XL is more. The lines name what the item touches today.

| Item | Effort | Risk | Needs | Notes |
|---|---|---|---|---|
| 1. The `.proto` files, `prost`, `protox`, `buf breaking` | M | low | nothing | New: about 300 lines of `.proto`, a `build.rs`, `buf.yaml`, a CI step. The book includes the files. See 07-5, 07-15, 07-16. |
| 2. The spike | M | high | nothing (it needs no protos) | An unstable SDK flag, a second bucket, a throwaway binary. See 07-3 and question 2. |
| 3. The log: `handle`, `apply`, the writer, the replay, the stores | XL | high | 1 | The largest rewrite: `state.rs` (3,442 lines) splits into `handle` and `apply`; the handlers of `lib.rs` (1,910) become commands; `store.rs` and `gcs.rs` (742) become the chunk writer and the replay; the people leave `token.rs` (07-10). The JSON API stays, so the CLI tests check it. See 07-4, 07-6, 07-9. |
| 4. The checkpoint, the start, `read` with a limit | M | medium | 3 | The schema version, the last 3, the fall back to an older one, the cursors. The client pages `read` (07-7). |
| 5. The sign-ins: chains, generations, the snapshot | L | medium | 3 | `token.rs` (2,142 lines). A bug here signs each person out. The rule "the current generation or the next one" after a crash needs its own tests. |
| 6a. gRPC beside JSON on the server | L | medium | 1, 3 | The generated service, the DPoP layer, the build layer (07-11), the status codes, the streams. The 20 hand-written tests move to the tonic client (07-8). |
| 6b. The client moves to gRPC | L | high | 6a, 4, 5 | `api.rs` (1,483 lines), the map from `wire.rs` (07-13), the outage rule (07-12), the fallback for the 0.8 line (07-1). Nothing runs until it all runs. |
| 7. The tools, the facts in `riff server`, cloud setup, the book | M | low | 4, 6a | `riff-server log`, `log verify`, `--use-http2`, the lifecycle rules, reflection (07-14), the operations pages. |
| 8. Go live | S | medium | all | The empty start, the sign-ins again, the owner and the admins from the environment, the old riff on the fallback. |

The riskiest item is 6 as written. It changes the client, the server
and the tests in one step, with no state in which both work. Item 3 is
the second: it is the largest change, but the JSON API and the CLI
tests check it at each step.

### 4. Is the order right? Is anything missing?

The order is right for the server: the store first, behind the JSON API
of today, then the transport. Three changes:

- Take item 2 out, or run it first as a throwaway: it needs no protos
  and blocks nothing (07-3).
- Split item 6 into 6a and 6b (07-8).
- Item 4 can run beside item 5: they touch different files.

The client side of each item:

| Item | The client |
|---|---|
| 1 | Signs the bytes of `MessageContent` with the prefix and the riff ID, in place of the JWS over JSON of today (`signed.rs`). |
| 3 | Sends `signed_content` and the signature in a post. Sends its call again on an error of the chunk write. |
| 4 | Pages `read` with `all`: `riff read`, the MCP `read` tool, and step 1 of the skill (07-7). |
| 5 | Nothing: the refresh token is opaque, and the client refreshes on a 401 today. |
| 6b | `api.rs`, `follow`, the outage rule, the build check, the map from `wire.rs`. |
| 8 | `riff login` on each machine; the update from the fallback (07-1). |

Missing from the items:

- The fallback for the 0.8 line (07-1) and the HTTP routes that stay
  (07-2).
- The user and the lead mark in `MessageContent` (07-5).
- The rule for a claim of an unknown session at replay (07-6).
- The rule for which replies wait (07-9).
- The `just fixtures` recipe and its step in the release (07-16).
- The `RIFF_DIR` file store is in item 3, but `RIFF_BUCKET` of today
  and the new setting for the log need names in `main.rs` and in the
  book.
- `--use-http2` in `deploy/deploy.sh`, with its line in the fake
  `gcloud` test of `cloud.rs`.

## Questions for the lead

1. Does the spike leave Wave 15 (07-3)? If it stays, does the list in
   question 2 count as its `Done when`?
2. Until which release does the JSON fallback for the 0.8 line stay
   (07-1)?
3. Do we split item 6 into 6a and 6b (07-8)?
4. Do `MessageContent` fields carry the user and the lead mark, or do
   R196 and R198 change (07-5)?
5. Does each call that makes a record wait for its chunk (07-9)?
