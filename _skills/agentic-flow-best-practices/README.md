# Agentic Flow Best Practices

Design, review, and harden agentic workflows and AI-agent harnesses: MCP surfaces, multi-agent routing, memory/context, schema contracts, human gates, observability, and production safety.

Use when the work is about the agent system boundary — not ordinary application feature code.

## Capabilities

- Review MCP tools/resources/prompts and handoff protocols
- Design multi-agent routing with clear ownership and gates
- Bound memory/context/cache so pressure stays measurable
- Apply Zod/JSON-schema contracts for tool I/O
- Add human gates, evals, and observability for production safety

## How It Works

```text
SCOPE HARNESS → MAP SURFACES → CONTRACTS → GATES → EVAL → HARDEN
```

Start from the agent boundary (tools, state, permissions, verification). Prefer measurable targets and held-out checks over prompt-only intuition. Ordinary app code stays out of scope unless it crosses the harness boundary.

## Installation

```bash
npx octocode skill --name agentic-flow-best-practices
```

## Maintainer Notes

Keep this README user-facing. Deep patterns live in the skill body and any `references/`. After edits, run `node ../octocode-skills/scripts/skill-review.mjs .` from a checkout that includes `octocode-skills`, or review via the installed skills skill.
