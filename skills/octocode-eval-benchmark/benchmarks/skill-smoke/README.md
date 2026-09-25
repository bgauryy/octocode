# Skill maintenance smoke checks

`cases.json` holds public prompts and lexical grader checks; `trigger-cases.json` holds activation examples. These fixtures and the grader's canned answers test maintenance behavior, not unseen agent performance or actual model activation.

Run from the skill root:
```bash
node scripts/check-description.mjs
node scripts/eval-skill.mjs --self-test
node scripts/eval-skill.mjs --case define-kpi --input <answer-file>
node scripts/eval-skill.mjs --batch <answer-directory>
```

Batch grading requires an answer for every expected case; missing answers stay in the denominator and fail the batch. Use an explicit case for a deliberate subset. Keep the source fixtures unchanged during a comparison and record any grader revision separately.

When saving a run, use `<output>/benchmarks/skill-smoke/results/<run-id>/` with the effective settings, checks and summary. Ad-hoc checks can remain in the terminal. `benchmarks/README.md` owns the common layout; do not create empty trial files for checks that never ran workers.
