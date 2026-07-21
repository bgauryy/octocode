# Self-Reflection Dialogue

Use role dialogue when an important, fuzzy, risky, or creative idea needs challenge.
Skip routine edits, status checks, and obvious verification.
Use `references/subagent-rubber-duck.md` for a real second agent; loop closure lives in `references/learning-loop.md`.

## Pattern

```text
QUESTION -> ROLE A -> ROLE B CHALLENGE -> SYNTHESIS -> VERIFY -> CAPTURE
```

Use two temporary lenses. Add a third only for a distinct user-approved job such as security or migration.

| Pair | Use when | Tension |
|---|---|---|
| Tutor / Student | Teaching/onboarding | clarity vs confusion |
| Builder / Tester | Designing behavior | capability vs failure modes |
| Supporter / Skeptic | Post-task lesson/tradeoff | value vs unverified claims |
| Historian / Futurist | Long-lived docs/memory | precedent vs future fit |
| User Advocate / Maintainer | UX/workflow | usefulness vs maintenance |
| Compression / Recall | Context-heavy work | brevity vs missing evidence |

## Awareness Form

Run `attend` before a hard judgment. For post-task learning, `reflect record --duo` returns advisory supporter/skeptic prompts; it does not spawn a subagent, store role output, or score it.

Use internal roles for a quick bounded challenge. Use a real rubber-duck subagent when independent source inspection, restatement, or assumption checking materially reduces risk.

Use the prompts for one pass: name what improved, one remaining uncertainty, and one concrete check. Structured eval failures belong in `--eval-failure-json` with a stable `failure_signature`; recurring patterns belong in `reflect mine-weakness`.

Publish a signal/refinement only when another run needs the unresolved question. Record memory only after synthesis is reusable, scoped, and verified.

## Guardrails

- Roles are temporary thinking lenses, not a persistent persona.
- Agreement is not proof; verify against source, tests, commands, or user feedback.
- Preserve dissent when evidence is missing; never claim fake consensus.
- Capture synthesis and next check, not raw dialogue.
- Keep one question, two roles, one pass, and one next action.
- Do not inject projections for ordinary brainstorming.

Output: question, roles, best arguments, synthesis, dissent, evidence/check, next action, and capture choice (`none|memory|refinement|signal`). Then follow `references/learning-loop.md` until the chosen output is applied, verified, and closed.
