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

The design thesis is a **Homeostatic Awareness Loop**: a human/agent-in-the-loop
software controller senses coordination, token, verification, memory, projection,
and harness pressure, then recommends the smallest bounded corrective action. The
repository is a “living system” only as an operational metaphor—not sentience,
autonomy, or authority. See [docs/THESIS.md](docs/THESIS.md).

## Install

Requires Node 22.13.0 or newer. This is the first Node 22 release where
`node:sqlite` is available without an experimental flag.

```bash
npm install --global @octocodeai/octocode-awareness
octocode-awareness maintenance init --compact
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --dry-run
# after reviewing destinations and approving the write:
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

`common` installs to `~/.agents/skills`. Use `claude`, `cursor`, `codex`, or `pi`
when that host does not scan the shared directory. Verify the bundled runtime and
get cwd-independent next commands:

```bash
node "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness/scripts/install.mjs"
```

The Awareness skill is required because it teaches agents when to use the CLI.
The bundled `octocode-skills` skill is optional and useful only for discovering,
reviewing, or improving skills:

```bash
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-skills" \
  --platform common --dry-run
# after approval, rerun with --force
```

The npm package is installed normally from npm. For the Agent Skill, use the
packaged `dist/skills/octocode-awareness` path; do not use a skill installer’s
registry/name lookup.

For one-off CLI use, prefer `npx @octocodeai/octocode-awareness`. In this monorepo
after build, use the local binary
`node packages/octocode-awareness/dist/bin/awareness.js`.

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
files are capped leads; run `repo inject` only when file readers need a refreshed
snapshot. Command flags and payloads come from focused help and schema:

```bash
octocode-awareness <command> --help
octocode-awareness schema commands --compact
```

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
- [skills/octocode-awareness/SKILL.md](skills/octocode-awareness/SKILL.md) — agent lobby

The mechanical command source of truth is always:

```bash
octocode-awareness schema commands --compact
```

## Develop and verify

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness pack:check
```

Edit the canonical skill only under
`packages/octocode-awareness/skills/octocode-awareness`; the package build refreshes
`dist/` and `.agents/skills/`. The Pi-extension build owns its packaged copy.
