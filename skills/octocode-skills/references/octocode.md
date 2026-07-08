# Octocode Research Delegation

Load when skill discovery or comparison needs Octocode-backed GitHub, package, or code research.

This skill does not define Octocode research rules. Use `octocode-research` for the router, tool choice, evidence grades, citation discipline, and MCP/CLI fallback behavior.

## How To Route

1. If `octocode-research` is installed, load it and request skill-candidate discovery or exact `SKILL.md` evidence.
2. If it is not installed, point the user or agent to https://github.com/bgauryy/octocode/tree/main/skills/octocode-research.
3. To install it with the Octocode CLI, run:

```bash
npx octocode skill --name octocode-research
```

Add `--platform <target>` when installing for a specific host, such as `codex`, `claude`, `cursor`, or `pi`.

Return the found skill folders here for linting, quality scoring, adaptation, install gating, and user-facing recommendations.
