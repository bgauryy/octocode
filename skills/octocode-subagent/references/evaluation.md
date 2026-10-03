# Evaluation And TDD

Load when EXECUTE or VERIFY changes behavior, compares orchestration strategy, or claims improvement beyond ordinary ship checks.

## TDD

- Select or write a failing behavioral case before you change code or instructions.
- Make the smallest change. Run the same case, then proportionate regression checks.
- Renames, explanations, read-only audits, and ordinary configuration edits can use an existing focused check.

## Improvement claims

- `octocode-eval-benchmark` owns KPI, held-out, keep/discard, and multi-agent measurement (graph-boundary outcome; worker metrics are guardrails). Load it when available.
- Without it, before strategy mutation record: the requester-visible goal, one primary KPI with baseline and target, up to three leading indicators, a fixed trial/token/time budget, counter-metric guardrails, held-out cases, and a binary accept/revert rule.
- Do not edit cases or graders during the experiment. Prefer deterministic anchors (test exits, types, builds, schemas); use a fresh-context critic only for judgment they cannot measure.
- Do not claim improvement without comparable evidence.

Next: load `references/completion.md` for acceptance; if shared state affected the run, load `references/shared-work.md` before closing.
