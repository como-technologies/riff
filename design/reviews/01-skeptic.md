# Review 01: the skeptic

Issue: #302. Design at commit bf325d9.

## Summary

The design has one part that pays for itself: an append-only log of small records, a checkpoint, and a pure `handle` / `apply` pair that the live path and the replay share. That part fixes the real cost of today's store, where each post writes the whole thread object again. The rest of the design is a second product. gRPC, protobuf, `buf`, a zonal-bucket spike, compaction, a 90-day log with reads of old messages, and a third store for the sign-ins each solve a problem that the target does not have. The target is dozens of people at less than 100 requests each second. The same log with JSON lines, over the wire that riff has today, does the same work with fewer crates, no new tools and no client change. One rule of the design, the lock around the write, does not work as written. It needs a fix in any case.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 01-1 | The rule "handle, append, wait for the write, then apply, all under the one lock" cannot batch records into a chunk. When the lock holds through the write, each chunk holds one record, the server does at most 10 to 20 commands each second, and each `who` and `read` waits behind a GCS write. When the lock does not hold through the write, two claims of one item both pass `handle`. | must-fix | Event sourcing / Chunks | Apply each record at once, under the lock. Release the lock. The reply waits for the chunk write outside the lock. A failed write stops the instance, as a conflict does today (R141). The next instance replays the log. Write the rule: memory is ahead of the log by at most one chunk. |
| 01-2 | gRPC changes the wire, the client, the deploy, the DPoP binding and the hand tools, and it is the only reason that the clients break at 1.0.0. The store change touches no call. Decision 5 is circular: the clients break one time only because gRPC makes them break. | should-fix | The wire / gRPC for the calls; Decisions of the review, 5 | Keep JSON over HTTP with `axum` and `reqwest`, and the server-sent events for watch and tail. Cut `tonic`, `--use-http2`, `grpcurl`, the gRPC status codes and the DPoP path change. Keep `/v1/build` as it is. |
| 01-3 | Protobuf in the store buys a check over kept bytes and skips of unknown fields. Both come from the rule "keep the signed bytes", not from protobuf. JSON with `serde` skips unknown fields and gives a missing field its default in the same way. The cost of protobuf is `prost`, `protox`, `buf` in CI, `.proto` anchors in the book, and a tool to read the log. | should-fix | The records; Signed messages; Protobuf for each message | Write each record as one JSON line. Keep the exact JSON string that the client signed as the signed content, and never encode it again. Keep the fixtures of each release in CI. Replace `buf breaking` with one rule in the review checklist: never give a field name a new meaning. Measure the size of one JSON record against one protobuf record from a real thread before the choice. |
| 01-4 | The 90-day log serves only reads of old messages from the chunks, and searches of old history are a non-goal. The restore window is 7 days in any case: the 3 kept checkpoints cover at most 3,000 records or 30 minutes of activity, and object versioning keeps a deleted checkpoint for 7 days. A full replay from position 1 is never possible, because the rule deletes the first chunk after 90 days. | should-fix | Reads; The checkpoint; Set up, lifecycle rules; Decisions of the review, 1 | Keep the log as long as the versioning window (7 days, one setting). Keep the last N messages of each thread in memory and in the checkpoint. Cut "older messages come from the chunks" and the index from each thread to its positions. `read --all` then gives what memory keeps. |
| 01-5 | Compaction and the zonal-bucket spike both solve one problem: too many objects under `log/`. With a 7-day log (01-4), the count is the rate times 7 days. A region move for a storage optimization is a cost that the target does not have. | should-fix | Compaction; Decisions of the review, 3 | Remove the spike and the region move from the decisions. Chunks are the simplest store that GCS gives: a normal bucket cannot append to an object. Add compaction only when a measure shows a need: the number of objects under `log/`, and the time of the list at start. |
| 01-6 | The sign-ins are a third store with their own rules: a write at most one time each second, a snapshot that can be one generation behind, and a server that takes the current or the next generation as good. The log already gives a durable write before a reply. | should-fix | The sign-ins; The classes of data | Make each sign-in change a record: sign in, refresh, revoke. The checkpoint holds the current chains. A refresh reply waits for its chunk, as a claim does. Cut `signins.pb`, its write rule and the "one generation behind" rule. The cost is about 1,000 records each day. |
| 01-7 | The go-live plan keeps the old objects in the same bucket. The lifecycle rule of today deletes only `threads/` objects (R46, R147). The objects `sessions`, `tokens` and `lease` stay for good. A 1.0.0 server that reads the old `lease` object at start can stop with "cannot read the saved object" (01M3MMXYS1V8CA89D2XHKPR6C4). | should-fix | Go live, step 7 | Give 1.0.0 a new bucket, or a prefix in the bucket. Or remove the old objects in the go-live step, with the command of the server. Do not depend on the 30-day rule. |
| 01-8 | Each number in the design is a guess for a load 50 to 250 times the load of today: 20,000 messages each day in the design, 386 messages in the repository thread after 5 days today. The design does not say what to measure. The cost table counts 30,000 chunks for 50,000 records each day, but a writer that writes as soon as the queue holds a record, at less than one record each second on average, writes about one chunk for each record. | note | Target and goals; Cost | Before Wave 15 picks the chunk wait, the checkpoint interval and the retention, measure: records each day by kind (from the thread objects of today), the size of the state in bytes, and the p50 and p99 of one GCS write from Cloud Run in us-central1. Put the measures in the design page. |
| 01-9 | `thread_seq` in the record is a second number for the replay check. The positions already find a missing or repeated chunk: the last position of a chunk plus one is the name of the next chunk. | note | One log for the riff; Appendix A | Keep the seq in memory and in the checkpoint, from `apply`. Do not write it in the record. Or keep it, and say that it is for people, not for the check. |
| 01-10 | `grpcurl` needs the `.proto` files or server reflection (`tonic-reflection`). The design names neither. | note | The wire / gRPC for the calls | Moot with 01-2. If gRPC stays, add `tonic-reflection` to the server, or name the `.proto` path in the how-to. |

