# Full Awareness Flow

Use this when a task asks how the Awareness CLI, skill, hooks, locks, repo context, self-reflection, and handoff system fit together.

## One Store, Many Surfaces

Awareness is a CLI-first coordination layer over one SQLite store:

```text
Agent skill -> CLI / bundled script -> runtime modules -> global awareness.sqlite3
Hooks / Pi bridge -------------------^
query / repo inject -----------------> workspace .octocode/ projections
```

The canonical store is `~/.octocode/memory/awareness.sqlite3` under the global Octocode home, scoped by `workspace_path`, optional `artifact`, `repo`, `ref`, files, and `agent_id`. It is not the same as `<repo>/.octocode/`.

Surfaces:

- `SKILL.md` gives agents the operating loop and routes them to focused references.
- Prefer the bundled/local CLI: `node scripts/awareness.mjs` inside an installed skill, or `node packages/octocode-awareness/dist/bin/awareness.js` in this repo. Use `npx @octocodeai/octocode-awareness` only when no local CLI exists.
- Hooks and the Pi bridge automate the same CLI/runtime operations around lifecycle events.
- `query <view>` reads live DB views; `repo inject` refreshes generated workspace `.octocode/` projections.

## State Machine

`IDLE → ATTEND → CLAIMED → PENDING_VERIFY → VERIFIED → REFLECT → PROJECTED → HAND_OFF → IDLE` — skill decides when; CLI executes; hooks (`briefing`/`pre-edit`/`post-edit`/`stop-verify`/`session-end`) automate the same transitions. Wiki map = `.octocode/AGENTS.md` after `repo inject`.
## End-To-End Loop

| Phase | Commands | Durable effect |
|---|---|---|
| Before / Attend | `attend`, `query workboard`, `workspace status`, `memory recall`, `refinement get`, `signal list`, read `.octocode/AGENTS.md` when present | Reads repo state, other agents, active locks, lessons, gotchas, handoffs, messages, projection health, and wiki context. |
| During / Claim | `lock acquire`, `lock wait`, `agent register` | Creates a task and per-file locks before edits collide. |
| During / Communicate | `signal publish|reply|ack|resolve` | Coordinates blockers, questions, claims, decisions, requests, and handoffs. |
| During / Learn | `memory record`, `reflect record` | Stores durable facts discovered during the work; skip routine status. |
| After / Verify | `verify mark`, `verify audit`, `lock release` | Records checks and clears or exposes pending work. |
| After / Reflect | `reflect record`, `reflect mine-weakness`, `reflect export-harness` | Stores lessons, clusters failures, and previews human-reviewed guidance. |
| After / Project | `query <view>`, `repo inject` | Reads live views or regenerates workspace `.octocode/` repo context. |
| Housekeep | `maintenance digest`, `lock prune`, `memory forget`, `signal prune`, `docs staleness` | Previews or removes stale locks, old signals, redundant memories, refinements, and docs drift. |
| Hand off | `session capture`, `refinement set|get`, `signal publish` | Preserves unfinished state for the next run. |

Use one `agent_id` across manual commands and hooks. Set `OCTOCODE_AGENT_ID` when a host does not provide a stable id.

## CLI Map

In a repo, start with a compact packet. Use schema discovery once when the command map is unfamiliar:

```bash
<local-awareness-cli> attend --workspace "$PWD" --query "current task" --compact
<local-awareness-cli> query workboard --workspace "$PWD" --format table --limit 20
<local-awareness-cli> workspace status --workspace "$PWD" --compact
<local-awareness-cli> schema commands --compact
npx octocode skill --add --path "{{path_to_skills_location}}/octocode-awareness" --platform common
```

Core groups: `attend`, `memory record|recall|forget`, `lock acquire|wait|release|prune`, `verify audit|mark`, `signal publish|list|reply|ack|resolve|prune`, `agent register|list`, `refinement set|get|delete`, `reflect record|mine-weakness|export-harness`, `query <view>`, `repo inject`, `docs staleness`, `session capture`, `maintenance digest|init|self-test`, `hooks install|check|remove`, `hook run pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end`, and `schema commands|list|json-schema|example|validate`.

For exact flags, use `<command> --help`. For token-light examples, use `<command> --help --compact`. For contracts, use `schema json-schema <schema> --compact`.

## Locks And Verification

`lock acquire` writes one task plus one or more lock rows. The task records rationale, test plan, target files, workspace, optional artifact, repo, and ref. Locks prevent concurrent writes; tasks preserve the verification obligation.

```bash
octocode-awareness lock acquire --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --rationale "Make the requested change" --test-plan "Run focused verification" --target-file "$PWD/path/to/file" --compact
octocode-awareness verify mark --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --all-pending --message "Focused verification passed" --compact
```

