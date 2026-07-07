# Memory & recall semantics

Read this when recording or recalling lessons, or when authoring memory-command payloads. Coordination lives in `coordination-protocol.md`; recall scoring lives in `self-harness.md`.

## Canonical payload contract

Use `scripts/schema.mjs` as the canonical JSON contract. CLI flags are not always field names: `--target-file` maps to `target_files`.

```bash
node <skill_root>/scripts/schema.mjs list
node <skill_root>/scripts/schema.mjs json-schema pre_flight_intent
node <skill_root>/scripts/schema.mjs example tell_memory
node <skill_root>/scripts/schema.mjs validate tell_memory payload.json
```

The CLI accepts underscore aliases: `tell_memory` -> `tell-memory`, `pre_flight_intent` -> `pre-flight-intent`, `notify_get` -> `notify-get`.
Unknown flags are hard errors with the known-flag list.

For token-efficient agent reads, pass `--compact` after the command or set `OCTOCODE_AWARENESS_COMPACT=1`; it minifies JSON without changing fields.

Memories are global per-machine by default. For a repo-local store, set `OCTOCODE_MEMORY_HOME=<repo>/.octocode/memory`. Never commit secrets or raw memory databases without explicit human approval.

## `get-memory`

Run before planning or editing when prior lessons may matter.

Important flags:
- `--query`: natural-language recall query.
- `--limit`: maximum memories, default `3`.
- `--min-importance`: filter low-value memories, default `1`.
- `--label`: repeatable category filter such as `BUG`, `GOTCHA`, `DECISION`, `ARCHITECTURE`, `SECURITY`, `OVERRIDE`, or `OTHER`.
- `--tag`: optional repeated tag filter.
- `--state`: repeatable lifecycle filter; default `ACTIVE` only. Pass `--state SUPERSEDED` to inspect memories replaced via `--supersedes`.
- `--file`: repeatable exact stored file-path filter (normalized to an absolute path).
- `--file-regex`: repeatable regex matched against stored memory file paths.
- `--reference`: repeatable exact provenance filter, matched against structured `references[]`; use this for "everything learned from source X".
- `--workspace` / `--artifact` / `--repo` / `--ref`: applicability filters. Default recall includes exact scope plus broader `NULL` rows.
- `--strict-scope` / `--global-only`: exact scope only, or only unscoped lessons.
- `--regex`: repeatable regex matched against task, observation, tags, references, label, workspace/repo/ref, file, and failure signature.
- `--sort`: `smart`/`score` (default salience blend), `importance`, `recent`, or `accessed`.
- `--explain`: attach `score_components` (importance/recency/access/relevance + weights) to each result — use it to understand or tune ranking.
- `--smart`: when strict recall under-fills, broaden safely: lower `--min-importance`, then drop label/tag filters. Use this for "fetch smart memories" moments before deciding the store has no relevant context.

Recall modes (default ranking blends importance + recency-of-use + access + lexical):
- `--as-of <ISO>`: **bi-temporal** point-in-time recall — only memories whose valid window (`valid_from`/`valid_to`) contains that instant.

**A zero-result recall is not proof of absence.** Retry with `--smart` or drop label/tag filters before concluding no match.

**`judgment_required` flag:** low-confidence recall returns `judgment_required: true` plus `judgment_reason`. Verify against current files or broaden the query before relying on it.

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
- `--importance`: `1-10` criticality rating.
- `--label`: memory category. Empty or omitted becomes `OTHER`. Prefer specific labels: `BUG`, `GOTCHA`, `IMPROVEMENT`, `DECISION`, `SECURITY`, `OVERRIDE` (contradicts a model default — always surfaced in briefing), etc.
- `--tag`: optional repeated keyword tag.
- `--reference`: repeated provenance string: URL, PR, repo, npm package, doc, or local file.
- `--workspace` / `--artifact` / `--repo` / `--ref`: applicability scope. `--repo`/`--ref` auto-fill from `--workspace` git when omitted.
- `--file`: the one file this memory correlates to, normalized like locks. Omit for general lessons.
- `--file-tree-fingerprint`: optional Git SHA or workspace state hash.
- `--supersedes`: repeatable; memory id(s) this new memory replaces — each is marked `SUPERSEDED` and points at the new memory. The one-step refine for "I learned a better version."

