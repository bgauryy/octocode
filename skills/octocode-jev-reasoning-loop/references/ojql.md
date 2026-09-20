# Jev CLI contract

Load when setting up the CLI request shape, runtime credentials, or coverage handling.

Inspect `octocode scheme jev --view query --compact` once. For hidden tool context, inspect `octocode scheme <name> --view query --compact` too. Execute `octocode jev --input request.json --compact`. In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.

Use `{queries:[...]}` for up to five independent `{reasoning,context,question}` pairs. When every question applies to every resource, use `{reasoning, resources:[{id,context}], questions:[{id,question}]}`. Matrix output is resource-major with both IDs; each resource is captured once. Maximums are 25 cells, 25 resources, and five questions. Reasoning is nonblank trace metadata excluded from provider evidence and grouping identity. Do not send retired `state`, `sources`, model, or workflow fields. A nested `query` is one ordinary query, not another bulk envelope.

Runtime configuration supplies `OCTOCODE_JEV_MODEL` and `OCTOCODE_JEV_KEY`. If the key is in a trusted home env file, use Node's `--env-file="$HOME/.octocode/.env"` option. Never put credentials in request values.

The supported read tools use their normal validation, availability and security policy. Jev runs the context tool, supplies its sanitized bounded result to the provider, and returns `{answer, model, usage, context?}` without retrieved bodies. Recursive Jev, astRewrite and ghCloneRepo are excluded. The model value is the configured alias, not a guarantee of a resolved provider version.

Noul returns probability of yes. Choice compares named alternatives; include insufficient when missing evidence matters. Score uses 2–10 ordered levels and returns their expected zero-based index. Instructions, criterion descriptions and inline values accept string/object/array/null. Use one atomic scoped question; structured instructions stay JSON.

Coverage receipts distinguish bounded query scope from explicitly partial results. Continue only using executable returned requests, or narrow terminally limited queries. Never infer global absence from a page, outline, missing body or execution error. Jev does not follow source continuations. For huge local/browser/HAR artifacts, split into bounded resources and submit successive matrices until every chunk is judged; the scraping/Chrome triage bridge does this automatically. There is no cross-page probability composition or judgment cache.

Jev response paging is unsupported: responseCharLength, responseCharOffset and responseSnapshot fail before execution. The complete provider request is limited to 4 MiB, with ordinary tool and shared runtime input limits also applying. Retain uncertain candidates and verify deciding evidence before action.

For choosing a question and request shape, see [prompt recipes](jev-workflows.md); use live `scheme jev` as the contract authority.
