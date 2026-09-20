# `semanticAssess` CLI contract

Load when setting up the CLI request shape, runtime credentials, or coverage handling.

Inspect `octocode scheme semanticAssess --view query --compact` once. For unread tool context, inspect `octocode scheme <name> --view query --compact` too. Execute `octocode semanticAssess --input request.json --compact`. In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.

Send one semantic query as `{reasoning,resources:[{id,context}],questions:[{id,question}]}`. Send `{queries:[semanticQuery,...]}` only when several matrices are independent. Every resource in a semantic query is evaluated against every question; do not put unrelated pairs into one cross-product. Matrix output carries stable query, resource, question, and page correlation. Maximums are 25 cells per matrix, 25 resources, and five questions. Reasoning is nonblank Octocode trace metadata excluded from provider evidence and grouping identity. Do not send retired flat `context + question`, `state`, `sources`, model, or workflow fields. A resource's nested `query` is one ordinary read query, not another bulk envelope.

Runtime configuration supplies `OCTOCODE_JEV_MODEL` and `OCTOCODE_JEV_KEY`. MCP omits `semanticAssess` when the resolved key is missing or blank. The CLI keeps the command discoverable but fails an attempted assessment with an actionable key setup error. Store the key in a trusted Octocode environment source; never put credentials in request values.

Supported read tools keep their normal validation, availability, redaction, and security policy. `semanticAssess` executes each unread resource query, supplies its sanitized bounded result to Jev, and returns native answers plus compact source receipts without retrieved bodies. Recursive assessment, astRewrite, and ghCloneRepo are excluded. Output distinguishes the configured `requestedModel` alias from the provider's `resolvedModel`.

Noul returns the probability of yes and has no separate confidence. Choice returns one named alternative, the full probability map, and distribution confidence; include `insufficient` when missing evidence matters. Score uses 2–10 ordered levels and returns their probability-weighted zero-based position, full distribution, confidence, and legend. Instructions are required non-null strings, objects, or arrays. Choice descriptions and Noul `true`/`false` descriptions may be null; Score levels may not. Resource values are non-null strings, objects, or arrays. Use one atomic scoped question and keep structured instructions as JSON.

Coverage receipts distinguish bounded query scope from explicitly partial results. For a large resource, the runtime follows validated same-resource continuations within its page/token budget, repeats the unchanged question set, and returns every page judgment. Never infer global absence from one page, outline, missing body, or execution error. Do not average or vote page probabilities: retain page-local results and read the deciding source. A terminal limit remains explicit and incomplete. There is no judgment cache.

The provider request is bounded by native request and response limits, with ordinary tool and shared runtime limits also applying. Retain uncertain candidates and verify deciding evidence before action. Provider answers are preserved; Octocode does not add explanations or expose hidden chain-of-thought.

For choosing a question and request shape, see [prompt recipes](jev-workflows.md); use live `scheme semanticAssess` as the contract authority.
