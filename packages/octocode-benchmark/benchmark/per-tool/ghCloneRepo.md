# ghCloneRepo

Clone a GitHub repo or subtree locally for repeated reads, local search, or LSP.
Requires local + clone enabled. Clones are shallow and cached (24h) unless
`forceRefresh`. `sparsePath` limits checkout to a repo-relative file/dir.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghCloneRepo --scheme`)

| param | type | notes |
|---|---|---|
| owner | string **req** | |
| repo | string **req** | |
| branch | string | branch/tag/SHA; omit = default |
| sparsePath | string | repo-relative file/dir; omit = full shallow clone |
| forceRefresh | boolean | re-clone, bypass 24h cache |

## Checks

1. **Full shallow clone** — `$CLI tools ghCloneRepo --queries '{"owner":"bgauryy","repo":"octocode"}' --compact`
   → PASS: `localPath` (absolute) + `next.localSearch`/`next.viewStructure` ready params. (`N/A` if clone disabled.)
2. **Subtree clone** — `... '{"owner":"bgauryy","repo":"octocode","sparsePath":"packages/octocode-tools-core"}'`
   → PASS: only that subtree checked out; `localPath` points at it.
3. **Cache hit** — repeat check 1 → PASS: served from cache (fast), same `localPath`.
4. **forceRefresh** — add `"forceRefresh":true` → PASS: re-clones, bypasses cache.
5. **Bad sparsePath honesty** — nonexistent path → PASS: honest error / empty checkout, suggests `ghViewRepoStructure` to verify.

## Workflows

- **Clone once → analyze many**: clone → `localSearchCode` (AST/text) + `lspGetSemantics` (references/callers) on `localPath` — bypasses GitHub API rate limits.
- **Remote-as-local proof**: turn a GitHub answer that needs AST/LSP into a bounded local proof via the checked-out subtree.
