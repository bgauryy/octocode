# Octocode Eval Benchmark

Design evals, calibrate judges, and compare agent or workflow changes without confusing leakage or noise with improvement. Use ordinary tests directly when they already establish the required outcome.

The skill follows: frame → validate harness → baseline → develop → sealed verify → decide → learn. `SKILL.md` routes the detailed guidance.

## Benchmarks and results

[benchmarks/README.md](benchmarks/README.md) owns the shared structure:
- [document-answering](benchmarks/document-answering/README.md): public questions, source fixtures, worker/judge/optimizer instructions and grading examples.
- [skill-smoke](benchmarks/skill-smoke/README.md): existing maintenance fixtures and commands.

Save each execution under `<workspace>/.octocode/benchmarks/<name>/results/<run-id>/` (home fallback for projectless work). Preserve effective settings, outcomes and evidence; use native runner logs where available. Runtime results stay outside the installed skill and outside worker access.

The examples are development data, not a private benchmark or implemented runner. Keep evaluator questions, answer keys and prior-trial results out of scored workers' context and tools. Task requirements remain visible.

Use `references/failure-repair.md` to diagnose task, grader, environment, leakage or solver defects before choosing a fix. `references/references.md` links the research behind the guidance.

## Install and maintain

```bash
npx -y octocode skill install octocode-eval-benchmark
node scripts/check-description.mjs
node scripts/eval-skill.mjs --self-test
```

Run the local checks from the skill root and use `octocode-skills` to review folder structure. Lexical fixture checks do not prove semantic quality or agent performance.
