# Subagents and agent communication

The extension exposes one model-callable `agent` facade for spawning workers and controlling their lifecycle. Workers have no parent conversation context, so every spawn query needs a self-contained task packet.

## Spawn profiles

| Profile | Resources | Default mode | Use |
|---|---|---|---|
| `researcher` | `web`, `MCPTool`, `file`, `skill`, `awareness`, `bash`, enabled skills | typed Octocode | Evidence gathering, prior art, and package or repository lookup. |
| `planner` | `web`, `MCPTool`, `file`, `skill`, `awareness`, `bash`, enabled skills | typed Octocode | Dependency-ordered plans, risks, verification strategy, and RFC handoffs. |
| `architect` | `bash`, `web`, `MCPTool`, `file`, `skill`, `awareness`, enabled skills | typed Octocode | Root-cause and architecture analysis with targeted debug or test loops. |
| `implementer` | `bash`, `MCPTool`, `file`, `skill`, `awareness`, enabled skills | typed Octocode | One bounded source change under exclusive ownership, with an observed acceptance check. |
| `reviewer` | `MCPTool`, `skill`, `awareness`, enabled skills | typed Octocode | Read-only verification of acceptance criteria with a PASS, WARN, or FAIL verdict. |
| `browser` | `chromeDebug`, `MCPTool`, `skill`, `awareness`, `bash`, enabled skills | typed Octocode | Multi-turn security, network, DOM, coverage, worker, or emulation workflows. |
| `custom` | caller-selected least-capability tools and explicit role prompt | explicit | A role not covered above; never an implicit catch-all. |

Typed profiles use packaged prompts and role-bounded tool sets. `custom` requires both a non-empty `systemPrompt` and an explicit `tools` list; the shared bounded-worker contract is always prepended. `tools:[]` becomes Pi's `--no-tools`. `resourceMode:"lean"` disables extension and skill loading; use broader modes only when the explicit tool list requires them. Pass `model`, `provider`, and `thinking` only when the task needs an override; resolve live model identifiers with `pi -ne --list-models`.

Workers never receive the `agent` facade, and worker-process registration omits the tool and skill smith surfaces, so workers can't spawn sub-workers recursively. A spawn returns an `agentId`; use it in a later lifecycle query. Spawn queries and lifecycle queries with explicit IDs can't share a batch because generated IDs aren't available during preflight.

Use `cohortId` to group workers that contribute to the same bounded outcome. An `inspect`
result includes attention-first cohort counts without copying task text or worker output.
At 80% of the configured step budget, the worker status warns that wrap-up should begin.
At the hard budget, it requests an explicit partial handback with evidence, verification,
confidence, and a next action instead of implying completion.

## Delegate a plan task

Isolated delegation needs no plan. Use plan tracking only for complex dependencies,
coordinated ownership, consequential risk, substantial work spanning sessions, or
an explicit planning request.

For independent work, omit `planStep`. For work tracked by the parent plan, pass `planStep` with the exact stable task ID from the `Task IDs for agent.planStep` line in a `plan` result. The active plan context also includes `task-id` values. Display indices and task labels aren't valid `planStep` values.

1. Start the reviewed revision and each runnable task you intend to delegate. The one Start decision authorizes the displayed revision and begins execution atomically. Independent tasks can run in parallel after their dependencies finish.
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

For consequential work, spawn `profile:"reviewer"` after the implementation evidence is
available. The reviewer has no `file`, `bash`, or browser-control capability and returns
one advisory PASS, WARN, or FAIL verdict. It cannot edit the workspace or complete the
parent plan.

The `wait` operation's `timeoutMs` sets a silence window. An active worker can keep the wait open beyond that window; a quiet worker can return a status snapshot before its turn finishes. Check the returned status before treating the result as complete.

### Handback best practice

After `wait` or `inspect` returns a result, the parent owns this sequence:

1. Verify each load-bearing finding against the cited source, semantic result, or observed check. A worker's confidence marker is not verification.
2. Reconcile verified findings with the active hypothesis, plan, risk, and next action. Do not copy the raw handback or treat worker confidence as persistence authority.
3. Persist a bounded session finding only when later recovery needs it, and anchor it to evidence the parent observed directly. Record reusable Awareness memory only when the normal memory policy independently requires it.
4. Kill the worker unless another bounded turn is intentional, then continue the user request.

