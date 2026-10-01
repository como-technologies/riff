# Review engine-01: the maintainer a year from now

Issue: #356. Design at commit 8006647.

## Summary

The design is good for a maintainer. It has one entry for each change,
one trait for each command, records that name their cause, and a
life cycle that is a function of three facts. A new command is small.
Two parts fail the goals as the page is written. The sketch lets the
future of the call move a command from `Written` to `Applied`, so a
call that the client drops leaves no apply in order, no journal line
and no wake. The value `other` lets an older build write a checkpoint
past a record that it read only in part, so a rollback and a roll
forward lose state. The other findings are places that a person can
forget with no signal: the role of a command, the list of kinds, the
checkpoint, and the `compile_fail` tests. Decide four names before
1.0.0, because they are in each record for good: the form of `by`, the
kinds of the commands, the fields that hold a person, and where the
rule for `used` lives.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| engine-01-1 | In the sketch of `Engine::dispatch`, the future of the call waits for the chunk, then does `written.apply()` and `applied.finish()`. A client that goes away drops that future. Then the records are in the log, but nothing applies them to the written copy, and the command gets no journal line and no effect. Two calls of one chunk can also apply in the wrong order. | must-fix | One path; The pipeline in the types | The writer task moves each command from `Queued` to `Written` to `Applied`, in the order of the positions. It writes the journal line and sends the effects. The call waits only for its reply. Say that a dropped call changes nothing of this. Add a test: drop the call while the write waits, then look at the written copy, the journal and the wake. See the notes. |
| engine-01-2 | A build reads a value that it does not know as `other`, and `apply` takes it "in the safe way". The record is not skipped, so the rule "no checkpoint past a skipped record" does not hold it. Build N writes `pause_set` with a new scope. A rollback to N-1 comes before N wrote a checkpoint. N-1 applies nothing for the record and writes a checkpoint past it. Build N starts again from that checkpoint, and the pause is gone. | must-fix | What the rules for a change of a record say; decision 11 | A record with a value that the build read as `other` counts as a skipped record for the checkpoint rule. `riff server` shows it in the count of skipped records. Add the case to the test with a new build, an old build and the new build again. See the notes. |
| engine-01-3 | Each command has a `PATH`, also `announce`, `forget`, `grant_owner`, `end_owner` and `admit`. In a riff with no sign-in "each role check passes", and the caller is the `me` of the body. So a call to `/v1/grant_owner` or `/v1/set_admin` works for each caller of the network. | should-fix | One path; The callers; Who can make which move | Two traits: `Command` (kind, `handle`, `reply`, `args`) and `Routed: Command` (the path). Only a `Routed` command gets a route. The commands of the server and `admit` are not `Routed`. Say that each command of the group "people" is refused in a riff with no sign-in. See the notes. |
| engine-01-4 | "The engine checks each role in `handle`", but the sketch of `Claim::handle` writes the check by hand. A new command with no check compiles and passes its own tests. The table "Who can make which move" is only in the book. Nothing compares the code with it. | should-fix | One path; Who can make which move; The moves that `handle` refuses | One function has the table: `permits(the class of the caller, the state of its life cycle, the kind of the command)`. The engine calls it before `handle`. A new kind with no row does not compile (a `match` with no `_` arm). One test runs each command as each class of caller and compares the result with the table. |
| engine-01-5 | "Each kind is in `Change::KINDS`". The list is written by hand. `Line::parse` reads a kind that is not in the list as a record of an unknown kind, and the replay skips it. So a new variant that I forget in the list works on the live path and is lost at each start. The test of today takes its records from a second list by hand. | should-fix | The records | Make the enum and the list from one place: a derive (for example `strum::VariantNames`) or one macro. The fixture test of E6 fails when a name of `KINDS` has no record in the fixture. |
| engine-01-6 | The design adds state that the log gives: the people and their roles, the request for the owner role, the `used` mark and the worker mark, the two pauses, and the time of the last `member_removed` and `signins_ended` of each user. The page does not name the checkpoint. The snapshot is a second set of types that are filled by hand. A field that I forget there is lost at each start from a checkpoint. | should-fix | The records; The life cycle of a session; Build items | Say in E3, E4 and E5 that each new part of the state goes into the checkpoint. Make the snapshot from the state type with serde, not by hand, or keep the two and add a test: each fixture log gives the same state from a full replay and from a checkpoint at each position. |
| engine-01-7 | "A doc test with `compile_fail` shows each of these rules." A `compile_fail` test passes on each compile error. After a rename of a type or a method, the test fails to compile for the wrong reason and stays green. The rule can then break with no signal. | should-fix | The pipeline in the types | Each `compile_fail` test has a twin doc test that compiles. The twin differs in one line: the line that the rule forbids. Or use `trybuild` and keep the compiler message of each case. Make sure if the stable rustdoc checks an error code in `compile_fail,E0NNN`. |
| engine-01-8 | The server gets `C::PATH` and `C::Reply` from the trait. The client writes the path as text for each call and names the reply type by hand. A new command or a new path changes two crates, and only a test of the two together finds a wrong path or a wrong reply type. | should-fix | One path; The pipeline in the types | Put a small trait in `riff-core` next to the wire type: the path and the reply type. `Routed` of the server builds on it. The client gets one generic `call::<C>`. Say that the command types are the types of `riff_core::wire`, not new ones. |
| engine-01-9 | The journal holds the reason of a refusal as text. The text changes between builds. A filter that works today finds nothing a month later, and a test that "compares the journal line" breaks at each change of words. | should-fix | The journal; The moves that `handle` refuses | `Refused` has a code from a fixed set, for example `held`, `paused`, `must_clear`, `not_allowed`, `not_holder`, `other_user`, and the text. The journal line has `code` and `reason`. The tests compare the code. |
| engine-01-10 | A call that the server refuses before `handle` is not a command, so it leaves no journal line: no token, a bad proof, a token that may not act as the `me` of the body, an old build of `riff`, a body that does not read. A person who got 401 or 403 a month ago finds nothing. The page names the request log of Cloud Run for the queries, but not how long it is kept. | should-fix | The journal; Queries | The token layer writes one journal line for each call that it refuses, with `result` `denied`, the path, the code, and the caller that the call claimed (marked as not proved). Do the same for a query that is refused, for example a `read` of a direct thread of two other sessions. Say how long the request log is kept. |
| engine-01-11 | `by` is one text for four classes of caller: `mike/a6cf`, `mike`, `mike@comotechnologies.io`, `riff`. A reader finds the class from the form of the text. That rule is in each record for good. A fifth class, for example a service account, needs a new form that no old text can have. | should-fix | The journal; The callers | Give `by` the form of `scope`: `{"session":"mike/a6cf"}`, `{"person":"mike"}`, `{"sign_in":"mike@comotechnologies.io"}`, `"server"`. A build reads a class that it does not know as `other`. Use the same form for `caller` in the journal. |
| engine-01-12 | "A new command needs no change of the format: `command` is text." The text is in each record, and it carries meaning: `owner_set` comes from five commands, and only `command` says which. A rename of a `KIND` changes how each tool reads the old records. | should-fix | What the rules for a change of a record say; The records | A rule: never rename a `KIND`, and never use the name of a removed command again. A file in the fixtures lists each `KIND` of each release. A test fails when a name of the list is gone from the code. A reader takes a `command` that it does not know as text, with no error. |
| engine-01-13 | The policy of MustClear is in `apply`: "a `claimed` record makes its session used". After 1.0.0, each change of that policy changes what a replay of the old records gives. An example of a change: a verify claim does not make a worker used. | should-fix | The life cycle of a session; decision 7 | Decide in `handle`, and write the decision in a record. `apply` only stores it. For example, the `released` record of the last claim of a worker has the field `must_clear`, or there is a kind `session_used`. Then a new policy changes only `handle`. |
| engine-01-14 | A claim of another session after the claim timer replaces the holder with one `claimed` record. The old holder gets no `released` record. So the log does not show that a session lost a claim. The life cycle of the old holder changes ("the last claim goes") with no record that names it. | should-fix | The life cycle of a session; The records | `handle` of `claim` gives a `released` record for the old holder, then the `claimed` record. The two are in one chunk. Add `claim` to "Made by" of `released`. Say in the table of the states that this move puts a worker in MustClear. |
| engine-01-15 | The journal names the caller of the token, but not the device key of the token. A person has one name on each machine. So the journal cannot say which device made a `remove` or a `pass_owner`. | should-fix | The journal | Add the thumbprint of the key of the token to each journal line (`key`). It is not a secret. Add the trace ID of the request, so that a journal line and its line in the request log go together. |
| engine-01-16 | The example of a journal line has `at_ms` and no `time` and no `message`. Each other log line of the server has `time`, `message` and `target`. A line with the result `failed` has the severity `INFO` in the examples, so the alert on `severity>=ERROR` does not see it. The page does not say which time `at_ms` is: the arrival of the call, or the result. | note | The journal | A journal line is a line of the format of today, with more fields. `failed` has the severity `ERROR`. Say that `at_ms` is the time of the check, and add `ms`: the time from the check to the result. |
| engine-01-17 | A call of a session that the state does not know runs `register` first, in the same lock. The page does not say if that is one journal line or two, and which `command` the records of the `register` have. | note | One path; The journal | Two journal lines. The records of the `register` have the `command` `register`. The line of the `register` has the field `for` with the kind of the command of the call. |
| engine-01-18 | `owner_asked` and `owner_denied` have a field `admin`. The name says a role, not what the field holds: an email or a user. The other people kinds use `email` and `user`. | note | The records | Name each field for what it holds: `email`. After 1.0.0, the name stays. |
| engine-01-19 | Each command waits for the pending position at its check, also `status`, which makes no record. The measures have about 80,000 `status` commands each day. Today a call that makes no record does not wait. | note | One path; decision 5 | Keep the one rule: it is easy to keep right. Measure it in E1: the time of a `status` call while 8 sessions post, before and after. If the wait is more than the time of one chunk write, say so on the page. |
| engine-01-20 | `pause_set` has `state` in the list of fields with the value `other`. The page says what `apply` does for a scope `other`, and for a reason `other`, but not for a state `other`. | note | What the rules for a change of a record say | Say it: a `pause_set` with the state `other` changes nothing. With engine-01-2, it also stops the checkpoint. |
| engine-01-21 | On one machine, the journal goes to the standard output of `riff-server`. The page does not say where that output goes or how long it is kept. On such a riff, "who did what a month ago" has an answer only for the commands that made a record. | note | The journal | Say it on the page. Or give `riff-server` a flag for a journal file with a size limit. |

