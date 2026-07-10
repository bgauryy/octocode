# Octocode Awareness Skill

Agent operating skill for always-on workspace work: awareness, collaboration,
learning, memory/wiki, bookkeeping, housekeeping, locks, verification, hooks,
reflection, and repo context. Users get a compact lobby; developers get
deterministic scripts, schemas, host hooks, and a canonical-source build workflow.

The operational model is a Homeostatic Awareness Loop: sense shared SQLite/hook
state, compare it with bounded targets, recommend the smallest guarded correction,
then re-measure. “Living system” is a metaphor for adaptive repository maintenance,
not sentience or autonomous authority. Human-facing details live in
[THESIS.md](../../docs/THESIS.md); the skill lobby routes agents to focused references.

## Install

Install the package, then install this bundled skill by its resolved local path:

```bash
npm install --global @octocodeai/octocode-awareness
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

The package also bundles sibling `octocode-skills` for skill install/review/
improvement. Install both skills from their packaged paths; do not depend on
registry/name lookup.

## Features

`SKILL.md` is the lobby:

```text
READ/ATTEND -> REASON/CHOOSE -> DO/COORDINATE -> VERIFY -> LEARN? -> CLEAN? -> PROJECT?
```

- Plans/tasks define collaborative work and one canonical queue.
- `work start|touch|end|list|show` records mandatory advisory file presence.
- Ordinary peers can share a file and see task/reason context.
- `--exclusive`/locks protect sensitive work and conflict with other live presence.
- Hooks guard and declare before writes, aggregate fallback edits by bounded
  agent/session/workspace/artifact scope, finalize once, and capture handoffs.
- Learning records only reusable verified outcomes; cleanup is pressure-driven,
  selector-bound, and previewed before any destructive action.
- SQLite is canonical; `.octocode/` files are bounded projections.
- Stable state is silent; changed state returns only the next decision packet.

## How It Works

The lobby routes conditional depth to one-concept references. When the next action
needs discovery, use one focused inventory:

```bash
node scripts/awareness.mjs schema commands --compact
node scripts/awareness.mjs docs list --compact
```

Prefer the published CLI when available:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
# monorepo local build:
node packages/octocode-awareness/dist/bin/awareness.js schema commands --compact
```

## Scripts

| Script | Purpose |
|---|---|
| `scripts/awareness.mjs` | Standalone bundled CLI/runtime. |
| `scripts/schema.mjs` | Public schemas and command map. |
| `scripts/hook-runner.mjs` | Shared host lifecycle implementation. |
| `scripts/extract-hook-files.mjs` | Host payload path extraction. |
| `scripts/install.mjs` | Runtime check and hook setup guidance. |
| `scripts/smoke-multi-agent.mjs` | Native multi-agent end-to-end smoke. |
| `scripts/hooks/*.sh` | Thin lifecycle wrappers. |

Compiled operational scripts are generated artifacts. Do not hand-edit the `.mjs`
copies; package maintainers regenerate them from canonical TypeScript sources.

## Hosts

- Claude may run frontmatter hooks while this skill is active.
- Codex/Cursor need `awareness hooks install` and `hooks check --strict`.
- Pi uses `wirePiAwarenessHooks(pi)`; never install shell hooks for Pi.
- Normal hooks are silent; changed peers/briefings and real conflicts are bounded.

## Verification

From the monorepo:

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:quiet
node skills/octocode-skills/scripts/skill-review.mjs \
  packages/octocode-awareness/skills/octocode-awareness
```

Build mirrors this skill to package `dist/skills/` and local `.agents/skills/`.
