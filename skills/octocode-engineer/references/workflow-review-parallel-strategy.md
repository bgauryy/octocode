# Review Parallel Strategy

Use only for large PRs/local diffs. Do not spawn review subagents for small reviews; the coordination cost is higher than the value.

## When to parallelize

| Changed files | Mode | Suggested split |
|---|---|---|
| ≤5 | Quick | Single pass |
| 6–15 | Full | Flow + Architecture/Quality |
| 16–30 | Full | Flow + Security/Error + Architecture/Quality |
| 30+ or guidelines loaded | Full | Flow + Security/Error + Architecture/Quality + Guidelines/Duplicates |

Rules:
- Agents must be read-only and evidence-only.
- Agents must not write final output or modify files.
- Merge findings by root cause; keep highest-confidence duplicate.
- Cap final report to ~5–7 findings.

## Agent lanes

### A. Flow Impact

Tools: `npx octocode search <symbol> <path> --view discovery` → `npx octocode search <file> --op callers|references --symbol <name> --line <lineHint>`; use raw `lspGetSemantics(groupByFile:true)` only when file grouping is needed.

Task:
- For every modified public/high-risk function/type, find lineHint.
- Map callers/references.
- Mark breaking change yes/no.

Output:
```text
symbol | file:line | callers/references | impact | confidence
```

### B. Security + Error Handling

Tools: `npx octocode search --pattern/--rule --lang <language>` or text search → `npx octocode search <file> --match-string <anchor> --content-view exact`.

Task:
- Injection/XSS/auth bypass/secrets/data exposure.
- Swallowed errors, missing log context, unclear error propagation.
- Validation on changed endpoints/inputs.

Output:
```text
issue | file:line | severity | reachable path | fix
```

### C. Architecture + Quality + Performance

Tools: `npx octocode search <path> --tree` → AST/search → `npx octocode search <file> --op references|callees --symbol <name> --line <lineHint>` → exact reads.

Task:
- Wrong layer/module, coupling, cycles, leaky abstractions.
- Duplicates, naming, TODO in new code.
- O(n²), blocking operations, unbatched I/O, memory/lifecycle leaks.

Output:
```text
issue | domain | file:line | architectural impact | fix
```

### D. Guidelines + Duplicates

Tools: guidelines/context reads → `npx octocode search` / AST for existing utilities and patterns.

Task:
- Apply loaded user/project guidelines.
- Search for existing utilities/patterns the change should reuse.
- Flag duplicate logic or convention drift.

Output:
```text
guideline | pass/fail | file:line | duplicate candidate | fix
```

## Merge protocol

1. Collect all lane outputs.
2. Remove duplicates by root cause and file line.
3. Cross-check against existing PR comments.
4. Sort: Security > Bug > Flow > Architecture > Performance > Error Handling > Quality.
5. Drop low-confidence findings or mark as uncertain notes.
6. Produce one final review artifact using `template-review-report.md`.
