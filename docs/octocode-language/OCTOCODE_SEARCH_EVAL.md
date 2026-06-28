# octocode search — Eval (questions & tool)

One eval per row: a research **question** and the exact `npx octocode search`
invocation that answers it. No expected output is encoded here — run the tool,
then judge the envelope (`evidence.kind`, `results`, `diagnostics`, `next.*`).

Conventions:
- Run with `npx octocode …`.
- Add `--json` for the machine envelope, `--explain --dry-run` to assert routing
  without executing.
- `<DIR>` = any local path; `<repo>` = `owner/name`.

## Capability coverage map

Every capability below has at least one eval row.

| Capability | Eval rows |
|---|---|
| Minification / content views (`none`/`standard`/`symbols`, PR `none\|standard`) | 9, 10, 10b, 11d |
| Pagination (result page, match page, char window, provider page) | P1, P2, P3, P4, 37 |
| LSP semantics (all 9 types) | 19, 20, 21, 21b, 21c, 21d, 22 |
| AST / structural (pattern, rule, metavars, remote-materialized) | 4, 5, 7 |
| Symbols (minify skeleton + LSP documentSymbols) | 10, 19 |
| GitHub search (code, repos, content, tree, PR/commits) | 6, 7, 11, 14, 23, 27, 29 |
| Clone repos / materialization (clone→local AST/LSP, bounded/unbounded) | 7, 22, 40 |
| Package (npm exact + keyword + repo handoff) | 25, 26 |
| Fetch (line range / file region, local + GitHub) | 9, 11, P3 |
| Match string (literal + regex anchor, with context) | 11b, 11c |
| Context per line (`contextLines`, detailed code view, only-matching) | 8b, 8c, 11b |
| Files / metadata / boolean / negation | 15, 16, 17, 18 |
| Continuations (`next.fetch` / `next.page` / `next.matchPage` / `next.charRange`) | 41, P1, P2, P3 |
| Diagnostics honesty (truncation, candidate, providerUnindexed, unsupported) | 37, 38, 39, 40, 34 |

---

## V1 — code (`localSearchCode` / `ghSearchCode`)

| # | Question | Tool |
|---|---|---|
| 1 | Where is `runOqlSearch` defined/used in this dir? | `npx octocode search "runOqlSearch" <DIR>` |
| 2 | Which lines match the regex `function\s+\w+`? | `npx octocode search --regex 'function\s+\w+' <DIR>` |
| 3 | Find functions whose name is preceded by `handle` (lookahead). | `npx octocode search --regex 'function\s+(?=handle)' <DIR> --pcre2` |
| 4 | Find every `diagnostic(...)` call site (AST). | `npx octocode search --pattern 'diagnostic($$$ARGS)' <DIR> --lang ts` |
| 5 | Find `await` expressions NOT inside a try (relational rule). | `npx octocode search --rule '{"pattern":"await $X","not":{"inside":{"kind":"try_statement","stopBy":"end"}}}' <DIR> --lang ts` |
| 6 | Search `useEffect` in a GitHub repo (provider, no clone). | `npx octocode search "useEffect" <repo> --type tsx` |
| 7 | Prove a structural pattern in a GitHub subtree (clone→local AST). | `npx octocode search --query '{"target":"code","from":{"kind":"github","repo":"<repo>"},"scope":{"path":"src"},"where":{"kind":"structural","lang":"ts","pattern":"eval($X)"},"materialize":{"mode":"auto","strategy":"subtree"}}'` |
| 8 | Does target:"code" without a predicate fail cleanly? | `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"}}'` |
| 8b | **Context per line**: matches with 3 lines of surrounding context (detailed). | `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"diagnostic"},"view":"detailed","controls":{"search":{"matchContentLength":500}}}'` |
| 8c | **Enumerate** each hit on a minified one-liner (`onlyMatching` + window). | `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"regex","value":"\\bdiagnostic\\b"},"controls":{"search":{"onlyMatching":true,"unique":true,"matchWindow":20}}}'` |

## V1 — content (`localGetFileContent` / `ghGetFileContent`)

