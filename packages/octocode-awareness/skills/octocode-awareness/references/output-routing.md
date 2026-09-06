# Awareness Output Routing

Load when deciding whether output belongs in live response, SQLite, or an export. This reference step ends here; return to the main skill flow.

Use live output for current work and durable SQLite rows for cross-run state. SQLite is canonical; no generated wiki projection — only optional read-only query exports.

| Need | Output |
|---|---|
| Start/action queue | `attend --compact`, then targeted command |
| File peers/exclusivity | `work list|show`; FilesUnderWork workboard lane |
| Tasks/verify/inbox | `attend`, then `task ready`, `verify audit`, or `signal list --limit 3` |
| Reusable learning | memory recall/record; verify before trust |
| Owned follow-up | task, signal, refinement |
| Automation/human bulk | query JSON/CSV or HTML export; not prompt expansion |
| Contracts | grouped `schema commands --compact`; `schema commands --all` for the flat catalog; exact `schema command <noun> [action]` for schema-backed routes |

Compact `attend` caps paths/peers/bodies/IDs, ≤1 row per actionable lane. Compact list defaults are bounded; explicit limits/full flags restore depth. `query workboard --limit N` caps each lane and can still be large. Normal hooks emit once. Request full rows only for the next decision. Load one `docs show` reference, never the whole set.

Empty results stay empty. Lean rows omit absent optional fields and cap repeated tags/references with omitted counts. Filter server-side before raising limits.

`query all --format html` (and JSON/CSV) writes a read-only view under `.octocode/` only when explicitly requested; it is an export, never a canonical store, and may contain local paths. There is no automatic `.octocode/` generation.

Return to `SKILL.md` after closing the owning row: verify work, resolve signals, complete refinements, or supersede stale memory.
