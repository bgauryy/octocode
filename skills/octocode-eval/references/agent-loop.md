# Agent Loop
Load when running the **inner experiment** loop. Why: hill-climb with a frozen harness.

For suite vs meta loops, load `nested-loops.md` first.

## Contract
```text
BASELINE → MUTATE (one subject) → MEASURE (fixed budget) → KEEP | DISCARD → REPEAT
```

**TDD mapping (same loop, agent-shaped):**
1. **Red** — pick/add a failing case or record baseline below target (held-out untouched).
2. **Green** — smallest subject change; re-run the **same** command until primary improves.
3. **Refactor / discard** — keep only if guardrails hold; else revert; grow suite **between** experiments only.

1. Freeze the eval harness (cases, graders, prepare scripts) for this experiment.
2. Record baseline under the same budget and command.
3. Make the **smallest** change to the subject (not the harness).
4. Re-measure with the same command and budget.
5. **Keep** only if primary improves and guardrails hold; else **discard**.
6. Log: id · metric · status · one-line hypothesis.
7. Do not pause mid-loop for permission unless the user interrupted.

## Stop gates
- Primary flat across N trials with no new hypothesis → escalate to suite/meta
- Guardrail breach · env flakiness · user interrupt

## Isolation
Each trial starts clean. Shared state or peeking at prior-trial artifacts corrupts independence.

## Creativity vs path grading
Grade **outcomes** over exact tool-call sequences.

Next: graders → `eval-techniques.md`; overfitting → `held-out-and-guards.md`.
