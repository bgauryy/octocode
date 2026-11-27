Load when connecting docs, configs or concepts to code, checking whether docs still match the code, or recording a link the graph did not find.

# Links beside the code graph

A build reads every Markdown file, `package.json`, `Cargo.toml`, `tsconfig` and tool registry under its root. Each reference that names exactly one target becomes a **link** that cites its line. Links never count in PageRank, cycles or other code metrics.

| Rule | From | Links when | Otherwise (`links --unresolved`) |
|---|---|---|---|
| `md-link` | `[text](<path>#anchor)`, images, reference definitions | The file or directory exists relative to the doc; `#anchor` matches a heading; `#L10-L20` is in range | `no-such-file`, `no-such-anchor`, `line-out-of-range` |
| `md-path` | A backticked path (`src/a.ts`, `a.ts`) | One file, trying in order: the doc's folder, its package, the root, then a unique suffix | `ambiguous` (candidates), or `no-such-file` when the path starts in this tree |
| `md-cite` | A backticked `path:line[-end]` | The file exists and the lines are in range. A refresh marks it `stale` when the cited lines changed under an unchanged doc line | `line-out-of-range`, `no-such-file` |
| `md-symbol` | A backticked code-shaped name (camelCase, snake_case, `Type::member`, `f()`) | Its section names the declaring file; or the name is qualified and declared once in the doc's package | `unconfirmed` or `ambiguous`, with candidates. Plain words are never mentions |
| `manifest`, `tsconfig` | `bin`, `main`, `exports`, `imports`; `[lib]`/`[[bin]] path`, `build`; `extends`, `references` | The named file exists | `build-output`: a `dist/` or `out/` path, with same-named sources as candidates. The source of a build output is never guessed |
| `contract` | A quoted string literal in a code file (`readResponse('lspSearch', …)`, `"lspSearch"`) | It spells a tool a registry declares: a JSON `tools` array in the MCP tool-definition shape (each entry a string `name` and an object `inputSchema`). The target is `<registry>#<name>`, so code in every language that names one tool meets at one entity | `ambiguous`: two registries declare the name |
| `md-contract` | A backticked name no code declares | One registry declares it | `ambiguous`, with both registries' entries |
| `agent` | `octocode-graph link … add` | Every quote is at its cited lines and occurs once in its file | Refused, with the reason |

## Settle what the rules could not

1. `octocode-graph query <out> "links --unresolved"` lists each open reference with its doc line, `reason` and `candidates`. `--rule md-symbol` narrows the list.
2. Read the doc line and each candidate with `localFetch`; confirm a symbol with `lspSearch` where it matters.
3. Propose the link with a quote from each endpoint's file:

```sh
octocode-graph link <out> add '{"relation":"documents","source":"README.md","target":"src/cli.ts#parseArgs",
  "claim":"the README describes the flag parser","proposedBy":"<agent>",
  "anchors":[{"path":"README.md","line":42,"quote":"`parseArgs` splits flags"},
             {"path":"src/cli.ts","line":10,"quote":"export function parseArgs("}]}'
```

The answer is the stored link with `confidence`, or a refusal: `quote-mismatch`, `quote-not-unique` (quote more), `citation-line`, `citation-file`, `missing-anchor` or `unknown-endpoint`.

| Confidence | When |
|---|---|
| `candidate` | Every quote is verified |
| `likely` | Also, an anchor's quote names the other endpoint (its file or symbol name) |
| `confirmed` | A reviewer other than the proposer ran `link <out> review <id> --by <name>` |

Relations are `documents`, `configures` and `relates`. Over MCP the same calls are `link_graph({ result, action: "add" | "remove" | "review" | "list", … })`. Every build and refresh checks each link again:

- **ok**: the quote is still at its lines.
- **moved**: the quote was found once elsewhere, so the anchor moves and the link stays `ok`.
- **stale**: the quote changed, and a `confirmed` link drops back.
- **missing**: the file is gone.

`diff` lists doc references that broke and citations that went stale since the previous graph.
