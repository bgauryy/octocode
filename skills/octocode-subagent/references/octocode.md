# Octocode Research Delegation

Load when subagent work needs local code, GitHub, package, history, or artifact research through Octocode. Optional — skip on hosts without Octocode.

This skill does not redefine research rules. Use `octocode-research` for routing, evidence grades, and citations.

## How To Route

1. If `octocode-research` is installed, load it inside the researcher/architect worker (or parent) for the probe.
2. If missing, point to https://github.com/bgauryy/octocode/tree/main/skills/octocode-research
3. Install:

```bash
npx octocode skill --name octocode-research
```

Return evidence into the subagent result packet, then synthesize in the parent.
