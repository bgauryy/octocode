# Eval Harness
Load when adding or extending machine-checkable suites in this monorepo. Why: deterministic floors beat prose claims.

## Octocode pattern
```text
evals/cases.json   # tasks + required/forbidden patterns + optional binaryQuestions
scripts/eval-*.mjs # grader: score patterns, citations, self-test samples
```
Existing examples: `octocode-research`, `octocode-brainstorming`, `octocode-rfc-generator`.

## Case shape (minimum)
- `id`, `prompt` or mode, `minScore`
- `required[]` / `forbidden[]` with named regex checks
- Optional `binaryQuestions[]`: `id`, `dimension`, `question`, `passPattern`, `failureSignature`, `suggestedLesson`
- Optional `--agentic` path: emit advisory questions without changing pass/fail

## Rules
1. Cases come from **real failures** and manual release checks (20–50 is enough to start).
2. Two experts should agree on pass/fail; include a **reference solution** that passes.
3. Do not edit cases to make a bad change pass — fix the subject or discard.
4. Keep CI floor deterministic; put semantic/LLM layers above it as advisory or calibrated judges.
5. Isolate trials; same command for baseline and candidate.

## This skill’s scripts
- `scripts/eval-eval.mjs` — grade answers for this skill’s cases
- `scripts/loop-report.mjs` — structural check that a loop report is complete

Next: held-out split → `held-out-and-guards.md`; close improve cycle → `improve-loop.md`.
