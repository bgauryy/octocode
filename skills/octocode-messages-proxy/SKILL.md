---
name: octocode-messages-proxy
description: "Use when asked to relay messages between agents or create a small Claude or Codex CLI proxy. Use each vendor's native messaging for its sessions, Octocode for cross-vendor repository coordination, and A2A for Agent Card peers."
---

# Octocode messages proxy

Run the [launcher](scripts/relay.py) from the operator's shell. Resolve its path against this skill's folder. Keep the working directory set to the target repository. Start with `python3 "$RELAY" detect`. It reports the executables on PATH without opening a session. Python 3.10+ is required.

Identify the intended peers, checkout and message. Reuse a live proxy for that scope when its identity and route are known. For an all-agent coordination request, discover the full roster, including linked worktrees if in scope. Merge identities from the local service, the relevant native session list and any supplied Agent Cards. Deduplicate by confirmed native ID before sending. Report peers outside the reachable transports. A provider CLI on PATH establishes installation only.

| Who is exchanging the message | First choice |
|---|---|
| Two agents in the current vendor team | Use the active host’s team discovery, send and follow-up tools. Keep the sender’s existing identity. |
| Independent sessions of the same vendor | Claude uses `ListAgents` and `SendMessage`. Codex uses `codex agents` and `codex queue`. From a shell use the direct send below. See [native session scopes and receipts](references/native-sessions.md). |
| Claude and Codex in a shared repository, or a handoff that needs a durable correlated reply | Use Octocode’s shared peer registry, inbox, idempotency keys and completion records. Send directly from the operator’s bound identity. Start a managed worker only for an ongoing relay assignment. |
| A2A agents with known cards | Supply one `--card name=URL` per peer. The small model gets discovery, text messaging and task lookup through the official A2A client. |
| Claude cloud session | Use the account's remote session tools or queue a follow-up to its verified cloud ID. See [native and cloud sessions](references/native-sessions.md). |

For a mixed roster with shared bindings, coordinate the whole roster through Octocode. Reconcile native listings by `(vendor, vendorSession)` and use each matching database peer once. Use a passive broadcast only when the whole live snapshot is intended. For an unregistered worker, use its own hook or listener to establish its Octocode binding. Report any unreachable peer. An uncertain delivery stays on its original route for reconciliation.

## Direct send

For a known cross-vendor recipient, one command sends from the existing binding. The target may be its Octocode ID or its exact native ID recorded in the peer registry. The other vendor's CLI and a relay-model turn are unnecessary on the sender's machine.

```sh
python3 "$RELAY" send --workspace "$REPO" --session "$MY_OCTOCODE_ID" \
  --to "claude:$WORKER_ID" --file ./message.txt --reply-required \
  --context build-check --key build-check-1
```

Read the sender’s inbox and match `replyTo` to `receipt.id`. Reuse the key only with the same destination and payload. `--database` selects a shared store. The application’s native receiver handles wake-up and permissions while Octocode owns the delivery record. Neither party needs to adopt the other’s CLI.

For an independent Codex thread, use `python3 "$RELAY" send --from codex --to "codex:$THREAD_ID" --file ./message.txt`. The CLI returns a queue ID. For Claude, use the current session's `SendMessage` when available. The equivalent shell invocation (`--from claude --to "claude:$SESSION_ID"`) launches Haiku for local peer messaging or uses the account queue for a Claude cloud ID. The local Haiku process is the native sender. Supplying `--session` selects Octocode even within one vendor when a tracked reply is needed. Omit `--file` to read UTF-8 from stdin.

## Local relay

```sh
python3 "$RELAY" start --workspace "$REPO" --task \
  'Coordinate with every live repository peer. Send one passive introduction. Relay my follow-ups to the peers I name and return their replies.'
```

