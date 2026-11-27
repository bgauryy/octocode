Load when running an ongoing relay by hand through the shared Octocode store: start a proxy, send through it, wait for the reply and stop it.

# Managed relay by hand

All participants need live bindings in the same Octocode store and repository scope (linked worktrees share scope); [bind a missing worker](routes.md#bind-a-missing-worker) first. Use one package installation throughout; both `comm` and the launcher prefer an `octocode-agents-communication` on PATH over `npx`:

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
