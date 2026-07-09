# Awareness Output Routing

Use live output for current work, durable rows for cross-run state, and generated
files only for discovery without SQLite.

| Need | Output |
|---|---|
| Start/action queue | `attend --compact`, then targeted command |
| File peers/exclusivity | `work list|show`; FilesUnderWork workboard lane |
| Tasks/verify/inbox | `query workboard`, `workspace status`, `verify audit`, `signal list` |
| Reusable learning | memory recall/record; verify before trust |
| Owned follow-up | task, signal, refinement, session capture |
| Automation/human bulk | query JSON/CSV or HTML; not prompt expansion |
| Repo discovery | bounded `repo inject` projection |
| Contracts | `schema commands --compact`; add `--examples` only if needed; `<command> --help`; `docs show` |

Compact output caps paths/peers/bodies/IDs and includes omitted counts. `attend --compact`
keeps only actionable workboard columns (≤1 row each) and slim file rows (path/peers/lock).
Normal hooks are silent; changed briefing/peer fingerprints emit once. Request full rows
only when the next decision needs them. Load **one** skill reference via `docs show` —
do not preload the 27-ref set.

Generated `.octocode/` files include bounded memory/gotcha/lesson/bookmark/developer
review indexes plus CSV/HTML/manifest. They are snapshots and may contain local paths.
Generate only after meaningful durable changes or explicit snapshot requests.

Close the owning row: verify work, ack/resolve signals, complete refinements, supersede
stale memory, or re-run cleanup/query. Projection mechanics:
`repo-context-management.md`.
