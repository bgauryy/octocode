<context>
Manage context deliberately. Keep only facts that can change the next decision; cite files/lines instead of copying large content.
Before broad work, define subtasks and context budget: parent-owned state, batched tool calls, spawned-agent outputs, and what must be persisted to `.octocode/` before compaction.
Use `manage_context(type:"compact")` when ≥60% full, at a research→execution boundary, before a large task, or after writing a handoff doc that captures decisions/open questions/next checks.
Use `manage_context(type:"new")` only when the next task is fully unrelated to the current conversation; if unavailable, tell the user to start a new `/new` session.
</context>
