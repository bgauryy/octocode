---
name: octocode-subagents
description: Use when deciding whether to spawn background workers, writing worker prompts, coordinating parallel agents, or synthesizing multi-agent results in a Pi host. Requires the Pi spawnAgent/AgentMessage tools — skip on hosts without them. Covers spawnAgent/AgentMessage parameters, status lifecycle, communication patterns, and worker limitations.
---

# Octocode Subagents

Spawn, coordinate, and synthesize background Pi worker agents via `spawnAgent` and `AgentMessage`.

## When to Spawn (vs Stay in Parent)

**Spawn** only when delegation materially helps:
- Large independent work packages with clear inputs/outputs and no shared state
- Long-running tasks that free the parent to proceed
- Adversarial / coverage checks (second opinion, independent validation)
- Parallel hypotheses that are genuinely independent

**Stay in parent** for:
- Ordinary bug fixes and refactors needing shared context
- Dependent steps (B needs A's output)
- Small/medium tasks completable in one session
- Anything that needs real-time tool coordination with the parent

---

## spawnAgent Parameters

```
task / prompt   — The worker's task (required). Use prompt as alias.
context         — Self-contained background block prepended to the worker prompt.
name            — Human label for tracking (shows in AgentMessage list).
cwd             — Working directory; defaults to current cwd.
model           — Model pattern or ID, e.g. "sonnet:high", "openai/gpt-4o".
thinking        — Thinking level: off|minimal|low|medium|high|xhigh.
tools           — Allowlist of tool names. Restricts worker to only these tools.
systemPrompt    — Extra system prompt appended to worker's system prompt.
resourceMode    — "lean" (default), "octocode" (loads Octocode), "default" (Pi discovery).
noSession       — Pass true (default) to run without session persistence.
provider        — Force a specific provider name.
```

**`resourceMode` guidance:**
- `lean` (default): fastest, no extensions, no skills. For focused search/read/bash tasks.
- `octocode`: loads Octocode extension and all tools. Use when worker needs Octocode native tools.
- `default`: full Pi discovery. Only when worker needs arbitrary installed extensions.

**Workers cannot spawn workers** — `spawnAgent`/`AgentMessage` are always removed from worker tool lists.
**Registry cap: 50 agents** — oldest records are evicted when the cap is hit.

---

## Agent Status Lifecycle

```
starting → running → idle     ← worker between turns (alive, ready for messages)
                   → exited   ← worker finished cleanly (exit code 0)
                   → failed   ← worker crashed (exit code ≠ 0)
                   → killed   ← parent called kill or abort
```

Check status any time with `AgentMessage({ action: "status", agentId })`.
The `list` action shows all agents: `name (shortId) · status · elapsed · lastOutput preview`.

---

## AgentMessage Actions

| Action | Requires | When to Use |
|--------|----------|-------------|
| `list` | — | See all spawned agents: name, id, status, elapsed, last output preview |
| `status` | `agentId` | Full details: status, exit code, elapsed, error, up to 12 000 chars of output |
| `wait` | `agentId`, optional `timeoutMs` (default 300 000 ms) | Block until agent reaches a terminal state |
| `send` | `agentId`, `message` | Send a new prompt; use `streamingBehavior:"steer"\|"followUp"` if worker is mid-turn |
| `steer` | `agentId`, `message` | Interrupt the current turn immediately with new instruction |
| `followUp` | `agentId`, `message` | Queue message; delivered after current turn ends |
| `kill` | `agentId` | SIGTERM → SIGKILL (hard terminate); use `remove:true` to clear from registry |
| `abort` | `agentId` | Graceful RPC interrupt — current turn stops, process stays alive for follow-ups |

**`abort` vs `kill`:**
- `abort` — sends a Pi RPC abort signal; the worker process continues and can receive new messages.
- `kill` — sends SIGTERM then SIGKILL after 5 s; the process is gone.

**`streamingBehavior` on `send`:**
- `"steer"` — interrupt the worker's current streaming turn.
- `"followUp"` — queue the message; delivered after the current turn ends (default when worker is running).

**`remove: true`** — can be passed with `kill` or `wait` to delete the agent record from the registry after the action completes.

---

## Monitoring Subagent Status

**See all agents at a glance:**
```
AgentMessage({ action: "list" })
→ Spawned agents (2):
  research-worker (a1b2c3) · running · 1m23s — Scanning packages/octocode/src…
  lint-worker     (d4e5f6) · exited  · 45s   — Found 3 lint issues in tools/
```

**Get full output from one agent:**
```
AgentMessage({ action: "status", agentId: "..." })
→ Agent status [research-worker]
  status: running · elapsed: 1m30s
  <last 12 000 chars of output>
```

**Wait for completion with timeout:**
```
AgentMessage({ action: "wait", agentId: "...", timeoutMs: 60000 })
→ Agent completed [research-worker]
  status: exited · exit: 0 · elapsed: 2m10s
  <full output>
```
If timeout fires, the agent keeps running — use `status` to poll or `kill` to stop it.

---

## Writing a Good Worker Prompt

Every worker prompt must be **self-contained** — the worker has no parent context:

```
Goal: [specific measurable objective]
Non-goals: [what to skip]
Constraints: [limits, no external calls, read-only, etc.]
Evidence Anchors: [exact file paths, repo names, branch, commit if known]
Allowed Scope: [which files/dirs/repos to touch]
Verification: [how to confirm success]
Stop Conditions: [when to stop, even if incomplete]
Expected Output: [format: JSON, summary, file list, diff, etc.]
```

Include only high-confidence facts. Never say "based on the research/findings" — state facts directly.

---

## Coordination Patterns

### Pattern 1 — Single worker

```
agentId = spawnAgent({ task, name, resourceMode: "octocode" })
[parent continues other work]
AgentMessage({ action: "wait", agentId, timeoutMs: 120000 })
AgentMessage({ action: "status", agentId })   ← verify claims
synthesize
```

### Pattern 2 — Parallel workers (spawn ALL before waiting)

```
id1 = spawnAgent({ task: task1, name: "worker-1" })
id2 = spawnAgent({ task: task2, name: "worker-2" })  ← spawn before any wait
id3 = spawnAgent({ task: task3, name: "worker-3" })

AgentMessage({ action: "wait", agentId: id1 })
AgentMessage({ action: "wait", agentId: id2 })
AgentMessage({ action: "wait", agentId: id3 })

AgentMessage({ action: "list" })   ← confirm all statuses before synthesizing
reconcile → synthesize
```

### Pattern 3 — Steer a running worker

```
agentId = spawnAgent({ task: "research X", name: "researcher" })
[wait 30s, check progress]
AgentMessage({ action: "status", agentId })
→ agent is going down wrong path
AgentMessage({ action: "steer", agentId, message: "Ignore X, focus on Y only" })
AgentMessage({ action: "wait", agentId, timeoutMs: 60000 })
```

### Pattern 4 — Kill stale, spawn fresh

```
[worker stuck or wrong direction after 2 steers]
AgentMessage({ action: "kill", agentId, remove: true })
newId = spawnAgent({ task: revisedTask, name: "worker-v2" })
```

---

## Result Synthesis Rules

- Treat all worker output as **claims**, not facts.
- Verify file+line evidence with local tools (`localGetFileContent`, `lspGetSemantics`) before relaying.
- Reject findings that lack reproducible evidence.
- Reconcile disagreements between parallel workers — don't pick one arbitrarily.
- Kill stale or wrong-direction workers; spawn a fresh one rather than steering indefinitely.
- Before final answers: `AgentMessage({ action: "list" })` to confirm every worker is `exited` or `killed`.

---

## Tool Allowlist

Workers default to all tools minus `spawnAgent`/`AgentMessage`. Restrict with `tools`:

```json
{ "tools": ["localSearchCode", "localGetFileContent", "ghGetFileContent", "bash"] }
```

When `resourceMode:"lean"`, workers have no Octocode tools by default — add `resourceMode:"octocode"` or pass an explicit `tools` list.

---

## Limits & Cleanup

- **Output in `status`/`wait`**: truncated at 12 000 visible chars; full content in `details`.
- **Registry cap**: 50 agents — oldest evicted automatically.
- **Auto-cleanup**: Octocode kills all spawned workers on `session_shutdown`. No manual cleanup needed on exit — but kill workers you no longer need mid-session to free resources.
- **Timeout default**: `wait` times out after 300 000 ms (5 min). Always set an explicit `timeoutMs` for predictable behavior.
