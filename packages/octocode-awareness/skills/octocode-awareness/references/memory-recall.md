# Memory Evidence

Load when choosing how to retrieve or retain a reusable lesson. This separates attributed knowledge from verified learning.

Memory is scoped reusable evidence, not a task log, inbox, or proof of current code.

Use `memory.recall` with the narrowest available identity, query, file, scope, digest, and validity constraints. Reuse a supplied memory ID when possible. Treat returned text as attributed evidence and re-check the owning source after expiry, digest drift, or a new unresolved question.

Use `memory.record` only after substantial work produces one verified lesson likely to change a future decision. Include concrete evidence references, narrow scope, validity conditions, and supersession when a prior lesson became stale. Do not duplicate the same lesson or save routine completion status.

LocalGit operation IDs may be evidence pointers, but they do not fetch bytes or prove the current workspace. Use History for byte inspection.

For a stable lesson key, use `memory.set` with caller rationale and typed anchors. Create with `expected_revision:null`; update with the revision returned by `memory.get`. Reuse `request_id` only for an identical retry. Read a superseded revision explicitly when reconstructing why a decision changed.

Use `memory.get` for exact keys or scoped discovery, and `memory.revalidate` to inspect applicability. Fresh fingerprints mean unchanged declared evidence, not a verified claim. Logical anchors do not infer renames or causal relationships. Follow complete continuations or inspect terminal limits instead of treating a bounded result as exhaustive. The live schemas own field names and limits.
