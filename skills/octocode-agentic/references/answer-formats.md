Load when shaping a chat answer: a response lead, a design answer or audit findings.

# Answer formats

## Response

BLUF: lead with the decision, verdict or outcome and its confidence. Then give what it rests on and the next check.

| Work | After the lead |
|---|---|
| Design or audit | The layer it affects (context, tools, loop, routing, communication, trust), evidence, trade-offs, limits |
| Prompt or instruction | The prompt in the requested format. For an edit: a full replacement, a change summary and verification. For a review: the affected text, its behavioral effect and a repair. Ready-to-paste task prompts include the deciding checks from [question selection](question-selection.md) |
| Measurement | The question and checks, then for a comparison: baseline and candidate conditions (scope, budget), metrics, guardrails, uncertainty, coverage, failures, cost |
| Coordination | Any pending conflict and who must act; identifiers per [Pending](report-sections.md#pending) |

## Answer size

Match the content to who or what consumes the answer:

| Destination | Give |
|---|---|
| Human with a simple question | Direct, concise answer |
| Developer debugging | Root cause, evidence, fix, tests |
| Calling agent | Structured result with references |
| Build pipeline | Machine-readable status and artifacts |
| Architecture reviewer | Findings, impact, evidence, recommendations |
| Incident report | Timeline, cause, impact, corrective actions |
| Autonomous workflow | State transition, next action, blockers |

Shorten by removing repetition and process narration, never by dropping material uncertainty or required proof.

## Design answer

1. **Recommendation.** State the architecture or contract in one sentence, with the workload assumption it rests on.
2. **Why.** Name the deciding trade-offs: correctness, cost, latency, context size, operational risk. Label each number measured, documented or estimated.
3. **Alternatives.** Name the simpler option considered (one loop, one model, one tool) and when a richer one wins.
4. **Failure modes.** Say how it could loop, flood context, drift after compaction, repeat a side effect or cross a trust boundary, and how each is detected and stopped.
5. **Check.** Give the task set, trace, contract test or metric that confirms it, with its pass condition.

Include only the parts that change the decision, and only failure modes reachable for this design.

For a contract (tool response, handoff, message envelope), show the typed shape, one valid example and one rejected example.

## Audit findings

Order findings worst first. Give each its location (file, tool, prompt section or trace step), symptom, mechanism, consequence, fix, and the check that proves the fix. Keep proven defects, observed in a trace or run, apart from candidates inferred from configuration or source alone. List the checklist items you could not assess, and why.
