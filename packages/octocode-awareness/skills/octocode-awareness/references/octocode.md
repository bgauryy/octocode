# Octocode Operations

Use this when Awareness needs code, GitHub, package, history, artifact, graph, or skill evidence. Awareness owns coordination/memory; `npx octocode` or Octocode MCP owns research and skill management. No Octocode binary is bundled in this skill. Prefer connected Octocode MCP tools; otherwise run the published CLI so the correct native engine resolves for the host:

Inspect the live catalog before constructing requests:

```bash
npx octocode tools --json
npx octocode tools localSearch localAnalyzeGraph localGetFileContent lspGetSemantics --scheme
```

## Research Recipes

```bash
# Exact JSON fields come from --scheme; local paths must be absolute.
npx octocode tools localSearch --queries '{"operation":"tree","path":"/absolute/workspace","maxDepth":2}'
npx octocode tools localSearch --queries '{"operation":"text","path":"/absolute/workspace","searchText":"term","resultView":"discovery"}'
npx octocode tools localGetFileContent --queries '{"path":"/absolute/workspace/README.md","minify":"symbols"}'

# Remote/package contracts
npx octocode tools ghSearch ghGetFileContent ghSearchPullRequests ghSearchIssues ghSearchCommits ghListReleases ghSearchDiscussions ghCloneRepo npmSearch --scheme
# Releases and Discussions appear only when their feature flags enable them.
```

Treat hits as leads. Cite paths/lines/IDs in locks, signals, memories, and refinements. Zero matches require one scope/mode/spelling adjustment before an absence claim. Install a dedicated research workflow skill separately for deeper evidence workflows.

## Skill Management

The host or package manager owns skill installation. Do not reconstruct local package paths in an agent prompt. Use `references/agent-cheatsheet.md` for Awareness initialization and live command discovery. Return research evidence to Awareness only when it informs a claim, decision, memory, signal, refinement, or verified reflection.
