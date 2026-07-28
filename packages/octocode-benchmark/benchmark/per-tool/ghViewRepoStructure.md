# ghViewRepoStructure

Browse a GitHub repository's directory tree before reading files.
Generated/vendor dirs are auto-excluded. Output is `structure[]`; page while
`pagination.hasMore`. Use `ghSearchCode(match:"path")` when you already know a
filename fragment.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghViewRepoStructure --scheme`)

| param | type | notes |
|---|---|---|
| owner | string **req** | |
| repo | string **req** | |
| branch | string | omit = default; unknown ref falls back + warns |
| path | string | `""`/`"."` = root, or a subdir; no leading slash |
| maxDepth | int 0–20 | immediate children unless raised |
| page | int 1–1000 | advance only on `pagination.hasMore` |
| itemsPerPage | int 1–200 | raise to cut round-trips |
| include | string[] | opt-in enrichment: `sizes` (file bytes), `languages` (+`dominantLanguage`), `contributors`, `branches`, `tags` |

## Checks

1. **Root tree** — `$CLI tools ghViewRepoStructure --queries '{"owner":"bgauryy","repo":"octocode","path":"","maxDepth":1,"itemsPerPage":50}' --compact`
   → PASS: top-level dirs/files; vendor dirs excluded.
2. **Scoped subtree + depth** — `... '{"owner":"bgauryy","repo":"octocode","path":"packages","maxDepth":2,"itemsPerPage":50}'`
   → PASS: `packages/*/…` two levels deep.
3. **include enrichment** — add `"include":["sizes","languages"]` → PASS: `fileSizes` + `languages`/`dominantLanguage` present (Go-dominance inferred without a languages-API call).
4. **Pagination** — small `itemsPerPage` → PASS: page 2 continues, `hasMore` honest.
5. **Bad ref fallback** — `"branch":"does-not-exist"` → PASS: falls back to default branch **with a warning** (not a silent success).
6. **404 honesty** — bad path → PASS: structured `next.retryParent` (parent dir) + `next.searchPath` (`ghSearchCode match:"path"`), not a dead-end error.

## Workflows

- **Orient → drill**: root tree → scope to `path` of interest → `ghGetFileContent` the file.
- **Confirm before claim**: an empty `ghSearchCode` is not absence — verify the path exists here first.
