# Octocode Awareness

<p align="center">
  <img src="assets/logo.png" alt="Octocode Awareness" width="300" />
</p>

Shared situational awareness for coding agents working in one workspace.

Awareness gives an agent four things that chat history cannot reliably provide:

- a live Plan → Task queue with reasons, acceptance criteria, paths, and dependencies;
- advisory visibility into which files every agent is working on and why;
- optional exclusive protection for sensitive changes;
- durable signals, verification receipts, lessons, and bounded workspace projections.

SQLite is canonical. `<workspace>/.octocode/` contains authored plan documents and
generated projections, never a second task database. There is no server or daemon.

The **Homeostatic Awareness Loop** senses coordination, verification, memory,
projection, token, and harness pressure, then recommends one bounded correction.
“Living system” is an operational metaphor—not sentience or authority. See
[docs/THESIS.md](docs/THESIS.md).

## Install

Requires Node 22.13.0 or newer. This is the first Node 22 release where
`node:sqlite` is available without an experimental flag.

```bash
npm install --global @octocodeai/octocode-awareness
octocode-awareness maintenance init --compact
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" \
  --platform common --dry-run
# after reviewing destinations and approving the write:
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" \
  --platform common --force
```

`common` installs to `~/.agents/skills`. Use `claude`, `cursor`, `codex`, or `pi`
when that host does not scan the shared directory. Verify the bundled runtime and
get cwd-independent next commands:

```bash
node "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness/scripts/install.mjs"
```

The Awareness skill is required because it teaches agents when to use the CLI.
Optionally install Research for evidence-first code work:

```bash
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-research" \
  --platform common --dry-run
# after approval, rerun with --force
```

The package bundles repo skills under `out/skills/` (Awareness and Research);
run `octocode-awareness --help` or
`scripts/install.mjs` (see its `bundled_skills` field) for the current, resolved list.

Published surfaces:

- CLI: `npx @octocodeai/octocode-awareness` → `out/octocode-awareness.js`;
- import-only library: `@octocodeai/octocode-awareness` → `out/index.js` plus declarations;
- import-only schema API: `@octocodeai/octocode-awareness/schema` → `out/schema-api.js`;
- portable Agent Skills under `out/skills/`.

Imports never execute the CLI; Awareness never bundles or delegates to the
Octocode research CLI. Install the required skill from
`out/skills/octocode-awareness`, never through registry/name lookup.

For one-off CLI use, prefer `npx @octocodeai/octocode-awareness`. In this monorepo
after build, use the local package entry
`node packages/octocode-awareness/out/octocode-awareness.js`.

## Start and work

Give each agent a stable identity and start from the bounded live packet:

```bash
export OCTOCODE_AGENT_ID="my-agent-id"
octocode-awareness attend --workspace "$PWD" --compact
```

The model is deliberately small:

```text
Plan (objective, lead, PLAN.md + docs/)
  └─ Task (reasoning, acceptance, paths, dependencies)
       └─ Run (one agent attempt + test plan)
            └─ RunFile (advisory presence; optional exclusivity)
```

Every edited path is declared. Ordinary overlap stays visible and allowed;
exclusivity is reserved for sensitive or non-mergeable work. A small change needs
no Plan or Task:

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --file src/parser.ts \
  --rationale "fix parser edge case" --test-plan "parser tests" --compact
# edit, run the declared check, then use the returned run_id
octocode-awareness work end --agent-id "$OCTOCODE_AGENT_ID" --run-id run_123 --compact
octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_123 --message "parser tests passed" --compact
octocode-awareness verify audit --workspace "$PWD" \
  --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Shared plans live under `.octocode/plan/<timestamp-name>/`; their Tasks are the
only durable work queue. “Today’s tasks” is a query, not another entity. See
[docs/SKILLS.md](docs/SKILLS.md) for plan creation, task claim/heartbeat/submit,
overlap decisions, sensitive locks, hooks, memory, and conditional closeout.

SQLite at `~/.octocode/memory/awareness.sqlite3` is canonical. Generated wiki
files are capped leads; run `wiki sync` only when file readers need a refreshed
snapshot. Command flags and payloads come from focused help and schema:

```bash
octocode-awareness <command> --help
octocode-awareness schema commands --compact
octocode-awareness schema path memory_recall --compact
```

`schema path <name>` exposes the matching generated
`out/schemas/<name>.schema.json` file to an agent. Consumers can import Zod-backed
contracts from `@octocodeai/octocode-awareness/schema`.

## Documentation

- [docs/README.md](docs/README.md) — concept-owner index
- [docs/THESIS.md](docs/THESIS.md) — bounded homeostatic control thesis
- [docs/SKILLS.md](docs/SKILLS.md) — installation and agent workflow
- [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md) — package/skill/hook architecture
- [docs/DB.md](docs/DB.md) — entities, schema, migration, journal safety
- [docs/LOCKS.md](docs/LOCKS.md) — advisory work and exclusivity
- [docs/HOOKS.md](docs/HOOKS.md) — host integration
- [docs/MEMORY_NAVIGATION.md](docs/MEMORY_NAVIGATION.md) — compact retrieval
- [docs/REFLECTION.md](docs/REFLECTION.md) — supervised learning loop
- [docs/WIKI.md](docs/WIKI.md) — live reads, durable writes, and generated projections
- [docs/HARNESS.md](docs/HARNESS.md) — maintainer invariants and verification matrix
- [docs/VERIFY.md](docs/VERIFY.md) — any-agent end-to-end health and release check
- [docs/REFERENCES.md](docs/REFERENCES.md) — evidence, prior art, and design limits
- [skills/octocode-awareness/SKILL.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-awareness/SKILL.md) — agent lobby

## Develop and verify

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness pack:check
yarn workspace @octocodeai/octocode-awareness verify
```

Edit the canonical skill only under repo-root `skills/octocode-awareness`; the
package build refreshes its generated runtime/schema helpers, `out/`, and
`.agents/skills/`. There is no package-local skill source tree. The Pi-extension
build owns its packaged copy.
