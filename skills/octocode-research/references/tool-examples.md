# Tool examples

Load when a query shape is non-obvious. `scheme <tool>` lists every variant with a runnable example; these cover the shapes agents most often get wrong. Replace paths, lines, refs, and numbers with observed values.

```json
[
  {"tool":"localSearch","query":{"goal":"<what>","reasoning":"<why>","path":"/ABS/repo/src","searchText":"withDataCache|createDataCache","resultView":"files"}},
  {"tool":"structureSearch","query":{"goal":"<what>","reasoning":"<why>","operation":"files","path":"/ABS/repo","names":["*.config.*"]}},
  {"tool":"localFetch","query":{"goal":"<what>","reasoning":"<why>","path":"/ABS/repo/src/example.ts","matchString":["export function","throw new"],"block":true}},
  {"tool":"localFetch","query":{"goal":"<what>","reasoning":"<why>","path":"/ABS/repo/src/example.ts","ranges":["10-30","120-140"]}},
  {"tool":"astSearch","query":{"goal":"<what>","reasoning":"<why>","operation":"symbols","path":"/ABS/repo/src","name":["createServer","listen"]}},
  {"tool":"lspSearch","query":{"goal":"<what>","reasoning":"<why>","uri":"/ABS/repo/src/example.ts","operation":"references","symbolName":"example","lineHint":10,"includeDeclaration":false,"groupByFile":true}},
  {"tool":"astTopology","query":{"goal":"<what>","reasoning":"<why>","analysis":"reachability","path":"/ABS/repo","entrypoints":["src/index.ts"]}},
  {"tool":"ghSearchCode","query":{"goal":"<what>","reasoning":"<why>","owner":"tokio-rs","repo":"tokio","keywords":["max_blocking_threads"],"path":"tokio/src/runtime"}},
  {"tool":"ghGetFileContent","query":{"goal":"<what>","reasoning":"<why>","owner":"tokio-rs","repo":"tokio","path":"tokio/src/runtime/builder.rs","matchString":"max_blocking_threads: 512","block":true}},
  {"tool":"ghGetHistoryItem","query":{"goal":"<what>","reasoning":"<why>","operation":"issue","owner":"cli","repo":"cli","number":14404}},
  {"tool":"ghGetHistoryItem","query":{"goal":"<what>","reasoning":"<why>","operation":"pullRequest","owner":"cli","repo":"cli","number":13541,"matchString":"MatchCategory","matchContext":0,"files":["pkg/"]}},
  {"tool":"ghGetHistoryItem","query":{"goal":"<what>","reasoning":"<why>","operation":"compare","owner":"octokit","repo":"octokit.js","base":"v4.0.0","head":"v5.0.0"}},
  {"tool":"artifactSearch","query":{"goal":"<what>","reasoning":"<why>","type":"npm","packageName":"express","version":"4.21.2"}},
  {"tool":"ghCloneRepo","query":{"goal":"<what>","reasoning":"<why>","owner":"octokit","repo":"octokit.js","branch":"main","sparsePath":["src","test"]}},
  {"tool":"clasify","query":{"goal":"<what>","reasoning":"<next read depends on>","resources":[{"tool":"ghGetFileContent","query":{"owner":"psf","repo":"requests","path":"src/requests/sessions.py","branch":"v2.32.3"},"prefilter":["Authorization"]}],"questions":[{"id":"auth","type":"locate","ask":"The condition for removing authorization on a cross-host redirect."}]}}
]
```

`astTopology` paths are relative to its `path`. `astRewrite` previews by default; apply only with `apply:true` and the full `expectedHashes` (or the returned `next.apply`). A sparse clone proves nothing about omitted files.

Next: run the call and return to the route that sent you; unfamiliar fields → `scheme <tool>`.
