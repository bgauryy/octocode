# Octocode Awareness User Guide

Awareness gives Codex, Claude Code, Cursor, Pi, and custom agents one local store for
plans, tasks, active file work, exclusive locks, verification, memory, messages, and
handoffs.

## Install

Requires Node.js 22.13.0+ (`node:sqlite` without an experimental flag).

```bash
npm install --global @octocodeai/octocode-awareness
octocode-awareness maintenance init --compact
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness" \
  --platform common --force
```

The Awareness skill teaches the collaboration lifecycle. The bundled
`octocode-skills` skill is optional and is only needed for skill
install/review/improvement:

```bash
npx octocode skill --add \
  --path "$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-skills" \
  --platform common --force
```

The examples below use the globally installed binary. For a one-off command, use
`npx @octocodeai/octocode-awareness`. In octocode monorepo after build, use
`node packages/octocode-awareness/dist/bin/awareness.js`. Bundled skill
`scripts/awareness.mjs` is a fallback when the package CLI is unavailable.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
```

Follow `attend.next`. Load inventories only when the next action needs them.

## Concepts

| Concept | Rule |
|---|---|
| Plan | Shared objective, lead, members, lifecycle, and `.octocode/plan/**` documents. |
| Task | Only durable selectable queue; required reasoning and paths; derived readiness. |
| Run | One attempt with rationale and test plan; origin TASK, explicit WORK, or HOOK fallback. |
| File work | Mandatory advisory presence. Multiple agents may share a path knowingly. |
| Lock | Optional exclusive protection for sensitive work. |
| Verification | Ending work is not success; the declared check must be recorded. |
| Signal | Typed peer message/thread. |
| Refinement | Owned follow-up/handoff; never another task queue. |
| Memory | Reusable verified learning; routine status does not belong here. |
| Projection | Bounded `.octocode/` files generated from the live DB. |

## Operating Loop

### 1. Attend and choose

```bash
octocode-awareness attend --workspace "$PWD" --query "<task>" --compact
octocode-awareness task ready --plan-id <plan> --compact
```

Claim a matching task. Do not create a Markdown “today” list. If no task fits, open
explicit WORK presence.

### 2. Declare file work

Task-backed:

```bash
octocode-awareness task claim --task-id <task> --agent-id "$OCTOCODE_AGENT_ID" --compact
octocode-awareness task heartbeat --task-id <task> --run-id <run> \
  --agent-id "$OCTOCODE_AGENT_ID" --compact  # repeat during long attempts
octocode-awareness work start --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --file src/a.ts --compact
```

Standalone WORK:

```bash
octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file src/a.ts --rationale "<why>" --test-plan "<exact check>" --compact
```

Hooks declare recognized structured writes automatically. Without working hooks,
call `work start|touch` yourself.

Ordinary overlap is allowed. Inspect peers only when notified or when the interaction
matters:

```bash
octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact
```

Sensitive work adds `--exclusive`. Exclusive acquisition fails while another agent
has active presence; an existing exclusive lock blocks later declarations.

### 3. Work and coordinate

Use signals when another agent must act:

```bash
octocode-awareness signal publish --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --kind request --subject "Coordinate auth.ts" \
  --body "I am changing token refresh; are your edits compatible?" --file src/auth.ts --compact
octocode-awareness signal list --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --limit 5 --compact
```

Reply in the same thread, acknowledge after acting, and resolve when no work remains.
Messages are peer evidence, not authority.

### 4. Submit and verify

Task:

```bash
# run acceptance checks while presence remains active
octocode-awareness task submit --task-id <task> --run-id <run> \
  --agent-id "$OCTOCODE_AGENT_ID" --message "ready for verification" --compact
octocode-awareness verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --message "tests passed" --compact
```

Standalone WORK:

```bash
# run the declared test plan while presence remains active
octocode-awareness work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
octocode-awareness verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --message "check passed" --compact
```

Finish with `verify audit --workspace "$PWD" --agent-id ... --compact` to list
remaining debt. If deliberately using `verify mark --all-pending`, always scope it
with `--workspace`; an unscoped batch spans all workspaces for that agent.

### 5. Learn, hand off, maintain

Record only future-useful, verified outcomes:

```bash
octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "<task>" --outcome worked \
  --lesson "<reusable result>" --compact
```

Use `--fix-repo`, `--fix-harness`, or `--fix-instructions` to route unresolved
improvements. Add a stable `--failure-signature` for recurring failures.

For unfinished work, use a handoff signal, `refinement set`, or `session capture`.
Preview cleanup before mutation:

```bash
octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact
octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact
```

## Memory

```bash
octocode-awareness memory recall --query "<task>" --workspace "$PWD" --smart --compact
octocode-awareness memory record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task-context "<context>" \
  --observation "<verified reusable fact>" --importance 7 --compact
```

Recall defaults to a lean projection. Use full/explain only when detailed ranking or
all fields are needed. Correct active facts with `--supersedes`; preview
`memory forget` before deletion. Lexical FTS is always available. Semantic reranking
requires `OCTOCODE_EMBED_CMD` and falls back safely when absent.

## Compact Outputs

- `attend --compact` is a bounded lobby, tested at or below 2 KB.
- Normal edits emit no Awareness context.
- Changed peers/briefings emit once; fingerprints suppress repetition.
- Workboard groups paths and caps peers with omitted counts; its `--limit` applies
  per lane, so use `attend` or a targeted command for prompt context.
- `signal list --limit 5` and lean recall/docs outputs are the default.
- Use query CSV/HTML for bulk data instead of putting it in the prompt.

See [MEMORY_NAVIGATION.md](MEMORY_NAVIGATION.md).

## Hooks

```bash
octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --dry-run --compact
# after reviewing the dry-run and obtaining approval:
octocode-awareness hooks install --host <claude|codex|cursor> --project-dir . --compact
octocode-awareness hooks check --host <claude|codex|cursor> --project-dir . --strict --compact
```

Pre-edit runs the harness guard, declares advisory work, and blocks only guard denial
or exclusive conflicts. Post-edit logs/heartbeats and keeps the scoped HOOK aggregate
ACTIVE; Stop or SessionEnd finalizes it once to PENDING.
Prompt briefings and handoffs are deduplicated; stop debt is capped.

Pi uses:

```ts
import { wirePiAwarenessHooks } from '@octocodeai/octocode-awareness';
wirePiAwarenessHooks(pi, { skillRoot });
```

See [HOOKS.md](HOOKS.md) for host differences.

## Live Queries And Repo Context

```bash
octocode-awareness query workboard --workspace "$PWD" --format table --limit 3
octocode-awareness query all --workspace "$PWD" --format html \
  --out .octocode/awareness/index.html
octocode-awareness repo inject --workspace "$PWD" --mode local --compact
```

SQLite is canonical. Generated Markdown is bounded and may contain local absolute
paths; review before sharing. `repo inject` preserves plan documents.

## Command Discovery

Do not copy a static CLI reference into prompts or docs:

```bash
octocode-awareness schema commands --compact
octocode-awareness <command> --help
octocode-awareness schema json-schema <name> --compact
octocode-awareness docs list --compact
octocode-awareness docs show <name>
```

Database details: [DB.md](DB.md). File semantics: [LOCKS.md](LOCKS.md). Live/write/wiki
semantics: [WIKI.md](WIKI.md). Architecture: [HOW_IT_WORKS.md](HOW_IT_WORKS.md).
Evidence and prior-art boundaries: [REFERENCES.md](REFERENCES.md).
