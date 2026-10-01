# Review engine-02: the implementer

Issue: #357. Design at commit 8006647. Code of today at commit 7ec71a5.

## Summary

The design fits the code of today. `handle`, `apply`, the two copies and the writer are there, so the engine is a change of form, not a new store. The pipeline in the types builds with axum 0.8 and the one lock: I built a sketch, and the borrow checker does not fight it. But the page gives the last stages (`Written`, `Applied`, the journal line, the effects) to the task of the call, and that breaks two things that the writer does today: the order of the records in the written copy, and a change whose call the client dropped. The writer must make these stages. The life cycle has one gap: the worker mark does not reach the `session_started` record with the calls of today. The build order is right, but E1 is too large for one item, and E3, E4 and E5 share more than the list of kinds.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| engine-02-1 | The task of each call applies its own records to the written copy (`written.apply()` in `dispatch`). One chunk holds the records of many calls, and the tasks run in any order after the write. In the sketch, a claim (position 1) and a release (position 2) are in one chunk, the task of the release runs first, and the written copy ends with the claim held and the position 1. A replay of the same log gives no claim and the position 2. Today the writer applies the chunk in the order of the positions (`write_log` in `crates/riff-server/src/lib.rs`, `State::written` in `crates/riff-server/src/state.rs`). | must-fix | The pipeline in the types; One path | The writer makes `Written` and `Applied`. It writes the chunk, applies its records in order under the lock, and then gives `Applied` to each call of the chunk. The task of the call only waits, and makes the reply. See the sketch. |
| engine-02-2 | axum drops the future of a handler when the client closes the connection. In `dispatch` of the page, the apply, the journal line and the effects come after `queued.written().await`. In the sketch, a call that is dropped in this wait leaves its record in the log, but not in the written copy, with no journal line and no wake. So a claim is in the log and no `who` shows it until the next start. Today a dropped call loses only its reply: the writer applies (`write_log`) and sends the wakes (`deliver_written` in `lib.rs`). | must-fix | The pipeline in the types; The journal | As engine-02-1. The entry in the queue holds the records, the journal line and the effects of its command. The writer does all of them. The goal "each command leaves one entry" then holds for a dropped call too. |
| engine-02-3 | The worker mark does not reach the log. A new agent process first sends `start` from the hook (`start_facts` in `crates/riff/src/main.rs`), and `Start` has only `me` (`crates/riff-core/src/wire.rs`). Then `riff mcp` sends `register` with `worker` (`crates/riff/src/mcp.rs`). The `start` makes the `session_started` record with no mark. The `register` then finds a known session, so it makes no record. The `register` that the engine runs first for an unknown session has no body, so it has no mark too. Then a worker is never in MustClear, and the rule "a worker is never the lead" cannot skip it. Today the mark is only in memory (`Session::worker` in `state.rs`), and `lib.rs` sets it after `State::register`, so the first lead rule (`lead_if_first`) does not see it. | must-fix | The life cycle of a session | `Start` gets `reason` and `worker`. An explicit `register` whose mark is not the mark of the state makes a `session_started` record with the reason `join` and the new mark. The `register` that the engine runs keeps the mark of the state, and makes no lead: only an explicit `register` or `start` makes the first lead. |
| engine-02-4 | `Command` does not fit each route of today. (a) The token layer cannot make `Authenticated<C>`: the trait gives no `me`, and `invite`, `remove` and the owner commands have no `me` in the body (`wire.rs`). (b) `const KIND` and `const PATH` name one kind for one path. `/v1/riff` reads the state, pauses and resumes with one body (`Riff` in `wire.rs`, `riff` in `lib.rs`). `/v1/idle` reads and sets (`idle_workers` in `lib.rs`). (c) The commands of the server (`forget`, `announce`, `grant_owner`, `end_owner`) have no route and no JSON body, but the trait needs `DeserializeOwned` and `PATH`. | should-fix | The pipeline in the types; The commands | Two traits. `Command`: `fn kind(&self) -> &'static str`, `Reply`, `handle`, `reply`, `args`. `Routed: Command + DeserializeOwned`: `const PATH`, and `fn me(&self) -> Option<&SessionUri>`. The engine module has one more way to make `Authenticated<C>`: for the caller `riff`. A read of `/v1/riff` and of `/v1/idle` (no value to set) is a query. Or give `pause`, `resume` and `set_idle` paths of their own: then say that the wire changes. |
| engine-02-5 | `reply(&self, caller, view, made)` cannot make each reply of today. The reply of a post has the selectors that matched no session (`Posted::unmatched`), which `handle` finds from the presence (`PostChanges` in `state.rs`). They are not in a record. The reply of `lead` names the old lead (`State::lead` in `state.rs`), which the written copy does not hold after the change. | should-fix | The pipeline in the types | `Command` gets `type Note`. `handle` gives `Decision` and a `Note`. The engine keeps the note with the call, and `reply` gets it. |
| engine-02-6 | A note of the server is an effect and also the command `announce`. So the effect of one command sends a second command after the write, with a second chunk. A stop between the two gives the change with no note. Today it is the same: `take_owner` saves, then `Server::announce` posts one note for each repository (`lib.rs`). With the page as it is, each effect needs a path back into `dispatch`. | should-fix | One path; The terms | The `handle` of the command that causes a note gives the `posted` records of the note in its own `Decision`. They are in the same chunk, with the `by` and the `command` of the cause. `announce` stays for the timers that only post: the stop of an idle worker and the warning to the owner. |
| engine-02-7 | The rule "only `apply` gets the riff as `&mut`, a signal gets only the presence" leaves out that a record also changes memory, and that `handle` reads memory. A `session_forgotten` record removes the session and its read cursors (`State::apply_written` in `state.rs`). A `claimed` and a `released` record set the time of the last change of the claims (`State::commit`). A pause sets the time for a stale status (`State::riff`). `handle` reads the presence for the claim timer and for the first lead (`View::holds`, `View::lead_if_first`). | should-fix | The pipeline in the types | Say it on the page. `View` holds the riff and the presence, read only. The writer calls `apply(&mut riff, record)` and then `Presence::applied(&record)`. `Engine::signal` gets `&mut Presence` only. E1 moves the three places above into `Presence::applied`. |
| engine-02-8 | The rule that a load drops a sign-in "older than" the last `member_removed` or `signins_ended` record needs the start time of each sign-in. The saved form has none: `SavedSignIn` holds `id`, `user`, `jkt` and `idle_until` (`crates/riff-server/src/token.rs`). The two times also come from two clocks: a record gets its time from the clock of the state (`State::ms` in `state.rs`), and the sign-ins use the system time (`Tokens::to_bytes`). | should-fix | What stays outside | Each sign-in keeps the position of the log at its start. A load drops a sign-in whose position is less than the position of the last such record of its user. A position has no clock. E3 adds the field to `signins.json`. |
| engine-02-9 | Two sources of roles are not in the log, and the page has no command for them. The setting `--owner` names the owner at each start (`Tokens::name_owner` in `token.rs`, called in `Service::build` in `lib.rs`). The admins of the settings add to the admins that the owner made (`Tokens::is_admin(user, &config.admins)`). | should-fix | The commands; The records | Add the command `name_owner` of the server. It runs one time after the load, and makes an `owner_set` record when the riff has no owner and had none. `View` holds the admins of the settings, so `handle` checks a role in one place. Say that these admins are not in the log. |
| engine-02-10 | The page does not say what the checkpoint gains. The checkpoint is written field by field (`Snapshot`, `State::snapshot` and `Snapshot::into_parts` in `state.rs`), so a new part of the state that is not in it is lost at each start from a checkpoint. The prune deletes the first chunk (`checkpoint::prune`), so the `riff_made` record goes, and the riff ID must be in the checkpoint. | should-fix | The records | Add a part "The checkpoint": the riff ID; the people (the email of each USER, the members, the admins, the owner, a riff whose owner is gone, the request for the owner role with its time); the position of the last end of sign-ins of each user; the worker mark, the `used` mark and the end of each session; each pause with who set it and when. E6 adds a test: for the fixture with one record of each kind, a start from a checkpoint gives the state of a full replay. |
| engine-02-11 | E3, E4 and E5 share more than the list of kinds. Each one adds variants to `Change` (`crates/riff-core/src/record.rs`), arms to `apply` and to `named` (`state.rs`), fields to `Snapshot`, and fields to `who` (`SessionInfo` in `wire.rs`, the views of `riff`). Three sessions that edit the same `match` and the same structs get merge conflicts. E1 is also the largest item: it changes the 18 routes of the sessions and 3 timers in `lib.rs`, and the 48 public methods of `State` that 107 unit tests and about 26 doc tests use. | should-fix | Build items | Split E1. E1a: `State` becomes the riff and the presence, each command becomes a type with `handle`, each group gets a file with its part of the state, its `apply` arms and its part of the checkpoint. No change of behavior, the old handlers stay. E1b: the engine, the stages, the one handler, the writer that makes `Applied`. Then E3 and E4 touch different files. E5 after E3, as the page says. |
| engine-02-12 | The page says only that the import of go-live writes "these kinds". #341 lists the members, the admins, the owner, the leads, the riff state, the claims, the thread members, the settings and the messages. It does not list the email of each USER (`Tokens::users`, rule R209). Without a `person_joined` record for each, another email can take a USER after go-live. It also does not list the request for the owner role, a riff whose owner is gone, and the worker mark of each session. The old `sessions` object has `worker` and `ended` for each session (`SavedSession` in `state.rs` at v0.8.0). | should-fix | The records | List the records of the import on the page: `riff_made` with the riff ID of today, `person_joined` for each USER, `member_invited`, `admin_set`, `owner_set`, `owner_asked`, `pause_set` for the riff, `setting_changed`, and for each session that is not ended `session_started` (reason `join`, the worker mark), then its `joined_thread`, `lead_set` and `claimed` records, then the `posted` records. The import is the command `import` of the server through `dispatch`, so it gets a journal line. |
| engine-02-13 | A `compile_fail` doc test can pass for the wrong reason. A doc test sees only the public items, and the stages are private to the engine module, so each such test fails on privacy, whatever it tries to show. I built a handler that holds `Checked` over an await: the error is E0277 at the `.route(...)` line, "the trait `Handler` is not implemented". It does not name the lock. `#[axum::debug_handler]` does not take a generic handler. | note | The pipeline in the types | Each `compile_fail` test names its error code, for example `compile_fail,E0624` for a private method. The rule "no await while `Checked` lives" needs no doc test: `dispatch` is the only code that can break it, and then the one handler does not build. Say this in the rustdoc of `dispatch`, with the text of the error. |
| engine-02-14 | One call can be two commands: the engine first runs `register` for an unknown or ended session. The page does not say how many journal lines the call gives, and which `command` its records name. A query does the same today: `who`, `threads`, `read` and `watch` make the session and wait for its records (`called` and `watch` in `lib.rs`). The flow chart shows a query with no write. | note | One path; The journal | Say: the `register` is a command of its own. It has its own journal line, and its records name `register`. A query of an unknown session runs it through `dispatch` and waits for the write, then reads the view. |
| engine-02-15 | A tag from `main` before go-live is a go-live with no import. The server of today is v0.8.0 (e3cfe5a), which does not hold the log. `main` starts with the log of the bucket (`crates/riff-server/src/main.rs` calls `Service::load` of `lib.rs`) and does not read the old objects. | note | Build items | No release from `main` until #341 is merged. Say it in the build items, or the first build item makes `riff-server` refuse to start on a bucket that has the old objects and no log. |

