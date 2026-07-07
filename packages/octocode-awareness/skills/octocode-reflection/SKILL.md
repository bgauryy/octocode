---
name: octocode-reflection
description: "Use when completed work needs learning, stale-memory cleanup, staged approval, skill learning, or harness/self-improvement review. Record durable lessons, reflect on outcomes, prune stale memory, and stage human-approved changes."
---

# Octocode Reflection

Use this as the learning and maintenance loop after work has evidence. It shares the `~/.octocode/memory/awareness.sqlite3` store with `octocode-awareness`; Awareness handles live coordination, Reflection handles what should be learned, cleaned, or proposed for future behavior.

Agent-facing Pi tools: `memory_record`, `memory_reflect`, `memory_export_harness`, and `memory_recall` for checking existing lessons.
User/CLI maintenance uses `memory_digest` and `memory_forget`. Scripts call `scripts/awareness.mjs <cmd>`.

## Default Loop
1. **Select** — keep only outcomes that change a future decision: root causes, corrections, decisions, repeated failures, durable workflows, or stale-memory cleanup.
2. **Record** — use `memory_record` for verified reusable facts; use `supersedes` when replacing old knowledge.
3. **Reflect** — use `memory_reflect` after non-trivial work to route `lesson`, `fix_repo`, `fix_harness`, `failure_signature`, `judgment_note`, or eval evidence.
4. **Stage** — propose skill, harness, AGENTS.md, or repo-instruction changes with evidence and rollback; keep standing guidance unchanged until human approval.
5. **Clean** — after work, use `references/memory-hygiene.md` to clear stale memories and historical pending tasks; preview or scope before mutating.
6. **Maintain** — preview `memory_digest`/`memory_forget` before pruning expired, stale, duplicate, wrong, or superseded memories.

## References
- `references/learning-loop.md` — after completed work, corrections, skill-learning candidates, eval failures, or background review.
- `references/memory-hygiene.md` — after work, before cleanup of stale memories, refinements, signals, or historical pending tasks.
- `references/staged-approval.md` — before proposing or applying repo, skill, harness, AGENTS.md, or corpus changes.
- `references/self-harness.md` — when using `memory_reflect`, failure signatures, mine-weakness, eval packets, or harness export.
- `references/harness.md` — when auditing verification gates, session capture, doc staleness, or harness logs.
- `references/brain-model.md` — when tuning salience, sleep, cleanup, or long-term memory layering.
- `references/agentic-flows.md` — when composing subagent receipts, reflection, cleanup, and hook handoffs.
- `references/corpus.md` — when curating `~/.octocode/awareness/corpus/` notes from repeated lessons.
- `references/data-view.md` — when showing/exporting/pruning awareness data on request.
- `references/references.md` — source audit for this split.

## Scripts
- `scripts/awareness.mjs` — record, reflect, digest, forget, audit cleanup, export-harness, mine-weakness, and data-view commands.
- `scripts/schema.mjs` — inspect JSON payload contracts before building wrappers or Pi adapters.

## Boundary
Use `octocode-awareness` before/during edits for recall, locks, signals, handoffs, and verification gates. Use this skill after the outcome is known, or when the user asks to learn, prune, stage, approve, or improve.
