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

## Install

Requires Node 22 or newer with `node:sqlite`.

```bash
npm install --global @octocodeai/octocode-awareness
octocode-awareness maintenance init --compact
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

The Awareness skill is required because it teaches agents when to use the CLI.
The bundled `octocode-skills` skill is optional and useful only for discovering,
reviewing, or improving skills:

```bash
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-skills" \
  --platform common --force
```

The npm package is installed normally from npm. For the Agent Skill, use the
packaged `dist/skills/octocode-awareness` path; do not use a skill installer’s
registry/name lookup.

For one-off CLI use, replace `octocode-awareness` with
`npx @octocodeai/octocode-awareness`.

## Start every active coding session

Set a stable identity once, then ask the live store for the smallest useful packet:

```bash
export OCTOCODE_AGENT_ID="my-agent-id"
octocode-awareness attend --workspace "$PWD" --compact
```

Use targeted discovery instead of loading every command or document:

```bash
octocode-awareness docs list --compact
octocode-awareness docs show agent-cheatsheet
octocode-awareness schema commands --compact
octocode-awareness <command> --help
```

`docs show` is raw Markdown by default. Add `--compact` only when a program needs
the JSON envelope.

## The work model

```text
Plan (objective, lead, PLAN.md, docs/)
  └─ Task (reasoning, acceptance, one or more paths, dependencies)
       └─ Run (one agent attempt and test plan)
            └─ RunFile (advisory file presence; optional exclusivity)
```

Every edited file must be declared under an active Task or standalone Work run.
Declaration is advisory by default: several agents may work on the same file and
see each other’s agent, reason, run, and expiry. Use exclusivity only when overlap
would be unsafe—for example schema/migration, security, hook/skill self-modification,
generated release metadata, or another non-mergeable operation.

Task paths are planning scope, not locks. There is one task queue; “today’s tasks”
is a query, not another entity.

## Standalone change

No Plan or Task is required for a small independent change:

```bash
START=$(octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --file src/parser.ts \
  --rationale "fix parser edge case" --test-plan "parser tests" --compact)
# retain run_id from START; edit; use work touch for a long-running change
octocode-awareness work end --agent-id "$OCTOCODE_AGENT_ID" --run-id run_123 --compact
octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" \
  --run-id run_123 --message "parser tests passed" --compact
```

For sensitive work, add `--exclusive` to `work start`. Low-level `lock` commands
exist for explicit protection/waiting, but ordinary edits should use advisory Work.

## Shared plan and tasks

The lead creates a managed plan folder under
`.octocode/plan/<timestamp-name>/`, adds supporting documents, and creates tasks:

```bash
octocode-awareness plan create --name "Parser hardening" \
  --objective "Make malformed input safe" --lead-agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --compact
octocode-awareness plan doc --plan-id plan_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --path docs/DESIGN.md --title "Parser design" --compact
octocode-awareness task create --plan-id plan_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --title "Reject malformed escapes" --reasoning "Avoid ambiguous tokenization" \
  --acceptance "parser tests cover malformed escapes" --path src/parser.ts --compact
octocode-awareness plan status --plan-id plan_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --status ACTIVE --compact
```

Agents choose dependency-ready work and retain the returned `run_id`:

```bash
octocode-awareness task ready --plan-id plan_123 --compact
octocode-awareness task claim --task-id task_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
octocode-awareness work start --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --file src/parser.ts --compact
# edit and verify
octocode-awareness task submit --task-id task_123 --run-id run_123 \
  --agent-id "$OCTOCODE_AGENT_ID" --message "parser tests passed" --compact
octocode-awareness verify mark --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "parser tests passed" --compact
```

Plan status governs readiness: only ACTIVE plans expose new claims. Pause before
replanning; do not complete/cancel while agents still own active runs.

## Hooks

Hooks automate the same declarations and finish warnings; they do not create a
second lifecycle. Preview, install with user approval, then check:

```bash
octocode-awareness hooks install --host <claude|codex|cursor> \
  --project-dir . --dry-run --compact
octocode-awareness hooks install --host <claude|codex|cursor> \
  --project-dir . --compact
octocode-awareness hooks check --host <claude|codex|cursor> \
  --project-dir . --strict --compact
```

Pi uses the in-process `wirePiAwarenessHooks` bridge rather than shell hook files.

## Storage and concurrency

The default store is `~/.octocode/memory/awareness.sqlite3`; override its directory
with `OCTOCODE_MEMORY_HOME`. `workspace_path` is the primary boundary and
`artifact` optionally narrows it.

Awareness enables WAL only when the embedded SQLite contains the concurrent WAL
reset fix (SQLite 3.44.6, 3.50.7, 3.51.3, or a newer fixed release). Older affected
runtimes automatically use rollback journaling, preserving correctness with less
write concurrency. Upgrade Node before relying on concurrent WAL throughput.

Generated projections are optional and capped. Refresh them only when file-based
readers need current context:

```bash
octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
octocode-awareness repo inject --workspace "$PWD" --mode local --compact
```

## Documentation

- [docs/SKILLS.md](docs/SKILLS.md) — installation and agent workflow
- [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md) — package/skill/hook architecture
- [docs/DB.md](docs/DB.md) — entities, schema, migration, journal safety
- [docs/LOCKS.md](docs/LOCKS.md) — advisory work and exclusivity
- [docs/HOOKS.md](docs/HOOKS.md) — host integration
- [docs/MEMORY_NAVIGATION.md](docs/MEMORY_NAVIGATION.md) — compact retrieval
- [docs/REFLECTION.md](docs/REFLECTION.md) — supervised learning loop
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