Levels: must-fix (the design fails its goals without it), should-fix (a real gain), note (for the lead to know).

## The questions of this point of view

### 1. Does the typestate pipeline compile in real Rust with axum and the one lock?

Yes. I built a sketch in a crate outside the repository: rustc 1.95, edition 2024, axum 0.8.9, tokio 1.53.1, one `std::sync::Mutex`. It has the trait `Command`, the commands `claim` and `release`, `Authenticated<C>` as an extractor, the one generic handler, the route `.route(C::PATH, post(command::<C>))`, and a writer task. 8 tests pass.

What I found:

- The `dispatch` of the page builds word for word. `Checked<'_, C>` holds the `MutexGuard`. `checked.queue()` takes it by value before the first await, so the future of the handler is `Send`.
- A handler that holds `Checked` over an await does not build: E0277 at the route (engine-02-13).
- `Authenticated<C>` as the last extractor works. It reads the `SignedIn` that the token layer of today puts in the request (`require_token` in `lib.rs`), then the JSON body. It needs the `me` of the command (engine-02-4).
- The trait needs no more bounds than the page has: `C: DeserializeOwned + Send + 'static` and `Reply: Serialize`.
- The stages after the write break as the page has them (engine-02-1, engine-02-2). The sketch has the two forms, with the same tests. In the form of the page, the test of the order and the test of the dropped call show the faults. In the form below, they pass.

