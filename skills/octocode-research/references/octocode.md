Load when constructing a research query. For invocation, availability, configuration, paging, and recovery, use the [shared Octocode guide](octocode-usage.md).

## Templates

- Replace paths, lines, refs, and numbers with observed values; wrap per [shared invocation](octocode-usage.md#discover-and-invoke).
- File graphs are a CLI command, not a tool row: `npx octocode graph ingest /ABS/repo`, then `npx octocode graph query issues --detector unreachable-file --entry src/index.ts` (refs are relative to the ingested root).

```json
[
  {"tool":"localSearch","query":{"path":"/ABS/repo/src","matchString":"withDataCache|createDataCache","regex":"rust","resultView":"files"}},
  {"tool":"structureSearch","query":{"operation":"files","path":"/ABS/repo","include":["*.config.*"]}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/src/example.ts","matchString":["export function","throw new"],"block":true}},
  {"tool":"localFetch","query":{"path":"/ABS/repo/src/example.ts","ranges":["10-30","120-140"]}},
  {"tool":"astSearch","query":{"operation":"symbols","path":"/ABS/repo/src","symbolName":["createServer","listen"]}},
  {"tool":"lspSearch","query":{"path":"/ABS/repo/src/example.ts","operation":"references","symbolName":"example","line":10,"includeDeclaration":false,"groupByFile":true}},
  {"tool":"ghSearchCode","query":{"owner":"tokio-rs","repo":"tokio","keywords":["max_blocking_threads"],"path":"tokio/src/runtime"}},
  {"tool":"ghGetFileContent","query":{"owner":"tokio-rs","repo":"tokio","path":"tokio/src/runtime/builder.rs","matchString":"max_blocking_threads: 512","block":true}},
  {"tool":"ghGetHistoryItem","query":{"operation":"issue","owner":"cli","repo":"cli","number":14404}},
  {"tool":"ghGetHistoryItem","query":{"operation":"pullRequest","owner":"cli","repo":"cli","number":13541,"matchString":"MatchCategory","contextLines":0,"include":["pkg/"]}},
  {"tool":"ghGetHistoryItem","query":{"operation":"compare","owner":"octokit","repo":"octokit.js","base":"v4.0.0","head":"v5.0.0"}},
  {"tool":"artifactSearch","query":{"ecosystem":"npm","packageName":"express","version":"4.21.2"}},
  {"tool":"ghCloneRepo","query":{"owner":"octokit","repo":"octokit.js","ref":"main","path":["src","test"]}},
  {"tool":"clasify","query":{"source":{"tool":"ghGetFileContent","query":{"owner":"psf","repo":"requests","path":"src/requests/sessions.py","ref":"v2.32.3","fullContent":true}},"questions":{"auth":{"type":"noul","instructions":"Does the file remove the Authorization header when a redirect changes host?"}}}}
]
```
