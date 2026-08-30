# Graph Report

Load when reporting a completed graph analysis. Why: separate predictions from proven issues and make the next decision auditable.

```markdown
## Scope
Question · root/entrypoints · tests/exclusions · limits · tool availability

## Graph snapshot
Operation · files scanned/skipped · result counts · warnings/truncation

## Findings
### [severity] <finding or candidate>
- Signal: <graph operation + exact result>
- Prediction: <mechanism → impact>
- Proof: <exact file:line edges; AST; LSP; check>
- Alternate: <killed or unresolved explanation>
- Confidence: confirmed | likely | candidate | dismissed
- Action: <smallest safe next step>

## Gaps
Unavailable lanes, uncertain roots, unsupported resolution, checks not run

## Verdict
Act now | investigate next | no issue shown
```

Lead with confirmed high-impact findings. Keep dismissed candidates short. Never hide empty/error status, inferred roots, partial scans, unresolved dynamic edges, or checks that did not run.

Return to graph triage only when a reported gap can change the verdict; otherwise this step ends here.
