Load when a skill step describes an exact operation the agent would do by hand, or when adding a script to a skill.

# Code over prose

Text is for judgment: what to choose, when to stop, how to read a result. Code is for exact operations. When a step's result must be exactly right, recurs on every run, or takes more than one mechanical step, write a script and make the step call it; do not describe the algorithm for the agent to perform by hand.

| Use a script when the step… | Keep it as text when the step… |
|---|---|
| parses or validates JSON, YAML, schemas or tool output; extracts fields | chooses between options or judges meaning |
| counts, sums, sorts, deduplicates, diffs, hashes or does date math | writes, reviews or summarizes for a reader |
| walks pagination, retries, or merges and rewrites files | is one fixed command: give it verbatim |
| calls an HTTP API with a fixed request shape or reads a credential | is already done by a tool the host has (Octocode MCP tools, a linter) |
| checks that links, paths, anchors or a manifest exist and match | sets scope, permissions or a stop condition |

How:

1. Put it in `scripts/<verb-noun>.mjs`: Node ESM with built-ins only, or a dependency bundled into `scripts/` by the skill's own private `package.json` (see `skills/common/README.md`), so the installed skill runs with nothing else. Read credentials only through the skill's `keys.mjs` (`getKey`).
2. Give it `--help` with inputs, outputs, effects and exit codes. Print JSON on stdout; fail with a stable error code (exit 2 for invalid input, 1 for a failed run); never print a secret. Keep a test under `scripts/tests/`.
3. In the step that needs it, write the exact command, its working directory, what it changes and how to read its result and failures. Link the script. Do not restate its logic in prose; point to `--help` for options.
4. Replace the prose procedure with that call and run it. A script the text never routes is code the agent will not use: the checker warns on it.
