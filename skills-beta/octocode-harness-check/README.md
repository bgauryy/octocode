# Octocode harness check

Audit how the Octocode tools behave for agents. It combines four sources:

- real Claude Code sessions, read with [orangu](https://www.npmjs.com/package/orangu) and a transcript miner
- a live probe of every tool on native CLI, node CLI, and MCP
- code evidence gathered with the local Octocode tools
- a fix plan whose checks are verified by rerunning the probe

## When to use it

- After a release, or before one, to find where agents actually hit errors, oversized outputs, or weak error messages.
- To check whether CLI and MCP still agree and what the MCP catalog costs in context.
- To turn scattered "the tools feel off" reports into a ranked, reproducible plan.

Use `octocode-research` for one known bug, and `octocode-eval-benchmark` to prove that a change improved agent behavior.

## Scripts

| Script | Output |
|---|---|
| `scripts/mine-transcripts.mjs` | Per-tool error rate, output-size percentiles, clustered error signatures from `~/.claude/projects/<repo>*` |
| `scripts/probe-surfaces.mjs` | `results.json`: per case × surface class, content check, chars, latency, `next.*` share; MCP catalog weight |
| `scripts/compare-runs.mjs` | Baseline vs candidate diff; exit 1 on regression |

Each script prints usage with `--help`. Artifacts go to `<workspace>/.octocode/harness-check/<run-id>/`.

## Trial status

This skill is in `skills-beta/` because it depends on a third-party CLI (orangu) and on Claude Code transcripts. It can be promoted to `skills/` after it has run on another checkout and a second host.
