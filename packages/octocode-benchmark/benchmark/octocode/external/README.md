# External tool benchmark questions

These rows exercise GitHub, npm, clone/materialize, PR/commit history, and external `octocode search` routes. Provider rows are candidate evidence until followed by exact content fetch, PR detail, diff, clone/materialized local proof, or LSP/graph proof.

Record auth state before running. Anonymous GitHub is acceptable only when rate limits and missing private access are reported honestly.

## External Setup

Use these stable aliases:

| Alias | Repository or package |
|---|---|
| `LCJS` | `langchain-ai/langchainjs` |
| `LCPY` | `langchain-ai/langchain` |
| `LGJS` | `langchain-ai/langgraphjs` |
| `LGPY` | `langchain-ai/langgraph` |
| `ZUSTAND` | `pmndrs/zustand` and npm package `zustand` |
| `HERMES_AGENT` | `NousResearch/hermes-agent` |
| `HERMES_ENGINE` | `facebook/hermes` |
| `OPENCLAW` | `Gen-Verse/OpenClaw-RL` plus repository-search candidates |

For any row that depends on live provider data, record `networkUsed:true`, `cacheMode`, rate-limit/auth diagnostics, and resolved branch/ref when returned.

## Schema Questions

| ID | Surface | Question | Command | Pass criteria |
|---|---|---|---|---|
| EXT-SCHEMA-01 | raw-tool | Do GitHub and npm schemes expose all fields used below? | `octocode tools ghSearchCode ghGetFileContent ghViewRepoStructure ghSearchRepos ghHistoryResearch ghCloneRepo npmSearch oqlSearch --scheme --compact --no-color` | Field names are documented: arrays vs strings, PR content selector, independent pages, minify modes, clone sparsePath, npm mode. |
| EXT-SCHEMA-02 | search-cli | Does `search --scheme` document external targets and routing limits? | `octocode search --scheme --compact --no-color` | Active targets include `repositories`, `packages`, `pullRequests`, `commits`, `diff`, `materialize`; evidence semantics warn that GitHub empty is not absence. |
| EXT-SCHEMA-03 | search-cli | Does `search --help` document external shorthand? | `octocode search --help --no-color` | Help explains owner/repo routing, `--repo`, `--materialize`, PR detail, commits, packages, artifacts, diff, and `--explain --dry-run`. |
| EXT-SCHEMA-04 | status | Is auth/cache state visible without leaking secrets? | `octocode status --json --compact` | Reports token/auth/cache state without token values. |

## Tool-By-Tool Questions

