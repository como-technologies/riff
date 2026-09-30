# Report for the lead: the seven reviews of the design

Issue: #309. Design at commit bf325d9. Reviews 01 to 07 at commit
affb33d, in this directory.

This report joins the findings of the seven reviews. It adds no finding
of its own. One finding is one item, also when two or more reviews give
it. The IDs name the reviews. The level of an item is the level of this
report. When a review gave another level, "Where the reviews do not
agree" says so. The rank is by impact: a fault that loses data or lets a
session act on a forged request comes first, then an outage, then a
limit that comes in the first year, then a cost, then a gap in the text.

## The five most important items

1. **Take the write out of the lock** (item 1: 01-1, 02-8, 04-1, 05-5,
   06-1). Five reviews found the same fault. The design holds the one
   lock through the chunk write. Then each chunk holds one record, the
   server does 10 to 20 commands each second, each `who` and `read`
   waits behind a GCS write, and one slow write stops the whole riff.
   When the lock does not hold through the write, two claims of one
   item both pass. The five reviews give one fix: `handle` and `apply`
   under the lock, the write outside it, the reply after the write, and
   a failed write stops the instance.
2. **A rollback loses records through the checkpoint** (item 2: 02-7,
   03-4, 05-1). A build that does not know a record kind skips it and
   writes a checkpoint past it. The next build starts from that
   checkpoint and never applies the record. A removed member stays a
   member, with no error. With item 5, the log cannot give the record
   back after 90 days.
3. **The signed message does not carry the user, the lead mark or the
   key** (items 3 and 4: 03-1, 05-2, 07-5; 03-2, 05-10). A reader takes
   `lead=true` from the server, outside the signature. So a changed
   server or a bucket writer can make each message a request of the
   lead, and a worker acts on it. And with no public key in `Post`, no
   reader can check a signature at all. Both fixes are two fields.
4. **Keep gRPC and protobuf, or cut them** (items 16 and 17: 01-2,
   01-3). Review 01 says the wire and the encoding are a second product
   for a target of less than 100 requests each second, and that JSON
   lines over the wire of today do the same work with no client break.
   Reviews 05 and 07 take gRPC and protobuf as given and refine them.
   The choice decides the shape of Wave 15. Items 9, 10, 43, 58, 62,
   68, 69 and 70 depend on gRPC. Items 44, 45, 63, 64 and 72 depend on
   protobuf.
5. **The log and the checkpoint need rules for a failed write, a
   duplicate chunk and the retention** (items 5, 6 and 7: 02-1, 02-2,
   07-4, 02-5, 06-8, 01-4). The design gives no rule for a GCS write
   that fails, and none for two instances that write one chunk name at
   a deploy. It says "replay from the start of the log" where the start
   is deleted after 90 days and the three kept checkpoints cover 30
   minutes.

The other must-fix items are 8 to 15: a deploy that does not start
(02-3), the old riff at go-live (07-1, 04-5), the sign-in routes
(07-2), the thread access on the new streams (03-3), the state that
never forgets a session (06-2), the `who` calls of the status line
(06-3), a restore that does not work (02-4), and the book anchors
(05-3).

## The view of the reviews on the three goals

**Simple.** Reviews 04, 05, 06 and 07 take the shape of the design as
right: one log, one `apply` for the live path and the replay, a
checkpoint, protobuf with number rules. Review 01 does not agree. It
says only the log, the checkpoint and `handle` / `apply` pay for
themselves. gRPC, protobuf, the zonal-bucket spike, compaction, the
90-day log with reads from the chunks, and the third store for the
sign-ins each solve a problem that the target does not have (01-2 to
01-6). Review 07 agrees on the spike (07-3), review 06 on compaction
(06-12), and reviews 02 and 06 on the sign-in store as written (02-10,
06-7).

