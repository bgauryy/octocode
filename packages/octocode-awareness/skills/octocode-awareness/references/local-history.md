# Local History

Load when inspecting recoverable evidence or preserving an investigation. Keep file restoration separate from non-file experience records.

File History is optional byte-level recovery evidence, not shared-work truth. The host owns file capture at mutation and lifecycle boundaries; routine agents do not create file captures or checkpoints.

Use `history.status` to confirm availability, `history.timeline` to find bounded operations, and `history.read` for one exact version. Follow typed continuations with the same host bindings.

Restore is two-phase:

1. Call `history.restore` with `action: preview`, an `operation_id`, and `side`.
2. Inspect the exact changed paths and expected digests.
3. After authorization, call `history.restore` with `action: apply` and the returned `preview_id`.

Apply must fail closed when the preview expires or workspace, identity, path, digest, mode, or protection state drifts. A successful restore still requires verification; it is not proof that the workspace is correct.

For non-file evidence, inspect `schema command history experience --compact`. Record meaningful attempts, decisions, outcomes, and gotchas with stable trace/event identities. Seal at a meaningful investigation boundary. Sealing freezes the trace even if optional archival fails; inspect `recover` and retry `seal` when appropriate. SQLite retains the canonical trace. `compare` reports recorded differences, not inferred causes. Never store secrets or whole transcripts.
