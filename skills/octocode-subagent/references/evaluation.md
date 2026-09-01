# Evaluation And TDD

Load when EXECUTE or VERIFY changes behavior, compares orchestration strategy, or claims improvement beyond ordinary ship checks. Why: regression tests cover normal correctness; strategy claims need a frozen outcome comparison.

## TDD

Select or write a failing behavioral case before changing code or instructions. Make the smallest change, run the same case, then run proportionate regression checks. Renames, explanations, read-only audits, and ordinary configuration edits may use an existing focused check.

## Improvement contract

Before strategy mutation record the user-visible goal, one primary KPI with baseline and target, up to three leading indicators, fixed trial/token/time budget, counter-metric guardrails, held-out cases, and a binary accept/revert rule. Do not edit cases or graders during the experiment to make the subject pass.

Prefer deterministic anchors: test exits, type diagnostics, builds, schemas, and artifact inspection. Use a fresh-context critic only for judgment dimensions those checks cannot measure. For multi-agent work, measure the result at the graph boundary; worker scores, latency, tokens, packet completeness, collisions, and verifier freshness are leading metrics or guardrails.

Use `octocode-eval-benchmark` when available for the full goal→KPI and held-out workflow. Without it, keep the compact contract above and do not claim improvement without comparable evidence.

Next: load `references/completion.md` for acceptance; if shared state affected the run, load `references/awareness.md` before closing.
