# Tool checks

One file per tool: real research queries, the workflow into the tools it relates to, and one bad input.
`run.mjs` runs them against a local `octocode` binary over MCP and reports, per tool:

- **Description:** length, an example, when-to-use wording, and which other tools it names.
- **Schema:** size, field count, and fields with no description.
- **Steps:** each query's expectations (fields present, status, error code with a hint or example, size).
- **Follow-ups:** steps that run a call the previous response offered (`related:<tool>`, `next:<key>`, `page`)
  unchanged.

```bash
node skills-dev/octocode-dev/scripts/dev.mjs build:dev          # the local build
node packages/octocode-benchmark/tool-checks/run.mjs             # every tool
node packages/octocode-benchmark/tool-checks/run.mjs lspSearch   # one tool
node packages/octocode-benchmark/tool-checks/run.mjs --billed    # also clasify steps that bill the provider
```

The run uses a temporary Octocode home (persistent storage, security checks on, graph home inside it) and
your GitHub login. Steps marked `"billed"` run only with `--billed`; the other clasify steps send no provider
request. The full JSON report goes to `--out` or a file in the OS temp folder; the exit code is 1 when a step
fails.

Step fields: `tool` + `args` (one query, or a full `{queries}` input), or `follow` (+ `from`, a step index);
`expect` takes `has` (paths such as `data.files[].path`, `$.` for the response root), `min`, `contains`,
`status`, `error`, `offers` and `maxChars`.
