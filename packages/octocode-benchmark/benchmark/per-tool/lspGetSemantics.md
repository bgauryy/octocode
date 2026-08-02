# lspGetSemantics

Run LSP semantic queries — definitions, references, callers/callees, call
hierarchy, hover, symbols, type hierarchy, diagnostics. Two-step recipe: run
`documentSymbols` first (no `lineHint` needed) to get each symbol's line, then
pass that line to a symbol op. **Never guess `lineHint`.**

```bash
CLI="node packages/octocode/out/octocode.js"
ROOT=$(pwd)
```

## Params (`tools lspGetSemantics --scheme`)

| param | type | notes |
|---|---|---|
| uri | string | absolute path / `file:///` — required except `workspaceSymbol` |
| type | enum(definition,references,callers,callees,callHierarchy,hover,documentSymbols,typeDefinition,implementation,workspaceSymbol,supertypes,subtypes,diagnostic) | operation |
| symbolName | string | bare identifier at `lineHint`; workspaceSymbol = fuzzy query |
| lineHint | int | line of `symbolName`; from `documentSymbols` — never guess |
| orderHint | int | disambiguate repeats on a line |
| depth | int 0–20 | recursion for callers/callees/callHierarchy |
| includeDeclaration | boolean=true | references: include declaration site |
| groupByFile | boolean | references: per-file summary |
| contextLines | int 0–100 | source around call sites |
| format | enum(structured,compact) | compact = lower token cost |
| workspaceRoot | string | when workspaceSymbol has no uri |
| page / itemsPerPage | int | pagination |

## Checks (run `documentSymbols` first)

1. **documentSymbols** — `$CLI tools lspGetSemantics --queries '{"uri":"'$ROOT'/packages/octocode-tools-core/src/tools/toolNames.ts","type":"documentSymbols"}' --compact`
   → PASS: outline with each symbol's line (the anchor source). (`N/A` if server missing.)
2. **definition** — anchor a symbol from (1) → `"type":"definition","symbolName":"...","lineHint":<L>` → PASS: declaration location.
3. **references** — `"type":"references"` → PASS: all usages; `groupByFile` gives per-file counts.
4. **callers** — a function symbol → PASS: incoming call sites (narrower than references).
5. **callees** — same function → PASS: outgoing calls.
6. **hover** → PASS: type/signature/docs.
7. **typeDefinition / implementation** → PASS: type decl / concrete impl.
8. **workspaceSymbol** — `"type":"workspaceSymbol","symbolName":"isLocalTool"` → PASS: fuzzy project-wide hits.
9. **diagnostic** — `"type":"diagnostic","uri":...` → PASS: diagnostics or clean.
10. **Honest gate** — no server → PASS: `serverUnavailable`/`unsupported` = capability absence, **not** "no usage".
11. **documentSymbols unsupportedOperation fallback** — a file whose language server lacks `documentSymbolProvider` and has no native (OXC)/markdown outline path (e.g. a Flow-typed `.js` file, or force it by picking a language with no configured server) → PASS: `payload.empty.category:"unsupportedOperation"` **and** a `next.textSearch` hint scoped to that file (`path` = the file, a declaration-shaped `searchText`, `regex:"perl"`) — not a dead end the caller has to rediscover a workaround for (P0 bug).
12. **Fallback regex speaks the file's language** — repeat check 11 against a `.rs` and a `.py` file (any language without a configured server works) → PASS: the `next.textSearch.searchText` uses that language's declaration idiom (`pub` for Rust, `def|class` for Python, `func|type|var|const` for Go) — never a blanket `^export` that guarantees an empty follow-up outside JS/TS.
13. **Stale lineHint is never a silent misbind** — pick a symbol with MULTIPLE same-named occurrences in one file, pass a `lineHint` a few lines off the intended one → PASS: the result's `resolvedSymbol` carries `isAmbiguous:true` and a `lineDeviation` count whenever the bound line differs from the hint — full-confidence resolution is reserved for exact-line or unique-symbol binds. Regression guard: deviations of 1–3 lines (inside the radius-5 search) used to resolve silently under `status:"ok"`.

## Judge notes

Score 0 for check 11 if `next` is absent or missing on an `unsupportedOperation` `documentSymbols` result — the whole point is a caller with no `symbolName` (documentSymbols has none) still gets pointed somewhere useful.

## Workflows

- **Search → semantic proof**: `localSearchCode` (text/AST) anchor → `references`/`callers` = ground truth for impact/dead-code.
- **Impact analysis before an edit**: `callers` + `references` to enumerate every call site.
- **Inheritance trace**: `supertypes`/`subtypes` for a class hierarchy.
