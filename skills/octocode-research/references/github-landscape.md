# GitHub Landscape

Read this when the research question asks "what repos exist?", "which implementation should we reuse?", "what open-source options implement X?", or when a prior-art map needs deeper GitHub comparison.

This mode is a structured repo ecosystem pass: discover broadly, rank cheaply, deep-dive selectively, then produce an integration blueprint. It is optional; do not use it for a single known repo or a narrow code bug.

## Output Goal

End with:

- repo clusters: active, partial, abandoned, research-only, production-ready, or not relevant
- a ranked repo database
- deep-dive notes for the top few repos
- an integration blueprint: what to reuse, what to avoid, and what proof remains

## Repo DB

Keep a compact table in chat for small runs, or a `repo_db.jsonl` artifact for long runs.

Recommended fields:

```json
{"repo":"owner/name","url":"https://github.com/owner/name","sourceQuery":"structural TypeScript search","stars":1200,"language":"TypeScript","license":"MIT","lastActivity":"2026-06-01","package":"name-or-null","fit":"high","activity":"active","evidenceIds":["ev1","ev2"],"notes":"Has tree-sitter parser and CLI docs"}
```

Useful scoring dimensions:

- `fit`: does the repo actually address the research question?
- `activity`: recent commits/releases/issues, not stars alone.
- `implementationEvidence`: source code, examples, tests, docs, or packages.
- `reusePotential`: API shape, license, dependency drag, maturity.
- `risk`: abandoned, unclear license, narrow language support, hidden service dependency, or unverifiable claims.

## Flow

1. Frame search terms: literal phrase, aliases, adjacent implementation terms, and package names.
2. Discover repos:
   ```text
   search <keywords> --target repositories --json
   search <topic> --target packages --json
   search <term> <owner/repo> --view discovery --json
   ```
3. Add web/product/paper names only as leads, then resolve each lead to GitHub/npm.
4. Build the repo DB from search results and exact reads.
5. Rank cheaply and select top repos for deep dive. Default: top 3-8, depending on budget.
6. Deep dive each selected repo:
   ```text
   search <owner/repo> --tree --depth 2 --json
   search <owner/repo/README.md> --content-view exact --json
   search <term> <owner/repo> --view discovery --json
   search <owner/repo/path> --match-string <anchor> --content-view exact --json
   search <owner/repo/path> --target commits --since <iso> --json when history matters
   ```
7. Upgrade claims with exact source evidence. Use `long-research.md` ledgers for large or contested landscapes.
8. Produce clusters, ranking, and integration blueprint.

## Ranking Rubric

Use `high`, `medium`, or `low` for each dimension:

| Dimension | High | Medium | Low |
|---|---|---|---|
| Fit | Directly implements the target capability | Adjacent or partial | Keyword-only |
| Evidence | Exact code/docs/tests prove the claim | README or examples only | Search snippet only |
| Activity | Recent release/commits and responsive issues | Some recent movement | Stale or archived |
| Reuse | Clear API, acceptable license, small dependency drag | Some adaptation needed | Hidden service, unclear license, or heavy rewrite |
| Risk | Known caveats are bounded | Some unknowns | Material blocker |

Downloads and stars are tiebreakers, not validation.

## Integration Blueprint

```text
Recommended repo(s)
Why these survive
Reusable pieces
Integration path
Known incompatibilities
License/dependency risks
Proof still needed
Next command or prototype
```

Tie every recommendation to evidence. If no repo is good enough, say so and identify the hardest missing capability.

## Stop Gates

Stop when one of these fires:

- top candidates are proven with exact evidence and next repo is unlikely to change the decision
- search surfaces are thin after synonym/package/web-lead retries
- license or service dependency risk makes reuse a user decision
- deep dive would require cloning or running untrusted code without approval

Ask before cloning many repos, running repo code, or writing artifacts outside the current skill/report scope.
