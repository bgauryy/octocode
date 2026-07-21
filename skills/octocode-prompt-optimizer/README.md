# Prompt Optimizer

Human-facing guide to a skill for improving prompts, `SKILL.md`, `AGENTS.md`, agent instructions, MCP guidance, and tool or schema contracts. Use it when an agent is unreliable, chooses tools poorly, wastes context, or needs a behavior change backed by evidence.

## Features

- A gated `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT` workflow, with a safe Fast Path for small, low-risk text.
- Focused references for tool contracts, agent handoffs, Zod boundaries, pagination, prompt caching, evaluation data, and untrusted content.
- Lean output: a validated rewrite or patch-style delta, with only the context the next agent needs.

## How it works

The agent loads only the reference for the current gate, preserves required intent and identifiers, validates every proposed change, and applies external or file changes only when it has authority. For tool-facing work, it chooses the specific contract reference by the boundary involved: tool metadata/results, agent handoff, or TypeScript/Zod packet.

## For users and developers

Users can ask to optimize a prompt or make an agent instruction more reliable. Developers and maintainers update the compact `SKILL.md` router and single-purpose files in `references/`; the reviewer verifies their structure and source sections.

## Installation

```bash
npx octocode skill --name octocode-prompt-optimizer
```
