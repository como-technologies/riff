# Backlog batch 8: #1818 to #1857

Issue: #1893, part of #1885. Read on 2026-11-11 against `origin/main` at
commit f789f08e. Each path and line number is of that commit, unless
the text names the branch of a pull request. "Open" is the state of a
pull request on 2026-11-11. No issue of this batch was closed when I
read it.

| Issue | Verdict |
|---|---|
| #1818 | rewrite |
| #1827 | rewrite |
| #1828 | keep |
| #1837 | close |
| #1843 | rewrite |
| #1851 | rewrite |
| #1852 | question |
| #1853 | close, when #1883 merges |
| #1857 | question |

## #1818 The pause of the keys posts one line for each fault, and riff resume --riff checks the role first

Verdict: rewrite

Why:

- The fault is in memory only. `Github` has `fault: Mutex<Option<String>>`
  (`crates/riff-server/src/github.rs:846`), and each start sets it to
  `None` (`:858`). `apps_of` calls `keys_denied` only for the first
  fault of this instance (`:1238` to `:1240`). `keys_denied` (`:1250`)
  pauses the riff (`:1256`), posts the line to the owner and each admin
  (`:1281`), and wakes each repository (`:1290` and on), with no test of
  an earlier post. So each new instance in the same fault posts and
  wakes again.
- `POST /v1/resume` reads the keys before the role check
  (`crates/riff-server/src/lib.rs:2700` to `:2707`). The role check is
  in `command(...)` after it (`:2708`). The test of the fault
  (`crates/riff-server/tests/github.rs:1451`) makes the member `bob`, but
  `bob` runs no resume.
- The book: in "Make the GitHub Apps of riff"
  (`docs/src/start-a-team-riff.md:216`), `riff github create` is at
  `:233`, and "Make the secret before the deploy" comes after it at
  `:268`. The deploy is earlier, in "Deploy the riff" (`:146`). For a
  new riff, `riff cloud create` in "Make the riff" (`:107`) makes the
  secret already (`:121` to `:124`). So the step is for a riff that was
  made before the Apps.
- The old body misses one fact. 01M6YC3SYATQW7GZZV3APFG85B
  (`docs/src/requirements.md:1035`, section "Tokens on GitHub") binds
  the order of today: "`riff resume --riff` reads the keys first"
  (`:1045` to `:1046`). It also says "posts one line" with no word of a
  restart. So the new requirement must supersede it in part and cut
  that text. Else the rule `in-part` of Hygiene fails the pull request.
- No merged pull request changed `apps_of`, `keys_denied` or `resume`
  since #1817. The open #1873 changes `docs/src/start-a-team-riff.md`
  at `:284` to `:289`, near the step to move.

New title: The pause of the keys posts one line for each fault, and
riff resume --riff checks the role first

New body:

```markdown
The pause of the keys posts one line for each fault, and
`riff resume --riff` checks the role first.

From the verify of #1759 (PR #1817).

## What is wrong

- riff-server keeps the fault of the keys in memory only
  (`crates/riff-server/src/github.rs`, `fault`). Each new instance
  that starts while the secret is still unreadable posts one more
  `riff-bot` line and wakes each repository again.
- `POST /v1/resume` reads the keys before the role check
  (`crates/riff-server/src/lib.rs`, `resume`). During the fault, a
  member who is not an admin gets the text of the fault, not "not
  allowed".
- In `docs/src/start-a-team-riff.md`, "Make the secret before the
  deploy" comes after `riff github create`, and after "Deploy the
  riff". A person must do it before the deploy.

## What

- riff-server keeps the fault with the pause in its state, and posts
  the line once for each fault, also over a restart and in a second
  instance.
- `POST /v1/resume` checks the role first, then the keys.
- "Make the secret before the deploy" moves to "Host your own riff on
  Cloud Run", before "Deploy the riff". It says that it is for a riff
  that was made before the Apps: `riff cloud create` of a new riff
  makes the secret.

Done when:
- A test of riff-server: a second instance that starts during the
  same fault posts no line and wakes no repository.
- A test of riff-server: a member who is not an admin gets "not
  allowed" from `riff resume --riff` during the fault.
- A requirement (`just req`) in "Tokens on GitHub" for one line for
  each fault and for the role check first. It supersedes
  01M6YC3SYATQW7GZZV3APFG85B in part, and cuts its words "reads the
  keys first".
- Docs: "Make the secret before the deploy" in
  `docs/src/start-a-team-riff.md` comes before "Deploy the riff".
```

