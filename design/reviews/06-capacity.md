# Review 06: the capacity planner

Issue: #307. Design at commit bf325d9.

## Summary

The store fits the target in size and in money: about 2 GB of log, a
few million small objects and some dollars each month for GCS. The
first limits are not in GCS. They are in the one lock, in the state
that never forgets a session, and in one call that the design does not
name: the status line of each session asks `who` for the whole riff.
Each of these is cheap to fix now and costly to fix after go-live. The
defaults are good, except N = 200 in `read`, which is too large for one
tool result of an agent.

## The numbers that this review uses

| Fact | Value | Source |
|---|---|---|
| People, repositories, months | 50, 50, 12 | the issue |
| Live sessions at a time | about 400 (8 for each person: agents, workers, `riff top`, `riff chat`) | an estimate; today 2 people have 14 |
| New session IDs each day | about 1,000 (20 for each person: each worker item and each `/clear` is a new session) | an estimate |
| Records each day | 50,000 (20,000 messages, 30,000 other) | "Cost" in the design |
| Size of a message | about 850 bytes on average, 2 to 4 KB for a verify result | the repository thread today: 396 messages gave 338,645 characters |
| Write of one chunk | 50 to 100 ms | the design; the spike must measure p50 and p99 |
| Cloud Run instance | 1 vCPU, 512 MiB (no `--memory` in `deploy/deploy.sh`), `--concurrency 1000`, `--timeout 3600`, one instance | `deploy/deploy.sh` and Cloud Run defaults |
| Access token life | 10 minutes (`ACCESS_TTL`) | `crates/riff-server/src/token.rs` |
| Keep-alive | each 60 s, each 10 s for a worker (`ALIVE_EVERY`, `WORKER_ALIVE_EVERY`) | `crates/riff-core/src/wire.rs` |
| Session expiry today | 30 days (`SESSION_EXPIRY`, R126) | `crates/riff-server/src/state.rs` |
| DPoP replay cache | at most 100,000 IDs (`MAX_PROOFS`) for a window of 310 s (`MAX_AGE` + `MAX_SKEW`) | `auth.rs`, `dpop.rs` |

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 06-1 | The server holds the one lock while it waits for the chunk write. So only one call that needs the log runs at a time: at 50 to 100 ms each, the whole server does at most 10 to 20 of them each second, and each other call waits behind them. Each call makes its own chunk, so no two calls share a write. A slow GCS write (the GCS client waits up to 30 s, `TIMEOUT` in `gcs.rs`) stops each call of each session, also `who` and the keep-alives. | must-fix | Event sourcing ("All of this is under the one lock"); The log / Chunks | Under the lock: `handle`, give the records their positions, put them in the queue, `apply`. Then let go of the lock, and wait for the write outside it. The writer takes all records in the queue into one chunk (group commit). Reads, wakes and views show only records up to the last written position. When a write fails, the server stops serving and starts again, so the state in memory never holds a record that is not in the log. Measure: the time that each call holds the lock and waits for it, p50 and p99. |
| 06-2 | The state never forgets a session. Read cursors are in the checkpoint, thread members are in the log, and a direct thread is one thread for each pair of sessions (`ThreadName::direct`). Today R126 drops a session after 30 days, at the load. The design has no such rule, and `apply` reads no clock, so it cannot add one. At about 1,000 new session IDs each day, the state holds 365,000 sessions after 12 months, with their cursors, memberships and direct threads. The checkpoint, the memory and the start time grow each month with no limit. | must-fix | The classes of data; The checkpoint; Target and goals ("the start time and each read have a limit") | A timer writes a record `ForgetSession` for each session with no sign of life for `SESSION_EXPIRY`. `apply` drops its cursors, its memberships, and each direct thread whose two sessions are gone. A given/when/then test for each. Show the numbers of sessions, cursors and threads in `riff server`. |
| 06-3 | The status line of each Claude Code session runs `riff statusline`, and it calls `who` for the whole riff (`statusline` in `crates/riff/src/main.rs`). Claude Code runs the status line each time the conversation changes, so a busy session calls `who` each few seconds. The reply holds each live session (about 300 to 500 bytes each). At one call each 10 s for each of 400 sessions: 40 calls each second, each about 160 KB (400 sessions × 400 bytes). That is about 6 MB each second out of Cloud Run, about 16 TB each month. This is the largest request rate, the largest CPU cost under the lock, and the largest money cost, and the design does not count it. It grows with the square of the sessions. | must-fix | The wire / gRPC for the calls; Operations / Cost | Give the status line its own small call that returns only the session of the caller (its state, claims, status and the build), or let `riff mcp` write the reply of each keep-alive to a local file that the status line reads with no call. Count it in the target. Measure: calls each second and bytes out each second for each method. |
| 06-4 | The replay reads the chunks one at a time. At the target a chunk holds 1 or 2 records, so the records after a checkpoint (up to 1,000) can be in up to about 600 chunks. At 20 to 30 ms for each read, the replay takes 10 to 20 s, on top of the 15 s gap of the lease. | should-fix | The start and the replay | Read up to 32 chunks at a time, and apply them in order. Also write a checkpoint after each 200 chunks, not only after 1,000 records. Measure: replay time, chunks read, time to load the checkpoint. |
| 06-5 | The design does not say where the last N messages of each thread and the index from thread to positions come from after a start. If they come from the chunks, a start reads up to 200 chunks for each active thread: about 10,000 reads for 50 repository threads. The index has one entry for each message in 90 days: about 1.8 million entries. As a `BTreeMap` it can take 50 to 100 MB of memory. | should-fix | Reads; The checkpoint | The checkpoint holds the index and the last N messages of each active thread. Say what "active" is, for example a message in the last 7 days. Keep the index small: for each thread, a sorted list of (seq, position) pairs, and drop the entries of chunks that the lifecycle rule deleted. Measure: the size of the checkpoint and of the index. |
| 06-6 | N = 200 is too large for one tool result. At 850 bytes each, 200 messages are about 170 KB, about 40,000 tokens. Today a `read` with `all` of 396 messages gave 338,645 characters, and Claude Code put the result in a file, not in the context. The design limits only unread messages. A `read` with `all` has no limit: at the target a repository thread holds 9,000 to 27,000 messages in 90 days (8 to 23 MB), and most of them come from chunks. The start routine of each session calls `read` with `all`. | should-fix | Reads | Limit each reply by bytes too, for example 64 KB, and by count. A `read` with `all` gives the newest messages first, with the same limits, and a `before` seq for the page before. Change the start routine to read one page. |
| 06-7 | `signins.pb` changes at each refresh. With access tokens of 10 minutes and 400 live sessions, the server refreshes about 0.7 times each second, so it writes `signins.pb` about each second: up to 86,400 writes each day, not 1,000. With object versioning on, each write keeps an older version for 7 days: up to 600,000 versions. Each (sign-in, session) has a chain, so the file grows with the sessions too (see 06-2). | should-fix | The sign-ins; Set up (object versioning, lifecycle rules); Cost | Add a lifecycle rule for `signins.pb` only: delete an older version when it has more than 50 newer versions (`numNewerVersions`). End a chain when its session is forgotten (06-2). Count the real write rate in the cost. |
| 06-8 | Three checkpoints are about 30 minutes of history at a checkpoint each 10 minutes. A bad build that writes bad checkpoints for 30 minutes leaves no good one. After day 90, the start of the log is gone, so "replays from the start of the log" cannot give the state again: the checkpoint is the only copy of the members, the leads and the cursors. The design writes the checkpoint "from time to time", but it does not say if the server encodes it under the lock. | should-fix | The checkpoint; Deploy and rollback | Keep the last 3 checkpoints and one for each day of the last 30 days. The checkpoint follows the same rules for versions as the records, so that release N-1 reads the checkpoint of release N. Encode a copy of the state outside the lock. Measure: the size, the time to encode and the time to write. |
| 06-9 | Each watch and each tail stream is one request of Cloud Run for as long as it is open. One instance takes at most 1,000 requests at a time (the Cloud Run maximum, and our setting). At about 400 sessions, the streams and the calls are 400 to 500. The limit comes at about 100 people, or earlier when each person runs more workers. At each deploy, each stream opens again in the same few seconds. | note | The wire / gRPC for the calls; Live messages; The lease | Show the number of open streams. A warning at 700. Past that, two streams of one client can share one watch, or a client can close the watch while its agent sleeps. |
| 06-10 | `deploy/deploy.sh` sets no memory, so the instance has the Cloud Run default of 512 MiB. The estimate at the target, with 06-2 and 06-5 fixed: about 10 MB of cursors and members, 30 MB of index, 10 to 40 MB of last messages, 15 MB of DPoP IDs, about 30 MB of stream buffers, plus the program: 130 to 200 MB. The encode of a checkpoint needs a copy. So 512 MiB has about 2 times room. Without 06-2 the state grows past it in the year. | note | The picture (memory); Operations / Set up | Set `--memory 1Gi` in `deploy.sh`: about $2.60 more each month at the instance price. Show the memory in use against the limit. |
| 06-11 | The cost table counts 30,000 chunks each day. Without group commit (06-1) each call that needs the log makes its own chunk, so the chunks are close to the records: about 50,000 each day. With 06-7, the writes are 3 to 4 million each month: about $15 to $20, not $5. That is still small. The Cloud Run instance with its CPU always on (`--no-cpu-throttling`) is about $50 each month at list price (1 vCPU at $0.000018 and 0.5 GiB at $0.000002 for each second). The egress of 06-3 can cost more than both. | note | Operations / Cost | Update the table with the rates of 06-3 and 06-7 after the spike measures them. |
| 06-12 | Without compaction the log has about 30,000 to 50,000 chunks each day: 2.7 to 4.5 million objects after 90 days. GCS has no limit for this, and the replay lists only from the checkpoint (`startOffset`), so the count does not slow the start. But `riff-server log verify` reads each chunk: millions of reads, hours in one task. `riff server` cannot count the chunks with a list. | note | Tools; Monitoring; Compaction | `log verify` takes a start position, and reads chunks in parallel. The server counts the chunks from the positions that it knows, not with a list. Compaction is not needed for capacity in the first year. |
| 06-13 | The DPoP replay cache keeps at most 100,000 proof IDs over 310 s: about 320 calls each second. Past that, the server refuses good proofs that are close to the oldest one it forgot. The status line load of 06-3 can come near it. | note | The sign-ins (DPoP replay IDs in memory) | Show how full the cache is. Fixing 06-3 keeps the rate well below it. |

