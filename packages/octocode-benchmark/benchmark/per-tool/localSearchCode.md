# localSearchCode

Search local files for text, regex, or AST patterns to find file + line.
`discovery` = paths only; `paginated` = snippets; `detailed` = snippets + context;
`structural` = AST search with `pattern` or `rule`. Local tools need an
**absolute path**.

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)   # run from repo root
```

## Params (`tools localSearchCode --scheme`) — key ones

| param | type | notes |
|---|---|---|
| path | string **req** | absolute; for clones use `localPath` |
| searchText | string | single string (text or regex); not an array |
| mode | enum(paginated,discovery,detailed,structural) | search shape |
| pattern | string | structural: `$X` one node, `$$$ARGS` node list; modifiers are part of the node |
| rule | string | structural: YAML ast-grep rule (not/inside/has/all/any) |
| regex | enum(smart,fixed,perl) | `smart` default; `fixed` literal; `perl` advanced (lookaheads/backrefs) |
| caseMode | enum(smart,sensitive,insensitive) | `smart` default |
| multiline | enum(off,on,dotall) | `off` default; `dotall` = `.` spans newlines |
| wholeWord / invertMatch | boolean | matchers |
| include / exclude / excludeDir | array<string> | glob/dir filters |
| output | enum(content,files,filesWithout,countLines,countMatches,matchOnly) | `content` default; `files`/`filesWithout` = path-only; `count*` = per-file counts; `matchOnly` = matched substring |
| unique | enum(off,list,count) | dedup matched values; needs `output:"matchOnly"` |
| matchWindow | int | chars around match; needs `output:"matchOnly"` |
| contextLines / matchContentLength / maxMatchesPerFile / maxFiles | int | shaping |
| langType | filter | precise language filter |
| sort / sortReverse / rankingProfile | | relevance ranking |
| page / itemsPerPage / matchPage / maxDepth | int | pagination (matchPage walks one noisy file) |

## Checks

1. **Text** — `$CLI tools localSearchCode --queries '{"path":"'$ROOT'/packages/octocode-tools-core/src","searchText":"buildDirectToolCommandPatterns","maxFiles":20}' --compact`
   → PASS: file + line + snippet anchors.
2. **discovery (paths)** — `... "mode":"discovery"` → PASS: paths only, no snippets.
3. **detailed (context)** — `... "mode":"detailed","contextLines":5` → PASS: self-contained snippets.
4. **structural pattern** — `... '{"path":"'$ROOT'/packages/octocode-tools-core/src/tools","mode":"structural","pattern":"eval($X)"}'` → PASS: AST matches with line/capture anchors (or honest empty — a bare signature without body matches nothing).
5. **structural rule** — a YAML `rule` with `inside`/`has` → PASS: relational match.
6. **regex modes** — same needle with `regex:"fixed"` vs default `smart` → PASS: distinct results, no regex misfire.
7. **output:"filesWithout"** — find files missing a required import → PASS: only non-matching files.
8. **output:"matchOnly" + unique:"count"** — extract + count distinct matches → PASS: deduped values with frequency.
9. **Pagination** — small `maxMatchesPerFile` on a noisy file → PASS: `file.pagination.hasMore` + `matchPage` walks it.
10. **Honest empty** — nonsense searchText → PASS: empty + broaden hint.

## Workflows

- **Find shape → prove identity**: `structural` pattern → feed matched anchor (uri+line) to `lspGetSemantics` for references/callers (semantic proof).
- **Triage → read**: `discovery` for candidate files → `localGetFileContent` for the exact region.
- **Dead-code hunt**: `structural` for a symbol's call shape + `lspGetSemantics` references = 0 → candidate.
