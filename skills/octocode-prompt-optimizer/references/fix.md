# FIX
Load after RATE and before VALIDATE. Why: repair evidenced issues in severity order and record deliberate deferrals.
## Rules
- Preserve intent, working logic, branches, identifiers/commands, and necessary metadata.
- Use MUST/NEVER only for critical, fragile, destructive, or permission-sensitive behavior.
- Prefer direct positive actions; keep prohibitions where crossing the boundary is dangerous.
- Use `references/conciseness-toolkit.md` for token cuts and `references/attention.md` for placement.
- Keep one term per concept and one owner per rule.
## Critical rule pattern

Use all three only when omission is high-risk:
1. State the required action.
2. Forbid the unsafe opposite.
3. Require a concrete verification signal.
## Change note

Use this for Critical/High issues; one rationale line is enough for smaller repairs.
```markdown
Current: <problem>
Goal: <preserved intent>
Change: <bounded repair>
Risk: <regression and check>
```
Fix Critical/High issues; fix or record the rest. Avoid optional-to-mandatory escalation, redesign, duplicate rule owners, or unverified writes. If a repair changes intent or working logic, revert it, and return to UNDERSTAND. Explain material growth instead of treating brevity as the only goal.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — clear, specific prompts at the right level of prescription.

Next: with the draft complete load `references/validate.md` — never present a fix that skipped it; for a reusable instruction pattern load `references/patterns.md`; for the domain-specific repair load `references/tool-contracts.md`, `references/agent-communication.md`, `references/context-budget.md`, or `references/untrusted-content.md`.