## #1827 The look sends no waits-for-a-verify note while a check of the head failed or a session claims the item

Verdict: rewrite

Why:

- The three claims hold on main. The look counts a pull request as
  waiting for a verify with this test only
  (`crates/riff/src/look.rs:542` to `:546`): the verify is asked, no
  `verify-` claim, no hold of the lead, no `CONFLICTING`, no wait for
  another merge. It reads no check of the head and no claim of
  `issue-N`. The claim of `issue-N` is used only for the message of a
  conflict (`crates/riff/src/look.rs:532`).
- The wave starts no verify in these cases. It waits while a session
  holds `issue-N` and until the full check of the head is done
  (`crates/riff/src/wave.rs:1227` to `:1230`, rule
  01M4ZV3B6NKPEZ5FNE27X95GF0 at `docs/src/requirements.md:5773`). It
  starts no verify of a head with a failed check
  (`crates/riff/src/wave.rs:3177`, rule 01M6JEQEQ45YMNR1WTX2RRTQ5B). So
  the note asks the lead for a verify that the wave holds back by
  itself. The issue misses a third case of the same kind: a full check
  that still runs. The comment of the issue says that the note came
  "while its job macOS failed or ran".
- The timer has the key (pull request, head)
  (`crates/riff/src/look.rs:551`). So each push starts a new 30 minutes,
  and the note comes again for each head, as the comment of the issue
  says.
- 01M6WCGNN0X2DGYYM97656RQK1 (`docs/src/requirements.md:346`, section
  "Waves") and 01M49Q316RXNATJP587DWGDNCD (`:2912`) are as the issue
  says. The old body asks for a requirement that supersedes
  01M6WCGNN0X2DGYYM97656RQK1 in part. But each word of that requirement
  stays true, so nothing would be cut, and the rule `in-part` of Hygiene
  fails such a pull request (`crates/hygiene/src/lib.rs:73`). A new
  requirement that adds the cases supersedes nothing.
- The open #1869 (for #1859) makes a failed check count only when the
  ruleset of main requires it. Then `Pull::failed_checks`
  (`crates/riff/src/rollout.rs:1571`) is the test of "a required check
  failed" that this issue needs.
- The book: "When a pull request stops"
  (`docs/src/how-it-works.md:2808`) lists the times when no message
  comes at `:2835` to `:2841`. It names no check and no claim of
  `issue-N`.
- No merged pull request changed the test. #1826 changed `look.rs`, but
  only the message of a conflict.

New title: The look sends no waits-for-a-verify note while the wave
holds the verify back: a failed or running check, or a claim of the item

New body:

```markdown
The look sends no "waits for a verify" note while the wave itself
holds the verify back.

The lead got (2026-11-09): "Pull request #1824 of issue-1646 waits for
a verify of commit 3348c90 for 30 minutes, and no session claims
verify-issue-1646. Give the verify to a free session." The job macOS
and the Gate of 3348c90 had failed, and the worker of issue-1646 still
held the item. The note came once for each new head.

## What

- `PullWatch::news` (`crates/riff/src/look.rs`) counts a pull request
  as waiting for a verify only when the wave can start the verify. So
  the 30 minutes of 01M49Q316RXNATJP587DWGDNCD do not run while:
  - a session claims `issue-N` (01M4ZV3B6NKPEZ5FNE27X95GF0),
  - a required check of the head failed (01M6JEQEQ45YMNR1WTX2RRTQ5B),
  - the full check of the head still runs.
- The 30 minutes start again when the three cases are over.

Needs: #1859 (a failed check counts only when the ruleset of main
requires it).

Done when:
- A test of the look: a pull request with a verify asked, a failed
  required check and a claim of `issue-N` gives no note after 30
  minutes. After the claim ends and the check passes, the note comes
  30 minutes later.
- A test of the look: a pull request whose full check still runs gives
  no note.
- A requirement (`just req`) in "Waves" for the three cases. It
  supersedes no requirement.
- Docs: "When a pull request stops" in `docs/src/how-it-works.md`
  names the three cases in its list of the times when no message
  comes.
```

