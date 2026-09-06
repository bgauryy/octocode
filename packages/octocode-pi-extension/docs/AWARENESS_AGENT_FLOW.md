# Awareness Agent Flow in Pi

Pi exposes one coordinated task flow over the Awareness ledger. The model uses
`plan` for session and shared execution; it does not manually synchronize a local
checklist with separate plan, task, work-presence, and verification tools.

Awareness remains the cross-host SQLite backend. Other agents use its canonical
CLI and library operations; Pi has one model-facing coordination surface.

## Pi surface

| Concern | Owner |
|---|---|
| Session or shared execution | `plan` |
| Unread direct peer input | Automatic count → `message` inbox |
| Exceptional non-mergeable exclusivity | `lock` |
| Necessary peer communication | `message` |
| Reusable verified learning | `memory` |
| Backend diagnostics and recovery | `$OCTOCODE_AWARENESS_CLI` |

The catalog is unconditional. The Lite CLI and library retain canonical backend
operations, and existing SQLite data remains readable.

## Identity and automatic lifecycle

- An explicit `OCTOCODE_AGENT_ID` remains stable.
- Otherwise Pi derives a session identity and refreshes it for `/new`, `/resume`, and
  forks.
- Pi joins and leaves the shared peer registry automatically.
- Spawned workers use child identities derived from their parent session.
- Routine advisory file presence is created by the mutation gate and cleaned at
  session shutdown; leases provide crash recovery.

Do not add manual join, start-presence, finish-presence, or status calls to a normal
solo task.

## Signals, not ceremony

The TUI shows passive shared state. The model receives a bounded
`<awareness_signal>` only when it has unread direct peer messages. Global plan, task,
work, lock, peer, and verification counts do not enter model context.

Peer-authored message bodies and task titles are not injected into the system prompt.
The signal routes directly to `message`; deeper diagnosis and recovery remain available
through the Awareness skill/CLI. A plan count, an agent count, automatic
presence, or already-read messages alone do not require a coordination call.

## Plan scope

`plan` accepts `scope: auto | session | shared`.

- `session` keeps the checklist local to the Pi session.
- `shared` projects the stable plan and step identities onto existing Awareness
  plans and tasks.
- `auto` stays session-local unless Pi can safely adopt one currently claimed shared
  task owned by this agent. It does not adopt by title or path and does not manufacture
  a shared plan for routine solo work.

Projection reuses Awareness's transactional materialization and reconciliation.
Repeated Start or projection is idempotent: stable source and step keys reconcile the
same rows, dependencies, paths, acceptance criteria, and declared check commands.

For an RFC plan, user **Accept** records the exact reviewed revision but creates no
shared rows. A separate user **Start** authorizes implementation and materializes the
shared graph. Repeated Start reconciles rather than duplicates it.

## Completion and observed receipts

For a mapped shared step, call `plan.complete` with the check that actually ran:

```text
receipt: {
  command: "<exact declared check command>",
  status: "SUCCESS" | "FAILED",
  message: "<concise observed result>"
}
```

The command must match the task's declared check command. On success, Pi completes the
shared task, records the check receipt, advances the local step, claims the next
ready dependency, and closes the shared plan after every task is verified.

A failed receipt reopens shared execution and leaves the local step active. If receipt
persistence fails after shared completion, Pi attempts compensation by reopening the
task. An unrecoverable compensation failure is reported as explicit verification debt;
it is never hidden behind a locally completed step.

Slash completion, removal, and clear operations cannot bypass mapped shared receipt or
unfinished-task safety. Separate submit/verify operations remain backend recovery or a
configured independent-review workflow, not the normal Pi completion path.

## Mutation-time coordination

Before identifiable mutations, Pi:

1. extracts every explicit target from structured write inputs or batched `queries[]`;
2. extracts explicit bash targets recognized by `extractBashWriteTargets` (for example,
   redirects, `tee`, `cp`, `mv`, and in-place editors);
3. checks all targets for peer-held locks before starting any advisory presence; and
4. starts or refreshes this session's advisory presence only after the complete lock
   pass succeeds.

A same-owner lock is allowed and a peer-owned lock blocks the mutation. If no Awareness
store exists, mutation safety fails open. If a store exists but lock state cannot be
queried, an identifiable mutation fails closed. Advisory-presence failures warn and
fail open.

Implicit generated output and opaque interpreters with no extracted path cannot be
preflighted and are not claimed as covered. Use an explicit `lock` for sensitive or
non-mergeable state when concurrent mutation would be unsafe.

## Exceptional tools

- `lock acquire|wait|release`: only for state that cannot be merged safely. Mutation
  checks already enforce peer-held locks; ordinary source edits do not need one.
- `message`: only when a peer needs a blocker, question, decision, evidence, handoff,
  or overlap notice that changes the recipient's next action.
- `memory`: recall only when prior learning can change the approach; store only
  verified reusable outcomes that source and docs do not already own.

With `storage.mode=memory`, these durable Awareness operations return explicit errors;
the extension does not pretend a lock, message, or memory write succeeded. Session-local
`session.json`, `plan/index.json`, `tasks/index.json`, and `backlog/index.json` remain
available as inspectable projections, never as replacements for the Awareness ledger.

## Diagnostics and recovery

Use `node "$OCTOCODE_AWARENESS_CLI" schema ...` to inspect exact Lite command shapes
before invoking backend recovery. The CLI remains appropriate for verification-debt
audit, unresolved continuation notes, stale ownership, or a backend state that the
reduced Pi tools intentionally do not expose.

Never hand-edit the SQLite database or generated Awareness state. Recovery records
evidence; it does not execute a check, authorize taking over another agent's task, or
make an expired lease count as success.