Levels: must-fix (the design fails its goals without it), should-fix (a
real gain), note (for the lead to know).

## The questions of this point of view

### 1. At the target: replay time, memory, objects, write latency, cost

- **Replay time at start.** The lease gap is about 15 s. Then the load
  of the checkpoint: 10 to 40 MB with the index and the last messages
  (06-5), less than 1 s to read and decode. Then the tail after the
  checkpoint: up to about 600 chunks. One at a time, as the design says,
  10 to 20 s. With 32 reads at a time (06-4), about 1 s. So a start is
  about 17 s with 06-4, and up to 35 s without it. Without 06-2 the
  checkpoint grows each month, and the start grows with it.
- **Memory of the instance.** 130 to 200 MB with 06-2 and 06-5 fixed
  (the parts are in 06-10). The limit is 512 MiB today.
- **Number of objects.** 2.7 to 4.5 million log chunks after 90 days
  (06-12), up to 7 days of older versions of the deleted chunks (about
  200,000 to 350,000), 3 checkpoints and their older versions (about
  1,000), and `signins.pb` with up to 600,000 older versions (06-7). With
  the rule of 06-7: about 3 to 5 million objects in all.
- **Write latency of a post.** One chunk write (50 to 100 ms, the design
  says), plus the wait for the write before it: p50 about 60 to 120 ms.
  The p99 comes from GCS, and the spike must measure it. Without 06-1,
  add the queue of other calls under the lock: at a peak of 20 calls
  that need the log, the last one waits 1 to 2 s.