Importance scale:
- `1-3`: local detail or minor workflow note.
- `4-6`: useful pattern or recurring gotcha.
- `7-8`: important project behavior future agents should know.
- `9-10`: critical architecture rule, data-loss risk, security issue, or repeated failure mode.

**Consolidation surface:** if a memory overlaps existing rows and lacks `--supersedes`, the response includes `consolidation`: `novelty_score`, `similar_memory_ids`, and a hint. Review candidates, then re-record with `--supersedes <id>` or `forget` the redundant row.

Novelty uses Jaccard similarity over tokenized text against the 12 closest existing memories. A score below `0.45` triggers consolidation advice. `session-capture` reports `consolidation_opportunities` for rows below `0.2`.

**Decay defaults by label:** durable labels get a 90-day half-life, `EXPERIENCE` gets 14 days, and everything else gets 30 days. Stored per row in `decay_half_life_days`.

Good observations name the cause, evidence, future action, and verification command.
For snippets, APIs, or commands, include the why/how in the memory itself.
Add source-code comments only while editing code and only when the comment prevents real confusion.

## `forget`

Run when a memory is wrong, stale, obsolete, redundant, superseded, or duplicated. Memories are evidence, not authority. Retire memories that would mislead future agents.

Choose the smallest safe cleanup: supersede better current facts, `forget` wrong/duplicate rows, and run `digest --dry-run` before expired/superseded retention cleanup.
For legacy tables (`agent_memories`, `memory_fts`, old intents/locks), use `references/legacy-migration.md`; `forget` only targets current `memories`.

Important flags (at least one selector is required; all provided filters combine with `AND`):
- `--memory-id`: repeat to target exact memory ids (from a prior `get-memory`).
- `--tag` / `--tags`: match memories carrying the tag(s).
- `--before`: delete memories created before this ISO timestamp (e.g. `2026-01-01T00:00:00Z`).
- `--max-importance`: safety ceiling — only delete memories at or below this importance, so high-value memories are not swept up by a broad filter.
- `--dry-run`: report `would_delete` and the matched memories without deleting. Preview first for any broad filter.

**Salience floor:** broad selectors (`--tag`/`--before` without explicit `--memory-id`) never delete memories with importance ≥ 8 unless `--max-importance` explicitly raises the ceiling; the response reports `salience_floor: 8` when the cap applied. Explicit `--memory-id` deletes always bypass the floor.

Deletes remove rows from `memories`, `memory_refs`, and `memories_fts`.
With no selector the command refuses instead of guessing.
For soft deletion, supersede with `tell-memory --supersedes` rather than hard-deleting.

## `reflect` — post-task self-reflection

`reflect` is the front door to the self-harness loop; see `self-harness.md` when running it.
After finishing or abandoning a task, capture what worked or failed and route it into action.
`reflect` routes into existing stores so the right reader picks each item up:

- `--task` (required) + `--outcome worked|partial|failed` (required), with optional `--worked` / `--didnt-work` narrative.
- `--lesson`: general memory, tagged `reflection` plus outcome; add `--failure-signature` for `mine-weakness`.
- `--fix-repo "<note>" [--fix-file <path>...]`: open workspace-scoped refinement for `refine-get`.
- `--fix-harness "<note>"`: harness-tagged learning surfaced by `export-harness`.

One call can emit all three. The result reports ids plus next steps. **Discipline is unchanged: reflect records and proposes — a human merges.** It never edits repo code or the skill itself.

## Research capture

Capture after meaningful convergence, not during exploration: prior art resolved, a root cause found, a decision and the evidence that drove it. Skip routine status, raw dumps, and secrets.

**Pi native tools:**

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

**CLI:** `scripts/awareness.mjs tell-memory` exposes the same fields via flags.

Reference examples: URL, `pr:owner/repo#123`, `commit:owner/repo@abc1234`, `repo:owner/repo@main`, `npm:fast-glob@3.3.2`, `file:/abs/path.ts:42`.
Scope flags (`workspace_path`, `artifact`, `repo`, `ref`, `file`) set where a lesson applies; `references` record provenance.
One distilled memory per session beats one per ledger row. When you find a better answer, `supersedes:` the old memory.
