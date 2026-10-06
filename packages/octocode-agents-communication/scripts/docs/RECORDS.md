# Records and scoped queries

Load when you query history, memories, events, or record fields. Why: distinguish history cursors from operational IDs.

## Envelope and IDs

Every entity/event uses `{recordId,path,from,to,type,timestamp,branch?,data}`.
`path` is the originating canonical worktree. `from` is the exact DB identity.
`timestamp` is Unix milliseconds. The runtime assigns these fields and `type`.
`to` is present and nullable for shared records. `branch` is optional.
`data` is a JSON object with fields selected by type.
Memory and event data also allow arbitrary JSON fields.

| Identifier | Use |
| --- | --- |
| `recordId` | History cursor and `fetch {recordId}` filter; never a message ID |
| `data.messageId` | Reply or complete |
| `data.leaseId` | Lease commands |

`comm schema types --compact` lists all types and fields.
`comm schema type message` returns one schema, semantics, and example.
Current v4 writers emit full coordination and lease snapshots.
Migrated history retains its original data; absent historical fields are unknown.
In/out records describe the same worktree agent entering/leaving, with the same name and vendor.
A crash creates no synthetic out record. Use `peers` for current liveness.

## Type summary

| Record types | `data` fields (`?` optional) | Writer |
| --- | --- | --- |
| `coordinate.in`, `coordinate.update`, `coordinate.profile`, `coordinate.out` | name, vendor, vendorSession?, task, status, expiresAt | join/resume, heartbeat, set_status, leave |
| `message` | messageId, body, ttlMs, expiresAt, reasoning, wake, replyRequired, key?, topic?, conversationId?, replyTo?; oversized delivered references add bodyOmitted? and next? | send_message, notify_all, complete |
| `lease.acquired`, `lease.renewed`, `lease.removed` | leaseId, path, kind, expiresAt, reasoning | lock/lock_many, renew/heartbeat, unlock/leave/prune |
| `subscription.added`, `subscription.removed` | topic | subscribe/leave |
| `attachment.created`, `attachment.updated` | transport, endpoint (nullable) | attach |
| `delivery.created`, `delivery.claimed`, `delivery.acknowledged` | messageId; claimed also has owner | send/fanout, delivery owner, complete |
| `dispatch.staged`, `dispatch.submitted`, `dispatch.uncertain`, `dispatch.ready` | messageId, token, transport, error? (nullable) | dispatch/bridge, retry_delivery |
| `document` | name, path, author, bytes, sha256, reasoning, context? {summary, path?, kind?, branch?, ttlMs?, expiresAt?} | share_document |
| `usage` | key, scope, model?, inputTokens?, outputTokens?, cachedInputTokens?, contextTokens?, cacheWriteTokens? | record_usage |
| `host.context` | generation, vendor | hook/host integration |
| `host.inbound` | vendor, permissionMode, crossSessionInbound? | Claude hook |
| `memory` | content, tags?, scopePath?, expiresAt?, arbitrary additional fields | record |
| `event` | name, arbitrary additional fields | record |

## Find the deciding records

Start with an indexed scope: `type`/`types`, `from`, `to`, `since`/`until`, or branch.
Use literal full-text `search` for content.
Use `where` for up to eight scalar dotted data fields; pair it with a type or sender.
Avoid loading the whole history to find one message.
Messages are participant-visible. Shared records use repository coordination scope and retain their origin path.
Lease ownership stays worktree-local. `context` shares matching relative-path notes across the repository.
Each context item names its originating workspace and branch; it grants no edit ownership.
An authorized operator audit can use `view` or a `db export` snapshot.
Participant-scoped `fetch` is not a whole-repository conversation audit.

```sh
agent fetch '{"incoming":true,"type":"message","limit":20}'
agent fetch '{"types":["coordinate.in","coordinate.out"],"from":"PEER_DB_ID","limit":20}'
agent fetch '{"type":"memory","search":"API compatibility","branch":"feature/api","limit":20}'
agent fetch '{"type":"dispatch.uncertain","where":{"messageId":17},"current":true}'
agent record '{"type":"memory","key":"api-fact-1","data":{"content":"API keeps the existing response shape","tags":["api"],"source":"src/api.ts"}}'
agent record '{"type":"event","data":{"name":"review.finished","result":{"checks":["API smoke"],"owner":"api-reviewer"}}}'
```

`incoming:true` reads pending mail for the bound recipient.
Start a new incoming scan from zero at a later boundary. Reads never acknowledge or reply.
`current:true` selects the current dispatch state/token during delivery investigation.

## Read complete pages

Call each `next.command` with `next.input` unchanged and the same binding.
Continue until no next remains, including empty pages with a continuation.
`after` is exclusive. `through` fixes a scan ceiling so concurrent writes cannot move its endpoint.
Fetch returns at most 100 records per page with a byte budget.
An oversized record remains intact with a diagnostic. Preserve all evidence and continuations.
Oversized hook references have `data.bodyOmitted:true` and `data.next`.
Follow that body continuation unchanged before acting; a reference is not the full message.

## Write generic records

`record` accepts only `memory` or `event`, with at most 16,384 UTF-8 bytes of JSON data.
Optional `to`, `branch`, and `key` control visibility, history, and unchanged retries.
Use dedicated commands for operational records so current state and audit history stay consistent.
Memory/event writes send no notification. Use messages when another owner must act.

For exact database/client rules, read [the database protocol](DB.md).
