# Reporting results
Load when presenting an evaluation. Why: show what changed and how far the evidence supports the conclusion.

Use the shortest useful format. Include the goal, baseline versus candidate result, relevant guardrails, scope/budget, checks and evidence, uncertainty/coverage, and verdict. Headings, section order and exact words are not grading criteria.

For a saved run, place the summary and evidence under `<output>/benchmarks/<name>/results/<run-id>/` as described in `benchmarks/README.md`. Reference native logs rather than copying them. Preserve failed candidates, missing answers, errors, Unknowns, retries and costs alongside successes.

Distinguish source review, maintenance checks, development measurements and sealed behavioral comparisons. Development KEEP is provisional; use `references/held-out-and-guards.md` for acceptance, rejection, inconclusive and invalid evidence. Explain why a loop stopped or changed direction when that affects interpretation.

For before/after ratings, define dimensions and score anchors before editing. Label editorial ratings as subjective and link them to concrete defects; never translate them into measured performance gains. Say when behavior remains unmeasured.

Capture reusable lessons only when supported by the results; a short failure category can help later diagnosis. The report is complete when the reader can assess the result and its limits—not when it satisfies a heading checker.
