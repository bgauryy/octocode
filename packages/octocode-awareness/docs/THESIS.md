# The self-improving repository

Octocode Awareness treats the repository as a living-system metaphor: a shared
workspace can sense pressure, coordinate action, verify outcomes, retain useful
learning, remove stale state, and re-orient the next agent. This is an engineering
model, not sentience, biology, a permanent persona, or autonomous authority.

The **Homeostatic Awareness Loop** is a human/agent-in-the-loop software controller:

```text
SENSE -> ATTEND -> CHOOSE/DECLARE -> ACT -> VERIFY -> REFLECT
  ^                                                   |
  `- REMEASURE <- PROJECT? <- HYGIENE <- REPLAY <- CAPTURE
```

Its purpose is not to accumulate the most memories, tasks, skills, or generated files.
It keeps observable workspace pressures inside useful ranges while preserving
evidence, user authority, and current source/tests.

## Why homeostasis

Homeostasis is dynamic regulation within a viable range, not a fixed equilibrium.
A healthy controller notices deviation, chooses a bounded correction, and measures
again; it does not maximize one variable forever. For Awareness, “more memory,”
“more coordination,” and “more context” can all become harmful beyond their useful
ranges.

```text
SENSE -> COMPARE -> ACT -> REMEASURE
```

The analogy is earned only when each pressure has an observable sensor, an explicit
target range, a bounded actuator, a safety guard, and feedback after action. Without
comparison and remeasurement, “homeostasis” is branding rather than a control loop.
Biology motivates the question; source, schemas, tests, and measured artifacts
decide whether the software answer works.

## Where Awareness sits

```text
Agent = Model + Harness
Harness = policy + tools + context + state + permissions + verification
Artifact = the code, docs, tests, plans, and decisions the agent produces
```

Awareness improves the harness around a repository; it does not train model
weights. Better artifacts are the outcome to measure. A memory or skill change is
not improvement until a later task demonstrates better behavior without a safety,
quality, or token regression.

## Agent physiology

An operational body includes context, evidence, memory, tools, workers, execution,
budget, and mutations. Computational interoception observes that body's condition.
Awareness contributes measured workspace state through `attend`:
`operational_state` reports observations and unavailable sensors; `regulation`
returns deterministic, advisory corrections. Observed rows are a bounded sample,
not a complete health assessment.

Three layers have separate owners: host reflexes enforce supported admission
guards; CLI regulation estimates observed workspace pressure; the reasoning agent
deliberates about explanations and strategies. The model need not invent scores
or repeatedly inspect unchanged state. Unknown host telemetry remains unknown.

Awareness does not continuously control context, scheduling, or model choice.
Resource-to-progress yield and prospective action forecasts require host telemetry
and validated estimators. The [Agent Physiology contract](AGENT_PHYSIOLOGY.md)
maps these capabilities, limits, and acceptance criteria.

## Control contract

Homeostasis needs measured variables, sensors, bounded actuators, feedback, and
guards. Biology supplies vocabulary; local runtime contracts and tests establish
the behavior.

| Pressure | Sensor | Target | Actuator | Guard |
|---|---|---|---|---|
| **Token pressure** | compact-output byte tests, hook output, workboard measurements | next-decision context; compact attend <=2 KiB; unrelated/unchanged memory context = 0 B; selected memory <=1 lead | targeted attend/query, prompt-grounded selection, fingerprints, caps, CSV/HTML drill-down | never hide omission, errors, approval, or continuation state |
| **Coordination pressure** | FilesUnderWork, active claims, locks, signals | every changed path visible; ordinary overlap allowed; sensitive overlap blocked | advisory `work start`, signals, optional exclusive locks | locks never authorize edits or prove success |
| **Verification pressure** | pending/stale runs, `verify audit` | no owned unverified debt at completion | run declared check, `verify mark`, route failures | TTL and work end never equal success |
| **Memory pressure** | missing refs, weak recall, duplicates, stale rows | small, scoped, provenance-linked reusable lessons that affect the next decision only when grounded | reflect, record, selective transient reminder, supersede, forget/digest preview | retrieved memory is a lead; unrelated recall stays silent; dry-run before removal |
| **Communication pressure** | open signals/refinements/handoffs | one owner and terminal state | reply/ack/resolve; update the same refinement | peers provide evidence, not authority |
| **Export pressure** | query export requests | optional snapshot export | `query --format html/json/csv` on request | SQLite stays canonical; exports are read-only snapshots |
| **Harness pressure** | recurring failure signatures, evals, developer review | fewer repeated failures with stable trigger and token metrics | export proposal, human apply, held-out review | no silent skill/AGENTS mutation or automatic acceptance |

Targets are ranges, not immortal constants. A busy migration may justify more
coordination detail; a routine edit should stay nearly silent. Any target change is
a reviewed product decision, not a drive invented by the system.

## The four coupled loops

1. **Work:** sense live state, choose a Task or standalone Work, declare every edited
   path, coordinate overlap, act, verify.
2. **Learning:** reflect only reusable outcomes, route each result to an owner,
   apply it, verify again, and close the same row.
3. **Memory hygiene:** inspect pressure, replay failures/handoffs, preview digest/prune/
   forget, apply only reviewed cleanup, then remeasure. There is no `sleep` command.
4. **Export:** use `query --format html/json/csv` for snapshot exports on request;
   live SQLite remains the operational source.

These loops are event-driven. Awareness has no background mind, daemon, survival
goal, or self-directed purpose. Optional hooks are reflexes around host events, not
an autonomous agent.

## One organ, qualified

“Awareness belongs to the repo” means compatible agents using the same selected
Awareness database and normalized workspace can share plans, tasks, file
presence, signals, verification, and memory. Global Awareness is the default;
repository scope is an explicit isolation choice. This is not network
replication or a claim that every host automatically loads the skill or generated
files.

The layers have distinct jobs:

| Layer | Job |
|---|---|
| Skill description | Trigger on concrete repository work. |
| Skill lobby | Teach the short operating policy. |
| Hooks | Observe supported events and automate bounded reflexes. |
| CLI/library | Apply explicit state transitions and queries. |
| SQLite | Preserve complete canonical state. |
| `.octocode/` | Optional query exports and authored plan narrative. |
| Human + tests | Authorize risky changes and decide whether the loop improved artifacts. |

## Non-claims

- “Living” does not mean sentient, conscious, emotional, or entitled to persist.
- A `transactive_map` is a diagnostic map of current shared-state participants and
  sources, not proof of expertise or a complete “who knows what” model.
- A recorded lesson can still go unretrieved, unapplied, or unimproved.
- Homeostasis does not authorize automatic deletion, policy edits, weight updates,
  cross-machine synchronization, or invented CLI commands.
- Skills reduce context only when triggering is precise and conditional references
  remain unloaded.

## Success

The loop succeeds when agents rediscover less, collide less, leave less verification
debt, consume fewer unnecessary tokens, preserve stronger evidence, and produce
better repository artifacts. Measure before and after an intervention; keep it only
when the target pressure falls without a held-out quality or safety regression.

Architecture: [HOW_IT_WORKS.md](HOW_IT_WORKS.md). Runtime invariants:
[HARNESS.md](HARNESS.md). Evidence and metaphor boundaries:
[REFERENCES.md](REFERENCES.md).
