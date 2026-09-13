# FIX
Load after RATE, or directly when the input is a goal rather than an existing prompt. Why: repair evidenced issues in severity order and record deliberate deferrals.

Use `references/writing/behavior.md` when a rule leaves its action or scope unclear. Add an example or consequence only when it resolves ambiguity; do not expand a clear instruction into a template.

## Rules
- Preserve intent, working logic, branches, identifiers/commands, and necessary metadata.
- Use MUST/NEVER only for critical, fragile, destructive, or permission-sensitive behavior.
- Prefer direct positive actions; keep prohibitions where crossing the boundary is dangerous.
- Use `references/writing/conciseness-toolkit.md` for token cuts and `references/writing/attention.md` for placement.
- Keep one term per concept and one owner per rule.
- Put field types and limits in the schema, selection guidance in the description, and workflow in the server instructions — never the same rule in two layers.
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

Next: with the draft complete load `references/flow/validate.md` — never present a fix that skipped it; for the rule shape load `references/writing/behavior.md`; for a reusable pattern load `references/writing/patterns.md`; load only the applicable domain owner: `references/tools/tool-contracts.md`, `references/tools/contract-audit.md`, `references/agents/agent-communication.md`, `references/agents/agent-prompt-integrity.md`, `references/context/context-budget.md`, `references/context/token-economics.md`, `references/context/prompt-caching.md`, or `references/context/untrusted-content.md`.