| # | Question | Tool |
|---|---|---|
| 9 | Read lines 1–40 of a file, exact text. | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/file.ts"},"fetch":{"content":{"range":{"startLine":1,"endLine":40},"contentView":"exact"}}}'` |
| 10 | Read a file as a symbol skeleton (cheap orient — minify `symbols`). | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/file.ts"},"fetch":{"content":{"contentView":"symbols"}}}'` |
| 10b | Read a file compact (minify `standard`/`compact` — default reading). | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/file.ts"},"fetch":{"content":{"contentView":"compact"}}}'` |
| 11 | Read a file region from a GitHub repo at a ref. | `npx octocode search --query '{"target":"content","from":{"kind":"github","repo":"<repo>","ref":"main"},"scope":{"path":"README.md"},"fetch":{"content":{"range":{"startLine":1,"endLine":20}}}}'` |
| 11b | **Match string**: read the region around a matched string, with context lines. | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/file.ts"},"fetch":{"content":{"match":{"text":"runOqlSearch"},"range":{"contextLines":3},"contentView":"exact"}}}'` |
| 11c | **Match string (regex)**: anchor the read on a regex match. | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/file.ts"},"fetch":{"content":{"match":{"text":"function\\s+run","regex":true},"range":{"contextLines":2}}}}'` |
| 11d | PR body content view supports `none`/`standard` only (not `symbols`). | `npx octocode search --query '{"target":"pullRequests","from":{"kind":"github","repo":"<repo>"},"params":{"prNumber":<N>,"minify":"none"}}'` |
| 12 | Is `where` on content rejected (no silent drop)? | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"x"},"where":{"kind":"text","value":"y"}}'` |

## V1 — structure (`localViewStructure` / `ghViewRepoStructure`)

| # | Question | Tool |
|---|---|---|
| 13 | What's the directory tree (depth 1) with sizes? | `npx octocode search --query '{"target":"structure","from":{"kind":"local","path":"<DIR>"},"fetch":{"tree":{"maxDepth":1,"includeSizes":true}}}'` |
| 14 | Browse a GitHub repo's top-level tree. | `npx octocode search --query '{"target":"structure","from":{"kind":"github","repo":"<repo>"},"fetch":{"tree":{"maxDepth":1}}}'` |

## V1 — files (`localFindFiles`)

| # | Question | Tool |
|---|---|---|
| 15 | List all `.ts` files. | `npx octocode search --query '{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"field","field":"extension","op":"=","value":"ts"}}'` |
| 16 | Files containing BOTH term A AND term B (boolean ∩). | `npx octocode search --query '{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"all","of":[{"kind":"text","value":"A"},{"kind":"text","value":"B"}]}}'` |
| 17 | Files that do NOT contain a term (negation, local universe). | `npx octocode search --query '{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"not","predicate":{"kind":"text","value":"TODO"}}}'` |
| 18 | Files modified within the last week. | `npx octocode search --query '{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"field","field":"modified","op":"within","value":"7d"}}'` |

## V2 — semantics (`lspGetSemantics`)

| # | Question | Tool |
|---|---|---|
| 19 | Outline the symbols of a file. | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"documentSymbols"}}'` |
| 20 | Where is symbol X defined? | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"definition","symbolName":"X","lineHint":<N>}}'` |
| 21 | Who calls function X (incoming call hierarchy)? | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"callers","symbolName":"X","lineHint":<N>}}'` |
| 21b | All references to symbol X. | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"references","symbolName":"X","lineHint":<N>}}'` |
| 21c | Hover signature/JSDoc of X. | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"hover","symbolName":"X","lineHint":<N>}}'` |
| 21d | Other 5 LSP ops — `callees`, `callHierarchy`, `typeDefinition`, `implementation` (member), and grouped/paged output. | `npx octocode search --query '{"target":"semantics","from":{"kind":"local","path":"<DIR>/file.ts"},"params":{"type":"callHierarchy","symbolName":"X","lineHint":<N>,"groupByFile":true}}'` |
| 22 | Resolve a symbol semantically in a GitHub file (clone→LSP). | `npx octocode search --query '{"target":"semantics","from":{"kind":"github","repo":"<repo>"},"params":{"type":"definition","uri":"src/x.ts","symbolName":"X","lineHint":<N>}}'` |

## V2 — repositories (`ghSearchRepos`)

| # | Question | Tool |
|---|---|---|
| 23 | Find repos about "tree-sitter", top 5 by relevance. | `npx octocode search --query '{"target":"repositories","params":{"keywords":["tree-sitter"],"limit":5}}'` |
| 24 | Find TypeScript repos with >1000 stars on a topic. | `npx octocode search --query '{"target":"repositories","params":{"topicsToSearch":["parser"],"language":"typescript","stars":">1000"}}'` |

## V2 — packages (`npmSearch`)

| # | Question | Tool |
|---|---|---|
| 25 | What is the npm package `zod` (version, repo)? | `npx octocode search --query '{"target":"packages","params":{"packageName":"zod"}}'` |
| 26 | Discover packages by keyword. | `npx octocode search --query '{"target":"packages","params":{"keywords":["json schema"],"mode":"lean"}}'` |

## V2 — pullRequests / commits / diff (`ghHistoryResearch`)

