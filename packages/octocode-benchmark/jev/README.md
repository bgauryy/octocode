# Terra research with and without Jev

This directory owns the Jev benchmark, moved from the runtime skill. The standalone `octocode-jev-reasoning-loop` skill remains in the repository's skills collection. Sealed historical artifacts retain the skill name and paths captured when those runs were frozen.

- [CONTRACT.md](CONTRACT.md): frozen Terra comparison design and grading rubric.
- [RESULTS.md](RESULTS.md): measured comparison, decision-attribution audit, and runtime-corrected bug grades.
- [ANSWERS.md](ANSWERS.md): all 22 original Terra answers, with and without Jev; proposed fixes are not automatically verified.
- [questions.json](questions.json): unchanged Q1–Q10 from the canonical [GitHub questions](../compare/github-questions/README.md), plus React issue #37655.
- [run-case.mjs](run-case.mjs): metered Octocode and Jev invocation wrapper; logs exact requests, responses, counts and elapsed time.
- [BENCHMARK.md](BENCHMARK.md): historical claim-check runbook and hypothesis-triage extensions. The frozen CONTRACT owns this campaign's differences.
- `runs/terra-20260918-v1/`: separate baseline, treatment and independent-curator artifacts.
- [repro](repro/README.md): separate actual React 19.2.8 reproduction and experimental fix matrix; not supplied to the evaluated arms.
- [summarize.mjs](summarize.mjs): verifies frozen hashes and event receipts and derives measured call/token totals.
- [frozen.json](frozen.json): frozen harness hashes.
- [compile-answers.mjs](compile-answers.mjs): compiles all completed arm answers without changing their claims.
- [Issues-v2 invalid-run report](issues-v2/REPORT.md): the six-issue updated-skill campaign produced zero gradeable pairs after all host slots hit an external usage cap; do not resume or cite it as efficacy evidence.
- [Previous pilot report](history/previous-pilot-results.md): the earlier four-case replication and its limits; original sealed artifacts remain unchanged under the workspace's `.octocode/` directory.

Both arms use fresh Terra high agents and the same research cap. Baseline makes no Jev calls; treatment makes one per case, including deterministic controls. This forced-call pilot measures possible decision changes and overhead; it does not endorse calling Jev for every research step. The React case verifies a public reported bug and proposed solution, rather than measuring independent bug discovery.

Run a logged lookup from the repository root:

```sh
node packages/octocode-benchmark/jev/run-case.mjs baseline Q1 schema ghSearch ghGetFileContent
node packages/octocode-benchmark/jev/run-case.mjs baseline Q1 octocode ghSearch --queries '<query-json>' --compact
node packages/octocode-benchmark/jev/run-case.mjs treatment Q1 jev <request-file>
```

The wrapper only logs and enforces per-case invocation caps. It is not a process sandbox; isolation relies on worker instructions. Host token telemetry is unavailable unless explicitly supplied by the host. Preserve failures and distinguish source verification, runtime reproduction and model advice.
