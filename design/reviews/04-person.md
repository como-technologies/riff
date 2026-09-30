# Review 04: the person in a riff

Issue: #305. Design at commit bf325d9.

## Summary

For a person, the design changes little from day to day. A post and a
claim get about 100 ms slower, and no change is lost any more. The
start gap of a deploy looks the same as today. The go-live is the one
day that hurts: each person signs in again, the owner invites each
member again, and the history is gone. Two steps of the go-live list
do not work in the order that the design gives them. After each
deploy, a session can get a wake and a read for a message that it
already acted on, because the read cursors live only in the
checkpoint. The design names gRPC codes for errors, but not the words
that a person reads, and it gives no way to read past the first 200
messages.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 04-1 | The reply of a post or a claim waits for the chunk write under the one lock. So each `who`, `top` and `read` waits behind a write of 50 to 100 ms, and a slow write from GCS stops the whole riff for its duration. The queue can hold only the records of one command, so "a chunk holds a few records" is not true: each chunk holds one command. Today the handlers do no I/O under the lock (`crates/riff-server/src/lib.rs`, "Design"). | should-fix | Event sourcing; The log / Chunks | Run `handle` and `apply` under the lock, then release the lock and wait for the write outside it. Reply after the write. When a write fails after its retries, stop the instance: the next start replays the log without the record. Measure: the p99 of `who` while 10 sessions post, and the number of records in each chunk. |
| 04-2 | The read cursors live only in the checkpoint, and the design writes a checkpoint each 1,000 records or each 10 minutes. Today the server saves each change within one second and on SIGTERM. So after each deploy, each session whose cursor moved since the last checkpoint reads the same messages again, and `missed` wakes it again for a message that it already acted on. A wake costs the session a read of its whole context. A verify request that comes twice can start a second verify. | should-fix | The checkpoint; The classes of data | The old instance writes a checkpoint when it sees the new lease, inside the 15 s wait of the new instance, and on SIGTERM. Its name is its position, so it cannot collide. Then a deploy loses no cursor. |
| 04-3 | `read` gives at most 200 messages and says how many it did not give. The design names no way to get the rest. Today `riff read --all` gives the full history: in one session today it gave 337,651 characters in 1,893 lines, and Claude Code refused the result as too big and wrote it to a file. The limit is a gain, but a session that meets it has no next step. | should-fix | Reads | `read` takes a `before` seq, or a page number. The text of a cut read says the exact next call. `riff read --all` in a shell pages by itself. |
| 04-4 | Go live, step 4 says each person signs in again, and step 5 says the owner invites the members again. riff-server refuses the sign-in of a person with no invite outside the allowed domains: "EMAIL is not a member of this riff; ask its owner to run: riff invite EMAIL" (`crates/riff-server/src/token.rs`). So each member who follows the list gets a refusal. | should-fix | Operations / Go live | Order the steps: the owner signs in, the owner invites each member, then each member signs in. Say what a member sees before the invite. Or import the members from the old `tokens` object one time at go-live, so step 5 goes away. |
| 04-5 | Go live, step 3 says the old riff sees the new build on `/v1/build`. The old riff reads the build from the `riff-build` header of each reply (`check_build` in `crates/riff/src/api.rs`), and the auto-update starts from that header. The new server has only `/v1/build` as a JSON route. When it answers an old route such as `POST /v1/post` with a 404 and no header, the old riff says "no riff build in the reply: status 404 Not Found from URL ... Update riff-server, on the machine of the riff": the wrong step, and the auto-update never starts. | should-fix | The wire / gRPC for the calls; Operations / Go live | The new server answers each old `/v1/` route with the `riff-build` header, for example 410 Gone with the header. Then the old riff says "Update riff on this machine, then start your sessions again", and a machine with `update.auto` updates itself. Test: an 0.8 riff against the new server. |
| 04-6 | "The client waits through the gap and shows no error." Today this is what a person sees in the gap: a shell command such as `riff who` hangs with no words for up to 15 s; `riff top` keeps its old table with no sign, and exits with an error after the 60 s busy limit; `riff chat` at its start says nothing for the gap; a chat line typed in the gap blocks the chat screen until the post goes, because the post runs inside the select loop (`crates/riff/src/chat.rs`). A stream shows `(reconnecting…)` and `(back)`. The design keeps all of this. | should-fix | The lease; Live messages | Say in the design what the person sees. After 2 s of waits, a command prints one dim line on stderr, for example `(riff-server starts, waits…)`, and `(back)` when the reply comes. `riff top` shows the line under the table. The chat sends a line in a task, so the screen goes on. |
| 04-7 | The checkpoint "holds the state that the log gives" and "memory keeps the last N messages of each active thread". The design does not say whether the checkpoint holds those messages. When it does not, the memory after a restart holds only the messages after the checkpoint. The first `riff chat` and the first `read` of each thread then come from the chunks: at 30,000 chunks a day, one GET each of 50 to 100 ms for each message, so 200 messages can take 10 to 20 s. Compaction is not in the first build. | should-fix | The checkpoint; Reads | The checkpoint holds the last N messages of each thread. Measure: the time of `riff chat` at its start, and of a `read` of 200 messages older than memory, after a restart with one day of chunks. When the measure is over 2 s, compaction goes into the first build. |
| 04-8 | The log keeps records 90 days. Today an active thread keeps each of its messages, and a thread with no change for 30 days goes away whole (`crates/riff-server/src/gcs.rs`). So a quiet thread lasts longer, and an active thread loses its messages older than 90 days, which it keeps today. The index in memory can name a position in a deleted chunk. The design does not say what a read of a gone message gives. | should-fix | The log / Chunks; Operations / Set up | A read of gone messages gives a count, not an error: "N older messages are gone: the log keeps 90 days". The book says the rule. |
| 04-9 | The design names gRPC status codes, not the words that a person reads. Today the book has a table "Get back into the riff" with each error text and the step. A person can meet new errors: the busy limit after a gap, a chunk write that fails, a read cut at N, gone older messages, and a sign-in that ended by reuse. The design gives the words of none of them. | should-fix | The wire / gRPC for the calls | Each gRPC status comes with a reason string in plain words, with the step. The design lists each new error in the form of the book: the text, what it means, the step. |
| 04-10 | A build that meets a signature scheme that it does not know shows the message as `(not verified)`. Today that mark means that the signature did not check, and a message with it never counts as the lead (R200). For an unknown scheme it means "this build cannot check it". A person reads the first meaning, and a request of the lead stops to count with no word why. The two-release rule makes it rare. | note | The log / Signed messages | A third mark, for example `(not checked: update riff)`, so the person knows the step. |
| 04-11 | gRPC needs HTTP/2 from the client to the server. Today the JSON calls and the streams work over HTTP/1.1 too, so they pass a proxy or a VPN that speaks only HTTP/1.1. With gRPC, such a proxy breaks each call. The person sees "cannot reach riff-server" on one network and not on another. | note | The wire / gRPC for the calls | Before the client moves, each person runs `riff who` from each network that they use (office, VPN, home) against a test server with gRPC. The result goes in the release issue. |
| 04-12 | Sessions and presence live in memory. After a restart, `who` and `top` show a session when its watch connects again or when its next call comes. The design does not say when the 5-minute claim timer starts after a restart. Today the book says: "After a restart with a bucket, each session counts as stopped. Its claims stay for 5 minutes." | note | The classes of data; The lease | Say it in the design: a restart is a stop of each session, the claim timer starts at the restart, and a session comes back with its claims when its watch connects again. |

