# Quality Signals Beyond Stars

Load when ranking candidates and you want evidence stronger than raw GitHub stars. Pairs with `quality-rubric.md` (content judgement) and `discovery-surfaces.md` (where these signals live).

- **Install count via search API** — query `https://www.skills.sh/api/search?q=<topic>&limit=100` (full curl in `search-playbook.md`), sort by `installs` descending. High installs with modest stars is usually a stronger battle-tested signal than the reverse (e.g. `find-skills`, the Lark/Feishu suite).
- **Per-skill index page** — `https://www.skills.sh/<owner>/<repo>/<skill-name>` (or `.../<org>/skills/<skill-name>` when the repo is named `skills`) shows install count, install command, audit badge, and related skills.
- **Leaderboard** — `https://www.skills.sh` ranks by install count across agents; good for spotting a dominant skill in a domain without knowing names.
- **Recency** — `pushed:>YYYY-MM-DD` on GitHub. Skip skills with no commits in the last 12 months unless the user wants archival.
- **Audit badges** — skills.sh exposes Gen Agent Trust Hub pass/fail; Microsoft uses the Sensei rubric (triggers + anti-triggers + compatibility scored Low/Medium/High).
- **Registry-side fields** — `aiskillstore.io` exposes `match_reasons`, `downloads_7d`, `days_since_update`, and a `/similar` endpoint for overlap-ranked alternatives.
- **Capability overlap** — `aiskillstore.io/v1/agent/skills/{id}/similar` ranks alternatives by tag/category overlap.
- **Demand signal** — `aiskillstore.io/v1/demand/most-wanted` shows searches that returned nothing; useful when deciding whether to adapt an existing skill or create a new one.