### 01-1: the lock and the write

The design says: "The server runs `handle`, appends the records to the log, waits for the write when the reply needs it, then runs `apply` for each record. All of this is under the one lock." It also says: "While it writes, new records wait in the queue for the next chunk. At less than 100 requests each second, a chunk holds a few records."

The two sentences do not agree. A record joins the queue only when its command ran `handle`. When the lock holds through the write, no other command runs `handle` during the write. So the queue holds one record, and each chunk holds one record. A write takes 50 to 100 ms, so the server does 10 to 20 commands each second. Each view waits for the lock too.

When the lock does not hold through the write, a second command runs `handle` against a state that does not hold the records of the first. Two claims of `issue-12` both see a free item, and both get a record. `apply` does not fail, so one claim wins in memory, and the other caller also saw "You hold issue-12".

The fix is the group commit rule: `apply` under the lock at once, then write, then reply. The state in memory is ahead of the log by at most one chunk. A write that fails stops the instance. This is the rule of today for a conflict (R141). The next instance replays the log, and its state is the log.

### 01-2 and 01-3: the wire and protobuf

Today riff-server serves JSON over HTTP with `axum` (`crates/riff-server/src/lib.rs`). Watch and tail are server-sent events (`axum::response::sse`). The client uses `reqwest`. The MCP side (`rmcp`) speaks JSON to the agent. Each of these exists, is tested, and works at the target load.

gRPC brings: `tonic` and `prost` in three crates, `protox` in the build, `buf` in CI, `--use-http2` on Cloud Run, `grpcurl` for a person, a DPoP proof that binds a gRPC path, a map from errors to gRPC status codes, and a second protocol next to `/v1/build`. It gives: smaller bytes, typed streams and clients in other languages. The target has less than 100 requests each second, one client, and no web client.

Protobuf in the store brings: the same crates and tools, `.proto` anchors in the book, and a tool to print the log. The design says why the server keeps the signed bytes: "When a 0.8 server decodes a 0.9 message and encodes it again, it drops field 7." This is true for JSON too, and the fix is the same: keep the bytes. `serde` gives a missing field its default with `#[serde(default)]`, and it skips a field that it does not know, unless a type says `deny_unknown_fields`. In the crates of today, only `Selector` says it (`crates/riff-core/src/selector.rs`).

