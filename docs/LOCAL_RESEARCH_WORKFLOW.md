# Local code research workflow

This workflow covers the local-code research layer of the Octocode agentic toolkit. The public local research tools are `localSearch`, `structureSearch`, `astSearch`, `astTopology`, `localFetch`, and `lspSearch`. CLI and MCP share core-owned schemas and routing instructions, with execution in the native Rust runtime. Inspect a tool’s current schema before constructing an unfamiliar call:

```sh
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
node packages/octocode/out/octocode.js scheme astSearch --view query --compact
node packages/octocode/out/octocode.js scheme astTopology --view query --compact
node packages/octocode/out/octocode.js scheme localFetch --view query --compact
node packages/octocode/out/octocode.js scheme lspSearch --view query --compact
```

| Question | Tool and operation | Evidence returned |
|---|---|---|
| Where are files? | `structureSearch`: `files` or `tree` | Paths, directory outlines, and bounded filesystem metadata |
| Where does this text or regex occur? | `localSearch` | Lexical file/line anchors; literal, Rust regex, or PCRE2 |
| What syntax or declarations are present? | `astSearch`: `match`, `symbols`, or `syntaxTree` | Syntax matches, declaration ranges, node kinds and parents |
| How do files depend on each other? | `astTopology`: `operation:"topology"`, selecting `analysis` | Dependencies, dependents, shortest paths, cycles/SCCs, reachability, dead-code candidates |
| What does the source contain? | `localFetch` | Exact content by default; path-only reads are valid, and selectors/views are optional |
| Which symbol is this, and where is it used? | `lspSearch` | Semantic provider results when available, or explicitly labeled syntactic/native fallback evidence |

Start from the evidence already available. A known file needs no discovery pass. Text search can go directly to an exact read; use structural matching only when code shape matters. Graph analysis is useful for file relationships and candidate selection. It does not prove safe deletion or runtime reachability.

1. Discover or search for a concrete anchor. `localSearch` has no `operation` field. Use `searchText` and `regex:"literal"`, `"rust"`, or `"pcre2"`.
2. When syntax matters, use `astSearch` with `operation:"match"`, exactly one nonblank `pattern` or `rule`, and `langType` for directory searches. A single source file can select its grammar from its extension. `operation:"syntaxTree"` pages one file's node IDs, parent IDs, and source ranges (`nodeOffset`/`nodeLimit`/`namedOnly`). Lines are 1-based and columns 0-based UTF-16; a `symbols` row's `name`+`line` or an identifier capture's `text`+`line` is `lspSearch` `symbolName`+`lineHint` as-is.
3. Read the relevant source with `localFetch`. Omitted `minify` means exact content; `path` alone is valid. Choose `fullContent`, a line range, or `matchString` when a bounded selector is useful. Choose `minify:"standard"` explicitly for compact source, or `"symbols"` for an outline. Security redaction still applies. For `matchString`, `matchedLines` are the exact 1-based LSP anchors; `matchRanges` are padded context windows.
4. Use `lspSearch` with `operation`, an observed `uri`, and either `symbolName` plus 1-based `lineHint` or a zero-based UTF-16 `position`. Use `orderHint` to disambiguate repeated names on the observed line. `documentSymbols` and `diagnostic` require only `uri`; `workspaceSymbol` requires `symbolName` plus `uri` or `workspaceRoot`. Read or re-anchor when the tool reports drift.
5. Verify the proposed change with appropriate tests and the real CLI, MCP, build, or runtime path.

At every step, inspect `status`, `meta.evidence`, and completeness/pagination metadata. Empty and error are different outcomes. Missing output is not evidence of success or absence. Follow returned executable `next.*` queries unchanged; this includes capture expansion, diagnostic pages, and scan expansion. A numeric cursor alone is insufficient. Treat `terminalLimit:true` as an explicit bounded result: narrow the query or record the completeness gap. Inspect `lsp.source`, `truncated`, and `partialReasons` before treating an LSP result as semantic identity.

Whole-response continuations carry `responseSnapshot`. If the result set changes, the tool returns a restart response and an executable offset-zero query. Discard previously collected response pages before restarting.

Keep topology `entrypoints`, `includeTests`, exclusions, scan/result caps, diagnostic settings, and `rustWorkspace` fixed when comparing results. Dynamic imports, shell execution, unresolved edges, and project configuration limit graph conclusions. Native LSP fallbacks are labeled syntactic; their presence does not establish cross-file semantic identity.

The separation follows the underlying interfaces: [ripgrep searches text](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md), [AST patterns match syntax nodes](https://ast-grep.github.io/guide/rule-config/atomic-rule.html), and [LSP supplies document and workspace language features](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/).
