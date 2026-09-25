# Agent loop
Load for subject improvement with a validated frozen harness. Why: keep development search separate from final confirmation.

```text
BASELINE → HYPOTHESIS → ONE SUBJECT CHANGE → DEVELOPMENT MEASURE → KEEP | DISCARD
                                                               ↓ selected candidate
                                                        SEALED VERIFY → DECIDE
```

1. Record the development baseline, harness hashes, comparable budget, and a failing case or below-target outcome before changing the subject.
2. State the failure mechanism and predicted effect. Make the smallest change that tests that hypothesis; one coherent factor aids attribution.
3. Run baseline/candidate comparisons under the frozen plan. Keep raw outcomes, cost, error categories, and all attempted hypotheses.
4. KEEP a development candidate only when the selection rule and guardrails hold; otherwise DISCARD its targeted changes. Never revert unrelated workspace edits.
5. Stop at the declared budget, target, or no-new-hypothesis condition. Inspect development failures for sensor defects or missing coverage before escalating to suite/meta work via `nested-loops.md`.
6. Select the candidate before sealed verification. Apply `held-out-and-guards.md` once; capture lessons only after the verdict.

TDD means a failing development check before a fix, then the same check after it. Repeatedly repairing against final-test feedback is test-set training, not TDD. New cases or grader fixes start a new experiment version.

Trial and role isolation belongs to `clean-lab.md`. Authorized parallel hypotheses use separate subject copies and development trials; comparing many candidates increases selection bias and consumes the declared budget. Never pick the luckiest attempt and report it as pass@1.

Before iterating, verify the sensor measures the intended outcome at useful cost. If noise obscures the expected effect, improve measurement first. A cheaper leading proxy can guide development, but final verification still measures the user-visible outcome.

Next: change loop level → `nested-loops.md`; report → `output.md`.
