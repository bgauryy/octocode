# Octocode + Jev provider arm primer

Inject with [primer-octocode.md](primer-octocode.md) for the `octojev` arm. Freeze
both primers and the live catalog/schema before comparing runs. Report setup
prompt tokens separately even when the campaign excludes setup from its primary
execution metric; never treat them as free.

Use the built CLI's single public semantic tool for every provider call:

```bash
node packages/octocode/out/octocode.js scheme semanticAssess --view query --compact
node packages/octocode/out/octocode.js semanticAssess --input request.json --compact
```

The operator configures `OCTOCODE_JEV_KEY` and the model through runtime config.
Without a nonblank key, the CLI still discovers `semanticAssess` and its schema.
Execution returns an actionable missing-key error, and MCP doesn't register the
tool.

Pass one SemanticQuery directly, or use a root `queries[]` array for independent
matrices when a cross-product is incorrect. A query has stable `id` and
`reasoning` fields, then applies every entry in `questions[]` to every entry in
`resources[]`. Keep each matrix at 25 cells or fewer and a batch at five queries
and 50 total cells. Each resource supplies either non-empty state as
`context:{value:...}` or one unexecuted bounded read as
`context:{tool,query}`. Set `maxChars` only to lower the 80,000-character
resource cap.

Each question has an `id` and one native typed `question`. Use Noul for one
binary proposition, Choice for 2–255 mutually exclusive caller labels, and Score
for one ordered 2–10 level rubric. Instructions must be non-empty. Noul's paired
`true`/`false` criteria and Choice descriptions can be `null`; Score levels must
not be `null`. Include an insufficient-evidence Choice label when the resource
might not decide the question.

For example, this two-resource × three-question matrix produces six correlated
cells:

```json
{
  "id": "screen-docs",
  "reasoning": "Choose which evidence warrants an exact read.",
  "resources": [
    {
      "id": "local-a",
      "context": {
        "tool": "localFetch",
        "query": {
          "reasoning": "Capture candidate A.",
          "path": "/ABS/a.md",
          "fullContent": true
        }
      }
    },
    {
      "id": "github-b",
      "context": {
        "tool": "ghGetFileContent",
        "query": {
          "reasoning": "Capture candidate B.",
          "owner": "ORG",
          "repo": "REPO",
          "path": "b.md"
        }
      }
    }
  ],
  "questions": [
    {
      "id": "supports",
      "question": {
        "type": "noul",
        "instructions": "Does this resource support the target claim?",
        "criteria": { "true": null, "false": null }
      }
    },
    {
      "id": "fit",
      "question": {
        "type": "choice",
        "instructions": "Classify this resource.",
        "criteria": {
          "relevant": null,
          "unrelated": null,
          "insufficient": null
        }
      }
    },
    {
      "id": "strength",
      "question": {
        "type": "score",
        "instructions": "Score evidentiary strength.",
        "criteria": ["No support", "Indirect support", "Direct support"]
      }
    }
  ]
}
```

The result correlates `queryId`, `resourceId`, `questionId`, and `pageIndex`.
Every success page preserves the typed provider response plus separate
`requestedModel` and `resolvedModel`. Context receipts contain hashes and
coverage without returning resource bodies. Large resources remain visible as
ordered page-local assessments; retain partial and error pages, and run
`next.assess` unchanged. The tool executes no action selected by a judgment, and
its result is not source proof.

Use `semanticAssess` only when a bounded judgment can change the next action
enough to repay preparation and latency. Use one matrix when questions share the
same resources; use root batching only for independent matrices. Keep dependent
steps sequential. Use cheap exact checks directly. Verify deciding evidence,
and never treat provider confidence as calibrated correctness or permission to
act.

Record the build version and repository SHA. For every result, inspect errors
before counting success. Sum usage fields that are present across success pages;
batched provider responses attach shared usage once. Report
`semanticAssessCalls`, `jevInputTokens`, `jevOutputTokens`, wall time, and host
context tokens per case and across the run. Zero provider calls is valid when no
useful decision was open. Keep retries, schema discovery, and failed calls
visible in the cost record.
