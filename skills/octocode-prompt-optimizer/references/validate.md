# VALIDATE

Load after FIX and before OUTPUT. Why: validate the complete draft before writing or presenting it.

## Required Checks

- [ ] Critical rules use proportionate enforcement; optional guidance stays optional.
- [ ] No conflicting instructions, ambiguous actions/referents, filler, or duplicate rule owners.
- [ ] Original intent, required branches, exact commands, and necessary frontmatter/metadata remain intact.
- [ ] Every intended branch has an explicit trigger, action, output, and recovery; branches do not overlap ambiguously.
- [ ] Expected outputs have concrete shapes; decision points have explicit routing such as IF/THEN or a decision table.
- [ ] Examples/reference data are separated only where they could be mistaken for live instructions.
- [ ] Critical rules are easy to find; tags are closed and used only for real separation.
- [ ] Before/after score is recorded; material growth is justified by behavior.

## Final questions

1. Would this execute reliably for every intended mode?
2. What is the weakest remaining branch or section?
3. Did any edit change intent? The answer must be No.

Do not output after a failed check or force a valid multi-mode prompt into one path. Repair local failures in FIX; return to UNDERSTAND when intent changed or a material choice remains unresolved.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — use verifiable outcomes and measure errors, calls, and tokens alongside task success.

Next: when every check passes load `references/output.md`; when a local repair is needed return to `references/fix.md`; when intent changed return to `references/gates.md`; when a reliability claim still needs proof load `references/evaluation-data.md`.
