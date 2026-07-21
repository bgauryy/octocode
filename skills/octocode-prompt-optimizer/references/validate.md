# VALIDATE Gate

Load after FIX and before OUTPUT. **STOP: validate the complete draft before writing or presenting it.**

### Pre-Conditions
- [ ] FIX completed or recorded every rated issue.

## Required Checks

- [ ] Critical rules use proportionate enforcement; optional guidance stays optional.
- [ ] No conflicting instructions, ambiguous actions/referents, filler, or duplicate rule owners.
- [ ] Original intent, required branches, exact commands, and necessary frontmatter/metadata remain intact.
- [ ] Every intended branch has an explicit trigger, action, output, and recovery; branches do not overlap ambiguously.
- [ ] Expected outputs have concrete shapes; decision points have explicit routing such as IF/THEN or a decision table.
- [ ] Examples/reference data are separated only where they could be mistaken for live instructions.
- [ ] Critical rules are easy to find; tags are closed and used only for real separation.
- [ ] Before/after score is recorded; line growth is under 10% or justified.

## Reflection

1. Would this execute reliably for every intended mode?
2. What is the weakest remaining branch or section?
3. Did any edit change intent? The answer must be No.

### Definition Of Done

- All intended execution paths are unambiguous; multiple valid branches are allowed.
- Inputs, outputs, permissions, stop conditions, and recovery are explicit where the source needs them.
- Cross-references resolve by stable names rather than position.

### Gate Check
- [ ] Required checks and reflection pass with no intent change.

### Forbidden
- Output after a failed check, removal of required metadata, or forcing a multi-mode prompt into one path.

### Allowed
- Return to FIX; preserve an intentional branch and explain why it remains.

### On Failure
- **IF** a repair is local → **THEN** return to FIX.
- **IF** intent changed or a material choice is unresolved → **THEN** return to UNDERSTAND and ask when needed.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — use verifiable outcomes and measure errors, calls, and tokens alongside task success.
