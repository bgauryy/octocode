# Precedence And Instruction Patterns

Load when instructions conflict or FIX needs a compact pattern. Gate files remain the source of truth.

## Precedence

Higher-priority instructions win:

| Priority | Source |
|---:|---|
| 1 | system and safety restrictions |
| 2 | developer/host policy and tool restrictions |
| 3 | explicit user request |
| 4 | applicable critical rules |
| 5 | skill/default workflow |
| 6 | soft preference |

Detect both rules, apply the higher source, and document one line: `Conflict: A vs B → priority N`. Stop when authority is ambiguous or the resolution would change user intent.

## Useful Patterns

| Need | Pattern |
|---|---|
| Checkpoint | `STOP: <observable condition>` plus a Gate Check |
| Required action | `MUST <action>` only for a genuine requirement |
| Prohibition | `NEVER <unsafe action>` plus the allowed alternative |
| Decision | `IF <condition> → THEN <action/recovery>` or a decision table |
| Critical hardening | state + unsafe-opposite boundary + verification |
| Multi-mode flow | named branch with trigger, steps, output, recovery |

Keep action tables, exact commands, explicit boundaries, and real output templates. Reduce unconstrained explanations, duplicate examples, decorative markup, catch-all verbs, and hedging inside critical rules.

## State Summaries

Full Path: summarize goal, progress, next step, and blockers at a real phase/context shift. Fast Path: summarize only when context changes materially.

## Common Mistakes

- Over-strengthening optional guidance.
- Treating required frontmatter/configuration as irrelevant metadata.
- Naming one concept with several terms or leaving an orphan referent.
- Compressing away subjects, constraints, commands, branches, or recovery.
- Using XML decoratively rather than to separate instructions from literal data.
- Repeating the same critical rule with different wording.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — distinct prompt sections and a specific-but-not-brittle instruction altitude.
