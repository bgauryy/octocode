# Octocode Awareness Skill

Agent operating skill for always-on workspace work: awareness, collaboration,
learning, memory/wiki, bookkeeping, housekeeping, locks, verification, hooks,
reflection, and repo context. Users get a compact lobby; developers get
deterministic scripts, schemas, host hooks, and a canonical-source build workflow.

## Install

Install the package, then install this bundled skill by its resolved local path:

```bash
npm install --global @octocodeai/octocode-awareness
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

The package also bundles sibling `octocode-skills` for skill install/review/
improvement. Direct package consumers use these paths; Octocode's
`--name octocode-awareness` route is valid when it resolves its bundled copy.

## Features

`SKILL.md` is the lobby:

```text
ATTEND -> CHOOSE -> DECLARE -> ACT -> SUBMIT/END -> VERIFY -> BOOKKEEP -> HOUSEKEEP -> PROJECT?
```

- Plans/tasks define collaborative work and one canonical queue.
- `work start|touch|end|list|show` records mandatory advisory file presence.
- Ordinary peers can share a file and see task/reason context.
- `--exclusive`/locks protect sensitive work and conflict with other live presence.
- Hooks guard and declare before writes, log after writes, dedupe briefing/peer
  context, cap verification debt, and capture handoffs.
- Bookkeeping turns verified outcomes into durable lessons; housekeeping prunes
  stale locks/signals/memory after dry-run.
- SQLite is canonical; `.octocode/` files are bounded projections.

## How It Works

The lobby routes conditional depth to one-concept references. When the next action
needs discovery, use one focused inventory:

```bash
node scripts/awareness.mjs schema commands --compact
node scripts/awareness.mjs docs list --compact
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
