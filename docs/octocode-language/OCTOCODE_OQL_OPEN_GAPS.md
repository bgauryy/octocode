# Octocode OQL — Open Gaps (continuation / coverage)

Last updated 2026-06-22.

These are the OQL gaps that remain **open after** the correctness fixes in the
parity checklist's gap log (items 1–14). None of these is a correctness bug —
each is a *coverage* or *continuation* gap: the backend call exists and the
result is honest, but the OQL surface does not yet expose the full workflow as a
first-class, executable shape. Until each is closed, an agent should fall back
to the raw tool (`tools <name>`) or quick command for that specific workflow.

Authoritative contract:
`docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md`
Audit guide:
`docs/octocode-language/OCTOCODE_SEARCH_PARITY_CHECKLIST.md`

Status legend: ⬜ open · 🟡 partial · ✅ closed.

---

## 7. ⬜ `clone` / `cache fetch` first-class materialization checkpoint

**What works today.** `search` materializes internally: a GitHub query with a
local-only predicate (`structural`, `pcre2`, negation, field `size`/`modified`)
and `materialize.mode:"auto"/"required"` clones a bounded subtree
(`ghCloneRepo`) and runs the local tool. Provenance carries
`backend:"ghCloneRepo"`, `materializedPath`, and `cache:"hit"|"miss"`.

**What is missing.** There is no first-class *checkpoint* result that an agent
can run on its own to clone/fetch once and then issue many follow-up queries
against the returned local path. Today materialization is a side-effect of a
search, not an addressable step.

**Target shape.**
- A `materialize`-only invocation (no `where`) returns a result row carrying
  `localPath`, `repoRoot`, `source`, `ref`, `cache`, and `complete`.
- That row exposes `next.search` / `next.structure` / `next.fetch`
  continuations whose `from` is `{kind:"materialized",localPath,…}`.
- Diagnostics: `staleCache`, `materializationFailed`,
  `materializationNotAllowed`, full-repo risk.

**Backed by.** `ghCloneRepo` (already wired through
`adapters/materialize.ts`). The work is surfacing it as a target/continuation,
not new backend behavior.

**Fallback until closed.** Use the `clone` command / `ghCloneRepo` raw tool,
then run local `search` against the returned path.

---

## 8. ⬜ `diff` — direct-file diff vs PR-patch diff split

**What works today.** `target:"diff"` is PR-patch only: it routes to
`ghHistoryResearch` with `content:{patches:{mode:"all"}}` and `params` carrying
`{prNumber, files?}`. Rows are `recordType:"diff"`.

**What is missing.** The contract names two distinct diff workflows that must be
represented separately:
1. **PR patch diff** — hunks/patches selected from a pull request (works).
2. **Direct file diff** — diffing a file/range between two refs or between local
   and remote. There is no OQL shape for this; a direct-file diff request is
   silently treated as a PR-patch call.

**Target shape.** Distinguish by `params` (or a typed sub-shape):
- `{prNumber}` → PR patch lane (current).
- `{baseRef, headRef, path}` → direct-file diff lane, with `next.diff` /
  `next.charRange` continuations and per-file additions/deletions.
- A diff request that fits neither lane should return a repair diagnostic, not
  an empty PR call.

**Fallback until closed.** Use the `diff` command / PR patch selectors directly.

---

## 9. ⬜ `unzip` / binary `unpack` — extracted-localPath follow-up continuations

**What works today.** `target:"artifacts"` routes to `localBinaryInspect` with
`params.mode` in `inspect|list|extract|decompress|strings|unpack`. Rows are
`recordType:"artifact"` and `stableId` already prefers `localPath` over `path`,
so an extracted path is *citeable*.

**What is missing.** When `extract`/`decompress`/`unpack` writes a new local
directory, the row does not emit executable `next.*` continuations to *continue
research in the extracted tree* (e.g. `next.structure` to list it, `next.search`
to grep it, `next.fetch` to read a file). The agent has the path but must
hand-author the follow-up query.

**Target shape.** Artifact rows that created a derived local path emit:
- `next.structure` → `{target:"structure",from:{kind:"local",path:localPath}}`
- `next.search` / `next.fetch` rooted at `localPath`
- `next.artifactEntries` / `next.artifactStrings` for archive entry / string
  scan pagination (today these page through raw `params`).

**Fallback until closed.** Read the extracted `localPath` from the row, then run
local `search`/`structure`/`cat` against it.

---

