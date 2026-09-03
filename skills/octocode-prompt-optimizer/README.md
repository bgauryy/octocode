# Octocode prompt optimizer

Improve prompts, policies, agent instructions, and handoffs. Preserve intent when you change tool contracts or schema contracts.

## Use when

- An instruction surface is unclear, unsafe, too expensive in context, or difficult to trigger.
- A handoff omits authority, evidence, acceptance, or return shape.
- A tool schema or pagination contract permits ambiguous or incomplete behavior.
- Reliability needs behavioral evaluation rather than wording judgment alone.

## Capabilities

- Finds evidenced instruction and contract failures.
- Makes the smallest repair that preserves the original job.
- Handles untrusted content, tool boundaries, caching, pagination, and structured outputs.
- Produces a validated rewrite or a focused patch-style delta.

## Workflow

```text
READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT
```

Small edits can combine adjacent phases. Significant behavior changes require a frozen sensor and measurable comparison.

## Install

```bash
npx octocode skill install octocode-prompt-optimizer --platform codex
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
