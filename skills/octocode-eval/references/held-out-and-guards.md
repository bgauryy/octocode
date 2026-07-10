# Held-Out And Guards
Load when accepting a change or expanding a suite. Why: overfitting and contamination are silent failures.

## Splits
| Set | Role |
|---|---|
| **Train / invent** | Failures you may study while designing the patch |
| **Held-out** | Never used to invent the edit; run only at JUDGE/VERIFY |
| **Regression** | Former capability tasks that must stay green |

If you peeked at held-out to craft the fix, rotate new cases.

## Contamination / leakage
Do not paste held-out items into prompts, RAG, or fine-tunes. Famous public benches are high-leakage risk — treat as orientation, not sole gate (`benchmarking.md`).

## Thesis / product guards (Octocode)
Remeasure thesis pressures when Awareness/harness is in play. Never hide omissions, skip verify, or silently mutate skills/AGENTS.

## Cheat resistance
- No rewriting graders mid-experiment to pass
- Passing must require solving the task
- ~0% pass@100 → debug task/grader first

## Accept rule
```text
ACCEPT iff primary(held-out) improves AND guardrails hold AND harness unchanged
else REVERT
```

Next: report → `output.md`; route → `routing.md`.
