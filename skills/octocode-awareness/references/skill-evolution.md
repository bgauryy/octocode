# Skill Evolution (SkillOpt → Octocode)

Use when creating, improving, or updating Agent Skills from real rollouts — not one-shot rewrites.
Paper: [SkillOpt](https://arxiv.org/abs/2605.23904) (arXiv:2605.23904). Operator: sibling skill **`octocode-skills`** (bundled next to this skill; install via `npx octocode skill --add --path …/out/skills/octocode-skills`).

## Thesis (agent-usable)

Treat the skill folder as the **trainable external state** of a frozen agent. An optimizer pass (you + `octocode-skills`) turns scored trajectories into bounded add/delete/replace edits.
Accept only edits that improve a held-out check. Keep rejected proposals as learning evidence, consolidate occasionally, and keep the deployed skill compact.

| SkillOpt control | Do this here |
|---|---|
| Rollout batch | Real tasks with the current skill; capture failures + successes (not anecdotes alone). |
| Reflection minibatch | Cluster recurring procedural misses; prefer failure-driven fixes, preserve working rules. |
| Edit budget (textual LR) | Few localized patches per round — never unbounded rewrite of the whole skill. |
| Held-out gate | Rate/review first; run `skill-review.mjs`; smoke the skill on a task **not** used to invent the edit. |
| Rejected buffer | Record failed proposals (`reflect record --fix-harness`, `memory record`) so the next round avoids them. |
| Slow/meta update | After several epochs: `reflect mine-weakness` / `export-harness` / `--fix-instructions` — durable lessons, not lobby bloat. |

## Operator loop (create / improve / update)

Always load **`octocode-skills`** for the skill work; keep this skill for attend/lock/verify/reflect.
For goal+KPI improve loops grounded in **actual results** and thesis pressures, also follow local `references/improve-loop.md` (shared contract with octocode-research).

```text
ATTEND → SET GOAL+KPI → RESEARCH (octocode-skills) → PLAN (bounded edits) → USER GATE → ACT → REVIEW → VALIDATE (actual checks) → REFLECT
```

1. **Attend** — `attend`; recall prior harness lessons / rejected edits for this skill path.
2. **Research opportunities** — with `octocode-skills`: discover prior art, inspect real `SKILL.md` folders, and rate fit. Use `npx octocode` / MCP for code or GitHub evidence (`references/octocode.md`).
   For hard judgment use `references/self-reflection-dialogue.md`; for independent challenge use `references/subagent-rubber-duck.md`.
3. **Create** — `octocode-skills` → `create-local-skill.md`: synthesize need → plan → approve → write lobby + one-concept refs → `skill-review.mjs`.
4. **Improve / update** — `self-improvement.md` (mode gate) → `skill-improve.md` (READ→…→VERIFY). Prefer patch-mode: add/delete/replace one concept; lobby owns flow; refs stay ≤50 one-concept.
5. **Gate** — no write without user approval when the skill is shared; no accept without review **0 ERROR** and a held-out smoke (task outside the failure that motivated the edit).
6. **Reject path** — if smoke/review regresses, revert the patch, `memory record` / `reflect record --fix-harness` with what was tried and why it hurt, then propose a smaller edit.
7. **Ship** — prune orphans (`skill-cleanup.md`); install/refresh with `npx octocode skill --add --path <skill-dir> --platform <host> --force` (awareness + skills both from the awareness package `out/skills/`).

## Hard rules

- Do **not** one-shot regenerate a working skill from a summary — read every behavior-affecting file first.
- Do **not** treat a plausible diagnosis as an accepted edit — held-out validation is mandatory.
- Do **not** dump trajectory dumps into `SKILL.md` — procedural rules only; instance detail stays in memory/reflect.
- Research gaps and marketplace candidates through **`octocode-skills`**; coordination and learning history through **awareness**.

## When to stop

One clear create/improve path; two High-rated candidates → pick one; three research angles add nothing; or a user gate is pending. Export only the best accepted skill artifact.
