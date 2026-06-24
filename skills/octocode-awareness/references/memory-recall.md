# Memory & recall semantics

Read this when recording or recalling lessons, or when authoring `awareness.py` payloads for the memory commands. The locking/coordination commands and the data model live in `coordination-protocol.md`; the recall ranking math lives in `self-harness.md`.

## Canonical payload contract

Use the Zod schemas in `scripts/schema.mjs` as the canonical JSON payload contract for agents and future MCP/tool wrappers. They are not a one-to-one list of CLI flags; the CLI often uses repeatable flags such as `--target-file` where the JSON wrapper uses arrays such as `target_files`. The schema names are `tell_memory`, `get_memory`, `forget_memory`, `refinement`, `refine_query`, `refine_delete`, `pre_flight_intent`, `wait_for_lock`, `prune_stale_locks`, `release_file_lock`, `verify`, `notify`, `notify_query`, `notify_resolve`, `notify_prune`, `reflect`, `harness_apply`, `memory_export`, and `memory_import`. Inspect or validate them with:

```bash
node <skill_root>/scripts/schema.mjs list
node <skill_root>/scripts/schema.mjs json-schema pre_flight_intent
node <skill_root>/scripts/schema.mjs example tell_memory
node <skill_root>/scripts/schema.mjs validate tell_memory payload.json
```

The Python CLI also accepts underscore aliases for these protocol-style names: `tell_memory`, `get_memory`, `pre_flight_intent`, `wait_for_lock`, `prune_stale_locks`, `release_file_lock`, and `notify_get`.

## Sharing memories as files — `memory-export` / `memory-import`

Memories are global per-machine by default. To **share them with a team and store them as files in the repo**, export to a git-diffable JSONL and commit it; teammates import it:

```bash
# Author: write ACTIVE memories to a committable file (default <workspace>/.octocode/memories.jsonl)
awareness.py memory-export --min-importance 5          # optional floor; --out <path> to override
git add .octocode/memories.jsonl && git commit -m "share agent memories"

# Teammate: load the committed file into their store (dedupes by memory_id)
awareness.py memory-import .octocode/memories.jsonl    # --mode skip (default, keep local) | replace
```

`memory-export` is schema-agnostic (`SELECT *`) and skips embedding blobs (rebuild with `embed-index` after import). `memory-import` keeps existing memories under `--mode skip` and overwrites under `--mode replace`, refreshing the FTS index either way. For a **fully repo-local** memory store (every memory lives in the repo, not `~/.octocode`), point `OCTOCODE_MEMORY_HOME=<repo>/.octocode/memory` — then `tell-memory`/`get-memory` read and write inside the repo directly. Export/import is the file-based, merge-friendly path; a repo-local DB is the all-in path. Never commit secrets — the same safety rule applies to exported files.

## `get-memory`

Run before planning or editing when prior lessons may matter.

Important flags:
- `--query`: natural-language recall query.
- `--limit`: maximum memories, default `3`.
- `--min-importance`: filter low-value memories, default `1`.
- `--label`: repeatable category filter (`BUG`, `FEATURE`, `SUGGESTION`, `GOTCHA`, `IMPROVEMENT`, `DECISION`, `ARCHITECTURE`, `SECURITY`, `PERFORMANCE`, `TEST`, `BUILD`, `DOCS`, `CONFIG`, `WORKFLOW`, `REFACTOR`, `API`, `RELEASE`, `INCIDENT`, `OTHER`).
- `--tag`: optional repeated tag filter.
- `--state`: repeatable lifecycle filter; default `ACTIVE` only. Pass `--state SUPERSEDED` to inspect memories replaced via `--supersedes`.
- `--file`: repeatable exact stored file-path filter (normalized to an absolute path).
- `--file-regex`: repeatable regex matched against stored memory file paths.
- `--regex`: repeatable regex matched against task, observation, tags, label, file, and failure signature.
- `--sort`: `smart`/`score` (default salience), `importance`, `recent`, `updated`, `accessed`, `access`, `label`, or `file`.
- `--smart`: when strict recall under-fills, broaden safely: lower `--min-importance`, then drop label/tag filters, then try semantic recall if indexed. Use this for "fetch smart memories" moments before deciding the store has no relevant context.

