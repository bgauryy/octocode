# localFindDeadCode

Purpose: find likely-unreferenced exports and dead-code clusters with repo-wide reachability. Results are candidates, not deletion proof. Always pass ABSOLUTE paths — relative paths resolve against the process cwd. (Nonexistent structural roots error loudly as of 2026-08-02.)

## Schema

```bash
$CLI tools localFindDeadCode --scheme --json --compact
```

Must show: `path`, optional `entrypoints`, `includeTests`, `excludeDir`, `maxFiles`, `limit`, `page`, `itemsPerPage`.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)   # run from repo root
CORPUS=$ROOT/packages/octocode-benchmark/context/react   # frozen corpus — see BENCHMARK.md "Corpus"
```

## Checks

1. Happy path — bounded package scan:

```bash
$CLI tools localFindDeadCode --queries '{"path":"'$CORPUS'/packages/scheduler","maxFiles":2000,"limit":5,"itemsPerPage":5}' --compact
```

→ exits 0; returns candidate rows or an honest empty result with stats/warnings.

2. Entrypoint override:

```bash
$CLI tools localFindDeadCode --queries '{"path":"'$CORPUS'/packages/scheduler","entrypoints":["'$CORPUS'/packages/scheduler/index.js"],"includeTests":false,"maxFiles":2000,"limit":5}' --compact
```

→ respects explicit roots; does not treat tests as roots when `includeTests:false`.

3. Pagination:

```bash
$CLI tools localFindDeadCode --queries '{"path":"'$CORPUS'/packages/scheduler","maxFiles":2000,"limit":20,"itemsPerPage":3,"page":2}' --compact
```

→ page 2 preserves filters and includes continuation/completeness state.

4. Honest failure:

```bash
$CLI tools localFindDeadCode --queries '{"path":"/definitely/missing","limit":5}' --compact
```

→ reports not-found/invalid path; no false dead-code claim.

5. Low-confidence entrypoint signal — point the tool at an internal subpackage with no `main`/`exports`/`bin` of its own (only consumed via cross-package relative imports), e.g.:

```bash
$CLI tools localFindDeadCode --queries '{"path":"'$CORPUS'/packages/react-reconciler/src","maxFiles":2000,"limit":20}' --compact
```

→ when `entrypointsResolved` comes only from the test-file heuristic (no manifest-derived entrypoint matched), the result includes `confidence:"low"` — not just a `warnings` string a caller can skim past (secondary finding from the 2026-07-28 React benchmark, Q8: 57 uninvestigated candidates from a subpackage scan with no real entrypoint).

6. Entrypoint path forms — pass `entrypoints` as an ABSOLUTE path under the scanned root (the form every other local tool requires):

```bash
$CLI tools localFindDeadCode --queries '{"path":"'$CORPUS'/packages/scheduler","entrypoints":["'$CORPUS'/packages/scheduler/index.js"],"includeTests":false,"limit":5}' --compact
```

→ resolves the same as the scan-relative form (`entrypointsResolved` contains it, no "not found" warning). An unresolvable entrypoint's warning names the accepted forms. Regression guard: absolute paths used to be silently dropped, degrading the scan to a no-entrypoint candidate flood.

7. Out-of-range page honesty — `page` far beyond `totalPages` → the response returns the last REAL page's items with `pagination.currentPage` equal to that clamped page (never echoing the requested number), `pagination.outOfRange:true`, and a warning naming the valid range. Regression guard: page 99 of a 2-page result used to return page-2 items labeled `currentPage:99`.

## Workflows

- Safe delete proof: `localFindDeadCode` candidate → `localGetFileContent` exact export → `lspGetSemantics references` → broad `localSearchCode`/AST/import search → tests/build before deletion.
- Refactor cleanup: run before and after a move; candidates that disappear are cleanup signal, not proof alone.

## Judge notes

Score 0 if the answer says a candidate is safe to delete without independent LSP/search/test proof.
