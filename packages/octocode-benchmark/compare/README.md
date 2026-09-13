# Benchmark comparisons

[Terra v3](terra-v3/README.md) is the active benchmark: its locked corpus,
contracts, and execution steps are in the [runbook](terra-v3/RUNBOOK.md).
[Advanced research v1](advanced-research-v1/README.md) is a separate controlled
local/GitHub pilot with its own eligibility and receipt rules.

## Historical GitHub arms

The legacy GitHub campaign compares the same Octocode anchor with one baseline at a
time. Every arm uses the shared [GitHub questions](github-questions/), while a
corpus-local matchup owns its `questions/` directory.

- [Plain gh](octocode-vs-gh/README.md)
- [gh + RTK](octocode-vs-gh-rtk/README.md)
- [gh + Headroom](octocode-vs-gh-headroom/README.md)

The historical protocol and fairness rule are in
[BENCHMARK.md](../skills/octocode-benchmark/references/BENCHMARK.md). Results are
preserved in [../results/](../results/README.md); do not compare or pool their
character measurements with Terra metrics.

## Maintain a historical arm

Add a GitHub question to the shared set, or a corpus-local question to that matchup.
Each question contains only a title, id, and self-contained checkable prompt. An arm
README documents its permitted read-only surface, exact invocations, measurement
wrapper, and pinned version or SHA; follow the
[matchup convention](../skills/octocode-benchmark/references/matchup-readme.md).