## #1828 riff top shows a rebase that waits for its turn as a wait, not as stuck

Verdict: keep

Why:

- The fault still holds on main. `riff top` builds its board with
  `with_pulls` (`crates/riff/src/top.rs:291`), which sets `verify` and
  `stuck` and no `waits`. Then `Top::view` calls `Board::of` on these
  issues (`crates/riff/src/top.rs:965`).
- `Board::of` puts a conflict in `rebase waits for #N` only when the
  issues have a wait for it (`crates/riff-client/src/waves.rs:520` to
  `:525`). With no `waits`, the item goes to `stuck`.
- `riff wave list` adds the waits from the facts of the daemons with
  `with_waits(riff_client::daemons::waits(&daemons, repo))`
  (`crates/riff/src/waves.rs:652`), and so does the Waves tab
  (`crates/riff/src/tui/waves.rs:948`). `riff top` has the same
  sessions with the facts (`who.sessions`, `crates/riff/src/main.rs:4299`
  to `:4302`), so the fix needs no call to a daemon:
  `riff_client::daemons::hosts` reads the facts from the sessions
  (`crates/riff-client/src/daemons.rs:627`).
- 01M6YPYTPG8RJY7PRERR2ZM10H (`docs/src/requirements.md:381`) names only
  `riff wave list`.
- No merged pull request changed it. The open #1882 (#1855) reads the
  waits in `riff daemon status` and does not touch `top.rs`.
- The book: the table under "See a pull request that cannot merge"
  (`docs/src/development.md:2762`, table at `:2793`) gives only
  `stuck: #12 (conflict)` for a conflict.

## #1837 The row N new lines below of the Chat tab counts only the lines that are not in sight

Verdict: close

Why:

- The fault does not exist as the issue says. The window of the log
  ends at `scroll` rows over the newest row, with or without the row of
  the new lines: `window` (`crates/riff/src/tui/chat.rs:1626`) takes the
  rows from `got.len() - scroll` up (`:1638` to `:1640`). With the row of
  the new lines, the window shows one row less at its top, and the row
  goes right under the last row of the window (`:1504` to `:1511`,
  `:1520`). So no log row hides under the row.
- `below` counts a new line when its last row is under the window,
  `under < scroll` (`crates/riff/src/tui/chat.rs:1665`). The rows of the
  window act only in the clamp at `:1661`. `scroll_by` clamps the scroll
  to the same window first (`:883`), so that clamp changes nothing
  there. The count at `:893` is the same with R or R-1 rows.
- A test covers the case of the issue:
  `the_row_of_the_new_lines_counts_the_lines_that_no_scroll_showed`
  (`crates/riff/src/tui/chat.rs:2445`). It scrolls down until the line
  `new 0` shows over the row, and gets "2 new lines below" (`:2459` to
  `:2463`). Then a scroll up keeps 2 (`:2465` to `:2467`).
- No commit changed `crates/riff/src/tui/chat.rs` after #1835
  (be1bea08), so the verifier of #1835 read this code.
- A narrower edge stays, and it is another fault. The draw clamps the
  scroll, but writes back only the window (`crates/riff/src/tui.rs:1506`).
  After a change of the layout (a taller tab, a shorter prompt, the list
  of names closes), `scroll` and `new` of `Chat` can stay one row too
  high. The lead can make a new issue for it when a person sees it.

## #1843 The fallback of the rights of the lead App says the right cause, once, and points each role to its own fix

Verdict: rewrite

Why:

