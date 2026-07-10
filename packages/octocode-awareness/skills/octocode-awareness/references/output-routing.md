# Awareness Output Routing

Use live output for current work, durable rows for cross-run state, and generated
files only for discovery without SQLite.

| Need | Output |
|---|---|
| Start/action queue | `attend --compact`, then targeted command |
| File peers/exclusivity | `work list|show`; FilesUnderWork workboard lane |
| Tasks/verify/inbox | `attend`, then `task ready`, `verify audit`, or `signal list` |
| Reusable learning | memory recall/record; verify before trust |
| Owned follow-up | task, signal, refinement, session capture |
| Automation/human bulk | query JSON/CSV or HTML; not prompt expansion |
| Repo discovery | bounded `repo inject` projection |
| Contracts | `schema commands --compact`; add `--examples` only if needed; `<command> --help`; `docs show` |

Compact `attend` caps paths/peers/bodies/IDs and keeps ≤1 row per actionable lane.
Generic `--compact` only guarantees minified JSON; `query workboard --limit N` caps
each lane and can still be large. Normal hooks are silent; changed fingerprints emit
once. Request full rows only for the next decision. Load one `docs show` reference,
never the whole set.

Generated `.octocode/` files include bounded memory/gotcha/lesson/bookmark/developer
review indexes plus CSV/HTML/manifest. They are snapshots and may contain local paths.
Generate only after meaningful durable changes or explicit snapshot requests.

Close the owning row: verify work, ack/resolve signals, complete refinements, supersede
stale memory, or re-run cleanup/query. Projection mechanics:
`references/repo-context-management.md`.