The form that works. The queue holds one entry for each accepted command. The writer makes `Applied`:

```rust
/// One accepted command in the queue.
struct Entry {
    made: Vec<Record>,
    journal: String,
    done: oneshot::Sender<Applied>,
}

/// The records of the command are in the log and in the written copy.
pub struct Applied {
    made: Vec<Record>,
}

impl Engine {
    pub async fn dispatch<C: Command>(&self, call: Authenticated<C>)
        -> Result<C::Reply, Refused>
    {
        let checked: Checked<'_, C> = self.check(call)?; // lock, handle
        let queued: Queued<C> = checked.queue();         // positions, the entry
        Ok(queued.applied().await?.reply())              // the writer did the rest
    }
}

// The writer, one task.
loop {
    let entries = std::mem::take(&mut writer.state().queue);
    if entries.is_empty() {
        writer.0.queued.notified().await;
        continue;
    }
    let chunk: Vec<Record> = entries.iter().flat_map(|e| e.made.clone()).collect();
    if !chunk.is_empty() {
        write(&chunk).await; // outside the lock
    }
    let mut state = writer.state();
    for record in &chunk {
        apply(&mut state.written, record);
    }
    drop(state);
    for entry in entries {
        journal(entry.journal);
        // The task of the call can be gone. The change is done.
        let _ = entry.done.send(Applied { made: entry.made });
    }
}
```

