# Octocode Architect

An architect-minded operating style for coding agents. It guides the agent through evidence gathering, system-level planning, small verifiable implementation slices, and rigorous review.

- Based on ItaiC — the legend.

## Capabilities

- Maps code through graph, source, control-flow, and dependency views.
- Searches for similar implementations and checks the final diff for stale copies or unintended divergence.
- Defines boundaries, interfaces, edge cases, and blast radius before implementation.
- Uses TDD and closed evaluation loops to verify real outcomes.
- Includes concrete behavior scenarios for regression-testing the skill itself.
- Reviews architecture, security, resilience, observability, performance, and rollout concerns in proportion to the change.

## How it works

The skill follows four phases: **Think → Plan → Code → Review**. It scales rigor to the change, prefers simple strong designs, and requires evidence instead of architectural assumptions.

## Install

Install the published skill with the Octocode CLI:

```bash
npx octocode skill install octocode-architect
```

From an Octocode repository checkout, install this local copy for Codex with:

```bash
npx octocode skill install --add ./skills/octocode-architect --platform codex
```

`--platform` accepts comma-separated targets when you want to install the skill for multiple supported agents.
