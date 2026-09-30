# Review 02: the on-call operator at 3 a.m.

Issue: #303. Design at commit bf325d9.

## Summary

The log is a good base for an operator. A chunk is written in one piece
or not at all, a start replays only a short tail, and each change has a
position that a tool can name. But the design does not say what the
server does when a write to GCS fails, when a chunk write finds a chunk
of the same name, or when a start meets a bad chunk. Three of the
recovery paths that it gives do not work as written: "remove the bad
chunks", "replay from the start of the log", and "a rollback works"
after a new record kind. A deploy that does not start takes the riff
down, because the new instance stops the old one before it loads. And
nothing tells the operator that the riff is down: each fact is in
`riff server`, which a person must run.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 02-1 | The design gives no rule for a failed chunk write: a GCS 429, 503, a timeout, or a failed token from the metadata server. The queue grows, the replies wait, and nobody knows for how long. The state in memory and the log can go apart. | must-fix | The log / Chunks | Retry each write with backoff for a fixed time (for example 10 s). A retry after a lost reply can get 412: read the object, and when its bytes are the same, count the write as done. After the time, stop for good: 503 to each call, one ERROR line that names the chunk and the error, and exit, so that Cloud Run starts a new instance. The new instance replays from GCS. So memory never goes ahead of the log for long, and the fix for a GCS outage is to wait. |
| 02-2 | The design says that no two chunks have the same name. During a deploy this is not true. The old instance can finish a chunk write after the new instance listed `log/`. The new instance then writes a chunk with the same first position. The design does not say that the write uses `ifGenerationMatch=0`, or what an instance does on 412. If the new instance writes under a new name, the records of the old chunk are in GCS but not in memory: the next checkpoint leaves them out, and a later replay puts them back. | must-fix | The log / Chunks; The lease | Write each chunk and each checkpoint with `ifGenerationMatch=0`, as `gcs.rs` does for a new object today. On 412: an instance that holds the lease lists `log/` again, replays the new chunks, and writes its records after them. An instance that does not hold the lease stops for good (as R141). Add a test with two instances on one store. |
| 02-3 | A deploy that does not start takes the riff down. Today `main.rs` binds the port before `Service::load`, and the default startup probe of Cloud Run is TCP on the port. So Cloud Run moves the traffic at once. Then the new instance takes the lease, and the old instance stops for good. When the replay of the new build then fails (a bug, a checkpoint that it cannot read, a panic in `apply`), no instance serves. The design keeps the same order: "take the lease, wait", then load. | must-fix | The start and the replay; The lease; Deploy and rollback | Load and replay first, with no lease, read only. When the load works, take the lease, wait 15 s, and replay the chunks that came since. Bind the port, or pass a startup probe on a route such as `/v1/build`, only then. When the load fails, the instance exits before it takes the lease. The new revision fails, Cloud Run keeps the traffic on the old revision, and the old instance serves on. |
| 02-4 | "Remove the bad chunks, then start" does not work. The replay checks that each `thread_seq` is the last seq plus 1 (Appendix A). A removed chunk in the middle makes a gap, so the start fails again. Object versioning does not help a chunk or a checkpoint either: each one is written once under a new name, so it has no older version. Versioning helps only against a delete or an overwrite (`lease`, `signins.pb`), for 7 days. | must-fix | Backup and restore; Appendix A | Restore is a cut of the log at a position: `riff-server log cut --after POSITION`. It deletes each chunk and each checkpoint after that position, and prints how many records it removed, in which threads. It refuses to cut before the oldest kept checkpoint. The book gives the steps: stop the server (02-6), `log verify`, `log cut`, start. Say that a cut loses each change after the position, and name them. |
| 02-5 | The lifecycle rule deletes `log/` objects after 90 days. The server keeps only the last 3 checkpoints, which is about 30 minutes at the target (one each 10 minutes). The design says that a build that cannot read the newest checkpoint "replays from the start of the log". After 90 days that start is gone. The members, the owner, the leads and the settings are only in a checkpoint then. A rollback past a checkpoint schema change, 30 minutes after the deploy, loses them all. | must-fix | The checkpoint; Deploy and rollback; Set up | Do not delete chunks by age in GCS. The server deletes a chunk only when it is older than a checkpoint that it keeps, and it keeps the newest checkpoint of each schema version that a supported build reads (N and N-1). Or keep one checkpoint each day for 90 days. Then "replay from the start" is never needed. Remove the claim from the design. |
| 02-6 | "Stop the server" has no command. The service runs with `--min-instances 1` (`deploy/deploy.sh`), so Cloud Run starts a new instance when the old one ends. While a server runs, it writes chunks and checkpoints, so the operator cannot change the bucket under it. | should-fix | Backup and restore | Add a hold mode: `RIFF_HOLD=1` on the service (`gcloud run services update riff-server --update-env-vars RIFF_HOLD=1`). The new instance takes the lease, so the old one stops, then it replies 503 and writes nothing. The operator changes the bucket, then removes the variable. Give both commands in the book. |
| 02-7 | A build that does not know a record kind skips it. When that build writes a checkpoint, the checkpoint holds the position after the record, but not its change. Roll forward again, and the newer build loads that checkpoint and never applies the record. The change is lost with no error. So "a rollback works" is true for the replay, but false for the state. | must-fix | The records; Deploy and rollback | A build that skipped a record writes no checkpoint after the position of that record. Or the checkpoint holds the lowest position that it skipped, and a build that knows the kind replays from there. Log each skip at WARN with its position, and count skips in `riff server`. Add a test: new build, old build, new build. |
| 02-8 | Each change waits for its GCS write (50 to 100 ms) under the one lock (Event sourcing). While GCS is slow, each call waits: `who`, `read`, the watch, not only posts. The operator sees a riff that hangs, with no error. It also stops the batch of records in a chunk that the Chunks section describes. | should-fix | Event sourcing; Chunks | Do `handle`, the append to the queue and `apply` under the lock, then wait for the write outside the lock. With 02-1, a write that fails for good stops the server, and the replay drops the change that nobody got a reply for. Wake a session for a record only after its write. |
| 02-9 | Only some replies wait for their write ("a post, a claim, a pause"). A crash loses each other change that the caller saw as done, for example a lead or a thread join. The design does not list them. | should-fix | Chunks | Each reply that says "done" waits for its write. At less than 100 requests each second, the cost is small. Or list each change that does not wait, and say why a loss is safe. |
| 02-10 | `signins.pb` is written at most one time each second, and the refresh reply does not wait. After a crash the file can be one generation behind, which the design allows. But while GCS writes fail, it gets many generations behind. After the restart, each refresh looks like reuse, and the server ends each such sign-in. The operator must then ask each person to run `riff login` on each machine. Today the reply waits for the save (R128). | should-fix | The sign-ins | Keep R128: a refresh reply waits for the save, or refuses with `UNAVAILABLE` while the file is not saved. Count failed sign-in writes in `riff server`. |
| 02-11 | Nothing tells the operator that the riff is down. `riff server` shows the facts only when someone runs it, and only while the server answers. | should-fix | Monitoring | `just cloud setup` adds a Cloud Monitoring uptime check on `/v1/build` and a log alert on the ERROR lines of riff-server, each with an email to the owner. Check the price when you set up; both are small at this load. |
| 02-12 | The logs are plain text. `tracing_subscriber::fmt()` in `main.rs` writes lines with the level in the text. Cloud Logging reads the severity only from a `severity` field in a JSON line. So `severity>=ERROR` does not find the errors of riff-server, and 02-11 cannot use it. The design names no log line for any failure. | should-fix | Monitoring | Write JSON lines with `severity` on Cloud Run (the `json` feature of `tracing-subscriber`). Give each failure of the table in "The questions" one fixed ERROR text that names the object, as 01M3MMXYS1V8CA89D2XHKPR6C4 does for a load today. |
| 02-13 | The facts in `riff server` miss what the operator needs first: whether the instance serves or replies 503, and why (the lease, a hold, a stop); the last error with its time; the skipped records (02-7); the memory in use and the limit. "Write errors since the start" resets at each restart, so a crash loop shows 0. | should-fix | Monitoring | Add these facts. Show the number of starts in the last hour, from the log or from the lease writes. |
| 02-14 | The tools cannot run where the operator is. `gcs.rs` gets its token only from the metadata server of Cloud Run. So `riff-server log` and `log verify` do not run on a laptop. | should-fix | Tools | The GCS store also takes the token of `gcloud` (application default credentials) when it runs outside Cloud Run. The book gives the command with the bucket, for example `riff-server log verify --bucket como-riff-state`. |
| 02-15 | `log verify` reads each chunk. At the target that is 30,000 chunks each day, and 2.7 million in 90 days while there is no compaction. At about 50 ms for each read, one after the other, that is more than a day. It cannot help at 3 a.m. | should-fix | Tools; Compaction | `log verify` starts at the oldest kept checkpoint by default, takes `--from POSITION`, and reads chunks in parallel. The list uses the `startOffset` of the GCS list call, not the whole `log/` prefix. Bring compaction into the first build, or measure the list and the verify at 2.7 million objects. |
| 02-16 | The instance has no disk: the file system of Cloud Run is in memory and counts against the memory limit. `deploy/deploy.sh` sets no `--memory`, so the limit is the default, 512 MiB. The design writes no local file on Cloud Run, so a "full disk" is a full memory. The queue (02-1), the last N messages of each thread and the index of each message grow in memory. When the instance goes over the limit, Cloud Run stops it, and only the logs say why. | note | Reads; Chunks | Set `--memory` in `deploy.sh`. Limit the queue (02-1 does). Measure the memory of the index with 90 days of messages at the target. |
| 02-17 | A crash in the middle of a chunk write loses nothing that a caller saw as done. The store uploads each object in one request (`uploadType=media`), and GCS keeps a whole object or none. The reply waits for the write. The retry of a post after a crash is a copy, and 01M3JEJVXXEPPNGT3FY4ZSFCWZ refuses it, when the replay puts the signatures of the last 5 minutes back in memory. | note | Chunks; Signed messages | Keep the copy check in `apply`, so that the replay gives it back. Add a replay test for it. |
| 02-18 | A rollback past go-live is a new start. The old build reads the old objects (`threads/`, `sessions`, `tokens`), not the log. Each change after go-live is lost for it. | note | Go live | Say it in the go-live steps. Keep the old objects until the first wave after go-live ends. Both builds use the object `lease`, so an old instance and a new instance never serve at the same time. Keep that name. |

