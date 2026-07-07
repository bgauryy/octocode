# Coordination

Use this when writing worker prompts or coordinating multiple workers.

## Worker Prompt

Every worker prompt needs enough context to run without parent memory:

```text
Goal: specific measurable objective
Non-goals: what to skip
Constraints: limits, read-only mode, no external calls, etc.
Evidence Anchors: exact files, repos, branches, or commits
Allowed Scope: files, dirs, repos, or tools
Verification: how success is checked
Stop Conditions: when to stop even if incomplete
Expected Output: JSON, summary, file list, diff, or findings table
```

State facts directly. Avoid phrases like "based on the research" when the worker cannot see that research.

## Single Worker

```text
agentId = spawnAgent({ task, name, resourceMode: "octocode" })
parent continues other work
AgentMessage({ action: "wait", agentId, timeoutMs: 120000 })
AgentMessage({ action: "status", agentId })
synthesize
```

## Parallel Workers

Spawn all independent workers before waiting:

```text
id1 = spawnAgent({ task: task1, name: "worker-1" })
id2 = spawnAgent({ task: task2, name: "worker-2" })
id3 = spawnAgent({ task: task3, name: "worker-3" })

AgentMessage({ action: "wait", agentId: id1 })
AgentMessage({ action: "wait", agentId: id2 })
AgentMessage({ action: "wait", agentId: id3 })
AgentMessage({ action: "list" })
```

## Steering

Use `steer` when a running worker is on a recoverable wrong path.
Use `followUp` when the worker should finish its current turn first.
If two steer attempts fail, kill the worker and spawn a fresh one with a corrected task.