- **Cost.** GCS: $15 to $20 each month (06-11). Cloud Run: about $50.
  Egress: small if 06-3 is fixed, else possibly hundreds of dollars.

### 2. Where is the first limit, and when?

In order, at the target:

1. **The `who` calls of the status line (06-3).** They grow with the
   square of the sessions. They come first in money and in CPU, at about
   20 to 30 people.
2. **The one lock across the write (06-1).** A burst of 10 to 20 calls
   each second that need the log fills the lock. Bursts come at a wave
   start, when many workers finish, and at a pause or a resume, which
   wake each session. At about 20 to 30 people. A slow GCS write stops
   the whole riff at any size.
3. **The state that never forgets (06-2).** The checkpoint, the memory
   and the start time grow each month. It shows after 3 to 6 months.
4. **Memory (06-10).** Not in the first year if 06-2 and 06-5 are fixed.
5. **The 1,000 requests of one instance (06-9).** At about 100 people.
6. **GCS.** Not a limit: less than 1 write each second on average, each
   object has a new name, and only `signins.pb` gets a write each
   second (the GCS limit is about one write each second for each
   object).

### 3. Are the defaults good for the target?

- **90 days of log.** Good for capacity: about 2 GB (50,000 records of
  about 500 bytes each day). But after day 90 the log has no start, so
  the checkpoint is the only copy of the state before it. Keep more
  checkpoints (06-8).
