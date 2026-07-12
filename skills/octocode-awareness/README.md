# Octocode Awareness Skill

This Agent Skill and the `octocode-awareness` CLI ship together in the
`@octocodeai/octocode-awareness` npm package. The public CLI is
`npx @octocodeai/octocode-awareness`; in this monorepo, build and run the CLI from
`packages/octocode-awareness` while editing this skill at repo-root
`skills/octocode-awareness`.

The skill provides always-on workspace awareness, collaboration,
learning, memory/wiki, bookkeeping, housekeeping, locks, verification, hooks,
reflection, and repo context. Users get a compact lobby; developers get
deterministic scripts, schemas, host hooks, and a canonical-source build workflow.

The operational model is a Homeostatic Awareness Loop: sense shared SQLite/hook
state, compare it with bounded targets, recommend the smallest guarded correction,
then re-measure. “Living system” is a metaphor for adaptive repository maintenance,
not sentience or autonomous authority. Human-facing details live in
[THESIS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/THESIS.md); the skill lobby routes agents to focused references.

## Install

Install the package, then install this bundled skill by its resolved local path:

```bash
npm install --global @octocodeai/octocode-awareness
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" \
  --platform common --dry-run
# after reviewing destinations and approving the write:
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" \
  --platform common --force
```

`common` installs to `~/.agents/skills`; use `claude`, `cursor`, `codex`, or `pi`
when the target host does not scan that shared directory. Run
`node "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness/scripts/install.mjs" --compact`
to verify the bundled runtime and receive cwd-independent next commands.

The package also bundles optional sibling `octocode-skills` for skill install,
review, and improvement. Install it from its packaged path only when that work is
needed; do not depend on registry/name lookup.

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
node packages/octocode-awareness/out/octocode-awareness.js schema commands --compact
```

## Scripts

| Script | Purpose |
|---|---|
| `scripts/awareness.mjs` | Standalone bundled CLI/runtime. |
| `scripts/schema.mjs` | Zod-built contracts; `schema path <name>` exposes each JSON Schema file. |
| `scripts/hook-runner.mjs` | Shared host lifecycle implementation. |
| `scripts/extract-hook-files.mjs` | Host payload path extraction. |
| `scripts/install.mjs` | Runtime check and hook setup guidance. |
| `scripts/smoke-multi-agent.mjs` | Native multi-agent end-to-end smoke. |
| `scripts/hooks/*.sh` | Thin lifecycle wrappers. |

Compiled scripts and `scripts/schemas/*.schema.json` are generated artifacts. Do
not hand-edit them; package maintainers regenerate them from `src/schema/*.ts`.

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
  skills/octocode-awareness
```

Build emits `out/octocode-awareness.js`, then mirrors this skill to package
`out/skills/` and local `.agents/skills/`.
