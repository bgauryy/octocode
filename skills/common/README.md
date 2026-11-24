# Shared skill assets

Private workspace `@octocodeai/skills-common` owns shared tool usage, configuration, evidence and protocol guidance, report code, and HTML templates. Edit sources here; build copies into skill folders so a skill installed alone needs neither this package nor sibling skills.

From the repository root:

```bash
node skills/common/build.mjs
node skills/common/build.mjs --check
yarn workspace @octocodeai/skills-common test
```

Repository builds run this workspace. `node skills-dev/octocode-dev/scripts/dev.mjs skills:check` checks generated copies without repairing them, so drift fails CI.

| Build decision | Rule |
|---|---|
| Scope | Scan `COLLECTIONS`: `skills`, `skills-dev`, for `SKILL.md`. Package skills (`packages/*/skills`, `packages/*/src/skills`) bundle no shared files and are excluded. |
| Reachability | Start at `SKILL.md`; follow Markdown links, relative imports and named whole paths through authored and canonical shared pages. Copy a bundle only when its entry is reached. |
| Removal | Delete an unreached copy only when byte-identical to its canonical source. Never touch authored content at a shared path. |
| Contract | `routeGraph(dir)` exposes routes; tests reject unreached copies and missing routed files. |
| New report consumer | Link `scripts/render-report.mjs` from the entry and `references/report-output.md` from output. Renderer bundles carry no guidance pages implicitly. |

Shared Markdown under `references/` is discovered automatically. Link a shared page from any skill Markdown file using its installed relative path. The build follows that page's shared dependencies from canonical content, so a fresh standalone install includes the entire reference chain. These are on-demand references, not additions to skill descriptions or automatically loaded instructions.

| Source | Installed copy | Consumer |
|---|---|---|
| `scripts/render-report.mjs` | `scripts/render-report.mjs` | Markdown report viewer |
| `scripts/lib/report/*.mjs` | Same paths | Report viewer modules (Markdown engine, page views, collection, checks), shipped with the viewer |
| `references/report-output.md` | `references/report-output.md` | Shared report instructions |
| `references/configuration.md` | Same path | Config lookup, credentials, ownership |
| `references/evidence.md` | Same path | Source vs runtime proof, absence, verification |
| `references/mental-models.md` | Same path | Select thinking lenses and turn assumptions into distinguishing checks |
| `references/writing-tells.md` | Same path | AI-writing tells, chat residue and placeholders: judge density in added text, repair the claim |
| `references/protocol-catalog.md` | Same path | Common task, finding, handoff, and reporting protocols |
| `references/presentation.md` | Same path | Visual-first output, HTML offer |
| `references/report-mermaid.md` | Same path | Diagram-family chooser |
| `references/report-widgets.md` | Same path | `graph`/`chart` JSON contract |
| `assets/architecture.html` | `assets/template.html` | Interactive architecture graph |
| `scripts/residue-scan.mjs` | Same path, with `scripts/lib/entrypoint.mjs` and `scripts/lib/fail.mjs` | Writing-tell residue scan (AI tool markers, tracking parameters, placeholders): JSON findings, exit 1 on any; reached from `references/writing-tells.md` |
| `scripts/lib/net-guard.mjs` | `scripts/lib/net-guard.mjs` | Public-network request validation |
| `scripts/lib/entrypoint.mjs` | `scripts/lib/entrypoint.mjs` | Symlink-safe script entry points |
| `scripts/lib/fail.mjs` | `scripts/lib/fail.mjs` | Script JSON errors and exit status |

The graph viewer consumes scan/model JSON; the report viewer consumes Markdown. They have distinct interaction models and each has one canonical template. Skill-specific status values, evidence requirements, and report sections remain in each skill's `output.md`.

A skill whose scripts read API keys is its own private workspace: `skills/<skill>/package.json` depends on `@octocodeai/config` (`workspace:*`), lists what ships in `files`, and builds `src/*.mjs` into `scripts/` with [bundle-skill.mjs](bundle-skill.mjs) (`yarn build`; `yarn lint` fails on a stale bundle). Its `scripts/keys.mjs` carries `getKey` and the `list`/`get`/`set` CLI, so the installed skill needs no packages. `dev skills:check` runs each skill workspace's lint.

Keep specialized methods with their owners: query recipes and classification decisions in Research; browser actions in Chrome DevTools; agent identity, delivery and sealed comparisons in Agentic; workflow roles and ledgers in Workflows. Similar words do not imply the same contract.

The report converter accepts Mermaid flows plus validated `graph` and `chart` JSON fences. It supplies shared controls, data tables, errors, export and themes; authors provide content only. Graphs load Cytoscape.js and charts load ECharts only when present. See [the input contract](references/report-widgets.md).

Report requirements are explicit `report-contract` records in each owning `output.md`; the converter does not infer rules from English phrasing. `--check` validates a saved report against its owner's contract, including section order, and writes no HTML; `--strict` rejects the same warnings before writing, while JSON results distinguish generated HTML from browser verification. Discovery defaults to compact pages with continuations; `--all` keeps full catalogs available for file processing. Frontmatter stays in the catalog, outside the reader body.
