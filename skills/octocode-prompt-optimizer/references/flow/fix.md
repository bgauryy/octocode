# FIX

Load after RATE, or directly when the input is a goal. Repair evidenced issues in severity order and record deliberate deferrals.

## Rules

- Fix Critical/High issues; fix or record the rest.
- Preserve intent, working logic, branches, identifiers, commands, and necessary metadata.
- Use MUST/NEVER only for critical, fragile, destructive, or permission-sensitive behavior; do not escalate optional guidance.
- State the wanted action; keep a prohibition only where crossing the boundary is dangerous.
- Add an example or consequence only when it resolves ambiguity; do not expand a clear rule into a template.
- Keep one term per concept and one owner per rule.
- Put field types and limits in the schema, selection guidance in the description, and workflow in the server instructions; never the same rule in two layers.
- Do not redesign, duplicate rule owners, or write unverified changes.
- Explain material growth; brevity is not the only goal.
- If a repair changes intent or working logic, revert it and return to UNDERSTAND.

## Critical rule pattern

Use all three only when omission is high-risk:

1. State the required action.
2. Forbid the unsafe opposite.
3. Require a concrete verification signal.

## Change note

For Critical/High issues; one rationale line is enough for smaller repairs.

```markdown
Current: <problem> | Goal: <preserved intent> | Change: <bounded repair> | Risk: <regression and check>
```

Next: rule shape and conflicts `../writing/rules.md`; wording and placement `../writing/style.md`; then `validate-output.md`. Never present a fix that skipped VALIDATE.
