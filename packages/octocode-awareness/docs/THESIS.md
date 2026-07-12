# The Self-Improving Repo

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

Its purpose is not to accumulate the most memories, tasks, skills, or wiki pages.
It keeps observable workspace pressures inside useful ranges while preserving
evidence, user authority, and current source/tests.

## Why Homeostasis

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

## Where Awareness Sits

```text
Agent = Model + Harness
Harness = policy + tools + context + state + permissions + verification
Artifact = the code, docs, tests, plans, and decisions the agent produces
```

Awareness improves the harness around a repository; it does not train model
weights. Better artifacts are the outcome to measure. A memory or skill change is
not improvement until a later task demonstrates better behavior without a safety,
quality, or token regression.

## Control Contract

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
| **Projection pressure** | manifest budgets, missing refs, stale timestamps | bounded optional file view | `wiki sync` when file readers need it | SQLite stays canonical; generated wiki is not default live state |
| **Harness pressure** | recurring failure signatures, evals, developer review | fewer repeated failures with stable trigger and token metrics | export proposal, human apply, held-out review | no silent skill/AGENTS mutation or automatic acceptance |

Targets are ranges, not immortal constants. A busy migration may justify more
coordination detail; a routine edit should stay nearly silent. Any target change is
a reviewed product decision, not a drive invented by the system.

## The Four Coupled Loops

1. **Work:** sense live state, choose a Task or standalone Work, declare every edited
   path, coordinate overlap, act, verify.
2. **Learning:** reflect only reusable outcomes, route each result to an owner,
   apply it, verify again, and close the same row.
3. **Metabolism:** inspect pressure, replay failures/handoffs, preview digest/prune/
   forget, apply only reviewed cleanup, then remeasure. There is no `sleep` command.
4. **Projection:** publish a bounded `.octocode/` snapshot only when file readers
   need it; live SQLite remains the operational source.

These loops are event-driven. Awareness has no background mind, daemon, survival
goal, or self-directed purpose. Optional hooks are reflexes around host events, not
an autonomous agent.

## One Organ, Qualified

“Awareness belongs to the repo” means compatible agents using the same canonical
database home and normalized workspace can share Plans, Tasks, file presence,
signals, verification, and memory. This supports one agent across sessions and many
agents across hosts on the same machine. It is not network replication or a claim
that every host automatically loads the skill or wiki.

The layers have distinct jobs:

| Layer | Job |
|---|---|
| Skill description | Trigger on concrete repository work. |
| Skill lobby | Teach the short operating policy. |
| Hooks | Observe supported events and automate bounded reflexes. |
| CLI/library | Apply explicit state transitions and queries. |
| SQLite | Preserve complete canonical state. |
| `.octocode/` | Project capped leads and authored plan narrative. |
| Human + tests | Authorize risky changes and decide whether the loop improved artifacts. |

## Non-Claims

- “Living” does not mean sentient, conscious, emotional, or entitled to persist.
- A `transactive_map` is a diagnostic map of current shared-state participants and
  sources, not proof of expertise or a complete “who knows what” model.
- Recording a lesson does not guarantee retrieval, application, or improvement.
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