### Notes on the findings

**engine-01-1.** The diagram of "One path" is right: the writer writes
the chunk, then comes the apply, the journal, the reply and the
effects. The sketch puts these steps into the future of the call:

```rust,ignore
let written: Written<C> = queued.written().await?; // the chunk
let applied: Applied<C> = written.apply();       // the written copy
Ok(applied.finish()) // the journal, the effects, the reply
```

The web server drops the future of a call when its client closes the
connection. The future is then at the `await`. The chunk is written,
but the two lines after the `await` never run. The code of today does
not have this fault. In `crates/riff-server/src/lib.rs`, the writer
task (`Service::write_log`) applies the records of each chunk
(`State::written`), marks the position as written, and sends each
delivery that waits (`Server::deliver_written`). A call puts its
delivery in the list before it waits (`Server::deliver_after` in
`post_message`). So the design must keep that property, and the types
must show it: the stage `Queued<C>` goes to the writer, not back to the
call. The writer holds commands of different types in one queue, so the
queue holds a boxed stage, or a closure that makes the reply. The page
must say which.

The order is the second part of the fault. `apply` needs the records in
the order of their positions. Two calls whose records are in one chunk
wake at the same time, and each one applies its own records. Nothing in
the sketch puts the two in order.

**engine-01-2.** The checkpoint rules of today are in
`crates/riff-server/src/checkpoint.rs`: a build writes no checkpoint
past the first record that it skipped, and none while the newest
checkpoint comes from a later version. The second rule does not help
here. The server writes a checkpoint each 1,000 records or each 60
minutes, and a rollback comes most often in the first hour of a
release. Then the newest checkpoint is from N-1, N-1 is not blocked,
and it writes the next one. The same failure is possible for a
`session_started` with a new reason: N-1 reads it as "not a fresh
start", and its checkpoint keeps a worker in MustClear that N made
Ready.

