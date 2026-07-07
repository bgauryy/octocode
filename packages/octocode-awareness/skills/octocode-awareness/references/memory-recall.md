# Memory & recall semantics

Read this when recording or recalling lessons, or when authoring `awareness.mjs` payloads for memory commands. The locking/coordination commands and data model live in `coordination-protocol.md`. Recall ranking lives in `self-harness.md`.

## Canonical payload contract

Use the Zod schemas in `scripts/schema.mjs` as the canonical JSON payload contract for agents and future MCP/tool wrappers. CLI flags are not one-to-one with JSON fields; for example, `--target-file` maps to `target_files`. Inspect schemas with:

```bash
node <skill_root>/scripts/schema.mjs list
node <skill_root>/scripts/schema.mjs json-schema pre_flight_intent
node <skill_root>/scripts/schema.mjs example tell_memory
node <skill_root>/scripts/schema.mjs validate tell_memory payload.json
```

The CLI accepts protocol-style underscore aliases for every command (`tell_memory` → `tell-memory`, `pre_flight_intent` → `pre-flight-intent`, `notify_get` → `notify-get`, …). Unknown flags are hard errors — the CLI never silently ignores a flag, so a typo or an unsupported option fails loudly with the known-flag list.

For token-efficient agent reads, pass `--compact` after the command or set `OCTOCODE_AWARENESS_COMPACT=1`; it minifies JSON without changing fields.

Memories are global per-machine by default. For a repo-local store, point `OCTOCODE_MEMORY_HOME=<repo>/.octocode/memory`; then `tell-memory`/`get-memory` read and write inside the repo. Never commit secrets or raw memory databases unless a human explicitly approves the storage model.

## `get-memory`

Run before planning or editing when prior lessons may matter.

Important flags:
- `--query`: natural-language recall query.
- `--limit`: maximum memories, default `3`.
- `--min-importance`: filter low-value memories, default `1`.
- `--label`: repeatable category filter (`BUG`, `FEATURE`, `SUGGESTION`, `GOTCHA`, `IMPROVEMENT`, `DECISION`, `ARCHITECTURE`, `SECURITY`, `PERFORMANCE`, `TEST`, `BUILD`, `DOCS`, `CONFIG`, `WORKFLOW`, `REFACTOR`, `API`, `RELEASE`, `INCIDENT`, `OVERRIDE`, `OTHER`). `OVERRIDE` memories contradict model training defaults (e.g. "this repo uses Bun, not npm") and are always surfaced in the smart briefing regardless of importance.
- `--tag`: optional repeated tag filter.
- `--state`: repeatable lifecycle filter; default `ACTIVE` only. Pass `--state SUPERSEDED` to inspect memories replaced via `--supersedes`.
- `--file`: repeatable exact stored file-path filter (normalized to an absolute path).
- `--file-regex`: repeatable regex matched against stored memory file paths.
- `--reference`: repeatable exact provenance filter, matched against structured `references[]`; use this for "everything learned from source X".
- `--workspace` / `--repo` / `--ref`: optional applicability filters. Default scoped recall includes broader global/applicable memories (`NULL OR exact`) so repo work still sees global developer gotchas. Add `--strict-scope` for exact matches only, or `--global-only` to inspect unscoped lessons.
- `--regex`: repeatable regex matched against task, observation, tags, references, label, workspace/repo/ref, file, and failure signature.
- `--sort`: `smart`/`score` (default salience blend), `importance`, `recent`, or `accessed`.
- `--explain`: attach `score_components` (importance/recency/access/relevance + weights) to each result — use it to understand or tune ranking.
- `--smart`: when strict recall under-fills, broaden safely: lower `--min-importance`, then drop label/tag filters. Use this for "fetch smart memories" moments before deciding the store has no relevant context.

Recall modes (default ranking blends importance + recency-of-use + access + lexical):
- `--as-of <ISO>`: **bi-temporal** point-in-time recall — only memories whose valid window (`valid_from`/`valid_to`) contains that instant.

