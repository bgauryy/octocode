# Octocode Subagents

Use this skill in Pi sessions when a task may benefit from background worker agents. It helps agents decide when to delegate, how to write self-contained worker prompts, how to use `spawnAgent` and `AgentMessage`, and how to synthesize worker output safely.

## Features

- Spawn decision guidance for parent-vs-worker tradeoffs.
- `spawnAgent` and `AgentMessage` parameter reference.
- Worker prompt checklist for self-contained tasks.
- Coordination patterns for single workers, parallel workers, steering, waiting, and cleanup.
- Verification rules for treating worker output as claims until checked.

## How It Works

`SKILL.md` stays short and routes agents to focused references. `references/spawn-decisions.md` handles delegation choice, `references/api.md` handles tool parameters, `references/coordination.md` handles prompts and orchestration, and `references/synthesis.md` handles verification and cleanup.

## Audience

Users use this skill to understand when Pi subagents help and what safety checks agents should apply.
Agents use it when they are about to spawn, message, wait for, or synthesize worker agents.
Developers and maintainers use it as the canonical root skill source copied into `packages/octocode-pi-extension/skills/` by the Pi extension build.

## Installation

```bash
npx octocode skill --name octocode-subagents --platform pi
```

In this repo, regenerate the Pi package copy with:

```bash
yarn workspace @octocodeai/pi-extension build:skills
```
