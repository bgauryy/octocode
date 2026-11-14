# ghCloneRepo: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GCL-01 | Normal | Materialize an owned clean checkout of cli/cli at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 limited to internal/config with historyDepth 1. Verify HEAD and exact selected source content, then repeat the same request and report reuse honestly. |
| GCL-02 | Advanced | In the isolated campaign cache only, request cli/cli at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 with another paths selection and another historyDepth. Verify distinct checkout identities and their selected files/history. Make one local edit in an owned checkout, retry its refresh, and verify refusal preserves the edit. |
| GCL-03 | Workflow | After a clean owned clone of vitejs/vite at f898888222297aa56d6fc281c585d4d4e4d51f58 scoped to packages/vite/src/node, answer three local declaration questions for mergeConfig, resolveConfig and createServer. Compare the same deciding declarations from targeted remote reads; report clone setup separately from recurring reads. |
| GCL-04 | Invalid | Send historyDepth 0, paths containing a parent traversal, and a missing repo. Preserve errors. Treat invalid ref/provider not-found separately from schema refusal. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
