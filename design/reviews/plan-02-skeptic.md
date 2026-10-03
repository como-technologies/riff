# Review plan-02: the skeptic and the operator

Issue: #366. Design at commit ec8b46e.

## Summary

The hold is a clear gain. It is small, it has one source (the
server), and it repairs the fault of Wave 16, where the lead claimed
#359 only to keep it from the workers. The copy of the forge plan on
the server is the weak part. It has a writer that only a lead can be,
no rule for two writers that cross, and no limit on its age. So a
stale plan blocks real work while no lead runs, and two looks can
write an old plan over a new one. Two claim checks refuse work that
people do each wave: the check `done` stops the checks after the
release, and decision 7 stops a person who works outside the wave.
The cost on GitHub is real: about 25 calls each minute for each lead.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| plan-02-1 | Check 6 (`done`) refuses each open item with the comment `Merged in`. That is the item with a check after the release left. The session that runs that check, and the session that fixes a failed check, cannot claim it. A closed issue is not in `items`, so check 6 hits only these items. | must-fix | The checks of a claim, 6 | Remove check 6. A claim of a closed issue gets `not_in_wave`; let its reason say "issue-N is closed". |
| plan-02-2 | Only a lead can send `plan`. But `riff pr wait` runs in the author or in the verifier, and neither is a lead. So the send after a merge fails with `not_allowed`. The refusal tells each session to run `riff plan sync`, which a worker or a session of another person also cannot run. | must-fix | 1. The source; Who can send each command; How long the two can differ | Let each member of the repository thread send `plan` and `riff plan sync`. The server cannot check the forge for a lead either. The record has `by`, and audit rule 8 compares each `plan_set` with the forge. |
| plan-02-3 | Lost update. The look reads the forge, then queries the server. When lead B sends a new plan between the two reads of lead A, A sees a difference and sends its older plan. Then B sends the new plan again. The same race occurs between a look and `riff pr wait`. Each step back makes an item "not done" again, so a claim gets `needs_open` for up to one minute. Each flip is one record. | must-fix | 1. The source ("Two leads read the same forge"); 3. The records | `plan` carries `base`: the position of the `plan_set` that the query gave. `handle` refuses `stale_base` when a newer `plan_set` exists. The look then reads the forge again on its next turn. Add a test with two looks that cross. |
| plan-02-4 | The plan has no limit on its age. With no lead live, the server keeps the old plan for hours. A wave closed on GitHub keeps its items "current". Each item of the new wave gets `not_in_wave`. A need merged after the last look gives `needs_open`. People in terminals and the sessions of other people still work in that time; only the rollout stops. | must-fix | How long the two can differ | Give the plan a time to live, for example 10 minutes from the last look that saw it. The look tells the server "same" with a signal (presence, no record). Past the limit, the server checks only the holds, as for "no plan", and the reply says "the plan is stale". |
| plan-02-5 | Decision 7 makes the rules the same for each session. The issue asks the design to "say which rules are for workers only". The skill says "A scope from your user still wins". Today a person, or a request of the lead (`request: claim issue-12`), can take an urgent item with no wave or an item of the next wave. With the design, the person must move the milestone and wait for a look (fails with plan-02-4 open). | should-fix | The checks of a claim; Decisions, 7 | The checks 4 and 5 refuse only a worker. The server knows the workers. For each other session the claim is granted with a warning in the reply. Audit rule 5 lists it. The hold applies to each session. |
| plan-02-6 | A hold stops the lead too. The case of #359 is "a held item is for the lead". The lead must free the item and then claim it. Between the two calls the rollout can see free work and start a worker for it. | should-fix | 3. The records; The checks of a claim, 3 | The session that held the item (or each session of its person) can claim it. That claim ends the hold in the same chunk. |
| plan-02-7 | Cost on GitHub. Each look reads the milestones, the open issues of the wave, the comments of each open issue (for `Merged in`), and the state of each need outside the wave: about 25 REST calls each minute, about 1,500 each hour for each lead. The limit is 5,000 each hour for each token. The workers, `riff pr`, the PR reads of #424 and CI use the same token. Two leads of one person double the look. | should-fix | 1. The source; Build items, P3 | One GraphQL query for each look (the issues of the milestone with state, body and the last comments), or ETag requests (a 304 does not count). Measure in the P3 test: the calls of one look for a wave of 20 items. |
| plan-02-8 | Rollback to 1.0.0. The old build skips `plan_set` and writes no checkpoint past the first one. Each start of an instance then replays the whole log from that checkpoint, and the replay grows each day. After the roll forward, the old holds and the old plan come back. A hold freed in that time cannot be freed (1.0.0 has no `free`). Claims granted by 1.0.0 look like faults to audit rule 5. | should-fix | 3. The records; 5. `riff audit` | Measure the start time of a replay of 7 days with no checkpoint. With plan-02-4 the old plan is stale at once after the roll forward. Audit rule 5 skips claims written while the last `plan_set` was stale. Say the rollback steps in `development.md`. |
| plan-02-9 | Mixed builds. A new lead against an old server gets an unknown command for `plan` and for the query, each minute. The design does not say what the look does then. | should-fix | 1. The source; Build items, P3 | The look treats an unknown command as "the server has no plan", writes one log line, and tries again after one hour. Add a test. |
| plan-02-10 | The plan starts in each repository where a lead of riff runs, for example `como-technologies/strata`. The people of that repository may use milestones in another way, or a stale open `Wave 2`. Then the server refuses their claims with no word from them. | should-fix | 1. The source; Decisions, 8 | The look sends `plan` only for a repository that turned the plan on (one setting, for example `riff plan on`, as a record). Else no `plan_set`, and the claims are as today. |
| plan-02-11 | The form check refuses the whole plan (`bad_request`) when one need is not `issue-N`. A `Needs:` line can name a pull request, or an issue of another repository (`owner/repo#12`). One such line then stops each update of the plan. | note | Who can send each command | The look drops a need that is not of this repository, with one log line. The server never refuses a whole plan for one need. |
| plan-02-12 | The plan checks only `issue-N`. A worker that claims `fix-366` passes each check of the plan. | note | The checks of a claim | Keep it (GEN;SET). Audit lists each claim in a planned repository that is not `issue-N` or `verify-issue-N`. |
| plan-02-13 | An issue reopened in a closed milestone is in no current wave. Nobody can claim the fix until the lead moves it. A milestone closed with open issues drops them out of the plan. | note | The terms ("current wave") | Say both in `waves.md`, with the `gh issue edit --milestone` step. |
| plan-02-14 | Not needed: the `item_freed` that `plan` makes when a held item is done. A hold on a merged item stops nothing that matters, and the lead frees it. The rule adds a second maker of `item_freed` and a rule in `handle`. | note | 3. The records | Remove it. Only `free` makes `item_freed`. |
| plan-02-15 | The words cross. The skill says a session "holds" a claim. The design adds a "held item" (a hold) and a code `held` (a claim of another session) next to `on_hold`. A worker that reads `held` cannot tell which it is. | note | The terms; The checks of a claim | Give the hold another word, for example "kept": `riff plan keep`, code `kept`. |
| plan-02-16 | GEN;SET order. The holds alone repair the fault that started the issue. They need no forge, no look and no sync. | note | Build items | Split P1 and P2: first the holds (`item_held`, `item_freed`, `hold`, `free`, check 3, the board). Then the plan. The holds then ship in one wave, with no wait for #424. |