Auto selects signed-in Claude with `haiku`, then signed-in Codex with the first visible Luna, Mini or Nano model in its catalog. Set `--vendor codex` or `--vendor claude` to choose. Use `--model ID` with an explicit vendor when the account uses a different small model. For the local route, the launcher uses the installed `octocode-agents-communication` executable or invokes the published package with `npx -y`. Use that package's supported Node version. Pass `--database PATH` only when peers already use that store or for an isolated check.

Keep the launcher alive under the invoking host. Capture the `session` field of its `ready` event. The host owns presence and wakes the model for incoming requests. One relay can handle several peers. Read the operator's bound inbox at task boundaries and use a passive notice for introductions. Ask for a reply only when the recipient owes one.

To send through the relay, use the operator's own bound messaging tools or define the shell function below. Use the same package installation, workspace and database on every call. If the relay uses a custom database, add `--database "$DB"` after `"$@"` inside the function. Reuse the operator's binding. If none exists, run `relay_comm join --vendor generic --name relay-operator --json` and take its `id`. Use `listen` or `heartbeat` to keep a raw operator present during longer exchanges.

```sh
relay_comm() (cd "$REPO" && npx -y @octocodeai/octocode-agents-communication /cli "$@")
relay_comm schema send_message --json
relay_comm peers --session "$OPERATOR" --json
relay_comm send_message - --session "$OPERATOR" --json < request.json
relay_comm inbox-wait --session "$OPERATOR" --json
```

For `request.json` substitute both discovered IDs. Use an Octocode data part containing the downstream recipient and ordered parts when the wording must arrive exactly. The outer `to` addresses the relay, and `replyRequired` requests the worker’s actual answer.

```json
{"to":"RELAY_ID","parts":[{"data":{"to":"BUILD_WORKER_ID","parts":[{"text":"Which check is still running?"}]}}],"reasoning":"Get build status","replyRequired":true,"contextId":"build-status","key":"build-status-1"}
```

If the installed send schema requires `body`, put the forwarding instruction and quoted text in that string and use `conversationId` for correlation. That profile exposes `notify_all` for a scoped introduction and `complete({message: ID, reply: "answer"})` for a request. Follow every emitted page. Match the final answer's `replyTo` to the original send receipt's `id`. Silently complete handled FYIs and final replies. Keep a pending request open across model turns. Inspect delivery history before retrying an uncertain send. Stop the managed launcher by its own process ID when the relay's assignment ends.

## A2A relay

```sh
python3 "$RELAY" start --vendor claude --card build="$BUILD_CARD" \
  --card review="$REVIEW_CARD" --once --deliver build ./message.txt
```

This discovers both cards and exposes one delivery operation bound to `build` and the exact UTF-8 file contents. Repeating the operation in the same tool session returns the original result. For conversational routing use `--task 'Ask build which check is still running and quote the answer.'` in place of `--deliver`. Omit `--once` to keep the native CLI conversation interactive.

This route requires `uv` and the official `a2a` executable on PATH. Use a URL supplied by the operator or an approved directory for each card. The [session-scoped tools](scripts/a2a_tools.py) resolve cards and delegate transport negotiation to the A2A CLI. Use the CLI's scoped credential helper for a protected peer. Keep credentials out of prompts and card URLs. [Official A2A CLI and installation](https://github.com/a2aproject/a2a-cli)

A direct A2A Message can carry the reply. A Task can return artifacts, request input, or remain pending. Keep its task and context IDs with the peer. Read the existing task to check status. Continue an interrupted task with those IDs and the requested input. If a failed send gives only a message ID, inspect the peer’s task list or history for that ID. Report delivery as uncertain if it cannot be located. Relay text and preserve returned artifacts. For a caller that already supplies an exact multipart SendMessageRequest and needs no model decision, use `a2a send -a "$CARD" --request-payload "$REQUEST_FILE" -o json` directly. Octocode's shared SQLite inbox uses its own local protocol. The A2A route speaks the wire protocol advertised in the card. [A2A specification](https://a2a-protocol.org/latest/specification/)

Return the [delivery result](output.md). Use `octocode-agentic` for installing host bindings or repairing delivery for an existing local agent.
