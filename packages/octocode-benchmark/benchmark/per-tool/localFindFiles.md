# localFindFiles

Find local files/directories by name, size, time, or permissions — returns
paths/metadata, **not** content (use `localSearchCode` for contents). Nothing is
excluded by default — pass `excludeDir` to prune build/vendor dirs. Absolute path.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)
```

## Params (`tools localFindFiles --scheme`)

| param | type | notes |
|---|---|---|
| path | string **req** | absolute |
| names | array<string> | basename globs; matches ANY |
| pathPattern | string | glob over full path (monorepo slicing) |
| regex | string | Rust regex over basename only |
| minDepth / maxDepth | int 0–100 | depth window (equal = exact band) |
| entryType | enum(f,d) | files or dirs; omit = both |
| empty | boolean | 0-byte files / childless dirs |
| time | object | `{ modifiedWithin?, modifiedBefore?, accessedWithin? }` — `7d`,`2h`,`1w`,`30m` |
| size | object | `{ greater?, less? }` — `100k`,`1m`,`500b` |
| permissions | string | exact permission filter |
| access | enum(executable,readable,writable) | permission-class filter |
| excludeDir | array<string> | prune dirs (nothing excluded by default!) |
| detail | enum(basic,modified,full) | `basic` names only; `modified` +mtime; `full` +size/perms |
| sortBy | enum(modified,name,path,size) | ordering |
| limit / page / itemsPerPage | int | discovery cap + pagination |

## Checks

1. **Basename globs** — `$CLI tools localFindFiles --queries '{"path":"'$ROOT'/packages/octocode-tools-core","names":["scheme.ts","package.json"],"entryType":"f","itemsPerPage":20}' --compact`
   → PASS: only matching files.
2. **Monorepo path glob** — `... '{"path":"'$ROOT'","pathPattern":"packages/*/src/tools/**","entryType":"f","itemsPerPage":20}'` → PASS: tool sources across packages.
3. **Prune build dirs** — `... "excludeDir":["node_modules","dist","coverage","out"]` → PASS: no vendor/build noise.
4. **Size filter** — `"size":{"greater":"100k"},"detail":"full"` → PASS: only large files, with metadata.
5. **Time window** — `"time":{"modifiedWithin":"7d"}` → PASS: recently changed files.
6. **entryType=d** — directories only → PASS: no files.
7. **regex basename** — `"regex":"^(index|main)\\.(ts|js)$"` → PASS: precise basename match.
8. **Pagination** — small `itemsPerPage` → PASS: page 2 preserves filters.
9. **Brace-expansion path glob** — `... '{"path":"'$ROOT'","pathPattern":"packages/{octocode,octocode-mcp}/src/**","entryType":"f","itemsPerPage":20}'` → PASS: files from *both* alternatives, matching shell `{a,b}` semantics (P1 bug: this used to silently match nothing — brace syntax was regex-escaped as a literal substring).
10. **Out-of-range page** — `itemsPerPage` small, `page` set well beyond the true `totalPages` → PASS: `pagination.outOfRange:true` and/or a warning — not a silent empty `files:[]` indistinguishable from "valid last page, no more results".

## Workflows

- **Locate config/entrypoints**: `names`/`regex` → hand paths to `localGetFileContent`.
- **Audit large/stale artifacts**: `size.greater` / `time.modifiedBefore` → cleanup candidates.
