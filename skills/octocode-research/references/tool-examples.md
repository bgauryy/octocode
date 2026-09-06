# Tool examples

Load when translating a question into a raw query. These are input templates, not a required sequence. Inspect the live schema before an unfamiliar call. Replace `/ABS/repo`, file paths, symbol lines, refs, and numbers with observed identities; paths in graph queries are relative to its `path` root.

Each JSON item contains a public tool and its query. Pass only `query` to that tool's `--queries` argument. Select the smallest example that answers the question.

```json
[
  {"tool":"localSearch","query":{"operation":"text","path":"/ABS/repo/src","searchText":"withDataCache","maxFiles":10}},
  {"tool":"localSearch","query":{"operation":"structural","path":"/ABS/repo/src","pattern":"withDataCache($$$ARGS)","langType":"typescript","resultView":"files"}},
  {"tool":"localSearch","query":{"operation":"files","path":"/ABS/repo","names":["README.md"],"limit":20}},
  {"tool":"localSearch","query":{"operation":"tree","path":"/ABS/repo/src","maxDepth":1}},
  {"tool":"localGetFileContent","query":{"path":"/ABS/repo/src/example.ts","startLine":1,"endLine":30,"minify":"none"}},
  {"tool":"lspGetSemantics","query":{"uri":"/ABS/repo/src/example.ts","type":"references","symbolName":"example","lineHint":10,"includeDeclaration":false,"pageSize":10}},
  {"tool":"localAnalyzeGraph","query":{"operation":"dependents","path":"/ABS/repo","file":"src/example.ts","depth":1}},
  {"tool":"localAnalyzeGraph","query":{"operation":"reachability","path":"/ABS/repo","entrypoints":["src/index.ts"],"includeTests":false}},
  {"tool":"ghSearch","query":{"operation":"repositories","keywords":["octokit"],"language":"TypeScript","pageSize":5}},
  {"tool":"ghSearch","query":{"operation":"code","owner":"octokit","repo":"octokit.js","keywords":["Octokit"],"pageSize":5}},
  {"tool":"ghSearch","query":{"operation":"tree","owner":"octokit","repo":"octokit.js","path":"src","pageSize":5}},
  {"tool":"ghGetFileContent","query":{"owner":"octokit","repo":"octokit.js","path":"src/octokit.ts","branch":"main","matchString":"Octokit","minify":"none"}},
  {"tool":"ghSearchHistory","query":{"operation":"commits","owner":"octokit","repo":"octokit.js","path":"src/octokit.ts","pageSize":5}},
  {"tool":"ghGetHistoryItem","query":{"operation":"pullRequest","owner":"octokit","repo":"octokit.js","number":2961,"content":{"changedFiles":true}}},
  {"tool":"ghGetHistoryItem","query":{"operation":"issue","owner":"octokit","repo":"octokit.js","number":2968,"content":{"body":true},"charLength":200}},
  {"tool":"ghGetHistoryItem","query":{"operation":"commit","owner":"octokit","repo":"octokit.js","ref":"main","includeDiff":true}},
  {"tool":"ghGetHistoryItem","query":{"operation":"compare","owner":"octokit","repo":"octokit.js","base":"v4.0.0","head":"v5.0.0","pageSize":5}},
  {"tool":"ghCloneRepo","query":{"owner":"octokit","repo":"octokit.js","branch":"main","sparsePath":"src"}},
  {"tool":"npmSearch","query":{"packageName":"@octokit/rest"}},
  {"tool":"npmSearch","query":{"keywords":["octokit"],"pageSize":2}}
]
```

Local line numbers and symbols above are placeholders, not claimed evidence. Search or read them before an anchored LSP call. GitHub examples use public identities but their live content can change; record the returned ref and fetch date. Clone needs persistent local storage and git. Do not use the sparse clone as proof about omitted files.

For continuation, copy the returned `tool` and `query`; do not construct page, match, diagnostic, body, or diff offsets from these examples. Examples demonstrate shape; runtime evidence and completeness still control conclusions.

Next: apply `references/workflow-local.md` or `references/workflow-external.md` to interpret results; use `references/octocode.md` for transport and errors.
