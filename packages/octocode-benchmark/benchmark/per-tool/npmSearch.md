# npmSearch

Look up npm packages and find their source repositories. Mainly to resolve an
npm package → its source repo. Exact names return one rich result; keyword
queries return paginated lean lists. `repositoryDirectory` scopes follow-up
GitHub tree/search calls.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools npmSearch --scheme`)

| param | type | notes |
|---|---|---|
| packageName | string **req** | exact name or keyword query; scoped needs full scope (`@octokit/rest`) |
| keywords | string \| array<string> | registry keyword query (array joined with spaces) |
| page | int 1–1000 | keyword results; exact-name = page 1 |

## Checks

1. **Exact name** — `$CLI tools npmSearch --queries '{"packageName":"zod"}' --compact`
   → PASS: one rich result incl. `repository` (owner/repo) + `repositoryDirectory`.
2. **Scoped package** — `... '{"packageName":"@octokit/rest"}'` → PASS: resolves the scoped name.
3. **Keyword query (paginated)** — `... '{"keywords":"state management"}'` → PASS: lean list; `pagination.hasMore`.
4. **Keyword page 2** — `... '{"keywords":"state management","page":2}'` → PASS: distinct results, pagination preserved.
5. **Monorepo dir scope** — a package inside a monorepo → PASS: `repositoryDirectory` points at its subdir.
6. **Honest empty** — misspelled name → PASS: empty + "check spelling / scoped vs unscoped" hint.

## Workflows

- **Package → source → read**: resolve `owner/repo` (+ `repositoryDirectory`) → `ghViewRepoStructure`/`ghSearchCode` scoped to that subdir → `ghGetFileContent`.
- **Verify-don't-assume**: the published `lib/` can mislead (e.g. a JS wrapper over a native core) — confirm the real implementation language in the source repo.
