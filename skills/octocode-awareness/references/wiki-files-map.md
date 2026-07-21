# Wiki Files Map

Use this to see how the minimal `.octocode/` projection relates to canonical SQLite. Durable-output routing: `references/learning-loop.md`.

| File | Source | Relation |
|---|---|---|
| `AGENTS.md` | command/projection metadata | Lean discovery map; routes agents to live reads and optional knowledge. |
| `KNOWLEDGE.md` | selected active memories | Nonempty bounded knowledge leads; never a complete memory dump. |
| `awareness/manifest.json` | generation scope/revision | Ownership, completeness, budgets, and reviewed retired-file cleanup receipts. |

State flow: `attend` / `memory record` / `reflect record` write SQLite (canonical, live) → `wiki sync` publishes the minimal projection (capped leads, not proof).

Next agent's `attend` / `query` / `memory recall` reads live SQLite first; open a file here only when SQLite is unavailable or `attend.next` routes there.

Next: for publish/share mechanics load `references/repo-context-management.md`; for what each reflect label writes load `references/learning-loop.md`.