## The questions of this point of view

1. The source of the plan. GitHub for the wave, the needs and done;
   the server for the holds. This is correct. But the copy needs a
   writer that each session can be (plan-02-2), a compare before the
   write (plan-02-3), and a limit on its age (plan-02-4).
2. When an item is done. Merged, as the skill says, is correct. The
   claim check of done is wrong for the items with a check after the
   release (plan-02-1).
3. The record kinds. Three new kinds fit the rules of the store. The
   cost of a rollback is a replay with no new checkpoint (plan-02-8).
   The log cost is small: a `plan_set` of 20 items is about 2 KB, and
   about 50 changes each day give about 100 KB. Without plan-02-3, a
   flip of two leads can give one record each minute.
4. The views. `riff top` and `riff plan` must show "stale" when the
   plan is past its limit, and the hold must show who can claim it
   (plan-02-6).
5. `riff audit`. Rule 5 from the log is a gain. It must skip a stale
   plan and the claims of a build that does not know the plan
   (plan-02-8).

Load on the server: one query and at most one command each minute for
each lead, and one more check in each claim. This is no real load.

Race of a look and a claim: a claim between a merge on GitHub and the
next look follows the old plan. With `riff pr wait` that each session
can send (plan-02-2), the gap is seconds. The refusal is safe: the
session picks another item.

## Questions for the lead

- Do the checks of the wave and of the needs apply to workers only
  (plan-02-5), or must a person move the milestone first?
- Can each member send `plan` (plan-02-2), or only a lead, with a
  hint "tell lead" in the refusal?
- What is the time to live of the plan (plan-02-4)? 10 minutes is a
  guess. Fail open (checks off) or fail closed past it?
- Is the plan opt-in for each repository (plan-02-10)?
- Can the lead of person B hold or free an item that the lead of
  person A held? The design lets each lead of the repository do it.
- Do the holds ship first, before the plan (plan-02-16)?
