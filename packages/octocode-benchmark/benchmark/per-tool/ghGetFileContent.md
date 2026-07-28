# ghGetFileContent

Read a file, a matched slice, a line range, or a clone-backed directory from a
GitHub repo. `matchRanges[].start/end` are LSP line anchors. `isPartial:true` →
continue with `charOffset` from `pagination.nextCharOffset`.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghGetFileContent --scheme`)

| param | type | notes |
|---|---|---|
| owner | string **req** | |
| repo | string **req** | |
| path | string **req** | repo-relative, exact case, no leading slash |
| branch | string | branch/tag/SHA; omit = default branch |
| startLine / endLine | int | range; both required together; excl. with fullContent/matchString |
| fullContent | boolean | whole file (small files only) |
| matchString | string | anchor text/regex → merged slices + `matchRanges[]` |
| matchStringIsRegex / matchStringCaseSensitive | boolean | anchor modifiers |
| contextLines | int 0–100 | extra lines around each matchString hit |
| type | enum(file,directory) = file | `directory` materializes a subtree (clone enabled) → `localPath` |
| forceRefresh | boolean | bypass cache |
| charOffset / charLength | int | paging; copy `nextCharOffset`, don't compute |
| minify | enum(none,standard,symbols) | none=verbatim; standard=strip comments/blanks; symbols=outline |

## Checks

1. **Line range** — `$CLI tools ghGetFileContent --queries '{"owner":"bgauryy","repo":"octocode","path":"package.json","startLine":1,"endLine":20,"minify":"none"}' --compact`
   → PASS: exactly lines 1–20 verbatim.
2. **Matched slice** — `... '{"owner":"bgauryy","repo":"octocode","path":"README.md","matchString":"octocode","contextLines":3}'`
   → PASS: merged slices + `matchRanges[]` line anchors.
3. **Regex anchor** — add `"matchStringIsRegex":true` with a signature-style pattern → PASS: flexible anchor hits.
4. **symbols minify** — read a large `.ts` with `"minify":"symbols"` → PASS: outline with line numbers, far smaller than full.
5. **Directory materialize** — `... '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core","type":"directory"}'`
   → PASS: `localPath` + `next.localSearch`/`next.viewStructure` ready params. (`N/A` if clone disabled.)
6. **404 honesty** — bad path → PASS: 404 with "verify path/branch or list tree" hint, no fake content.

## Workflows

- **Search → read exact**: `ghSearchCode` path → `ghGetFileContent` with `matchString` → exact `matchRanges` anchors.
- **Fetch subtree → local intelligence**: `type:"directory"` → pass `localPath` to `localSearchCode` (AST) / `lspGetSemantics`.
- **Large-file paging**: `charLength` small → follow `isPartial`/`nextCharOffset` losslessly to the end.