A test must also show that the type reads the unknown value. `scope` is
a text (`"riff"`) or an object (`{"repository": ...}`). A new scope can
have each of the two forms. The fixture of E6 needs one record with an
unknown text and one with an unknown object, for each such field.

**engine-01-3.** Today the commands of the server have no route. The
timers call the state direct: `forget_sessions`, `Server::announce` and
`owner_tick` in `crates/riff-server/src/lib.rs`. The routes of the
people (`/v1/invite`, `/v1/remove`, `/v1/admin`, `/v1/owner`, and the
others) are always behind `require_token`, also in a riff with no
sign-in (`Service::router`, `admin_routes`). The design must not lose
the two properties. With two traits, the compiler keeps the first one:
`.route(C::PATH, ...)` does not compile for `Announce`. The constructor
of `Authenticated` for the caller `riff` is the second door: keep it
private to the engine module, and give the timers one function for
each command of the server.

**engine-01-5.** `crates/riff-core/src/record.rs` has the enum
`Change`, the list `Change::KINDS` and `Line::parse`. The test
`each_kind_reads_back_and_has_its_name` compares `KINDS` with the list
of `one_of_each`, by length and by order. A variant that is in no list
passes the test. The design goes from 9 kinds to 20.

**engine-01-6.** `crates/riff-server/src/state.rs` has `Snapshot`,
`SnapshotThread`, `SnapshotClaim`, `SnapshotLead`, `SnapshotSession`
and `SnapshotCursor`. `State::snapshot` and `Snapshot::into_parts` copy
each field by hand. The test
`a_start_from_a_checkpoint_and_the_records_after_it_gives_the_state_of_a_full_replay`
finds a lost field only when its given records set that field. A
fixture with one record of each kind (E6) makes that test complete,
when the test uses the fixture.

