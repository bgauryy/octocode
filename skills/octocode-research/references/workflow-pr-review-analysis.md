# PR review analysis

Load when `references/workflow-pr-review.md` has resolved target, guidelines, context, and checkpoint scope. Why: sizing, flow proof, and finding shape decide what the review can claim.

## Size and domains
| Mode | Trigger | Depth |
|---|---|---|
| Quick | ≤5 files and all LOW risk | surface scan → verify → finalize |
| Full | >5 files, any HIGH/MED risk, or requested | every step below |

Default Full when uncertain. Review in impact order: Security → Bug/Correctness → Flow → Architecture → Performance → Error Handling → Quality/Duplicates. Exclude linter-only style, unchanged/generated/vendor code, speculative hypotheticals, and resolved comments.

Severity is impact (`HIGH/MED/LOW`); confidence is proof (`confirmed/likely/uncertain`). Include actionable changed-code findings supported as confirmed/likely.

## Analysis
1. Derive 3-5 focus queries; check every changed file against loaded guidelines.
2. Match changed public/high-risk symbols to proof:
   - signature → incoming callers; new function → callees; type/interface → references;
   - transformation → trace each boundary; removed export → graph dependents/path + LSP references;
   - import/module reshape → graph cycles/reachability before and after when topology risk is material.
3. Exact-read every affected consumer before calling it broken; record changed values/types/side effects.
4. Check APIs/schemas/dependencies, edge cases, auth/injection/data exposure, error context, hot paths, and TODO/FIXME on added lines.
5. Run the smallest applicable project test/typecheck/lint check. If unavailable/unsafe, record `not run`, and keep recommendation below `APPROVE`.
6. For high-risk changes, require rollout/rollback or a reason they are unnecessary.

Do not inspect excluded areas. Empty results require one changed query/surface. Dead-end flow proof stays an explicit limitation.

## Optional parallel split
Use only when the runtime/user permits and the review is Full. Split independent Flow, Security/Error, Architecture/Quality, and loaded-guideline checks.
Workers return scope, findings, checked non-findings, and limits; they never write. Merge by root cause/location, keep stronger proof, and cap the top 5-7.

## Finalize
- Reconcile existing comments; refine MED/LOW-confidence candidates with one targeted proof and delete disproven items.
- Guidelines win according to recorded precedence; document conflicts.
- Keep the top 5-7 by severity/domain, with lower signal in Additional Notes.

```text
[DOMAIN-1] title
Severity: HIGH|MED|LOW
Confidence: confirmed|likely|uncertain
Location: path:line
Evidence: exact read + proof lane
Impact: caller/user/data/contract consequence
Fix: minimal direction or diff
```

Avoid `#1` finding labels because GitHub auto-links them.

Next: write the verdict with `references/workflow-pr-review-report.md`; when a finding needs stronger proof first load `references/code-research.md`; when the diff authorizes a fix hand off to `references/workflow-change.md`.