Cancelling a `wait` call releases its timers, listeners, and liveness probes. It preserves the worker and other waits, including when the cancelled call requested `remove:true`. Use `type:"abort"` to interrupt the worker's turn or `type:"kill"` to terminate its process. Cancelling spawn preparation prevents subsequent process creation; a worker whose spawn already returned keeps running until explicitly stopped or the session shuts down.

Use `task` for the worker assignment and the `agent` tool for every profile and lifecycle operation. There are no separate spawn or message tool aliases.

Open `/octocode-inbox`, pick a worker, then choose **View output**, **Steer**, or **Stop** when the process supports that action. The output inspector preserves every retained line and supports scrolling. Escape closes the current view.

### Worker states and footer updates

| State | Meaning |
|---|---|
| `starting` | Process created and initial prompt sent; no `agent_start` observed yet. |
| `running` | A turn started and has not ended, including an explicit compaction retry. |
| `queued` | A follow-up awaits its next `agent_start`; the previous turn has ended. |
| `idle` | Turn ended without a structured terminal handback; process can accept work. |
| `done` | Completed handback or clean process exit; parent verification is separate. |
| `blocked` | Worker reported unresolved work. A clean exit does not erase the blocker. |
| `failed` | Process failure or a failed handback after the turn ended. |
| `killed` | Explicit process termination; overrides prior handback text. |

Fresh running or queued work supersedes an old handback. A dead process cannot
receive follow-ups even when its retained outcome is blocked. The inbox gates
actions on process liveness and keeps blocked elapsed time fixed at the last
observation.

The footer shows a bounded, stable list of live worker names, states, and updates.
A running tool takes precedence over an old message; otherwise the latest output
or message provides the update. Attention comes first, and the inbox retains the
full roster and settled results. See [the UI contract](UI.md).

## Durable peer communication

Awareness `signal` and `handoff` persist cross-host coordination in the shared ledger. Cooperating agents need the same physical database and distinct stable agent IDs. Each uses its physical checkout; linked Git worktrees share peers, signals and memory while work and verification stay local to the checkout. Pi workers use the native `awareness` tool with host-supplied context; external CLI hosts preserve equivalent store bindings. Names and vendor labels are self-reported metadata, not routing IDs or authority.

```bash
npx @octocodeai/octocode-awareness signal publish \
  --db "$AWARENESS_DB" --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" \
  --to-agent "$PEER_AGENT_ID" --kind question --subject "<summary>" --body "<request>"
npx @octocodeai/octocode-awareness signal list \
  --db "$AWARENESS_DB" --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --include-bodies
```

Use `signal reply --in-reply-to <signal-id>` for the existing thread, acknowledge handled rows, and resolve only when no response or work remains. Follow returned executable continuations. These commands inspect and record coordination; they do not start a peer turn. Use the `agent` facade for urgent parent-worker control. Reuse native lifecycle records and run the closing `verify audit` against your own identity in the same store.

## Isolation

Use `profile:"custom"` with `resourceMode:"lean"` and `tools:[]` for a parent-only tool-less worker that shouldn't join the Awareness peer bus. Add only the resources it needs. Use typed profiles or the default custom mode when the worker must research through Octocode and coordinate through the same Awareness workspace.

## Example

```text
agent({queries:[{
  reasoning:"Delegate an independent evidence-gathering lane.",
  type:"spawn",
  profile:"researcher",
  goal:"Identify the exact caller and contract.",
  context:"The parent observed …",
  scope:"Read-only evidence for package X.",
  ownership:"No writes; inspect package X only.",
  acceptance:"Return exact source and semantic anchors.",
  returnShape:"Findings, evidence, confidence, and remaining gap."
}]})
→ agentId: "abc123"

agent({queries:[{reasoning:"Collect the worker turn.",type:"wait",agentId:"abc123",timeoutMs:60000}]})
agent({queries:[{reasoning:"Free the completed worker process.",type:"kill",agentId:"abc123",remove:true}]})
```
