# localFindDeadCode

Purpose: find likely-unreferenced exports and dead-code clusters with repo-wide reachability. Results are candidates, not deletion proof.

## Schema

```bash
$CLI tools localFindDeadCode --scheme --json --compact
```

Must show: `path`, optional `entrypoints`, `includeTests`, `excludeDir`, `maxFiles`, `limit`, `page`, `itemsPerPage`.

## Checks

1. Happy path — bounded package scan:

```bash
$CLI tools localFindDeadCode --queries '{"path":"packages/octocode-tools-core/src","maxFiles":2000,"limit":5,"itemsPerPage":5}' --compact
```

→ exits 0; returns candidate rows or an honest empty result with stats/warnings.

2. Entrypoint override:

```bash
$CLI tools localFindDeadCode --queries '{"path":"packages/octocode/src","entrypoints":["packages/octocode/src/index.ts"],"includeTests":false,"maxFiles":2000,"limit":5}' --compact
```

→ respects explicit roots; does not treat tests as roots when `includeTests:false`.

3. Pagination:

```bash
$CLI tools localFindDeadCode --queries '{"path":"packages/octocode-tools-core/src","maxFiles":2000,"limit":20,"itemsPerPage":3,"page":2}' --compact
```

→ page 2 preserves filters and includes continuation/completeness state.

4. Honest failure:

```bash
$CLI tools localFindDeadCode --queries '{"path":"/definitely/missing","limit":5}' --compact
```

→ reports not-found/invalid path; no false dead-code claim.

## Workflows

- Safe delete proof: `localFindDeadCode` candidate → `localGetFileContent` exact export → `lspGetSemantics references` → broad `localSearchCode`/AST/import search → tests/build before deletion.
- Refactor cleanup: run before and after a move; candidates that disappear are cleanup signal, not proof alone.

## Judge notes

Score 0 if the answer says a candidate is safe to delete without independent LSP/search/test proof.
