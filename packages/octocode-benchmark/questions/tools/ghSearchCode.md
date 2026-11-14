# ghSearchCode: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GHC-01 | Normal | Find the mergeConfig declaration in vitejs/vite packages/vite/src/node. Distinguish default-branch search-index discovery from source lines read at f898888222297aa56d6fc281c585d4d4e4d51f58. Read the deciding declaration at that commit. |
| GHC-02 | Advanced | Find code paths matching mergeConfig in vitejs/vite packages/vite/src/node at requested ref f898888222297aa56d6fc281c585d4d4e4d51f58, using a one-row tool window and a small response window. Read every selected page and explain what the index can and cannot prove about that ref. |
| GHC-03 | Empty | Search cli/cli for octocode_benchmark_no_such_symbol_20261114. Report the actual coverage; do not infer repository-wide source absence from an index miss. |
| GHC-04 | Invalid | Submit a request without owner, one without any search selector, and a wrong-type keywords value. Preserve the actionable repairs and verify the corrected request. |
| GHC-05 | Views | Search cli/cli path names under internal/config, once with fields path and once with matchIn path. Verify path hits do not invent content lines. Check repository identity if the provider redirects an observed public repository; report redirect coverage as unverified unless actually observed. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
