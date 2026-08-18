# GitHub Landscape

Rare path: load for repo discovery, reuse decisions, open-source implementation search, or ecosystem comparison. Skip for one known repo or a narrow bug. `references/algorithm.md` owns proof; `references/octocode.md` owns syntax.

## Flow
1. Frame literal, alias, adjacent implementation, and package terms.
2. Discover repos/packages; turn web/product/paper names into repo/package leads.
3. Record a compact table or approved `repo_db.jsonl` artifact.
4. Rank cheaply; deep-read only the top 3-8 within budget.
5. For each finalist, inspect tree, README, exact source/test anchors, issues/PRs, history, and license. Release history needs `ghListReleases` with `ENABLE_RELEASES=1`; without it use commit/tag history and mark releases skipped.
6. Upgrade claims from snippets/README to exact evidence; use `long-research.md` for contested/large runs.
7. Return clusters, ranking, integration blueprint, and proof still needed.

```json
{"repo":"owner/name","url":"https://github.com/owner/name","sourceQuery":"term","stars":1200,"language":"TypeScript","license":"MIT","lastActivity":"2026-06-01","package":"name","fit":"high","activity":"active","evidenceIds":["ev1"],"notes":"exact capability"}
```

## Ranking
| Dimension | High | Medium | Low |
|---|---|---|---|
| fit | direct capability | adjacent/partial | keyword only |
| evidence | exact code/docs/tests | README/example | snippet |
| activity | recent release/commits/issues | some movement | stale/archive |
| reuse | clear API/license, low drag | adaptation needed | hidden service/unclear license/rewrite |
| risk | bounded caveats | unknowns | blocker |

Stars/downloads are tiebreakers, never validation.

## Output
`repo clusters → ranked table → finalist proof → reusable pieces → integration path → incompatibilities/license/dependencies → remaining proof → next prototype`

Stop after finalist proof converges, retries stay thin, license/service risk needs user choice, or deeper work requires unapproved clone/execution.
Ask before cloning many repos, running untrusted code, or writing artifacts.

Next: when a finalist needs exact in-repo proof load `references/workflow-external.md`; when the winner must wire into local code load `references/workflow-combination.md`; when the ranking must survive as a durable brief load `references/long-research.md`.

