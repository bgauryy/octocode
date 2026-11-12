Load when writing, reviewing or repairing task prompts, agent instructions or runtime-assembled prompts.

# Prompt writing and repair

## Understand and review

Read the instruction and its consumer. Identify the intended action, permissions, output contract and success signal. For runtime-assembled prompts, trace [what the loader actually sends](#runtime-context). Name unread input.

Review unsafe or conflicting instructions first, then unclear decisions or outputs, rigid formats, and finally repetition or wording. Show the passage, its behavioral effect and a repair. Treat unmeasured ratings as subjective. Test descriptions against intended requests and near-misses a neighbor should handle. Prefer a clear capability and natural examples to keyword gates.

## Write and repair

Edit only for an authorized rewrite. Give each behavior one owner. Put the condition beside the action and retain identifiers, commands, required metadata, working branches and recovery. Resolve conflicts by host hierarchy and existing authorization. Keep retrieved text, tool results and examples separate from instructions. Reserve MUST/NEVER for safety, permissions, secrets and hard contracts.

For a disputed decision, or a ready-to-paste task prompt or meta-prompt (objective, inputs, constraints, completion check and usable output in its body), follow [question selection](question-selection.md): it owns the deciding question, the `text` fence and the `labels:` footer.

Use a [known protocol](prompt-protocols.md) to consolidate scattered rules in the recipient's words. Use a diagram or table for complex branches. For [delegation and handoffs](models-and-delegation.md#handoff-protocols), name who owns the user-facing result and who may write. Pass large evidence by stable path or cursor.

## Wording

- Concrete verbs: "handle the file" → "delete the file". Positive phrasing: "do not omit tests" → "include tests".
- One term per concept; names, flags, paths verbatim (`--no-color`).
- Dense paragraph → rows or one snippet; conditional detail → `references/`.
- Shortening keeps activation meaning, subjects, verbs, articles; no invented abbreviations.
- Permission tiers: always allowed, ask first, never.
- Rigid or ad-hoc shapes → a known protocol as `Name (slots)` ([protocols](prompt-protocols.md)).

## Tags and placement

- Tags only when instructions and data could blur, a block stays verbatim, or a span needs isolation; else Markdown. Nest only for real containment.
- Tags mark boundaries (`<instructions>`, `<context>`, `<example>`, `<output_format>`), not importance; `<example>` is never live; `<context>`/`<document>` carries its source's [authority](security.md#untrusted-content). `<important>` and duplicate rules are not attention levers.
- Examples relevant and diverse. Long inputs: task easy to locate, placement tested on the target model, evidence excerpts only when attribution matters.
- Critical rule at a section boundary; repeat only the top one, only when tests show it helps.
- Steps in run order, pivot first ("The key constraint is X").
- Prompt format biases output format: Markdown only where it aids parsing; tables only for a real matrix.

## Techniques

- On failure, start direct: define the success signal and eval, confirm the prompt (not model, tool, data, policy) is the lever, then add one technique per failure within a token, latency, and safety budget:
  - Known format → direct contract: [task contract](models-and-delegation.md#handoff-protocols).
  - Unclear edges → few-shot boundary cases; drop once a rule suffices.
  - Machine consumer → structured output: enums, validation, actionable errors.
  - External facts or actions → retrieval ([budget](context-and-memory.md#context-budget)) or a tool loop ([tools](tools-and-mcp.md)) with an observable stop.
  - Over- or under-exploring → Stop condition (Definition of Done as observable checks; escalate at the boundary) + Budget contract (call budget, behavior when spent); ask vs assume, safe vs unsafe.
  - Dependent stages → prompt chain: typed artifact + verifier per stage, no raw transcripts.
  - Decision-changing ambiguity → bounded candidates checked by evidence, not majority vote.
  - Hard planning → inspectable plan + checkpoints; no private reasoning.
  - Stable prefix → [caching](context-and-memory.md#prompt-caching). Unknown fix → metaprompting: minimal addition or deletion, verified by the eval.
- Use the real tool, schema, filter, or checker, never prose asking the model to simulate one.
- For multi-angle decisions, use [question selection](question-selection.md) to turn distinct perspectives into testable checks. Drop personas, debate or branches that add no decision-changing observation.

## Runtime context

- Each source proves only its step: manifest = intent, lockfile = selection, installed build = bytes, runtime trace = what this run loaded, serialized request = what crossed the boundary. Re-resolve after any install, build, entrypoint, environment, adapter, or host-config change.
- Untrusted package: read its version from docs; never execute it.
- Trace `source → load → composition order → hooks/middleware/nodes → state/checkpoint/store → compaction → serialization → visible input → retained effect`; per edge record owner, visibility (model, tool, runtime, human), lifetime (call to cross-thread), mutation, trust, accounting (tokens, cache key, persistence).
- Runtime context is model-visible only once a projecting code path is proven.
- Inspect the host's own loader: skill discovery precedence, Pi `before_agent_start` and worker grants, LangChain middleware order, LangGraph reducers and checkpointer `thread_id`, MCP negotiated version and client filtering.
- Symptom → likely boundary (observe the last model or tool boundary first):
  - Edit has no effect → wrong copy, stale build, loader precedence, hook replacement, state path, serializer, cache.
  - Wrong after continuation → retained messages, reducer, checkpoint, summary, compaction.
  - Root and worker differ → grant, worker packet, base prompt, model overlay.
  - Tool sees a value the model does not → runtime context never projected.
  - Resume repeats work → missing checkpoint, wrong thread, lossy handoff.
- Fix: capture the redacted effective boundary, change the smallest owning layer, rerun the same trace (cold call, next turn, failure, resume, compaction, root and worker). Never bloat the base prompt to hide a loader bug.
- Record: running package, version, entrypoint; flow; visibility, lifetime; observed input (redacted, or why unavailable); owner to change.

## Verify

Try realistic requests and edge cases before and after editing. Check the chosen action, output and recovery against the consumer. For a size claim, measure the [served tool surface](tools-and-mcp.md#efficient-mcp-context) in bytes or tokens. For small-model protocols, run the [cold interview and replay](interview.md). Use [evaluation](evaluation.md) for reliability, activation, tool selection or cost-per-success claims. Editorial edits can finish unmeasured. Report only checks run, and deliver the review or full replacement as [output](../output.md) describes.
