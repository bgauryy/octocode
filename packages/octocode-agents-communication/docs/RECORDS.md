# Records and scoped queries

Load when you query history, memories, events, or record fields.

## Envelope and IDs

Every entity and event uses `{recordId,path,from,to,type,timestamp,branch?,data}`, assigned by the runtime; field meanings and storage rules are in the [database protocol](DB.md#unified-records). Memory and event `data` also allow arbitrary JSON.

| Identifier | Use |
| --- | --- |
| `recordId` | History cursor and `fetch {recordId}` filter; never a message ID |
| `data.messageId` | Reply or complete |
| `data.leaseId` | Lease commands |

`npx -y @octocodeai/octocode-agents-communication /cli schema types --compact --json` lists all types and fields; `schema type message --json` returns one schema, its meaning and an example. These discovery commands take no workspace, database or session flags, so call them without the bound `comm` helper.

Writers emit full coordination and lease snapshots. A crash creates no out record; use `peers` for current liveness.

## Type summary

| Record types | `data` fields (`?` optional) | Writer |
| --- | --- | --- |
| `coordinate.in`, `coordinate.update`, `coordinate.profile`, `coordinate.out` | name, vendor, vendorSession?, task, status, expiresAt | join/resume, heartbeat, set_status, leave |
| `message` | messageId, parts, ttlMs, expiresAt, reasoning, wake, replyRequired, outcome?, key?, topic?, contextId?, replyTo?; oversized delivered references add contentOmitted? and next? | send_message, complete |
| `lease.acquired`, `lease.renewed`, `lease.removed` | leaseId, path, kind, expiresAt, reasoning | lock/lock_many, renew/heartbeat, unlock/leave/prune |
| `subscription.added`, `subscription.removed` | topic | subscribe/leave |
| `attachment.created`, `attachment.updated` | transport, endpoint (nullable) | attach |
| `delivery.created`, `delivery.claimed`, `delivery.acknowledged` | messageId; claimed also has owner | send/fanout, delivery owner, complete |
| `dispatch.staged`, `dispatch.submitted`, `dispatch.uncertain`, `dispatch.ready` | messageId, token, transport, error? (nullable) | dispatch/bridge, retry_delivery |
| `document` | name, path, author, bytes, sha256, reasoning, supersedes? (ADR link), context? {summary, path?, kind?, branch?, ttlMs?, expiresAt?} | share_document |
| `usage` | key, scope, model?, inputTokens?, outputTokens?, cachedInputTokens?, contextTokens?, cacheWriteTokens? | record_usage |
| `host.context` | generation, vendor | hook/host integration |
| `host.inbound` | vendor, permissionMode, crossSessionInbound? | Claude hook |
| `memory` | content, tags?, scopePath?, expiresAt?, arbitrary additional fields | record |
| `event` | name, arbitrary additional fields | record |
| `checkpoint` | goal, next, done?, inProgress?, openQuestions?, files? | record; `binding` and `resume` return the latest |

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
agent inbox '{"after":0}'
agent fetch '{"types":["coordinate.in","coordinate.out"],"from":"PEER_DB_ID","limit":20}'
agent fetch '{"type":"memory","search":"API compatibility","branch":"feature/api","limit":20}'
agent fetch '{"type":"dispatch.uncertain","where":{"messageId":17},"current":true}'
agent record '{"type":"memory","key":"api-fact-1","data":{"content":"API keeps the existing response shape","tags":["api"],"source":"src/api.ts"}}'
agent record '{"type":"event","data":{"name":"review.finished","result":{"checks":["API smoke"],"owner":"api-reviewer"}}}'
agent record '{"type":"checkpoint","data":{"goal":"Review API","done":["src/api.ts"],"next":"Review tests/api.ts"}}'
```

`incoming:true` reads pending mail for the bound recipient; start a new scan from zero at a later boundary. Reads never acknowledge or reply. `current:true` selects the current dispatch state and token during delivery investigation.

## Read complete pages

Call each `next.command` with `next.input` unchanged and the same binding until no `next` remains, including after empty pages. `after` is exclusive; `through` fixes the scan ceiling so concurrent writes cannot move it. A page holds at most 100 records within a 16 KiB target; an oversized record stays intact with a diagnostic. Oversized hook references carry `data.contentOmitted:true` and `data.next`: follow it before acting, since a reference is not the full message.

## Write generic records

`record` accepts only `memory`, `event` or `checkpoint`, with at most 16,384 UTF-8 bytes of JSON data. Optional `to`, `branch` and `key` control visibility, history and unchanged retries. Operational records come only from their dedicated commands. Record writes notify no one; send a message when another owner must act.

Exact database and client rules: [database protocol](DB.md).
