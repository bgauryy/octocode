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
  [--lesson "<reusable learning>" --failure-signature "<sig>"] \
  [--fix-repo "<fix this in the code>" --fix-file <path>] \
  [--fix-harness "<improve this skill>"]
```

It records nothing new of its own — it **routes** the reflection into the existing surfaces so the right reader acts on each piece:

- **Learning** (`--lesson`) → a general memory (§3 recall, §2 `mine-weakness` when given `--failure-signature`).
- **Repo/code fix** (`--fix-repo`) → an open, `quality:bad` refinement the next agent sees via `refine-get` — your durable *"fix this here"* indication, stored with the repo.
- **Harness improvement** (`--fix-harness`) → a `harness`-tagged memory that §4 `export-harness` surfaces for `AGENTS.md`/`CLAUDE.md`.

So the everyday flow is: **do the work → `verify` it (§1) → `reflect` on it (worked/didn't) → fixes land where they'll be picked up → a human merges (§4).** Everything in §1–4 below is what `reflect` feeds.

## 1. Validate before you conclude

The flagship failure class is declaring success without checking the artifact.

- At `pre-flight-intent` you already declare a `--test-plan`. After doing the work,
  **run it and record that it ran**:
  - `verify --agent-id <a> --intent-id <id> --message "yarn test: 273 passed"`, or
  - `verify --agent-id <a> --workspace "$PWD" --all-pending --message "yarn test: 273 passed"` after hook-managed edits, or
  - `release-file-lock ... --status SUCCESS --verified --verified-note "ran it"`.
- A `VERIFIED` event is written to `intent_events`. If you release `--status SUCCESS`
  on an intent that declared a test-plan but recorded no verification, the response
  carries an `unverifiedConclusion` warning and stores the intent as `PENDING`.
  The post-edit hook also releases file locks as `PENDING`, so coordination is
  unblocked while verification remains auditable.
- The **Stop / SubagentStop hook** (`hooks/stop-verify.sh`) runs `audit-unverified`
  for your session and blocks the conclusion **once** with a reminder if any active
  or pending intent has a test-plan but no `VERIFIED` event. It is loop-guarded
  (`stop_hook_active`) and opt-out via `OCTOCODE_NO_VERIFY_GATE=1`.

`audit-unverified [--agent-id <a>]` lists intents missing verification and exits `1`
when any exist (so it composes in scripts/hooks).

## 2. Mine recurring failures

Tag a failure with a stable signature when you record it:

```bash
tell-memory ... --failure-signature "mechanism:retry-loop|cause:test-timeout"
```

`mine-weakness [--limit N]` clusters memories by `failure_signature` and ranks each
cluster by **support × max-importance**, with up to three example observations. This
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

A lesson that keeps recurring should stop being "might recall" and become a standing
instruction. `export-harness [--min-importance N] [--limit N]` previews the top
recurring **general** (file-less) lessons as a Markdown block for `AGENTS.md` /
`CLAUDE.md`. It is **preview-only** — it prints the block and never writes harness
files.

### When to propose a refinement

Propose an improvement — **to the repo's code, or to this harness/skill itself**
(prompts, hooks, checks, references, the `awareness.py` surfaces) — in either case:

- **The user asks** for it, or
- **You sense one is needed** — e.g. `mine-weakness` shows a recurring mechanism, a
  check is missing, a step is flaky, the same gotcha keeps biting, or a lesson is
  important and general enough to belong in `AGENTS.md`.

Discipline: **mine → propose-as-note → a human merges.** Surface the proposal (and,
for standing lessons, the `export-harness` preview); let the user decide.

### Applying a harness fix — `harness-apply` (gated, branch-only)

An agent **may** edit the skill itself (prompts, hooks, checks, `awareness.py`,
references) — but only through a gate, never unattended:

1. **Human opens the gate**: a person exports `OCTOCODE_ALLOW_HARNESS_APPLY=1` for
   the session (the approval signal).
2. **Dedicated branch**: the skill's repo must be on a branch that is not
   `main`/`master` (override with `OCTOCODE_HARNESS_BRANCH_OK=1` for unusual setups),
   so the change is isolated and reversible.
3. **Announce + audit**: run `harness-apply --agent-id <a> --approved-by <human>
   --change "<summary>" --file <skill file…>`. It records a `HARNESS_APPLY` audit
   event, **broadcasts a `decision` notification** so the human and other agents know
   it's happening, and returns a `humanMessage` to surface.
4. **Edit on the branch, verify, open a diff/PR.** A human reviews and merges.

The **`harness-guard` PreToolUse hook enforces this**: it blocks any `Write`/`Edit`
to a file inside the skill's own directory (exit 2) unless the gate is open and the
branch check passes — the same mechanism `pre-edit.sh` uses for file locks, turned on
the harness itself. So "an agent can fix its own harness" is true *and* safe: gated,
branch-isolated, announced, human-merged.

## Hard NOs

- No **unattended** self-modifying loop. An agent may edit the skill **only** via the
  gated `harness-apply` path above (human-approved env + dedicated branch + announced
  + human-merged) — never silently, never on `main`, never auto-merged.
- No numeric regression-gate infrastructure (held-in/held-out splits + verifier
  services) — out of scope for a service-free local skill. Capture the conservative
  spirit by flagging regressions in notes, not by building a benchmark harness.
- Failure signatures are for the weakness view only — never the sole recall path.
