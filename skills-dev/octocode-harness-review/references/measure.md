# Measure

Take every number before the change and again after it, on the same tasks and a pinned binary. Keep the run's files in `.octocode/octocode-harness-review/<date>/`.

## Pin the binary

`$DEV pin` prints the frozen copy's `path` and `sha256`, or answers `noBinary` when nothing is built: run `$DEV build:dev` first. Every measured call carries the environment in SKILL.md step 1; the launcher reports an ignored override only on stderr.

## Fixed cost per session

UTF-8 bytes of the server instructions and every `tools/list` page, through the [mcp-surface](../../../skills/octocode-agentic/scripts/mcp-surface.mjs) script (`--out` refuses an existing file):

```bash
mkdir -p .octocode/octocode-harness-review/<date>
node skills/octocode-agentic/scripts/mcp-surface.mjs capture --cmd 'NODE_ENV=development OCTOCODE_NATIVE_BIN=<path> node packages/octocode-mcp/dist/index.js' --out .octocode/octocode-harness-review/<date>/surface-<label>.json
node skills/octocode-agentic/scripts/mcp-surface.mjs size .octocode/octocode-harness-review/<date>/surface-<label>.json
```

`size` gives `instructionsBytes`, `toolBytes`, `surfaceBytes` and per-tool bytes; bytes are not tokens. `diff <before.json> <after.json>` shows whether a text change left the schemas structurally equal.

## Fresh agents for the numbers

Use agents with no project instructions, skills or memory. Cover search → fetch, symbol trace, GitHub search → read, history, package lookup, large file, many hits and a paginated walk. Reuse the `packages/octocode-benchmark` tasks: workflow chains in `CHECKS.md`, and questions W1–W10 (`questions/workflows/`) with stored numbers.

```bash
node skills-dev/octocode-harness-review/scripts/harness-tally.mjs <transcript.jsonl>... [--before <tally.json>]
```

[harness-tally.mjs](../scripts/harness-tally.mjs) counts calls, failed calls, characters, tokens (new, cached, output) and time per run and per task; `--before` adds each delta and whether it exceeds run-to-run spread (`beyondSpread`). It does not judge correctness: read the answers.

## Dogfood

Research this repo only through local Octocode MCP/CLI and graph; log each wasted call, bad default, misleading hint, missing `next.*` or oversized reply (call, layer) in `.octocode/octocode-harness-review/<date>/REFLECT.md`.

## Engine time

`$DEV profile <tool> '<query json>'` (`--help` lists phases and options).
