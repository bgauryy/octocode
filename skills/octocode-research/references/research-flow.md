# Research Flow
Read this when executing an Octocode research workflow. The front door picks the mode; this file gives compact command chains and proof rules. Start each workflow with a **surface plan**: local, GitHub, packages, PR/history, artifacts, web, and any skipped surfaces with reasons. Update it when cross-pollination changes the route.
For long, contested, or public-facing decision briefs, read `references/long-research.md` before deep work. For repo ecosystem comparisons or "which implementation should we reuse?" questions, read `references/github-landscape.md`.

## Mode Flows
### Map: landscape / prior art

```text
frame terms: literal + 2 synonyms
-> search <keywords> --target repositories --concise --json
-> search <package-or-topic> --target packages --json
-> search <owner/repo> --tree --depth 1 --json
-> search <term> <owner/repo> --view discovery --json
-> search <owner/repo/path> --match-string <anchor> --content-view exact --json
-> cluster: active, abandoned, solved, partial, white-space
```
Package evidence = last publish, maintainers, cadence, issue/PR ratio, dependency freshness; downloads alone are not validation.

### Validate: should this exist / should we add it?

```text
light diverge: reframe, invert, analogize, decompose
-> choose 1-3 framings
-> local-first if it touches this repo
-> GitHub/package research
-> optional web/product research
-> cross-pollinate every lead
-> advocate vs critic
-> verdict: build, do not build, narrow, or prototype hardest unknown
```
Hypothesis map: `Crowded if...` / `Underserved if...` / `Blocked if...` / `Worth prototyping if...`.

### Investigate: behavior / bug / root cause

```text
search <path> --tree --depth 1 --json
-> search <symbol-or-error> <path> --view discovery --json
-> search <file> --match-string <anchor> --content-view exact --json
-> search <file> --op definition|references|callers|callees --symbol <name> --line <lineHint> --json
-> search <path> --pattern '<shape>' --lang <lang> --json when shape matters
-> search <owner/repo/path> --target commits or pullRequests when intent matters
```
Keep two plausible explanations alive until a command disconfirms one.

### Plan: implementation / refactor

```text
current behavior + invariants
-> file/flow orientation
-> blast radius with LSP references/callers
-> AST/import checks for boundaries and cycles
-> existing pattern to copy
-> options and safest next step
```
Gate before public contract changes, cross-package edits, deletes/renames, or broad consumer impact.

## Surface Recipes

Local:
```text
search <path> --tree --depth 1
search <query> <path> --search path
search <term> <path> --view discovery
search <file> --content-view symbols
search <file> --match-string <anchor> --content-view exact
search <file> --op references|callers|callees --symbol <name> --line <lineHint>
```

Remote/package:
```text
search <package> --target packages
search <keywords> --target repositories --lang <language> --stars ">100" --concise
search <owner/repo> --tree
search <symbol> <owner/repo> --view discovery
search <owner/repo/path> --match-string <anchor> --content-view exact
```

Remote as local:
```text
cache fetch <owner/repo> <path> --depth tree --json
clone <owner/repo[/path][@ref]>
search <repo-relative-path> --repo <owner/repo[@ref]> --pattern '<shape>' --lang <lang>
```

PR/change intent:
```text
search <owner/repo#N> --target pullRequests --json
search <owner/repo#N> --target pullRequests --comments --json
search <owner/repo#N> --target pullRequests --patches --file <path> --json
search <owner/repo[/path]> --target commits --since <iso> --json
```

Dead code / reachability / drift:
```text
search --scheme --compact
-> search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
-> follow returned next.graph with search --query '<returned graph JSON>' --json
-> if no next.graph is returned, run search --scheme before writing graph JSON
-> confirm with exact reads, AST/import search, LSP, and tests before deletion
```

Artifacts:
```text
search <artifact> --target artifacts --inspect|--list|--strings
search <artifact> --target artifacts --extract <entry>
unzip <archive>
search <localPath> --tree
search <term> <localPath>
search <file> --content-view exact
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

- Snippets are leads, not proof.
- Exact content, AST, LSP, PR/commit evidence, binary metadata, or tests can prove.
- LSP needs a real `lineHint`; get it from search/symbols/AST first.
- Empty LSP references/callers are inconclusive until likely consumers are loaded.
- `target:"research"` and `target:"graph"` rows are candidates until upgraded.
- Follow `next.*`, pagination, char offsets, match/file/comment/commit pages.
- Cite local evidence as `path:line`; cite remote evidence as full URL or PR/commit id.

## Before Answering

Confirm:

1. The corpus is explicit: local path, package, owner/repo, branch/ref, PR number, artifact path, or materialized `localPath`.
2. The surface is justified: MCP, `search`, OQL, raw tool, local shell, web, or skipped surface with reason.
3. Raw-tool fields came from the active `--scheme`; OQL JSON came after `search --scheme`.
4. Candidate results were converted into exact evidence when the claim depends on them.
5. Pagination and continuations were followed or declared unnecessary.
6. Diagnostics and provider limitations were handled.
7. Claims distinguish syntax proof, semantic proof, history proof, binary proof, and runtime/test proof.
8. Fallbacks are named when used.
