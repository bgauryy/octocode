# Skill maintenance smoke checks

`cases.json` holds public prompts and lexical grader checks; `trigger-cases.json` holds activation examples.

Run from the skill root:
```bash
node scripts/eval-skill.mjs --case define-kpi --input <answer-file>
node scripts/eval-skill.mjs --batch <answer-directory>
```

Batch grading requires an answer for every expected case; a missing answer fails the batch. Use an explicit case for a deliberate subset. Ad-hoc checks can remain in the terminal; saved runs follow `benchmarks/README.md`.