**engine-01-7.** The crates have no `compile_fail` test today, so there
is no habit to copy. I did not make sure of the behavior of
`compile_fail,E0NNN` on the stable toolchain. If rustdoc checks the
code only on nightly, the code in the fence gives no safety.

**engine-01-8.** `crates/riff/src/api.rs` has one line for each call,
for example `self.call("claim", &claim(me, thread, item))` and
`self.call("owner/take", &TakeOwner {})`. The request and reply types
are in `crates/riff-core/src/wire.rs`. The sketch of the design shows
`pub struct Claim { me, thread, item }` with `#[derive(Deserialize)]`
in the server. If that is a second type, the wire has two truths.

**engine-01-13 and engine-01-14.** The page already does this right in
other places: a start gives one `released` record for each claim, and
the first session of a user gets a `lead_set` record. In the two cases
`handle` decides, and the log holds the decision. `apply` of `claimed`
in `crates/riff-server/src/state.rs` replaces the holder today, and
`handle` gives no `released` record for the old holder. For the life
cycle and for `riff audit`, the record is necessary.

## The questions of this point of view

### 1. A new command, a new record kind, a new rule for a session: how many places change, and what tells me that I forgot one?

**A new command.** In the design: one type with the trait, and one
line in the list of routes. That is a large gain. Today a command
changes five places in the server: a variant of `state::Command`, an
arm of `State::handle`, a method of `State`, a handler in `lib.rs` that
picks `change` or `settled` by hand, and a route. The places that stay
after the design:

| Place | What tells me that I forgot it |
|---|---|
| The type and its `handle`, `reply`, `args` | The compiler: the trait needs each item. |
| The route | Nothing in the server. The client gets 404. A test of the client finds it. |
| The role and the life cycle check | Nothing (engine-01-4). |
| The client: the path as text, the reply type | Nothing at compile time (engine-01-8). |
| `args`: no body, no token, no key | Nothing. A review only. Add one test that runs each command with a marked body and looks for the mark in the journal line. |
| The tables of the book, the how-to, the skill text, the MCP tool | The rule "User docs" of `CLAUDE.md`, by hand. |

With engine-01-3, engine-01-4 and engine-01-8, the compiler tells me
about each place in the code.

**A new record kind.** The places: the variant of `Change`, the list
`KINDS`, the arm in `apply`, the arm in `named` (which session a record
names), the snapshot when the state gets a new field, the fixture, and
the table of the page. The compiler tells me about `apply` and `named`:
the two are a `match` with no `_` arm (`crates/riff-server/src/state.rs`).
Nothing tells me about `KINDS` (engine-01-5) or the snapshot
(engine-01-6). The equal check of two states is safe: `same_log_state`
compares the whole `Riff` with a derived `PartialEq`.

