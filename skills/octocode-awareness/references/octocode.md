# Octocode Operations

Use this when Awareness needs code, GitHub, package, history, artifact, graph, or skill evidence. Awareness owns coordination/memory; `npx octocode` or Octocode MCP owns research and skill management.

No Octocode binary is bundled in this skill. Prefer connected Octocode MCP tools; otherwise run the published CLI so the correct native engine resolves for the host:

```bash
npx octocode <command> ... --no-color
```

## Research Recipes

```bash
# Structure and local evidence
npx octocode tools localViewStructure --queries '{"path":"<dir>","maxDepth":2}' --no-color
npx octocode tools localSearchCode --queries '{"path":"<path>","searchText":"<term>"}' --no-color
npx octocode tools localGetFileContent --queries '{"path":"<file>","minify":"symbols"}' --no-color
npx octocode tools lspGetSemantics --queries '{"uri":"<file>","type":"references","symbolName":"<Name>","lineHint":<N>}' --no-color
npx octocode tools localFindFiles --queries '{"path":"<dir>","names":["<glob>"]}' --no-color

# Repositories, packages, PRs, commits
npx octocode tools ghSearchRepos --queries '{"keywords":["<keywords>"]}' --no-color
npx octocode tools npmSearch --queries '{"packageName":"<pkg>"}' --no-color
npx octocode tools ghSearchPullRequests --queries '{"owner":"<owner>","repo":"<repo>","prNumber":<N>}' --no-color
npx octocode tools ghSearchCommits --queries '{"owner":"<owner>","repo":"<repo>","path":"<path>"}' --no-color

# Contract before any raw tool call
npx octocode tools <name> --scheme --compact --no-color
```

Treat hits as leads. Cite paths/lines/IDs in locks, signals, memories, and refinements. Zero matches require one scope/mode/spelling adjustment before an absence claim. Load `octocode-research` for deeper evidence workflows when available.

## Skill Management

For copy-pasteable install and refresh commands, load `references/agent-cheatsheet-tooling.md`; it owns package-path and host-platform setup. Gate skill installation as a write.

Return research evidence to Awareness only when it informs a claim, decision, memory, signal, refinement, or verified reflection.
