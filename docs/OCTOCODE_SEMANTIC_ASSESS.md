# Semantic assessment reference

`semanticAssess` applies bounded, typed semantic questions to supplied state or to an unread Octocode read request. It is a decision aid, not an evidence source: verify claims with the original source, an exact lookup, or a test before relying on them.

The public tool and CLI command are both named `semanticAssess`. Jev remains the internal provider/model family and the environment variable remains `OCTOCODE_JEV_KEY`.

## Availability

- MCP registers `semanticAssess` only when the resolved `OCTOCODE_JEV_KEY` is present and nonblank. When the key is absent or blank, the tool is omitted from MCP discovery.
- The CLI command remains directly callable even when the available-tool catalog excludes it. Calling it without a nonblank key fails with an actionable error that names `OCTOCODE_JEV_KEY` and tells the caller to set it.
- Do not put provider credentials in `.octocoderc` or commit them.

## Input shape

Pass either one complete `SemanticQuery` directly or `{ "queries": [...] }` containing one to five independent queries. A query contains:

- `id`: stable correlation ID.
- `reasoning`: why this bounded judgment can change the next action. It is trace metadata, not a request for free-form reasoning.
- `resources`: one to 25 resources, each with an `id`, a `context`, and optional `maxChars`.
- `questions`: one to five typed questions, each with an `id` and a Noul, Choice, or Score question.

Every question is applied to every resource. Keep each `resources[] × questions[]` matrix at 25 cells or fewer and a batch at 50 cells or fewer. Use multiple questions in one query when they share the same resources, so each resource is captured once. Use `queries[]` when the matrices are independent or their cross-product is invalid.

A resource context is exactly one of:

- `{ "value": ... }` for already-observed non-empty state. `value` accepts a string, object, or array; it cannot be `null` or empty.
- `{ "tool": "...", "query": { ... } }` for one unread request to a supported Octocode read tool. The runtime executes and sanitizes the read without returning its body.

## Typed questions

All three primitives require `instructions`. Instructions accept a non-empty string, object, or array; `null` and empty values are invalid.

| Primitive | Use | Criteria | Result |
|---|---|---|---|
| Noul | One binary proposition | Optional. Omit `criteria`, or provide both `true` and `false`; either description may be `null`. | Probability `noul` from 0 to 1. |
| Choice | One of several declared alternatives | Required object with 2–255 distinct labels; descriptions may be `null`. | Selected `choice`, full `probabilities`, and `confidence`. |
| Score | One ordered dimension | Required array of 2–10 non-null, non-empty string/object/array level definitions, low to high. | Expected zero-based `score`, full `probabilities`, `legend`, and `confidence`. |

Choice and Score confidence measures distribution concentration. It is not the winning probability or a probability that the answer is correct.

## Direct example

```json
{
  "id": "candidate-screen",
  "reasoning": "Choose which candidate to verify next.",
  "resources": [
    {
      "id": "axios-core",
      "context": {
        "tool": "ghGetFileContent",
        "query": {
          "owner": "axios",
          "repo": "axios",
          "path": "lib/core/Axios.js",
          "fullContent": true,
          "reasoning": "Screen this candidate without returning its body."
        }
      }
    }
  ],
  "questions": [
    {
      "id": "implements-ordering",
      "question": {
        "type": "choice",
        "instructions": "Does this file implement request interceptor ordering?",
        "criteria": {
          "direct": "Implements it.",
          "unrelated": "Different concern.",
          "insufficient": null
        }
      }
    },
    {
      "id": "verification-priority",
      "question": {
        "type": "score",
        "instructions": "How strongly should this candidate be prioritized for an exact verification read?",
        "criteria": ["No relevant signal", "Plausible lead", "Strong direct signal"]
      }
    }
  ]
}
```

Run a saved request with:

```bash
npx octocode semanticAssess --input request.json
```

For independent matrices, wrap complete query objects in `queries`:

```json
{
  "queries": [
    {
      "id": "api-risk",
      "reasoning": "Decide whether the API change needs compatibility review.",
      "resources": [
        { "id": "diff-summary", "context": { "value": { "removedFields": ["legacyMode"] } } }
      ],
      "questions": [
        { "id": "breaking", "question": { "type": "noul", "instructions": "Does this state indicate a breaking API change?" } }
      ]
    },
    {
      "id": "test-risk",
      "reasoning": "Decide whether focused failure-path tests are needed.",
      "resources": [
        { "id": "coverage-summary", "context": { "value": { "failurePathsCovered": false } } }
      ],
      "questions": [
        { "id": "needs-tests", "question": { "type": "noul", "instructions": "Are focused failure-path tests needed?" } }
      ]
    }
  ]
}
```

## Output and pagination

The runtime automatically preserves ordered same-resource pages as separate `pages[]` entries. `maxChars` budgets sanitized evidence payload, not repeated result-envelope or continuation metadata; supplied `context.value` objects retain full serialized-size accounting. It does not average, vote, or otherwise reduce page answers. Correlate every result by `queryId`, `resourceId`, `questionId`, and `pageIndex`.

Each resource-question result reports `coverage` as `complete`, `partial`, or `error`. Successful pages include `requestedModel` and `resolvedModel` separately, because a requested Jev model alias can resolve to a different provider model. Retain error pages and incomplete coverage; they are part of the result, not noise.

If a query-level `next.assess` continuation is present, execute it unchanged and append its page results. A partial result cannot establish global absence. Do not silently collapse pages or treat the first page as the whole resource.

## Workflow boundary

Use `semanticAssess` only when a bounded semantic judgment changes the next action. Exact presence, counts, symbols, references, diagnostics, and deterministic assertions belong to ordinary search, AST/LSP, or tests.

For research, scout broadly enough to identify candidates, assess only the ambiguous candidates, then verify the winning claims against source evidence. Do not cite a semantic answer as proof. See [Semantic Assessment Research Guide](SEMANTIC_ASSESS_RESEARCH_GUIDE.md).

## Source of truth

Inspect the installed contract before hand-authoring calls:

```bash
npx octocode scheme semanticAssess --compact
```

The live schema is authoritative for limits, supported read tools, and output fields. Provider primitive semantics are documented by TypeSafe: [Noul](https://docs.typesafe.ai/primitives/noul), [Choice](https://docs.typesafe.ai/primitives/choice), [Score](https://docs.typesafe.ai/primitives/score), and [advanced usage](https://docs.typesafe.ai/primitives/advanced).
