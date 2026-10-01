# Tool examples

Load when a query shape is non-obvious. `scheme <tool>` lists every variant with a runnable example; these cover the shapes agents most often get wrong. Replace paths, lines, refs, and numbers with observed values.

```json
[
  {"tool":"localSearch","query":{"goal":"<what>","reasoning":"<why>","path":"/ABS/repo/src","searchText":"withDataCache","resultView":"files"}},
  {"tool":"structureSearch","query":{"goal":"<what>","reasoning":"<why>","operation":"files","path":"/ABS/repo","names":["*.config.*"]}},
  {"tool":"localFetch","query":{"goal":"<what>","reasoning":"<why>","path":"/ABS/repo/src/example.ts","matchString":"example","block":true}},
  {"tool":"lspSearch","query":{"goal":"<what>","reasoning":"<why>","uri":"/ABS/repo/src/example.ts","operation":"references","symbolName":"example","lineHint":10,"includeDeclaration":false}},
  {"tool":"astTopology","query":{"goal":"<what>","reasoning":"<why>","analysis":"reachability","path":"/ABS/repo","entrypoints":["src/index.ts"]}},
  {"tool":"ghGetHistoryItem","query":{"goal":"<what>","reasoning":"<why>","operation":"pullRequest","owner":"octokit","repo":"octokit.js","number":2961,"matchString":"Octokit","files":["src/"]}},
  {"tool":"ghGetHistoryItem","query":{"goal":"<what>","reasoning":"<why>","operation":"compare","owner":"octokit","repo":"octokit.js","base":"v4.0.0","head":"v5.0.0"}},
  {"tool":"ghCloneRepo","query":{"goal":"<what>","reasoning":"<why>","owner":"octokit","repo":"octokit.js","branch":"main","sparsePath":"src"}},
  {"tool":"clasify","query":{"goal":"<what>","reasoning":"<next read depends on>","resources":[{"tool":"ghGetFileContent","query":{"goal":"Find redirect conditions.","reasoning":"Locate before reading.","owner":"psf","repo":"requests","path":"src/requests/sessions.py","branch":"v2.32.3","fullContent":true},"prefilter":["Authorization"]}],"questions":[{"id":"auth","type":"locate","ask":"The condition for removing authorization on a cross-host redirect."}]}}
]
```

`astTopology` paths are relative to its `path`. `astRewrite` previews by default; apply only with `apply:true` and the full `expectedHashes`. A sparse clone proves nothing about omitted files.

Next: run the call and return to the route that sent you; unfamiliar fields → `scheme <tool>`.
