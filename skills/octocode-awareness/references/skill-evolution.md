# Skill Evolution

When Awareness skill guidance (or a closely related Agent Skill) must change with evidence.

Paper: [SkillOpt](https://arxiv.org/abs/2605.23904) (arXiv:2605.23904). Operator: edit the skill folder under repo-root `skills/`, then rebuild Awareness and run package tests.

## Model

Treat the skill folder as the **trainable external state** of a frozen agent. An optimizer pass turns scored trajectories into bounded add/delete/replace edits.

## Gates

| Gate | Rule |
|---|---|
| Held-out gate | Rate/review first; smoke the skill on a task **not** used to invent the edit. |
| User gate | Do not ship silent instruction changes without approval when the change is consequential. |

## Flow

```text
ATTEND → SET GOAL+KPI → RESEARCH → PLAN (bounded edits) → USER GATE → ACT → VALIDATE (actual checks) → REFLECT
```

1. **Attend** — claim/lock the skill paths you will edit.
2. **Research opportunities** — use `octocode-research` / `npx octocode` / MCP for evidence (`references/octocode.md`).
3. **Create or improve** — synthesize need → plan → approve → write lobby + one-concept refs → rebuild + package tests.
4. **Validate** — held-out prompt or smoke; revert if the KPI did not move.
5. **Reflect** — record only verified reusable lessons.

## Rules

- Keep Awareness for attend/lock/verify/reflect; use Research for evidence while changing skill text.
- Never hand-edit `out/skills/` or `.agents/skills/` mirrors — rebuild instead.
