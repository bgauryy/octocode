# CLI command reference

Load when you need a command example. Why: keep the full catalog outside ordinary worker context.

Use the shell bindings from [installation](INSTALLATION.md). Reuse a host binding when one exists.

`comm` invokes `npx -y @octocodeai/octocode-agents-communication /cli` with workspace and database flags.
`agent` adds the current `--session`. The default npm entry starts MCP instead.
Typed flags use kebab-case names: `ttlMs` becomes `--ttl-ms`. Use `--help --json` for the complete CLI schema.
Multiword commands also accept one token: `db info` becomes `db-info`, and `inbox wait` becomes `inbox-wait`.
The CLI adds `schema-types` and `schema-type` for record-type discovery.
See [entry points](INTERFACES.md) for stdin, arrays, booleans, and workspace selection.

Each row gives a runnable form after replacing IDs/paths/placeholders. JSON shown here is input; full optional fields and validation are available through `comm schema COMMAND` or `comm COMMAND --help`. Multiword commands use separate words, for example `comm schema 'db export'`.

| Command | When to use it | Example |
| --- | --- | --- |
| `join` | No supplied identity; enter a workspace. | `comm join '{"name":"reviewer","vendor":"claude"}'` |
| `binding` | Confirm own identity, branch, and host binding before work. | `agent binding` |
| `heartbeat` | Maintain live presence; renew owned live leases or update branch. | `agent heartbeat '{"ttlMs":300000,"renewLeases":true,"branch":"feature/api"}'` |
| `set_status` | Advertise changed task/status; does not renew presence. | `agent set_status '{"task":"Review API","status":"busy"}'` |
| `resume` | Reuse an expired identity of the same vendor; reacquire leases. | `agent resume '{"vendor":"claude"}'` |
| `leave` | Finish a lifecycle this process owns; release leases/subscriptions/claims. | `agent leave` |
| `peers` | Find live collaborators and exact recipient IDs. | `agent peers` |
| `send_message` | Ask one peer or publish to a subscribed topic. | `agent send_message '{"to":"api-reviewer","body":"Review the API","reasoning":"Resolve compatibility"}'` |
| `notify_all` | A relevant announcement to current active peers. | `agent notify_all '{"body":"API review is ready","reasoning":"Coordinate review","replyRequired":false}'` |
| `inbox` | Read pending unexpired deliveries; optionally one message. | `agent inbox '{"message":17}'` |
| `inbox wait` | Explicitly wait for pending mail; does not renew presence or acknowledge it. | `agent inbox wait '{"timeoutMs":1000}'` |
| `complete` | Handle a request with a final reply, or acknowledge read FYIs. | `agent complete '{"message":17,"reply":"Checked; supporting evidence: ..."}'` |
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
| `retry_delivery` | Explicitly retry an inspected uncertain attempt; duplication is possible. | `agent retry_delivery '{"message":17,"reason":"Receiver inspection confirms retry is needed"}'` |
| `host-config` | Generate a hook settings preview to merge into the host. | `comm host-config --vendor codex` |
| `host-hook` | Host hook handler; consumes actual host event JSON on stdin. | `comm host-hook --vendor codex < /absolute/host-event.json` |
| `completion-check` | Optional Claude Stop recovery guard; host supplies stop-event JSON. | `agent completion-check - < /absolute/stop-event.json` |
| `mcp` | Expose bound tools to a host; managed mode owns lifecycle. | `comm mcp --managed --name reviewer --vendor claude --tools review` |
| `run` | Launch a requested new worker, not message delivery. | `comm run --vendor codex --model HOST_MODEL --prompt 'Review the API' --tools review` |
| `share_document` | Store immutable larger evidence with optional contextual summary. | `agent share_document '{"name":"api-review.md","content":"Evidence...","reasoning":"Preserve review","context":{"summary":"API compatibility review","path":"src/api.ts"}}'` |
| `read_document` | Verify hash and read document pages, following every continuation. | `agent read_document '{"name":"api-review.md","offset":0,"limit":4096}'` |
| `context` | Discover live path/branch notes; follow next even on an empty page. | `agent context '{"path":"src/api.ts","branch":"feature/api"}'` |
| `activity` | Read Git files/commits/reflog evidence; not edit ownership. | `comm activity '{"view":"files","limit":20}'` |
| `record_usage` | Record measured request/turn/cumulative usage without inventing missing counts. | `agent record_usage '{"key":"api-turn-1","scope":"turn","model":"HOST_MODEL","inputTokens":120,"outputTokens":40}'` |
| `fetch` | Query the unified typed history or pending incoming mail efficiently. | `agent fetch '{"type":"event","where":{"name":"review.finished"},"limit":20}'` |
| `record` | Save generic JSON memory/events; does not notify or mutate operational state. | `agent record '{"type":"memory","data":{"content":"Verified API behavior","tags":["api"]}}'` |
| `health` | Read delivery issues without creating storage or exposing bodies. | `comm health '{"staleAfterMs":300000,"limit":20}'` |
| `prune` | Remove expired leases in batches of 100; preserve audit history. | `comm prune` |
| `view` | Open a loopback read-only dashboard; Ctrl-C stops it. | `comm view '{"open":true}'` |
| `db info` | Inspect storage compatibility before joining; creates nothing. | `comm db info` |
| `db protocol` | Get complete SQL schema/transaction rules for a custom client. | `comm db protocol` |
| `db export` | Snapshot the entire DB/WAL to a new absolute file; documents need separate backup. | `comm db export '{"path":"/absolute/new-backup.sqlite"}'` |
| `db retention` | Inspect aged-record counts/pages across workspaces; deletes nothing. | `comm db retention '{"limit":100}'` |
| `db compact` | Explicit storage maintenance with VACUUM; retains all records. | `comm db compact` |
| `db migrate` | Upgrade recognized v1/v2/v3/v4 storage to v5 with a new backup, after stopping old clients. | `comm db migrate '{"backup":"/absolute/new-pre-migration.sqlite"}'` |
| `skill` | Read the detailed operating guide through the CLI. | `comm skill` |
| `schema` | Discover commands, full record types, or one type/command. | `comm schema type coordinate.out` |
| `schema tools` | Inspect tool definitions for a profile or explicit list. | `comm schema tools --tools messaging` |
