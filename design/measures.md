# The load of today

Build item 1 of `docs/src/design-storage.md` (issue #333). Measured on
2026-09-30 at 23:20 UTC on the shared riff (build v0.8.0, e3cfe5a).
The riff had 2 people (mike, brett), 2 repositories (riff, strata) and
8 to 10 live sessions.

`G` is the path of `gcloud`, for example `~/google-cloud-sdk/bin/gcloud`.
Each command only reads.

## Sources

| Name | What it is | Command |
|---|---|---|
| calls | Each POST call to riff-server, from 2026-09-28 23:22 to 2026-09-30 23:19 UTC: time, path, status. 2026-09-29 is the one full day. | `$G logging read 'resource.type="cloud_run_revision" AND resource.labels.service_name="riff-server" AND httpRequest.requestMethod="POST"' --project como-riff --freshness 4d --limit 200000 --format 'value(timestamp,httpRequest.requestUrl,httpRequest.status)' > calls.tsv` |
| objects | Each object of the bucket, with its size. | `$G storage ls -l -r gs://como-riff-state/ > objects.txt` |
| bucket | The size of the bucket. | `$G storage du -s gs://como-riff-state` |
| who | Each session of the riff, also the gone ones. | `riff who --all --long --color never` |
| thread | The repository thread, with its seq numbers. | `riff read --all --thread como-technologies/riff` |

To count the calls of one path on one day, with a 2xx status:

```sh
grep '^2026-09-29' calls.tsv | grep '/v1/post' | grep -c $'\t2[0-9][0-9]$'
```

## The records each day

The calls that change the state give the records of the new log. The
count is of calls with a 2xx status.

| Record | Call | 2026-09-29 | 2026-09-30 (to 23:19) |
|---|---|---:|---:|
| `posted` (each thread, also direct and chat) | `/v1/post` | 594 | 278 |
| `claimed` | `/v1/claim` | 141 | 58 |
| `released` | `/v1/release` | 115 | 53 |
| `joined_thread` | `/v1/join` | 35 | 5 |
| `left_thread` | `/v1/leave` | 0 | 1 |
| `lead_set` | `/v1/lead` | 0 | 2 |
| `person_changed` | `/v1/owner/take`, `/v1/invite`, `/v1/remove`, `/v1/admin`, `/v1/owner` | 1 | 2 |
| `setting_changed` | `/v1/idle` | 0 | 1 |
| `session_forgotten` | none today | 0 | 0 |

- Each new session also joins its repository thread when it registers:
  about 55 `joined_thread` records each day (see "Sessions").
- A session that stops frees its claims. riff-server gives each of them
  a `released` record. `/v1/end` (183 on 2026-09-29) is the upper bound.
- `/v1/riff` is also the read of the state (25,599 calls on
  2026-09-29), so the calls do not give the count of `riff_state_set`.
  A pause or a resume comes a few times each day at most.
- `/v1/status` (2,788 calls on 2026-09-29) makes no record: a status is
  in memory.

The busiest day, 2026-09-29, has about **950 records**: 594 + 141 +
115 + 35 + 55 + 1 + a few. With the releases of stopped sessions, it
has at most about 1,130.

## The peak

From calls, the calls that can write (post, claim, release, join,
leave, lead, owner, idle, register) with a 2xx status:

| Measure | Value | When (UTC) |
|---|---:|---|
| Peak second | 5 | 2026-09-29 03:10:23 |
| Peak minute | 35 | 2026-09-29 17:57 |
| Peak hour | 208 | 2026-09-29 01:00 |
| Hours with writes on 2026-09-29 | 13 of 24 | |

## Sessions

From who:

- 152 sessions (116 of mike, 38 of brett, 41 workers). The state holds
  them since the first message of the repository thread, at 2026-09-28
  04:25 UTC: 67 hours. So about **55 new session IDs each day**, 28
  for each person.
- 8 sessions were live at the measure (6 idle, 2 blocked). 10 were live
  one hour before.

## The size of the state

From objects and bucket, at 2026-09-30 23:18 UTC:

| Object | Bytes |
|---|---:|
| `tokens` (the sign-ins) | 4,764,690 |
| `sessions` (152 sessions, with cursors and claims) | 95,664 |
| `threads/como-technologies%2Friff` (463 messages, from thread) | 682,326 |
| `threads/como-technologies%2Fstrata` | 89,649 |
| `threads/chat` | 30,439 |
| `threads/dm%3A...` (125 direct threads) | 688,035 |
| All threads (129) | 1,496,100 |
| The bucket | 6,355,955 |

- The state as JSON, without the sign-ins: about **1.6 MB**.
- A message is about 1,470 bytes of JSON: 682,326 / 463.
- A session is about 630 bytes of JSON: 95,664 / 152.
- A direct thread is about 5,500 bytes: 688,035 / 125. The riff makes
  about 45 direct threads each day, 23 for each person.
- The sign-ins are 75 % of the bucket. `/v1/token` gave 7,742 calls
  with a status that is not 2xx in the two days.

## The GCS write

Not measured. Mike decided on 2026-09-30: skip the probe until the
latency is a problem. The server of today saves in the background each
second, so the latency of a call does not hold the GCS write. The
design uses the published numbers. `riff server` shows the time of the
last chunk write after build item 8.

## The load at the target

Take for each person 8 live sessions and 20 new session IDs each day.
Today 2 people have about 10 live sessions. So each live session gives
the counts of 2026-09-29 divided by 10.

| Record | Each person, each day |
|---|---:|
| `posted` | 475 (594 / 10 × 8) |
| `claimed` | 113 |
| `released` | 113 |
| `joined_thread` | 48 (28 + 20 new sessions) |
| `session_forgotten` | 20 (as many as new sessions, after 30 days) |
| Other | 2 |
| **All** | **about 770** |

At 36 people (3 dozen):

| Measure | Value |
|---|---:|
| Records each day | 27,700 |
| Records each second, mean | 0.3 |
| Records each second, peak hour | 1.7 (208 / 10 × 288 live, each hour) |
| Log each day, at about 1 KB for each record | 28 MB |
| Chunk writes each month, at 1 record for each chunk | 830,000 |
| Sessions in the state (30 days × 20 × 36) | 21,600, about 14 MB |
| Direct threads in the state (30 days × 23 × 36) | 24,800, about 140 MB |
| Repository threads (36 × 50 messages × 1,470 bytes) | about 3 MB |
| **A checkpoint** | **about 160 MB** |

The direct threads are most of the checkpoint.
