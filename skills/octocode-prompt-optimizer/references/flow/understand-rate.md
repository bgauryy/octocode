# READ, UNDERSTAND, RATE

Load when an optimization starts. A complete intent map and evidenced severity keep the repair aimed and proportional.

## READ

- Read every section. Record the document type, purpose, and any part you skipped or could not read.
- If you cannot read the path or inline content, request the missing input.

## UNDERSTAND

Identify the executing surface first. When a host, framework, skill loader, middleware, graph, or dependency assembles the context, load `runtime-context.md`; do not infer effective context from the edited source.

```markdown
## Understanding
Goal: <intended outcome> | Parts: <section -> purpose>
Surface: <host/framework/skill/direct API + runtime-resolved dependency>
Flow: <execution/routing order> | Effective boundary: <last observable model/tool input, or why unavailable>
Invariants: <intent, frozen contracts, identifiers, permissions> | Delivery: <output, write authority, budget, success check, urgency>
Assumptions: <safe, reversible; impact if wrong> | Unknowns: <choices that change intent, scope, or risk>
```

- Proceed with stated, reversible assumptions.
- Ask one focused question when interpretations change behavior, scope, or risk.
- Do not draft from partial input, invented text, or unresolved material choices.

## RATE

| Severity | Issue |
|---|---|
| Critical | weak modal in a truly critical rule; safety/permission conflict |
| High | missing enforcement; ambiguous action/referent; contradiction that changes intent (precise models spend reasoning reconciling it — [OpenAI GPT-5 guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)); a preference with no decidable boundary |
| Medium | missing output/gate; duplication; low density; imprecise term; buried rule; unmarked example/data; irrelevant metadata; rule in the wrong layer; over-prompting ("always use X", capitals) that causes over-triggering |
| Low | wordy sentence; repeated example; cosmetic residue; motivational or role-play framing |

Score each dimension 1–5 and average: A 4.5–5 · B 3.5–4.4 · C 2.5–3.4 · D <2.5.

| Dimension | 5 means |
|---|---|
| Clarity | concrete verbs, stable terms, explicit referents |
| Enforcement | proportionate boundaries and phase gates |
| Structure | visible order; examples/data separated where needed |
| Density | every sentence directs an action, sets a boundary, defines a distinction, or explains a consequence; one owner per rule |
| Output | concrete shape for every deliverable |
| Integrity | intent, metadata, branches, and commands preserved |

Record `Part | Issue | Severity | Fix` rows and `Dimension | Before | Evidence`, then `Overall: <avg> → <grade>`. Rate every part, cite evidence, and do not inflate severity. Keep optional modals optional. If the scan is unexpectedly clean, recheck modals, referents, conflicts, branches, and outputs once.

Next: `fix.md`. A severity that rests on assumed behavior needs `octocode-eval-benchmark` first. A multi-tool MCP server: rate the set with `../tools/contract-audit.md`.