| ID | Tool / command | Question | Command or query | Pass criteria |
|---|---|---|---|---|
| EXT-REPO-01 | `ghSearchRepos` | Can repo discovery identify benchmark families with complete candidate identity? | `tools ghSearchRepos --queries '[{"keywords":["langchain"],"limit":5},{"keywords":["langgraph"],"owner":"langchain-ai","limit":5},{"keywords":["zustand"],"limit":5},{"keywords":["open claw"],"limit":10},{"keywords":["hermes"],"limit":10}]' --json --compact` | Five groups return owner/repo, stars, language, description/topic fields when available; OpenClaw uncertainty is candidate-grade. |
| EXT-REPO-02 | `octocode search` | Does repository shorthand lower arrays and filters correctly? | `octocode search "mcp server" --target repositories --lang TypeScript --stars ">100" --limit 5 --json --compact` | Target is `repositories`; output preserves filters and page context. |
| EXT-CODE-01 | `ghSearchCode` | Can GitHub content search preserve snippets and match indices? | `tools ghSearchCode --queries '{"keywords":["streamEvents"],"owner":"langchain-ai","repo":"langchainjs","extension":"ts","match":"file","limit":5,"concise":false}' --json --compact` | Rows include owner/repo/path/snippet/match indices/query context and fetch continuation. |
| EXT-CODE-02 | `ghSearchCode` | Can GitHub path search find files cheaply? | `tools ghSearchCode --queries '{"keywords":["package.json"],"owner":"pmndrs","repo":"zustand","match":"path","limit":10}' --json --compact` | Rows include paths without unnecessary snippets; page continuation is preserved. |
| EXT-CODE-03 | `octocode search` | Does remote shorthand with `--type ts` route to GitHub code search correctly? | `octocode search "_streamChatModelEvents" langchain-ai/langchainjs --type ts --json --compact` | Non-empty TypeScript-scoped results or an honest indexing/auth diagnostic; no local-only language alias bug. |
| EXT-CODE-04 | `octocode search` | Does `--explain --dry-run` reveal provider vs materialized routing? | `octocode search --explain --dry-run --query '{"target":"code","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"where":{"kind":"regex","value":"streamEvents\\(","dialect":"pcre2"},"scope":{"language":"ts"},"materialize":{"mode":"auto"}}' --json` | Plan shows PUSHDOWN/RESIDUAL/ROUTE/UNSUPPORTED decisions and materialization policy without executing. |
| EXT-CONTENT-01 | `ghGetFileContent` | Can GitHub content fetch exact proof from a search anchor? | `tools ghGetFileContent --queries '{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","matchString":"createStore","contextLines":4,"minify":"none"}' --json --compact` | Exact content includes `createStore`, context, match line anchors, and no hidden truncation. |
| EXT-CONTENT-02 | `ghGetFileContent` | Do GitHub minification modes differ and stay paginated? | `tools ghGetFileContent --queries '[{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","fullContent":true,"minify":"symbols"},{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","fullContent":true,"minify":"standard"},{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","startLine":1,"endLine":80,"minify":"none"}]' --json --compact` | Symbols, standard, and exact/range views are distinct; partial flags and ranges are explicit. |
| EXT-CONTENT-03 | `octocode search` | Does OQL content fetch preserve `contentView` mapping? | `octocode search --query '{"target":"content","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src/vanilla.ts"},"fetch":{"content":{"match":{"text":"createStore"},"range":{"contextLines":4},"contentView":"exact"}}}' --json --compact` | Maps to GitHub content proof and keeps exact view plus anchors. |
| EXT-STRUCTURE-01 | `ghViewRepoStructure` | Can remote tree browsing page without losing path/depth? | `tools ghViewRepoStructure --queries '{"owner":"langchain-ai","repo":"langgraphjs","path":"libs","maxDepth":2,"itemsPerPage":10,"page":1}' --json --compact` | Entries preserve repo/path/depth; `hasMore` carries exact page 2 query. |
| EXT-STRUCTURE-02 | `octocode search` | Does OQL structure expose a runnable `next.page`? | `octocode search --query '{"target":"structure","from":{"kind":"github","repo":"langchain-ai/langgraphjs"},"scope":{"path":"libs"},"fetch":{"tree":{"maxDepth":2}},"itemsPerPage":10,"page":1}' --json --compact` | `next.page` keeps repo/path/maxDepth/itemsPerPage and returns a different window when followed. |
| EXT-HISTORY-01 | `ghHistoryResearch` | Can PR list search find relevant discussions? | `tools ghHistoryResearch --queries '{"type":"prs","owner":"langchain-ai","repo":"langchainjs","keywordsToSearch":["streamEvents"],"state":"merged","limit":5,"page":1}' --json --compact` | PR rows include number/title/state/date/author and page context. |
| EXT-HISTORY-02 | `ghHistoryResearch` | Can PR detail deep-read selected content without silently dropping sections? | `tools ghHistoryResearch --queries '{"type":"prs","owner":"langchain-ai","repo":"langchainjs","prNumber":10924,"reviewMode":"full","matchString":"_streamChatModelEvents","itemsPerPage":20}' --json --compact` | Metadata, body, changed files, patches, comments, reviews, and commits are present or independently paginated. |
| EXT-HISTORY-03 | `ghHistoryResearch` | Can PR pagination continue body/comments/files/commits independently? | Repeat EXT-HISTORY-02 with `itemsPerPage:5`, then follow `filePage`, `commentPage`, `commitPage`, `charOffset`, or `commentBodyOffset` as exposed. | Page 2 of each section preserves PR number, selector, and match filters. |
| EXT-HISTORY-04 | `ghHistoryResearch` | Can commit history be path-scoped and paginated? | `tools ghHistoryResearch --queries '{"type":"commits","owner":"langchain-ai","repo":"langchainjs","path":"libs/langchain-core","perPage":5,"page":1}' --json --compact` | Commit rows include SHA/date/author/message/path context and next page when present. |
| EXT-HISTORY-05 | `octocode search` | Does PR shorthand support list, detail, selected patch, and match string? | `octocode search langchain-ai/langchainjs#10924 --target pullRequests --deep --match-string "_streamChatModelEvents" --json --compact` | Shorthand lowers to target `pullRequests` with PR number, content selector, match string, and independent pagination. |
| EXT-DIFF-01 | `octocode search` | Can direct GitHub file diff compare two refs without PR context? | `octocode search --query '{"target":"diff","from":{"kind":"github","repo":"bgauryy/octocode-mcp"},"params":{"baseRef":"main","headRef":"main","path":"README.md"}}' --json --compact` | Returns base/head/path and either a patch or explicit identical-files diagnostic. |
| EXT-CLONE-01 | `ghCloneRepo` | Can a bounded sparse clone become a local proof corpus? | `tools ghCloneRepo --queries '{"owner":"pmndrs","repo":"zustand","sparsePath":"src"}' --json --compact` | Returns local path/location/source ref; follow-up local tools can use the path as-is. |
| EXT-MATERIALIZE-01 | `octocode search` | Does OQL materialize return source identity and local continuations? | `octocode search --query '{"target":"materialize","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src"},"materialize":{"mode":"required"}}' --json --compact` | Row includes localPath/repoRoot/ref/cache/complete and `next.structure`/`next.files`. |
| EXT-REMOTE-LOCAL-01 | `--repo` quick commands | Can remote-as-local preserve repo-relative paths? | `octocode search "createStore" src --repo pmndrs/zustand --type ts --json --compact`, then `octocode search src/vanilla.ts --repo pmndrs/zustand --mode none --match-string createStore --json --compact`. | First call materializes or fetches; second uses the same repo/path identity and exact content anchor. |
| EXT-NPM-01 | `npmSearch` | Can npm resolve a package to a source repo handoff? | `tools npmSearch --queries '{"packageName":"zustand","mode":"full"}' --json --compact` | Package version, dist tags, repository/source fields, and page context are present. |
| EXT-NPM-02 | `octocode search` | Does packages target distinguish exact package from keyword search? | `octocode search zustand --target packages --mode full --json --compact` | Exact package metadata returns source repo; keyword ambiguity is explicit if present. |
| EXT-OQL-01 | `oqlSearch` | Does raw OQL external code preserve the same evidence contract as search CLI? | Run EXT-CODE-03 once through `octocode search` and once through `tools oqlSearch --queries '<same OQL object>' --json --compact`. | Repo/path/line/snippet/proof/evidence/diagnostics/pagination/continuations are semantically equivalent. |
| EXT-OQL-02 | `octocode search` | Do unsupported or reserved targets fail cleanly? | `octocode search --query '{"target":"dataflow","from":{"kind":"github","repo":"pmndrs/zustand"}}' --json --compact` | Explicit unsupported-target diagnostic; no silent fallback to code search. |

## External Reflection Questions

| Prompt | What to record |
|---|---|
| Issues | Auth/rate-limit gates, GitHub indexing empties, missing repo/path/PR/commit anchors, lossy OQL mapping, missing independent PR pagination. |
| Improvements | Why the product, benchmark, or docs should change; prioritize search-to-fetch and PR pagination gaps. |
| Good flow | Best provider-to-proof chain and the exact continuation that made it work. |
| Instruction gaps | Missing/contradictory guidance in search help, OQL scheme, raw tool schemes, or agent instructions. |
