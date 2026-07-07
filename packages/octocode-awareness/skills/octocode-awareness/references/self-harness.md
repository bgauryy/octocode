# Self-harness reference

The store is not just a passive memory — it drives a closed improvement loop:
**mine recurring failures → propose a fix → a human validates**. Performance is set
as much by the *harness* (prompts, tools, checks, recovery rules) as by the model,
and many failures are harness failures: concluding without checking an artifact,
retrying an unproductive pattern, or losing the source of truth in long context.
These surfaces make the skill enforce its own stated philosophy.

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

The `reflect` command records nothing new of its own — it **routes** the reflection into the existing surfaces so the right reader acts on each piece:

- **Learning** (`lesson:`) → a general memory (§3 recall, §2 `mine-weakness` when given `failure_signature:`).
- **Repo/code fix** (`fix_repo:`) → a refinement (`quality:good` if outcome=worked, `quality:bad` if partial/failed) visible via `memory_refine_get` — the durable *"fix this here"* queue.
- **Harness improvement** (`fix_harness:`) → a `harness`-tagged memory that §4 `export-harness` surfaces for `AGENTS.md`/`CLAUDE.md`.

Use `--judgment-note` when the conclusion needs nuance: name checked evidence, remaining uncertainty, and why any eval/checklist prompt mattered.
Use `--duo` when the outcome is substantial, ambiguous, or likely to teach the harness.
`--duo` adds a `reflection_duo` packet with two advisory reviewer roles.
The packet is not stored, scored, or enforced.

For subagent-heavy work, pair `--judgment-note` with the compact evidence receipt from `agentic-flows.md`: scope, claims, evidence anchors, verification status, decision impact, and open questions.
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
  - **Pi:** `memory_verify({ intent_id: "...", status: "SUCCESS" })` or `memory_verify({ allPending: true })`
  - **Pi:** `file_lock({ type: "release", intent_id: "...", verified: true, verified_note: "ran yarn test, 273 passed" })`
  - **CLI:** `awareness.mjs verify --agent-id <a> --all-pending --message "command=... exit=0 evidence=<ref>"`
- A `VERIFIED` event is written to `intent_events`. If you release `--status SUCCESS`
  on an intent that declared a test-plan but recorded no verification, the response
  carries an `unverifiedConclusion` warning and stores the intent as `PENDING`.
  The post-edit hook also releases file locks as `PENDING`, so coordination is
  unblocked while verification remains auditable.
- The **Stop/SubagentStop hook** runs `memory_audit_unverified` and blocks conclusion once if any intent has no `VERIFIED` event. Loop-guarded; opt-out via `OCTOCODE_NO_VERIFY_GATE=1`.

`audit-unverified` returns two intent categories — both count toward the gate:
- **`unverified`** — `PENDING` intents: file lock released, but no verify call recorded. Normal post-edit state; run the declared test plan and call `memory_verify`.
- **`stale_active`** — `ACTIVE` intents whose file locks have all expired without an explicit release (orphaned by a crash or unexpected exit). Clear with `audit-unverified --abandon` (marks FAILED, records ABANDONED event) if the work was abandoned, or verify explicitly if the work was actually completed. Do not confuse stale-active with PENDING: PENDING means "released but unverified"; stale-active means "lock TTL expired before any release."

`memory_audit_unverified` (Pi) / `audit-unverified --agent-id <a>` (CLI) lists unverified intents; exits `1` when any exist.

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

Valid surfaces: `verify-gate`, `lock-conflict`, `fts-miss`, `briefing-miss`, `doc-drift`. Including the surface helps Stage 2 (`export-harness`) target the right file when generating a harness proposal. It does NOT change clustering: `mine-weakness` merges signatures that share the same `mechanism:X|cause:Y` base (stripping `|surface:Z`) into one cluster and reports the distinct surfaces found. A cluster with `surfaces: ["verify-gate"]` means every failure in it hit the verify gate — strong signal for which harness surface to fix.

`mine-weakness [--min-count N] [--limit N]` (CLI only — deliberately not a Pi tool)
clusters memories by `failure_signature` (base only, surface stripped) and ranks each
cluster by **support × avg-importance**, with up to three example observations. Clusters need
`--min-count` occurrences (default 2) — one-off failures stay out of the view. A Jaccard diversity
filter (threshold 0.5) prevents the output from showing N variants of the same mechanism; each
returned cluster covers a distinct failure pattern. This turns N anecdotal "failed again" rows
into one ranked recurring-mechanism record.
Exact-signature grouping is brittle on free text, so signatures power *this view only*
— general recall still uses FTS5 + decay (below).

