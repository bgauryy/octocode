# Quality Signals

Load when ranking candidates beyond GitHub stars. Why: installs/recency/audits are stronger battle-test signals. Pairs with `quality-rubric.md` (content) and `discovery-surfaces.md` (where signals live).

- **Install count** — `skills.sh/api/search?q=` (curl in `search-playbook.md`); sort by `installs`. High installs + modest stars usually beats the reverse.
- **Per-skill page** — `skills.sh/<owner>/<repo>/<skill>` — installs, install cmd, audit badge, siblings.
- **Leaderboard** — `skills.sh` ranks by installs across agents.
- **Recency** — GitHub `pushed:>YYYY-MM-DD`; skip >12 months stale unless archival wanted.
- **Audit badges** — skills.sh Gen Agent Trust Hub; Microsoft Sensei (triggers + anti-triggers + compatibility).
- **Registry fields** — aiskillstore: `match_reasons`, `downloads_7d`, `days_since_update`, `/similar`.
- **Capability overlap** — `aiskillstore.io/v1/agent/skills/{id}/similar`.
- **Demand** — `aiskillstore.io/v1/demand/most-wanted` — zero-result searches; adapt vs create.

Installs are a tiebreaker after content fit — never blind-recommend the top install.

Next: when presenting load `references/output-format.md`; if a surface 404s load `references/recovery.md`; for CLI installers load `references/discovery-manifests.md`.
