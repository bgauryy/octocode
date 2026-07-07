# Self-harness reference

The store drives a closed improvement loop: **mine recurring failures -> propose a fix -> a human validates**. Many failures are harness failures: unverified conclusions, repeated bad recovery, or lost source-of-truth context.

## 0. Reflect — the front door

`reflect` is the one command that turns "I just finished a task" into the loop below. After finishing (or abandoning) work, run it with what happened:

```bash
reflect --agent-id <a> --task "<what I did>" --outcome worked|partial|failed \
  [--worked "..."] [--didnt-work "..."] \
  [--judgment-note "<evidence checked + uncertainty>"] \
  [--lesson "<reusable learning>" --failure-signature "<sig>"] \
  [--eval-failure-json '[{"id":"...","dimension":"...","failure_signature":"mechanism:...|cause:...","suggested_lesson":"..."}]'] \
  [--fix-repo "<fix this in the code>" --fix-file <path>] \
  [--fix-harness "<improve this skill>"] [--duo]
```

`reflect` routes the reflection into existing surfaces:

- **Learning** (`lesson:`) → a general memory (§3 recall, §2 `mine-weakness` when given `failure_signature:`).
- **Repo/code fix** (`fix_repo:`) → a refinement (`quality:good` if outcome=worked, `quality:bad` if partial/failed) visible via `memory_refine_get` — the durable *"fix this here"* queue.
- **Harness improvement** (`fix_harness:`) → a `harness`-tagged memory that §4 `export-harness` surfaces for `AGENTS.md`/`CLAUDE.md`.

Use `--judgment-note` for checked evidence, uncertainty, and eval/checklist relevance.
Use `--duo` for substantial or ambiguous outcomes; it emits an advisory `reflection_duo` packet and does not affect storage.
For subagent-heavy work, pair `--judgment-note` with the compact evidence receipt from `agentic-flows.md`.
For awareness maintenance, include preview and verification evidence in `--judgment-note` (for example `forget --dry-run`, `digest --dry-run`, recall after cleanup, or test output).

### Binary-question eval failures

Treat failed binary questions as diagnostic packets, not auto-patch instructions.

- Record high-signal recurring failures with `memory_reflect({ lesson: "...", failure_signature: "<sig>" })` (Pi) or `reflect --lesson ... --failure-signature <sig>` (CLI).
- Prefer `reflect --eval-failure-json '[...]'` when the eval output is structured. Each entry keeps `id`, optional `dimension`, `failure_signature`, and `suggested_lesson`. `reflect` tags the memory as `eval` and uses the first provided signature for `mine-weakness` when `--failure-signature` is omitted.
- If an eval emits `agenticEval`, use its generated questions as seed prompts for a semantic eval agent.
  The eval agent may rewrite, add, or drop questions based on intent; they guide judgment, not fixed pass/fail.

## 1. Validate before you conclude

The flagship failure class is declaring success without checking the artifact.

- At `pre-flight-intent` you already declare a `--test-plan`. After doing the work,
  **run it and record that it ran**:
  - **Pi:** `memory_verify({ task_id: "...", status: "SUCCESS" })` or `memory_verify({ allPending: true })`
  - **Pi:** `file_lock({ type: "release", task_id: "...", verified: true, verified_note: "ran yarn test, 273 passed" })`
  - **CLI:** `awareness.mjs verify --agent-id <a> --all-pending --message "command=... exit=0 evidence=<ref>"`
- A `VERIFIED` event is written to `task_log`. If you release `--status SUCCESS`
  on a task that declared a test-plan but recorded no verification, the response
  carries an `unverifiedConclusion` warning and stores the task as `PENDING`.
  The post-edit hook also releases file locks as `PENDING`, so coordination is
  unblocked while verification remains auditable.
- The **Stop/SubagentStop hook** runs `memory_audit_unverified` and blocks conclusion once if any task has no `VERIFIED` event. Loop-guarded; opt-out via `OCTOCODE_NO_VERIFY_GATE=1`.

`audit-unverified` returns two task categories — both count toward the gate:
- **`unverified`** — `PENDING` tasks: locks released, verify call missing. Run the declared test plan and call `memory_verify`.
- **`stale_active`** — `ACTIVE` tasks whose locks expired without explicit release. Use `audit-unverified --abandon` if abandoned, or verify if completed. `PENDING` means released but unverified; stale-active means TTL expired before release.

