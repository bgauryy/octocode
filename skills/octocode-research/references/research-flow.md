# Research Flow

Read when executing an Octocode research workflow. `SKILL.md` picks the mode; this file gives compact per-mode tool chains.
The router, evidence grades, anti-patterns, and failure signals live in `references/algorithm.md`. Tool names and CLI forms live in `references/octocode.md`.
For local/external routing, debug/root-cause flow, change flow, or PR/local review, read `references/workflows.md` (index) or the matching `workflow-*.md` file before choosing a tool chain here.

Start each workflow with a **surface plan** (local, GitHub, packages, PR/history, artifacts, web, plus skipped surfaces with reasons); update it when cross-pollination changes the route.

Rare paths, skip by default: long/contested/public decision briefs → `references/long-research.md`; repo ecosystem comparisons → `references/github-landscape.md`.

Each step below names the MCP tool; substitute the CLI form from `references/octocode.md`'s Tool Matrix when MCP isn't exposed.

## Mode Flows

### Map: landscape / prior art

```
frame terms: literal + 2 synonyms
-> ghSearchRepos(keywords, concise)          + npmSearch(package-or-topic)
-> ghViewRepoStructure(owner/repo, depth:1)  -> ghSearchCode(term, owner/repo)
-> ghGetFileContent(matchString: anchor)
-> cluster: active, abandoned, solved, partial, white-space
```

Package evidence = last publish, maintainers, cadence, issue/PR ratio, dependency freshness; downloads alone are not validation.

### Validate: should this exist / should we add it?

```
light diverge: reframe, invert, analogize, decompose -> choose 1-3 framings
-> local-first if it touches this repo -> GitHub/package research -> optional web/product research
-> cross-pollinate every lead -> advocate vs critic
-> verdict: build, do not build, narrow, or prototype hardest unknown
```

Hypothesis map: `Crowded if...` / `Underserved if...` / `Blocked if...` / `Worth prototyping if...`.

### Investigate: behavior / bug / root cause

```
localViewStructure(path, depth:1)
-> localSearchCode(symbol-or-error, path)
-> localGetFileContent(matchString: anchor)
-> lspGetSemantics(op: definition|references|callers|callees, symbol, lineHint)
-> localSearchCode(mode:"structural", pattern) when shape matters
-> ghHistoryResearch(type: commits|prs, owner/repo/path) when intent matters
```

Keep two plausible explanations alive until a call disconfirms one.

### Plan: implementation / refactor

```
current behavior + invariants -> file/flow orientation
-> blast radius: lspGetSemantics(references|callers)
-> AST/import checks for boundaries and cycles -> existing pattern to copy
-> options and safest next step
```

Gate before public contract changes, cross-package edits, deletes/renames, or broad consumer impact.

## Surface Recipes

Wiki/docs orientation (when present — a lead, not proof; see `algorithm.md` router):

```
localViewStructure/ghViewRepoStructure(depth:1)   -> spot ARCHITECTURE.md, droid-wiki/, openwiki/, .devin/wiki.json
ghGetFileContent(ARCHITECTURE.md as exact content)  -> extract named entry points, then verify each claim via the router
```

A GitHub Wiki tab or DeepWiki/Code Wiki page (if linked from the README) is the same lead, read externally instead of via tree.

```
Local:          localViewStructure -> localFindFiles -> localSearchCode -> localGetFileContent(symbols) -> localGetFileContent(matchString) -> lspGetSemantics
Remote/package: npmSearch -> ghSearchRepos -> ghViewRepoStructure -> ghSearchCode -> ghGetFileContent(matchString)
Remote as local: ghGetFileContent(type:"directory") or ghCloneRepo -> localSearchCode/lspGetSemantics on the materialized path
```

PR/change intent:

```
ghHistoryResearch(type:"prs", owner/repo#N)
ghHistoryResearch(type:"prs", owner/repo#N, comments:true)
ghHistoryResearch(type:"prs", owner/repo#N, patches:true, file:<path>)
ghHistoryResearch(type:"commits", owner/repo[/path], since:<iso>)
```

Dead code / reachability / drift:

```
search --scheme --compact  (or raw `oqlSearch` schema when using MCP directly)
-> oqlSearch(target:"research", from:{kind:"local",path:"."}, goal:"find unused exports, transitive dead code, unused files, and package drift")
-> follow returned next.graph -> oqlSearch(<returned graph query>)
-> if no next.graph is returned, read --scheme before writing graph JSON by hand
-> confirm with exact reads, AST/import search, LSP, and tests before deletion
```

Artifacts:

```
localBinaryInspect(target:"artifacts", inspect|list|strings)
localBinaryInspect(extract:<entry>) / unzip <archive>
localViewStructure/localSearchCode/localGetFileContent on the extracted path
```

## Cross-Pollination

- Local framework/library names -> GitHub/npm queries.
- Package README competitors -> repo/package searches for each competitor.
- Web/product names -> repository/package lookup and code search.
- GitHub issue complaint -> search commits/PRs for fixes.
- Empty result -> synonym retry, narrower path/ref, then materialize before calling absence.
- Large tool outputs -> compress to claim ledger entries before the next step.

## Advocate Vs Critic

1. Advocate: strongest case for, each claim with reason + citation.
2. Critic: strongest case against, each claim with reason + citation.
3. Rebut each side's strongest claim.
4. Keep survived claims, drop conceded claims, mark unresolved claims as decision points.

## Evidence Gates

Grades, triangulation, and failure signals: `references/algorithm.md`. Additional gates for these flows:

- LSP needs a real `lineHint`; get it from search/symbols/AST first.
- Empty LSP references/callers are inconclusive until likely consumers are loaded.
- `target:"research"` and `target:"graph"` rows are candidates until upgraded.
- Cite local evidence as `path:line`; cite remote evidence as full URL or PR/commit id.

## Before Answering

Confirm:

1. The corpus is explicit: local path, package, owner/repo, branch/ref, PR number, artifact path, or materialized `localPath`.
2. Every surface is justified — MCP, `search`, OQL, raw tool, local shell, web — and each skipped surface has a stated reason.
3. Schemas were read before raw or OQL calls: MCP tool description, `npx octocode tools <name> --scheme`, or `search --scheme --compact` before OQL JSON.
4. Candidates were upgraded to exact evidence wherever a claim depends on them; pagination and continuations were followed or declared unnecessary.
5. Claims distinguish syntax, semantic, history, binary, and runtime/test proof; diagnostics, provider limits, and fallbacks are named.

For repeated Act→Observe→Learn cycles, convergence goals, local code-check loops, or "keep going until evidence converges", read `references/loop-mode.md`.