| # | Question | Tool |
|---|---|---|
| 27 | List merged PRs in a repo. | `npx octocode search --query '{"target":"pullRequests","from":{"kind":"github","repo":"<repo>"},"params":{"state":"merged","limit":5}}'` |
| 28 | Read one PR in full (body, files, reviews). | `npx octocode search --query '{"target":"pullRequests","from":{"kind":"github","repo":"<repo>"},"params":{"prNumber":<N>,"reviewMode":"full"}}'` |
| 29 | Commit history for a path. | `npx octocode search --query '{"target":"commits","from":{"kind":"github","repo":"<repo>"},"params":{"path":"src","limit":5}}'` |
| 30 | The patch/diff of a specific PR. | `npx octocode search --query '{"target":"diff","from":{"kind":"github","repo":"<repo>"},"params":{"prNumber":<N>}}'` |

## V2 — artifacts (`localBinaryInspect`)

| # | Question | Tool |
|---|---|---|
| 31 | Inspect a native binary (format/arch/symbols). | `npx octocode search --query '{"target":"artifacts","from":{"kind":"local","path":"<file>.node"},"params":{"mode":"inspect"}}'` |
| 32 | Extract printable strings from a binary. | `npx octocode search --query '{"target":"artifacts","from":{"kind":"local","path":"<file>.node"},"params":{"mode":"strings","minLength":8}}'` |
| 33 | List entries inside an archive. | `npx octocode search --query '{"target":"artifacts","from":{"kind":"local","path":"<file>.zip"},"params":{"mode":"list"}}'` |

## Pagination (each domain keeps its own cursor)

| # | Question | Tool |
|---|---|---|
| P1 | Page result rows; does `next.page` carry an executable follow-up? | `npx octocode search --query '{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"field","field":"extension","op":"=","value":"ts"},"itemsPerPage":5,"page":1}'` |
| P2 | Page within one noisy file's matches (`matchPage`/`maxMatchesPerFile`). | `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"diagnostic"},"controls":{"search":{"maxMatchesPerFile":5,"matchPage":1}}}'` |
| P3 | Char-window a large content body (`charOffset`/`charLength` → `next.charRange`). | `npx octocode search --query '{"target":"content","from":{"kind":"local","path":"<DIR>/big.ts"},"fetch":{"content":{"charOffset":0,"charLength":2000}}}'` |
| P4 | Page GitHub repo/PR/commit result rows (provider page). | `npx octocode search --query '{"target":"repositories","params":{"keywords":["parser"],"limit":5,"page":2}}'` |

## Reserved (V3) — must refuse

| # | Question | Tool |
|---|---|---|
| 34 | Does a `fixes`/`dataflow` target refuse with `unsupportedTarget`? | `npx octocode search --query '{"target":"dataflow","from":{"kind":"local","path":"."}}'` |

## Cross-cutting — language & flow features

| # | Question | Tool |
|---|---|---|
| 35 | What does the OQL schema expose (targets, params)? | `npx octocode search --scheme` |
| 36 | How does a query route (backend, exactness, materialize)? | `npx octocode search --explain --dry-run --query '<oql>'` |
| 37 | Does a noisy search report match-truncation honestly? | `npx octocode search "diagnostic" <DIR> --json` |
| 38 | Does GitHub regex flag itself as candidate (approximate)? | `npx octocode search --explain --dry-run --query '{"target":"code","from":{"kind":"github","repo":"<repo>"},"where":{"kind":"regex","value":"use[A-Z]"},"materialize":{"mode":"never"}}'` |
| 39 | Does an empty GitHub result warn (`providerUnindexed`) not claim absence? | `npx octocode search --json --query '{"target":"code","from":{"kind":"github","repo":"<repo>"},"where":{"kind":"text","value":"zzq_absent"},"materialize":{"mode":"never"}}'` |
| 40 | Does unbounded materialization refuse to clone the whole repo? | `npx octocode search --query '{"target":"code","from":{"kind":"github","repo":"<repo>"},"where":{"kind":"structural","lang":"ts","pattern":"x($A)"},"materialize":{"mode":"required","strategy":"subtree"}}'` |
| 41 | Do code hits carry an executable `next.fetch` continuation? | `npx octocode search --json "runOqlSearch" <DIR>` (inspect `results[].next["next.fetch"]`) |
| 42 | Does a bounded batch run independent queries? | `npx octocode search --query '{"queries":[{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"a"}},{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"field","field":"extension","op":"=","value":"ts"}}]}'` |
| 43 | Does `combine:"merge"` reject incompatible row kinds? | `npx octocode search --query '{"combine":"merge","queries":[{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"a"}},{"target":"files","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"field","field":"extension","op":"=","value":"ts"}}]}'` |
| 44 | Is an unknown field rejected (`unknownField`)? | `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"a"},"bogus":1}'` |
| 45 | Does shorthand equal the full-JSON form? | `npx octocode search "diagnostic" <DIR>` vs `npx octocode search --query '{"target":"code","from":{"kind":"local","path":"<DIR>"},"where":{"kind":"text","value":"diagnostic"}}'` |
