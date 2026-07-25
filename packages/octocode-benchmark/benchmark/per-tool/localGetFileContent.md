# localGetFileContent

Read a local file, matched slices, or a line range. `matchRanges[].start/end`
are LSP line anchors. `isPartial:true` → continue with `charOffset` from
`pagination.nextCharOffset`. Absolute path.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)
```

## Params (`tools localGetFileContent --scheme`)

| param | type | notes |
|---|---|---|
| path | string **req** | absolute file (dirs → localViewStructure) |
| fullContent | boolean | whole file (small only) |
| matchString | string | anchor text/regex → merged slices + `matchRanges[]` |
| matchStringIsRegex / matchStringCaseSensitive | boolean | anchor modifiers |
| startLine / endLine | int | range; both required together |
| contextLines | int 0–100 | lines around each matchString hit |
| charOffset / charLength | int | paging; copy `nextCharOffset` |
| minify | enum(none,standard,symbols) | none=verbatim; standard=compact; symbols=outline |

## Checks

1. **Line range** — `$CLI tools localGetFileContent --queries '{"path":"'$ROOT'/packages/octocode-tools-core/package.json","startLine":1,"endLine":30,"minify":"none"}' --compact`
   → PASS: exactly lines 1–30 verbatim.
2. **Matched slice + context** — `... '{"path":"'$ROOT'/packages/octocode-tools-core/src/tools/directToolCatalog.meta.ts","matchString":"buildKnownDirectToolCommandPatternQueries","contextLines":8,"minify":"standard"}'`
   → PASS: merged slices + `matchRanges[]`.
3. **Regex anchor** — `"matchStringIsRegex":true` with a signature pattern → PASS: flexible hit.
4. **minify standard vs none** — same file both ways → PASS: standard is smaller yet faithful; none is byte-exact.
5. **minify symbols** — a large `.ts` with `"minify":"symbols"` → PASS: outline with line numbers.
6. **Large-file paging** — small `charLength` → PASS: `isPartial` + `nextCharOffset` walk to end losslessly.
7. **Not-found honesty** — bad path → PASS: explicit error + "list dir with localViewStructure" hint.
8. **Redaction warning** — a file with a secret-shaped token → PASS: `warnings` note redaction (built-in).

## Workflows

- **Search → read exact**: `localSearchCode` line → read with `startLine/endLine` or `matchString`.
- **Token-cheap orientation**: `minify:"symbols"` to outline a big file before a targeted full read.
