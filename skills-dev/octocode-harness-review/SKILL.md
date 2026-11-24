---
name: octocode-harness-review
description: "Load this skill first when asked to make Octocode cheaper per answer (fewer tokens, calls or wall time) or to review its harness: server instructions, tool descriptions, schemas, hints and the native code that shapes replies. It traces each cost to the layer that owns it and keeps only fixes a rerun proves. Measuring an agent setup's configured context: octocode-context-audit."
---

# Octocode harness review

Cut tokens (new input, cached re-reads, output), calls and seconds per correct, cited answer. The cost lives in two
layers: **context** (instructions, tool descriptions, schemas, hint text) in `../octocode-mcp-host/packages/octocode-core`,
and **code** (validation, execution, reply shaping, pagination, caching) in `packages/octocode-native`. Evidence is
never the saving: cut duplication, metadata and round trips, never requested evidence.

## Measure

- **Dogfood.** Research this repo only through local Octocode MCP/CLI and graph; log each wasted call, bad default,
  misleading hint, missing `next.*` or oversized reply (call, layer) in `.octocode/octocode-harness-review/<date>/REFLECT.md`.
- **Fresh agents for the numbers.** Use agents with no project instructions, skills or memory. Cover search → fetch,
  symbol trace, GitHub search → read, history, package lookup, large file, many hits and a paginated walk. Reuse the
  `packages/octocode-benchmark` tasks: workflow chains in `CHECKS.md`, and questions W1–W10 with stored numbers.
  Tally calls, failed calls, characters, tokens (new, cached, output) and time with `node skills-dev/octocode-harness-review/scripts/harness-tally.mjs <transcript.jsonl>... [--before <tally.json>]` ([harness-tally.mjs](scripts/harness-tally.mjs); `--before` adds each delta and whether it exceeds run-to-run spread); judge correctness by reading the answers.
- **Pin the binary** with `$DEV pin` (prints the frozen copy's path and sha256), then run with `NODE_ENV=development OCTOCODE_NATIVE_BIN=<path>`. Without `NODE_ENV`, the launcher
  ignores the override. Engine time: `$DEV profile <tool> '<query json>'`.
- **Fixed cost per session**: characters in the server instructions and `tools/list`.

  ```bash
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    | perl -e 'alarm 30; exec @ARGV' -- node packages/octocode-mcp/dist/index.js 2>/dev/null \
    | node -e 'const r=require("fs").readFileSync(0,"utf8").trim().split("\n").map(JSON.parse);console.log({instructions:[...r[0].result.instructions].length,toolList:[...JSON.stringify(r[1].result.tools)].length})'
  ```

## Break each issue down

Walk the chain and stop at the first step that went wrong; that step owns the fix: instructions → tool description →
schema and defaults → the call sent → validation → execution → reply shaping and pagination → hints and `next.*` →
the agent's next call. For a context issue, quote the sentence that misled the agent, or name the missing one, and
write the replacement. For a code issue, read the path end to end and measure it before proposing a change. Judge
whole chains: three calls where one would do is an issue even when each call is correct.

## Propose, fix, prove

Write or update `<repo>/docs/rfcs/context-efficiency.md`. Give each item its breakdown (`path:line`), measurement,
change, expected saving, risk and proving check, and rank items by saving per risk. Cite
[tool writing](https://www.anthropic.com/engineering/writing-tools-for-agents),
[context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) or the
[MCP spec](https://modelcontextprotocol.io/specification/latest) where they apply. For a review-only request, stop here.

Land one item at a time: core text first (then regenerate), native after. Rerun the same tasks on a newly frozen
binary. Keep a change only if answers stay correct and cited and cost drops by more than run-to-run noise; undo the
rest by editing. Finish with `$DEV gates`. Report the result first, then before → after per task, each change
(layer, file, saving), what was undone, and open items. Label unmeasured claims as assumptions.
