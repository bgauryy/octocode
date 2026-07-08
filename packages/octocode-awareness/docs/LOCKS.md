# File Locks And Verification

**Audience**: agents and maintainers who need to understand edit coordination.

File locks prevent overlapping agents from silently editing the same files. A lock is not just a mutex; it is tied to a `task` with a rationale and a declared verification plan.

## Mental Model

```text
claim intent -> hold file locks -> edit -> release as pending -> verify -> mark success/failure
```

Tables involved:

| Table | Role |
|---|---|
| `tasks` | One claimed unit of work, including rationale, files, test plan, and status. |
| `locks` | One active row per file claimed by the task. |
| `task_log` | Verification/audit events. |
| `sessions` | Optional grouping if the host supplies a session id. |

## Task State Machine

```text
ACTIVE -> PENDING -> SUCCESS
              \----> FAILED
```

| State | Meaning |
|---|---|
| `ACTIVE` | Agent owns active locks and is expected to edit or release. |
| `PENDING` | Locks were released, but verification is still owed. |
| `SUCCESS` | The declared verification was recorded as passing. |
| `FAILED` | Verification was recorded as failing or the task was released failed. |

The important rule: unverified `SUCCESS` is downgraded to `PENDING`. This keeps "I am done" separate from "I ran the promised check."

## Acquire

```bash
octocode-awareness lock acquire \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --rationale "Edit awareness docs" \
  --test-plan "git diff --check && yarn workspace @octocodeai/octocode-awareness build" \
  --target-file "$PWD/packages/octocode-awareness/docs/DB.md"
```

Acquire does three things atomically:

1. Evicts expired locks.
2. Checks for conflicts from other agents.
3. Inserts one `tasks` row and one `locks` row per target file inside `BEGIN IMMEDIATE`.

`EXCLUSIVE` locks conflict with any other active lock on the same file. `SHARED` locks conflict only with active `EXCLUSIVE` locks.

## Wait

```bash
octocode-awareness lock wait \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --target-file "$PWD/packages/foo/src/index.ts" \
  --wait-seconds 120 \
  --retry-interval 5 \
  --compact
```

Use wait when another agent has the exact file. If the conflict is conceptual rather than mechanical, publish a signal instead of waiting silently.

## Release

```bash
octocode-awareness lock release \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --task-id task_abc \
  --status PENDING \
  --compact
```

Release deletes matching `locks` rows. If all locks for the task are gone, it updates the task status.

To mark verified success during release:

```bash
octocode-awareness lock release \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --task-id task_abc \
  --status SUCCESS \
  --verified \
  --verified-note "markdown checks passed" \
  --compact
```

Without `--verified`, a requested `SUCCESS` is stored as `PENDING`.

## Verify

```bash
octocode-awareness verify audit \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --compact
```

```bash
octocode-awareness verify mark \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --all-pending \
  --message "git diff --check and build passed" \
  --compact
```

`verify mark` updates matching pending tasks to `SUCCESS` or `FAILED` and writes `task_log` events. Scope it to the same workspace/artifact used during acquire.

## TTL, Prune, And Renew

Default lock TTL is 10 minutes and is hard-capped at 10 minutes. TTL exists to recover from crashed agents; it is not a substitute for releasing locks.

```bash
octocode-awareness lock prune \
  --workspace "$PWD" \
  --expired-only \
  --dry-run \
  --compact
```

Pi and the library API also support `renew` for long-running tasks. Renew extends lock expiration but does not remove the verification obligation.

## Hook Automation

When hooks are installed:

| Hook | Effect |
|---|---|
| `pre-edit.sh` | Claims files before a write tool runs. |
| `post-edit.sh` | Releases the claim as `PENDING`. |
| `stop-verify.sh` | Blocks session end while pending verification remains. |
| `session-end.sh` | Captures handoff/refinement state from locks and git state. |

Manual CLI calls and hooks use the same tables and state machine. See [HOOKS.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-awareness/docs/HOOKS.md) for host setup and hook behavior.

## Conflict Response

When acquire reports a conflict:

1. Read the conflict holder, file, and expiration.
2. If the lock is recent, wait or send a targeted `signal publish --kind question`.
3. If the lock is expired, use `lock prune --expired-only`.
4. If you can make progress elsewhere, switch to non-overlapping files.
5. Do not overwrite the file just because the user asked for speed; the lock is the shared workspace contract.

## Common Failure Modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `verify audit` still shows work after release | Task is `PENDING` | Run the declared test plan, then `verify mark`. |
| Another agent cannot acquire a file | Active unexpired lock | Wait, signal, or ask holder to release. |
| Locks keep appearing from hooks | Host hook is installed and sees write tools | This is expected; complete verification or tune hook config. |
| Docs staleness has no edit data | No bundled post-edit hook/Pi bridge ran, path extraction failed, or the host needs richer edit metadata | Install/check hooks with `--strict`, or use library `insertEditLog()` from host integration. |
| Locks look cross-project | Commands used different `--workspace` values | Keep one canonical workspace path per task. |