- `github_checked` adds the fix "See \"Accept the new rights of the lead
  App\"" to each role that misses a right, with no test of the role
  (`crates/riff/src/text.rs:5665` to `:5673`). A worker or a verifier
  seldom gets there: GitHub answers 422 to a token with a right that the
  install lacks, and a role with no fallback gives the `problem` line
  with no fix (`crates/riff/src/text.rs:5660`). A test must say which
  of the two paths it covers.
- `refused()` is true for each 403 and 422
  (`crates/riff-server/src/github.rs:560` to `:567`). So a 403 of a rate
  limit also gets the fallback (`:1018`), and its warning says that
  GitHub refused the rights on the alerts (`:1027` to `:1036`).
- `grant` keeps no record of a warning that it gave, so each token logs
  it again. A test asserts this today: "each token logs one warning"
  (`crates/riff-server/tests/github.rs:1322`), and two warnings after
  the token of the lead and of the daemon (`:1380`, `:1386`). The book
  says "logs one warning" (`docs/src/start-a-team-riff.md:339`,
  `docs/src/how-it-works.md:6288`).
- 01M6WPMEEWX09DHJZF18BKBPYW (`docs/src/requirements.md:3516`) and
  01M6XZJFGNVG6B7DF5VQ3H1DNX (`:3531`) still name
  01M6WG52MWZB1SVQMMH6BKJSJ5 for the rights of the lead, and say that
  riff-server refuses a token with other rights. The rights of the lead
  are in 01M6ZEJ4NEZGQFV3Q4BER5AW3S (`:3556`), and the fallback in
  01M6ZHWVPD7MN6GH6GS0GVETHA (`:3563`). All are in the section
  "Security" (`:3369`).
- The old Done when asks that "the two requirements name" the new IDs:
  an edit by hand of old texts. The rules of the repository want a new
  requirement that supersedes them in part and cuts the old words.
- No merged pull request changed these files after #1842. The open #1873
  changes `crates/riff-server/src/github.rs`, but not `refused` or the
  fallback.

New title: The fallback of the rights of the lead App says the right
cause, once, and points each role to its own fix

New body:

```markdown
The fallback of the rights of the lead App says the right cause, once
for each install, and `riff github check` points each role to its own
fix.

From the verify of #1839 (PR #1842).

## What is wrong

- `github_checked` (`crates/riff/src/text.rs`) points each role that
  misses a right to "Accept the new rights of the lead App", also a
  worker or a verifier of the session App.
- `refused()` (`crates/riff-server/src/github.rs`) takes each 403 as a
  refusal of the rights, also a rate limit. Then the fallback makes one
  more call, and its warning names the wrong cause.
- With a lead App that lacks the new rights, each new token of the
  lead and of the daemon logs the warning again. The book says "logs
  one warning".
- 01M6WPMEEWX09DHJZF18BKBPYW and 01M6XZJFGNVG6B7DF5VQ3H1DNX name
  01M6WG52MWZB1SVQMMH6BKJSJ5 for the rights of the lead, and say that
  riff-server refuses a token with other rights, with no word of the
  fallback (01M6ZHWVPD7MN6GH6GS0GVETHA).

Done when:
- A test of `riff github check`: a worker that misses a right gets the
  fix of the session App, and the lead gets the fix of the lead App.
  The test names its path: the list of missing rights, or the 422 of
  GitHub.
- A test of riff-server: a 403 of a rate limit gets no fallback and no
  warning of the rights. A refusal of the rights warns once for each
  install, not for each token. The test at
  `crates/riff-server/tests/github.rs` that counts two warnings changes.
- A requirement (`just req`) in "Security" that supersedes
  01M6WPMEEWX09DHJZF18BKBPYW, 01M6XZJFGNVG6B7DF5VQ3H1DNX and
  01M6ZHWVPD7MN6GH6GS0GVETHA in part: the rights of the lead are those
  of 01M6ZEJ4NEZGQFV3Q4BER5AW3S, with the fallback, and one warning
  for each install.
- Docs: none for a person; the rustdoc of `refused` says which answers
  count as a refusal of the rights.
```

