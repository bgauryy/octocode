---
name: octocode-harness-review
description: "Load this skill first when asked to cut Octocode tokens, calls or time per answer by reviewing its harness: instructions, tool descriptions, schemas, hints, reply-shaping code. Keeps only fixes a rerun proves."
---

# Octocode harness review

Cut tokens (new input, cached re-reads, output), calls and seconds per correct, cited answer. The cost lives in two
layers: **context** (instructions, tool descriptions, schemas, hint text) in `../octocode-mcp-host/packages/octocode-core`,
and **code** (validation, execution, reply shaping, pagination, caching) in `packages/octocode-native`. Evidence is
never the saving: cut duplication, metadata and round trips, never requested evidence.

Run from the repo root; `$DEV` is `node skills-dev/octocode-dev/scripts/dev.mjs` ([AGENTS.md](../../AGENTS.md#work-locally)).

## Steps

1. **Measure the baseline** on a pinned binary: `$DEV pin`, then run every call with
   `NODE_ENV=development OCTOCODE_NATIVE_BIN=<path>`; without `NODE_ENV=development` the launcher ignores the override
   and measures the installed binary. Take the fixed cost per session, fresh-agent task tallies and a dogfood log as
   [measure](references/measure.md) gives them.
2. **Break each issue down.** Walk the chain and stop at the first step that went wrong; that step owns the fix:
   instructions → tool description → schema and defaults → the call sent → validation → execution → reply shaping
   and pagination → hints and `next.*` → the agent's next call. For a context issue, quote the sentence that misled
   the agent, or name the missing one, and write the replacement. For a code issue, read the path end to end and
   measure it before proposing a change. Judge whole chains: three calls where one would do is an issue even when
   each call is correct.
3. **Propose.** Write or update `<repo>/docs/rfcs/context-efficiency.md`. Give each item its breakdown (`path:line`),
   measurement, change, expected saving, risk and proving check, and rank items by saving per risk. Cite
   [tool writing](https://www.anthropic.com/engineering/writing-tools-for-agents),
   [context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) or the
   [MCP spec](https://modelcontextprotocol.io/specification/latest) where they apply. A review-only request stops
   here and returns the ranked items.
4. **Fix and prove.** Land one item at a time: core text first (then `$DEV contracts:regen`), native after. Rerun the
   same tasks on a newly pinned binary. Keep a change only if answers stay correct and cited and cost drops beyond
   run-to-run spread (`harness-tally.mjs --before`); undo the rest by editing. Finish with `$DEV gates`, or hand the
   change to the gates owner in a multi-agent round.

## Return

The result first, then before → after per task (calls, failed calls, tokens new / cached / output, seconds, answer
correct), each kept change (layer, file, saving), what was undone, and open items. Label unmeasured claims as
assumptions.
