# Native and cloud sessions

First match the worker to its repository and live session ID. A native session becomes a local Octocode peer after it has a shared-store binding. Existing sessions keep their identity. A local thread ID, a Claude cloud ID, an Octocode database ID and an A2A task ID address different resources.

**Vendor-native relay.** For an ongoing same-vendor assignment, start `python3 "$RELAY" start --native --vendor claude --task "$TASK"` or select `--vendor codex`. The launcher opens that vendor's small-model CLI. In a Claude session, send as the current Claude agent. In a Codex team, use the communication tools actually exposed by the host for that team, such as `list_agents`, `send_message` and `followup_task` when available. Independent Codex threads use the CLI queue. A queue receipt alone does not confirm a worker read the message.

**Cross-vendor delivery.** Existing Claude and Codex agents each keep one Octocode identity on the same store and repository scope. The sender calls `send_message` once. The recipient’s existing host binding delivers it and the recipient completes it through Octocode. For a native listener, the Claude adapter uses the session's messaging socket and the Codex adapter uses its owning app server. The recipient owns its attachment. Inspect `/cli skill --json` for the installed package's `referenceRoot` and read `HOST_SETUP.md` there for hook or native attachment setup. Use `health` for uncertain delivery. A native socket write, queued prompt or successful API submission establishes only the corresponding transport receipt.

**Claude.** Run `claude agents --cwd "$REPO" --json` to find local sessions. A session connected by Remote Control still runs on its original machine. For a local Claude-only proxy, launch `claude --model haiku --name octocode-messages-proxy --bg "$TASK"`. In its task, identify the authorized worker, instruct it to use `ListAgents` and `SendMessage`, and ask it to return the worker's actual reply. Keep the ID printed by `--bg` for `claude attach` or `claude logs`. Cross-machine visibility depends on the account, connection and receiver settings. [Claude cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging)

For a local shell send, the [Claude adapter](../scripts/claude_send.py) resolves the CLI session UUID to a unique local name. Claude's peer discovery uses its own address for that name. A one-run hook checks the destination and exact text before dispatch. The command returns the native message receipt. Consult the recipient transcript for handling.

For a requested cloud-hosted proxy use the launcher from a checkout whose contents the user intends to send.

```sh
python3 "$RELAY" start --cloud --vendor claude --workspace "$REPO" --task "$TASK"
claude -p --output-format json --cloud "$RELAY_CLOUD_ID" "$FOLLOW_UP"
```

Cloud creation may upload the local repository when GitHub cloning is unavailable. A clean checkout of the published branch suffices for message-only work. Record the cloud ID and verify the selected model in the session. The follow-up command queues a message. Read the relay transcript for the actual outbound call and recipient reply. For a Remote Control worker, the cloud account's session tools may find the target even when local peer discovery is empty. If the account requires a trusted-device action, retain the pending request and surface the exact permission to the owner. An explicitly authorized operator can carry the message through the signed-in CLI and verify the reply in the relay. Identify who performed that hop. [Claude cloud CLI](https://code.claude.com/docs/en/claude-code-on-the-web)

**Codex.** `codex agents` browses sessions on the shared local daemon. For an authorized message to a known native thread, use `codex queue --thread "$THREAD_ID" --message "$TEXT"`. Its success means queued. Confirm receipt or a reply in that thread. For a persistent cross-provider relay use the launcher's local route and the shared communication binding. Inspect `codex cloud exec --help` before attempting hosted execution: the CLI requires an environment and its task-submission interface does not specify a relay model or provide an existing-task messaging inbox. Report that limit for a Codex-cloud-only request. [Codex CLI reference](https://developers.openai.com/codex/cli/reference/)
