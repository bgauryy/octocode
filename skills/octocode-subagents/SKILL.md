---
name: octocode-subagents
description: Use when deciding whether to spawn background workers, writing worker prompts, coordinating parallel agents with AgentMessage, or synthesizing multi-agent results. Covers spawnAgent/AgentMessage parameters, task decomposition, result claims, and worker limitations.
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

## spawnAgent Parameters

```
task / prompt   — The worker's task (required if no task set). Use prompt as alias.
context         — Self-contained background block prepended to the worker prompt.
name            — Human label for tracking (shows in AgentMessage list).
cwd             — Working directory; defaults to current cwd.
model           — Model pattern or ID, e.g. "sonnet:high", "openai/gpt-4o".
thinking        — Thinking level: off|minimal|low|medium|high|xhigh.
tools           — Allowlist of tool names. Restricts worker to only these tools.
systemPrompt    — Extra system prompt appended to worker's system prompt.
resourceMode    — "lean" (default, no extensions/skills), "octocode" (loads Octocode), "default" (Pi discovery).
noContextFiles  — Pass true to skip AGENTS.md loading in the worker.
noSession       — Pass true (default) to run without session persistence.
provider        — Force a specific provider name.
```

**resourceMode guidance:**
- `lean` (default): fastest, no extensions, no skills. Use for focused search/read/bash tasks.
- `octocode`: loads Octocode extension and all tools. Use when worker needs Octocode tools.
- `default`: full Pi discovery. Use only when worker needs arbitrary installed extensions.

**Workers cannot spawn workers** — `spawnAgent`/`AgentMessage` are always removed from worker tool lists.

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
Expected Output: [format: JSON, summary, file list, etc.]
```

Include only high-confidence facts. Never say "based on the research/findings" — state facts directly.

## AgentMessage Actions

| Action | When to Use |
|--------|-------------|
| `list` | See all spawned agents (id, name, status, elapsed) |
| `status` | Get details on one agent (last output, exit code, events) |
| `wait` | Block until agent completes (use `timeoutMs` to avoid hanging) |
| `send` | Send a follow-up message (queued if agent is idle) |
| `steer` | Interrupt current turn with new instruction |
| `followUp` | Queue a message to deliver after agent's current turn ends |
| `kill` | Force-terminate an agent process |
| `abort` | Graceful interrupt (Pi RPC abort, doesn't kill process) |

**Pattern: Spawn → Work → Wait → Synthesize**
```
1. spawnAgent({ task, context, name, ... })           → agentId
2. [continue parent work if any]
3. AgentMessage({ action: "wait", agentId, timeoutMs: 120000 })
4. AgentMessage({ action: "status", agentId })        → claims
5. Verify claims; reject unsupported findings
6. Synthesize into parent context
```

**Pattern: Multiple parallel workers**
```
1. spawnAgent(worker1) → id1
2. spawnAgent(worker2) → id2        ← spawn all before waiting
3. AgentMessage("wait", id1)
4. AgentMessage("wait", id2)
5. Reconcile; reject contradictions; synthesize
```

## Result Synthesis Rules

- Treat all worker output as **claims**, not facts.
- Verify artifacts with local tools before relaying success.
- Reject findings that lack file+line evidence or aren't reproducible.
- Reconcile disagreements between parallel workers; don't pick one arbitrarily.
- Kill stale or wrong-direction workers with `AgentMessage({ action: "kill" })`.
- Spawn a fresh worker after a wrong approach rather than steering indefinitely.

## Tool Allowlist

Workers default to all tools minus `spawnAgent`/`AgentMessage`. Restrict with `tools`:

```json
{ "tools": ["localSearchCode", "localGetFileContent", "ghGetFileContent"] }
```

When `resourceMode: "lean"`, workers have no Octocode tools unless you pass `tools` explicitly **and** set `resourceMode: "octocode"`.

## Cleanup

Octocode automatically kills all spawned workers on `session_shutdown` (new session, reload, quit). You don't need to manually clean up on exit — but kill workers you no longer need mid-session to free resources.