Levels: must-fix (the design fails its goals without it), should-fix (a real gain), note (for the lead to know).

## The questions of this point of view

### 1. What is faster, slower or different: a post, a read, `riff top`, the start of chat, a claim?

- A post and a claim: slower by one chunk write, 50 to 100 ms. Today
  a post replies from memory, and the server saves within one second
  (`SAVE_EVERY` in `crates/riff-server/src/lib.rs`). Only the
  sign-in, member and owner calls wait for the save today. A person
  does not feel 100 ms. An agent makes one tool call
  for a post, so it does not feel it either. The gain: a post that
  got its reply is in the log. Today a crash can lose the changes of
  the last second.
- A read: the same when the messages are in memory. A read is cut at
  200 messages, with a count of the rest (04-3). A read of older
  messages comes from the chunks and can take seconds (04-7).
- `riff top`: the same. It is a view of the state in memory. It waits
  behind each chunk write while the write is under the lock (04-1).
- The start of chat: the same when the checkpoint holds the last N
  messages of the chat thread. When it does not, the first chat after
  a restart reads its history from the chunks (04-7).
- A claim: the same as a post. A claim after a restart comes back
  with the session, as today (04-12).

### 2. What do I see during the start gap of about 15 s, and at go-live?

The gap is the same as today: the new instance takes the lease and
waits 15 s, the old instance replies 503, and riff waits through it.

