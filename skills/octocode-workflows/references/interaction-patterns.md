Load when designing a practical path for several interacting things, or looking beyond human role metaphors.

# Interaction patterns

These are design analogies, not measured claims about agent performance. Choose the simplest structure that exposes a necessary dependency, ownership boundary or check. Splitting duties across agents follows the [composition contract](composition.md#orchestrator-and-actor-contract). A queue, artifact or service is a participant, not automatically an agent.

## Turn a goal into a path

1. State the outcome as an observable condition, the initial state, constraints and the domain owner. Separate designing the procedure from permission to execute it.
2. Inventory participants by capability: active decision-maker, deterministic executor, state store, transport or passive artifact. Name the real reader/writer for every passive object and who may change each shared resource.
3. Choose a pattern below by what must happen between participants. Reject a metaphor if it adds labels without changing an action, message or check. Combine patterns only at a named boundary, such as pipeline stages using a queue.
4. Describe each handoff with the [handoff contract](handoff-contract.md). Order dependent steps; allow independent work concurrently only when authorized. Mark joins, decisions, cancellation and recovery explicitly.
5. Walk one normal case and one adverse case using concrete inputs. Check a duplicate or reordered delivery, stale evidence, unavailable participant or interruption wherever it can change the result. A thought walkthrough checks the design; it does not establish runtime behavior.
6. Map the path to the [catalog](catalog.md), use [composition](composition.md) for delegated execution and [verification](verification.md) for acceptance. Return the proposed path or the observed result through [output](../output.md).

## Choose by communication structure

| Pattern / metaphor | Participants and exchange | Practical agent scenario | Completion, failure and execution route |
|---|---|---|---|
| Pipeline / assembly line | Producer → transformer → validator → consumer; versioned artifacts cross stage boundaries | Source research yields an evidence packet; a writer creates a guide; a checker tests its examples | Consumer accepts the required representation and provenance. Reject a malformed stage output at its owner. Use bounded execution; independent acceptance only with a distinct checker. |
| Dependency graph / build system | Artifacts declare prerequisites; a scheduler runs ready steps and joins their results | Schema change → generated contract → adapters → interface checks | All required consumers pass against matching inputs. Invalidate downstream evidence after an input change; independent branches may use authorized parallel write lanes. |
| Queue / postal network | Producer submits identified work; queue delivers it; consumer records effect and acknowledgement | Repository events become investigation jobs processed by a tool-using agent | Check the durable outcome, not just delivery. Inspect operation identity before replay after lost acknowledgement; use stateful procedure and handoff. Never assume exactly-once delivery. |
| Shared workspace / blackboard | Specialists read and contribute typed artifacts; one owner selects the next unresolved question | Logs, source anchors and rival hypotheses accumulate in an incident workspace | A distinguishing check resolves the question or leaves an explicit gap. Keep provenance and invalidate stale contributions; use hypothesis checking or independent evidence lanes, with one acceptance ledger. |
| Feedback loop / thermostat | Sensor reports state; controller compares target; actuator changes state; sensor observes again | A test runner observes failure; an agent proposes a fix; the build/runtime owner activates it; replay measures behavior | The stable acceptance condition holds on the intended artifact. Diagnose oscillation or unchanged observations before another change; use runtime surgery or resource flow. Do not invent retry budgets or let the controller weaken its target. |
| Reservation / railway signals | Contenders request ownership; resource owner grants a lease; holder returns evidence and releases it | Agents need the same generated file, process or build output | Only the current authorized holder mutates it; verify current ownership before acting after interruption. Use parallel write lanes and the coordination owner's real lease protocol; a message saying “mine” is not a lock. |
| Reconciliation / gardener | Desired state and observed state produce a plan; an executor applies authorized differences; observer checks again | Source docs and skill bundles drift; an agent identifies owning sources and repairs the requested scope | Required state matches with no unexplained differences. Plan from fresh state, avoid duplicate effects, and retain external changes; use stateful procedure and the domain owner. |
| Compensation / expedition checkpoints | Steps commit separate durable effects; each records a recovery or compensating action | A migration updates a service, consumer and stored data with separate owners | Consumer invariant holds across the transition. On partial failure inspect committed state and choose recovery from it; use stateful procedure or runtime surgery. Compensation may be impossible or require authority; never promise atomic rollback. |
| Competing hypotheses / immune response | Independent detectors report signals; an investigator distinguishes false alarms from real damage | Security signal, parser failure and runtime symptom suggest different causes | A discriminating check supports the action. Correlated detectors are not independent evidence; use hypothesis checking, with the domain owner controlling remedies. |
| Contract negotiation / adapter handshake | Producer advertises supported contract; consumer declares required contract; adapter verifies compatibility | An MCP client, launcher and runtime must agree on fields and representations | A real consumer accepts the intended contract, including denial and partial results. Unknown capability blocks that route; use independent acceptance or a code/package investigation. No silently guessed field conversion. |

## Worked path: artifacts communicating through tools

Request: “Keep a generated API client aligned with its schema and prove consumers still work.”

- **Participants:** schema (passive source), generator (executor), client (artifact), consumer test (observer), agent (controller). The build owner runs generation; a test failure is an observation, not permission to rewrite the schema.
- **Path:** pin schema and consumer inputs → generate from the owning source → check the generated diff → run affected consumer checks → accept matching evidence. Use the dependency-graph pattern with reproduce → fix → verify when an authorized repair is needed.
- **Normal case:** a schema field changes; generated client and consumer adaptation agree; consumer checks pass against that exact generated artifact.
- **Adverse case:** schema changes again during generation. The output no longer proves compatibility with current inputs. Mark dependent evidence stale, reconcile the source and regenerate through the owner before checking again.
- **Limit:** this walkthrough is a proposed procedure. Until its commands run, runtime compatibility remains unverified.