## #1851 riff release cut checks first that the ruleset releases lets the pusher of the tag push it

Verdict: rewrite

Why:

- The cut checks no ruleset yet. `cycle::cut`
  (`crates/riff/src/cycle.rs:1091`) checks the last release, the
  version, a skipped version, a deploy that waits, and the alerts
  (`:1140` to `:1148`). Then it makes the release issue and the pull
  request. The `Forge` trait has no read of a ruleset.
- `riff init` checks the ruleset `releases` (`releases_check`,
  `crates/riff/src/repo_setup.rs:1538`; 01M6WZ40YED6THKVJ2JQRXFK4N at
  `docs/src/requirements.md:6690`). It reads the bypass actors, and
  passes when GitHub hides them (`crates/riff/src/repo_setup.rs:1558`).
- The old plan does not work for a lead. GitHub shows the bypass actors
  only to an admin, and no App of riff has the right `administration`
  (01M6WG52MWZB1SVQMMH6BKJSJ5 at `docs/src/requirements.md:3445`, and
  01M6ZEJ4NEZGQFV3Q4BER5AW3S at `:3556`). So the cut of a lead would
  always say "cannot check" and go on, also in the case of the issue. A
  read of the ruleset `releases` of this repository with the token of
  the session App, on 2026-11-11, gave `"bypass_actors": null` and
  `"current_user_can_bypass": "never"`.
- GitHub gives `current_user_can_bypass` of the caller on
  `GET repos/OWNER/REPO/rulesets/ID`. So the lead can check itself with
  no list of the bypass actors. The one who cuts is not always the one
  who publishes: the cut is for the lead or a person, the publish for
  an admin or the lead (`crates/riff/src/cycle.rs:18` and `:20`). An
  admin sees the bypass actors, so the cut of a person can run the
  check of `riff init`.
- The headings exist: "The release commands"
  (`docs/src/requirements.md:7235`) and "Cut the release pull request"
  (`docs/src/development.md:7086`).
- No merged or open pull request adds this check. The open #1868
  changes the alert gate only.

New title: riff release cut checks first that the ruleset releases lets
the lead App push the tag

New body:

```markdown
`riff release cut` checks first that the ruleset `releases` lets the
lead App push the release tag.

`riff release publish` by the lead failed at its last step on
2026-11-09: GitHub refused the push of the tag ("GH013 ... Cannot
create ref due to creations being restricted"). The lead App was not a
bypass actor of the ruleset `releases`. `riff init` checks that
ruleset, but nobody ran it after the install of the lead App.

## What

Before it makes the release issue, `riff release cut` checks that the
ruleset `releases` lets the lead App push the tag:

- The cut of the lead reads the ruleset with the token of the lead:
  `GET repos/OWNER/REPO/rulesets/ID`. `current_user_can_bypass` is
  `always`: the cut goes on. The lead reads no bypass actors: GitHub
  shows them only to an admin, and the lead App has no right
  `administration`.
- The cut of a person runs the check of the ruleset of `riff init`:
  the lead App is a bypass actor. When GitHub hides the bypass actors
  (the person is not an admin), the cut says that it cannot check, and
  goes on.
- On a gap, or with no ruleset `releases`, the cut refuses with the
  fix: an admin runs `riff init`.

Done when:
- A test of `riff release cut` with the fake forge of the release
  cycle: the lead with `never` gets the refusal with the fix; the lead
  with `always` goes on; a person with a ruleset with no lead App gets
  the refusal; a person with hidden bypass actors goes on with the
  note; no ruleset gets the refusal.
- A requirement (`just req`) in "The release commands".
- Docs: "Cut the release pull request" in `docs/src/development.md`
  names the check and the fix.
```

## #1852 riff lead start: a new lead stops at the Bypass Permissions warning, the first start does not show it, and --yes answers it

Verdict: question

Why:

- riff starts the lead with `--dangerously-skip-permissions`
  (`crates/riff/src/lead.rs:250`, 01M5GG2X5F1HXCQJ2JJWV6305K at
  `docs/src/requirements.md:6325`).
