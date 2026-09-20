# Jev CLI contract

Inspect `octocode tools jev --scheme --scheme-view query --json --compact` once. For hidden tool context, inspect `octocode tools <name> --scheme --scheme-view query --json --compact` too. Execute `octocode tools jev --input request.json --json --compact`. In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.

Each query is `{context: {value: ...} | {tool, query}, question: {type, instructions, criteria?}}`. The outer `{queries: [...]}` supports up to five independent queries, one question each. Repeat context explicitly for another question. Do not send `state`, `questions`, `sources`, model or workflow fields. A nested `query` is one ordinary query with that tool's required fields, not another bulk envelope.

Runtime configuration supplies `OCTOCODE_JEV_MODEL` and `OCTOCODE_JEV_KEY`. If the key is in a trusted home env file, use Node's `--env-file="$HOME/.octocode/.env"` option. Never put credentials in request values.

The supported read tools use their normal validation, availability and security policy. Jev runs the context tool, supplies its sanitized bounded result to the provider, and returns `{answer, model, usage, context?}` without retrieved bodies. Recursive Jev, astRewrite and ghCloneRepo are excluded. The model value is the configured alias, not a guarantee of a resolved provider version.

Noul returns probability of yes. Choice compares named alternatives; include insufficient when missing evidence matters. Score uses 2–10 ordered levels and returns their expected zero-based index. Instructions, criterion descriptions and inline values accept string/object/array/null. Use one atomic scoped question; structured instructions stay JSON.

Coverage receipts distinguish bounded query scope from explicitly partial results. Continue only using executable returned requests, or narrow terminally limited queries. Never infer global absence from a page, outline, missing body or execution error. There is no automatic pagination, cross-page probability composition or judgment cache. Ordinary retrieval caching may save network bytes, not repeated provider tokens.

Jev response paging is unsupported: responseCharLength, responseCharOffset and responseSnapshot fail before execution. The complete provider request is limited to 4 MiB, with ordinary tool and shared runtime input limits also applying. Retain uncertain candidates and verify deciding evidence before action.

For a complete request example and current boundaries, see [the tool guide](../../../docs/OCTOCODE_JEV.md). For choosing a question, see [prompt recipes](jev-workflows.md).