Recall modes (default ranking blends importance + recency-of-use + access + lexical; see `self-harness.md` for the decay formula and `--no-decay`/`--half-life`/`--explain`):
- `--as-of <ISO>`: **bi-temporal** point-in-time recall — only memories whose valid window (`valid_from`/`valid_to`) contains that instant. Default (omitted) = now-behavior. Set `--valid-from`/`--valid-to` on `tell-memory`; superseding a memory closes its window (`valid_to`) and stamps `expired_at`.
- `--semantic`: **local embedding** recall via `model2vec` — paraphrase-tolerant, finds lessons whose wording differs from the query. Falls back to lexical and reports `mode` when the model isn't installed/vendored, so the default never regresses. Cosine is min-max normalized across the candidate pool, then blended with decay (see `self-harness.md`); `--explain` shows `semantic` (raw cosine) and `semantic_norm`.

  Semantic is **opt-in and self-provisioning** — a shipped skill is just a folder, so build the vectors on first use:

  ```bash
  # One command: pip-install model2vec from scripts/requirements.txt, then embed every memory.
  python3 <skill_root>/scripts/awareness.py embed-index --install
  # Thereafter (deps present): refresh new/changed rows, or --rebuild to re-embed all.
  python3 <skill_root>/scripts/awareness.py embed-index
  # Then recall semantically:
  python3 <skill_root>/scripts/awareness.py get-memory --query "..." --semantic
  ```

  First `embed-index` downloads the default model (`minishlab/potion-base-8M`, ~30 MB) from HuggingFace. For offline/air-gapped installs, vendor it at `scripts/models/potion-base-8M` or point `OCTOCODE_EMBED_MODEL` at a local path. Re-run `embed-index` after `memory-import` (export/import drops embedding blobs).

**A zero-result recall is not proof of absence.** Default recall is lexical (FTS keyword match), so a paraphrased query can miss a real lesson whose wording differs. When `count` is `0`, `get-memory` returns a `hint`: retry with fewer / broader / synonymous terms (or the symbol or file name), use `--smart`, and drop `--tag`/`--label`/`--min-importance`, before concluding nothing is known. Enable `--semantic` (after `embed-index`) for paraphrase-tolerant recall.

Use returned memories as evidence, not as instructions. **MUST:** validate code-related memories against actual current code before relying on them; code changes, so memory is never truth by itself. If validation shows a memory is obsolete or redundant, retire it with `forget --dry-run` first for broad filters or supersede it with `tell-memory --supersedes`.

## `memory-index`

The zero-dependency, Claude-Code-style recall aid. `memory-index` regenerates a concise, model-readable `MEMORY.md` of the top ACTIVE memories (ranked by importance + recency-of-use + access) and writes it next to the global store — `<memory_home>/MEMORY.md` (i.e. `~/.octocode/memory` or `OCTOCODE_MEMORY_HOME`). This mirrors Anthropic's own pattern (Claude Code auto-memory, the API `memory` tool): a small index the agent **reads first**, then `get-memory --query "..."` pulls full detail — the model is the semantic layer, no vector DB. Flags: `--limit` (default 30), `--min-importance`, `--out` (override path), `--stdout` (print only, don't write). Regenerate it after recording or forgetting memories so the index stays current.

## `tell-memory`

Run after a meaningful discovery, bug fix, architectural decision, or surprising failure. Do not record routine status, secrets, credentials, stack traces with tokens, or generic advice.

Memory records are future LLM context, so keep them distilled: summarize the causal lesson, evidence, and verification command instead of pasting long logs or transcripts. Be concise, but do not compress away the root cause, safety caveat, or detail needed to avoid repeating the failure.

