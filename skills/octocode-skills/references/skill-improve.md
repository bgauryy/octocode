# Skill improve protocol

Load when improving, refactoring, or rewriting an Agent Skill. Why: preserve its job while making the folder leaner and easier to navigate.

## Inspect first

1. Read the target `SKILL.md`, inventory its files, and read the references, scripts, and assets that affect the requested change. For a whole-skill review, inspect every behavioral route; report any unexamined surface.
2. Run `scripts/skill-review.mjs <skill-dir>` first (`references/skill-review.md`).
3. Understand real files — never rewrite from a summary.

## Preserve ownership

- Keep entry decisions, shared constraints, the tools/related-skill convention, and short reusable rules in `SKILL.md`; place only substantive conditional procedures in their owning references.
- References add detail without redefining shared rules.
- Reuse references already read. Batch independent reads when useful; follow a `Next:` route only when it resolves the current question.

- Keep one owner per concept. Cross-link instead of restating workflows or paragraphs.
- Prefer fewer, sharper references over parallel near-duplicates. Remove routing wrappers and fallback stubs whose effective instruction fits coherently in the lobby.

## Target shape

- Description: strong `Use when …` triggers (≤1024 chars; lead with the when-clause).
- Every capability: same-line **when** + **why** to a ref or script.
- Refs/docs: one complete conditional job with a clear entry condition. Aim for short files; review the 50-line advisory in context instead of splitting a coherent procedure just to meet a count. Skill→ref and ref→ref are valid routes only when the hop changes the next action.
- Scripts: deterministic work; list each with when/why, and retain it only when execution is more reliable or efficient than the equivalent lobby instruction.
- Outputs: chat stays in chat; repository/workspace artifacts stay under `<workspace>/.octocode/`, projectless or explicitly user-scoped artifacts stay under `<home>/.octocode/`, and source/install/config mutations keep their approved targets.

## Improve loop

`READ → MAP INTENT → RATE → DEDUPE → REWRITE → CLEANUP → REVIEW → VERIFY`

Preserve core job → score through `references/quality-rubric.md` → remove overlaps → split bloat → prune orphans (`references/skill-cleanup.md`) → re-review to 0 ERROR → report residual risk.

Done after you inspect the real files and preserve intent. Keep navigation intact. Remove dead material and duplicates. Finish with 0 ERROR.

Next: when pruning orphans load `references/skill-cleanup.md`; when picking rate vs rewrite mode load `references/self-improvement.md`.
