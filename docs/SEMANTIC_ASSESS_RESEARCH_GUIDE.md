# Semantic assessment research guide

Use `semanticAssess` to rank or classify ambiguous evidence, then prove the selected claim with an ordinary source read, exact lookup, or test. The tool compresses a decision; it does not create evidence.

## Recommended loop

1. **Frame the decision.** State the next action that can change and choose Noul, Choice, or Score.
2. **Scout candidates.** Use search, AST, LSP, history, or package discovery to identify bounded candidate resources.
3. **Assess the matrix.** Put shared candidates in `resources[]` and shared typed questions in `questions[]`. The runtime captures each resource once and evaluates the cross-product.
4. **Follow coverage.** Keep every ordered page result. If `next.assess` is returned, execute it unchanged. `partial` coverage cannot support a global absence claim.
5. **Verify the winner.** Fetch the decisive lines, inspect the symbol/reference, or run the focused test. Cite that evidence, not the semantic result.

Do not automatically turn every scouting step into a semantic call. Skip it when the answer is already known, an exact operation can decide it, or every candidate must be read anyway.

## Query design

- Prefer one query with multiple questions when all questions apply to all resources. This avoids repeated capture and gives every row stable `resourceId` and `questionId` correlation.
- Use batched `queries[]` for independent matrices whose cross-product is meaningless.
- Keep a matrix at 25 cells or fewer and a batch at 50 cells or fewer.
- Keep questions atomic. `reasoning` records why the answer matters; it is not a prompt for generic free-form analysis.
- Bound large resources with `maxChars`. The runtime emits same-resource pages in order and never hides them behind an implicit reducer.

## Primitive selection

- **Noul**: decide one binary proposition. Criteria are optional; if supplied, include both `true` and `false`. Their descriptions may be `null`.
- **Choice**: select among 2–255 declared alternatives. Criterion descriptions may be `null` when labels are self-explanatory.
- **Score**: rate one ordered dimension with 2–10 non-null, non-empty level definitions.

For all primitives, `instructions` must be a non-null, non-empty string, object, or array. Supplied state in `context.value` has the same non-null, non-empty boundary.

## Interpret results

Correlate output by `queryId`, `resourceId`, `questionId`, and `pageIndex`. Do not merge page answers unless your application declares an explicit reducer outside the tool.

Successful pages preserve both `requestedModel` and `resolvedModel`. Record both when comparing Jev model aliases or investigating provider behavior. Choice/Score `confidence` describes concentration of the returned distribution, not factual correctness.

## Availability

MCP advertises `semanticAssess` only when `OCTOCODE_CLASSIFICATION_API` resolves to a nonblank value. The CLI command remains discoverable, but a call without the key fails with a message naming the missing environment variable and the setup action.

See [Semantic Assessment Reference](OCTOCODE_SEMANTIC_ASSESS.md) for the complete public contract.
