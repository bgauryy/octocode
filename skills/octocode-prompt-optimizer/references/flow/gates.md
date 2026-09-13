# READ and UNDERSTAND

Load when an optimization starts, before rating, or drafting. Why: a complete intent map prevents repairs aimed at the wrong problem.

## Read

Read every section. Record the document type, purpose, and any part you skipped or could not read. When you cannot read the path or the inline content, request the missing input.

## Understand

Identify the executing surface before mapping the instruction flow. When a host, framework, skill loader, middleware, graph, or dependency assembles the context, load `references/flow/runtime-context.md`; do not infer effective model context from the edited source alone.

```markdown
## Understanding
Goal: <intended outcome>
Parts: <section -> purpose>
Surface: <host/framework/skill/direct API and runtime-resolved dependency>
Flow: <execution/routing order>
Effective boundary: <last observable model/tool input, or why it is unavailable>
Assumptions: <safe, reversible assumptions and impact if wrong>
Unknowns: <material choices that change intent, scope, or risk>
```

Proceed with stated, reversible assumptions. Ask one focused question when interpretations materially change behavior, scope, or risk. Do not draft from partial input, invented text, or unresolved material choices.

## Sources
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) — start minimal, then add instructions from observed failure modes.

Next: with the map complete load `references/flow/rate.md`; for instruction conflicts load `references/writing/patterns.md`; when a material unknown remains, ask before RATE.