- An entry with no record (a `status`, a claim that changes nothing) goes through the same queue. So the one wait rule of the page needs no second path: the entry is done after each entry before it.
- The effects of an entry go out in the writer too, in the order of the positions. Today `deliver_written` does this. A `tail` reader then gets the messages of a thread in the order of their seq.
- The table of the stages becomes: `Authenticated`, `Checked`, `Queued` for the task of the call; `Written` and `Applied` for the writer.

### 2. How do the people and the owner move from `signins.json` into the log, and what does that change for #341?

Today the people are fields of `Tokens` (`token.rs`): `users` (the email of each USER), `owner`, `members`, `admins`, `no_owner`, `take` (the request for the owner role) and `riff_id`. `Saved` writes them to `signins.json`. Each change waits for the write of that object (`saved` in `lib.rs`). The handlers check the roles (`admin_only`, `owner_only` in `lib.rs`).

The move, in E3:

1. Each field becomes a part of the riff (the state of the log), changed only by `apply` of the new kinds. `Tokens` keeps the sign-ins, the chains and the access tokens.
2. Each handler of `lib.rs` from `revoke` to `pass_owner` becomes a command. The role checks move into `handle`. The timers of the owner (`owner_tick` in `lib.rs`, `crates/riff-server/src/owner.rs`) send `grant_owner` and `end_owner`.
3. The sign-in of a person (`exchange` in `lib.rs`, `Tokens::admit`) becomes two steps: the command `admit` through `dispatch`, then the chain. `admit` makes no record when the person is known, so a try again after a stop is safe. A first sign-in waits for two writes: the chunk, then `signins.json`.
4. `who`, `members` and `GET /v1/sign-in` read the people and the riff ID from the written copy, not from `Tokens`.
5. The checkpoint gets the people (engine-02-10). The sign-ins get the position of their start (engine-02-8). The setting `--owner` becomes a command (engine-02-9).

No production data moves from `signins.json`. The server of today is v0.8.0, which keeps the people in the old `tokens` object. The `signins.json` of `main` was never deployed (engine-02-15). So E3 changes the form of `signins.json` with no step for old files.

What it changes for #341:

