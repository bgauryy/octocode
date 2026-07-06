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

- **Learning** (`lesson:`) → a general memory (§3 recall, §2 `memory_mine_weakness` when given `failure_signature:`).
- **Repo/code fix** (`fix_repo:`) → a refinement (`quality:good` if outcome=worked, `quality:bad` if partial/failed) visible via `memory_refine_get` — the durable *"fix this here"* queue.
- **Harness improvement** (`fix_harness:`) → a `harness`-tagged memory that §4 `memory_export_harness` surfaces for `AGENTS.md`/`CLAUDE.md`.

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

`memory_audit_unverified` (Pi) / `audit-unverified --agent-id <a>` (CLI) lists unverified intents; exits `1` when any exist.

## 2. Mine recurring failures

Tag a failure with a stable signature when you record it:

```bash
memory_record({ ..., failure_signature: "mechanism:retry-loop|cause:test-timeout" })  # Pi
# CLI: awareness.mjs tell-memory ... --failure-signature "mechanism:retry-loop|cause:test-timeout"
```

`memory_mine_weakness` (Pi) / `mine-weakness [--limit N]` (CLI) clusters memories by `failure_signature` and ranks each
cluster by **support × avg-importance**, with up to three example observations. This
turns N anecdotal "failed again" rows into one ranked recurring-mechanism record.
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

The `relevance` term (the `lexical` weight) is filled differently per mode, but is
always normalized to `0..1` so the weights mean what they say:
- **lexical** (default): FTS5 `bm25` squashed monotonically via `rel/(1+rel)` (or term-hit
  ratio on the no-FTS fallback). *Earlier builds mis-normalized this to a constant `1.0`,
  which silently removed lexical relevance from ranking — fixed; verify with `--explain`.*
- **semantic** (`--semantic`, needs `embed-index`): cosine similarity over stored vectors,
  **min-max normalized across the candidate pool** so the most-similar memory scores `1.0`
  and the least `0.0`. `--explain` shows both raw `semantic` (cosine) and `semantic_norm`.
  Static-embedding cosines bunch in a narrow band, so the normalization is what makes
  similarity actually reorder results; decay then re-ranks within.

Every recall bumps `access_count` + `last_accessed_at` for the rows it returns, so
frequently-useful lessons stay near the top. Flags: `--no-decay` (importance+relevance
only), `--half-life <days>` (default 30), `--sort` (`smart`/`score`/`recent`/…),
`--explain` (emit `score_components` per result — use it to tune). Older databases
migrate automatically.

## 4. Refine the harness — the loop's last step

A recurring lesson should stop being "might recall" and become standing guidance.
`memory_export_harness` (Pi) / `export-harness` (CLI) previews lessons for `AGENTS.md` / `CLAUDE.md` in two tiers:
- **Tier 1** (always first): `harness`-tagged memories from `memory_reflect fix_harness:`.
- **Tier 2**: high-importance general memories (importance ≥ 7, label ≠ EXPERIENCE).
`memory_export_harness` is preview-only and never writes files.

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
  prompts. They are evidence for `memory_reflect`, `memory_mine_weakness`, and human-reviewed
  harness proposals.
- No numeric regression-gate infrastructure (held-in/held-out splits + verifier
  services) — out of scope for a service-free local skill. Flag regressions in notes instead.
- Failure signatures are for the weakness view only — never the sole recall path.
