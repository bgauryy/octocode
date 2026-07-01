# VALIDATE Gate

Load when validating all fixes, after FIX and before OUTPUT.

**STOP. DO NOT output yet. Validate all fixes against the checklist.**

### Pre-Conditions
- [ ] FIX gate completed
- [ ] All Critical/High issues addressed

### Validation checklist
Required checks:
- [ ] No weak words in critical sections
- [ ] Critical rules use MUST/NEVER/FORBIDDEN
- [ ] No conversational filler
- [ ] No meta-noise: no talk-about-the-prompt, stray metadata/frontmatter, or redundant content
- [ ] Best-practices score recorded (before → after)
- [ ] No conflicting instructions
- [ ] Logical flow preserved
- [ ] Original intent preserved
- [ ] Shortened lines keep all original logic (no dropped signal words, no new ambiguity)
- [ ] Triple Lock applied to critical rules
- [ ] Line-count target met (under 10%), or a justified exception is recorded
- [ ] Any over-10% increase carries a one-line reason tied to required gate/clarity fixes

Additional checks (if applicable):
- [ ] Gates have Pre-Conditions, Gate Check, Forbidden, Allowed, On Failure
- [ ] Outputs have format specifications
- [ ] IF/THEN rules for decision points

Referential clarity:
- [ ] No ambiguous pronoun or positional reference without an explicit antecedent
- [ ] Every entity has a stable name (same term throughout)
- [ ] Steps/outputs referenced by name, not position
- [ ] All cross-references are unambiguous
- [ ] Markdown is the default; XML only for attention-control needs
- [ ] One consistent term per concept; catch-all verbs replaced with concrete ones
- [ ] No critical (MUST/NEVER/FORBIDDEN) rule buried mid-section; non-negotiables sit at a section start or end
- [ ] Literal examples/reference data separated from instructions with `<example>`/`<context>` tags; every tag closed

### Reflection
Answer these:
1. Would I trust this prompt to execute reliably?
2. What is the weakest remaining section?
3. Did I change any original intent? (Answer must be No.)

**IF** a weakness is identified → **THEN** fix it or record it as a limitation.
**IF** intent changed → **THEN** STOP, revert, and return to UNDERSTAND.

### Definition of Done
All must be true before OUTPUT:
- [ ] Single execution path (no ambiguous branches)
- [ ] All inputs/outputs explicitly defined
- [ ] All decision points use IF/THEN
- [ ] No orphan references (every "it/this" resolved)

### Gate Check
- [ ] All required checks pass
- [ ] Reflection questions answered
- [ ] No intent changes

### Forbidden
- Outputting without completing validation
- Skipping checklist items
- Proceeding with a failed check
- Using XML tags outside attention-control needs

### Allowed
- Text output (validation results)
- Returning to the FIX gate

### On Failure
- **IF** validation fails → **THEN** return to the FIX gate.
- **IF** intent changed → **THEN** return to the UNDERSTAND gate.
