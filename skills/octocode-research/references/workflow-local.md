# Workflow: Local Research

Use when the running repo, local checkout, local artifact, or installed dependency is source of truth.
Read `algorithm.md` first; read `octocode.md` only when tool or CLI syntax is unclear.

```text
localViewStructure / localFindFiles
-> localSearchCode for terms, identifiers, or changed anchors
-> localGetFileContent(symbols or matchString)
-> lspGetSemantics for definition, references, callers, callees, hover
-> localSearchCode structural/OQL when shape, reachability, or drift matters
```

## Artifacts (archives / binaries / native)

When the lead is a zip, tarball, `.node`, wasm, or other binary — not source text:

```text
localBinaryInspect mode:inspect|list|strings
-> list before extract; strings for printable leads
-> extract|decompress|unpack one bounded entry
-> continue with the normal local spine on the returned path
```

Do not claim contents from filenames alone. Prefer `list`/`strings` before `extract`; after unpack, treat the landed path as a normal local corpus.

Local-first defaults:
- For package behavior, inspect `node_modules/<pkg>` before GitHub; it is the version that runs.
- For impact claims, diff broad text hits against LSP results before saying "unused", "only", or "safe".
- For edits, find a local pattern first, patch the smallest scope, then run the targeted verification.

Use external surfaces only when they answer something local cannot: upstream intent, fixes in newer versions, PR/commit history, source repo tests, or ecosystem alternatives — see `workflow-external.md`.

Validate: `node scripts/eval-research.mjs --case local-research`.