Today the signature is a detached JWS over the JSON of `Content`, and the reader makes that JSON again from the fields (`crates/riff-core/src/signed.rs`). A new field in `Content` changes the bytes that an older reader makes, so its check fails. The gain of the design is real, and it comes from "keep the bytes". A JSON log keeps them as one string.

What to measure: the size of one record as JSON and as protobuf, for a real message from the repository thread. If the storage stays under $1 each month at the JSON size, protobuf has no gain for the target.

### 01-4 and 01-5: the retention, the old reads, the spike

The checkpoint carries the old state forward. So the log only needs to stay until a checkpoint that the server can still read covers it. The server keeps 3 checkpoints. It writes one each 1,000 records or each 10 minutes when records came. So the 3 checkpoints cover at most 3,000 records or 30 minutes of activity. Object versioning keeps a deleted object for 7 days. So a restore can go back 7 days, and not more. The chunks from day 8 to day 90 serve one thing: "Older messages come from the chunks when a session asks for them." Searches of old history are a non-goal.

Today `read` has no limit, and `read` with `all` gives the whole history (`State::read` in `crates/riff-server/src/state.rs`). The full history that the `read` tool gave this session is more than 300,000 characters after 5 days. No session can use it. The limit of 200 in the design is the fix. The last N messages of each thread in memory, with N as a setting, is enough for `--all` too.

With a 7-day log, the number of objects under `log/` is the rate times 7 days. At the rate of the cost table, that is about 210,000 objects. A list from a position costs one call for each 1,000 objects. The start lists only from the checkpoint position. So compaction has no need at the target, and the spike has no need at all. The design already says "Compaction is not in the first build". Remove both from the decisions. Add compaction when a measure shows a need.

### 01-6: the sign-ins in the log

Today the token store is one object, saved before the reply of a token call or an admin call (`save_tokens_since` in `crates/riff-server/src/lib.rs`). The design keeps a separate snapshot, `signins.pb`, at most one write each second, and it accepts a snapshot that is one generation behind. The log removes the need for this: a refresh record is in its chunk before the reply, and a crash loses nothing. A sign-in change is small and rare (1,000 each day in the cost table). One log, one checkpoint, one loss class.

### 01-7: the old objects at go-live

`deploy/lifecycle.json` deletes objects under `threads/` after 30 days. `sessions`, `tokens` and `lease` are outside `threads/` on purpose (R147). The design says the old state goes away after 30 days. It does not. A 1.0.0 server that starts in the same bucket finds the old `lease`. When the new lease format differs, the load fails with "cannot read the saved object", and the server stops (01M3MMXYS1V8CA89D2XHKPR6C4). The go-live step must remove the old objects, or 1.0.0 must use a new bucket or a prefix.

### 01-8: the numbers

The repository is 5 days old (first commit 2026-09-26). The repository thread holds 386 messages. The design plans for 20,000 messages and 30,000 other records each day. That is the target, and a target can be large. But each setting in the design (a chunk of a few records, a checkpoint each 1,000 records, 90 days, 200 unread) is picked with no measure. A review cannot say if a setting is right. The design page can say what to measure, and Wave 15 can measure it in its first item.

## The questions of this point of view

### 1. What can we cut or merge, and what does it cost to cut it?

| Cut or merge | What it costs |
|---|---|
| gRPC (`tonic`, `--use-http2`, `grpcurl`, the status codes, the DPoP path change) | Nothing at the target. The wire of today stays. |
| Protobuf in the store (`prost`, `protox`, `buf`, `.proto` anchors) | Larger records: measure the ratio; a common ratio is 2 to 3. Storage stays under $1 each month at 3 times 5 GB. The version rule moves from `buf` to a checklist rule plus the fixtures of each release. |
| The zonal-bucket spike and the region move | Nothing. Chunks in a normal bucket do the work. |
| Compaction | More objects under `log/`. With a 7-day log, about 210,000 at the target rate. |
| The 90-day log, the reads of old messages from the chunks, the index from each thread to its positions | `read --all` gives what memory keeps. An operator reads older records with the log tool inside the versioning window. No forensics past 7 days. |
| The sign-in snapshot, merged into the log | About 1,000 records each day. Token hashes in the log; the snapshot holds them too. |
| `thread_seq` in the record | The replay check uses positions and chunk names. The seq stays in memory and in the checkpoint. |
| `riff-server log` as a tool | With JSON lines, `gcloud storage cat` and `jq` print the log. `log verify` stays: it checks the positions and the signatures. |

