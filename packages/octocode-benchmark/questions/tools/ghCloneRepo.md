# ghCloneRepo: independently checkable questions

Use the [shared protocol](README.md) and [rating rubric](../../RATINGS.md).

| ID | Case | Question |
|---|---|---|
| GCL-01 | Normal | Materialize an owned clean checkout of cli/cli at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 limited to internal/config. Verify HEAD and exact selected source content, then repeat the same request and report reuse honestly. |
| GCL-02 | Advanced | In the isolated campaign cache only, request cli/cli at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 with another paths selection. Verify distinct checkout identities, their selected files and that each holds the commit's full history. Make one local edit in an owned checkout, retry its refresh, and verify refusal preserves the edit. |
| GCL-03 | Workflow | After a clean owned clone of vitejs/vite at f898888222297aa56d6fc281c585d4d4e4d51f58 scoped to packages/vite/src/node, answer three local declaration questions for mergeConfig, resolveConfig and createServer. Compare the same deciding declarations from targeted remote reads; report clone setup separately from recurring reads. |
| GCL-04 | Invalid | Send the removed historyDepth field, paths containing a parent traversal, and a missing repo. Preserve errors. Treat invalid ref/provider not-found separately from schema refusal. |
