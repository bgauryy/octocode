# Octocode research delegation

Load when a roast needs local code, GitHub, package, history, or artifact research through Octocode.

This skill does not define Octocode research rules. Use `octocode-research` for the router, tool choice, evidence grades, citation discipline, and MCP/CLI fallback behavior.

## How To Route

1. When `octocode-research` is available, load it, and request code evidence for the roast target.
2. Otherwise, ask for consent before installation or continue with normal repository tools and mark reduced coverage.
3. After consent, install with the Octocode CLI:

```bash
npx octocode skill install octocode-research
```

Add `--platform <target>` after you approve installation for a specific host, such as `codex`, `claude`, `cursor`, or `pi`.

Return the evidence here for severity ranking, tone calibration, and the fix checkpoint.
