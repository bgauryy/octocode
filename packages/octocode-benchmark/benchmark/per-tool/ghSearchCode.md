# ghSearchCode

Search code contents or file paths across GitHub repositories. Default-branch,
capped. `match:"path"` finds files cheaply; `match:"file"` returns snippets with
`matchIndices` (snippet offsets, **not** line numbers).

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchCode --scheme`)

| param | type | notes |
|---|---|---|
| keywords | array<string> | terms, all ANDed; alternatives go in separate queries |
| owner | string | user/org scope; alone spans all their repos |
| repo | string | repo name only; requires `owner` |
| extension | string | ext without dot (`ts`) |
| filename | string | match by file name — prefer over keywords when name is known |
| path | string | repo path **prefix** (GitHub `path:` qualifier) |
| language | string | GitHub language qualifier (broader than extension) |
| match | enum(file,path) = file | `file` = contents+snippets; `path` = paths only, cheaper |
| limit | int 1–100 | per page |
| page | int 1–1000 | GitHub caps at ~1000 results (~10 pages) |
| concise | boolean | flat `owner/repo:path`, no snippets |

## Checks

1. **Content search** — `$CLI tools ghSearchCode --queries '{"keywords":["localSearchCode"],"owner":"bgauryy","repo":"octocode","extension":"ts","limit":5}' --compact`
   → PASS: results with path + snippet + `matchIndices`; typed `next`.
2. **Path search (cheap existence)** — `... '{"keywords":["package.json"],"owner":"bgauryy","repo":"octocode","match":"path","concise":true,"limit":5}'`
   → PASS: flat `owner/repo:path` rows, no snippets.
3. **filename anchor** — `... '{"filename":"tsconfig.json","owner":"bgauryy","repo":"octocode","match":"path"}'`
   → PASS: hits the named file(s) only.
4. **language qualifier** — `... '{"keywords":["structuralSearch"],"owner":"bgauryy","repo":"octocode","language":"Rust","limit":5}'`
   → PASS: only Rust files.
5. **Pagination** — repeat check 1 with `"limit":2` then `"page":2` → PASS: page 2 preserves filters; `pagination.nextPage` honest.
6. **Honest empty** — search a nonsense keyword → PASS: explicit empty state that does **not** claim absence (suggests structure/clone verification).

## Workflows

- **Locate → read**: `match:"path"` to confirm a file exists → feed the path to `ghGetFileContent`.
- **Search → prove**: content search returns a candidate → `ghCloneRepo` the subtree → `localSearchCode` for bounded local proof (snippets are discovery, not proof).
