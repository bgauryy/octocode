# Redemption Flow

Load after the autopsy when fixes are wanted; it keeps critique separate from mutation.

## Checkpoint

For a critique-only request, describe useful repair options without applying them. An optional menu can help the user choose:

```text
Redemption options
| # | Finding | Repair | Priority |
|---|---|---|---|
| 1 | {finding} | {smallest safe fix} | NOW/HIGH/MED |

Choose: one number, several numbers, a category, all, more critique, or stop.
```

If the original request already authorized fixes, map them to the findings and proceed within that scope. Otherwise wait for a selection.

## Execute selected repairs

- Re-read the exact evidence and current file state.
- Apply the smallest fix that addresses the mechanism.
- Preserve unrelated behavior; avoid drive-by cleanup.
- Run targeted checks, then any required package/repository checks; report failures.
- For credential-shaped literals, remove the value, and advise rotation if real; claim exposure only with supporting evidence.

## Report

Example summary; omit counts or fields that add no value:

```text
Repairs completed: {count}
Files modified: {count}
Checks: {command/result}
Remaining high-value findings: {count}
Next checkpoint: {one action}
```

Next: the roast ends here — report the checks and the remaining findings; when you ask for another pass, restart at `references/roast-playbook.md` § 1 Target.
