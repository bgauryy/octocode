# Output

Deliver task content only. Never append probe output or generated process metadata.

Use Markdown for reviews and change summaries. `skill-review.mjs --json` returns `errorCount`, `warnCount`, and per-skill `results`; these are structural findings, not quality scores. `skill-sync.mjs --json` returns a destination plan or application results.

## Response

For review: readiness verdict, actionable findings with file anchors and impact, and any unchecked area. Distinguish structural lint, editorial judgment, and measured activation. For edits: changed skills, support-file changes, and checks run. For install: destination and reload need.

## Saved result

Keep authored skills in their own folders. Save a separate review only when requested.
