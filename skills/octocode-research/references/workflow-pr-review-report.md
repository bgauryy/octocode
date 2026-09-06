# PR review report

Load after analysis produces deduped findings, verification receipts, and a recommendation.

## Chat Report
```markdown
| Field | Value |
|---|---|
| Recommendation | APPROVE / REQUEST_CHANGES / COMMENT |
| Risk | High/Medium/Low: <reason> |
| Verification | <check: passed/failed/not run/not applicable> |

## High / Medium / Low
1. [SEC-1] <title>
   Severity: HIGH; Confidence: confirmed; Location: src/auth.ts:42
   Evidence: <exact proof>; Impact: <consequence>; Fix: <minimal repair>
```

`APPROVE` requires all applicable tests or verification to pass. Use `REQUEST_CHANGES` for a blocker/failing check and `COMMENT` when no blocker is proven but verification is incomplete.

Report recommendation, risk, verification, guidelines status, and findings ordered by severity. Use full blob URLs for remote PR code and `file:line` locally; distinguish requirements from preferences.

## Optional Document
Write when the user requested a saved report or already authorized it; otherwise keep the review in chat. Paths:
- PR: `.octocode/reviewPR/<session>/PR_<number>.md`
- Local: `.octocode/reviewLocal/<session>/REVIEW_<branch>_<timestamp>.md`

If writing fails, return the document in chat. Sections: executive summary; affected areas/business/flow; 1-5 ratings for correctness/security/performance/maintainability; PR/change health; guidelines compliance; findings; flow impact; next steps. Never provide duration estimates.

## Verification Checklist
- [ ] Target/mode, availability, guidelines, diff/PR context, risk sizing, and checkpoint were handled. <!-- style-lint: ignore-line passive-voice -->
- [ ] Full mode traced every modified function/method and covered requested focus only.
- [ ] Findings were deduped; each has location, severity, confidence, evidence, impact, and fix. <!-- style-lint: ignore-line passive-voice -->
- [ ] Every check has an explicit status; APPROVE appears only after applicable checks pass.
- [ ] Chat report came before any approved document; written issue numbering is sequential.

Next: when a finding is disproven or a box stays unchecked return to `references/workflow-pr-review-analysis.md`; when the review authorizes a fix hand off to `references/workflow-change.md`. Otherwise the review ends here — deliver the chat report.