- A shell command hangs with no words, up to 60 s.
- `riff top` keeps its old table. It exits with an error after 60 s.
- `riff chat` shows `(reconnecting…)`, then `(back)`. A line that I
  type in the gap blocks the screen until the post goes.
- A session gets one wake after the gap when an addressed message is
  unread. With 04-2, it can get a wake for a message that it already
  acted on.

See 04-6 for what the design can say and show.

At go-live, in the order that a person meets it:

1. The lead ends the wave, and the riff is paused. The lead writes
   its handoff in the release issue. Each worker stops.
2. The tag deploys the new server. Each riff command on each machine
   now fails. With 04-5 the words say to update riff-server; the
   right step is `riff update` on each machine, or the auto-update.
3. After the update, each command says "This riff is new. Run riff
   login." (the riff ID changed, as after a restart with no bucket).
4. `riff login` fails for each member until the owner signs in and
   invites them (04-4).
5. `riff read --all` and `riff chat` show nothing. The history is
   gone. `riff who` shows no lead. Each lead runs `lead` again and
   reads its handoff from the release issue.
6. The lead starts its workers again, and the owner resumes the riff.

It is one day, one time, and the list in the design needs the order
of 04-4 and the fix of 04-5.

### 3. Which errors can I meet that I do not meet now? Are their words clear?

The design gives no words (04-9). The new errors:

| Error | When | Words today |
|---|---|---|
| The busy limit ends in a gap | a start that takes more than 60 s | "riff-server at URL does not answer: its front end replied 503" |
| A chunk write fails | GCS refuses the write after the retries | none: the design does not say what the reply is |
| A read is cut | more than 200 unread messages | none: the design says "it says how many it did not give" |
| Older messages are gone | a read past 90 days | none (04-8) |
| A sign-in ended by reuse | a refresh token comes back a second time | "the refresh token was used before; sign in again" |
| A gRPC status | each refused call | a code: `UNAVAILABLE`, `UNAUTHENTICATED`, `PERMISSION_DENIED`, `FAILED_PRECONDITION` |
| An unknown signature scheme | a build behind by one release | `(not verified)`, which means something else today (04-10) |
| HTTP/2 blocked | a proxy or a VPN that speaks only HTTP/1.1 | "cannot reach riff-server at URL" (04-11) |

The words that exist today are clear. Each new error needs its text
and its step in the design, and then in the book.

### 4. Does anything feel worse than now?

- The go-live day: sign in again on each machine, wait for the invite,
  lose the history. One time.
- After each deploy, a wake and a read for messages that I already
  acted on (04-2). Each deploy, until the fix.
- The first chat and the first read after a deploy can take seconds
  (04-7), until the checkpoint holds the messages.
- `riff read --all` stops at 200 with no next step (04-3).
- An active thread loses its messages older than 90 days (04-8). The
  lead keeps its notes on GitHub, so I did not miss them yet.
- Everything else feels the same or better: a post is never lost, a
  quiet thread lasts 90 days and not 30, and the gap looks as it does
  today.

## Questions for the lead

1. Does the checkpoint hold the last N messages of each thread, or
   only the state and the cursors (04-7)?
2. What is N for `read`, and does `riff read --all` page by itself
   (04-3)?
3. At go-live, is a one-time import of the members from the old
   `tokens` object acceptable, so that the owner does not invite each
   member again (04-4)?
4. Is a third mark for an unknown signature scheme wanted, or does
   `(not verified)` stay (04-10)?
5. Does the client keep the 60 s busy limit for a gap, or does the
   limit become a setting (04-6)?
