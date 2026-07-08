# FIX Gate

Load when fixing rated issues, after RATE and before VALIDATE.

**STOP. Fix issues in priority order: Critical → High → Medium → Low.**

### Pre-Conditions
- [ ] RATE gate completed
- [ ] Issues table produced

### Fix priority (follow the order)
1. **Critical** — weak words in MUST/FORBIDDEN contexts.
2. **High** — missing enforcement, ambiguous instructions.
3. **Medium** — missing output formats, missing gates.
4. **Low** — redundancy, density, wordiness (only where value is added).

### Command strength hierarchy
| Strength | Keywords | Use for |
|----------|----------|---------|
| Absolute | never, always, must, forbidden, critical | Non-negotiable rules |
| Stop | STOP, HALT, DO NOT proceed, WAIT | Gates and checkpoints |
| Required | required, mandatory | Essential steps |
| Soft | should, prefer | Optional guidance only |

Prefer positive framing (tell the agent what to do); reserve prohibitions for destructive, fragile, or order-dependent rules.

### Triple Lock (for critical rules)
1. STATE: "The agent must X"
2. FORBID: "FORBIDDEN: not doing X"
3. REQUIRE: "Require verification that X is complete"

### Conciseness pass (shorten without losing logic)
Apply when a line is wordy or indirect (RATE `wordy-indirect`). Use `conciseness-toolkit.md` for the specific moves.
- Density over length: minimal is not the same as short. Cut only tokens that carry no signal; keep every token that changes behavior.
- **IF** a sentence packs more than one directive or exceeds about 20-25 words → **THEN** split into one instruction per sentence.
- **IF** a line uses a nominalization, passive voice, expletive opener, or double negative → **THEN** rewrite per the toolkit, preserving exact logic.

### Attention & structure pass (place what survives where it lands)
Apply after the conciseness pass, for `imprecise-wording`, `buried-critical-rule`, or `unmarked-example-or-data`. Use `attention.md` for the specific moves.
- **IF** a verb is a catch-all or a concept uses shifting synonyms → **THEN** swap in the concrete verb / one consistent term, keeping exact commands and versions intact.
- **IF** a MUST/NEVER rule sits mid-section → **THEN** move it to the section start and restate the single most important one at the end; never re-specify it differently.
- **IF** literal examples or reference data are mixed into instructions → **THEN** wrap them in `<example>`/`<context>` tags; keep Markdown default elsewhere.

### Reasoning block (before changes)
Required on Full Path, or on Fast Path with any Critical/High issue. Optional on Fast Path with only Medium/Low issues — include a one-line rationale instead.

```markdown
<reasoning>
1. Current state: [What exists now]
2. Goal: [What we are trying to achieve]
3. Approach: [Why this specific change]
4. Risk: [What could go wrong]
</reasoning>
```

### Gate template (when adding gates)
```markdown
## [Name] Gate
**STOP. DO NOT proceed. [What to verify]**
### Pre-Conditions
- [ ] [Previous step completed]
### Required actions
1. [Action]
### Gate Check
- [ ] [Condition]
### Forbidden
- [What not to do]
### Allowed
- [What is permitted]
### On Failure
- **IF** [condition] → **THEN** [recovery]
```

### Gate Check
- [ ] All Critical issues fixed
- [ ] All High issues fixed
- [ ] Medium/Low addressed or recorded as skipped
- [ ] Reasoning requirement satisfied (block produced, or Fast Path low-risk rationale recorded)

### Forbidden
- Over-strengthening soft guidance (keep "should" for optional items)
- Changing logic that already works
- Adding unnecessary complexity
- Skipping Critical/High issues
- Bloating: over 10% line increase without a justification in VALIDATE

### Allowed
- Text output (draft fixes)
- Iterating on fixes

### On Failure
- **IF** over-strengthening detected → **THEN** revert and re-assess with the RATE criteria.
- **IF** unsure whether logic changed → **THEN** compare before/after intent.
