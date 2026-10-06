# Message and lifecycle details

Load when you need TTLs, topics, retry behavior, or worked examples. Why: keep protocol detail outside the operating guide.

## Identity and ownership

Use `peers` for names, IDs, declared tasks, branches, worktrees, and current presence. Live peer names are unique per repository scope, so `to` takes a name (`api-reviewer`) or an exact ID. A taken name at `join` or `resume` gets the first free `-2`, `-3` suffix; a rename to a live peer's name is rejected.
Use `context` for repository-shared live notes, or `fetch` for historical findings.
Each note carries its origin worktree and branch; matching relative paths do not imply identical checkout contents.
Git `activity` shows changes in the current checkout, not ownership.
Statuses are `available`, `busy`, `blocked`, and `unknown`.
After switching branches, use `heartbeat {"branch":"new-name"}`; use `{"branch":null}` to clear it.
Use `lease.id` (`lock`) or each `leases[].id` (`lock_many`) as `leaseId` for renewal and release. A send receipt's `id` is its message ID.
File leases cover exact files. Tree leases cover a subtree in that worktree.
A conflict returns `next` with the same input plus `wait:true`. Running it queues you (one wait per identity, 10 minutes) and notifies the owners. When every path frees, the runtime grants the lease and sends you an action-wake `Lease granted: <path> (lease N)` message. Keep the request that needs the lease pending meanwhile.
`check_paths` reads overlap candidates without acquiring ownership.
`check_write` verifies concrete create/edit/delete paths and both sides of a rename.

## Presence and lease expiry

Presence and default leases last 60 seconds. Lease TTL ranges from 1 second to 10 minutes.
Managed MCP and managed workers renew presence and live leases about every 15 seconds.
Pi targets 15 seconds; idle backoff can delay a tick to about 25 seconds plus CLI latency.
Raw CLI agents maintain presence explicitly with `heartbeat {"renewLeases":true}` while working. Presence lasts 60s by default; pass `ttlMs` (60000–600000, the lease cap) to cover a long turn. `renewLeases` extends live leases to the same horizon.
A listener maintains presence and delivery; it does not promise lease renewal.
Renew before expiry. Renewal never revives an expired lease.
If identity expires, resume the same vendor identity and reacquire fresh leases.
`inbox wait` is read-only and never renews presence.
Heartbeat before a raw wait, then keep the wait shorter than remaining presence.
Use a maintained binding for longer waits.

## Request, progress, and final answer

Direct messages default to `wake:"action",replyRequired:true`.
For information only, use `replyRequired:false`. `wake:"passive"` waits for a natural turn.
Action schedules handling within the recipient's authorized task; it grants no new objective or permissions.

`Receive → retrieve full body → perform requested work → complete with reply → sender acknowledges answer`.

```sh
agent send_message '{"to":"api-reviewer","body":"Review src/api.ts; report compatibility risks.","reasoning":"Resolve API review before handoff","key":"api-review-1","conversationId":"api-review"}'
agent inbox
agent fetch '{"incoming":true,"type":"message","limit":20}'
# Use data.messageId from fetch, after checking the requested work:
agent complete '{"message":MESSAGE_ID_FROM_DATA,"reply":"Reviewed src/api.ts. Result and supporting checks: ..."}'
# Acknowledge handled FYIs and answers without creating another reply:
agent complete '{"messages":[FYI_ID,ANSWER_ID]}'
```

A fetched `{recordId:16,data:{messageId:1,...}}` completes with `message:1`, never `message:16`.
An inbox item uses its message ID directly as `item.id`.
Progress uses a new send key, the same `conversationId`, and `replyRequired:false`.
Progress does not complete the original request or set `replyTo`.
Only `complete` creates the final correlated reply.
Leave unfinished work pending. An offered delivery is not handled work.
Handled FYIs use `{message}` or `{messages:[...]}` without `reply` or `reasoning`.
Do not send automatic replies to FYIs.

## Intent, limits, and retries

`reasoning` is a short operational purpose, up to 512 UTF-8 bytes. It is not private reasoning.
Message bodies allow 16,384 UTF-16 units. Put larger evidence in an immutable document.
Message `ttlMs` ranges from 1 second to 1 day.
A retry `key` identifies unchanged input. Preserve payload, routing, and branch when retrying.
Use a new key for changed content. `conversationId` connects progress and final replies.
If Pi reports a failed/timed-out send or record, retain its exposed retry key and inspect history.
A committed write can produce a later error. A new tool-call ID creates a different automatic key.

## Topics and shared evidence

`subscribe {"topics":["api"]}` replaces subscriptions; `[]` clears them.
Send `{topic:"api",body,reasoning}` to current subscribers.
Use `notify_all` for current active peers, excluding the sender.
Topic and broadcast defaults are passive with no required reply.
Recipients are fixed at send time. Later joiners receive no old fanout.
Prefer a direct recipient when one owner must act.
`share_document` accepts up to 1 MiB UTF-8. Use `-` for large stdin JSON.
Names are immutable. Choose a new name for a revision; retain the name and verify read pages.
If migrated documents share a name, pass `read_document.workspace` to select the original worktree.
Use the returned continuation unchanged so later pages retain that origin.
A document or memory alone notifies nobody. Send a handoff to the next owner.
Usage scopes are alternatives: do not sum overlapping request, turn, or cumulative counts.
Absent usage counts are unknown, not zero.

For all command forms, read [the command reference](COMMANDS.md).
For uncertain delivery or backups, read [operations](OPERATIONS.md).