- #341 needs E6, as the page says. Its `Needs:` line names #333, #334, #335 and #340 today.
- The import reads the people from the old `tokens` object and writes records, not fields of `signins.json`. The list of records is longer than the list of #341 (engine-02-12). The `person_joined` records are the important ones.
- The riff ID of today goes into the `riff_made` record. The store design still says that go-live makes a new riff ID. #341 says to keep it. #341 wins: update the store design.
- A refresh token of today works one time (#341). This path stays outside the engine. But the chain that it makes needs the USER of the person in the log, so the import runs before the first refresh.
- The import sets the pause of the riff with `by` `riff`. After go-live, only the owner and the admins resume the whole riff. Mike is the owner, so the resume works. A lead of another person cannot resume the riff after go-live. Tell the other leads before the deploy.

### 3. What is the size of each build item, what is the risk, and is the order right?

The sizes are my estimates from the code of today. S is less than 300 changed lines, M is 300 to 1,000, L is 1,000 to 2,500.

| Item | Size | Risk | Why |
|---|---|---|---|
| E1 | L, or more | high | The 18 routes of the sessions and 3 timers in `lib.rs` (3,255 lines). `state.rs` (4,296 lines) splits into the riff and the presence. 107 unit tests and about 26 doc tests of `state.rs` and `state/rules.rs` call the 48 public methods of `State`. The given/when/then tests of `rules.rs` already use `Command` and `handle`, so they move with small changes. Split it (engine-02-11). |
| E2 | M | low | `Record` gets `by` and `command` (about 25 places in the crates make a `Record`). The journal line is one function in the writer. The log bucket is a change of `just cloud setup`, with a check after the release. |
| E3 | L | high | About 560 lines of `token.rs`, `owner.rs` (372 lines), 230 lines of handlers, and the tests of the members, the owner and the sign-in (`crates/riff-server/tests/`). The rules are security rules: R209, the end of sign-ins at a removal, the first owner. Each rule of today needs a given/when/then test before the old code goes. |
| E4 | M | medium | The server part is small: the function of three facts, two kinds, four refusals. The risk is in the client: `Start` gets fields (engine-02-3), the reply to a release tells riff to clear, and a wake waits for a session in MustClear. #353 does the clear. |
| E5 | M | low | One kind, `Pauses`, the role table, the views, the rollout. It needs the roles of E3. |
| E6 | S | low | The value `other`, the fixtures, the replay in CI, the test of the checkpoint (engine-02-10). |

The order is right: E1, E2, then E3 and E4, then E5, then E6, then #341. Three changes:

- Split E1 into E1a and E1b (engine-02-11). E1a has no change of behavior, so its verify is the test suite of today.
- E3 and E4 run at the same time only when E1a gave each group its own file, with its `apply` arms and its part of the checkpoint.
- The chain is seven items deep: E1a, E1b, E2, E3 or E4, E5, E6, #341. The rule of the waves puts each item in a later wave than its needs. A wave ends with a release, and no release can go out before #341 (engine-02-15). See "Questions for the lead".

### 4. What does the session state machine do with the sessions of today that are in the middle of a claim at go-live?

The life cycle is a function of three facts: the claims, the worker mark and the `used` mark. So it needs no record of the past. The import gives it the facts.

- A session that is not a worker and holds a claim: the import writes its `claimed` records. The session is Working. Nothing changes for it. When it releases the last claim, it is Ready.
- A worker that holds a claim: the import writes `session_started` with the worker mark, then `claimed`. `apply` of `claimed` makes it used. The worker is Working. When it releases its last claim, it is in MustClear, and riff clears it (#353). This is the rule that we want. It needs the worker mark in the import (engine-02-12): the old `sessions` object has it.
- A worker that holds no claim at go-live: it is Ready and not used. It can claim one time with an old context. This is a small loss. A used mark for each imported worker is worse: each idle worker must clear before it can work.
- A session that the import does not name: its first call runs `register`, with the reason `join`. It is Ready.
- A session that ended before go-live: the import writes nothing for it, also no claim. A later call is a `join`.
- The claim timer: the server is offline for a short time. After the load, each session is gone until it calls, and its claims hold for 5 minutes from the load (`Session::holds` and `State::load` in `state.rs`). A session that comes back in this time keeps its claims. After that time, another session can take the item. The old holder then holds no claim. If it is a worker, it is in MustClear, with no `released` record: the `claimed` record of the new holder is the record. This is right, but the page does not say it (note 3 of the verify of #355).
- A `start` with no reason does not come: a riff of 0.8 cannot call a server of 1.0.0 (`compatible` and `Semver::line_before` in `crates/riff-core/src/build.rs`). So `reason` and `worker` can be required fields of `Start`. A compaction sends no `start` (`Source` in `crates/riff/src/hook.rs`), so it changes no state.
- One thing is outside the engine: a `riff mcp` process of 0.8 that runs in a session with a claim gets a refusal for each call until it runs the new build. #341 must show that this process comes back in the 5 minutes of the claim timer, or the import gives each imported claim a longer time.

## Questions for the lead

1. The chain of the build items is seven deep, and no release can go out from `main` before go-live. Does each item get a wave of its own with no release at its end, or do the items stay in one wave with `Needs:` lines?
2. `pause`, `resume` and `set_idle`: keep the paths of today and take the kind from the body, or give each command its own path and change the wire (engine-02-4)?
3. Is a worker with no claim at go-live Ready (it can claim one time with an old context), or MustClear?
4. Does the `register` that the engine runs for an unknown session make the first lead of a user, as today, or only an explicit `register` (engine-02-3)?
5. Does a note that a command causes go into the chunk of that command (engine-02-6)? Then the log shows the note with the `by` of the person, and the sender of the message is still the server.