## 10. ⬜ `lsp` quick command vs raw `documentSymbols` parity narrative

**What works today.** `target:"semantics"` reaches all nine raw LSP operations
through `params.type` (`definition`, `references`, `callers`, `callees`,
`callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`,
`implementation`). Remote semantics materialize first (clone → local LSP).

**What is missing.** A clear, documented parity story between:
- the quick `lsp` command (which does **not** expose `documentSymbols`),
- `ls --symbols` / file outline,
- and raw `lspGetSemantics type:"documentSymbols"`.

The capability exists via `params`, but there is no narrative telling an agent
which surface is authoritative for a symbol outline, nor typed `next.semantic`
continuations from a file/line/symbol anchor.

**Target shape.**
- Document that `documentSymbols` is reachable only via
  `target:"semantics",params:{type:"documentSymbols"}` or raw LSP, and that
  `ls --symbols` is the quick-command outline path.
- Emit `next.semantic` continuations from code/content rows that carry a
  file/line/symbol anchor.

**Fallback until closed.** Use raw `lspGetSemantics` for `documentSymbols`;
`ls --symbols` for a quick outline.

---

## 11. ⬜ PR content `none|standard`-only (no `symbols`) visibility

**What works today.** File content (`target:"content"`) supports all three
views — `exact`/`compact`/`symbols` (→ `minify:none|standard|symbols`).

**What is missing.** PR bodies/patches (`pullRequests`/`diff` via
`ghHistoryResearch`) only support `none|standard` — there is **no `symbols`
view** for PR content. Today nothing tells the agent that a `symbols`-view
request against PR content is unsupported; it can silently fall back.

**Target shape.** When a `symbols` content view is requested for PR/diff
content, return a `signatureUnsupported` (or `partialResult`) diagnostic and a
content-fetch continuation in `none`/`standard`, instead of silently degrading.

**Fallback until closed.** For PR content, request `exact`/`compact` only; treat
`symbols` as a file-content-only view.

---

## 12. ⬜ Structural search metavariable captures (`metavars`) not returned

**What works today.** `target:"code"` with `where.kind:"structural"` (pattern or
JSON rule) runs the local structural engine and returns match rows with
`path`/`line`/`column`/`snippet`, plus the engine's AST node `kind`.

**What is missing.** The contract and parity checklist say structural metavars
(`$X`, `$$$ARGS`) must "survive in row data and renderer", and `OqlCodeResultRow`
has a `metavars` field — but the backing tool's match type
(`LocalSearchCodeMatch`) exposes **no capture/metavariable field**. So
`select:["metavars"]` and `view:"detailed"` return rows with `metavars`
undefined. This is a backend coverage gap, not a mapper bug: the OQL layer does
not (and must not) fabricate captures the engine never produced.

**Target shape.** Either (a) the structural engine returns per-match captures
that `mapCodeResult` forwards into `row.metavars`, or (b) until then OQL emits a
`partialResult`/`signatureUnsupported`-style diagnostic when a structural query
explicitly `select`s `metavars`, so the absence is explicit rather than silent.

**Fallback until closed.** Use the matched `snippet` + `line` and re-read exact
content; do not rely on `metavars` for structural proof yet.

> Note (not a gap): `fetch.content.range.contextLines` applies to **match-
> anchored** reads (`fetch.content.match`), not to explicit `startLine/endLine`
> ranges — there the range itself is the exact window. This is intentional tool
> semantics, verified by `tests/oql/content-views.test.ts`.

## Summary table

| # | Gap | Backend exists | Missing piece | Fallback |
|---|---|---|---|---|
| 7 | clone/cache checkpoint | ✅ `ghCloneRepo` | addressable materialize step + continuations | `clone` command |
| 8 | diff direct-file vs PR-patch | ✅ `ghHistoryResearch` | direct-file diff lane | `diff` command |
| 9 | unzip/unpack continuations | ✅ `localBinaryInspect` | extracted-path `next.*` | read `localPath`, local tools |
| 10 | lsp vs documentSymbols | ✅ `lspGetSemantics` | parity narrative + `next.semantic` | raw LSP / `ls --symbols` |
| 11 | PR content symbols | ✅ `ghHistoryResearch` | `signatureUnsupported` on PR `symbols` | request `none`/`standard` |
| 12 | structural metavars | 🟡 `localSearchCode` (no captures) | engine captures → `row.metavars` | use `snippet`+`line`, re-read exact |

These are tracked as items 7–12 in the parity checklist's **Current Gap Log**.
