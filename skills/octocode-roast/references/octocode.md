# Octocode Research Delegation

Load when a roast needs local code, GitHub, package, history, or artifact research through Octocode.

This skill does not define Octocode research rules. Use `octocode-research` for the router, tool choice, evidence grades, citation discipline, and MCP/CLI fallback behavior.

## How To Route

1. If `octocode-research` is installed, load it and request code evidence for the roast target.
2. If it is not installed, ask for consent before installation or continue with normal repo tools and mark reduced coverage.
3. After consent, install with the Octocode CLI:

```bash
npx octocode skill --name octocode-research
```

Add `--platform <target>` when the user approved installation for a specific host, such as `codex`, `claude`, `cursor`, or `pi`.

Return the evidence here for severity ranking, tone calibration, and the fix checkpoint.
