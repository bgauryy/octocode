# Review Report Template

Use for PR and local change reviews. Keep the chat summary short; ask before writing review files.

## Tone

Direct, evidence-backed, non-performative. Do not nitpick style unless it creates risk. Prefer fixable findings over commentary.

## Chat summary

```markdown
Review complete: <PR/local scope>

Scope: <files/lines/areas>
Risk: <low|medium|high> — <why>
Guidelines used: <none|list>
Existing comments checked: <yes/no/N/A>

Top findings:
1. [SEV][DOMAIN] <title> — <file:line> — <one-line impact>
2. ...

Recommendation: <safe to merge | fix before merge | split PR | needs more context>
```

## Finding format

```markdown
[SEC-1] <Short title>
Severity: HIGH|MED|LOW
Confidence: confirmed|likely|uncertain
Location: path/to/file.ext:123
Domain: Security|Bug|Flow|Architecture|Performance|Error Handling|Quality|Guidelines

Problem:
<What is wrong, limited to changed code or direct blast radius.>

Evidence:
- <file:line evidence from diff/read/LSP/AST/history>

Impact:
<Why this matters to callers, users, data, contracts, or operations.>

Suggested fix:
```diff
<minimal diff or precise code direction>
```
```

## PR report outline

```markdown
# PR Review: <owner/repo#N> — <title>

## Executive Summary
| Aspect | Value |
|---|---|
| Files changed | N |
| Lines changed | +A / -D |
| Risk | Low/Medium/High |
| Recommendation | Safe / Fix before merge / Split |

## Guidelines Applied
- <source>: <rule summary>

## High Priority Issues
<findings>

## Medium Priority Issues
<findings>

## Low Priority / Notes
<only if useful>

## Flow Impact Analysis
- Changed symbol → callers/references → compatibility status

## Existing PR Comments
- Resolved: <list>
- Still unresolved: <list>

## Suggested Next Steps
- [ ] <action>
```

## Local changes report outline

```markdown
# Local Changes Review: <branch>

## Executive Summary
| Aspect | Value |
|---|---|
| Scope | staged / unstaged / both / files |
| Files changed | N |
| Risk | Low/Medium/High |
| Recommendation | Commit / Fix first / Split |

## Changed Areas
- <area>: <files>

## Findings
<same finding format>

## Flow Impact Analysis
- Changed symbol → callers/references → compatibility status

## Suggested Next Steps
- [ ] Run tests
- [ ] Fix listed issues
- [ ] Split into commits if needed
```
