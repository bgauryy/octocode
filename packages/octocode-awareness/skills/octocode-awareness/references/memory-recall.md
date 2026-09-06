# Memory Recall and Trust

Load when prior learning, external references, or prompt-time memory could change the approach.

Memory is a ranked lead, never authority. Current user instructions, source, and fresh tests win.

## Recall

```bash
<cli> memory recall --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --query "<current task>" --smart --compact
```

Use compact recall for orientation. Expand only relevant IDs and verify every decision-changing file, URL, version, or test against current state. Preserve provenance and distinguish repository coordination from global reusable learning.

## Store

Store only verified, reusable facts with narrow scope and references. Prefer a lesson that changes a future decision over status, raw dialogue, or a transcript. Reflect after the check so outcome and evidence remain joined.

## Freshness and conflict

- A stale file reference lowers confidence; it does not silently update itself.
- Conflicting memories remain visible until current evidence resolves them.
- Supersede obsolete knowledge; archive weak material; restore only archived rows.
- Preview forget/digest operations and review exact IDs before deletion.
- Never load a human thesis or large corpus automatically into prompt context.

Prompt-time selection is transient and bounded. A hook may emit `Awareness state changed.` while leaving details in the ledger for targeted reads.

Before writing, ask: Is it verified? Will it change a later action? Is its scope clear? Can a future agent re-check the cited source? If any answer is no, keep it out of durable memory.

Next: use `references/learning-loop.md` to route a verified outcome or return to `SKILL.md`.
