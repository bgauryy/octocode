# Review Domain Checklist

Use this for PR review, local diff review, and safe-to-merge checks. Project/user guidelines override this default checklist.

## Domain priority matrix

| Domain | Look for | High priority | Medium priority | Usually skip |
|---|---|---|---|---|
| Bug / correctness | crashes, data corruption, null access, race, API misuse | hot-path crash, data loss, broken contract | edge-case runtime error | compiler/linter-only issue |
| Security | injection, XSS, auth bypass, secrets, data exposure | exploitable path, credential leak, privacy breach | insufficient validation/logging risk | speculative issue with no path |
| Flow impact | changed callers, changed return/type/data semantics | caller breakage, data-flow change, incompatible behavior | dependent code needs update | internal refactor with same behavior |
| Architecture | coupling, wrong layer, circular dependency, leaky abstraction | public API break, core cycle, domain imports infrastructure | pattern deviation increasing debt | framework-standard pattern |
| Performance | O(n²), blocking ops, unbatched I/O, leaks, missing cache | large-data O(n²), event-loop block, memory leak | frequent-path inefficiency | micro-optimization |
| Error handling | swallowed exceptions, missing context, unclear errors | hidden critical failure | poor diagnostics | trusted internal path with surrounding handling |
| Code quality | naming, duplicate logic, magic values, TODO in new code | public typo/API naming bug | internal DRY/convention issue | pure formatting |
| Guidelines | project-specific rules | explicit guideline violation | ambiguous convention mismatch | personal style |

Priority order for final report: Security > Bug > Flow Impact > Architecture > Performance > Error Handling > Code Quality > Guidelines/Duplicates.

## Global exclusions

Do not report:
- unchanged code except direct blast-radius impact;
- generated/vendor files;
- test implementation style unless tests are wrong or missing coverage for changed contract;
- issues already raised in existing PR comments unless unresolved;
- compiler/linter/type errors without added reasoning;
- speculative “what if” findings without a reachable path.

## Verification checklist

Before delivering review:

- [ ] Target mode resolved: PR / Local changes / History / File scope.
- [ ] Guidelines/context files checked and applied.
- [ ] Existing PR comments fetched and deduped (PR mode).
- [ ] Changed files grouped by functional area and risk.
- [ ] High-risk patches or local changed function bodies read with exact line evidence.
- [ ] Flow impact analyzed for modified public/high-risk functions.
- [ ] AST/search checks run for relevant domains.
- [ ] All findings cite exact `file:line`.
- [ ] All findings have severity, confidence, impact, and actionable fix.
- [ ] Findings capped to ~5–7 key issues.
- [ ] No `#1`/`#2` labels.
- [ ] User approved before writing any review file.
