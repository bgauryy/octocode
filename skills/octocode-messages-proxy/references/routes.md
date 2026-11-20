# Other routes

Use these when `ask` does not fit: an ongoing relay, a native same-vendor proxy, A2A Agent Cards, a Claude cloud proxy, or a manual managed relay.

## Manual managed relay

All participants need live bindings in the same Octocode store and repository scope (linked worktrees share scope). Establish a missing worker binding from its own host (`host-config --vendor <label>`; see `octocode-agentic`). Use one package installation throughout; both `comm` and the launcher prefer an `octocode-agents-communication` on PATH over `npx`:

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
comm peers --session "$OPERATOR" --json
```

Set `OPERATOR` to your binding (or `comm join --name message-operator --vendor "$OPERATOR_VENDOR" --json`, kept alive with `listen` or `heartbeat`). Follow every emitted continuation. For an all-agent request, merge the native roster and store peers, deduplicate by confirmed vendor/native ID, and report unreachable peers. Set `WORKER` and `WORKER_VENDOR` from the peers; the proxy inherits no conversation, so the task supplies identities and scope:

```sh
TASK="Operator: $OPERATOR. Authorized worker: $WORKER. Relay only between these identities and return the worker's actual replies and outcomes."
python3 "$RELAY" start --worker-vendor "$WORKER_VENDOR" --workspace "$REPO" --task "$TASK" --dry-run
RUN_LOG=$(mktemp "${TMPDIR:-/tmp}/octocode-proxy.XXXXXX")
python3 "$RELAY" start --worker-vendor "$WORKER_VENDOR" --workspace "$REPO" --task "$TASK" >"$RUN_LOG" 2>&1 &
PROXY_PID=$!
```

The dry run shows the operator, proxy vendor, model and command; it proves no execution or usage. Keep the ready event's `session` as `PROXY`. With an explicit store, pass the same `--database PATH` to every launcher and `comm` call. Send with outer `to` = proxy, data `to` = worker, parts exact and ordered:

```json
{"to":"PROXY_ID","parts":[{"data":{"to":"WORKER_ID","parts":[{"text":"Which check is still running?"}]}}],"reasoning":"Get worker status","replyRequired":true,"contextId":"build-status","key":"build-status-1"}
```

```sh
comm send_message - --session "$OPERATOR" --json < request.json
comm inbox-wait --session "$OPERATOR" --json
```

Match the final answer's `replyTo` to the receipt's `id`, then `comm complete '{"message":ANSWER_ID}' --session "$OPERATOR" --json`. Reuse a key only for unchanged input. Silence, a timeout or queuing proves no handling. Stop the proxy with `kill "$PROXY_PID"`.

## Native, A2A and cloud

| Need | Run / check |
|---|---|
| Cross-vendor one-off send without a reply | `python3 "$RELAY" send --workspace "$REPO" --to codex:THREAD_UUID --session "$OPERATOR" --file message.txt` (optional `--reply-required --context ID --key KEY`). |
| Ongoing same-vendor native proxy | `python3 "$RELAY" start --native --worker-vendor "$VENDOR" --workspace "$REPO" --task "$TASK"`. A separate session, not a member of the operator's team; use the managed route for durable reply correlation. |
| Native discovery | Claude: `claude agents --cwd "$REPO" --json`, then `ListAgents`/`SendMessage`. Codex: `codex agents` is an interactive roster; `codex queue --thread "$THREAD_ID" --message "$TEXT"` queues to a verified UUID with that thread's model. Octocode: `agent` type `coordinate` `list`. |
| A2A peers with operator-supplied Agent Cards | Needs `uv` and the official `a2a` CLI: `python3 "$RELAY" start --card "worker=$CARD_URL" --once --deliver worker message.txt`. Repeat `--card` per peer; use `--task "$TASK"` instead of `--deliver` for conversational routing; omit `--once` for an interactive relay. Keep returned task/context IDs and artifacts; inspect the existing task after an uncertain send. |
| Explicitly requested Claude cloud proxy | `python3 "$RELAY" start --cloud --vendor claude --workspace "$REPO" --task "$TASK"`; the repository may be uploaded. Queue follow-ups with `claude -p --output-format json --cloud "$CLOUD_ID" "$FOLLOW_UP"`; inspect the session for the worker's reply. Codex cloud is not a relay interface here. |

Check the selected CLI's installed help before native or cloud commands. Keep local thread, cloud, Octocode and A2A task IDs distinct. Remote Control workers run on their original machine; use the account's remote tools for cross-machine discovery. Native queues and socket writes are receipts, not completion. Surface receiver permission or trusted-device holds; never bypass them. For protected A2A peers use the CLI's scoped credential helper; keep credentials out of messages and card URLs. Save a receipt only when requested, at the named destination or `<workspace>/.octocode/octocode-messages-proxy/`.
