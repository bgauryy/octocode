# FIX: repair order, rule shape, and precedence

Load after RATE, or directly for a goal; also when a rule is unclear, instructions conflict, or FIX needs a stock pattern. Repair evidenced issues in severity order; record deliberate deferrals.

## Repair rules

Add an example or consequence only when it resolves ambiguity. Do not redesign.

## Clarify a rule (add only what is missing)

| Question | Add |
|---|---|
| What action changes? | A concrete instruction |
| What nearby behavior is confused with it? | A distinction or small example |
| Why does it matter? | The consequence; models generalize from the reason |
| Where does it apply? | Scope, prerequisites, exceptions |
| How is success shown? | Evidence that fits the claim |

- Merge answers that fit one sentence.
- Ask for wanted extra effort directly ("include edge cases and tests").

## Precedence

Stop when the resolution changes intent. Remove or reconcile contradictions.

## Patterns

| Need | Pattern |
|---|---|
| Checkpoint | `STOP: <observable condition>` plus a gate check |
| Required action | `MUST <action>` only for critical, fragile, destructive, or permission-sensitive behavior |
| Prohibition | `NEVER <unsafe action>` plus the allowed alternative |
| Decision | `IF <condition> → THEN <action/recovery>` or a decision table |
| Critical rule, only when omission is high-risk | 1 state the action · 2 forbid the unsafe opposite · 3 require a verification signal |
| Multi-mode flow | named branch with trigger, steps, output, recovery |

- One firm sentence usually fixes a misbehavior.
- Give tool defaults a condition ("use X when it changes your answer").
- State summaries: Full Path at a real phase or context shift (goal, progress, next step, blockers); Fast Path only when context changes materially.

Common mistakes: required frontmatter treated as metadata; schema types copied out of the schema; recovery paths compressed away.

## Change note

For Critical/High issues; smaller repairs get one rationale line.

```markdown
Current: <problem> | Goal: <preserved intent> | Change: <bounded repair> | Risk: <regression and check>
```

Source: [OpenAI GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide).

Next: wording and placement `../writing/style.md`; then `validate-output.md`.
