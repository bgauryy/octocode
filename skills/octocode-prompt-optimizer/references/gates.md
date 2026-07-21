# READ And UNDERSTAND Gates

Load at optimization start. Read the complete input and map intent before rating or drafting.

## READ
**STOP. DO NOT analyze or edit yet.**

### Pre-Conditions
- [ ] A readable file or inline prompt exists.

### Required Actions
1. Read every section.
2. Record document type, purpose, and approximate line count.

### Gate Check
- [ ] Input is complete enough to evaluate; skipped/unreadable parts are named.

## UNDERSTAND
**STOP. Map intent before RATE.**

```markdown
## Understanding
Goal: <intended outcome>
Parts: <section -> purpose>
Flow: <execution/routing order>
Assumptions: <safe, reversible assumptions and impact if wrong>
Unknowns: <material choices that change intent, scope, or risk>
```

Safe, reversible assumptions may proceed when stated. Material unknowns require one focused question and a pause.

### Gate Check
- [ ] Goal, parts, flow, assumptions, and material unknowns are explicit.
- [ ] Required branches and constraints remain intact.

### Forbidden
- Editing during READ; partial-read conclusions; invented missing text.
- Editing from unstated assumptions or unresolved material choices.

### Allowed
- Read-only access, line counts, rereads, stated low-risk assumptions, and focused clarification.

### On Failure
- **IF** a path fails but inline content exists → **THEN** use the inline content.
- **IF** no readable content exists → **THEN** ask for the correct path/content.
- **IF** interpretations change behavior → **THEN** present options and wait.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — start minimal, then add instructions from observed failure modes.
