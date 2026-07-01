# OUTPUT Gate

Load when formatting the final document and change report, after VALIDATE passes.

**STOP. DO NOT proceed. Verify the VALIDATE gate passed before outputting.**

### Pre-Conditions
- [ ] VALIDATE gate completed
- [ ] All required checks passed
- [ ] No intent changes confirmed

### Choosing the variant
- **IF** the user wants a complete rewritten document → **THEN** use Variant A.
- **IF** the user wants minimal edits/delta only → **THEN** use Variant B.
- **IF** the user wants review-only, or the runtime cannot write safely → **THEN** use Variant B unless a full rewrite was explicitly requested.
- **IF** the user did not specify → **THEN** default to Variant A.

### Report header (both variants)
```markdown
# Optimization Complete
## Summary
- Issues Found: [N]
- Fixes Applied: [N]
- Intent Preserved: Yes
- Best-Practices Grade: [before] → [after]
## Changes Made
| Category | Count | Examples |
|----------|-------|----------|
| Command Strengthening | [N] | [Brief example] |
| Gates Added/Fixed | [N] | [Brief example] |
| Redundancy Removed | [N] | [Brief example] |
```

### Variant A — full document (default)
```markdown
## Optimized Document
[Full optimized content]
```

### Variant B — patch-style delta (minimal edits)
```markdown
## Patch-Style Delta
| Section | Before | After | Why |
|---------|--------|-------|-----|
| [Section name] | [Old text] | [New text] | [Reason] |
```

### Gate Check
- [ ] Selected variant matches user intent
- [ ] Report header included
- [ ] Deliverable present (full document for A, delta table for B)

### Forbidden
- Deviating from the selected output variant
- Outputting without a validation pass
- Omitting the required deliverable
- Claiming a file was updated when the write policy prevented edits

### Allowed
- Safe file edit/write to save the optimized content
- Text output (summary plus document)

### On Failure
- **IF** the format deviates → **THEN** regenerate the output.
- **IF** the user requests changes → **THEN** return to the FIX gate.
