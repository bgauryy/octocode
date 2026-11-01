# Octocode graph dev

Develop the native code graph (`octocode graph`, `graphBuild`, `graphQuery`) and measure each change against pinned repositories, one per grammar.

```bash
node scripts/graph-bench.mjs --out before.json
node scripts/bench-compare.mjs before.json after.json
```

Start with [SKILL.md](SKILL.md) for the change workflow, the [engine map](references/engine-map.md) for where code lives, and the [battery guide](references/battery.md) for running and reading measurements. Requires Node 20+, ripgrep, the clones in `packages/octocode-benchmark/local/repos`, and a built octocode launcher.