- **A checkpoint each 1,000 records or 10 minutes.** At 50,000 records
  each day (about 0.6 each second), 10 minutes comes first: about 350
  records. That is good. Add a limit on chunks (06-4), because the
  replay cost is in chunks, not in records.
- **N = 200 in `read`.** Too large. 200 messages are about 170 KB,
  more than one tool result of an agent can hold. Use a limit in bytes
  too, for example 64 KB, and give `all` the same limit (06-6).

### 4. What must `riff server` show, to see a limit before we hit it?

Add these facts to the list in "Monitoring". Each has a warning level.

| Fact | Warning |
|---|---|
| Time that a call holds the lock and waits for it, p50 and p99 | p99 more than 100 ms |
| Chunk write, p50, p99 and the slowest in the last hour; records waiting in the queue | p99 more than 500 ms |
| Calls each second and bytes out each second, for each method | `who` more than half of all calls |
| Open watch and tail streams | more than 700 (the limit is 1,000) |
| Memory in use, against the limit | more than 70% |
| Sessions, read cursors, threads (with direct threads), index entries, messages in memory | a count that grows each week |
| Checkpoint: size, time to encode, time to write, records and chunks since | size more than 50 MB, encode more than 200 ms |
| Last start: time of the lease wait, of the checkpoint load, of the replay; chunks read | replay more than 10 s |
| `signins.pb`: size, writes each hour, live chains | more than one write each second for an hour |
| DPoP replay cache: IDs held, against 100,000 | more than 50% |

Cloud Run has its own metrics for memory use and for requests at a
time. An alert on these two covers the time when `riff server` cannot
answer.

## Questions for the lead

1. How many live sessions and how many new session IDs does one person
   make each day? This review uses 8 and 20. The findings 06-2, 06-3,
   06-7 and 06-9 scale with these numbers.
2. Does the fix of the status line (06-3) go in the wave of the store,
   or in a wave before it? It needs no store, and it is the first limit.
3. 06-1 lets reads see only the records up to the last written
   position. Is this the right rule, or must each reply also wait for
   its own write, as the design says now?
