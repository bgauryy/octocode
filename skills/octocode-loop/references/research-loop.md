# General Research Loop

Load when the goal and research path are clear and the work lives on a single surface (one repo, one package, one local tree) — orient, search, read exact evidence, prove, refine. For loop mechanics and stopping see `loop-protocol.md`; for tool names and `status` see `tools.md`.

## Shape

```
Frame → [ orient → search → read → prove ] × N → synthesize
```

Each bracket is one Act→Observe→Learn iteration. Move down the cost ladder only when the cheap rung justifies it.

## Cost ladder (cheap → expensive)

1. **Orient** — map structure before reading. List the tree / repo layout; find files by name or path. Cheap, narrows the search box. (`--tree`, path search, `ghViewRepoStructure`.)
2. **Search** — concise/discovery first: text or OQL search scoped to the oriented box; symbols-only or path-only when you only need *where*. Capture match anchors.
3. **Read** — exact slices at the captured anchors before whole files. Expand to full files only when the slice is ambiguous.
4. **Prove** — confirm the claim with the strongest cheap proof available: an exact-string read at the line, an AST/structural match for code shape, an LSP op for symbol identity, or history for "when/why". A lead is not a conclusion until proven.

## Iteration discipline

- One scoped call per iteration; read `status` before deciding the next (`empty` → adjust one variable or broaden; `error` → fix and retry; results → extract anchors).
- Carry anchors and `next.*` cursors forward verbatim. Follow returned pagination/offsets — never invent them.
- Keep the alternate hypothesis alive until an observation kills it.

## Worked example (find where a config flag is enforced)

1. Frame: "Where is `X` enforced? End when I have the file:line that gates on it, proven by an exact read."
2. Orient: tree the likely dir → narrows to `src/config` + `src/middleware`.
3. Search: text search `X` path-scoped to those dirs → 3 match anchors.
4. Read: exact slices at the 3 anchors → 2 are reads, 1 is the gate.
5. Prove: exact-string read at the gate line + (if needed) an LSP/AST check that it's the live branch. Alternate ("flag unused") killed by the gate hit.
6. Stop: question answered, alternate dead. Emit anchor + one-line trace.

## Convergence

Converged when the framed question has a grounded answer and the alternate is eliminated. If two iterations add nothing new, switch query shape (text↔structural↔semantic↔path) or surface rather than repeating — see `loop-protocol.md` (Escaping stalls). For high-stakes answers, re-run from a second entry angle and reconcile (best-of-K).
