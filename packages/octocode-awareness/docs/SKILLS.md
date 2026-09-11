# Octocode Awareness user guide

Awareness gives supported coding hosts and custom agents one façade for plans,
tasks, active file work, exclusive locks, verification, memory, signals, and
session capture. The default store is `$OCTOCODE_HOME/awareness/awareness.sqlite3`;
explicit repository scope uses `<workspace>/.octocode/awareness.sqlite3`. It is
separate from Agent runtime and control databases.

## Install

Requires Node.js `^24.15.0` (`node:sqlite` without an experimental flag).

```bash
npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run
# After approval, rerun without --dry-run.
npx @octocodeai/octocode-awareness maintenance init --compact
```

The CLI bundles only `octocode-awareness`. It atomically materializes a durable
copy under `$OCTOCODE_HOME/skills/octocode-awareness`, then links the selected
host directory to that copy. The link never targets an npm or `npx` cache. Use
`skill install --help` for host-specific user/project destinations; do not derive
package paths in a prompt. `claude-desktop` reuses Claude Code's destination;
`codex-native`, `shared`, `common`, and `agents` reuse Codex's `.agents/skills`
destination. `--platform all` selects each distinct destination once.
Initialization and identical installs are safe to repeat.
Use `--upgrade` to refresh the durable canonical copy while preserving managed
platform links or copies; arbitrary destination drift still requires `--force`.

Inspect and remove the bundled skill through the same lifecycle:

```bash
# Bundled catalog plus durable-copy state.
npx @octocodeai/octocode-awareness skill list --compact

# With no platform, check only the durable canonical copy.
npx @octocodeai/octocode-awareness skill check --compact
# Platform checks require the exact scope that was installed.
npx @octocodeai/octocode-awareness skill check --platform shared \
    --project-dir "$PWD" --compact

# Removal previews by default. --confirm applies the exact previewed paths.
npx @octocodeai/octocode-awareness skill remove --platform shared \
    --project-dir "$PWD" --compact
npx @octocodeai/octocode-awareness skill remove --platform shared \
    --project-dir "$PWD" --confirm --compact
```

Platform removal always preserves `$OCTOCODE_HOME/skills/octocode-awareness`.
Removing that durable copy is a separate explicit operation: preview
`skill remove --canonical`, then add `--confirm`. Canonical-only removal preserves
platform entries, which become broken links until the skill is installed again or
each scoped platform entry is removed. This safer contract differs from Octocode's
broad no-platform removal: Awareness never infers a destructive scope. `--force`
does not select a removal scope.

The package bundles only the Awareness skill for the collaboration lifecycle. The
separately owned `octocode-orchestrator` skill remains in the sibling
[`octocode-agent` repository](https://github.com/bgauryy/octocode-agent/tree/main/skills/octocode-orchestrator).
Install other workflow skills separately with `npx octocode skill install <skill>`
when that work is needed. Discover the package-bundled list with
`npx @octocodeai/octocode-awareness --help` — do not hardcode a skill list from prose.

The examples below are for external CLI hosts. Pi uses its native `awareness`
tool; other native hosts import the [command API](API.md). All interfaces share
the same command schemas and selected Awareness store.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
npx @octocodeai/octocode-awareness attend --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
```

Attend once or reuse a host briefing, then communicate when a peer needs to know or
act. Default attendance reads registered peers only. Use `--details` for deeper
work/evidence routing, and load one unfamiliar command schema when needed.
Tracking and learning are conditional. Omit `--db-scope` for the global default; use
`--db-scope repo` only for deliberate repository isolation, and use `--db` only
for an explicit isolated path.

## Agent activation map

| Surface | Job |
|---|---|
| `AGENTS.md` / host instructions | Trigger Awareness and point at the smallest owner. |
| `octocode-awareness` skill | Decide when and how to plan, coordinate, lock, verify, remember, or clean. |
| API / CLI / SQLite | Read and mutate canonical live plans, tasks, presence, verification, signals, and memory. |
| Host hooks | Register peers and deliver changed messages by default; guard/full opt into tracking. |
| `.octocode/` | Discover authored plan docs and bounded generated snapshots when live SQLite is unavailable to a reader. |

Agents should begin with `attend`, not by reading `.octocode/`. A plan document may
explain intent; live state comes from `attend`, `query`, `memory recall`, or `docs show`.
`.octocode/` query exports are read-only snapshots — never hand-edit them or read them as state.

Do not add a separate coordination task for routine solo work. Keep scope,
conflict resolution, memory judgment, and observed verification with the work owner.

## Concepts

| Concept | Rule |
|---|---|
| Plan | Shared objective, lead, members, lifecycle, and `.octocode/plan/**` documents. |
| Task | Only durable selectable queue; required reasoning and paths; derived readiness. |
| Run | One attempt with rationale and test plan; origin TASK, explicit WORK, or HOOK fallback. |
| File work | Advisory presence for tracked work. Multiple agents may share a path knowingly. |
| Lock | Optional exclusive protection for sensitive work. |
| Verification | Ending work is not success; the declared check must be recorded. |
| Signal | Typed peer message/thread. |
| Refinement | Owned follow-up/handoff; never another task queue. |
| Memory | Reusable verified learning; routine status does not belong here. |
| Query export | Read-only `.octocode/` snapshot written on request via `query --format html/json/csv`. |

## Track shared or substantial work

Use this lifecycle when shared ownership or explicit verification tracking helps.
Routine solo edits need no Awareness work row. Reuse native task/run IDs when the
host already owns the lifecycle; do not repeat its bookkeeping manually.

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

Guard/full hooks declare recognized structured writes automatically. For tracked
work without those hooks, call `work start|touch` on the owning run.

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

After substantial work or a meaningful event, record only future-useful, verified
outcomes. Skip routine edits and repeated lessons:

```bash
npx @octocodeai/octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "<task>" --outcome worked \
  --lesson "<reusable result>" --compact
```

Use `--fix-repo`, `--fix-harness`, or `--fix-instructions` to route unresolved
improvements. Add a stable `--failure-signature` for recurring failures.

For unfinished continuation, use `handoff add/list/clear` and reuse the host's
existing handoff. Refinement and session capture are specialist routes, not
additional completion steps. Routine work needs no reflection or memory record.
Preview cleanup before mutation:

```bash
npx @octocodeai/octocode-awareness maintenance digest --workspace "$PWD" --dry-run --compact
npx @octocodeai/octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact
npx @octocodeai/octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact
```

## Memory

Recall only when prior learning could change the approach. Record only verified
reusable learning; neither operation is a per-task or per-turn requirement.

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

- Default `attend` returns bounded registry pages; `--details` selects the separately budgeted observer.
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

With guard/full tracking, pre-edit runs the harness guard, declares advisory work, and blocks only guard denial
or exclusive conflicts. A successful post-edit logs/heartbeats and keeps the scoped
HOOK aggregate ACTIVE; a failed write discards hook-created presence and creates no
edit audit or verification debt. Stop, PreCompact, or SessionEnd finalizes successful
work once to PENDING. PreCompact keeps the session reusable; SessionEnd marks it ended.
Peer messages are deduplicated. Stop reminders and automatic handoffs require
their global feature switches; both default off. The coordination profile creates
no per-edit work records. Missing configuration uses defaults without onboarding.

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
