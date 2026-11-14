# ghSearchCode: independently checkable questions

Use the [shared protocol](README.md) and [rating rubric](../../RATINGS.md).

| ID | Case | Question |
|---|---|---|
| GHC-01 | Normal | Find the mergeConfig declaration in vitejs/vite packages/vite/src/node. Distinguish default-branch search-index discovery from source lines read at f898888222297aa56d6fc281c585d4d4e4d51f58. Read the deciding declaration at that commit. |
| GHC-02 | Advanced | Find code paths matching mergeConfig in vitejs/vite packages/vite/src/node at requested ref f898888222297aa56d6fc281c585d4d4e4d51f58, using a one-row tool window and a small response window. Read every selected page and explain what the index can and cannot prove about that ref. |
| GHC-03 | Empty | Search cli/cli for octocode_benchmark_no_such_symbol_20261114. Report the actual coverage; do not infer repository-wide source absence from an index miss. |
| GHC-04 | Invalid | Submit a request without owner, one without any search selector, and a wrong-type keywords value. Preserve the actionable repairs and verify the corrected request. |
| GHC-05 | Views | Search cli/cli path names under internal/config, once with fields path and once with matchIn path. Verify path hits do not invent content lines. Check repository identity if the provider redirects an observed public repository; report redirect coverage as unverified unless actually observed. |
