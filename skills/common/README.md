# Shared skill assets

Private workspace `@octocodeai/skills-common` owns shared tool usage, configuration, evidence and protocol guidance, report code, and HTML templates. Edit sources here; build copies into skill folders so a skill installed alone needs neither this package nor sibling skills.

From the repository root:

```bash
node skills/common/build.mjs
node skills/common/build.mjs --check
yarn workspace @octocodeai/skills-common test
```

Repository builds run this workspace. `skills:check` checks generated copies without repairing them, so drift fails CI.

The build scans `skills`, `skills-beta`, and `skills-dev` for `SKILL.md`. A skill that references or already ships a bundle's entry file receives that bundle. New report consumers link `scripts/render-report.mjs` from their lobby and `references/report-output.md` from their output guide.

Shared Markdown under `references/` is discovered automatically. Link a shared page from any skill Markdown file using its installed relative path. The build follows that page's shared dependencies from canonical content, so a fresh standalone install includes the entire reference chain. These are on-demand references, not additions to skill descriptions or automatically loaded instructions.

| Source | Installed copy | Consumer |
|---|---|---|
| `scripts/render-report.mjs` | `scripts/render-report.mjs` | Markdown report viewer |
| `scripts/lib/report/*.mjs` | Same paths | Report viewer modules (Markdown engine, page views, collection, checks), shipped with the viewer |
| `references/report-output.md` | `references/report-output.md` | Shared report instructions |
| `references/octocode-usage.md` | Same path | Research MCP/CLI selection, schema discovery, paging, errors |
| `references/configuration.md` | Same path | Config lookup, credentials, ownership |
| `references/evidence.md` | Same path | Source vs runtime proof, absence, verification |
| `references/protocol-catalog.md` | Same path | Common task, finding, handoff, and reporting protocols |
| `assets/architecture.html` | `assets/template.html` | Interactive architecture graph |
| `scripts/lib/net-guard.mjs` | `scripts/lib/net-guard.mjs` | Public-network request validation |
| `scripts/lib/entrypoint.mjs` | `scripts/lib/entrypoint.mjs` | Symlink-safe script entry points |
| `scripts/lib/fail.mjs` | `scripts/lib/fail.mjs` | Script JSON errors and exit status |

The graph viewer consumes scan/model JSON; the report viewer consumes Markdown. They have distinct interaction models and each has one canonical template. Skill-specific status values, evidence requirements, and report sections remain in each skill's `output.md`.

Keep specialized methods with their owners: query recipes and classification decisions in Research; browser actions in Chrome DevTools; identity and delivery in agents-communication; sealed comparisons in eval-benchmark; workflow roles and ledgers in Workflows. Similar words do not imply the same contract.

The report converter accepts Mermaid flows plus validated `graph` and `chart` JSON fences. It supplies shared controls, data tables, errors, export and themes; authors provide content only. Graphs load Cytoscape.js and charts load ECharts only when present. See [the input contract](references/report-widgets.md).

Report requirements are explicit `report-contract` records in each owning `output.md`; the converter does not infer rules from English phrasing. `--strict` rejects validation warnings before writing, while JSON results distinguish generated HTML from browser verification. Discovery defaults to compact pages with continuations; `--all` keeps full catalogs available for file processing. Frontmatter stays in the catalog, outside the reader body.
