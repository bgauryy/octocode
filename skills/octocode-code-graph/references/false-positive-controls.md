# False-Positive Controls

Load immediately after a graph signal involving cycles, reachability, dead code, barrels, or verification. Why: syntactic topology hides scope and runtime distinctions that commonly reverse the verdict.

| Reported signal | Hidden alternate | Required control |
|---|---|---|
| Cycle with `runtimeCycle:false` or older `static-import` output | the SCC closes only through `import type` | prefer `runtimeCycleEdges`; `static-import` is not runtime proof and may be type-only, so exact-read older output because one type-only back edge kills a runtime-cycle claim |
| `immediateDominator` chokepoint | missing dynamic/framework/config edge bypasses it | verify roots and resolution warnings, then exact-read alternate registrations and LSP consumers |
| `transitiveEdge:true` | direct edge carries a distinct symbol contract or side effect | compare imported bindings and module side effects before calling the edge removable |
| Dead export from a narrow path | tests, scripts, configs, or consumers are outside the scanned graph root | search beyond the scan root; `includeTests` cannot include files outside `path` |
| Unreachable file from one root | package manifest/exports, subpath entrypoint, CLI, plugin, bundler, or framework registration | inspect package manifest and exports for a subpath entrypoint, then search configs and dynamic imports |
| Reachable file | one export inside it is unused | file reachability does not prove symbol or export liveness; run LSP references and export-chain checks |
| Barrel-only export | external consumer or deep import contract | treat the barrel as a possible public contract; check package surface and external consumers before removal |
| Passing assertions | command exits non-zero on coverage, build, lint, or teardown | the process exit code/status controls green; a non-zero coverage failure is failed verification |

Record each control as `alternate → check → killed|unresolved`. A candidate stays below confirmed while an applicable control is unresolved.

Next, load `references/issue-catalog.md` to map surviving signals to impact, then `references/proof-ladder.md` for claim-specific proof.
