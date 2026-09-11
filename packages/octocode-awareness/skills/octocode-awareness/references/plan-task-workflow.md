# Shared Work

Load this reference only when ownership, dependencies, or resumption justify durable work. Ordinary solo edits need no record.

Use `work.create` with `kind: plan`, `task`, or `standalone`. Inspect existing records with `work.list` and `work.show`; do not duplicate host-owned work. Use `work.depend` for real dependency edges and `work.claim` for an available task.

Transitions use `work.update`: `heartbeat`, `submit`, `release`, `retry`, `touch`, `end`, `join`, `document`, or `status`. The selected transition determines required fields; inspect its schema before calling it.

Completion is receipt-gated: run the declared check, transition the work, then call `work.verify` with `action: mark` and the observed result. Use `action: audit` after final writes. Never infer success from a claim, lease expiry, message, or clean workboard.
