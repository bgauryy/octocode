# Evaluate workflow benefit

Load before claiming better decisions, lower cost, or calibrated thresholds. Contract tests and a successful live call do not establish comparative benefit.

## Runnable checks

`npm test` covers transport, request validation, routing, provisional application, source bounds, and batching. `scripts/verify-reasoning-loop.mjs` checks the request contracts and native dry-runs. `scripts/eval-decision-loop.mjs` consumes `evals/decision-cases.json`; `scripts/eval-content-ref.mjs` checks evidence identity and authoring cost.

For semantic recovery, `scripts/eval-recovery-heldout.mjs` consumes `evals/recovery-heldout.json`. Its default mode validates packets offline. With a key loaded and a fixed model, run:

```sh
node <skill-dir>/scripts/eval-recovery-heldout.mjs --live --output <workspace>/.octocode/octocode-jev-reasoning-loop/benchmark/recovery
```

`evals/kpi-contract.json` defines the recovery metrics and guardrails. Preserve requests, responses, and results under the workspace artifact root, outside the skill. An absolute live recovery rate is characterization until compared with a matched host-only baseline.

## Compare the actual workflow

Freeze representative tasks, labels, model version, retrieval budget, and grading before measuring. Include relevant and irrelevant files, incomplete excerpts, wrappers, negation, ambiguous claims, and confirming controls. Keep deterministic-driver and Jev-assisted-driver ergonomics comparable so orchestration savings are not mistaken for model benefit.

Measure final correctness, relevant-source recall and false skips, unsupported claims, elapsed time, and both host-model and Jev token usage. Calibrate each question/primitive on its own cases; do not transfer thresholds between unrelated tasks or tune on held-out results. A model, prompt, or policy change requires fresh confirmation.

Keep historical answers, run logs, grading manifests, and one-off migration benchmarks in the workspace evaluation artifacts. They are not operating instructions or runtime fixtures.
