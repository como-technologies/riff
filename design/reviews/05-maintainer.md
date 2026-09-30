# Review 05: the maintainer a year from now

Issue: #306. Design at commit bf325d9.

## Summary

The design makes the right choices for a maintainer: one log, one
`apply` for the live path and the replay, protobuf with number rules,
and `buf breaking` in CI. The rules for the wire are good. The rules
for the state are not complete. A rollback to the build before can
lose state through the checkpoint, and after 90 days the log cannot
give it back. The signed content of a message drops two fields that
the code signs today. The book anchors have no check, so a renamed
anchor gives an empty page with no error. Each of these has a small
fix. With them, I can change this code in a year with a test that
tells me when I break riff N-1.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 05-1 | A rollback to N-1 writes a checkpoint that drops each field and each record kind of N. A roll-forward loads it. The state of N before that checkpoint is gone, and after 90 days the log cannot replay it. | must-fix | The checkpoint; Deploy and rollback | Each checkpoint names the version that wrote it. A build writes no checkpoint while the newest checkpoint comes from a later version, or after it skipped a record. See the notes. |
| 05-2 | `MessageContent` has no lead mark and no user of the sender. R196 says the signature covers both, and `signed.rs` signs both today. Without them, a server can add `lead=true` to a message. | must-fix | Signed messages | Add `bool lead` and `string sender_user`, or one `string sender` in the form `user/session` of `Who`. Add a test that compares the fields of `MessageContent` with R196. |
| 05-3 | The book anchors have no check. mdbook 0.5.4 renders an empty code block for a missing anchor, with no message. For a missing file it logs an error and exits 0. So the book rots with no signal. | must-fix | The book shows the real code | `just book` fails on an `ERROR` line of mdbook. A test in the `hygiene` crate reads `docs/book/*.html` and fails on `{{#include` or on an empty code block. |
| 05-4 | `handle` returns `Vec<Record>`, but `position` and `written_at_ms` belong to the log. A test cannot compare them, and two callers can fill them in two ways. | should-fix | Event sourcing | `handle` returns `Vec<Change>`: the `oneof`. The log writer wraps each change in a `Record`. Say that `handle` sets `thread_seq` from the state. |
| 05-5 | The design holds the one lock through the write of the chunk. Then the writer queue never holds more than the records of one command, and each command waits its own 50 to 100 ms. The queue and the lock do not agree. | should-fix | Event sourcing; Chunks | Under the lock: `handle`, `apply`, put the records in the queue. Release the lock. Wait for the chunk, then reply. A failed write stops the server. Add a test: two posts in one chunk. |
| 05-6 | No CI check runs riff N-1 against riff-server N. The line rule is in `build.rs`, and #269 checked it by hand. A change of behavior that breaks the old client passes CI. | should-fix | The wire; Deploy and rollback | A CI job builds `riff` of the last release tag and runs a smoke test against the new server in a temporary directory: register, post, read, who, watch. |
| 05-7 | `buf breaking` checks the form, not the meaning. Nothing checks that `apply` of the current build gives the same state for the records of an older build, or that a new field has a safe default. | should-fix | The wire | Rule: the default value of a new field means the behavior of the build before it. Each release commits a golden log and the text of its state. A test replays each golden log and compares. |
| 05-8 | The design names no category for `buf breaking`. The default `FILE` fails each delete and each rename. Only `WIRE` and `WIRE_JSON` allow a delete of a reserved number, which the design permits. The Gate checks out with depth 1 and no tags, so `--against` the last tag finds nothing. | should-fix | The wire | Pick `WIRE_JSON`, and reserve the name of a removed field too. Add `buf.yaml`. Install `buf` in `just init` and in the Gate. Fetch the tags in the Gate. |
| 05-9 | One package for the store and the API. The store has a stronger rule: N-1 reads it after a rollback, and the checkpoint keeps it for years. The API has the line rule. One package cannot tell the two apart in CI or in a review. | should-fix | The wire | Two packages: `riff.store.v1` and `riff.api.v1`. A break of the API after 1.0 is a new package `riff.api.v2`, never a change in `v1`. Rewrite 01M3N73EAAD1G88TG7SNFWE4P1: a new field no longer starts a new line. |
| 05-10 | `Post` names no key. Today the JWS header carries the JWK, and the reader checks its thumbprint against the live keys of the user (`Message::verified`, R199). A reader of a chunk from month 1 has no key to check. | should-fix | Signed messages; Reads | Add `bytes signer_key` (the public key) or `string signer_thumbprint` to `Post`. Say what a reader does with a message whose key is not live. See the questions for the lead. |
| 05-11 | A chunk is a bare string of length-prefixed records, named `.pb`. It has no magic, no format version and no check per record. Decision 3 and the compaction give a second layout with the same name form. A wrong length makes the rest of the chunk unreadable, and `log verify` cannot say which record is bad. | should-fix | Chunks; Tools | A header of a magic and a format version at the start of each object. A CRC32C after each record. Name the objects `.log`, not `.pb`. |
| 05-12 | The record kinds read as commands: `ClaimItem`, `SetLead`, `ChangePerson`. `handle(claim) -> [ClaimItem]` does not say which is the command and which is the event. `Post` is a record kind, an rpc and today the request type `wire::Post`. | should-fix | The records; gRPC for the calls | Name each record in the past tense: `MessagePosted`, `ItemClaimed`, `LeadSet`, `PersonChanged`. Keep `Post` for the rpc. |
| 05-13 | Small names: the signed field is `wake`, the tool and the skill say `to`. `sender_session` does not say whether it holds `user/session` or the session ID. `MESSAGE_KIND_MESSAGE` says nothing. | note | Signed messages | Name the field `to`. Name the sender field for its form. Name the kind `MESSAGE_KIND_TO_READ`, or leave it and write one line of doc. |
| 05-14 | `apply` "does not fail" has no rule for a record that the state cannot take, for example a claim of a held item from a newer `handle` after a rollback. | should-fix | Event sourcing | Rule: `apply` trusts the log; the last record wins; it never panics; it counts each record that it could not take, and `riff server` shows the count. A test applies records in a bad order. |
| 05-15 | The two-release rollout of a signature scheme has no table that a test can read. "A test fails when a build signs with a scheme that the build before it cannot check" needs to know what the build before checks. | note | Signed messages | One table in `riff-core`: each scheme, the first version that checks it, the first version that signs with it. A test asserts that the second is a later line than the first. |
| 05-16 | "Each release adds some real signed messages to the test fixtures" is a habit, not a check. | note | The wire | `deploy/release-check.sh` fails when the fixture directory of the tag is missing. A throwaway key signs the fixtures, never a device key of a person. |
| 05-17 | `riff_core::wire` holds the JSON types of each call today. After item 6, it and the generated types are two truths. | note | Build items | Item 6 deletes `wire.rs`, or the design says which types stay and why. |
| 05-18 | The design names `tonic` for the calls and `prost` for the types. In tonic 0.14 the prost code generation is in the crate `tonic-prost-build`, not in `tonic-build`. | note | Build items | Name `tonic-prost-build` in item 1 and `tonic-prost` in item 6. |
| 05-19 | `ChangeSetting` as a generic record needs a table by hand for each key, and `buf breaking` cannot see a removed key. | note | The records | One field per setting in a `Settings` message, or an enum of keys with `reserved` for a removed key. |