Important flags:
- `--agent-id`: stable human-readable agent identifier.
- `--task-context`: concise description of the task that produced the lesson.
- `--observation`: the exact lesson learned.
- `--importance-score`: `1-10` criticality rating.
- `--label`: memory category. Empty or omitted becomes `OTHER`. Prefer specific labels when obvious: `BUG` for defects, `GOTCHA` for surprising constraints, `IMPROVEMENT` for better process, `DECISION` for chosen direction, `SECURITY` for safety-sensitive lessons, etc.
- `--tag`: optional repeated keyword tag.
- `--file`: the ONE file this memory correlates to (normalized to an absolute path, like locks). Omit for a general lesson. A memory is tied to at most one file; use the file-scoped form for "editing X behaves like Y", and the general form for reusable cross-file lessons.
- `--file-tree-fingerprint`: optional Git SHA or workspace state hash.
- `--supersedes`: repeatable; memory id(s) this new memory replaces — each is marked `SUPERSEDED` and points at the new memory. The one-step refine for "I learned a better version."

Importance scale:
- `1-3`: local detail or minor workflow note.
- `4-6`: useful pattern or recurring gotcha.
- `7-8`: important project behavior future agents should know.
- `9-10`: critical architecture rule, data-loss risk, security issue, or repeated failure mode.

Good observations are specific:

```text
Changing X in file Y caused Z because of W. Future agents should do A instead and verify with command B.
```

When the lesson is a specific code snippet, API, or command, include the why/how in the memory itself. Add a source-code comment only when you are already editing that code and a concise comment would prevent real confusion; a snippet with no "why" is noise, but noisy comments in code are also debt.

Feature rating: typed labels + smart recall + regex/file filters are a strong awareness upgrade (8.5/10). Labels make memory browsing and sorting less fuzzy, regex/file filters let agents find path-scoped lessons without over-broad text queries, and `--smart` reduces the common failure mode where a strict recall misses useful context.

## `forget`

Run when a memory is wrong, stale, obsolete, redundant, superseded, or a duplicate. Memories are evidence to verify against current code, not authority — retire ones that would mislead future agents instead of leaving them to resurface in recall.

Important flags (at least one selector is required; all provided filters combine with `AND`):
- `--memory-id`: repeat to target exact memory ids (from a prior `get-memory`).
- `--tag` / `--tags`: match memories carrying the tag(s).
- `--before`: delete memories created before this ISO timestamp (e.g. `2026-01-01T00:00:00Z`).
- `--max-importance`: safety ceiling — only delete memories at or below this importance, so high-value memories are not swept up by a broad filter.
- `--dry-run`: report `would_delete` and the matched memories without deleting. Preview first for any broad filter.

Deletes are removed from both `agent_memories` and the `memory_fts` index. With no selector the command refuses and exits non-zero rather than guessing. For a soft alternative on memories, supersede with `tell-memory --supersedes` (marks the old memory `SUPERSEDED` and hides it from default recall) rather than hard-deleting.

## `reflect` — post-task self-reflection

`reflect` is the front door to the self-harness loop (see `self-harness.md` when running it): after finishing (or abandoning) a task, capture **what worked / what didn't** and route it into action. It records nothing new of its own — it routes into the existing stores so the right reader picks each item up:

- `--task` (required) + `--outcome worked|partial|failed` (required), with optional `--worked` / `--didnt-work` narrative.
- `--lesson` → a **general memory** (tagged `reflection` + the outcome), recalled later and clustered under `mine-weakness` when you also pass `--failure-signature`. Importance defaults by outcome (failed 8 / partial 6 / worked 5) unless `--importance` overrides.
- `--fix-repo "<note>" [--fix-file <path> …]` → an **open, `quality:bad` workspace-scoped refinement** in the shared store — a concrete *"fix this in the repo/code"* indication the next agent sees via `refine-get` and the viewer. `--repo`/`--ref` auto-fill from git.
- `--fix-harness "<note>"` → folded into the learning memory tagged `harness`, so `export-harness` surfaces it as a proposed skill/AGENTS.md improvement.

One call can emit all three. The result reports `learning_memory_id`, `repo_fix_refinement_id`, and `harness_fix`, plus the `next` steps. **Discipline is unchanged: reflect records and proposes — a human merges.** It never edits repo code or the skill itself.