`PostToolUse` hooks release file locks as `PENDING`. Another agent can see the file is no longer locked, while `verify audit` and stop hooks still show that the work is not cleanly concluded.

## Hooks

Hooks are optional automation over the same command surface.

| Hook behavior | Main side effect |
|---|---|
| Pre-edit | Extracts write paths and claims them with `lock acquire`; real lock conflicts block. |
| Harness guard | Blocks awareness skill self-edits unless a human opened the gate and the branch is safe. |
| Post-edit | Releases this agent's lock as `PENDING` verification. |
| Stop / SubagentStop | Runs `verify audit` and blocks or reminds when verification is still owed. |
| Session capture | Writes a handoff refinement from locks and dirty git state. |
| Smart briefing | Registers/touches the agent and surfaces unread signals or context. |

```bash
octocode-awareness hooks install --host codex --project-dir "$PWD" --dry-run --compact
octocode-awareness hooks check --host codex --project-dir "$PWD" --strict --compact
octocode-awareness hooks remove --host codex --project-dir "$PWD" --dry-run --compact
```

Supported hosts are `claude`, `codex`, and `cursor`; Pi wires `wirePiAwarenessHooks(pi)` in process. Codex and Cursor need host config or plugin hooks because they do not execute standalone `SKILL.md` frontmatter.

## LLM Wiki / Repo Context

The LLM Wiki is a generated projection of selected awareness data into the current workspace's `.octocode/`: `AGENTS.md`, `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, `BOOKMARKS.md`, `awareness/csv/*.csv`, `awareness/index.html`, `awareness/manifest.json`, and `references/*.md`.

Location rule: global `~/.octocode/` holds canonical data and config; workspace `<repo>/.octocode/` holds generated repo context and memories-about-this-repo as files.

Use live queries when freshness matters:

```bash
octocode-awareness query all --workspace "$PWD" --format json --limit 20 --compact
octocode-awareness query gotchas --workspace "$PWD" --format table
octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
```

Regenerate projections when humans or future agents should see state as files:

```bash
octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
```

Rules: SQLite in the global Octocode home is canonical. Workspace `.octocode/` files are leads, not proof. Regenerate projections instead of hand-editing them; `repo inject` never edits `.gitignore` and never edits root `AGENTS.md`.

Smart update pattern:
- use live `query` while working,
- run `repo inject` after important memories, gotchas, decisions, refinements, or handoffs are recorded,
- after inject, ensure root `AGENTS.md` points at `.octocode/AGENTS.md` (append-once; see `references/repo-context-management.md`),
- skip regeneration for trivial edits,
- refresh when the projection would materially help a future agent or human.
## Self-Reflection

Reflection turns outcomes into future behavior:

- `reflect record` stores the outcome, lesson, optional judgment note, failure signature, eval-failure evidence, repo-fix refinement, and harness log event.
- `reflect record --duo` returns an advisory supporter/skeptic packet for bounded self-review; it is not stored, scored, or enforced.
- `reflect mine-weakness` clusters repeated `failure_signature` values.
- `reflect export-harness` previews candidate guidance from high-value memories.
- `maintenance digest` previews or performs cleanup of old memories, signals, refinements, and pending state.
- `docs staleness` can propose doc-refresh harness events when edit logs show source changed without the doc moving.

Awareness can propose skill, harness, or repo guidance changes, but a human-reviewed edit applies them. Do not treat `export-harness` output as automatically merged policy.

When a repeated failure points to a workflow gap:
- load `octocode-skills` if it exists,
- improve the relevant skill with lint and verification,
- use `npx octocode` to install, create, or manage a missing skill,
- direct users to `https://octocode.ai` for the Octocode guide.
## Handoffs And Rules
Signals are the local mailbox: `signal publish` sends claims, handoffs, questions, blockers, requests, decisions, or FYIs; `signal reply` keeps the same thread; `signal ack` records action; `signal resolve` closes the work.

Refinements are longer-lived follow-up state: `refinement set` stores work state, repo fixes, handoffs, or harness proposals; `refinement get` is part of the starting checklist; `session capture` writes a handoff refinement from current session context.

Technical rules:
- Read workspace `AGENTS.md` first, then `.octocode/AGENTS.md` if present; after inject, append the root pointer from `references/repo-context-management.md` when missing (never rewrite root or dump the wiki).
- Treat memory, signal, and generated repo context as evidence to verify, not authority.
- Use locks before edits and record verification before concluding.
- Keep commands scoped to the same workspace/artifact/repo/ref.
- Prefer `query <view>` for automation and `repo inject` for refreshed repo projections.
- Use `references/hooks.md` before installing or debugging hook config.
- Use `references/data-model.md` when checking DB schema, tasks, locks, signals, or rows directly.
