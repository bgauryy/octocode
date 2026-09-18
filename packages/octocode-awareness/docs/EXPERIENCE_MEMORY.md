# Experience and anchored memory

Awareness can preserve what an investigation learned, why a decision was made, and which evidence still applies. No source-file change is required. SQLite remains canonical; LocalGit is optional immutable evidence, not an instruction store or a second mutable database.

## Choose the operation

| Need | Operation |
|---|---|
| Record a verified reusable lesson through the existing workflow | `memory.record` |
| Create or revise an attributed lesson under a stable key | `memory.set` |
| Read a current or historical keyed lesson, or discover by anchors | `memory.get` |
| Inspect current applicability and a review queue | `memory.revalidate` |
| Record, inspect, compare, or archive an investigation | `history.experience` |
| Briefly recall relevant knowledge before acting | `context.orient` with `file`, `flow`, `failure_signature`, or `query` |

Read the live descriptors before constructing calls:

```bash
npx @octocodeai/octocode-awareness schema command memory set --compact
npx @octocodeai/octocode-awareness schema command memory get --compact
npx @octocodeai/octocode-awareness schema command history experience --compact
```

The examples below use a host-bound `client` as shown in [the API reference](API.md#client). They do not supply database, workspace, or actor parameters inside model-controlled inputs.

## Set, retrieve, and revise a lesson

```ts
const result = await client.execute({
  operation: 'memory.set',
  params: {
    key: 'message-reply-identity',
    title: 'Reply using the message identity',
    lesson: 'Read messages with include_bodies, then use the returned signal_id as in_reply_to.',
    why: 'The strict reply contract does not accept notification_id.',
    anchors: [{ kind: 'flow', value: 'message.reply' }],
    expected_revision: null,
    request_id: 'investigation-1-lesson-1',
  },
});

const current = await client.execute({
  operation: 'memory.get', params: { key: 'message-reply-identity' },
});
```

Inspect `exitCode` and the payload before using a result. Creation requires `expected_revision: null`. Updating requires the current revision from `memory.get`; a competing update returns a conflict instead of silently overwriting it. Reusing a `request_id` with identical input replays its result; reusing it with different input conflicts. Revisions retain the lesson, caller-supplied `why`, constraints, and actor/session attribution. Read an older revision with both `key` and `revision`; superseded revisions are not current advice.

Seven anchor kinds are supported: `file`, `directory`, `symbol`, `flow`, `failure`, `task`, and `decision`. Filesystem anchors normalize inside the bound workspace. A file lookup also matches lessons on its ancestor directories: `src/a` does not match `src/ab`. Logical anchors match explicit identities; they do not infer symbol renames, task equivalence, or causal relationships.

Discovery accepts multiple anchors as alternatives. A supplied text query further narrows the matches. Exact-key reads cannot be mixed with discovery filters. A default discovery read returns current keyed lessons from the bound workspace, not unrelated clones or sibling worktrees.

## Check applicability without inventing verification

`memory.set` accepts optional `applicability` with declared `files` and `capture_fingerprint: true`, or an existing native fingerprint. It can also retain declared validity dates and evidence references. `memory.get` and `memory.revalidate` assess these declarations:

| State | Meaning |
|---|---|
| `fresh` | The declared file evidence still matches |
| `stale` | Declared evidence changed or the validity interval does not apply |
| `unknown` | Applicability cannot be established from available evidence |

All keyed lessons retain `claim_verification: "unverified"`; supplied evidence references are `unchecked`. A matching fingerprint does not prove the lesson true, a successful tool call does not prove task progress, and `memory.revalidate` does not rewrite a lesson or mark it verified.

`context.orient` can include a compact advisory knowledge projection. Its revision incorporates that projection, so a declared file change can invalidate a prior orientation even without a new ledger event. Follow the attached exact-revision read to inspect a lesson before relying on it. Hosts must supply relevant context fields to receive this projection; the package does not infer every host's active flow.

## Preserve an investigation without editing files

`history.experience` uses an `action` discriminator:

| Action | Result |
|---|---|
| `record` | Append an attributed event to an open trace; exact retries are idempotent |
| `get` | Read a bounded trace from its journal or verified archive |
| `list` | Discover workspace traces with executable pagination |
| `seal` | Freeze the trace and attempt durable LocalGit archival |
| `compare` | Report differences between recorded trace events, without inferring causes |
| `recover` | List traces lacking an archive receipt for inspection and retry |

```ts
await client.execute({
  operation: 'history.experience',
  params: {
    action: 'record', trace_id: 'reply-investigation', event_id: 'attempt-1',
    kind: 'gotcha', title: 'Reply rejected',
    summary: 'The request used a notification identifier instead of a message identifier.',
    outcome: 'failure', rationale: 'Inspect the live descriptor before retrying.',
    anchors: [{ kind: 'flow', value: 'message.reply' }],
    evidence: [{ title: 'Validation result', text: 'in_reply_to and subject are required.' }],
  },
});
await client.execute({
  operation: 'history.experience',
  params: { action: 'seal', trace_id: 'reply-investigation' },
});
```

Events distinguish attempts, decisions, results, gotchas, and verification receipts. Outcomes and rationale remain attributed reports, not independent proof. Record meaningful boundaries, not every tool invocation. Keep secrets and transcripts out; validation rejects recognized secret patterns but cannot establish that arbitrary sensitive text is safe to store.

Sealing prevents further append even if LocalGit is unavailable. The SQLite trace remains readable, and another `seal` retries archival. Archive reads verify the receipt, workspace identity, manifest, and evidence bytes. Private experience refs keep archived evidence reachable without changing source Git's index, HEAD, branches, or remotes. Ordinary event-delivery pruning preserves canonical experience events and receipts.

## Bounds and current limits

Keep `partial`, `snapshot`, executable `next` calls, and `terminal_limit` diagnostics. Execute continuations with the same host bindings. A changed memory snapshot returns a restart call; do not combine it with pages from the previous snapshot. If a row or its complete continuation cannot fit the requested byte budget, the result reports an explicit terminal limit instead of dropping data. Context briefings have their own compact budget and point to detailed reads.

Experiences are bounded to 128 events per trace, with bounded evidence per event. A sealed trace cannot be reopened; start another trace for subsequent work. Revision history and sealed evidence are retained: automatic forgetting, destructive purge, and storage reclamation are not implemented by these operations. Non-file evidence is supplied explicitly; there is no automatic transcript ingestion, rename inference, or causal analysis. Callable contracts and tests do not establish activation in every agent host or prove fewer repeated mistakes in production.
