# Octocode Eval Benchmark

Design evals, calibrate judges, and compare agent or workflow changes without mistaking leakage or noise for improvement. Use ordinary tests directly when they already prove the outcome.

`SKILL.md` holds the flow and rules.

- [benchmarks/README.md](benchmarks/README.md): shared layout and run lifecycle.
- [document-answering](benchmarks/document-answering/README.md): public development starter (questions, fixtures, worker/judge/optimizer instructions). Development data, not a private benchmark or runner.
- [skill-smoke](benchmarks/skill-smoke/README.md): maintenance fixtures and commands.

```bash
npx -y octocode skill install octocode-eval-benchmark
node scripts/check-description.mjs
node scripts/eval-skill.mjs --self-test
```

Run the checks from the skill root; use `octocode-skills` to review folder structure.