**A new rule for a session.** The life cycle is a function of three
facts, and it is not in a record. That is the right choice: a new
state costs no change of the format. A rule that holds for more than
one command, for example "a session in MustClear cannot become the
lead", goes into the `handle` of each command by hand. With the one
table of engine-01-4, it is one row. The policy for `used` is the one
part that is not free to change (engine-01-13).

### 2. Do the types stop the faults that we had (a reply before the write, a path outside the pipeline), or only the tests?

**A reply before the write: the types stop it, for a handler.** Today
each handler picks `change` (wait only for the own records) or
`settled` (wait for each pending record) by hand in
`crates/riff-server/src/lib.rs`, and `post_message` has its own copy of
the steps. A wrong pick is a reply that tells of a change that is not
written, and only a test finds it
(`who_shows_a_claim_only_after_its_write` and the tests next to it).
In the design a handler cannot pick: `reply` gets only the written
copy, and only `Applied` gives it. The one wait rule (decision 5)
takes away the choice. This part is real.

**The stages protect the engine from itself, not from the handlers.**
A handler never holds a stage. The stage types stop a person who
changes `dispatch` from a wrong order. That is of value, but it is a
smaller claim than the goal "the compiler enforces the order". What
the stages do not stop is the dropped call (engine-01-1): the types
say which step comes next, not who runs it.

**A path outside the pipeline: the privacy stops it, not the stages.**
The fault of today is that each part of the server can lock the state:
`Server::state` is open to the whole crate, and the timers, `watch`,
`WatchGuard::drop` and the handlers use it. The people commands change
the token store on a second path with its own save (`saved`,
`save_tokens_since`). The design closes this with two things: only the
engine locks the state, and only `apply` gets the riff as `&mut`. Rust
keeps the two only when the fields and the constructors are private to
the engine module. So say on the page which module that is, and that
no child module makes a stage. The command files of E1 must be next to
the engine module, not below it.

**What only the tests stop.**

- The rule that `apply` and `Engine::signal` see two different types
  holds only while nobody adds a method. The `compile_fail` tests keep
  it, when they do not rot (engine-01-7).
- "The guard is not `Send`" stops an `await` under the lock only where
  the future must be `Send`: a handler, and a task that `tokio::spawn`
  starts. A test and a `block_on` do not need it. The guard of
  `std::sync::Mutex` has this property. The guard of
  `tokio::sync::Mutex` does not. Name the lock on the page.
- The roles and the life cycle: only tests, until engine-01-4.
- The routes of the commands of the server: nothing, until
  engine-01-3.
- The sign-in chains stay outside. After E3, `signins.json` must hold
  no role and no member. A test must show that the sign-in module has
  no path to the people of the state.

### 3. Is the journal enough to answer "who did what, when, and why was it refused" a month later?

For a command that made a record: yes, and from the log alone. `by`
and `command` in each record are the best choice of the page. The log
does not lose a line, and `riff audit` needs no second source.

For the other cases, not yet:

- **Why was it refused.** The reason is text that changes
  (engine-01-9). A refusal before `handle` leaves no line
  (engine-01-10): no token, a wrong `me`, an old `riff`. These are the
  refusals that a person asks about.
- **Who.** The journal has the user and the session, not the device
  (engine-01-15). The place of a session is in its `register` line
  only when `args` of `register` holds it: say so on the page. After
  the server forgets a session, that line is the only place that says
  where the session was.
- **When.** One time, and the page does not say which
  (engine-01-16).
- **What.** `args` is enough to find the change. For `remove` and
  `revoke`, add the count of the sign-ins that ended to the journal
  line: it comes from an effect, and no record holds it. The log line
  of today has it (`remove` and `revoke` in
  `crates/riff-server/src/lib.rs`).
- **A month later.** In the cloud, 400 days: yes. On one machine, the
  page does not say (engine-01-21). The journal is not in the log of
  the store, so a line can be lost when the instance stops between the
  write and the line. With engine-01-1 the writer makes the line, and
  the gap is small. Say on the page that the journal is "best effort"
  and that the log is the truth for each change.
