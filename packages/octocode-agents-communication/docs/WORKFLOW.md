# Message and lifecycle details

Load for TTLs, topics, retry behavior, or worked examples. Message rules live in [the operating guide](../OPERATING.md#send-and-handle-messages).

## Identity and ownership

Use `peers` for names, IDs, declared tasks, branches, worktrees, and current presence. Live peer names are unique per repository scope, so `to` takes a name (`api-reviewer`) or an exact ID. A taken name at `join` or `resume` gets the first free `-2`, `-3` suffix; a rename to a live peer's name is rejected.
Use `context` for repository-shared live notes (each carries its origin worktree and branch) and `fetch` for history. Git `activity` shows changes in the current checkout, not ownership. Statuses are `available`, `busy`, `blocked` and `unknown`. After switching branches, run `heartbeat {"branch":"new-name"}` (`null` clears it).

Use `lease.id` (`lock`) or each `leases[].id` (`lock_many`) as `leaseId` for renewal and release; a send receipt's `id` is its message ID. File leases cover exact files, tree leases a subtree in that worktree. A conflict returns `next` with the same input plus `wait:true`. Running it queues you (one wait per identity, 10 minutes) and notifies the owners. A queued wait holds no lease but reserves its paths in first-come order: a direct `lock` or a later wait on an overlapping path conflicts with it and queues behind it, so a third agent cannot take a path as it frees. When every path frees, the runtime grants the lease and sends you an action-wake `Lease granted: <path> (lease N)` message. Keep the request that needs the lease pending meanwhile. `check_paths` reads overlapping leases (not queued reservations) and acquires nothing; `check_write` verifies your coverage for concrete create, edit and delete paths and both sides of a rename.

## Presence and lease expiry

Presence and default leases last 60 seconds; lease TTL ranges from 1 second to 10 minutes.

| Owner | Presence and lease renewal |
| --- | --- |
| Managed MCP, managed `run` worker | `heartbeat {"renewLeases":true}` every 15 seconds |
| Pi inbox adapter | Targets 15 seconds; idle backoff can stretch a tick to about 25 seconds plus CLI latency. Live leases extend to at least 60 seconds from the tick |
| `listen` | Presence and delivery only; no lease renewal |
| Raw CLI | Explicit `heartbeat {"renewLeases":true}` while working; `ttlMs` (60000–600000) covers a long turn and `renewLeases` extends live leases to the same horizon |

Renew before expiry: renewal never revives an expired lease, and an expired identity must `resume` (same vendor) and reacquire leases. A custom lease TTL shorter than the owner's tick can expire before renewal. `inbox wait` (up to 60 seconds) is read-only and never renews presence: heartbeat first and keep the wait shorter than remaining presence.

## Request, progress, and final answer

Send a message as parts. Set `replyRequired:true` when the recipient owes a correlated result; it defaults to `false`. The payload is opaque to the runtime. Task objective, constraints, evidence references, and progress are shared as text, JSON data, URLs, or named artifacts, but none of those fields trigger implicit validation or side effects.

`Receive → inspect all parts → do the authorized work → complete with outcome and reply → sender reads and acknowledges the answer`.

```sh
agent send_message '{"to":"api-reviewer","parts":[{"text":"Review the src/api.ts response change for compatibility. Return risks and supporting evidence."}],"replyRequired":true,"reasoning":"Resolve API review before handoff","key":"api-review-1","contextId":"api-review"}'
agent inbox '{"after":0}'
# Use the message id from inbox, after checking the requested work:
agent complete '{"message":MESSAGE_ID,"outcome":"completed","reply":[{"text":"No breaking change. Evidence: exports remain unchanged at src/api.ts:42."}]}'
# A handled notice or answer needs no reply or outcome:
agent complete '{"messages":[FYI_ID,ANSWER_ID]}'
```

Request outcomes are `completed`, `failed`, `rejected`, or `canceled`. A fetched `{recordId:16,data:{messageId:1,...}}` uses `message:1`, never `recordId:16`. An unfinished request stays pending. A send receipt proves storage, a transport receipt only proves the adapter's reported submission, acknowledgement records handling, and the outcome records the recipient's report; none establishes correctness or approval.

Progress is a new message with the same `contextId` and a new idempotency key; it does not answer or acknowledge the request. A request that needs input can remain pending while the recipient sends another message to ask. Explicit `replyRequired` and `wake` are independent: action schedules delivery, while only `replyRequired` controls the completion contract. No message payload implicitly copies recipients, transfers leases, pins or rewrites evidence, or changes expiry policy.

## Purpose, limits, and retries

`reasoning` is the purpose, up to 512 UTF-8 bytes; it is not private reasoning.
Message parts have no application-level size cap; host delivery can return a `contentOmitted` reference with a continuation when context exceeds its budget. Use an immutable document for reusable evidence with verified content hash and discovery metadata; URL parts are references and are never fetched automatically.
Message `ttlMs` ranges from 1 second to 1 day.
An idempotency `key` identifies unchanged input. Preserve payload, routing, and branch when retrying.
Use a new key for changed content. `contextId` connects progress and final replies.
If Pi reports a failed/timed-out send or record, retain its exposed idempotency key and inspect history; retry unchanged input with that key only when needed.
A committed write can produce a later error. A new tool-call ID creates a different automatic key.

## Topics and shared evidence

`subscribe {"topics":["api"]}` replaces subscriptions; `[]` clears them.
Send `{topic:"api",parts:[{text:"…"}],reasoning}` to current subscribers.
Use `send_message` with `to:"*"` for all current active peers, excluding the sender. Set `replyRequired:true` only when each recipient is expected to return a result.
Wake and reply requirements are explicit message fields; fanout recipients are fixed at send time.
Recipients are fixed at send time. Later joiners receive no old fanout.
`share_document` accepts up to 1 MiB UTF-8; pass large JSON on stdin with `-`. Names are immutable: choose a new name for a revision; an ADR revision sets `supersedes` to the published document it replaces. Usage scopes (request, turn, cumulative) are alternatives, never summed; absent counts are unknown, not zero.

Command forms: [command reference](COMMANDS.md). Uncertain delivery and backups: [operations](OPERATIONS.md).