Levels: must-fix (the design fails its goals without it), should-fix (a
real gain), note (for the lead to know).

## The questions of this point of view

### 1. For each failure: what do I see, and what do I run?

"As written" is the design today. "With the proposals" adds the
findings.

| Failure | What I see, as written | What I see and run, with the proposals |
|---|---|---|
| A crash in the middle of a chunk write | Clients wait through the gap of about 15 s. The logs show the crash (a panic, or "Memory limit exceeded" from Cloud Run). No record is lost (02-17). | The same, and the uptime check emails me when the gap is long (02-11). I run nothing. If it repeats, I read the ERROR line (02-12). |
| A bad chunk | The design does not say. A replay gap stops the start, maybe. I cannot remove the chunk (02-4). | The start stops with one ERROR line that names the chunk and the position. I run `riff-server log verify --from POSITION`, set `RIFF_HOLD=1` (02-6), run `riff-server log cut --after POSITION`, and remove the hold. With 02-3, a new build that meets the chunk fails, and the old instance serves on. |
| A bad checkpoint | The start uses the one before it. I see nothing, unless I compare the checkpoint age in `riff server`. | The same, and a WARN line names the checkpoint. `riff server` shows the age of the newest good checkpoint. I run nothing. When all 3 are bad, the start stops, and I cut or roll back (02-5). |
| A lost lease | The instance replies 503 when a lease read fails for more than 5 s. It stops for good when it reads another ID. The WARN text is "the lease read failed". | `riff server` shows "503: the lease read fails" and the last error (02-13). When another ID holds the lease, the other instance serves, and I run nothing. |
| A GCS error | The design does not say (02-1). Replies wait with no limit. `riff server` counts write errors, if it can answer. | The server retries for 10 s, then stops with an ERROR line, and Cloud Run starts it again. The alert emails me (02-11). I look at the GCS status page, and I wait. When GCS comes back, the riff comes back with no action. |
| A full disk of the instance | There is no disk. A full memory stops the instance, and Cloud Run starts it again (02-16). | The Cloud Run log says "Memory limit exceeded". `riff server` shows the memory. I raise `--memory` with a deploy, and file an issue for the cause. |
| A deploy that does not start | The new instance stops the old one first, so the riff is down (02-3). I roll back: `gh workflow run CI --ref main -f tag=vX.Y.Z`. | Cloud Run marks the new revision as failed, and the old one serves on. I read the ERROR line of the new revision, and file an issue. I roll back only for a bug that shows after the start. |

