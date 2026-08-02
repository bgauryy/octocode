# localGetFileContent

Read a local file, matched slices, or a line range. `matchRanges[].start/end`
are LSP line anchors. `isPartial:true` → continue with `charOffset` from
`pagination.nextCharOffset`. Absolute path.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)
CORPUS=$ROOT/packages/octocode-benchmark/context/react   # frozen corpus — see BENCHMARK.md "Corpus"
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

1. **Line range** — `$CLI tools localGetFileContent --queries '{"path":"'$CORPUS'/packages/react/package.json","startLine":1,"endLine":30,"minify":"none"}' --compact`
   → PASS: exactly lines 1–30 verbatim.
2. **Matched slice + context** — `... '{"path":"'$CORPUS'/packages/react-reconciler/src/ReactFiberWorkLoop.js","matchString":"scheduleUpdateOnFiber","contextLines":8,"minify":"standard"}'`
   → PASS: merged slices + `matchRanges[]`.
3. **Regex anchor** — `"matchStringIsRegex":true` with a signature pattern → PASS: flexible hit.
4. **minify standard vs none** — same file both ways → PASS: standard is smaller yet faithful; none is byte-exact.
5. **minify symbols** — a large `.ts` with `"minify":"symbols"` → PASS: outline with line numbers.
6. **Large-file paging** — small `charLength` → PASS: `isPartial` + `nextCharOffset` walk to end losslessly.
7. **Not-found honesty** — bad path → PASS: explicit error + "list dir with localViewStructure" hint.
8. **Redaction warning** — a file with a secret-shaped token → PASS: `warnings` note redaction (built-in).
9. **minify:"symbols" over the raw-size threshold** — a file >100KB (generate one, or use a large real file such as a vendored bundle) with `"minify":"symbols"` and no `startLine`/`matchString`/`charLength` → PASS: skeleton returned, no `fileTooLarge` error. Regression guard for the size gate applying to raw/`minify:"none"` reads only (P0 bug: the gate used to fire on source size before minification ran, making `minify:"symbols"` unusable on exactly the large files it exists for).
10. **minify:"none" still gated on raw size** — same large file with `"minify":"none"` and no bounds → PASS: `fileTooLarge` error (the gate must still block a genuinely unbounded verbatim read).
11. **matchString blocks minify (by design)** — anchor `matchString` on text that lives INSIDE A COMMENT, with an explicit `"minify":"standard"` → PASS: the returned slice contains the matched comment line verbatim, plus a warning that minify is not applied to matchString extractions. Regression guard: standard minification used to run AFTER extraction and could delete the very line `matchRanges` pointed at — evidence contradicting its own anchors.

## Workflows

- **Search → read exact**: `localSearchCode` line → read with `startLine/endLine` or `matchString`.
- **Token-cheap orientation**: `minify:"symbols"` to outline a big file before a targeted full read.
