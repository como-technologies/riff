# Review 03: the attacker

Issue: #304. Design at commit bf325d9.

## Summary

The design keeps the strong parts of the current auth. Device keys
stay on the device. Each token is bound to its key with DPoP. Each
message has a signature, and the signature now names the riff. But the
new `MessageContent` does not sign the user or the lead mark of the
sender, and the `Post` record does not keep the public key. So a reader
cannot check a message, and a server or a bucket could give any
message `lead=true`. The design also does not say who may read a
thread over the new `Tail` stream and from old chunks, and a rollback
can lose a record for good. The trust in the bucket is not stated: an
account that can write the bucket controls the riff.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| 03-1 | `MessageContent` has `sender_session`, but not the user and not the lead mark. Today the signature covers both (R196, `riff_core::signed::Content`). A reader gets the user and `lead=true` from the server, outside the signature. So a changed server, or a person who writes the bucket, can mark each message as from the lead. A worker then acts on it as a request from its user. This breaks R198 and R200. | must-fix | The log / Signed messages | Add `string sender_user` and `bool lead` to `MessageContent`. The server refuses a post with `lead = true` from a session that is not the lead (R198). A reader takes the user and the lead mark only from the decoded signed bytes. |
| 03-2 | `Post` keeps `signature` and `signature_scheme`, but no public key. Today the key is in the JWS header (`jwk`), and `read` gives only the thumbprints of the keys (`Keys` in `wire.rs`). A thumbprint cannot check a P-256 signature. So with the proto of the design, no reader can check a message. | must-fix | The log / Signed messages | Add the public key of the signer to `Post` (for example `bytes signer_key`, a SEC1 point), outside `signed_content`. The reader checks the signature with it, then checks that its thumbprint is one of the keys of the sender user. |
| 03-3 | The design does not say who may read a thread with the new `Tail` stream, or with a read of old messages from the chunks. `read` today refuses a direct thread to a session that is not in it (`State::read`). A stream or a chunk read with no such check gives the direct messages of other sessions, for example the requests of a lead, to each member. | must-fix | The wire / gRPC for the calls; Reads | State one rule for each path that gives messages (`Read`, `Tail`, `Watch`, a read from the chunks): the caller acts as its token, and gets a direct thread only when it is one of its two sessions. Add a given/when/then test for each path. |
| 03-4 | A build skips a record kind that it does not know, then writes a checkpoint past it. After a rollback, a record such as a new kind of member removal has no effect. The next checkpoint of the old build holds the state without it. A newer build then starts from that checkpoint and never applies the record again. So the removed person stays a member. This breaks the goal "each change that must not be lost". | must-fix | The log / The records; The checkpoint; Deploy and rollback | A build writes no checkpoint past the first record that it skipped. Or the checkpoint keeps the positions of the skipped records, and a newer build applies them at start. Add a test: an old build meets a new record, writes a checkpoint, and a new build still applies the record. |
| 03-5 | The design does not say how the server finds a copy of a signed post (01M3JEJVXXEPPNGT3FY4ZSFCWZ). Today it compares the signature bytes. P-256 ECDSA in `p256` 0.14 accepts a signature and its twin with `s` changed to `n - s` (`NORMALIZE_S = false`). The twin is other bytes, so it is not found as a copy. After a start, memory holds only the messages after the checkpoint and the last N messages, so a copy from before the start can also pass. | should-fix | The log / Signed messages; The start and the replay | Find a copy by the hash of `signed_content`, not by the signature. Keep the hashes of the last 5 minutes of posts in the checkpoint, or replay the last 5 minutes at start. Refuse a signature with a high `s`. A reader skips a second message with the same `signed_content` in a thread. |
| 03-6 | The signature input is "a fixed prefix, the riff ID and `signed_content`", with no rule for the bytes. The riff ID is a string of free length (`RiffId` in `token.rs`). With a plain join, two different pairs (riff ID, content) can give the same input. `signature_scheme` is not signed. | should-fix | The log / Signed messages; Appendix B | Define the input exactly: the prefix of the scheme, a zero byte, the length of the riff ID, the riff ID, then `signed_content`. Each `SignatureScheme` has its own prefix, so the scheme is bound too. Add a test with fixed bytes. |
| 03-7 | "A refresh with an older generation is reuse: the server ends the sign-in." The server keeps no hash of an older generation, so it cannot check the secret of that token. After a crash, the design takes "the next" generation as good, but the server has no hash of it either. If the server checks the generation before the DPoP key, a person who knows a chain ID can end each sign-in of that chain. | should-fix | The sign-ins | Write the order: 1. check the DPoP proof and that its key is the key of the sign-in; 2. check the secret against the hash; 3. only then count an older generation as reuse. Keep the hash of the next generation in the snapshot before the reply gives it, so "the next one" is checked too. |
| 03-8 | Each person who can write the bucket controls the riff. The log records (members, admins, owner, leads) are not signed, and `signins.pb` holds the key thumbprints that readers trust. A writer can make a person the owner, or add a key to a user, and that key then signs "verified" messages. The design does not state this trust. The replay checks only the `thread_seq` of posts: a removed chunk of other records is not found. | should-fix | The classes of data; The log / Chunks; Set up | State the trust: bucket write is admin of the riff. Keep the IAM of today: only the `riff-server` account has `roles/storage.objectUser`, on the bucket, with uniform access and public access prevention (`deploy/cloud-setup.sh`). The replay checks that the positions have no gap. Each chunk holds the hash of the chunk before it, so a changed, removed or reordered chunk is found. |
| 03-9 | A watch stream checks the caller only when it opens (`watch` in `lib.rs`), and the design gives no other rule for its streams. A Cloud Run request can live 3600 s. A member that an admin removes, or a sign-in that ends, keeps an open stream and gets new messages until it closes. | should-fix | Live messages | End each open stream of a sign-in when the sign-in ends or its person is removed. Or end each stream when its access token expires (`ACCESS_TTL`, 10 minutes), and the client opens it again with a new token. |
| 03-10 | The design keeps DPoP replay IDs in memory, but it does not say that a start refuses each proof from before the start (`refuse_before(start)` in `lib.rs`). Without it, a new instance accepts again each proof of the last 5 minutes. The token endpoint takes a proof with no access token, so it is the path at risk. | note | The sign-ins; The lease | Keep the rule: a new instance refuses each proof with an `iat` before its start. The lease makes sure that two instances do not serve at the same time. |
| 03-11 | A new field in `MessageContent` or in a `Selector` that limits a message (for example a time of expiry, or a narrower address) is skipped by an older build. The older build shows the message as verified with a wider meaning. | note | Appendix B / A new field | A field that limits the meaning of a message comes with a new `SignatureScheme` (or a list of critical fields, as `crit` in JWS). An older build then shows the message as not verified. Add this rule to "The rules for a change". |
| 03-12 | A message is part of the log for 90 days, and older versions of each object stay 7 days more. A person who posts a secret by mistake (against R11) cannot take it out. A person with read access to the bucket reads each message, also direct messages, and the email of each member. | note | The classes of data; Set up; Backup and restore | Write a how-to to remove a message: stop the server, write the chunk again without it, delete the old versions, start. The chunk hash chain of 03-8 then needs a new start from a checkpoint. |
| 03-13 | Go live says "the old state in the bucket goes away after 30 days". The lifecycle rule of today deletes only `threads/` objects (`deploy/lifecycle.json`). The old token store, with the email of each person and the hash of each refresh token, stays. | note | Go live | Delete the old objects in the go-live steps, or add a rule for them. |
| 03-14 | `PostRequest` carries the thread, the sender and the selectors, and the signed bytes carry them again. The server compares them. Two copies give room for a bug in the compare. | note | The log / Signed messages | The call carries only `signed_content`, the signature, the scheme and the key. The server takes the fields from the decoded bytes. |

