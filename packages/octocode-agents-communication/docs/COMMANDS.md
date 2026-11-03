# CLI command reference

Load when you need a command example. `comm` and `agent` are the shell helpers from [installation](INSTALLATION.md#reuse-or-create-the-binding): `comm` adds workspace and database flags, `agent` also adds `--session`. Reuse a host binding when one exists. Typed flags, stdin and multiword forms: [entry points](INTERFACES.md#supply-cli-input).

Each example runs after you replace IDs, paths and placeholders. For every optional field and validation rule, use `comm schema COMMAND` (multiword: `comm schema 'db export'`) or `comm COMMAND --help`.

| Command | When to use it | Example |
| --- | --- | --- |
| `join` | No supplied identity; enter a workspace. | `comm join '{"name":"reviewer","vendor":"claude"}'` |
| `binding` | Confirm own identity, branch, and host binding before work. | `agent binding` |
| `heartbeat` | Maintain live presence; renew owned live leases or update branch. | `agent heartbeat '{"ttlMs":300000,"renewLeases":true,"branch":"feature/api"}'` |
| `set_status` | Advertise changed task/status; does not renew presence. | `agent set_status '{"task":"Review API","status":"busy"}'` |
| `resume` | Reuse an expired identity of the same vendor; reacquire leases. | `agent resume '{"vendor":"claude"}'` |
| `leave` | Finish a lifecycle this process owns; release leases/subscriptions/claims. | `agent leave` |
| `peers` | Find live collaborators and exact recipient IDs. | `agent peers` |
| `send_message` | Send parts to one peer, every current peer (`to:"*"`), or a subscribed topic. `replyRequired` is explicit; `contextId` groups related messages. | `agent send_message '{"to":"api-reviewer","parts":[{"text":"Review src/api.ts for compatibility risks and return evidence."}],"replyRequired":true,"reasoning":"Resolve compatibility before release"}'` |
| `inbox` | Read pending unexpired deliveries; optionally one message. | `agent inbox '{"message":17}'` |
| `inbox wait` | Explicitly wait for pending mail; does not renew presence or acknowledge it. | `agent inbox wait '{"timeoutMs":1000}'` |
| `complete` | Report an explicit request outcome with reply parts, or silently acknowledge handled messages. | `agent complete '{"message":17,"outcome":"completed","reply":[{"text":"No breaking change; the signature is unchanged (src/api.ts:42)."}]}'` |
| `subscribe` | Replace the topics this identity receives. | `agent subscribe '{"topics":["api"]}'` |
| `lock` | Reserve one file or subtree before editing; `wait:true` queues on conflict. | `agent lock '{"path":"src/api.ts","kind":"file","reasoning":"Implement reviewed API change"}'` |
| `lock_many` | Reserve up to 32 distinct paths atomically; conflicts grant none. `wait:true` queues the set until every path is free. | `agent lock_many '{"paths":[{"path":"src/api.ts"},{"path":"tests/api.ts"}],"reasoning":"Change API and its tests"}'` |
| `renew` | Extend a still-live owned lease; false means reacquire. | `agent renew '{"leaseId":LEASE_ID,"ttlMs":60000}'` |
| `unlock` | Release a finished owned lease. | `agent unlock '{"leaseId":LEASE_ID}'` |
| `locks` | Inspect live or expired reservations and owners. | `agent locks '{"presence":"all","path":"src"}'` |
| `check_paths` | Inspect overlap candidates; it grants no ownership. | `agent check_paths '{"paths":[{"path":"src","kind":"tree"}]}'` |
| `check_write` | Verify own live coverage for concrete write paths. | `agent check_write '{"paths":[{"path":"src/api.ts","kind":"file"}]}'` |
| `attach` | Bind a real existing receiver or choose manual/raw delivery. | `agent attach '{"transport":"raw"}'` |
| `dispatch` | A delivery owner submits one pending batch to its existing receiver. | `agent dispatch` |
| `hook` | Offer incoming context to a raw/custom host bridge. | `agent hook '{"format":"json"}'` |
| `confirm_delivery` | Confirm SDK acceptance using the exact offered ID/token pairs. | `agent confirm_delivery '{"items":[{"id":17,"dispatchToken":"EXACT_UUID"}]}'` |
| `listen` | Own a delivery loop without starting an LLM; maintain presence. | `agent listen --duration-ms 60000` |
| `retry_delivery` | Explicitly retry an inspected staged or uncertain attempt; duplication is possible. A submitted attempt (host accepted it) is refused, never resent. | `agent retry_delivery '{"message":17,"reason":"Receiver inspection confirms retry is needed"}'` |
| `host-config` | Generate a hook settings preview to merge into the host. | `comm host-config --vendor codex` |
| `host-hook` | Host hook handler; consumes actual host event JSON on stdin. | `comm host-hook --vendor codex < /absolute/host-event.json` |
| `completion-check` | Optional Claude Stop recovery guard; host supplies stop-event JSON. | `agent completion-check - < /absolute/stop-event.json` |
| `mcp` | Expose bound tools to a host; managed mode owns lifecycle. | `comm mcp --managed --name reviewer --vendor claude --tools review` |
| `run` | Launch a requested new worker, not message delivery. | `comm run --vendor codex --model HOST_MODEL --prompt 'Review the API' --tools review` |
| `share_document` | Store immutable larger evidence with optional contextual summary; an ADR revision names the document it supersedes. | `agent share_document '{"name":"api-review.md","content":"Evidence...","reasoning":"Preserve review","context":{"summary":"API compatibility review","path":"src/api.ts"}}'` |
| `read_document` | Verify hash and read document pages, following every continuation. | `agent read_document '{"name":"api-review.md","offset":0,"limit":4096}'` |
| `context` | Discover live path/branch notes; follow next even on an empty page. | `agent context '{"path":"src/api.ts","branch":"feature/api"}'` |
| `activity` | Read Git files/commits/reflog evidence; not edit ownership. | `comm activity '{"view":"files","limit":20}'` |
| `record_usage` | Record measured request/turn/cumulative usage without inventing missing counts. | `agent record_usage '{"key":"api-turn-1","scope":"turn","model":"HOST_MODEL","inputTokens":120,"outputTokens":40}'` |
| `fetch` | Query the unified typed history efficiently. Use `inbox` for pending deliveries. | `agent fetch '{"type":"event","where":{"name":"review.finished"},"limit":20}'` |
| `record` | Save generic JSON memory/events or a checkpoint (`type:"checkpoint"`, returned by `binding` and `resume`); does not notify or mutate operational state. | `agent record '{"type":"memory","data":{"content":"Verified API behavior","tags":["api"]}}'` |
| `health` | Read delivery issues without creating storage or exposing bodies. | `comm health '{"staleAfterMs":300000,"limit":20}'` |
| `prune` | Remove expired leases in batches of 100; preserve audit history. | `comm prune` |
| `view` | Open a loopback read-only dashboard; Ctrl-C stops it. | `comm view '{"open":true}'` |
| `db info` | Inspect storage compatibility before joining; creates nothing. | `comm db info` |
| `db protocol` | Get complete SQL schema/transaction rules for a custom client. | `comm db protocol` |
| `db export` | Snapshot the entire DB/WAL to a new absolute file; documents need separate backup. | `comm db export '{"path":"/absolute/new-backup.sqlite"}'` |
| `db retention` | Inspect aged-record counts/pages across workspaces; deletes nothing. | `comm db retention '{"limit":100}'` |
| `db compact` | Explicit storage maintenance with VACUUM; retains all records. | `comm db compact` |
| `skill` | Read the detailed operating guide through the CLI. | `comm skill` |
| `schema` | Discover the catalog (it includes the `results` schema of every command) or one command; record types use unbound `schema types` / `schema type`. | `comm schema send_message` |
| `schema tools` | Inspect tool definitions for a profile or explicit list. | `comm schema tools --tools messaging` |

## Activity pathRegex

`activity`'s `pathRegex` filters changed paths with a JavaScript `RegExp` compiled with the `u` flag, so write ECMAScript syntax:

- `\d`, `\w`, `\s` and `\b` are ASCII-only (`\s` is the ECMAScript whitespace set).
- Named groups are `(?<name>…)`; `(?P<name>…)`, `\A`, `\Z` and inline flags such as `(?i)` are refused. Anchor with `^` and `$` (`$` matches only at the end of the path).
- Under `u`, identity escapes such as `\-` outside a character class are errors.
- Look-around and backreferences are refused with `Invalid pathRegex: look-around and backreferences are not supported`; an invalid pattern reports the engine's message.
- Matching stops at the request's shared 5-second deadline (the same one Git gets), so a catastrophic pattern fails instead of hanging.