### Notes on the findings

**05-1.** prost 0.14.4 drops each field that it does not know. I
decoded a message with a new field 7 as the old type and encoded it
again: 15 bytes in, 10 bytes out. I decoded a `Record` with a new
`oneof` field 19 as the old type: `change` is `None`, and the old type
writes it back with 2 bytes in place of 6. So after a rollback, N-1
reads the N checkpoint with no error, folds the log, and writes a
checkpoint with the fields of N-1 only. The design keeps 3
checkpoints. After 3 checkpoints of N-1, the last checkpoint of N is
gone. A roll-forward to N loads a checkpoint of N-1. Each record of N
before that position is in the log for 90 days and then gone. "Replay
from the start of the log" is not possible after 90 days. The
proposal: each checkpoint carries `written_by` (the crate version). A
build that finds a newer `written_by` in the newest checkpoint, or
that skipped a record (`change` is `None`), writes no checkpoint, and
`riff server` says so. The log grows for the time of the rollback,
which is short. The server deletes a chunk only below the newest
checkpoint; the bucket rule stays as a backstop at a longer age.

**05-3.** I built a book with three includes: a good anchor, a missing
anchor and a missing file. The good anchor rendered the message. The
missing anchor rendered nothing, and mdbook printed nothing about it.
The missing file rendered the text `{{#include ...}}` in the page, and
mdbook printed `ERROR Error updating ... Could not read file` and
exited 0. `just ci` would pass. The Gate installs mdbook with
`taiki-e/install-action`, so it has the same behavior.

