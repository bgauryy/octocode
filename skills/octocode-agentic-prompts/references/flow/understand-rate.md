# READ, UNDERSTAND, RATE

Load when you need the UNDERSTAND record template or the full RATE rubric. Why: `SKILL.md` owns the phase rules; this page holds the template and scoring detail.

## UNDERSTAND record

```markdown
## Understanding
Goal: <intended outcome> | Parts: <section -> purpose>
Surface: <host/framework/skill/direct API + runtime-resolved dependency>
Flow: <execution/routing order> | Effective boundary: <last observable model/tool input, or why unavailable>
Invariants: <intent, frozen contracts, identifiers, permissions> | Delivery: <output, write authority, budget, success check, urgency>
Assumptions: <safe, reversible; impact if wrong> | Unknowns: <choices that change intent, scope, or risk>
```

## RATE

Severity detail for issues the `SKILL.md` severity line does not name:

| Severity | Issue |
|---|---|
| High | ambiguous referent; a contradiction that changes intent |
| Medium | low density; imprecise term; unmarked example or data; irrelevant metadata; over-prompting such as "always use X" or capitals |
| Low | repeated example; motivational framing |

| Dimension | 5 means |
|---|---|
| Clarity | concrete verbs, stable terms, explicit referents |
| Enforcement | proportionate boundaries and phase gates |
| Structure | visible order; examples and data separated where needed |
| Density | every sentence directs an action, sets a boundary, defines a distinction, or explains a consequence |
| Output | concrete shape for every deliverable |
| Integrity | intent, metadata, branches, and commands preserved |

Record `Part | Issue | Severity | Fix` rows and `Dimension | Before | Evidence`, then `Overall: <avg> → <grade>`. If the scan is unexpectedly clean, recheck modals, referents, conflicts, branches, and outputs once.

Next: `fix.md`. A multi-tool MCP server: rate the set with `../tools/tool-contracts.md` (Set audit).