Levels: must-fix (the design fails its goals without it), should-fix
(a real gain), note (for the lead to know).

## The questions of this point of view

1. **Can I forge or replay a message, or use a signature in another
   riff or for other data?**
   - Forge: not without the device key of the sender. The server
     refuses a post that is not signed with the key of the token of the
     caller (R197). But the design as written lets a server or a bucket
     writer set the user and the lead mark outside the signature (03-1),
     and gives the reader no key (03-2).
   - Replay: the signed time and the 5-minute window stop an old copy.
     The copy check inside the window depends on the signature bytes,
     and ECDSA gives a second valid form of each signature (03-5).
   - Another riff: the riff ID in the signature input stops it. Today
     the signature does not name the riff, so this is a gain. The bytes
     of the input need an exact rule (03-6).
   - Other data: the prefix `riff/message/v1` separates a message from
     a DPoP proof and from each other signed thing, if each has its own
     prefix. Today `typ` does this job.
2. **Can I reuse a refresh token, or steal a sign-in from the
   snapshot, the checkpoint or the log?**
   - A refresh token is bound to the device key of its sign-in, so a
     copied token alone does not work. A second use of an old
     generation ends the sign-in. The check order must put the key
     first, or a stranger can end a sign-in (03-7).
   - The snapshot holds only the hash of the current generation, the
     key thumbprint and the email. No token and no private key. So a
     reader of the bucket cannot sign in. The checkpoint and the log
     hold no token.
   - A writer of the bucket can add a sign-in or a key (03-8).