## 3. Salience-decayed recall

`get-memory` ranks by a blend, not importance alone:

```
recency    = exp(-ln2 * age_days / half_life)   # age from LAST USE — re-use keeps it fresh
importance = importance_score / 10
access     = log1p(access_count) / log1p(50)     # saturating
relevance  = query match, normalized to 0..1    # the "lexical" weight slot
final = 0.25*importance + 0.30*recency + 0.15*access + 0.30*relevance
```

The `relevance` term (the `lexical` weight) is normalized to `0..1` so the weights
mean what they say:
- **lexical** (the CLI's only mode): FTS5 `bm25` normalized as `bm25 / (poolMax + 1)` —
  a weak-pool guard, so the best hit of an all-weak pool no longer inflates to `1.0`
  (strong matches land ~0.6–0.9). When the pool's bm25 is degenerate (near-empty store
  or a term present in every row, IDF collapse) relevance is neutral `0.5`; same for
  the no-FTS fallback. Inspect with `--explain`.
- **`judgment_required`**: zero results, FTS-fallback mode, or a top relevance below
  `0.35` sets `judgment_required: true` + `judgment_reason` on the response — recall
  is a lead, not an answer; verify before relying on it.
- **semantic** exists only at the library level: `storeEmbedding()` persists vectors
  (the embedding source — API or local model — is the caller's responsibility) and
  `semanticSearch()` ranks by cosine similarity. The CLI has no embedding source, so
  `get-memory --semantic` returns lexical results plus an explicit warning — it never
  pretends to be semantic.

Every recall bumps `access_count` + `last_accessed_at` for the rows it returns, so
frequently-useful lessons stay near the top. Half-life is per-memory
(`decay_half_life_days`), defaulted by label at write time: durable labels
(`DECISION`/`ARCHITECTURE`/`SECURITY`/`GOTCHA`) 90d, `EXPERIENCE` reflections 14d,
everything else 30d. Flags: `--sort` (`smart`/`score`, `importance`, `recent`,
`accessed`), `--explain` (emit `score_components` per result — use it to tune).
Older databases migrate automatically.

## 4. Refine the harness — the loop's last step

A recurring lesson should stop being "might recall" and become standing guidance.
`export-harness` (CLI) previews lessons for `AGENTS.md` / `CLAUDE.md` in two tiers:
- **Tier 1** (always first): `harness`-tagged memories from `memory_reflect fix_harness:`.
- **Tier 2**: high-importance general memories (importance ≥ 7, label ≠ EXPERIENCE).
`export-harness` is preview-only and never writes files.

### Harness improvement gate

Valid surfaces include `AGENTS.md`/`CLAUDE.md`, docs/READMEs/reference files, standing
memory-corpus changes, and this skill's prompts, hooks, scripts, schemas, and tests.
Propose changes when the user asks, or when evidence shows a repeated failure/opportunity:
`mine-weakness`, user correction, eval failure, unverified intent, recurring gotcha, or a missing check.

Before applying any harness change, ask the user with a concrete fix request:
- Target surface/files.
- Observed correction or opportunity.
- Why future agents need it; name the failure or decision it changes.
- Evidence source: memory/refinement/eval/user correction/file.
- Proposed change, risk/rollback, and verification plan.

Until approved, keep the proposal in conversation or a proposal-only refinement; do not
edit files or add/supersede/prune standing harness memories. Use `memory_reflect fix_harness:`
only when the user asks for or approves durable proposal capture. One approval covers only
the scoped change being discussed; never treat it as blanket permission.

## Hard NOs

- No **unattended** self-modifying loop. An agent may edit the skill **only** after
  explicit human approval for the scoped change — never silently, never on `main`,
  never auto-merged.
- No automatic prompt rewrite from failed binary questions or advisory `agenticEval`
  prompts. They are evidence for `memory_reflect`, `mine-weakness` (CLI), and human-reviewed
  harness proposals.
- No numeric regression-gate infrastructure (held-in/held-out splits + verifier
  services) — out of scope for a service-free local skill. Flag regressions in notes instead.
- Failure signatures are for the weakness view only — never the sole recall path.