`memory_audit_unverified` (Pi) / `audit-unverified --agent-id <a>` (CLI) lists unverified tasks; exits `1` when any exist.

## 2. Mine recurring failures

Tag a failure with a stable signature when you record it:

```bash
memory_record({ ..., failure_signature: "mechanism:retry-loop|cause:test-timeout" })  # Pi
# CLI: awareness.mjs tell-memory ... --failure-signature "mechanism:retry-loop|cause:test-timeout"
```

**Optional `|surface:Z` suffix** — names the harness surface where the failure originates:

```
mechanism:unverified-conclusion|cause:missing-verify|surface:verify-gate
mechanism:fts-miss|cause:empty-store|surface:briefing-miss
mechanism:lock-conflict|cause:ttl-expired|surface:lock-conflict
```

Valid surfaces: `verify-gate`, `lock-conflict`, `fts-miss`, `briefing-miss`, `doc-drift`.
The surface helps `export-harness` target a proposal. Clustering strips `|surface:Z` and reports distinct surfaces separately.

`mine-weakness [--min-count N] [--limit N]` is CLI-only.
`mine-weakness` clusters by base `failure_signature`, ranks by **support x avg-importance**, and includes up to three example observations.
`--min-count` defaults to `2`; one-off failures stay out of the view.
A Jaccard diversity filter prevents near-duplicate mechanisms from filling the result.
Signatures power this view only; general recall still uses FTS5 plus decay.

## 3. Salience-Decayed Recall

`get-memory` ranks by a blend:

```
recency    = exp(-ln2 * age_days / half_life)   # age from last use
importance = importance / 10
access     = log1p(access_count) / log1p(50)     # saturating
relevance  = query match, normalized to 0..1    # the "lexical" weight slot
final = 0.25*importance + 0.30*recency + 0.15*access + 0.30*relevance
```

| Term | Behavior |
|---|---|
| lexical | CLI mode; normalized FTS5 BM25, neutral `0.5` on weak/no-FTS fallback |
| `judgment_required` | Set on zero results, fallback mode, or top relevance below `0.35` |
| semantic | Library-only vectors; CLI `--semantic` warns and returns lexical results |

Every recall bumps `access_count` and `last_accessed_at`.
Half-life defaults by label: durable labels 90d, `EXPERIENCE` 14d, everything else 30d.
Use `--sort` and `--explain` to inspect scoring.

## 4. Refine the harness — the loop's last step

A recurring lesson should stop being "might recall" and become standing guidance.
`export-harness` (CLI) previews lessons for `AGENTS.md` / `CLAUDE.md` in two tiers:
- **Tier 1** (always first): `harness`-tagged memories from `memory_reflect fix_harness:`.
- **Tier 2**: high-importance general memories (importance ≥ 7, label ≠ EXPERIENCE).
`export-harness` is preview-only and never writes files.

### Harness improvement gate

Valid surfaces include `AGENTS.md`/`CLAUDE.md`, docs, reference files, memory-corpus changes, and this skill's prompts, hooks, scripts, schemas, and tests.
Propose changes when the user asks, or when evidence shows a repeated failure/opportunity:
`mine-weakness`, user correction, eval failure, unverified task, recurring gotcha, or a missing check.

Before applying any harness change, ask the user with a concrete fix request:
- Target surface/files.
- Observed correction or opportunity.
- Why future agents need it; name the failure or decision it changes.
- Evidence source: memory/refinement/eval/user correction/file.
- Proposed change, risk/rollback, and verification plan.

Until approved, keep the proposal in conversation or a proposal-only refinement.
Do not edit files or mutate standing harness memories.
Use `memory_reflect fix_harness:` only when the user asks for durable proposal capture.
One approval covers only the scoped change.

## Hard NOs

- No **unattended** self-modifying loop. Edit the skill only after explicit human approval for the scoped change.
- No automatic prompt rewrite from failed binary questions or advisory `agenticEval` prompts. Treat them as evidence for human-reviewed proposals.
- No numeric regression-gate infrastructure. This service-free local skill flags regressions in notes instead.
- Failure signatures are for the weakness view only — never the sole recall path.
