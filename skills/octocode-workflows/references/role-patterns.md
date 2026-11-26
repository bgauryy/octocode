Load when a workflow needs concrete role duties, an independent check, or a reason to remain single-actor.

# Role patterns

Metaphors label observable duties; they grant no authority, expertise, or extra worker. Ledger ownership and actor returns follow the [orchestrator and actor contract](composition.md#orchestrator-and-actor-contract); “inspector”, “analyst”, and “tester” are actor duties, not separate workflow owners.

For nonhuman participants or communication topologies, use [interaction patterns](interaction-patterns.md). The patterns below describe responsibility splits; a tool, queue or document is not an independent checker merely because it has a separate name.

| Pattern / metaphors | Actor duties and handoff | Choose when | Procedure, evidence, and limit |
|---|---|---|---|
| Evidence lanes — Researcher + research assistant | Lead actor frames sources; assistant actor returns primary anchors, applicability, and counterexamples | Sources split cleanly | [Independent evidence lanes](catalog.md#execution-patterns); lead rereads deciding source; remain single actor for a bounded fact |
| Runtime surgery — Surgeon + operating assistant | Code/architecture actor proves and changes boundary; runtime actor controls process, asserts identity, replays scenario | Running behavior decides validity | [Runtime surgery](runtime-surgery.md); runtime observation and loaded identity; no source edits by runtime actor |
| Acceptance inspection — Builder + inspector; Builder + adversarial tester; Editor + fact-checker | Builder actor implements/writes; inspector derives contract edge checks before receiving conclusion | Shared interface, failure behavior, or costly factual error | [Independent acceptance](catalog.md#execution-patterns); preserve the oracle and source anchors; no split for a tiny correction |
| Distinguishing check — Detective + forensic analyst; Scientist + replicator | Investigator states cause and strongest rival; analyst returns a discriminating check or comparable replication | Competing causes or performance claim | [Hypothesis checking](catalog.md#execution-patterns); preserve disproof; sealed evaluations stay sealed and use `octocode-agentic` |
| Traversal — Cartographer + field scout | Orchestrator may assign bounded inventory/trace evidence only for separable paths | Broad scope with authorized separable work | [Independent evidence lanes](catalog.md#execution-patterns); explicit single-actor BFS/DFS never fans out |
| Decision challenge — Advocate + challenger + adjudicator | Case actors develop competing evidence; orchestrator applies criteria and asks for deciding check | Consequential unresolved tradeoff | [Independent evidence lanes](catalog.md#execution-patterns); agreement is not proof; no automatic third worker |
| Stateful cross-check — Pilot + copilot; Incident commander + responder | Action actor owns a transition; checker actor confirms target, prerequisites, durable effect, and replay conditions | Current state or interruption changes the next action | [Stateful procedure](catalog.md#execution-patterns); timeout is unknown, inspect state before replay |
| Handoff — Relay team | Outgoing actor sends a Handoff packet: state, evidence anchors, constraints (authority), next action (resume), row as next-call id; incoming actor reconciles before acting | Capability/context boundary changes | [Composition](composition.md#compose-and-resume); delivery is not completion |
| Resource flow — Agentic plumber + flow tester | Plumber actor traces resource path; tester actor independently replays correctness/measurement when useful | Waste, stale context, cache behavior, or runtime flow is suspected | [Resource-flow audit](resource-flow.md); no metric-only conclusion or unnecessary pairing |

Reject every pattern if its second role lacks a distinct check or decisive evidence. The orchestrator records that limitation instead of role-playing coverage.
