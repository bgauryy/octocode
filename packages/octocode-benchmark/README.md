# @octocodeai/octocode-benchmark

An agent-vs-agent eval. It asks the same code-research questions of two Claude agents, one **with Octocode** and one **without it**, and compares their correctness, tokens, cost, tool calls and time. Published results: [docs/BENCHMARKS.md](../../docs/BENCHMARKS.md).

| Arm | Tools the agent gets |
|---|---|
| `octocode` | The Octocode MCP server (`packages/octocode-mcp`) with local tools enabled and clasify available. It has no built-in Claude Code tools. |
| `rg-gh` | Claude Code's Bash tool, restricted to commands that start with `rg` or `gh`. It has no MCP servers. |

Both arms run the same model, prompt, turn limit and timeout. Each agent decides its own tool calls; nothing in the prompt tells it which tool to use or how.

## Files

| Path | Purpose |
|---|---|
| [`eval/QUESTIONS.md`](eval/QUESTIONS.md) | The 20 questions (10 GitHub, 10 local) with their sources and pinned corpus |
| `eval/questions.json` | Machine-readable copy of the questions, read by the harness |
| `eval/run.mjs` | Launches the solver agents with headless Claude Code and records each run |
| `eval/judge.mjs` | Blind pairwise judge: establishes ground truth, grades both answers in both orders, and breaks ties |
| `eval/report.mjs` | Aggregates a run into `summary.json` and regenerates `docs/BENCHMARKS.md` |
| `eval/BENCHMARKS.template.md` | Prose for `docs/BENCHMARKS.md`; `report.mjs` fills in the numbers |
| `eval/lib.mjs` | Shared helpers: process spawning, stream parsing and bounded parallelism |
| `eval/results/<run-id>/` | Run output. Only `manifest.json`, `summary.json` and `report.md` are tracked. |

## Run it

Prerequisites:

- Claude Code (`claude`) logged in;
- `gh` authenticated;
- the Octocode MCP server built (`packages/octocode-mcp/dist/index.js`);
- the two local corpora cloned at their pinned commits, as listed in [octocode-local-testing/repos/README.md](../../octocode-local-testing/repos/README.md) (rows `langchain` and `nextjs`).

The Octocode arm reads the clasify key from `~/.octocode/.env`, as the product does.

```sh
cd packages/octocode-benchmark/eval
node run.mjs --run-id smoke --questions G09,L03 --passes 1   # validate the harness first
node run.mjs --run-id <id> --passes 3 --concurrency 4        # 20 questions × 2 arms × 3 passes
node judge.mjs --run-id <id>                                 # blind judge, both orders per pair
node report.mjs --run-id <id> --write-docs                   # summary.json and docs/BENCHMARKS.md
```

Runs resume: a finished run or judge call is not repeated. The harness refuses to resume a run ID if `questions.json`, `run.mjs`, `lib.mjs`, the MCP config or the MCP server build has changed since the run started. After any harness change, use a new run ID and rerun both arms.

## Isolation

- Every solver starts in a fresh, empty temporary directory outside the repository, with `--setting-sources ""` and `--strict-mcp-config`. It sees no CLAUDE.md, project memory, user settings or other MCP servers.
- The Octocode arm's local tools are limited to the corpus paths (`ALLOWED_PATHS`).
- The rg + gh arm can read only the corpus paths (`--add-dir`), so it cannot read this package or its results.
- Every run records the tools offered and the tools called, and fails its isolation check if the arms cross.
- Neither arm is blocked from public GitHub, which is part of the task for GitHub questions.
- The judge sees scrubbed answers labelled X and Y, never which arm wrote them. Its verdicts are never shown to a solver.
- No answer keys are stored.
