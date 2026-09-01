# Prompt Optimizer

Improve prompts, agent instructions, handoffs, and tool/schema contracts while preserving intent and measuring behavior where reliability matters.

## Features

- A proportional `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT` workflow; small edits may combine adjacent phases.
- Focused references for tool contracts, agent handoffs, Zod boundaries, pagination, prompt caching, evaluation data, and untrusted content.
- Lean output: a validated rewrite or patch-style delta, with only the context the next agent needs.

## How it works

The skill reads the whole input, records evidenced issues, makes the smallest useful repair, and validates intent and behavior before delivery. Conditional guidance lives in focused references for context, tools, handoffs, schemas, caching, and untrusted content.

## Installation

```bash
npx octocode skill --name octocode-prompt-optimizer
```
