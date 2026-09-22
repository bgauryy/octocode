# Semantic assessment reference

**Judge before you read.** `clasify` rates *unread* candidates — files, search hits, or supplied text — with bounded, typed questions (Noul / Choice / Score) and returns **only a verdict, never file bodies**. An agent screens many resources server-side and opens only the few that matter, so a routing decision costs a fraction of the context that reading every candidate would. It is a decision aid, not an evidence source: verify claims with the original source, an exact lookup, or a test before relying on them.

The public tool and CLI command are both named `clasify`. Jev remains the internal provider/model family and the environment variable remains `OCTOCODE_CLASSIFICATION_API`.

## Migration from `semanticAssess`

`clasify` replaces the former `semanticAssess` tool in a hard cutover — there is no `semanticAssess` alias, and calling the old name fails as unknown. Update any first-party callers and operator allowlists (`tools.enabled`, `tools.disabled`, `TOOLS_TO_RUN`, `DISABLE_TOOLS`) that named `semanticAssess` to `clasify`. This is a tool-name change only; the configuration schema, provider-key resolution, and environment variables are unchanged.

| Before | After |
|---|---|
| `octocode semanticAssess` | `octocode clasify` |
| `scheme semanticAssess` | `scheme clasify` |
| `next.assess` | `next.clasify` |
| `octocode-semantic-assess` | `octocode-clasify` |

## Availability

- MCP registers `clasify` only when the resolved `OCTOCODE_CLASSIFICATION_API` is present and nonblank. When the key is absent or blank, the tool is omitted from MCP discovery.
- The CLI command remains directly callable even when the available-tool catalog excludes it. Calling it without a nonblank key fails with an actionable error that names `OCTOCODE_CLASSIFICATION_API` and tells the caller to set it.
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
npx octocode clasify --input request.json
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

If a query-level `next.clasify` continuation is present, execute it unchanged and append its page results. A partial result cannot establish global absence. Do not silently collapse pages or treat the first page as the whole resource.

## Research workflow

Use `clasify` only when a bounded semantic judgment changes the next action. Exact presence, counts, symbols, references, diagnostics, and deterministic assertions belong to ordinary search, AST/LSP, or tests. Skip it when the answer is already known, an exact operation can decide it, or every candidate must be read anyway.

1. **Frame the decision.** State the next action that can change and pick Noul, Choice, or Score.
2. **Scout candidates.** Use search, AST, LSP, history, or package discovery to identify bounded candidates.
3. **Assess the matrix.** Put shared candidates in `resources[]` and shared typed questions in `questions[]`; each resource is captured once and evaluated across the cross-product.
4. **Follow coverage.** Keep every ordered page and execute any `next.clasify` unchanged. `partial` coverage cannot support a global-absence claim.
5. **Verify the winner.** Fetch the decisive lines, inspect the symbol/reference, or run the focused test — and cite that evidence, not the semantic result.

## Source of truth

Inspect the installed contract before hand-authoring calls:

```bash
npx octocode scheme clasify --compact
```

The live schema is authoritative for limits, supported read tools, and output fields. Provider primitive semantics are documented by TypeSafe: [Noul](https://docs.typesafe.ai/primitives/noul), [Choice](https://docs.typesafe.ai/primitives/choice), [Score](https://docs.typesafe.ai/primitives/score), and [advanced usage](https://docs.typesafe.ai/primitives/advanced).
