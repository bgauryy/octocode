# Subagent API

Use this when setting `spawnAgent` or `AgentMessage` parameters.

## spawnAgent

| Parameter | Meaning |
|---|---|
| `task` / `prompt` | Worker task; `prompt` is an alias. |
| `context` | Self-contained background block prepended to the worker prompt. |
| `name` | Human label for status/list output. |
| `cwd` | Working directory; defaults to current cwd. |
| `model` | Model pattern or id, such as `sonnet:high`. |
| `thinking` | `off`, `minimal`, `low`, `medium`, `high`, or `xhigh`. |
| `tools` | Tool allowlist for the worker. |
| `systemPrompt` | Extra system prompt appended to the worker prompt. |
| `resourceMode` | `lean`, `octocode`, or `default`. |
| `noSession` | Run without session persistence; defaults true. |
| `provider` | Force a provider name. |

## Status Lifecycle

```text
starting -> running -> idle
                   -> exited
                   -> failed
                   -> killed
```

`idle` means the worker is alive and ready for messages. `exited`, `failed`, and `killed` are terminal.

## AgentMessage

| Action | Requires | Use |
|---|---|---|
| `list` | none | See all spawned agents. |
| `status` | `agentId` | Read state, elapsed time, errors, and recent output. |
| `wait` | `agentId` | Block until terminal state or timeout. |
| `send` | `agentId`, `message` | Send a new prompt. |
| `steer` | `agentId`, `message` | Interrupt the current turn with new direction. |
| `followUp` | `agentId`, `message` | Queue a message after the current turn. |
| `kill` | `agentId` | Terminate the process. |
| `abort` | `agentId` | Interrupt the turn but keep the process alive. |

Use `remove:true` with `kill` or `wait` to delete the registry record after completion.
