# FIX Gate
Load after RATE and before VALIDATE. Fix Critical → High → Medium → Low; record deliberate deferrals.
### Pre-Conditions
- [ ] RATE produced evidenced issues and a before score.
## Rules
- Preserve intent, working logic, branches, identifiers/commands, and necessary metadata.
- Use MUST/NEVER only for critical, fragile, destructive, or permission-sensitive behavior.
- Prefer direct positive actions; keep prohibitions where crossing the boundary is dangerous.
- Use `conciseness-toolkit.md` for token cuts and `attention.md` for placement.
- Keep one term per concept and one owner per rule.
## Critical Rule Pattern — add all three locks only when omission is high-risk
1. State the required action.
2. Forbid the unsafe opposite.
3. Require a concrete verification signal.

## Reasoning Receipt
Required on Full Path or for Critical/High issues; otherwise one rationale line is enough.
```markdown
Current: <problem>
Goal: <preserved intent>
Change: <bounded repair>
Risk: <regression and check>
```
## Gate Template
```markdown
## <Name> Gate
STOP: <condition>
Pre-Conditions: <state>
Required Actions: <ordered actions>
Gate Check: <observable pass>
Forbidden: <unsafe action>
Allowed: <safe action>
On Failure: IF <condition> → THEN <recovery>
```
### Gate Check
- [ ] Critical/High issues fixed; others fixed or recorded.
- [ ] Intent/branches remain unchanged; growth is under 10% or justified.

### Forbidden
- Optional-to-mandatory escalation, redesign, conflicting duplicates, or unverified writes.

### Allowed
- Draft text, iterative comparison, and scoped structural changes.

### On Failure
- **IF** intent or working logic changed → **THEN** revert that edit and return to UNDERSTAND.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — clear, specific prompts at the right level of prescription.
