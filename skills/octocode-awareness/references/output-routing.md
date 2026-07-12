# Awareness Output Routing

Use live output for current work, durable rows for cross-run state, and generated
files only for discovery without SQLite.

| Need | Output |
|---|---|
| Start/action queue | `attend --compact`, then targeted command |
| File peers/exclusivity | `work list|show`; FilesUnderWork workboard lane |
| Tasks/verify/inbox | `attend`, then `task ready`, `verify audit`, or `signal list --limit 3` |
| Reusable learning | memory recall/record; verify before trust |
| Owned follow-up | task, signal, refinement, session capture |
| Automation/human bulk | query JSON/CSV or HTML; not prompt expansion |
| Repo discovery | bounded `wiki sync` projection |
| Contracts | grouped `schema commands --compact`; `schema commands --all` for the flat catalog; exact `schema command <noun> [action]` for schema-backed routes |

Compact `attend` caps paths/peers/bodies/IDs and keeps ≤1 row per actionable lane.
Compact list defaults are bounded; explicit limits/full flags restore depth.
`query workboard --limit N` caps each lane and can still be large. Normal hooks emit
once. Request full rows only for the next decision. Load one `docs show` reference,
never the whole set.

Empty results stay empty. Lean rows omit absent optional fields and cap repeated
tags/references with omitted counts. Filter server-side before raising limits.

Generated `.octocode/` files include a lean AGENTS map, optional nonempty bounded
KNOWLEDGE, and the awareness manifest. They are snapshots and may contain local paths;
CSV/HTML remain explicit query exports only.
Generate only after meaningful durable changes or explicit snapshot requests.

Close the owning row: verify work, ack/resolve signals, complete refinements, supersede
stale memory, or re-run cleanup/query. If projection mechanics are the next unresolved
question and that owner is not already loaded, use `references/repo-context-management.md`.
