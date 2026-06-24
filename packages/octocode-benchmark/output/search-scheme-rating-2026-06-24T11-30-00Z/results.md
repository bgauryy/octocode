# Octocode Search Scheme Rating

Created: 2026-06-24T11:30:00Z

Inputs:
- `/Users/guybary/Documents/octocode-mcp/packages/octocode-benchmark/output/search-only-2026-06-24T11-12-03-076Z/results.md`
- `/Users/guybary/Documents/octocode-mcp/packages/octocode-benchmark/output/search-flow-coverage-2026-06-24T11-21-34-295Z/results.md`
- `node packages/octocode/out/octocode.js search --scheme --compact --no-color`
- Subagent reviews: returned-data/all-tool coverage, pagination/minification, OQL naming.

## Scores

| Dimension | Rating | Evidence |
|---|---:|---|
| Returned data quality | 8.3/10 | Unified envelope has `results`, `diagnostics`, `provenance`, `evidence`, `proofGrade`, and runnable `next.*`. Weakness: proof slices can still have `answerReady:false`, which is correct but agent-confusing. |
| All tools through `search` | 8.2/10 | Benchmarks covered local code/content/files/structure/semantics/diff/artifacts/research, plus external packages/repos/code/structure/PRs/commits/diff/materialize. Niche PR/artifact pagination modes need deeper rows. |
| Pagination controls | 7.3/10 | `next.page`, `next.matchPage`, char windows, LSP pages, artifact continuations, and PR/commit pages work. Naming is fragmented: `limit`, `itemsPerPage`, `page`, `filePage`, `commentPage`, `commitPage`, `entryPageNumber`, `scanOffset`, `charOffset`. |
| Minification/content controls | 7.3/10 | Functional across local, GitHub, and materialized code. Naming drifts: CLI `--mode none|standard|symbols`, OQL `fetch.content.contentView exact|compact|symbols`, backing tools `minify none|standard|symbols`, PR params `minify`. |
| GitHub/local parity | 7.3/10 | Structure/content/minify/materialized-local proof work. GitHub `target:"files"` field predicates are not provider-native; exact file-row behavior requires materialization. |
| Search scheme quality | 7.0/10 | Powerful, typed, strict, evidence-aware, and explainable. Still too many public synonyms for agents. |
| OQL as an agent language | 7.8/10 | Core grammar is good: `target + from + scope + where + fetch + controls + materialize + next + evidence`. Needs a smaller canonical vocabulary. |

Overall rating: 7.7/10.

## Verdict

`octocode search` is viable as the unified research surface. The benchmark evidence supports replacing the old read/search/browse/history/package command families with `search` plus OQL. The remaining problem is not capability; it is public-language cleanup.

OQL is a good agent language at the core. It becomes confusing when CLI sugar, old backing-tool names, and canonical OQL names are all visible at once.

## Alignment Plan

Canonical OQL should keep one preferred spelling per concept:

| Concept | Canonical OQL | CLI sugar / aliases | Adapter responsibility |
|---|---|---|---|
| Result shape | `view: "discovery" | "paginated" | "detailed"` | `--view`; old `--mode discovery|paginated|detailed` only as sugar | Normalize to `query.view`; adapters read only canonical `view`. |
| Content form | `fetch.content.contentView: "exact" | "compact" | "symbols"` | Prefer `--content-view`; keep `--mode none|standard|symbols` as compatibility sugar where needed | Normalize `none -> exact`, `standard -> compact`, `symbols -> symbols`; map to backing `minify`. |
| Result pagination | `page` + `itemsPerPage` | `--page`, `--items-per-page`; `--page-size` and `--limit` as sugar | Normalize aliases before adapters; adapters collapse backend `perPage/filesPerPage/entriesPerPage` to OQL. |
| Match pagination | `controls.search.matchPage` + `controls.search.maxMatchesPerFile` | `--match-page`, `--max-matches` | Keep separate from result pagination; make diagnostics say this is per-file paging. |
| Content window | `fetch.content.charOffset` + `fetch.content.charLength` | `--char-offset`, `--char-length`, `--page-size` only for content reads if documented | Emit `next.charRange`; document line-boundary expansion. |
| Search predicate | `where` | text positional, `--regex`, `--pattern`, `--rule` | Normalize shorthand to predicates; do not overload fetch matching. |
| Content anchor | `fetch.content.match` | `--match-string`, `--match-regex`, `--match-case-sensitive` | Use only for reads/PR content narrowing. |
| Provider search field | `params.providerMatch` or `params.searchIn` | Current `--match path|file` | Avoid collision with content `match`; map provider-only values in GitHub adapter. |
| Source | `from` | `--repo`, `--owner`, positional owner/repo/path | Normalize source sugar; adapters consume `from` only. |
| Scope path | `scope.path` | positional path, `--path` | Keep separate from field predicate `field:"path"`. |
| Language filter | `scope.language` | `--lang`; `--ext` only for file extension filters | Structural `lang` should become sugar for `where.language` or `scope.language` plus structural engine language. |

## Immediate Fixes

1. Add `--content-view exact|compact|symbols` and document it as the preferred content/minify control. Keep `--mode none|standard|symbols` as shorthand sugar until docs migrate.
2. Change `search --scheme` wording to say canonical OQL uses `contentView`, while CLI `--mode` and raw `minify` are aliases.
3. Add a `next.materialize` repair continuation to GitHub `target:"files"`/field predicate diagnostics where materialization is the correct next step.
4. Prefer `itemsPerPage` in OQL examples; demote `limit` to CLI/provider sugar unless the target truly has a total cap.
5. Make continuation keys and package/source handoffs consistently OQL-shaped. Avoid old `data.next.{tool,query}` envelopes.
6. Clarify evidence semantics for exact slices: proof of the slice can coexist with `answerReady:false` when more slices/pages remain.

## Recommendation

Do not rename the canonical schema in one big breaking sweep. Instead:

1. Add aliases in the normalizer/shorthand boundary.
2. Update `search --scheme` and CLI help to teach the canonical names.
3. Keep local/GitHub/artifact/LSP adapters canonical-only.
4. Add tests that assert aliases normalize to one canonical OQL object.
5. After docs and tests are green, remove old public wording for removed commands.

