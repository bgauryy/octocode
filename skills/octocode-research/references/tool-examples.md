# Tool examples

Load when translating a question into a raw query. These are input templates, not a required sequence. Inspect the live schema before an unfamiliar call. Replace `/ABS/repo`, file paths, symbol lines, refs, and numbers with observed identities; paths in graph queries are relative to its `path` root.

Each JSON item contains a public tool and its query. Pass only `query` to that tool's `--queries` argument. Select the smallest example that answers the question.

```json
[
  {"tool":"localSearch","query":{"path":"/ABS/repo/src","searchText":"withDataCache","maxFiles":10}},
  {"tool":"astSearch","query":{"operation":"match","path":"/ABS/repo/src","pattern":"withDataCache($$$ARGS)","langType":"typescript","resultView":"files"}},
  {"tool":"astSearch","query":{"operation":"files","path":"/ABS/repo","names":["README.md"],"limit":20}},
  {"tool":"astSearch","query":{"operation":"tree","path":"/ABS/repo/src","maxDepth":1}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/src/example.ts","startLine":1,"endLine":30,"minify":"none"}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/README.md","minify":"symbols"}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/README.md","fullContent":true}},
  {"tool":"astSearch","query":{"operation":"symbols","path":"/ABS/repo/src/example.ts","name":"example"}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/src/example.ts","matchString":"example","contextBytes":256,"chunkType":"bytes","offset":0,"limit":1024}},
  {"tool":"lspSearch","query":{"uri":"/ABS/repo/src/example.ts","operation":"references","symbolName":"example","lineHint":10,"includeDeclaration":false,"pageSize":10}},
  {"tool":"astSearch","query":{"operation":"topology","analysis":"dependents","path":"/ABS/repo","file":"src/example.ts","depth":1}},
  {"tool":"astSearch","query":{"operation":"topology","analysis":"reachability","path":"/ABS/repo","entrypoints":["src/index.ts"],"includeTests":false}},
  {"tool":"astRewrite","query":{"path":"/ABS/repo/src","langType":"typescript","pattern":"console.log($A)","rewrite":"logger.info($A)"}},
  {"tool":"ghSearch","query":{"operation":"repositories","keywords":["octokit"],"language":"TypeScript","pageSize":5}},
  {"tool":"ghSearch","query":{"operation":"code","owner":"octokit","repo":"octokit.js","keywords":["Octokit"],"pageSize":5}},
  {"tool":"ghSearch","query":{"operation":"tree","owner":"octokit","repo":"octokit.js","path":"src","pageSize":5}},
  {"tool":"ghGetFileContent","query":{"owner":"octokit","repo":"octokit.js","path":"src/octokit.ts","branch":"main","matchString":"Octokit","contextBytes":256,"chunkType":"bytes","offset":0,"limit":1024}},
  {"tool":"ghGetFileContent","query":{"owner":"octokit","repo":"octokit.js","path":"README.md","branch":"main","minify":"symbols"}},
  {"tool":"ghGetFileContent","query":{"owner":"octokit","repo":"octokit.js","path":"README.md","branch":"main","fullContent":true}},
  {"tool":"ghSearchHistory","query":{"operation":"commits","owner":"octokit","repo":"octokit.js","path":"src/octokit.ts","pageSize":5}},
  {"tool":"ghGetHistoryItem","query":{"operation":"pullRequest","owner":"octokit","repo":"octokit.js","number":2961,"content":{"changedFiles":true}}},
  {"tool":"ghGetHistoryItem","query":{"operation":"issue","owner":"octokit","repo":"octokit.js","number":2968,"content":{"body":true},"charLength":200}},
  {"tool":"ghGetHistoryItem","query":{"operation":"commit","owner":"octokit","repo":"octokit.js","ref":"main","includeDiff":true}},
  {"tool":"ghGetHistoryItem","query":{"operation":"compare","owner":"octokit","repo":"octokit.js","base":"v4.0.0","head":"v5.0.0","pageSize":5}},
  {"tool":"ghCloneRepo","query":{"owner":"octokit","repo":"octokit.js","branch":"main","sparsePath":"src"}},
  {"tool":"artifactSearch","query":{"type":"npm","packageName":"@octokit/rest"}},
  {"tool":"artifactSearch","query":{"type":"npm","keywords":["octokit"],"pageSize":2}},
  {"tool":"artifactSearch","query":{"type":"pypi","packageName":"requests"}},
  {"tool":"artifactSearch","query":{"type":"crates","keywords":["async","runtime"]}}
]
```

`astRewrite` previews by default. Inspect its exact patch and retain every returned `beforeHash`; apply only with `apply:true`, the complete `expectedHashes` map, and the explicit mutation gate enabled.

`localSearch` takes `searchText` and has no `operation`; choose `regex:"literal"`, `"rust"`, or `"pcre2"`. `localFetch` also accepts `{path:"/ABS/repo/src/example.ts"}` with no selector; omitted `minify` is exact content. For LSP, anchored operations use either `symbolName` plus 1-based `lineHint` or a 0-based UTF-16 `position`; `documentSymbols`/`diagnostic` use only `uri`, and `workspaceSymbol` uses `symbolName` plus `uri` or `workspaceRoot`. Inspect `lsp.source` because native fallback evidence is syntactic. Local line numbers and symbols above are placeholders, not claimed evidence. Search or read them before an anchored LSP call. GitHub examples use public identities but their live content can change; record the returned ref and fetch date. Clone needs persistent local storage and git. Do not use the sparse clone as proof about omitted files.

For continuation, copy the returned `tool` and `query`; do not construct page, match, diagnostic, body, or diff offsets from these examples. Examples demonstrate shape; runtime evidence and completeness still control conclusions.

Next: apply `references/workflow-local.md` or `references/workflow-external.md` to interpret results; use `references/octocode.md` for transport and errors.
