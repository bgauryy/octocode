# Octocode Eval Benchmark

Design evals, calibrate judges, and compare agent or workflow changes without confusing leakage or noise with improvement. Use ordinary tests directly when they already establish the required outcome.

Flow: frame → validate harness → baseline → develop → sealed verify → decide → learn. `SKILL.md` routes the detailed guidance.

## Benchmarks and results

[benchmarks/README.md](benchmarks/README.md) owns the shared structure:
- [document-answering](benchmarks/document-answering/README.md): public questions, source fixtures, worker/judge/optimizer instructions, and grading examples.
- [skill-smoke](benchmarks/skill-smoke/README.md): maintenance fixtures and commands.

Save each run under `<workspace>/.octocode/benchmarks/<name>/results/<run-id>/` (home fallback for projectless work). Keep effective settings, outcomes, and evidence; use native runner logs where available. Keep results outside the installed skill and outside worker access.

The examples are development data, not a private benchmark or implemented runner. Keep evaluator questions, answer keys, and prior-trial results out of scored workers' context and tools. Task requirements stay visible.

## Install and maintain

```bash
npx -y octocode skill install octocode-eval-benchmark
node scripts/check-description.mjs
node scripts/eval-skill.mjs --self-test
```

Run the checks from the skill root; use `octocode-skills` to review folder structure. Lexical fixture checks do not prove semantic quality or agent performance.
