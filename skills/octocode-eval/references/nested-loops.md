# Nested Loops
Load when choosing which improvement loop to run. Why: one flat loop conflates experiment, suite health, and meta-policy.

## Three loops
```text
1) EXPERIMENT LOOP (inner, fast)
   baseline → mutate subject → measure → keep|discard
   Owner: agent under a frozen harness
   KPI: primary metric this run

2) SUITE LOOP (middle)
   error-analyze traces → add/fix tasks → rebalance capability/regression
   Owner: human + agent; harness *may* grow (new cases), never to pass a bad subject
   KPI: coverage of top failure modes; regression stay-green rate

3) META / HARNESS LOOP (outer)
   improve program.md / skill / graders / budgets
   Owner: human gate; held-out review required
   KPI: fewer repeated failure signatures; skill-review / thesis guards
```

## Coupling rules
- Never “fix” a failing experiment by editing graders mid-run (cheating).
- Suite growth is allowed **between** experiments, from real failures — not to greenwash.
- Meta changes require a new baseline and held-out VERIFY (`improve-loop.md`).
- Stop inner loop when flat; escalate to suite (missing cases) or meta (bad program).

## Mapping to Octocode
| Loop | Typical actuators |
|---|---|
| Experiment | one file/prompt/skill paragraph |
| Suite | `evals/cases.json`, binaryQuestions, failureSignatures |
| Meta | skill lobby/refs, Awareness harness proposals (human apply) |

Next: run inner loop → `agent-loop.md`; grow suite → `error-analysis.md` + `eval-harness.md`.