What not to cut:

- The log, the chunks and the checkpoint. Today each post writes the whole thread object again, each second (`State::changes` in `crates/riff-server/src/state.rs`, R127). The object grows with each message and never shrinks. The log fixes this.
- `handle` and `apply` as pure functions, with the given/when/then tests. This is the part that makes a replay give the same state as the live path. `State` of today already does no I/O and reads no clock, so it is a refactor, not a new system.
- The lease. It exists (`crates/riff-server/src/lease.rs`).
- Signed messages and the rule "keep the signed bytes". The rule is the gain.
- The N and N-1 rule between riff and riff-server. It exists (`compatible` in `crates/riff-core/src/build.rs`).
- The two-release rule for a new signature scheme. It is one test.

### 2. A simpler design, and does it win?

**Design A: the JSON log.** One log of JSON lines in chunks, with the names and the positions of the design. Each `Post` record holds the exact JSON string that the client signed, its signature and its scheme. A checkpoint is one JSON object, each 1,000 records or each 10 minutes. The log stays 7 days, the same as the versioning window. Memory holds the last N messages of each thread. Sign-in changes are records. The wire stays as it is: JSON over HTTP and server-sent events. `handle`, `apply`, the tests, the lease, the read limit, the group commit rule of 01-1, and the fixtures of each release stay.

Design A wins for the target. It keeps each goal of the design: one instance decides, each change is a record, each write is small, the start time has a limit, old records go away by a rule, the formats have version rules with a CI check, and the cost is small. It removes the gRPC and protobuf crates, two tools (`buf`, `grpcurl`), one spike, one region move, one store, one protocol and one index. The clients do not break: the store is behind the API. Wave 15 becomes the store, the checkpoint, the read limit and the tools. It loses smaller bytes and typed streams, which the target does not need.

**Design B: one object for the whole state.** The state as one JSON object, saved each second when it changed, with `ifGenerationMatch`. This is the store of today, merged into one object. It does not win. The write is the whole state, so it grows with the kept messages: 50 threads with 200 messages of 1 KB is 10 MB each second. A crash loses up to one second, so a claim reply can lie. The one-write-each-second limit of GCS caps the save rate. Goals 2 and 3 fail.

**Design C: no checkpoint, a full replay at start.** It does not win. The checkpoint is not only for the start time. As soon as a rule deletes old chunks, the checkpoint is the only thing that carries an old invite or an old lead forward. With no checkpoint the log can never be deleted, and the start time grows for 12 months. Goal 4 fails.

**Design D: no chunks.** A normal GCS bucket cannot append to an object. One object for each write batch is the smallest thing that works. The spike is design D with a special bucket. It does not win: see 01-5.

### 3. Which parts solve a problem that the target does not have?

- gRPC. Smaller bytes, typed streams and clients in other languages, for one client at less than 100 requests each second.
- Protobuf in the store. Compact records, for less than 5 GB of storage.
- The zonal-bucket spike and a region move. Fewer objects, for a bucket that can hold millions.
- Compaction. The same.
- The 90-day log and the reads of old messages from the chunks, with an index from each thread to its positions. History, which the design lists as a non-goal.
- The sign-in snapshot with its own write rule and its generation workaround. A durable write, which the log gives.
- `thread_seq` in the record. A replay check that the positions give.

## Questions for the lead

1. Does 1.0.0 need to break the clients? With design A, the store changes and the API does not. The go-live can then be a server deploy with a new bucket, and a client update for the read limit only.
2. Who reads a message that is older than what memory keeps, and how? There is no search. If the answer is nobody, 01-4 stands.
3. Is the group commit rule of 01-1 what the design meant? If yes, the design page needs the sentence. If no, which of the two failures does the design accept?
4. Is the 50 to 100 ms wait for a chunk write on each post and claim acceptable, against the one-second save of today? It is the price of a claim reply that does not lie. A measure of the p99 from Cloud Run decides it.
5. Which measures does Wave 15 take before it picks the settings (01-8)?
