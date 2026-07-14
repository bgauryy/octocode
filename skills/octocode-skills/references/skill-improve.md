# Skill Improve Protocol

Load when improving, refactoring, or rewriting any Agent Skill — including this one. Why: fixed contract so other skills get the same lean, non-overlapping map.

## Before any edit

1. Read the full target `SKILL.md` and every behavior-affecting `references/`, `scripts/`, `assets/` file.
2. Run `scripts/skill-review.mjs <skill-dir>` first (`references/skill-review.md`).
3. Understand real files — never rewrite from a summary.

## Lobby (`SKILL.md`) owns the flow

- Put every workflow, hard rule, stop condition, and route table in `SKILL.md` — that file is the lobby agents always see.
- Refs hold one concept of depth only; they never redefine the main flow.
- Load **one** ref (or script) at a time; follow its `Next:` only when that step needs it.

## No duplicate / no overlap

- One owner per concept: if two files teach the same rule, keep one and cross-link.
- Do not restate `SKILL.md` workflow steps inside refs.
- Do not copy the same paragraph across refs — link instead (`duplicate-content` review rule).
- Prefer fewer, sharper refs over parallel near-duplicates.

## Target shape

- Description: strong `Use when …` triggers (≤1024 chars; lead with the when-clause).
- Every capability: same-line **when** + **why** to a ref or script.
- Refs: one short H1, one concept, ≤50 lines. Skill→ref and ref→ref OK.
- Scripts: deterministic work; list each with when/why.

## Improve loop

`READ → MAP INTENT → RATE → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

Preserve core job → score via `references/quality-rubric.md` → remove overlaps → split bloat → prune orphans (`references/skill-cleanup.md`) → re-review to 0 ERROR → report residual risk.

Done only when: files were read, lobby has the workflows, routes state when/why, no concept overlap, standalone folder has no dead files, refs ≤50 one-concept, review 0 ERROR.

Next: when pruning orphans load `references/skill-cleanup.md`; when picking rate vs rewrite mode load `references/self-improvement.md`.