- riff writes no accept of the warning. The Claude folder of a lead gets
  `hasCompletedOnboarding`, `theme` and `hasTrustDialogAccepted`
  (`crates/riff/src/run.rs:914` to `:929`, 01M501P9SDGVWG3MTVCW5BDRQQ at
  `docs/src/requirements.md:6253`). No file of the repository names
  `skipDangerousModePermissionPrompt` or `bypassPermissionsModeAccepted`.
- The claim "the first start does not draw again" is wrong. Each attach
  of `riff lead start` sets the size of the terminal first, also the
  first one (`crates/riff/src/lead.rs:598`, and the first start ends
  in `attach_here` at `:763`). This is so since #880, and
  01M51PD7M04HZR2PVVJA4WCBK5 (`docs/src/requirements.md:6227`) says
  "Before each attach". So the screen stays empty although riff draws
  again. A likely cause: `claude` does not draw its warning
  again after a change of the size. Nobody checked it.
- With no terminal, the lead starts and riff leaves. The draw does
  nothing then (`crates/riff/src/podman.rs:637` to `:641`), and no check
  of a terminal stops the start. `riff lead start` has no flag: `Start`
  has no field (`crates/riff/src/main.rs:1664`).
- A lead with no turn shows "monitoring work"
  (`crates/riff/src/state.rs:364`). Nothing sees a lead that waits at a
  dialog.
- The issue misses `riff lead move HOST`. It starts a new lead with a
  new Claude folder on the other host, with no person at its terminal
  (`crates/riff/src/lead.rs:991` to `:1003`). A `--yes` of
  `riff lead start` does not reach it.
- The issue asks the lead to choose the default of `--yes`. But the
  accept is the consent of a person to a warning of Claude Code, and
  01M5GG2X5F1HXCQJ2JJWV6305K chose only the mode. So it is a question
  for mike.
- The book: "Start the lead" (`docs/src/development.md:4037`) says "When
  `claude` asks a question at its first start, answer it once"
  (`:4105`). No page names the warning. No merged or open pull request
  does a part of the issue.

Questions for mike:

- Q1: Does riff write the accept of the Bypass Permissions warning for
  each lead, with no flag, or only with `riff lead start --yes`?
  Proposal: for each lead. #1202 chose the bypass mode for the lead,
  and the container is the sandbox. Only a default covers
  `riff lead move` and a start again of the lead after an update. riff
  writes `"skipDangerousModePermissionPrompt": true` in the
  `settings.json` of the Claude folder of the lead, also in a
  `settings.json` that is there and lacks it. It is a new requirement
  next to 01M501P9SDGVWG3MTVCW5BDRQQ.
- Q2: With the accept as the default, does the issue still need the
  refusal with no terminal, the line "press ↓ and Enter", and a new
  draw? Proposal: no. Drop the three. The first start draws again
  already.
- Q3: Does `riff who` show a lead that waits at a dialog of `claude`?
  Proposal: not in this issue. With the accept, riff knows no other
  dialog of a lead. Make a new issue when one shows.

With these proposals, the new Done when: a test of the lead spec and of
`riff lead move`: the Claude folder has the accept before `claude`
starts; a live check on a host: `riff lead start` in a new container
gives a lead that works with no key press. Docs: "Start the lead" in
`docs/src/development.md` says that riff accepts the warning for the
lead.

## #1853 A chat line that riff posts for a person wakes only that person, not a name in a quoted title

Verdict: close

Why:

- The fault still holds on main today. The look makes the ask with the
  title of the issue, `text::wave_question(&wave.repo, number, title)`
  (`crates/riff/src/look.rs:1364`, text at `crates/riff/src/text.rs:10592`).
  `ask_person` then takes `to` from the whole body,
  `riff_client::chat::to(&body)` (`crates/riff/src/wave.rs:3963`).
