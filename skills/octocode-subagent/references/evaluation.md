# Evaluation And TDD

Load when EXECUTE or VERIFY changes behavior, compares orchestration strategy, or claims improvement beyond ordinary ship checks.

- TDD: select or write a failing behavioral case before you change code or instructions. Make the smallest change; rerun that case, then proportionate regression checks. Renames, explanations, read-only audits, and ordinary config edits may use an existing focused check.
- `octocode-eval-benchmark` owns KPI, held-out, keep/discard, and multi-agent measurement (graph-boundary outcome; worker metrics are guardrails). Load it when available.
- Without it, record before strategy mutation: the requester-visible goal, one primary KPI with baseline and target, up to three leading indicators, a fixed trial/token/time budget, counter-metric guardrails, held-out cases, a binary accept/revert rule.
- Never edit cases or graders mid-experiment. Prefer deterministic anchors (test exits, types, builds, schemas); use a fresh-context critic only for judgment they cannot measure.
- No improvement claim without comparable evidence.

Next: `references/completion.md` for acceptance; if shared state affected the run, `references/shared-work.md` before closing.
