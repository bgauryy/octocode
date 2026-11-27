Load to pick the tool for where the evidence is. The next parts of this guide, in order: [shape and scout](tool-flow-shape.md), [carry across sources](tool-flow-carry.md), [analysis, history and recovery](tool-flow-recover.md).

Live schemas own fields; [evidence](evidence.md) owns proof.

## Route: where the evidence is

The routes the server instructions serve, generated from core (`node skills/common/tool-flow.mjs`); a route is served only while every tool it names is listed. CLI: `octocode schema --view full`, field `instructions`.

| Route (server instructions §1) | Served when listed |
|---|---|
| this checkout first: layout, names, sizes → structureSearch; text, spellings, counts → localSearch; then localFetch | `structureSearch`, `localSearch`, `localFetch` |
| code shape: declarations, calls, nesting → astSearch → localFetch ranges | `astSearch`, `localFetch` |
| definition, every caller or use → lspSearch at a search hit's line | `lspSearch` |
| known file → localFetch | `localFetch` |
| repo or library not in this checkout: owner/repo unknown → ghSearchRepo; tree or refs → ghStructure; text → ghSearchCode; read → ghGetFileContent | `ghSearchRepo`, `ghStructure`, `ghSearchCode`, `ghGetFileContent` |
| why, when, who → ghSearchHistory → ghGetHistoryItem | `ghSearchHistory`, `ghGetHistoryItem` |
| first release with a change → commit or PR, ghStructure refs, ghGetHistoryItem compare | `ghSearchHistory`, `ghStructure`, `ghGetHistoryItem` |
| GitHub package version → artifactSearch → ghStructure → ghGetFileContent at its release ref; other hosts: use the source URL and ref | `artifactSearch`, `ghStructure`, `ghGetFileContent` |
| before using a package, commit or repo: vulnerabilities → securityCheck | `securityCheck` |
| importers, cycles, impact, dead code → graphBuild → graphQuery | `graphBuild`, `graphQuery` |
| many searches over a few paths of another repo → ghStructure tree materialize:true → local tools | `ghStructure`, `localSearch` |
| a whole repo, or paths you cannot name → ghCloneRepo → local tools | `ghCloneRepo` |
| keep only hits meaning X (cause, kind, intent) → clasify source:{tool, query} predicates | `clasify` |
| offload reading: too many hits, unread files, threads, logs or PNG/JPEG images → clasify → next.read | `clasify` |
| web page or URL → clasify source.url → next.read | `clasify` |
| an uncited quick check (a known spelling, line range or count), commands, git state → shell (rg, sed -n, git); a write or one small GitHub item → gh | always |

Prefer the local checkout for code and GitHub for discussion. Trace `entry → transformations → state → output → consumers`; change tools when the next claim needs different evidence. Batch independent queries, wait for dependent results, and reuse observed paths, lines and refs.
