# Octocode research delegation

Load when brainstorming needs local code, GitHub, package, history, or artifact research through Octocode.

This skill does not define Octocode research rules. Use `octocode-research` for the router, tool choice, evidence grades, citation discipline, and MCP/CLI fallback behavior.

## How To Route

1. When `octocode-research` is available, load it, and run the needed Map / Validate / Investigate flow.
2. Otherwise, point readers to https://github.com/bgauryy/octocode/tree/main/skills/octocode-research.
3. To install it with the Octocode CLI, run:

```bash
npx octocode skill install octocode-research
```

Add `--platform <target>` when installing for a specific host, such as `codex`, `claude`, `cursor`, or `pi`.

Return the resulting evidence to the brainstorming claim ledger, then apply this skill's framing, stress-test, and verdict rules.