### 2. Can I restore to a known point, and how long does it take?

As written: no. The only restore step is to remove bad objects, and a
removed chunk makes a gap that stops the start (02-4). The only known
points are the last 3 checkpoints, about 30 minutes (02-5).

With 02-4, 02-5 and 02-6: yes, to each position after the oldest kept
checkpoint. The steps take about 5 minutes of work: a hold, a verify of
the range, a cut, a start. The start itself takes about:

- 15 s for the lease wait, plus 1 s,
- one read of the checkpoint,
- the replay of at most 10 minutes of chunks: about 210 chunks at the
  target. One after the other at 30 to 60 ms each, that is 6 to 13 s.
  In parallel, it is less.

So about 30 s. When the newest checkpoint does not read, add 6 to 13 s
for each older one. Measure the replay of 1,000 chunks in the build item
of the checkpoint.

### 3. Is each failure visible, and can I recover from each one without data loss in the log?

As written: no, for three reasons.

- Visible: no failure makes an alert, and the logs have no severity
  that a filter can use (02-11, 02-12). A GCS stall looks like a hang
  (02-8).
- Recovery: there is no working path for a bad chunk (02-4), and none
  to stop the server (02-6). A deploy that does not start is an outage
  (02-3).
- Loss: a record that an old build skipped can be lost after a roll
  back and forward (02-7). The chunks of an old instance can go out of
  step with the state in memory (02-2). A rollback past a checkpoint
  schema change can lose the members after 90 days (02-5). The sign-ins
  can be lost after a GCS outage (02-10).

With the must-fix findings, each failure of question 1 is visible and
has a recovery. Only a cut (02-4) loses records, and the cut names them.
That loss is the choice of the operator, not a silent one.

## Questions for the lead

1. Is a cut of the log (02-4) the restore that we want? The other way
   is a tool that skips one bad record. That keeps more data, but the
   state after it can differ from the state that the people saw.
2. Who is on call? The alerts of 02-11 go to the owner. Should they
   also go to each admin?
3. Do we keep chunks by age (90 days) or by checkpoint (02-5)? The
   second keeps the log smaller when the load is low, and never breaks a
   replay. But it needs code in the server, not a lifecycle rule.
