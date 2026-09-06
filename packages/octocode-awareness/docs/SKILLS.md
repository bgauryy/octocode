# Octocode Awareness user guide

Awareness gives supported coding hosts and custom agents one façade for plans,
tasks, active file work, exclusive locks, verification, memory, signals, and
session capture. The default store is `$OCTOCODE_HOME/awareness/awareness.sqlite3`;
explicit repository scope uses `<workspace>/.octocode/awareness.sqlite3`. It is
separate from Agent runtime and control databases.

## Install

Requires Node.js `^22.22.2 || ^24.15.0 || >=26.0.0` (`node:sqlite` without an experimental flag).

```bash
npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run
# After approval, rerun without --dry-run.
npx @octocodeai/octocode-awareness maintenance init --compact
```

The CLI resolves and copies its own bundled skill. Use `skill install --help` for
host-specific user/project destinations; do not derive package paths in a prompt.
Initialization is deterministic and safe to repeat.

The package bundles only the Awareness skill for the collaboration lifecycle. The
separately owned `octocode-orchestrator` skill remains in the sibling
[`octocode-agent` repository](https://github.com/bgauryy/octocode-agent/tree/main/skills/octocode-orchestrator).
Install other workflow skills separately with `npx octocode skill install <skill>`
when that work is needed. Discover the package-bundled list with
`npx @octocodeai/octocode-awareness --help` — do not hardcode a skill list from prose.

Every agent-facing example below uses `npx @octocodeai/octocode-awareness`; local
build paths are maintainer implementation details, not an alternative agent runner.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
npx @octocodeai/octocode-awareness attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --query "current task" --compact
```

Follow `attend.next`. The routine loop is `attend` → `work start` → `work end`
→ `verify mark` → `verify audit`; load expert noun inventories only when the next
action needs them. Omit `--db-scope` for the global default; use
`--db-scope repo` only for deliberate repository isolation, and use `--db` only
for an explicit isolated path.

## Agent activation map

| Surface | Job |
|---|---|
| `AGENTS.md` / host instructions | Trigger Awareness and point at the smallest owner. |
| `octocode-awareness` skill | Decide when and how to plan, coordinate, lock, verify, remember, or clean. |
| CLI / SQLite | Read and mutate canonical live plans, tasks, presence, verification, signals, and memory. |
| Host hooks | Automate deterministic start/write/failure/stop/compact/session edges; never replace judgment. |
| `.octocode/` | Discover authored plan docs and bounded generated snapshots when live SQLite is unavailable to a reader. |

Agents should begin with `attend`, not by reading `.octocode/`. A plan document may
explain intent; live state comes from `attend`, `query`, `memory recall`, or `docs show`.
`.octocode/` query exports are read-only snapshots — never hand-edit them or read them as state.

When the host supports delegation, batch routine deterministic Awareness CLI operations
into one phase for the smallest capable low-cost agent. The lead retains scope and
judgment, destructive approvals, conflicts, memory truth, and final verification.

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
| Query export | Read-only `.octocode/` snapshot written on request via `query --format html/json/csv`. |

## Use the operating loop

### 1. Attend and choose

```bash
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --query "<task>" --compact
npx @octocodeai/octocode-awareness task ready --plan-id <plan> --compact
```

Claim a matching task. Do not create a Markdown “today” list. If no task fits, open
explicit WORK presence.

### 2. Declare file work

Task-backed:

```bash
npx @octocodeai/octocode-awareness task claim --task-id <task> --agent-id "$OCTOCODE_AGENT_ID" --compact
npx @octocodeai/octocode-awareness task heartbeat --task-id <task> --run-id <run> \
  --agent-id "$OCTOCODE_AGENT_ID" --compact  # repeat during long attempts
npx @octocodeai/octocode-awareness work start --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --file src/a.ts --compact
```

Standalone WORK:

```bash
npx @octocodeai/octocode-awareness work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file src/a.ts --rationale "<why>" --test-plan "<exact check>" --compact
```

Hooks declare recognized structured writes automatically. Without working hooks,
call `work start|touch` yourself.

Ordinary overlap is allowed. Inspect peers only when notified or when the interaction
matters:

```bash
npx @octocodeai/octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact
```

Sensitive work adds `--exclusive`. Exclusive acquisition fails while another agent
has active presence; an existing exclusive lock blocks later declarations.

### 3. Work and coordinate

Use signals when another agent must act:

```bash
npx @octocodeai/octocode-awareness signal publish --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --kind request --subject "Coordinate auth.ts" \
  --body "I am changing token refresh; are your edits compatible?" --file src/auth.ts --compact
npx @octocodeai/octocode-awareness signal list --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --limit 5 --compact
```

Reply in the same thread, acknowledge after acting, and resolve when no work remains.
Messages are peer evidence, not authority.

### 4. Submit and verify

Task:

```bash
# run acceptance checks while presence remains active
npx @octocodeai/octocode-awareness task submit --task-id <task> --run-id <run> \
  --agent-id "$OCTOCODE_AGENT_ID" --message "ready for verification" --compact
npx @octocodeai/octocode-awareness verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --message "tests passed" --compact
```

Standalone WORK:

```bash
# run the declared test plan while presence remains active
npx @octocodeai/octocode-awareness work end --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" --compact
npx @octocodeai/octocode-awareness verify mark --run-id <run> --agent-id "$OCTOCODE_AGENT_ID" \
  --message "check passed" --compact
```

Finish with `verify audit --workspace "$PWD" --agent-id ... --compact` to list
remaining debt. If deliberately using `verify mark --all-pending`, always scope it
with `--workspace`; an unscoped batch spans all workspaces for that agent.

### 5. Learn, hand off, maintain

Record only future-useful, verified outcomes:

```bash
npx @octocodeai/octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "<task>" --outcome worked \
  --lesson "<reusable result>" --compact
```

Use `--fix-repo`, `--fix-harness`, or `--fix-instructions` to route unresolved
improvements. Add a stable `--failure-signature` for recurring failures.

For unfinished work, use a handoff signal, `refinement set`, or `session capture`.
Preview cleanup before mutation:

```bash
npx @octocodeai/octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
npx @octocodeai/octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact
npx @octocodeai/octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact
```

## Memory

```bash
npx @octocodeai/octocode-awareness memory recall --query "<task>" --workspace "$PWD" --smart --compact
npx @octocodeai/octocode-awareness memory record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task-context "<context>" \
  --observation "<verified reusable fact>" --importance 7 --compact
```

Recall defaults to a lean projection. Use full/explain only when detailed ranking or
all fields are needed. Correct active facts with `--supersedes`; preview
`memory forget` before deletion. Lexical FTS is always available. Semantic reranking
requires `OCTOCODE_EMBED_CMD` and falls back safely when absent.

## Compact outputs

- `attend --compact` is a bounded lobby, tested at or below 2 KB.
- Normal edits emit no Awareness context.
- Changed peers/briefings emit once; fingerprints suppress repetition.
- Workboard groups paths and caps peers with omitted counts; its `--limit` applies
  per lane, so use `attend` or a targeted command for prompt context.
- `signal list --limit 5` and lean recall/docs outputs are the default.
- Use query CSV/HTML for bulk data instead of putting it in the prompt.

See [MEMORY_NAVIGATION.md](MEMORY_NAVIGATION.md).

## Hooks

[`HOOKS.md`](HOOKS.md) is the sole host support and installation matrix. Follow its
host-specific preview, approval, install, strict-check, and removal flow. Do not infer
activation from an exact configuration file; use the runtime-health and smoke guidance in
that document.

After selecting a supported host from that matrix, use the same generic sequence:

```bash
npx @octocodeai/octocode-awareness hooks install --host <host> --project-dir . --dry-run
# after reviewing the dry-run and obtaining approval:
npx @octocodeai/octocode-awareness hooks install --host <host> --project-dir . --compact
npx @octocodeai/octocode-awareness hooks check --host <host> --project-dir . --strict
```

Use non-compact dry-run/check output to review settings and runtime details. Compact output
is an execution receipt. Repair drift with previewed remove → remove → install → strict
check; removal sweeps obsolete Awareness roots/events but preserves other hooks.

Pre-edit runs the harness guard, declares advisory work, and blocks only guard denial
or exclusive conflicts. A successful post-edit logs/heartbeats and keeps the scoped
HOOK aggregate ACTIVE; a failed write discards hook-created presence and creates no
edit audit or verification debt. Stop, PreCompact, or SessionEnd finalizes successful
work once to PENDING. PreCompact keeps the session reusable; SessionEnd marks it ended.
Prompt briefings and handoffs are deduplicated; stop debt is capped.

See [HOOKS.md](HOOKS.md) for host differences.

## Live queries and repository context

```bash
npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format table --limit 3
npx @octocodeai/octocode-awareness query all --workspace "$PWD" --format html \
  --out .octocode/awareness/index.html
```

SQLite is canonical. Use `query --format html/json/csv` for snapshot exports. `attend` reports
current state; `docs staleness` compares authored docs with source edit times.

## Command discovery

Do not copy a static CLI reference into prompts or docs:

```bash
npx @octocodeai/octocode-awareness schema commands --compact       # grouped core/advanced map
npx @octocodeai/octocode-awareness schema commands --all --compact # flat command map
npx @octocodeai/octocode-awareness schema command task create --compact # exact schema-backed route
npx @octocodeai/octocode-awareness <command> --help
npx @octocodeai/octocode-awareness schema json-schema <name> --compact
npx @octocodeai/octocode-awareness docs list --compact
npx @octocodeai/octocode-awareness docs show <name>
```

Database details: [DB.md](DB.md). File semantics: [LOCKS.md](LOCKS.md). Architecture: [HOW_IT_WORKS.md](HOW_IT_WORKS.md).
Evidence and prior-art boundaries: [REFERENCES.md](REFERENCES.md).