**05-6.** `riff update` installs a release with `cargo install --tag`
from git (`auto_update.rs`). No binary is on the release. The CI job
builds the last tag from source; `Swatinem/rust-cache` keeps it warm.

**05-8.** `buf config ls-breaking-rules --version v2` on buf 1.63.0
lists 51 rules in `FILE`, among them `FIELD_NO_DELETE` and
`FIELD_SAME_NAME`. `WIRE_JSON` has 22 rules, among them
`FIELD_NO_DELETE_UNLESS_NUMBER_RESERVED` and `FIELD_SAME_NAME`. `WIRE`
has 15 and allows a rename. `WIRE_JSON` keeps the names, which the
`riff-server log` text and the book show. The Gate uses
`actions/checkout@v6` with the default depth of 1 and no tags.

## The questions of this point of view

### 1. Can I add a field, a record kind, a call or a signature scheme without breaking riff N-1? Which rule or CI check stops my mistakes?

- **A field.** Yes on the wire. N-1 skips it, and N reads a missing
  field as its default. I checked it with prost 0.14.4. The rule that
  is missing: the default value of a new field means the behavior of
  the build before it (05-7). `buf breaking` stops a number that I use
  again, a type that I change, and a delete with no `reserved`. No
  check stops a new meaning of an old field, or a checkpoint that a
  rollback cannot keep (05-1).
- **A record kind.** Yes for the log: N-1 reads the record as an empty
  change and skips it. The rollback loses it through the checkpoint
  (05-1). No check stops that today; the golden logs (05-7) and the
  `written_by` rule (05-1) do.
- **A call.** A new rpc is a new field of the service. An old client
  never calls it. The other way needs a rule: after a rollback of the
  server across a line, each machine has a newer `riff`, and the
  server refuses a later line (01M3MX1E1EY1M7JGNCN6FCEVQK). The design
  says "so a rollback works". It works for the records, not for the
  clients: each machine runs `riff update --tag` too. Write it in
  "Deploy and rollback". The N-1 smoke job (05-6) is the check.
- **A signature scheme.** Yes, in two releases, when the two releases
  are two lines. The test that the design names needs the table of
  05-15. The fixtures check that a new build reads old messages. The
  smoke job (05-6) checks that an old build reads new messages.

