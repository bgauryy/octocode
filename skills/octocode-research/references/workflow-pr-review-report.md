# PR Review Report

Use after `workflow-pr-review-analysis.md` has produced deduped findings and recommendation.

## Report

Present a chat summary before writing any file:

```markdown
| Field | Value |
|---|---|
| Recommendation | REQUEST_CHANGES |
| Risk Level | High: auth token handling changed |

## High
1. [SEC-1] Token logged on retry
Severity: HIGH; Confidence: confirmed; Location: src/auth.ts:42
Evidence: retry logger reads raw token.
Impact: token can leak to logs.
Fix: redact token before logging.
```

The summary includes recommendation, risk level, findings grouped High/Medium/Low, `file:line`, and guidelines status.
Ask before creating a document: "Would you like me to create the detailed review document?"
Only write a file after the user says yes.

If approved, write the document using the sections below.
For PR Mode, write `.octocode/reviewPR/<session-name>/PR_<prNumber>.md`.
For Local Mode, write `.octocode/reviewLocal/<session-name>/REVIEW_<branch>_<timestamp>.md`.
Use a short `<session-name>` slug, such as `auth-refactor`.
If writing fails, output the document content in chat.
Never write the file before approval.

## Document Sections

- **Executive Summary** table:
  PR Goal/scope, Files Changed, Risk Level, Review Mode, Review Effort, Recommendation.
- **Narrative bullets**:
  Affected Areas, Business Impact, and Flow Changes.
- **Ratings** table:
  Correctness, Security, Performance, Maintainability; each uses `X/5`.
- **PR Health**:
  clear description, ticket/issue if applicable, size, and relevant tests.
- **Changes Health**:
  cohesive concern, size/split recommendation, and relevant tests.
- **Guidelines Compliance**:
  Source | Rule | Status, only when guidelines were loaded.
- **Issues by priority**:
  High, Medium, Low; number sequentially across all three.
- **Flow Impact Analysis**:
  affected callers/consumers list, or before/after diagram.
- **Suggested Next Steps**:
  Local Mode only: tests, fixes, split commits, or ready to commit.

Tone: professional, constructive, and about the code rather than the author.
Explain reasoning and distinguish requirements from preferences.
Use full GitHub blob URLs for PR Mode code references.
Use `file:line` for Local Mode.
Never give timing or duration estimates.

## Verification Checklist

Before delivering the review, confirm:

- [ ] Target/mode resolved, including file-scoped local checks.
- [ ] Availability Gate passed for the resolved mode.
- [ ] Guidelines Gate ran, even when the answer was "skip".
- [ ] Diff/PR context collected for the active mode.
- [ ] User Checkpoint presented, unless the change was tiny and LOW risk.
- [ ] Flow impact analyzed for every modified function/method in Full mode.
- [ ] User focus areas covered and excluded areas skipped.
- [ ] Findings deduped against PR comments and each other.
- [ ] Every finding has `file:line`, severity, confidence, evidence, and a fix.
- [ ] Guidelines compliance checked and reported when guidelines were loaded.
- [ ] No `#<number>` notation used anywhere in the output.
- [ ] Chat summary presented and approval requested before writing a document.
- [ ] Written documents number issues sequentially across High/Medium/Low.

Validate: `node scripts/eval-research.mjs --case pr-local-review`.
