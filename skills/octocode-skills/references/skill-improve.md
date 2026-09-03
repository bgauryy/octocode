# Skill improve protocol

Load when improving, refactoring, or rewriting an Agent Skill. Why: preserve its job while making the folder leaner and easier to navigate.

## Inspect first

1. Read the full target `SKILL.md` and every behavior-affecting `references/`, `scripts/`, `assets/` file.
2. Run `scripts/skill-review.mjs <skill-dir>` first (`references/skill-review.md`).
3. Understand real files — never rewrite from a summary.

## Preserve ownership

- Put every workflow, hard rule, stop condition, and route table in `SKILL.md` — that file is the lobby agents always see.
- Refs hold one concept of depth only; they never redefine the main flow.
- Load **one** ref (or script) at a time; follow its `Next:` only when that step needs it.

- Keep one owner per concept. Cross-link instead of restating workflows or paragraphs.
- Prefer fewer, sharper references over parallel near-duplicates.

## Target shape

- Description: strong `Use when …` triggers (≤1024 chars; lead with the when-clause).
- Every capability: same-line **when** + **why** to a ref or script.
- Refs: one short H1, one concept, ≤50 lines. Skill→ref and ref→ref OK.
- Scripts: deterministic work; list each with when/why.
- Outputs: chat stays in chat; generated artifacts stay under `<workspace>/.octocode/`; source/install/config mutations keep their approved targets; no user-level artifact fallback.

## Improve loop

`READ → MAP INTENT → RATE → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

Preserve core job → score through `references/quality-rubric.md` → remove overlaps → split bloat → prune orphans (`references/skill-cleanup.md`) → re-review to 0 ERROR → report residual risk.

Done after you inspect the real files and preserve intent. Keep navigation intact. Remove dead material and duplicates. Finish with 0 ERROR.

Next: when pruning orphans load `references/skill-cleanup.md`; when picking rate vs rewrite mode load `references/self-improvement.md`.
