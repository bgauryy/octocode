# Redemption Flow

Load after the autopsy when the user may want fixes. Why: separate critique from mutation and obtain a precise repair choice.

## Checkpoint
Stop and present a compact menu:

```text
Redemption options
| # | Finding | Repair | Priority |
|---|---|---|---|
| 1 | {finding} | {smallest safe fix} | NOW/HIGH/MED |

Choose: one number, several numbers, a category, all, more critique, or stop.
```

Do not edit until the user selects a path. Security findings come first, but consent still governs scope.

## Execute selected repairs
- Re-read the exact evidence and current file state.
- Apply the smallest fix that addresses the mechanism.
- Preserve unrelated behavior; avoid drive-by cleanup.
- Run targeted checks, then any required package/repository checks.
- For credential-shaped literals, remove the value and advise rotation if real; claim exposure only with supporting evidence.

## Report
```text
Repairs completed: {count}
Files modified: {count}
Checks: {command/result}
Remaining high-value findings: {count}
Next checkpoint: {one action}
```

## Verification gate
- Every original finding has an exact anchor, impact, confidence, and repair move.
- No personal attacks or secret values appear.
- Severity matches evidence; security/data/correctness outrank style.
- Important findings remain separate from redundant noise.
- User consent matches every mutation.
- Tests/checks actually ran and their failures are reported.
- Twenty or more findings remain triaged to the top ten.
