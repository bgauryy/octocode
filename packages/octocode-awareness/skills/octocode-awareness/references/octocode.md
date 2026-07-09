# Octocode Operations

Use this when Awareness needs code, GitHub, package, history, artifact, graph, or skill evidence. Awareness owns coordination/memory; `npx octocode` or Octocode MCP owns research and skill management.

No Octocode binary is bundled in this skill. Prefer connected Octocode MCP tools; otherwise run the published CLI so the correct native engine resolves for the host:

```bash
npx octocode <command> ... --no-color
```

## Research Recipes

```bash
# Structure and local evidence
npx octocode search <dir> --tree --max-depth 2 --no-color
npx octocode search "<term>" <path> --no-color
npx octocode search <file> --content-view symbols --no-color
npx octocode search <file> --op references --symbol <Name> --line <N> --no-color
npx octocode search <dir> --search path --name "<glob>" --no-color

# Repositories, packages, PRs, commits
npx octocode search <keywords> --target repositories --no-color
npx octocode search <pkg> --target packages --no-color
npx octocode search owner/repo#N --target pullRequests --no-color
npx octocode search owner/repo/path --target commits --no-color

# Contract before raw OQL
npx octocode search --scheme --compact --no-color
```

Treat hits as leads. Cite paths/lines/IDs in locks, signals, memories, and refinements. Zero matches require one scope/mode/spelling adjustment before an absence claim. Load `octocode-research` for deeper evidence workflows when available.

## Skill Management

For copy-pasteable install and refresh commands, load `references/agent-cheatsheet-tooling.md`; it owns package-path and host-platform setup. Gate skill installation as a write.

Return research evidence to Awareness only when it informs a claim, decision, memory, signal, refinement, or verified reflection.