Today, no CI check stops a break of riff N-1. The check is by hand
(#269, thread message 357). The line rule in `build.rs` tells a client
that it is old; it does not test that the old client works.

### 2. Are the names in the `.proto` files clear? Is the mix of `Record`, `Post`, `MessageContent` and `MessageKind` easy to follow?

The names of the fields are clear: `written_at_ms`, `thread_seq`,
`woken_sessions`. The rule "each enum value has the name of its enum in
front, and value 0 is `UNSPECIFIED`" is the buf lint rule, and buf can
check it. Add `buf lint` next to `buf breaking`.

The mix is easy to follow after one reading of the sentence under the
`Post` message: a `Record` is a log entry, `Post` is the record kind
for a message, `MessageContent` is the signed part, and `MessageKind`
is message, status request or note. It is not easy at the second look
in a year. `Post` is three things (05-12). The record kinds read as
commands (05-12). `wake` is `to` everywhere else (05-13).
`ChangePerson` and `ChangeSetting` hide their cases (05-19). The past
tense for records and two packages (05-9) fix most of it.

### 3. Is the event sourcing (`handle`, `apply`, views) easy to read and to test? Is given/when/then enough?

It is easy to read. `handle` is pure, `apply` is pure, and the views
are functions of the state. The current `state.rs` already reads no
clock and does no I/O, so the move is small.

Given/when/then is enough for `handle`. It is not enough for the
whole:

- The `then` cannot compare a `Record` while `handle` fills
  `position` and `written_at_ms` (05-4).
- `apply` needs its own rule and test for a record that the state
  cannot take (05-14).
- The replay needs the golden logs of each release (05-7), not only a
  test log of the current build.
- The views need snapshot tests: `who`, the board and the state of
  each session from one given state.
- The order of the lock, the write and `apply` needs a test (05-5).

### 4. Does the book show the real code (anchors), so it does not rot?

mdbook 0.5.4 supports `{{#include file:anchor}}` with `ANCHOR:` and
`ANCHOR_END:` comments, and strips the anchor lines. The book uses no
include today; the one on the design page is escaped. With a check, the
rule works. Without a check, it rots in silence: a renamed anchor gives
an empty block, and a moved file gives an error that CI does not see
(05-3). The pages workflow copies the rustdoc into `docs/book/api`, so
the book can also link to the rustdoc of the proto module for the
details, as `CLAUDE.md` asks.

## Questions for the lead

1. A rollback of `riff-server` across a line: is it also a rollback
   of each machine with `riff update --tag`? Or does the API package
   accept a client of the line after the server, so that only the
   server rolls back?
2. A message from a person whose sign-in ended: today it shows
   `not verified` (R199). With a year of history in the chunks, do we
   keep that rule, or does the server attest the key of a sender at
   the time of the post, for example with a `KeyAdded` record?
3. One package or two (05-9)? Two packages change the layout of item
   1 and the line rule 01M3N73EAAD1G88TG7SNFWE4P1.
4. Does the default rule of 05-7 become a requirement in
   `requirements.md`, with an ID from `just rid`?

## Evidence

Each check runs from a checkout of bf325d9.

- prost: a crate with `prost = "0.14"` (it resolved 0.14.4) and two
  types, `Old` with fields 1 and 3, `New` with fields 1, 3 and 7.
  `Old::decode` of the bytes of `New` gives fields 1 and 3.
  `encode_to_vec` of the result gives 10 bytes for 15. A `Record` with
  a `oneof` field 19 decodes as the old type with `change: None`.
- mdbook: `mdbook --version` gives `v0.5.4`. A book with a good
  anchor, a missing anchor and a missing file: `mdbook build` exits 0
  and prints one `ERROR` line for the missing file.
- buf: `buf --version` gives `1.63.0`.
  `buf config ls-breaking-rules --version v2 --format json` lists the
  rules by category.
- tonic: docs.rs of `tonic-build` 0.14.6 says "For protobuf
  compilation via prost, use the `tonic-prost-build` crate instead."
- protox 0.9.1: "an implementation of the protobuf compiler in rust",
  with `protox::compile` and `prost_build::compile_fds`.
- The code: `crates/riff-core/src/build.rs` (`compatible`,
  `Semver::line`), `crates/riff-core/src/signed.rs` (`Content`: `from`,
  `lead`, `thread`, `to`, `body`, `kind`, `at_ms`),
  `crates/riff-core/src/wire.rs` (`Message::verified`, `Keys`),
  `crates/riff/src/auto_update.rs` (`cargo install --tag`),
  `.github/workflows/ci.yml` (`actions/checkout@v6`, no `fetch-depth`
  in the Gate), `justfile` (`ci`, `book`, `init`).
