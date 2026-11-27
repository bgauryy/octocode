Load when context, tool work, caches, memory, queues, or repeated computation need a measured end-to-end audit.

# Resource-flow audit

Use the agentic-plumber metaphor only to label duties. Trace one representative request through inputs, loaders, serializers, tool calls, handoffs, stores, retries, and consumers; map each observed issue to its owning boundary.

| Examine | Evidence |
|---|---|
| Supply | User input, instructions, retrieved evidence, tool output, memory: origin and purpose |
| Pipes/junctions | Loaders, transforms, queues, parsers, handoffs, consumers, and required fields |
| Leaks/clogs | Repeated reads, copies, retries, slow stages, output limits, queue growth |
| Tanks/valves | Cache keys, scope, freshness, invalidation, buffers, selection, cancellation |
| Quality/meter | Contradictory instructions, provenance, intended action; tokens/cache counters, bytes, CPU, memory, I/O, latency, correctness |

## Map, meter, diagnose, repair, replay

1. The orchestrator assigns a plumber actor one representative task, baseline, correctness check, and units. The actor maps the effective consumer payload and names unavailable observations.
2. Measure boundaries and state a rival cause. Token count is not heap, CPU, or algorithmic complexity; persistent agent memory is not process memory.
3. Trace the largest demonstrated issue to its owner: `octocode-agentic` for effective instructions, `octocode-research` for evidence/tool semantics, `octocode-engineer` for code/dataflow/lifetime, and `octocode-agentic` measurement for comparisons. `octocode-context-audit` is an optional inventory helper when installed.
4. Repair loop (PDCA): plan from the step 1 baseline, correctness check, and units; do one authorized correction that preserves required evidence and continuations; check by replaying the same acceptance conditions and fixed inputs on the baseline and on the change. Save each run beside the ledger as `before.json` and `after.json` (`acceptancePassed`, `metrics` named with their units, the returned `evidence` anchors; shape in `--help`), with repeated samples for a metric that varies between runs, such as time or memory. Then compare them with [ledger.mjs](../scripts/ledger.mjs): `node <skill-folder>/scripts/ledger.mjs --before before.json --after after.json`. Exit 1: the change failed acceptance or `lostEvidence` lists dropped evidence; revert it. Exit 0: keep the change only when the target metric's `verdict` moved the intended way (`lower` for a cost) and no metric you must hold moved the other way; `equal` or `within-spread` is no benefit, so revert. Exit 2 names the malformed field. A flow tester actor is useful only when it can independently check correctness or replicate a comparable measurement.
5. Record bottleneck, owner, change, before/after (the compared medians and `verdict`), and gaps in the shared ledger.

Do not trim required instructions or tool-result evidence for a token metric; do not assume a cache hit from latency; do not add a cache without reuse and invalidation evidence. Uncertain mutation and sealed evaluation constraints from [verification](verification.md) and the domain owner still apply.
