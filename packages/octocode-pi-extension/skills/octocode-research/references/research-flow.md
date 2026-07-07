# Research Flow

Read this when executing an Octocode research workflow: `SKILL.md` picks the mode; this file gives compact per-mode tool chains. The router, evidence grades, anti-patterns, and failure signals behind every step live in `references/algorithm.md` — read that first. Tool names and their CLI form live in `references/octocode.md`.

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
ghGetFileContent(ARCHITECTURE.md, content:exact)  -> extract named entry points, then verify each claim via the router
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
oqlSearch --scheme --compact  (or `search --scheme --compact`)
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

- Snippets are leads, not proof; exact content, AST, LSP, PR/commit evidence, binary metadata, or tests can prove (grades: `references/algorithm.md`).
- LSP needs a real `lineHint`; get it from search/symbols/AST first.
- Empty LSP references/callers are inconclusive until likely consumers are loaded.
- `target:"research"` and `target:"graph"` rows are candidates until upgraded.
- Follow `next.*`, pagination, char offsets, match/file/comment/commit pages.
- Cite local evidence as `path:line`; cite remote evidence as full URL or PR/commit id.

## Before Answering

Confirm:

1. The corpus is explicit: local path, package, owner/repo, branch/ref, PR number, artifact path, or materialized `localPath`.
2. The surface is justified: MCP, `search`, OQL, raw tool, local shell, web, or skipped surface with reason.
3. Tool calls used the correct schema: read the MCP tool description or `npx octocode tools <name> --scheme` (CLI) before calling; OQL JSON was built after `search --scheme`.
4. Candidate results were converted into exact evidence when the claim depends on them.
5. Pagination and continuations were followed or declared unnecessary.
6. Diagnostics and provider limitations were handled.
7. Claims distinguish syntax proof, semantic proof, history proof, binary proof, and runtime/test proof.
8. Fallbacks are named when used.

For repeated Act→Observe→Learn cycles, convergence goals, local code-check loops, or "keep going until evidence converges", read `references/loop-mode.md`.
