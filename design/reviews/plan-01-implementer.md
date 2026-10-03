# Review plan-01: the implementer

Issue: #366. Design at commit ec8b46e.

## Summary

The design answers the five questions, and its shape fits the engine:
new kinds, checks in `handle`, a part of the riff and the checkpoint.
But a builder must guess in some places, and some rules disagree with
the code. The check order refuses a claim that the session holds
already. A worker of an admin can change the plan. `riff pr wait`
runs in a worker, so its `plan` is refused. The age of the plan
measures the last change, not the last look. The plan also overlaps
the item facts of #424: the server gets the needs two times. P1 and
P4 are too large for one worker each.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| plan-01-1 | Checks 3 to 6 come before check 7 (design-plan.md:187-200). Today a claim of the own item is granted before each other check (work.rs:285). With the new order, a session that holds `issue-12` gets `on_hold`, `not_in_wave`, `needs_open` or `done` at a claim again. This disagrees with line 231: "granted again, also when the plan changed". | must-fix | 3. The checks of a claim | Put "the session holds the item already: granted" after check 2, in the list and in the diagram. Add a test for each code. |
| plan-01-2 | Line 175-180 says that the owner and each admin can send `plan`, `hold` and `free`, and that a worker cannot. But the role comes from the user (people.rs:357), and a worker session of Mike has the role owner. So a worker of an admin passes. Also `permits` reads only the caller (command.rs:526). "A lead of the repository" reads the presence, so `permits` cannot check it. | must-fix | Who can send each command | Say: `permits` gives a person or a session, role member, no worker mark (as for `lead`, command.rs:541). `handle` checks "lead of this thread, or admin" and refuses with `not_allowed`. Add the rows to the table of `permits` and to its test. |
| plan-01-3 | `riff pr wait` runs in the author or in the verifier (SKILL.md:288, 361), and each is a worker. Its `plan` gets `not_allowed`. Also the `Merged in` comment comes after `riff pr wait` ends (SKILL.md:363-365), and the issue stays open until the checks after the release. So at that time the forge shows no change. | must-fix | 1. The source of the plan; P3 | Remove the `plan` of `riff pr wait`. A worker sends nothing. The next look (60 s) of the lead finds the comment. Or let `riff pr wait` send only the item fact, as in #424 (look.rs `tell_fact`). |
| plan-01-4 | The look sends `plan` only on a change (line 66-70). So the time of the `plan_set` record is the last change, not the last look. A plan that did not change for two days is current, but `riff plan` says "older than 5 minutes" (line 263) and a refusal says "3 minutes old" (line 107). The checkpoint keeps only the position of the record (line 241), not its time. | must-fix | How long the two can differ; 4. What the views show | The look sends `plan` each look. `handle` gives no record for no change, and the server keeps "seen at" as presence, not in the log. The age is now minus "seen at". Keep `written_at_ms` of the record in the riff and in the checkpoint too. Then the query `plan` each minute is not necessary. |
| plan-01-5 | The look of #424 already sends the open needs of each item each minute (`ItemFacts` with `all`, wire.rs of #424 at `ItemFact.needs`). The plan sends the needs again in `plan_set`. Two copies on the server break the goal "one source for each part" (line 19-20). The two reads of `gh` differ too: `forge_facts` reads `number,body`, the plan also needs `milestone` and `comments` (rollout.rs:1112-1122). | should-fix | 1. The source of the plan; P3 | One `gh issue list` with `number,body,comments,milestone` gives both. Say which copy `waits` and the claim check read. Proposal: the server makes `Waits::Needs` from the plan, and `ItemFact.needs` goes. |
| plan-01-6 | `items` holds only the open issues of the wave (line 31). A closed item of the wave is not in `items`, and is not in `done` when no item needs it. So a claim of it gets `not_in_wave` with the name of the current wave, which is false. Check 6 `done` comes only for an open issue with `Merged in`. A done item with an open need gets `needs_open`. | should-fix | The terms; The checks of a claim | Put the check `done` before `not_in_wave`, and say that `done` holds each done item of the wave and each done need. Or let `items` hold each issue of the wave. |
| plan-01-7 | A plan with no wave: line 111-114 and the skill (SKILL.md:129) say "each open item is in the current wave". The list makes check 5 depend only on "in `items`" (line 197). The diagram skips checks 4, 5 and 6 when there is no current wave (line 210-211). Does the look send each open issue in `items` with `wave` none? The rollout of today gives no free work with no wave (rollout.rs:1125). | should-fix | 1. How long the two can differ; the diagram | Decide: with no wave, `items` holds each open issue, and check 5 applies. Draw the diagram so: the arrow "no current wave" goes to check 5. Say what the rollout does with no wave and with no plan. |
| plan-01-8 | The format test binds the code to `fixtures/1.0.0`. `kinds.json` must equal the kinds of the code, and `log.jsonl` must hold each kind (format.rs:89-136). "Never write `log.jsonl` again; a later release adds a directory of its own" (format.rs:28-30). P1 says only "the fixtures of the new kinds" (line 294). | should-fix | 3. The records; P1 | Say in P1: a new directory for the next release, with its kinds, its log and its replay. The test of the kinds takes the union of the lists. The 1.0.0 log still replays to its bytes: the part `plans` has `skip_serializing_if` empty. Add the arms to `Record::of_repository` (record.rs:498) and the lines in the tables of record.rs and riff.rs. |
| plan-01-9 | A 1.0.0 client shows the reason of a code that it does not know: `call` makes a `Refusal`, and its text is "claim failed (409 Conflict): REASON" (api.rs:1700-1712, 2110-2114). So the claim of line 235 is right. But it is an error, not "not granted" (api.rs:1372-1379): the CLI exits through `?`, and the MCP tool gives a tool error (mcp.rs:492). The status of a new code is not in the design. `Failed::status` is a full match (engine.rs:332-336). | should-fix | The checks of a claim; P2 | Give the four codes the status 409. Say that a new client treats them as `held`: not granted, with the reason, exit 1. The test of P2 sends a code that the client does not know, and checks the text. |
| plan-01-10 | `POST /v1/log` gives `LogReply { records: Vec<Record> }` (wire.rs:1829). A `riff audit` of 1.0.0 reads it with serde. A `plan_set` record in the reply fails the whole read: an unknown variant. Line 136 ("a build of 1.0.0 skips them") is true for the store only. | should-fix | 3. The records; P5 | Say it in the design. Or make `LogReply` skip a kind that it does not know, as `Line::parse` does, in P1. |
| plan-01-11 | Rule 8 needs "a lead, the owner or an admin" at each record. The log of a repository has no record of the people (record.rs:515-524), and the admins of the settings are in no record (view.rs `Settings`). The audit has only `lead_set`, not the live lead of `View::lead_of`. | should-fix | 5. How `riff audit` uses the plan | Rule 8 checks only "the `by` is not a worker". Or the reply of `/v1/log` adds the roles. Say which. |
| plan-01-12 | P1 has three records, three commands, a query, a part of the riff and of the checkpoint, and a new fixture release. P4 has two CLI commands, two MCP tools, the board, the rollout, the skill and the book. Each is more than one worker can do in one context. | should-fix | Build items | Split P1: P1a the records, `apply`, checkpoint and fixtures; P1b the commands and the query (needs P1a). Split P4: P4a `riff plan` and the MCP tools with the book (P2, P3); P4b the board of `riff top` (P4a); P4c the rollout from the plan (P3). |
| plan-01-13 | The design names no wire types and no paths. The command `plan` and the query `plan` have the same name, and each call is `POST` (wire.rs `Call`). It names no group file in `state/`, and `Riff::restore` takes each part (riff.rs:83-100). | should-fix | 3. The records and the commands | Name them: `/v1/plan` (`PlanSet`), `/v1/plan/hold`, `/v1/plan/free`, query `/v1/plan/show`. A new group file `state/plan.rs` with `Plans` and its `Saved`. |
| plan-01-14 | Some refusals are not in the design: `free` of an item with no hold, `hold` with an empty reason, a limit on the reason length, `hold` of a done item, a `plan` from a person in a terminal. | note | Who can send each command | Add one line for each: what `handle` does. |
| plan-01-15 | The order list omits the check of the item name, `check("claim", item)`, between `must_clear` and `paused` (work.rs:263). | note | The checks of a claim | Add it as check 1a, `bad_request`. |
| plan-01-16 | The rule of done changes for a need. Today a need that is open with `Merged in` blocks the item (rollout.rs:676-677, the doc test of #6). The design makes it done (line 118-121). The audit already uses the new rule (audit.rs `merged_ms`). | note | 2. When an item is done | Say it in P3 or P4c, and change the doc test of `free_items`. |
| plan-01-17 | `riff release ITEM --session ID` works only for a session of the same user (`release_for`, design-engine.md "The commands"). A lead cannot free the claim of a worker of another person. | note | 3. The records (line 163-165) | Say so, or name the owner path. |
| plan-01-18 | The issue asks about `riff next`. The CLI has no `riff next` today. | note | 4. What the views show | Say that `riff plan` takes its place. |

## The questions of this point of view

1. The source of the plan. GitHub, sent by the look of the lead. The
   rule is clear, but `riff pr wait` cannot send (plan-01-3), the age
   is wrong (plan-01-4), and the needs come two times (plan-01-5).
2. When an item is done. Closed, or `Merged in`. Clear. The order of
   the checks hides `done` behind `not_in_wave` (plan-01-6).
3. The record kinds. `plan_set`, `item_held`, `item_freed` fit the
   rules of a change. The fixture work and `LogReply` are missing
   (plan-01-8, plan-01-10).
4. The views. `riff plan` is clear. The board of `riff top` does not
   say where an item that waits goes (the board has `free`,
   `claimed`, `verify`, top.rs:709-735), and whether it reads the plan
   from the server or from `gh`. `riff next` is not there
   (plan-01-18).
5. The audit. Rule 5 from the records is clear. Rule 8 cannot see the
   admins (plan-01-11).

The order of the build items is right: P2 can ship before P3,
because with no `plan_set` record no claim is checked. But P1 and
P4 are too large (plan-01-12).

## Questions for the lead

- Which version gets the new fixture directory: 1.1.0?
- Does the server make `waits` from the plan, so that `ItemFact.needs`
  goes (plan-01-5)?
- Does a worker of an admin ever change the plan? This review says no.
- Is a break of `riff audit` of 1.0.0 against the new server
  acceptable (plan-01-10)? "No users yet" says yes.