**A zero-result recall is not proof of absence.** Retry with `--smart` or drop label/tag filters before concluding no match.

**`judgment_required` flag:** when recall confidence is low — zero results, FTS unavailable, or a weak top match — the response carries `judgment_required: true` plus a `judgment_reason`. Treat those results as leads: verify against current files or broaden the query before relying on them.

**Validate code memories against current files** before relying on them; supersede or `memory_forget` obsolete results.

## `tell-memory`

Run after a meaningful discovery, bug fix, architectural decision, or surprising failure. Record durable lessons only. Skip routine status, secrets, credentials, token-bearing stack traces, and generic advice.

Before recording: state why — which future decision it improves or failure it prevents. If you cannot name one, skip it.
When converting a capture packet to `tell-memory`, fold the reason into `--task-context` or `--observation`.

Memory records are future LLM context, so keep them distilled.
Summarize the causal lesson, evidence, and verification command instead of pasting logs.
Be concise, but keep the root cause, safety caveat, or detail needed to avoid repeating the failure.

Important flags:
- `--agent-id`: stable human-readable agent identifier.
- `--task-context`: concise description of the task that produced the lesson.
- `--observation`: the exact lesson learned.
- `--importance-score`: `1-10` criticality rating.
- `--label`: memory category. Empty or omitted becomes `OTHER`. Prefer specific labels: `BUG`, `GOTCHA`, `IMPROVEMENT`, `DECISION`, `SECURITY`, `OVERRIDE` (contradicts a model default — always surfaced in briefing), etc.
- `--tag`: optional repeated keyword tag.
- `--reference`: repeated provenance string. Examples: URL, PR, repo, npm package, doc, or local file. Use references for research findings so recall surfaces the conclusion and sources. References are indexed and folded into FTS/docs/viewer/import-export.
- `--workspace` / `--repo` / `--ref`: optional applicability scope. Use for repo-specific lessons. Omit for global gotchas and cross-repo learning. `--repo`/`--ref` auto-fill from `--workspace` git when omitted.
- `--file`: the ONE file this memory correlates to, normalized like locks. Omit for general lessons. Use file scope for "editing X behaves like Y"; use general scope for reusable cross-file lessons.
- `--file-tree-fingerprint`: optional Git SHA or workspace state hash.
- `--supersedes`: repeatable; memory id(s) this new memory replaces — each is marked `SUPERSEDED` and points at the new memory. The one-step refine for "I learned a better version."

Importance scale:
- `1-3`: local detail or minor workflow note.
- `4-6`: useful pattern or recurring gotcha.
- `7-8`: important project behavior future agents should know.
- `9-10`: critical architecture rule, data-loss risk, security issue, or repeated failure mode.

**Consolidation surface:** when a new memory overlaps existing ones (low novelty) and no `--supersedes` was given, the response carries a `consolidation` block — `novelty_score`, `similar_memory_ids`, and a hint. Review the candidates and either re-record with `--supersedes <id>` (replace the older version) or `forget` the redundant one. The store surfaces candidates; the calling agent decides.

Novelty is measured by Jaccard similarity (`SIMILARITY_THRESHOLD = 0.45`) over tokenized text (camelCase split, lowercased, stop-words removed) against the `SIMILARITY_PREFETCH = 12` most-similar existing memories. A score below 0.45 means the memory is very similar to an existing one and a consolidation advisory fires. `session-capture` also reports a `consolidation_opportunities` count of memories with novelty < 0.2 so you know when the store needs a digest pass.

**Decay defaults by label:** durable labels (`DECISION`, `ARCHITECTURE`, `SECURITY`, `GOTCHA`, `OVERRIDE`) get a 90-day recall half-life; `EXPERIENCE` reflections decay fast (14 days); everything else uses the standard 30 days. Stored per row in `decay_half_life_days`, so individual memories can be tuned later.

Good observations are specific:

```text
Changing X in file Y caused Z because of W. Future agents should do A instead and verify with command B.
```

When the lesson is a specific code snippet, API, or command, include the why/how in the memory itself.
Add a source-code comment only when already editing that code and a concise comment would prevent real confusion.
A snippet with no "why" is noise; noisy comments in code are also debt.