**Reliable.** No review takes the design as reliable as written.
Review 02 finds three recovery paths that do not work ("remove the bad
chunks", "replay from the start of the log", "a rollback works"), and a
deploy that does not start takes the riff down. Review 03 finds that a
reader cannot check a message, and that the lead mark is not signed.
Review 05 finds the loss at a rollback. Review 06 finds that the state
and the checkpoint grow with no limit. Each of these has a small fix,
and the reviews say so. The log itself is a good base (01, 02, 05).

**Feasible.** Review 07 says yes, with the crates that the design names
and the static musl image, after three corrections (07-1, 07-2, 07-11).
Item 6 of the build, the gRPC API, is the riskiest and needs a split.
Item 3, the log, is the largest. Review 06 says the store fits the
target in size and money: about 2 GB of log and some dollars each
month. The first limits are the lock, the sessions that are never
forgotten, and the status line. Reviews 01 and 05 say that `handle` /
`apply` is a refactor of the `state.rs` of today, which reads no clock
and does no I/O.

## Where the reviews do not agree

- **The wire and the encoding.** Review 01 cuts gRPC (01-2) and
  protobuf in the store (01-3). Review 05 calls protobuf with number
  rules and `buf breaking` the right choice for a maintainer, and
  refines them (05-7, 05-8, 05-9). Review 07 keeps gRPC and splits its
  build item (07-8). Review 03 finds DPoP in gRPC metadata as strong as
  over HTTP. Review 04 names one cost of gRPC: a proxy that speaks only
  HTTP/1.1 (04-11).
- **The retention.** Review 01 keeps the log 7 days and cuts the reads
  of old messages from the chunks (01-4). Reviews 02 and 06 keep the 90
  days and add checkpoints: one each day (02-5, 06-8), or a delete by
  checkpoint and not by age (02-5). Review 04 wants a read of a gone
  message to give a count (04-8). All agree that "replay from the start
  of the log" is not possible.
- **A duplicate chunk name (412).** Review 02 says the instance that
  holds the lease lists `log/` again, replays the new chunks and writes
  after them; only an instance without the lease stops (02-2). Review
  07 says each 412 stops the instance for good, as R141 does today
  (07-4).
- **The old objects at go-live.** Review 01 removes them, or gives 1.0.0
  a new bucket or a prefix (01-7). Review 02 keeps them until the first
  wave after go-live ends, for a rollback, and keeps the name `lease`
  so that an old and a new instance never serve at the same time
  (02-18).
- **Which replies wait for the write.** Review 02 says each reply that
  says "done" waits (02-9). Review 07 says each call that makes a
  record waits (07-9). Review 06 lets a read, a wake and a view see
  only the records up to the last written position, and asks whether
  each reply must still wait (06-1, question 3).
- **The levels.** The lock (item 1): must-fix in 01 and 06, should-fix
  in 02, 04 and 05. The lead mark (item 3): must-fix in 03 and 05,
  should-fix in 07, which also offers to change R196 and R198 instead.
  The key in `Post` (item 4): must-fix in 03, should-fix in 05. The
  retention (item 5): must-fix in 02, should-fix in 06. The duplicate
  chunk (item 7): must-fix in 02, should-fix in 07. The old riff at
  go-live (item 9): must-fix in 07, should-fix in 04; 07 replies 409
  and 04 replies 410. This report takes the higher level in each case:
  each of these faults loses data, lets a session act on a forged
  request, or takes the riff down.

## The ranking

### Must-fix

| # | Finding | IDs | Section of the design |
|---|---|---|---|
| 1 | The one lock holds through the chunk write. Each chunk holds one record, each view waits behind a GCS write, and a slow write stops the riff. Without the lock, two claims of one item both pass. | 01-1, 02-8, 04-1, 05-5, 06-1 | Event sourcing; The log / Chunks |
| 2 | A build that skips a record kind writes a checkpoint past it. After a rollback and a roll-forward, the record is lost with no error. A rollback to N-1 also drops each field of N from the checkpoint. | 02-7, 03-4, 05-1 | The log / The records; The checkpoint; Operations / Deploy and rollback |
| 3 | `MessageContent` does not sign the user and the lead mark of the sender. A changed server or a bucket writer can mark each message as from the lead (R196, R198, R200). | 03-1, 05-2, 07-5 | The log / Signed messages |
| 4 | `Post` keeps no public key. A thumbprint cannot check a P-256 signature, so no reader can check a message. | 03-2, 05-10 | The log / Signed messages; Reads |
| 5 | Three checkpoints cover 30 minutes, and the start of the log is gone after 90 days. "Replay from the start of the log" is not possible. The members, the owner, the leads and the settings live only in a checkpoint. The restore window is 7 days in any case. | 02-5, 06-8, 01-4 | The checkpoint; Operations / Deploy and rollback; Operations / Set up |
| 6 | No rule for a failed chunk write: a 429, a 503, a timeout, a failed token. The queue grows, the replies wait, and memory and the log go apart. | 02-1 | The log / Chunks |
| 7 | At a deploy, the old instance can write a chunk after the new instance listed `log/`. Two chunks get one name. The design says nothing of `ifGenerationMatch=0` or of a 412. | 02-2, 07-4 | The log / Chunks; The lease |
| 8 | A deploy that does not start takes the riff down: the new instance takes the lease and stops the old one before it loads. | 02-3 | The start and the replay; The lease; Operations / Deploy and rollback |
| 9 | At go-live, an 0.8 riff sends `POST /v1/who`, gets 404 with no `riff-build` header, shows the wrong step and never updates. | 07-1, 04-5 | The wire / gRPC for the calls; Operations / Go live; Build items (6) |
| 10 | `/v1/token`, `/v1/sign-in` and the two `/.well-known` documents are HTTP by the OAuth and MCP specs. The design keeps only `/v1/build` as a JSON route, so no one signs in. | 07-2 | The wire / gRPC for the calls; Build items (6) |
| 11 | No rule says who may read a thread on `Tail` or from the chunks. Today `read` refuses a direct thread to a session that is not in it. | 03-3 | The wire / gRPC for the calls; Reads |
| 12 | The state never forgets a session: cursors, memberships and direct threads stay for good. About 365,000 sessions after a year. The checkpoint, the memory and the start time grow with no limit. | 06-2 | The classes of data; The checkpoint; Target and goals |
| 13 | The status line of each session calls `who` for the whole riff each few seconds. It is the largest request rate, CPU cost and egress. It grows with the square of the sessions, and the design does not count it. | 06-3 | The wire / gRPC for the calls; Operations / Cost |
| 14 | "Remove the bad chunks, then start" does not work: a removed chunk makes a seq gap, and the start fails again. Versioning does not help a chunk. There is no restore. | 02-4 | Operations / Backup and restore; Appendix A |
| 15 | The book anchors have no check. mdbook 0.5.4 renders an empty block for a missing anchor, and exits 0 for a missing file. The book rots with no signal. | 05-3 | The wire / The book shows the real code |

### Should-fix

| # | Finding | IDs | Section of the design |
|---|---|---|---|
| 16 | gRPC changes the wire, the client, the deploy, the DPoP binding and the hand tools. It is the only reason that the clients break at 1.0.0. The target has one client at less than 100 requests each second. | 01-2 | The wire / gRPC for the calls; Decisions of the review (5) |
| 17 | Protobuf in the store buys a check over kept bytes and skips of unknown fields. Both come from "keep the signed bytes", not from protobuf. The cost is `prost`, `protox`, `buf`, `.proto` anchors and a tool to read the log. | 01-3 | The wire / Protobuf for each message; The log / The records |
| 18 | The sign-ins are a third store with their own rules. The refresh reply does not wait, so a GCS outage loses sign-ins, and each of them ends as reuse after the restart. The file is written about each second, 86,400 times each day at the target, with 7 days of versions. The log already gives a durable write. | 01-6, 02-10, 06-7 | The sign-ins; The classes of data; Operations / Set up |
| 19 | `read` stops at 200 messages with no next step. 200 messages are about 170 KB, too much for one tool result. `read` with `all` has no limit, and the start routine calls it. | 04-3, 06-6, 07-7 | Reads |
| 20 | The design does not say whether the checkpoint holds the last N messages and the index. When it does not, the first `read` and the first `riff chat` after a start read one chunk at a time: 10 to 20 s for 200 messages. The index can take 50 to 100 MB. | 04-7, 06-5, 07-7 | The checkpoint; Reads |
| 21 | The design names three replies that wait for the write, of about twelve record kinds. A crash loses each other change that its caller saw as done. | 02-9, 07-9 | The log / Chunks |
| 22 | The read cursors live only in the checkpoint. After each deploy, a session reads again, and is woken again, for messages that it acted on. A verify request that comes twice can start a second verify. | 04-2 | The checkpoint; The classes of data |
| 23 | Each person who can write the bucket controls the riff. The member, admin, owner and lead records are not signed, and a removed chunk of such records is not found. The design does not state this trust. | 03-8 | The classes of data; The log / Chunks; Operations / Set up |
| 24 | The copy check compares signature bytes. ECDSA accepts a twin signature with `n - s`. After a start, memory holds no signature from before the checkpoint, so a copy passes. | 03-5 | The log / Signed messages; The start and the replay |
| 25 | The signature input has no rule for its bytes. The riff ID has free length, so two pairs can give one input. `signature_scheme` is not signed. | 03-6 | The log / Signed messages; Appendix B |
| 26 | The refresh check has no order. When the generation is checked before the DPoP key, a person who knows a chain ID can end each sign-in of the chain. The server keeps no hash of an older or of "the next" generation. | 03-7 | The sign-ins |
| 27 | A stream checks its caller only when it opens. A removed member or an ended sign-in keeps its stream for up to 3600 s. | 03-9 | Live messages |
| 28 | The replay reads chunks one at a time: up to 600 chunks after a checkpoint, 10 to 20 s on top of the lease gap. | 06-4 | The start and the replay |
| 29 | Claims and leads are in the log, sessions are in memory. The design does not say what `apply` does with a claim of a session that the server does not know, or when the 5-minute claim timer starts after a restart. | 07-6, 04-12 | The classes of data; The start and the replay; Build items (3) |
| 30 | `apply` "does not fail" has no rule for a record that the state cannot take, for example after a rollback. | 05-14 | Event sourcing |
| 31 | The 90-day log serves only reads of old messages, a non-goal. An active thread loses messages older than 90 days, which it keeps today, and a read of a gone message has no defined result. | 01-4, 04-8 | Reads; The checkpoint; Operations / Set up; Decisions of the review (1) |
| 32 | The zonal-bucket spike and the region move cost more than they give: an unstable SDK flag, no versioning, no compose, one writer, and Rapid storage at 5 times the price. Compaction has no need at the target. | 01-5, 07-3 | Decisions of the review (3); Compaction; Build items (2) |
| 33 | The old objects `sessions`, `tokens` and `lease` stay in the bucket at go-live: the lifecycle rule deletes only `threads/`. A 1.0.0 server that reads the old `lease` can stop at the load. | 01-7, 03-13 | Operations / Go live |
| 34 | Go live, steps 4 and 5 are in the wrong order: a member who signs in before the invite is refused. | 04-4 | Operations / Go live |
| 35 | Nothing tells the operator that the riff is down. The logs have no `severity` field, so a Cloud Logging filter finds no error. `riff server` misses the first facts: serves or 503 and why, the last error, the skipped records, the memory. | 02-11, 02-12, 02-13 | Operations / Monitoring |
| 36 | "Stop the server" has no command. With `--min-instances 1`, Cloud Run starts a new instance, and the operator cannot change the bucket under it. | 02-6 | Operations / Backup and restore |
| 37 | `log verify` reads each chunk from the start: millions of reads, more than a day. | 02-15, 06-12 | Tools; Compaction |
| 38 | The tools get a token only from the metadata server of Cloud Run. `riff-server log` does not run on a laptop. | 02-14 | Tools |
| 39 | No CI check runs riff N-1 against riff-server N. The line rule tells an old client that it is old. Nothing tests that it works. | 05-6 | The wire; Operations / Deploy and rollback |
| 40 | Nothing checks that a new field has a safe default, or that `apply` of a new build gives the same state for the records of an old build. | 05-7 | The wire |
| 41 | A chunk has no magic, no format version and no check for each record. A wrong length makes the rest unreadable, and `log verify` cannot name the bad record. | 05-11 | The log / Chunks; Tools |
| 42 | `handle` returns `Vec<Record>`, but `position` and `written_at_ms` belong to the log. A test cannot compare them. | 05-4 | Event sourcing |
| 43 | Item 6 rewrites the client, the server and 20 hand-written test files in one step. Nothing passes until all of it passes. | 07-8 | Build items (6) |
| 44 | One protobuf package for the store and the API, with two different rules for a change. | 05-9 | The wire |
| 45 | `buf breaking` has no category. The default `FILE` fails each delete. The Gate has no tags to compare against, and the first release has no earlier tag with `.proto` files. | 05-8, 07-15 | The wire / Protobuf for each message |
| 46 | The record kinds read as commands (`ClaimItem`, `SetLead`), and `Post` is three things. | 05-12 | The log / The records; The wire / gRPC for the calls |
| 47 | "The client waits through the gap and shows no error". A shell command hangs with no words for up to 15 s, `riff top` exits after 60 s, and a chat line typed in the gap blocks the screen. | 04-6 | The lease; Live messages |
| 48 | The design names gRPC status codes, not the words that a person reads. Each new error needs its text and its step: a cut read, a failed write, a gone message, a sign-in ended by reuse. | 04-9 | The wire / gRPC for the calls |

Items 21, 29, 33, 37 and 45 each hold one note of a review (07-9,
04-12, 03-13, 06-12, 07-15) next to a should-fix of another review.

### Notes

| # | Note | IDs | Section of the design |
|---|---|---|---|
| 49 | Each number in the design is a guess for 50 to 250 times the load of today. Measure the records each day by kind, the size of the state, and the p50 and p99 of a GCS write before Wave 15 picks the settings. Update the cost table after the measures. | 01-8, 06-11 | Target and goals; Operations / Cost |
| 50 | `deploy.sh` sets no `--memory`: 512 MiB, and the file system counts against it. Set `--memory 1Gi` and show the memory in use. | 02-16, 06-10 | The picture; Operations / Set up |
| 51 | A crash in the middle of a chunk write loses nothing that a caller saw as done. Keep the copy check in `apply`, so that the replay gives it back. | 02-17 | The log / Chunks; The log / Signed messages |
| 52 | A rollback past go-live is a new start: the old build reads the old objects. Keep them until the first wave after go-live ends, and keep the name `lease`. | 02-18 | Operations / Go live |
| 53 | Keep `refuse_before(start)` for DPoP proofs, or a new instance accepts each proof of the last 5 minutes again. | 03-10 | The sign-ins; The lease |
| 54 | A new field that limits the meaning of a message needs a new signature scheme, or an old build shows the message with a wider meaning. | 03-11 | Appendix B |
| 55 | A person cannot take a secret out of the log for 97 days. A how-to to remove a message is missing. | 03-12 | The classes of data; Operations / Backup and restore |
| 56 | `PostRequest` carries the thread, the sender and the selectors twice: in the call and in the signed bytes. | 03-14 | The log / Signed messages |
| 57 | `(not verified)` means "the signature failed" today. For an unknown scheme it would mean "this build cannot check it". A third mark. | 04-10 | The log / Signed messages |
| 58 | gRPC needs HTTP/2 end to end. A proxy or a VPN that speaks only HTTP/1.1 breaks each call. Test from each network before the client moves. | 04-11 | The wire / gRPC for the calls |
| 59 | Small names: `wake` is `to` everywhere else, `sender_session` does not say its form, `MESSAGE_KIND_MESSAGE` says nothing. | 05-13 | The log / Signed messages |
| 60 | The two-release rule for a signature scheme needs a table that a test can read. | 05-15 | The log / Signed messages |
| 61 | "Each release adds real signed messages to the fixtures" is a habit, not a check, and names no command. A `just fixtures` recipe, and a release-check step that fails when the fixtures of the tag are missing. | 05-16, 07-16 | The wire; Operations / Go live |
| 62 | After item 6, `wire.rs` and the generated types are two truths. Delete `wire.rs`, or keep it as the JSON and schema types of the client and map in `api.rs`. | 05-17, 07-13 | Build items (6) |
| 63 | Name `tonic-prost-build` in item 1 and `tonic-prost` in item 6. | 05-18 | Build items |
| 64 | `ChangeSetting` as a generic record hides its keys from `buf breaking`. | 05-19 | The log / The records |
| 65 | One instance takes at most 1,000 requests at a time, and each stream is one. The limit comes at about 100 people. Show the open streams, and warn at 700. | 06-9 | Live messages; The lease |
| 66 | The DPoP replay cache holds 100,000 IDs for 310 s, about 320 calls each second. Show how full it is. | 06-13 | The sign-ins |
| 67 | Item 3 already takes the people out of the token store. Item 5 takes only the chains. Say the split. | 07-10 | Build items (3, 5) |
| 68 | Each error reply must carry the build, also a tonic `Status`. One tower layer, and a test. | 07-11 | The wire / gRPC for the calls |
| 69 | tonic maps a front-end 429, 502, 503 and 504 to `UNAVAILABLE`. The outage rule of #290 ports on "`UNAVAILABLE` with no build in the metadata". | 07-12 | The wire / gRPC for the calls |
| 70 | `grpcurl` needs the `.proto` files or server reflection. Add `tonic-reflection`. | 01-10, 07-14 | The wire / gRPC for the calls; Tools |
| 71 | `thread_seq` in the record is a second number for the replay check. The positions already find a gap. | 01-9 | The log / One log for the riff; Appendix A |
| 72 | After item 1, each machine compiles `protox`, `prost` and `tonic` in `riff update`. Measure the install time. | 07-17 | The wire / Protobuf for each message; Operations / Go live |

## The must-fix items

### 1. The lock and the write

Section: Event sourcing ("All of this is under the one lock"); The log
/ Chunks.

The problem. The design says: run `handle`, append the records, wait for
the write when the reply needs it, then run `apply`, all under the one
lock. It also says that new records wait in the queue for the next
chunk, so a chunk holds a few records. The two do not agree. A record
joins the queue only after its command ran `handle`, and no command runs
`handle` during the write. So each chunk holds one record, the server
does 10 to 20 commands each second, and each `who`, `top` and `read`
waits behind a write of 50 to 100 ms (01-1, 02-8, 04-1, 05-5, 06-1). A
slow GCS write, up to the 30 s timeout of the client, stops each call of
each session (02-8, 06-1). When the lock does not hold through the
write, a second command runs `handle` against a state without the
records of the first: two claims of one item both get "You hold
issue-12" (01-1).

The proposals.

- Under the lock: `handle`, the positions, the queue, `apply`. Release
  the lock. Wait for the write outside it. Reply after the write (01-1,
  02-8, 04-1, 05-5, 06-1).
- The writer takes each record in the queue into one chunk: group
  commit. Memory is ahead of the log by at most one chunk (01-1, 06-1).
- A write that fails after its retries stops the instance, as a
  conflict does today (R141). The next instance replays the log, and the
  replay drops the change that nobody got a reply for (01-1, 02-8,
  04-1, 06-1).
- Reads, wakes and views show only the records up to the last written
  position (06-1). Wake a session for a record only after its write
  (02-8).
- Tests and measures: two posts in one chunk (05-5); the p99 of `who`
  while 10 sessions post, and the records in each chunk (04-1); the time
  that each call holds the lock and waits for it, p50 and p99 (06-1).

### 2. A rollback loses the records that an old build skipped

Section: The log / The records; The checkpoint; Operations / Deploy and
rollback.

The problem. A build that does not know a record kind skips it. When
that build writes a checkpoint, the checkpoint holds the position after
the record, but not its change. A newer build then loads that
checkpoint and never applies the record. A removed person stays a
member, with no error (02-7, 03-4). Review 05 checked with prost 0.14.4
that N-1 also drops each new field of N. So a rollback to N-1 writes a
checkpoint with the fields of N-1 only, and after three such checkpoints
the last checkpoint of N is gone (05-1). After 90 days, the log cannot
give the records back (item 5).

The proposals.

- A build writes no checkpoint past the first record that it skipped
  (02-7, 03-4, 05-1).
- Or the checkpoint holds the lowest position that the build skipped,
  and a build that knows the kind replays from there (02-7, 03-4).
- Each checkpoint names the version that wrote it. A build writes no
  checkpoint while the newest checkpoint comes from a later version.
  `riff server` says so (05-1).
- Log each skip at WARN with its position. Count the skips in
  `riff server` (02-7).
- A test: new build, old build, new build (02-7, 03-4, 05-1).

### 3. The user and the lead mark in the signed content

Section: The log / Signed messages.

The problem. `MessageContent` has `sender_session`, but not the user
and not the lead mark. Today the signature covers both (R196, `Content`
in `signed.rs`). A reader gets the user and `lead=true` from the server,
outside the signature. So a changed server, or a person who writes the
bucket, can mark each message as from the lead, and a worker acts on it
as a request of its user. This breaks R198 and R200 (03-1, 05-2, 07-5).

The proposals.

- Add `sender_user` and `lead` (or `as_lead`) to `MessageContent`
  (03-1, 05-2, 07-5). Or one `sender` in the form `user/session` of
  `Who` (05-2).
- The server refuses a post with `lead = true` from a session that is
  not the lead (03-1).
- A reader takes the user and the lead mark only from the decoded signed
  bytes (03-1).
- A test that compares the fields of `MessageContent` with R196 (05-2).
- Or change R196 and R198 by decision, and say which in the design
  (07-5).

### 4. The key of the signer

Section: The log / Signed messages; Reads.

The problem. `Post` keeps `signature` and `signature_scheme`, but no
public key. Today the key is in the JWS header, and `read` gives only
the thumbprints of the keys. A thumbprint cannot check a P-256
signature. So no reader can check a message, and a reader of a chunk
from month 1 has no key at all (03-2, 05-10).

The proposals.

- Add the public key of the signer to `Post`, outside `signed_content`:
  `bytes signer_key`, a SEC1 point (03-2, 05-10). Or a
  `signer_thumbprint` (05-10).
- The reader checks the signature with the key, then checks that its
  thumbprint is one of the keys of the sender user (03-2).
- Say what a reader does with a message whose key is not live any more
  (05-10). See the questions.

### 5. The checkpoints and the start of the log

Section: The checkpoint; Operations / Deploy and rollback; Operations /
Set up.

The problem. The server keeps the last 3 checkpoints, one each 1,000
records or each 10 minutes: about 30 minutes (02-5, 06-8, 01-4). The
lifecycle rule deletes `log/` after 90 days. So "a build that cannot
read the newest checkpoint replays from the start of the log" is not
possible after day 90. The members, the owner, the leads and the
settings are only in a checkpoint then. A rollback past a checkpoint
schema change, 30 minutes after the deploy, loses them all (02-5). A bad
build that writes bad checkpoints for 30 minutes leaves no good one
(06-8). Object versioning keeps a deleted checkpoint 7 days, so the
restore window is 7 days in any case (01-4).

The proposals.

- Do not delete chunks by age. The server deletes a chunk only when it
  is older than a checkpoint that it keeps, and it keeps the newest
  checkpoint of each schema version that a supported build reads, N and
  N-1 (02-5). Review 05 says the same with the version in the
  checkpoint (item 2), with the bucket rule as a backstop at a longer
  age (05-1).
- Or keep one checkpoint each day: for 90 days (02-5), or the last 3
  plus one each day for 30 days (06-8).
- The checkpoint follows the same rules for versions as the records, so
  that N-1 reads the checkpoint of N. Encode a copy of the state outside
  the lock. Measure the size, the time to encode and the time to write
  (06-8).
- Remove the claim "replays from the start of the log" from the design
  (02-5).
- Review 01 goes the other way: keep the log only as long as the
  versioning window, 7 days, and keep the last N messages of each thread
  in memory and in the checkpoint (01-4, item 31).

### 6. A failed chunk write

Section: The log / Chunks.

The problem. The design gives no rule for a failed chunk write: a GCS
429 or 503, a timeout, or a failed token from the metadata server. The
queue grows, the replies wait, and nobody knows for how long. The state
in memory and the log can go apart (02-1).

The proposals (02-1).

- Retry each write with backoff for a fixed time, for example 10 s.
- A retry after a lost reply can get 412: read the object, and when its
  bytes are the same, count the write as done.
- After the time, stop for good: 503 to each call, one ERROR line that
  names the chunk and the error, and exit. Cloud Run starts a new
  instance, and it replays from GCS. The fix for a GCS outage is to
  wait.
- Item 1 says the same for the reply: it waits for the write, and a
  write that fails for good stops the server (01-1, 02-8, 04-1, 06-1).

### 7. Two instances, one chunk name

Section: The log / Chunks; The lease.

The problem. The design says that no two chunks have the same name. At
a deploy this is not true: the old instance can finish a chunk write
after the new instance listed `log/`, and the new instance writes a
chunk with the same first position. The design does not say that the
write uses `ifGenerationMatch=0`, or what an instance does on 412. If
the new instance writes under a new name, the records of the old chunk
are in GCS but not in memory: the next checkpoint leaves them out, and a
later replay puts them back (02-2). The design drops the rule of today,
R141, without a word (07-4).

The proposals.

- Write each chunk and each checkpoint with `ifGenerationMatch=0`, as
  `gcs.rs` does today for a new object (02-2, 07-4).
- On 412, review 02: an instance that holds the lease lists `log/`
  again, replays the new chunks, and writes its records after them. An
  instance that does not hold the lease stops for good (02-2).
- On 412, review 07: keep R141. A 412 means that another instance
  serves, and the instance stops for good. The lease stays for the start
  gap. The callers wait for the write, get an error, and send their
  calls again (07-4).
- A test with two instances on one store (02-2).

### 8. A deploy that does not start

Section: The start and the replay; The lease; Operations / Deploy and
rollback.

The problem. Today `main.rs` binds the port before the load, and the
default startup probe of Cloud Run is TCP on the port. So Cloud Run
moves the traffic at once, the new instance takes the lease, and the old
instance stops for good. When the replay of the new build then fails,
no instance serves. The design keeps the same order: take the lease,
wait, then load (02-3).

The proposal (02-3). Load and replay first, with no lease, read only.
When the load works, take the lease, wait 15 s, and replay the chunks
that came since. Bind the port, or pass a startup probe on a route such
as `/v1/build`, only then. When the load fails, the instance exits
before it takes the lease. Cloud Run keeps the traffic on the old
revision, and the old instance serves on.

### 9. The old riff at go-live

Section: The wire / gRPC for the calls; Operations / Go live; Build
items (6).

The problem. An older riff never calls `GET /v1/build`. It reads the
`riff-build` header of the reply to each of its own calls, and the
auto-update starts from that header. At go-live an 0.8 riff sends
`POST /v1/who`. A gRPC server with one JSON route replies 404 with no
header. The riff shows "no riff build in the reply: status 404 ...
Update riff-server", the wrong step, and never updates (07-1, 04-5).

The proposals.

- Keep a fallback JSON handler for each `/v1/*` path. It replies with
  the `riff-build` header: 409, as the layer `check_build` does today
  (07-1), or 410 Gone (04-5). Then the riff says "Update riff on this
  machine", and a machine with `update.auto` updates itself (04-5).
- Keep the fallback until the line before 1.0 is gone (07-1). See the
  questions.
- A test: an 0.8 riff, or a call of the old JSON form, against the new
  server gets the header (07-1, 04-5).
- Review 01 says the clients do not break with its design A, where the
  wire of today stays (01-2, item 16).

### 10. The HTTP routes that stay

Section: The wire / gRPC for the calls; Build items (6).

The problem. The design keeps one JSON route and moves each call to
gRPC. But `POST /v1/token` is an OAuth 2.1 token endpoint, `GET
/v1/sign-in` names the provider, and the two `/.well-known` documents
follow the MCP authorization spec (R22, RFC 9728, RFC 8414). They are
HTTP by spec, and `riff login` and each token refresh use them. Without
them, no one signs in (07-2).

The proposal (07-2). Name the HTTP routes that stay: `/v1/token`,
`/v1/sign-in`, the two `/.well-known` documents and `/v1/build`. Each
other call is gRPC. `curl` stays for these routes.

### 11. Who may read a thread

Section: The wire / gRPC for the calls; Reads.

The problem. The design does not say who may read a thread with the
new `Tail` stream, or with a read of old messages from the chunks.
Today `read` refuses a direct thread to a session that is not in it. A
stream or a chunk read with no such check gives the direct messages of
other sessions, for example the requests of a lead, to each member
(03-3).

The proposal (03-3). One rule for each path that gives messages:
`Read`, `Tail`, `Watch`, a read from the chunks. The caller acts as its
token, and gets a direct thread only when it is one of its two sessions.
A given/when/then test for each path.

### 12. The state never forgets a session

Section: The classes of data; The checkpoint; Target and goals.

The problem. Read cursors are in the checkpoint, thread members are in
the log, and a direct thread is one thread for each pair of sessions.
Today R126 drops a session after 30 days, at the load. The design has
no such rule, and `apply` reads no clock, so it cannot add one. At about
1,000 new session IDs each day, the state holds 365,000 sessions after
12 months. The checkpoint, the memory and the start time grow each
month with no limit (06-2).

The proposal (06-2). A timer writes a record `ForgetSession` for each
session with no sign of life for `SESSION_EXPIRY`. `apply` drops its
cursors, its memberships, and each direct thread whose two sessions are
gone. A given/when/then test for each. Show the numbers of sessions,
cursors and threads in `riff server`.

### 13. The `who` calls of the status line

Section: The wire / gRPC for the calls; Operations / Cost.

The problem. The status line of each Claude Code session runs
`riff statusline`, which calls `who` for the whole riff, each few
seconds in a busy session. At one call each 10 s for each of 400
sessions: 40 calls each second, each about 160 KB, about 16 TB each
month out of Cloud Run. It is the largest request rate, the largest CPU
cost under the lock, and the largest money cost, and it grows with the
square of the sessions. The design does not count it (06-3).

The proposal (06-3). Give the status line its own small call that
returns only the session of the caller: its state, claims, status and
the build. Or let `riff mcp` write the reply of each keep-alive to a
local file that the status line reads with no call. Count it in the
target. Measure the calls each second and the bytes out each second for
each method.

### 14. A restore that works

Section: Operations / Backup and restore; Appendix A.

The problem. "Remove the bad chunks, then start" does not work. The
replay checks that each `thread_seq` is the last seq plus 1. A removed
chunk in the middle makes a gap, so the start fails again. Object
versioning does not help a chunk or a checkpoint: each one is written
once under a new name, so it has no older version (02-4).

The proposal (02-4). Restore is a cut of the log at a position:
`riff-server log cut --after POSITION`. It deletes each chunk and each
checkpoint after that position, prints how many records it removed and
in which threads, and refuses to cut before the oldest kept checkpoint.
The book gives the steps: stop the server (item 36), `log verify`, `log
cut`, start. Say that a cut loses each change after the position, and
name them.

### 15. The book anchors

Section: The wire / The book shows the real code.

The problem. The book anchors have no check. Review 05 built a book
with a good anchor, a missing anchor and a missing file: mdbook 0.5.4
rendered an empty code block for the missing anchor with no message,
and printed one `ERROR` line for the missing file and exited 0. So the
book rots with no signal, and `just ci` passes (05-3).

The proposal (05-3). `just book` fails on an `ERROR` line of mdbook. A
test in the `hygiene` crate reads `docs/book/*.html` and fails on
`{{#include` or on an empty code block.

## Questions for the lead

The questions of the seven reviews, in groups. The ID in front is the
review and its question number.

The wire and the encoding:

1. 01-1: Does 1.0.0 need to break the clients? With design A of review
   01, the store changes and the API does not.
2. 05-3: One protobuf package or two (item 44)?
3. 07-3: Split build item 6 into 6a and 6b (item 43)?
4. 07-2: Until which release does the JSON fallback for the 0.8 line
   stay (item 9)?
5. 05-1: A rollback of riff-server across a line: is it also a rollback
   of each machine with `riff update --tag`?

The lock and the writes:

6. 01-3: Is the group commit rule of item 1 what the design meant?
7. 01-4, 06-3, 07-5: Does each call that makes a record wait for its
   chunk, 50 to 100 ms? Or do reads and views see only the written
   records, with no wait in each reply (items 1 and 21)? A measure of
   the p99 from Cloud Run decides the cost.

The retention and the reads:

8. 01-2: Who reads a message that is older than what memory keeps, and
   how? If nobody, item 31 stands.
9. 02-3: Chunks by age (90 days) or by checkpoint (item 5)?
10. 04-1: Does the checkpoint hold the last N messages of each thread
    (item 20)?
11. 04-2: What is N for `read`, and does `riff read --all` page by
    itself (item 19)?

The signed messages:

12. 07-4: Do the `MessageContent` fields carry the user and the lead
    mark, or do R196 and R198 change (item 3)?
13. 03-2: Must the admin changes in the log (invite, remove, admin,
    owner) carry a signature of the admin, as a post does (item 23)?
14. 03-3, 05-2: A message from a person whose sign-in ended shows
    `not verified` today (R199). With a year of history in the chunks,
    keep that rule, or attest the key of a sender at the time of the
    post, for example with a `KeyAdded` record (item 4)?
15. 04-4: A third mark for an unknown signature scheme, or does
    `(not verified)` stay (item 57)?
16. 05-4: Does the default rule of item 40 become a requirement in
    `requirements.md`?

Operations:

17. 02-1: Is a cut of the log the restore that we want (item 14)? The
    other way is a tool that skips one bad record.
18. 02-2: Who is on call? The alerts of item 35 go to the owner. Also to
    each admin?
19. 04-3: A one-time import of the members from the old `tokens` object
    at go-live, so that the owner does not invite each member again
    (item 34)?
20. 04-5: Does the client keep the 60 s busy limit for a gap, or does
    the limit become a setting (item 47)?
21. 07-1: Does the spike leave Wave 15 (item 32)? If it stays, does the
    list in question 2 of review 07 count as its `Done when`?

The measures:

22. 01-5: Which measures does Wave 15 take before it picks the settings
    (item 49)?
23. 06-1: How many live sessions and how many new session IDs does one
    person make each day? Review 06 uses 8 and 20. Items 12, 13, 18 and
    65 scale with these numbers.
24. 06-2: Does the fix of the status line (item 13) go in the wave of
    the store, or in a wave before it? It needs no store.

One more: review 03 sent the lead a direct message about one more
finding that is not in its file (03-1). The lead of mike decided
(thread message 433): no separate fix and no advisory. The redesign of
Wave 15 replaces tail, so the finding goes into the design through item
11 (03-3) and #299. This report does not hold it.

## Index

Each ID of each review, with its item in this report. For the
verifier.

- 01: 01-1 → 1; 01-2 → 16; 01-3 → 17; 01-4 → 5, 31; 01-5 → 32;
  01-6 → 18; 01-7 → 33; 01-8 → 49; 01-9 → 71; 01-10 → 70.
- 02: 02-1 → 6; 02-2 → 7; 02-3 → 8; 02-4 → 14; 02-5 → 5; 02-6 → 36;
  02-7 → 2; 02-8 → 1; 02-9 → 21; 02-10 → 18; 02-11, 02-12, 02-13 → 35;
  02-14 → 38; 02-15 → 37; 02-16 → 50; 02-17 → 51; 02-18 → 52.
- 03: 03-1 → 3; 03-2 → 4; 03-3 → 11; 03-4 → 2; 03-5 → 24; 03-6 → 25;
  03-7 → 26; 03-8 → 23; 03-9 → 27; 03-10 → 53; 03-11 → 54; 03-12 → 55;
  03-13 → 33; 03-14 → 56.
- 04: 04-1 → 1; 04-2 → 22; 04-3 → 19; 04-4 → 34; 04-5 → 9; 04-6 → 47;
  04-7 → 20; 04-8 → 31; 04-9 → 48; 04-10 → 57; 04-11 → 58; 04-12 → 29.
- 05: 05-1 → 2; 05-2 → 3; 05-3 → 15; 05-4 → 42; 05-5 → 1; 05-6 → 39;
  05-7 → 40; 05-8 → 45; 05-9 → 44; 05-10 → 4; 05-11 → 41; 05-12 → 46;
  05-13 → 59; 05-14 → 30; 05-15 → 60; 05-16 → 61; 05-17 → 62;
  05-18 → 63; 05-19 → 64.
- 06: 06-1 → 1; 06-2 → 12; 06-3 → 13; 06-4 → 28; 06-5 → 20; 06-6 → 19;
  06-7 → 18; 06-8 → 5; 06-9 → 65; 06-10 → 50; 06-11 → 49; 06-12 → 37;
  06-13 → 66.
- 07: 07-1 → 9; 07-2 → 10; 07-3 → 32; 07-4 → 7; 07-5 → 3; 07-6 → 29;
  07-7 → 19, 20; 07-8 → 43; 07-9 → 21; 07-10 → 67; 07-11 → 68;
  07-12 → 69; 07-13 → 62; 07-14 → 70; 07-15 → 45; 07-16 → 61;
  07-17 → 72.
