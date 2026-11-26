Load when a worker has no store binding, or when `ask` does not fit: a cross-vendor send without a reply, a native same-vendor proxy, A2A Agent Cards or a Claude cloud proxy. An ongoing relay by hand is the [managed relay](managed-relay.md).

# Other routes

## Bind a missing worker

A worker with no store binding is unreachable until it binds from its own host; the operator cannot bind it.

| Worker host | Bind |
|---|---|
| Cursor or Grok | `octocode-agents-communication /cli host-config --json --vendor cursor` (or `grok`) `--workspace "$REPO"` prints the hooks. Merge them into `.cursor/hooks.json` or `.grok/hooks/communication.json` and reload the host; Grok also needs a trusted Git project root (`grok inspect --json`). |
| Pi | Load the package's `pi-inbox` extension (`registerPiInbox`). Only when a new worker is requested, `run --vendor pi` starts one already bound. |

Version-specific steps: `HOST_SETUP.md` under the `referenceRoot` that `octocode-agents-communication /cli skill --json` returns. The worker is bound once `python3 "$RELAY" peers --workspace "$REPO"` lists it under `store`; then `ask`.

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