## `forget`

Run when a memory is wrong, stale, obsolete, redundant, superseded, or duplicated. Memories are evidence, not authority. Retire memories that would mislead future agents.

Important flags (at least one selector is required; all provided filters combine with `AND`):
- `--memory-id`: repeat to target exact memory ids (from a prior `get-memory`).
- `--tag` / `--tags`: match memories carrying the tag(s).
- `--before`: delete memories created before this ISO timestamp (e.g. `2026-01-01T00:00:00Z`).
- `--max-importance`: safety ceiling — only delete memories at or below this importance, so high-value memories are not swept up by a broad filter.
- `--dry-run`: report `would_delete` and the matched memories without deleting. Preview first for any broad filter.

**Salience floor:** broad selectors (`--tag`/`--before` without explicit `--memory-id`) never delete memories with importance ≥ 8 unless `--max-importance` explicitly raises the ceiling; the response reports `salience_floor: 8` when the cap applied. Explicit `--memory-id` deletes always bypass the floor.

Deletes remove rows from `agent_memories` and `memory_fts`.
With no selector the command refuses instead of guessing.
For soft deletion, supersede with `tell-memory --supersedes` rather than hard-deleting.

## `reflect` — post-task self-reflection

`reflect` is the front door to the self-harness loop; see `self-harness.md` when running it.
After finishing or abandoning a task, capture what worked or failed and route it into action.
`reflect` routes into existing stores so the right reader picks each item up:

- `--task` (required) + `--outcome worked|partial|failed` (required), with optional `--worked` / `--didnt-work` narrative.
- `--lesson` → a **general memory** (tagged `reflection` + the outcome), recalled later and clustered under `mine-weakness` when you also pass `--failure-signature`. Importance defaults by outcome (failed 8 / partial 6 / worked 5) unless `--importance` overrides.
- `--fix-repo "<note>" [--fix-file <path> …]` → an **open, `quality:bad` workspace-scoped refinement**. The next agent sees it via `refine-get` and the viewer. `--repo`/`--ref` auto-fill from git.
- `--fix-harness "<note>"` → folded into the learning memory tagged `harness`, so `export-harness` surfaces it as a proposed skill/AGENTS.md improvement.

One call can emit all three. The result reports ids plus next steps. **Discipline is unchanged: reflect records and proposes — a human merges.** It never edits repo code or the skill itself.

## Research capture

Capture after meaningful convergence, not during exploration: prior art resolved, a root cause found, a decision and the evidence that drove it. Skip routine status, raw dumps, and secrets.

**Pi (native tools) — preferred:**

```typescript
memory_record({
  task_context: "why a future agent needs this — fold the reason here",
  observation:  "durable verdict or decision-changing constraint",
  label:        "DECISION",        // or BUG, GOTCHA, ARCHITECTURE, …
  references:   ["pr:owner/repo#123", "npm:fast-glob@3.3.2", "https://docs.example.com"],
  supersedes:   ["mem_old_id"],    // omit if no prior version
  importance:   7,
})
// For post-task lessons with fix_repo / failure_signature:
memory_reflect({ task, outcome, lesson, references, fix_repo, failure_signature })
```

**CLI** (`scripts/awareness.mjs tell-memory`): same fields via `--task-context`, `--observation`, `--label`, `--reference` (repeatable), `--supersedes` flags.

### Reference format

| Form | Example |
|------|---------|
| URL | `https://docs.acme.dev/page` |
| Pull request | `pr:owner/repo#123` |
| Commit | `commit:owner/repo@abc1234` |
| Repository | `repo:owner/repo@main` |
| npm package | `npm:fast-glob@3.3.2` |
| Local file | `file:/abs/path/to/file.ts:42` |

Scope flags (`workspace_path`, `repo`, `ref`, `file`) set *where* a lesson applies. `references` record *provenance* (where you learned it).
One distilled memory per session beats one per ledger row. When you find a better answer, `supersedes:` the old memory.
