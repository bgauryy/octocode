# localViewStructure

Browse a local directory tree. `detail:"modified"`/`"full"` switches to
structured `entries[]`. Page while `pagination.hasMore`. Use
`localFindFiles` for name/metadata filters, `localSearchCode(mode:"discovery")`
for files containing text. Absolute path.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)
```

## Params (`tools localViewStructure --scheme`)

| param | type | notes |
|---|---|---|
| path | string **req** | absolute |
| detail | enum(basic,modified,full) | `basic` grouped lists; `modified` +mtime; `full` per-entry size/perms/dates (`entries[]`) |
| hidden | boolean | include dotfiles |
| sortBy | enum(name,size,time,extension) | ordering |
| reverse | boolean | reverse sort |
| pattern | string | glob or substring name filter |
| entryType | enum(f,d) | files only / dirs only; omit for both |
| recursive | boolean | descend (set maxDepth to bound cost) |
| extensions | array<string> | ext whitelist (no dots) |
| maxDepth | int 0–20 | 1 = immediate children; enables recursion on its own |
| limit / page / itemsPerPage | int | discovery cap + pagination |

## Checks

1. **Shallow tree** — `$CLI tools localViewStructure --queries '{"path":"'$ROOT'/packages/octocode-tools-core/src/tools","maxDepth":2,"itemsPerPage":50}' --compact`
   → PASS: two levels of the tools tree.
2. **Files only at depth 1** — `... '{"path":"'$ROOT'/packages/octocode-engine/src","maxDepth":1,"entryType":"f","itemsPerPage":100}'` → PASS: files only.
3. **entryType:"d"** → PASS: dirs only.
4. **extensions whitelist** — `"extensions":["ts"]` → PASS: only `.ts` files (dirs still shown).
5. **detail:"full"** — `"detail":"full"` → PASS: structured `entries[]` with size/perms/dates.
6. **hidden** — `"hidden":true` on repo root → PASS: `.github`, `.git`, etc. appear.
7. **Pagination** — small `itemsPerPage` → PASS: page 2 continues.
8. **Out-of-range page** — small `itemsPerPage`, `page` set well beyond the true `totalPages` → PASS: the response is clamped to the last real page (returns its entries, not an empty list) **and** `pagination.outOfRange:true` plus a warning naming the valid range — not a silent clamp that lets a caller believe `currentPage` was the page it actually requested.

## Workflows

- **Map before read**: view a package's `src/` shape → drill with `localGetFileContent`.
- **Post-clone survey**: after `ghCloneRepo`, view the `localPath` tree before AST/LSP work.