- The other posts that the issue names take `to` from their body in the
  same way: the owner line (`crates/riff/src/watches.rs:782`),
  `ServerCall::ChatPost` (`crates/riff/src/daemon.rs:1381`), and
  `Reach::chat_post` (`crates/riff/src/reach.rs:357`).
- The open #1883 (for #1812, in Wave 54) removes each of these four
  posts and `post_by_riff`. riff-server writes these lines itself
  (`crates/riff-server/src/bot.rs` on that branch). It sets `to` from
  the fact in the code, `Selector::name` of the owner or of the person,
  and puts no title in the ask of the label `question`. Its new
  requirement 01M74BPHQZHXFYYNXAJ1WFASQQ says: "No text of a client goes
  into the line: the line of the label `question` has the link of the
  issue and no title".
- On that branch, the only `riff_client::chat::to` call that stays is
  the line that a person types (`crates/riff/src/chat.rs:691` there,
  `:729` on main). There, the names of the body are the names that the
  sender means, as the issue asks.
- So the lead closes #1853 as done by #1883 when #1883 merges. While
  #1883 is open, #1853 stays open.

## #1857 A riff tell to a session that is offline waits for its next start, and is not lost

Verdict: question

Why:

- The help of `riff tell` says "It wakes that session", and nothing of a
  session that is gone (`crates/riff/src/main.rs:137`). The tool `tell`
  says the same (`crates/riff/src/tools.rs:554`).
- riff-server refuses a direct message to a gone session: "the session
  ... is gone: it ended, or it stopped"
  (`crates/riff-state/src/state/threads.rs:316` to `:322`). The refusal
  writes no record, so nothing waits. A test holds it:
  `a_direct_message_to_a_gone_session_is_refused`
  (`crates/riff-state/src/state/rules.rs:720`).
- This is a rule of today: R206 (`docs/src/requirements.md:2719`) says
  "A direct message to it fails and says that the session is gone", and
  R60 (`:3103`) says "A session that matches later gets no wake". The
  book says "A gone session gets no messages. A `tell` to it fails."
  (`docs/src/how-it-works.md:2276`, under "When a session ends").
- The words of the issue do not fit the code. "Offline" in `who` means
  "no open watch" (`crates/riff-core/src/wire.rs:749`), and a `tell` to
  an offline session that is not gone works. The refusal comes from
  "gone": it ended, or 3 minutes with no call (R206). A gone session
  that calls again is live again with the same ID (R207, `:2727`). The
  code has no "ended for good". The nearest is a session that riff-server
  forgets after 30 days (01M3TBZBZVH907QD359AB8TBSX).
- The host session of the issue is the `claude` of a person on the
  host. riff has no hook and no tool there (01M59BXNPR3AZ0HAS9GQCZ4PH2,
  `docs/src/requirements.md:5181`), so nothing can give it a message
  "before its first turn". Only `riff read` there could show it.
- A short ID of a gone session gives "no session matches", not "is
  gone": the client looks for the ID only in the live sessions
  (`crates/riff-client/src/api.rs:1427` to `:1446`).
- No merged or open pull request changes the refusal.

Questions for mike:

- Q1: Does a direct message to a gone session wait for it? That
  supersedes R206 and R60 in part. Proposal: no. Keep the refusal, so the
  sender knows at once that nobody got the message. A host session has
  no way to get a message (01M59BXNPR3AZ0HAS9GQCZ4PH2).
- Q2: How does the lead reach a person whose host session is gone?
  Proposal: as the strata lead does now, with a post to
  `{user: USER, person: true}` in the thread of the repository. The
  refusal of `riff tell` names that way.
- Q3: With Q1 and Q2 as proposed, the issue becomes small. Proposal for
  the new body: the help of `riff tell` and the tool `tell` say that a
  `tell` to a gone session fails, and how to reach its person. The
  client finds a short ID also among the gone sessions, so the refusal
  says "is gone". Done when: a test of `riff tell` with a short ID of
  a gone session gets "is gone" and the way to reach the person; the
  help says it. Docs: "Answer the sender of a message"
  (`docs/src/how-it-works.md:4190`) says what happens when the session
  is gone.