- **Three of four lines are `status`.** About 80,000 of 106,000 lines
  each day. The cost is small. A person who reads the journal needs a
  filter that leaves them out. Put it in the how-to of E2.

### 4. What will be hard to change after 1.0.0 fixes the record format?

In the order of the cost of a late change:

1. **The form of `by`** (engine-01-11). It is in each record.
2. **The kinds of the commands** (engine-01-12). They are text in each
   record, and they carry meaning.
3. **What `apply` derives.** Each rule that `apply` works out from
   other records is part of the format: `used` (engine-01-13), and the
   loss of a claim with no record (engine-01-14). The rules of the
   store design speak of fields and kinds. They do not speak of the
   meaning of `apply`. Add one rule: `apply` only stores what a record
   says.
4. **The value `other`** (engine-01-2, engine-01-20). The rule is good.
   Without the checkpoint part, it loses state.
5. **The names of the fields that hold a person** (engine-01-18):
   `email`, `user`, `admin`.
6. **A session is a full URI in the change, and `user/session` in
   `by`.** Two forms of one thing in one record. It costs no fault, so
   leave it. Say on the page why the two are different: the change
   holds the place at the time of the record.
7. **`setting_changed` holds each setting.** A second setting is a new
   field with a default that means "not changed". That works with the
   rules of today. Say on the page that each setting is a field that
   can be absent.

Not hard to change, and good so: the life cycle states (not in a
record), the journal (not in the store), the roles table, the HTTP
paths (the line rule of the builds covers them).

## Questions for the lead

1. engine-01-1: does the writer task finish each command, or does
   `dispatch` start a task for each call? The first keeps the order
   with no more code. The second keeps the sketch.
2. engine-01-3: in a riff with no sign-in, are the people commands
   refused, as today? Then such a riff has no `person_joined` and no
   `owner_set` record, and "each role check passes" is only for
   `pause`, `resume` and `set_idle`.
3. engine-01-10: is a refusal of the token layer a journal line, or is
   the request log enough? If the request log is enough, how long does
   `just cloud setup` keep it?
4. engine-01-11 and engine-01-12 change the envelope. Do they go into
   E2, or into a new item before E2?
5. engine-01-13: a field on `released`, or a kind `session_used`? A
   kind is one more name in the fixed list.
6. Does the rule "`apply` only stores what a record says" become a
   requirement, with an ID from `just rid`?

## Evidence

Each check is a read of a checkout of 8006647. I ran no spike.

- `crates/riff-core/src/record.rs`: `Record`, `Change` (9 kinds),
  `Change::KINDS`, `Line::parse`, `Line::Unknown`, the test
  `each_kind_reads_back_and_has_its_name`.
- `crates/riff-server/src/state.rs`: `Command` (13 variants),
  `State::handle`, `State::run`, `State::commit`, `apply`, `named`,
  `Snapshot` and its types, `State::snapshot`, `same_log_state`,
  `State::set_status` (memory only), `State::claim` (reads the pending
  copy).
- `crates/riff-server/src/lib.rs`: `Server::state`, `change`,
  `settled`, `acts_as`, `post_message`, `Service::router` (three groups
  of routes, `admin_routes` behind `require_token`), `admin_only`,
  `owner_only`, `saved`, `Service::write_log` (the writer applies and
  delivers), `Server::deliver_after`, `Server::deliver_written`,
  `forget_sessions`, `Server::announce`, `watch`, `WatchGuard`.
- `crates/riff-server/src/checkpoint.rs`: the rules of the module doc
  (01M3TBZBQDF0ES4KM54FJQF6Z8).
- `crates/riff-server/src/log.rs`: `replay_after` counts a skipped
  record only for `Line::Unknown`.
- `crates/riff-server/src/logline.rs`: the fields `severity`, `time`,
  `message`, `target` of each log line.
- `crates/riff-server/src/auth.rs`: `SignedIn` holds `who` and `jkt`;
  `SignedIn::may_act_as`.
- `crates/riff-server/src/token.rs` and `owner.rs`: `SERVER_USER` is
  `riff`, and no person can sign in as that user.
- `crates/riff/src/api.rs`: `Api::call` with the path as text.
- `design/measures.md`: 475 posts, 113 claims and 113 releases for each
  person each day.
