# Rule shape, precedence, and patterns

Load when a rule leaves the next action or scope unclear, instructions conflict, or FIX needs a stock pattern. A diagnostic, not a template: "Be efficient with tools" leaves the choice open; "Reuse a schema already read; inspect it again when the tool is unfamiliar or its version changes" decides the next call.

## Clarify a rule (add only what is missing)

| Question | Add |
|---|---|
| What action changes? | A concrete instruction |
| What nearby behavior is confused with it? | A distinction or small example |
| Why does it matter? | The consequence; models generalize from the reason ([Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)) |
| Where does it apply? | Scope, prerequisites, exceptions |
| How is success shown? | Evidence that fits the claim |

- Merge answers that fit one sentence; add no contrast, rationale, or section to an action that is already clear.
- Golden-rule test: a colleague with minimal context follows the rule without asking (same source).
- Ask for wanted extra effort directly ("include edge cases and tests"); models do not infer "above and beyond".
- Weak: "Delete a symbol when `lspSearch` references returns empty." Repair: "Before deleting a symbol, read its exact source, check applicable `lspSearch` references and entrypoint/config paths, then run the relevant checks. An empty result describes only that query's completed scope." Errors, partial results, and unavailable servers never support absence; calling every tool is not a proof requirement.

## Precedence

Higher wins: 1 system and safety restrictions · 2 developer/host policy and tool restrictions · 3 explicit user request · 4 applicable critical rules · 5 skill/default workflow · 6 soft preference.

- Apply the higher source and record `Conflict: A vs B → priority N`. Stop when authority is ambiguous or the resolution changes intent.
- Remove or reconcile contradictions: a precise instruction-follower spends reasoning reconciling them instead of acting ([OpenAI GPT-5 guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)).

## Patterns

| Need | Pattern |
|---|---|
| Checkpoint | `STOP: <observable condition>` plus a gate check |
| Required action | `MUST <action>` only for a genuine requirement |
| Prohibition | `NEVER <unsafe action>` plus the allowed alternative |
| Decision | `IF <condition> → THEN <action/recovery>` or a decision table |
| Critical hardening | required action + forbidden unsafe opposite + concrete verification signal |
| Multi-mode flow | named branch with trigger, steps, output, recovery |

- Emphasis is not enforcement: drop shouting, all-caps, bribes, and blanket "always use X"; current models over-apply them. One firm, clear sentence usually fixes a misbehavior ([OpenAI GPT-4.1 guide](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide)).
- Give tool defaults a condition: "use X when it changes your answer", not "if in doubt, use X" (Anthropic, above).
- State summaries: Full Path at a real phase or context shift (goal, progress, next step, blockers); Fast Path only when context changes materially.

## Common mistakes

- Optional guidance over-strengthened; required frontmatter/config treated as metadata.
- Several terms for one concept; orphan referents; field types and limits copied out of their schema.
- Subjects, constraints, commands, branches, or recovery compressed away.
- XML as decoration; one critical rule repeated in other words; motivational language, role-play, decorative terms.

Next: wording and placement `style.md`; apply under `../flow/fix.md`.
