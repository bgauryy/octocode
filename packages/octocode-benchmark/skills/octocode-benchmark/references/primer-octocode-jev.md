# Octocode + Jev arm primer

Inject with [primer-octocode.md](primer-octocode.md) for the `octojev` arm. Freeze
both primers and the live catalog/schema before comparing runs. Report setup
prompt tokens separately even when the campaign excludes setup from its primary
execution metric; never treat them as free.

Use the built CLI for every Jev call:

```bash
node packages/octocode/out/octocode.js tools jev --scheme --scheme-view query --json --compact
node packages/octocode/out/octocode.js tools jev --input request.json --json --compact
```

The operator configures `OCTOCODE_JEV_KEY` and the model through runtime config.
The request requires `{state, questions}`. Supply evidence in state or named
`sources` references to local/GitHub files; bodies go directly to Jev and results
contain source/hash/byte receipts. Each independent question supplies its own
type and instructions. Use
Noul for a binary probability, Choice for mutually exclusive labels, and Score
for an ordered rubric. Include an insufficient-evidence option where needed.
Without sources, provider state stays unchanged. With sources, it becomes
`{context: state, sources: {ID: {source, content}}}`. The tool executes no actions
selected by a judgment.

Use Jev only when a semantic judgment can change the next action enough to repay
preparation and latency. Batch questions sharing evidence; keep dependent steps
sequential. Use cheap exact checks directly. Verify deciding evidence, and never
treat provider confidence as calibrated correctness or permission to act.

Record the build version and repository SHA. For every result, inspect errors
before counting success. Sum `data.usage.input_tokens` and
`data.usage.output_tokens` over all result rows; report `jevCalls`,
`jevInputTokens`, `jevOutputTokens`, wall time, and host context tokens per case
and across the run. Zero Jev calls is valid when no useful decision was open.
Keep retries, schema discovery, and failed calls visible in the cost record.
