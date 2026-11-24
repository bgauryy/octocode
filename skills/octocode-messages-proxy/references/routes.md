# Other routes

Use these when `ask` does not fit: an ongoing relay, a native same-vendor proxy, A2A Agent Cards, a Claude cloud proxy, or a manual managed relay.

## Bind a missing worker

A worker with no store binding is unreachable until it binds from its own host; the operator cannot bind it.

| Worker host | Bind |
|---|---|
| Cursor or Grok | `octocode-agents-communication /cli host-config --json --vendor cursor` (or `grok`) `--workspace "$REPO"` prints the hooks. Merge them into `.cursor/hooks.json` or `.grok/hooks/communication.json` and reload the host; Grok also needs a trusted Git project root (`grok inspect --json`). |
| Pi | Load the package's `pi-inbox` extension (`registerPiInbox`). Only when a new worker is requested, `run --vendor pi` starts one already bound. |

Version-specific steps: `HOST_SETUP.md` under the `referenceRoot` that `octocode-agents-communication /cli skill --json` returns. The worker is bound once `python3 "$RELAY" peers --workspace "$REPO"` lists it under `store`; then `ask`.

## Manual managed relay

All participants need live bindings in the same Octocode store and repository scope (linked worktrees share scope); [bind a missing worker](#bind-a-missing-worker) first. Use one package installation throughout; both `comm` and the launcher prefer an `octocode-agents-communication` on PATH over `npx`:

```sh
comm() (
  cd "$REPO" || exit
  if command -v octocode-agents-communication >/dev/null 2>&1; then
    octocode-agents-communication /cli "$@"
  else
    npx -y @octocodeai/octocode-agents-communication /cli "$@"
  fi
)
comm schema send_message --json   # authoritative; `body` instead of `parts` means the installation predates this skill
python3 "$RELAY" peers --workspace "$REPO" --session "$OPERATOR"   # read-only: every peer, deduplicated
```

Set `OPERATOR` to your binding (or `comm join --name message-operator --vendor "$OPERATOR_VENDOR" --json`, kept alive with `listen` or `heartbeat`). Follow every emitted continuation. `peers` merges the store's live peers (every page) with Claude's native roster, one row per confirmed vendor/native ID. A row whose `routes` holds `store` is reached by its `store[].id`, and `native` by `claude:<nativeId>`. For an all-agent request, message every row with nonempty `routes`. Report as unreachable each row with empty `routes` (its `reason` says why) and each `processes` row with `binding: "none"`. A `binding: "unconfirmed"` process may be one of the store peers, but no ID ties them. Exit 1 (`rosterUnread`) still lists what was read, so report the unread roster. Set `WORKER` to a `store[].id` and `WORKER_VENDOR` to its `vendor`; the proxy inherits no conversation, so the task supplies identities and scope:

```sh
TASK="Operator: $OPERATOR. Authorized worker: $WORKER. Relay only between these identities and return the worker's actual replies and outcomes."
python3 "$RELAY" start --worker-vendor "$WORKER_VENDOR" --workspace "$REPO" --task "$TASK" --dry-run
mkdir -p "$REPO/.octocode/tmp/octocode-messages-proxy"   # scratch: the relay log of this run
RUN_LOG=$(mktemp "$REPO/.octocode/tmp/octocode-messages-proxy/relay.XXXXXX")
python3 "$RELAY" start --worker-vendor "$WORKER_VENDOR" --workspace "$REPO" --task "$TASK" >"$RUN_LOG" 2>&1 &
PROXY_PID=$!
```

The dry run shows the operator, proxy vendor, model and command; it proves no execution or usage. Keep the ready event's `session` as `PROXY`. With an explicit store, pass the same `--database PATH` to every launcher and `comm` call. Send with outer `to` = proxy, data `to` = worker, parts exact and ordered:

```json
{"to":"PROXY_ID","parts":[{"data":{"to":"WORKER_ID","parts":[{"text":"Which check is still running?"}]}}],"reasoning":"Get worker status","replyRequired":true,"contextId":"build-status","key":"build-status-1"}
```

```sh
comm send_message - --session "$OPERATOR" --json < request.json   # receipt: {"id": RECEIPT_ID, ...}
python3 "$RELAY" wait --workspace "$REPO" --session "$OPERATOR" --receipt RECEIPT_ID
```

`wait` waits for the answer whose `replyTo` is the receipt's `id`, completes it and prints `reply` (verbatim parts), `outcome`, `sent` and `answer`. Exit 0 means `completed`, and exit 1 means another outcome or `{ok:false, error}`: `noReply` after `--timeout` seconds (default 600; the message stays pending), `bindingInactive` or `commFailed`. Reuse a key only for unchanged input. Silence, a timeout or queuing proves no handling. Stop the proxy with `kill "$PROXY_PID"`, then remove `"$RUN_LOG"` unless a failure needs it.

## Native, A2A and cloud

For a launcher failure, inspect the helper for the route that failed: [host detection](../scripts/hosts.py), [managed request and reply](../scripts/ask.py), [Claude direct send](../scripts/claude_send.py) or [A2A delivery](../scripts/a2a_tools.py). Run the relay launcher for normal use.

| Need | Run / check |
|---|---|
| Cross-vendor one-off send without a reply | `python3 "$RELAY" send --workspace "$REPO" --to codex:THREAD_UUID --session "$OPERATOR" --file message.txt` (optional `--reply-required --context ID --key KEY`). |
| Ongoing same-vendor native proxy | `python3 "$RELAY" start --native --worker-vendor "$VENDOR" --workspace "$REPO" --task "$TASK"`. A separate session, not a member of the operator's team; use the managed route for durable reply correlation. |
| Native discovery | Claude: `claude agents --cwd "$REPO" --json`, then `ListAgents`/`SendMessage`. Codex: `codex agents` is an interactive roster; `codex queue --thread "$THREAD_ID" --message "$TEXT"` queues to a verified UUID with that thread's model. Octocode: `agent` type `coordinate` `list`. |
| A2A peers with operator-supplied Agent Cards | Needs `uv` and the official `a2a` CLI: `python3 "$RELAY" start --card "worker=$CARD_URL" --once --deliver worker message.txt`. Repeat `--card` per peer; use `--task "$TASK"` instead of `--deliver` for conversational routing; omit `--once` for an interactive relay. Keep returned task/context IDs and artifacts; inspect the existing task after an uncertain send. |
| Explicitly requested Claude cloud proxy | `python3 "$RELAY" start --cloud --vendor claude --workspace "$REPO" --task "$TASK"`; the repository may be uploaded. Queue follow-ups with `claude -p --output-format json --cloud "$CLOUD_ID" "$FOLLOW_UP"`; inspect the session for the worker's reply. Codex cloud is not a relay interface here. |

Check the selected CLI's installed help before native or cloud commands. Keep local thread, cloud, Octocode and A2A task IDs distinct. Remote Control workers run on their original machine; use the account's remote tools for cross-machine discovery. Native queues and socket writes are receipts, not completion. Surface receiver permission or trusted-device holds; never bypass them. For protected A2A peers use the CLI's scoped credential helper; keep credentials out of messages and card URLs. Save a receipt only when requested, at the named destination or `<workspace>/.octocode/octocode-messages-proxy/`.