3. **Are secrets in the log, the checkpoint or the sign-ins? Who can
   read the bucket?**
   - No token, no private key and no client secret. The OIDC client
     secret stays in Secret Manager.
   - The log holds each message, also direct messages, and the email
     of each member. These are private, and they stay for 90 + 7 days
     (03-12).
   - The `riff-server` account reads and writes the bucket. A project
     owner can too. CI deploys as `riff-server` only from `main` and
     from `v*` tags (the workload identity condition in
     `deploy/cloud-setup.sh`). So the people who can merge to `main` or
     push a tag can read and change the state through a build.
4. **Is DPoP in gRPC metadata as strong as DPoP over HTTP? What does
   the proof bind?**
   - Yes, for a unary call. The proof binds the method `POST`, the URL
     (the public URL of the server plus the gRPC path, for example
     `/riff.v1.Riff/Post`), the time, a new `jti`, and the hash of the
     access token. That is the same as today. DPoP over HTTP does not
     bind the body either. TLS protects the body.
   - The server must make the URL from its own public URL, as today,
     not from the `:authority` of the request.
   - A retry after `UNAVAILABLE` needs a new proof with a new `jti`.
   - A stream is bound only when it opens (03-9). A later bidirectional
     chat stream must sign each message in it, as a post.
5. **What can an old build that does not know a new field or a new
   signature scheme be made to accept?**
   - A new scheme: nothing. The old build shows the message as not
     verified. That fails closed, which is correct.
   - A new field that limits a message: the old build skips it and
     shows the message with a wider meaning (03-11).
   - A new record kind: the old build skips it, and its checkpoint can
     lose it for good (03-4).
   - A new `MessageKind` value: prost keeps it as an unknown number.
     The old build must show it as a plain message, not as a note and
     not as a status request.

## Questions for the lead

1. I sent the lead a direct message about one more finding. It is not
   in this file. The lead decides how to track it.
2. Must the admin changes in the log (invite, remove, admin, owner)
   carry a signature of the admin, as a post does? This gives a record
   of who did each change, and a bucket writer cannot forge one. It
   costs a signature check on each admin call.
3. Must a reader check old messages after a sign-in ends? Today a
   message is verified only while the key of its sender has a live
   sign-in (R199). After a person signs in again, each older message of
   that person shows as not verified.
