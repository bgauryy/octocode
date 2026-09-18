# Reasoning-loop benchmark

Load before claiming better decisions, lower authoring cost, or calibrated policy. Contract validity and one successful recovery do not prove comparative efficacy.

## Frozen suites

`evals/kpi-contract.json` owns goals and gates.

```sh
node scripts/eval-decision-loop.mjs
node scripts/eval-run-loop.mjs
node scripts/eval-recovery-heldout.mjs
```

- `decision-cases.json`: 15 deterministic routing/policy regressions.
- `run-loop-cases.json`: observed legacy boilerplate versus compact runner. It measures author-input bytes, lobby words, command count, packet equivalence, APPLY behavior, and extra API calls.
- `recovery-heldout.json`: synthetic wrong-lean and confirming controls, frozen before its first live run. The default command validates packets without network access.

Run semantic characterization only with an authorized Jev key and fixed model:

```sh
node scripts/eval-recovery-heldout.mjs --live \
  --output <workspace>/.octocode/octocode-jev-reasoning-loop/benchmark/recovery-heldout-v1
```

The live runner records initial lead, evidence delta, final lead, claim status, **Wrong-Lean Recovery Rate**, false-recovery rate, unsupported-claim rate, calls, and input/output tokens. A wrong lean recovers when contradictory evidence causes abandonment or reframing before claim assertion. A confirming control is a false recovery when the loop abandons a correct lead despite confirming evidence.

## Decision boundaries

Accept the compact runner only when author-input and lobby reductions meet the frozen floor, route packets remain equivalent except for intentionally minimal reasoning summaries, APPLY stays provisional, no extra API call is added, and all regression checks pass.

Treat the live held-out result as **absolute characterization** until a matched host-only baseline uses identical tasks, evidence, model, retrieval budget, and grader. Comparative semantic acceptance still requires Wrong-Lean Recovery Rate improvement without worse false recovery, unsupported claims, correctness, or cost.

Do not tune on held-out outcomes. If the sensor population is too small because Jev never takes the attractive wrong lead, mark the run INVALID and rotate new cases between experiments rather than rewriting the frozen run. Preserve request, response, APPLY, and report artifacts under the output directory.

Next: an ACCEPT/REVERT report must distinguish deterministic ergonomics evidence, absolute semantic characterization, and comparative efficacy.
