# Active Memory Navigation

**Status**: recommended prototype, not a shipped command.

This note records the current product decision for applying NapMem-style active memory navigation to `@octocodeai/octocode-awareness`.

Research anchor: [From Passive Retrieval to Active Memory Navigation: Learning to Use Memory as a Structured Action Space](https://arxiv.org/html/2607.05794v1), arXiv:2607.05794v1, 2026-07-07.

## Decision

The best next improvement is a deterministic `memory navigate` / `awareness navigate` prototype that orchestrates the existing Awareness surfaces and returns a traceable plan.

Do this before building persistent topic tracks, before making semantic recall mandatory, before adding a new evidence schema, and before storing raw conversations.

Why: Awareness already has the pieces agents use during planning:

- `workspace status` for locks, memory counts, and open state,
- `query repo-profile`, `query gotchas`, and `query lessons` for scoped summaries,
- `memory recall` with smart broadening, labels, references, regex, files, temporal filters, and confidence flags,
- `reflect mine-weakness` for repeated failure patterns,
- `refinement get` and `signal list` for handoffs and messages,
- `repo inject` for generated repo-readable projections.

The missing product behavior is not another storage layer first. It is an agent-native navigation step that decides which of those surfaces to inspect for the current task, records what it tried, and tells the agent what evidence is strong, weak, missing, or stale.

## Proposed Command

```bash
octocode-awareness memory navigate \
  --query "task or risk to investigate" \
  --workspace "$PWD" \
  --file packages/foo/src/bar.ts \
  --limit 5 \
  --compact
```

Alternative command group: `awareness navigate`. Prefer `memory navigate` if the first prototype stays focused on memory and planning evidence.

Expected output shape:

```json
{
  "ok": true,
  "query": "task or risk to investigate",
  "navigation_trace": [
    {
      "step": "workspace_status",
      "reason": "check locks and pending verification",
      "result_summary": "no conflicting locks"
    },
    {
      "step": "query",
      "view": "repo-profile",
      "reason": "inspect high-level awareness state"
    },
    {
      "step": "memory_recall",
      "query": "task or risk to investigate",
      "reason": "retrieve direct lessons",
      "judgment_required": false
    }
  ],
  "evidence": [
    {
      "memory_id": "mem_...",
      "source": "memory_recall",
      "strength": "direct",
      "reason": "high lexical score plus file/reference match"
    }
  ],
  "gaps": [
    "No file-scoped gotcha matched packages/foo/src/bar.ts"
  ],
  "next": "Verify cited memories against current files before relying on them."
}
```

## Routing Rules

Start with deterministic rules. Do not add RL until traces show repeated bad routing that rules and prompt guidance cannot fix.

Suggested first-pass routing:

| Signal | Navigation action |
|---|---|
| Any task | `workspace status` |
| Query mentions a file or command passes `--file` | `memory recall --file`, then broader `memory recall --smart` if under-filled |
| Query mentions a failure, regression, flaky test, or repeated mistake | `reflect mine-weakness` plus `query gotchas` |
| Query is broad or architecture-oriented | `query repo-profile`, `query lessons`, then targeted `memory recall` |
| Recall returns `judgment_required` | retry with `--smart`, drop narrow label/tag filters, and mark the result as a lead |
| Recall cites file references | emit those references as verification targets, not final proof |
| No useful evidence after broadening | return an explicit gap instead of pretending absence is evidence |

## Tradeoff Matrix

| Option | Benefit | Cost / risk | Decision |
|---|---|---|---|
| Deterministic memory navigation | Highest workflow value per change; reuses existing commands; easy to test with trace fixtures; makes agent planning less ad hoc. | Needs careful output discipline so it does not become another verbose briefing. Rules will need tuning from real traces. | Do first. |
| Generated topic tracks | Gives medium-range narrative continuity over memories, closer to NapMem topic tracks. | Adds persistence, invalidation, projection, merge, and staleness complexity. Bad tracks can become convincing but stale summaries. | Do after navigation traces show repeated clusters worth summarizing. |
| Hybrid semantic recall | Helps vocabulary misses and paraphrases. | Requires an embedding source, ranking calibration, model/version metadata, storage growth, and more recall tests. Existing CLI/Pi recall is lexical/salience unless a caller explicitly uses embedding helpers. | Keep optional; wire after navigation can measure recall misses. |
| Evidence-strength calibration | Reduces overgeneralizing one event into a rule. | A schema-wide `memory_kind` / `evidence_strength` field touches store, CLI, Pi, docs, viewers, projections, tests, and migration. | Start as computed navigation output; persist only once useful categories stabilize. |
| Raw conversation storage | Strongest evidence fidelity. | Privacy, forgetting, storage, and redaction risk. Coding-agent awareness usually has better evidence pointers: file refs, task ids, command output summaries, and PR/URL refs. | Do not store raw conversations by default. |

## MVP

1. Add a pure library planner that accepts query, scope, files, labels, and limits.
2. Have the planner call existing in-process functions, not shell subprocesses.
3. Expose it through CLI and Pi as one read-only command.
4. Return `navigation_trace`, `evidence`, `gaps`, `judgment_required`, and `next`.
5. Add fixtures for common planning cases:
   - file-scoped gotcha,
   - broad architecture query,
   - recurring failure pattern,
   - zero-result recall,
   - weak recall requiring verification,
   - open lock / pending verification.

## Documentation Rules

Until the command ships:

- Do not list `memory navigate` in command maps as an available command.
- Refer to this document as a product decision or prototype plan.
- Keep direct workflow docs pointing to the current commands: `workspace status`, `memory recall`, `refinement get`, `signal list`, `query`, and `reflect mine-weakness`.
- Keep semantic recall docs explicit: the shipped CLI recall path uses lexical FTS plus salience; embedding helpers are library-level building blocks unless a host wires them into recall.
