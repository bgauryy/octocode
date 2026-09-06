# Subagents and agent communication

The extension exposes one model-callable `agent` facade for spawning workers and controlling their lifecycle. Workers have no parent conversation context, so every spawn query needs a self-contained task packet.

## Spawn profiles

| Profile | Resources | Default mode | Use |
|---|---|---|---|
| `researcher` | `web`, `MCPTool`, installed Octocode skills | typed Octocode | Evidence gathering, prior art, and package or repository lookup. |
| `planner` | `web`, `MCPTool`, installed Octocode skills | typed Octocode | Dependency-ordered plans, risks, verification strategy, and RFC handoffs. |
| `architect` | `bash`, `web`, `MCPTool`, installed Octocode skills | typed Octocode | Root-cause and architecture analysis with targeted debug or test loops. |
| `browser` | Chrome DevTools specialist prompt and tools | typed browser | Multi-turn security, network, DOM, coverage, worker, or emulation workflows. |
| `custom` | Explicit `tools` and `systemPrompt` | `resourceMode:"lean"` | A clean bounded worker with only the resources the parent provides. |

Typed profiles use their packaged system prompts and tool sets. The custom profile accepts `resourceMode:"lean"|"octocode"|"default"`. Pass `model`, `provider`, and `thinking` when the task needs an override; resolve live model identifiers with `pi -ne --list-models`.

Workers never receive the `agent` facade, so they can't spawn or control sub-workers recursively. A spawn returns an `agentId`; use it in a later lifecycle query. Spawn queries and lifecycle queries with explicit IDs can't share a batch because generated IDs aren't available during preflight.

## Delegate a plan task

For work tracked by the parent plan, pass `planStep` with the exact stable task ID from the `Task IDs for agent.planStep` line in a `plan` result. The active plan context also includes `task-id` values. Display indices and task labels aren't valid `planStep` values. Omit `planStep` for independent work that has no parent plan assignment.

1. Start the accepted plan and any runnable task you intend to delegate. Independent tasks can run in parallel after their dependencies finish.
2. Call `plan` with `action:"show"` and copy the task's stable ID.
3. Spawn a worker with that ID as `planStep` and a bounded assignment. The plan must be executing, the task must be `doing`, every dependency must be `done`, and no interaction can remain pending. These checks use the canonical plan, including shared task statuses.
4. Collect the result with `agent` using `type:"wait"` or `type:"inspect"`. Check its evidence and verification before completing the parent task.

The worker receives the plan ID, task ID, task text, declared paths, acceptance criteria, and check command. The extension rechecks the plan and task after asynchronous spawn preparation, before creating the process. It rejects an assignment context longer than 12,000 characters; narrow the task contract before retrying.

A task can have one live assigned worker within the same parent session and plan. An idle worker retains the assignment because its process can accept another turn. Send follow-up work to that worker, or collect its result and kill it before assigning a replacement. Workers from a different session or plan don't conflict. A worker's completion report doesn't complete the parent task; the parent verifies the result and updates the plan.

## Live parent-worker control

The `agent` query `type` selects an operation:

| Type | Use |
|---|---|
| `spawn` | Start one typed, browser, or custom worker. |
| `inspect` | List workers without `agentId`, or inspect one worker with it. |
| `wait` | Wait for the current turn and return the retained output/history. |
| `message` | Start an idle turn or queue a follow-up through `delivery:"send"|"followUp"`. |
| `steer` | Redirect a running turn after its current tool call. |
| `abort` | Interrupt the active turn gracefully; keep the process alive. |
| `kill` | Terminate the process and optionally remove its record. |

Worker-to-parent results are pull-based: inspect or wait for `[DONE]`, `[BLOCKED]`, or `[FAILED]` markers. Workers can't steer, abort, or kill each other. Kill a worker after collecting its final receipt unless another turn is planned; idle workers still hold a process until killed or session shutdown.

The `wait` operation's `timeoutMs` sets a silence window. An active worker can keep the wait open beyond that window; a quiet worker can return a status snapshot before its turn finishes. Check the returned status before treating the result as complete.

Cancelling a `wait` call releases its timers, listeners, and liveness probes. It preserves the worker and other waits, including when the cancelled call requested `remove:true`. Use `type:"abort"` to interrupt the worker's turn or `type:"kill"` to terminate its process. Cancelling spawn preparation prevents subsequent process creation; a worker whose spawn already returned keeps running until explicitly stopped or the session shuts down.

Use `task` for the worker assignment and the `agent` tool for every profile and lifecycle operation. There are no separate spawn or message tool aliases.

The `/octocode-agents` command accepts `help`, `list`, `inspect <id> [full]`, `kill <id>`, `kill-all`, `prune`, and `hide`. Omit the command verb to list workers. Use `list` to restore the footer after `hide`; use `inspect <id> full` for the retained detailed output. Other command spellings return usage guidance.

## Durable peer communication

Awareness `message` and `handoff` are a separate, asynchronous plane shared by agents on any host. Directed messages use `--to`; broadcasts omit a target. Workspace scope isolates plans, tasks, locks, work presence, handoffs, and messages.

```bash
npx -p @octocodeai/octocode-awareness octocode-awareness message send \
  --workspace "$PWD" --from A --to B --topic "<topic>" --text "<message>"
npx -p @octocodeai/octocode-awareness octocode-awareness message inbox \
  --workspace "$PWD" --agent-id B
```

Peer messages are pull-based, not real-time. Use the `agent` facade for urgent parent-worker control and Awareness messages for coordination that must survive a worker turn or cross host boundaries.

## Isolation

Use `profile:"custom"` with `resourceMode:"lean"` for a parent-only worker that shouldn't join the Awareness peer bus. Add only the tool paths and resources it needs. Use typed or `resourceMode:"octocode"` profiles when the worker must coordinate through the same Awareness workspace.

## Example

```text
agent({queries:[{
  reasoning:"Delegate an independent evidence-gathering lane.",
  type:"spawn",
  profile:"researcher",
  task:"Goal: …\nContext: …\nScope: …\nOwnership: read-only …\nAcceptance: …\nReturn: …"
}]})
→ agentId: "abc123"

agent({queries:[{reasoning:"Collect the worker turn.",type:"wait",agentId:"abc123",timeoutMs:60000}]})
agent({queries:[{reasoning:"Free the completed worker process.",type:"kill",agentId:"abc123",remove:true}]})
```
