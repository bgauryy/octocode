# Workflow: Local Research

Use when the running repo, local checkout, or installed dependency is source of truth.
Read `references/algorithm.md` first; read `references/octocode.md` only when tool or CLI syntax is unclear.

```text
localViewStructure / localFindFiles
-> localSearchCode for terms, identifiers, or changed anchors
-> localGetFileContent(symbols or matchString)
-> localAnalyzeGraph for file topology when the question is dependencies, dependents, paths, cycles, reachability, or dead code
-> lspGetSemantics for symbol identity: definition, references, callers, callees, hover
-> localSearchCode structural for code shape
```

| Question | Graph operation and required fields | Proof upgrade |
|---|---|---|
| What does this file import? | `dependencies` + `file` (+ optional `depth`) | exact-read returned import edges |
| What imports this file? | `dependents` + `file` (+ optional `depth`) | LSP references/callers for changed symbols |
| How can source reach target? | `path` + `file` + `target` | exact-read every returned edge |
| Which files form cycles? | `cycles` | inspect SCC imports before changing architecture |
| What is reachable from roots? | `reachability` + optional `entrypoints`/`includeTests` | verify inferred roots and truncation |
| What may be dead? | `deadCode` + optional `entrypoints`/`includeTests` | exact read + LSP/search/tests before deletion |

Local-first defaults:
- For package behavior, inspect `node_modules/<pkg>` before GitHub; it is the version that runs.
- For impact, use graph `dependents`/`path` to map files, then LSP references/callers to prove changed symbols.
- For deletion, use graph `deadCode` as candidates, then exact read + LSP excluding declarations + broad search + tests/build.
- Graph edges are syntactic file evidence; LSP is semantic symbol evidence. Do not run the graph for a simple lookup.
- For edits, find a local pattern first, patch the smallest scope, then run the targeted verification.

Use external surfaces only when they answer something local cannot: upstream intent, fixes in newer versions, PR/commit history, source repo tests, or ecosystem alternatives — see `references/workflow-external.md`.

Next: when remote code must be proven with local-grade tools bridge through `references/workflow-combination.md`; for the proof ladder on a local claim load `references/code-research.md`; when the local finding turns into an edit go to `references/workflow-change.md`.
