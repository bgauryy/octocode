Load when reviewing the coordinator's architecture or considering an alternative.

# Research basis and design choices

The rules below are Octocode design decisions, not claims that one topology always performs best. Recheck provider documentation before adopting API-specific behavior.

| Primary source | Relevant finding | Application here |
|---|---|---|
| [Anthropic: Building effective agents](https://www.anthropic.com/research/building-effective-agents), 2024-12-19 | Distinguishes predefined workflows from dynamically directed agents; describes orchestrator/workers and evaluator/optimizer patterns. Iterative evaluation needs useful criteria and feedback. | Choose the least coordination the task needs; use repair loops only with an observable acceptance check. |
| [Anthropic: Multi-agent research system](https://www.anthropic.com/research/multi-agent-research-system), 2025-06-13 | Reports benefits for independently searchable branches, limitations for tightly dependent work, checkpointed recovery and passing artifacts by reference. Its measured gains concern its own research evaluation. | Partition evidence questions, preserve source references and resume existing work. Do not generalize reported performance to repository editing or mandate a worker count. |
| [LangGraph: Persistence](https://docs.langchain.com/oss/python/langgraph/persistence) | Separates thread checkpoints from longer-lived stored knowledge. | Preserve execution state needed to resume; do not confuse remembered claims with current evidence. |
| [LangGraph: Fault tolerance](https://docs.langchain.com/oss/python/langgraph/fault-tolerance#retry-boundaries-and-idempotency) | Retrying execution does not make external effects exactly-once; durable identity and provider reconciliation matter when acknowledgement is lost. | Treat ambiguous writes as unknown, inspect postconditions and reuse supported operation identity before replay. |

## Fit with the existing collection

`octocode-agentic` already owns architecture, execution-policy diagnosis, models, context and communication setup. `octocode-workflows` owns workflow patterns, composition and verification procedures. This skill supplies the coordinator's repeated decision: given current evidence and assignments, what can safely advance acceptance now?

Keep neighbor skills optional so the bundle works alone. Use their domain details when available, and reuse their existing ledger rather than creating competing state. Shared message formats and evidence rules come from `skills/common`; generated copies keep the standalone bundle complete.

| Alternative | Fit and tradeoff |
|---|---|
| Put everything in `octocode-agentic` | Keeps one agent-design entry, but mixes designing a harness with operating a live assignment queue. Retain it as the diagnosis owner. |
| Make another workflow catalog | Duplicates `octocode-workflows`. Route there for pattern selection; keep this entry about execution decisions. |
| Require a fixed planner/researcher/builder/reviewer team | Provides predictable roles but adds work even when tasks are coupled or one actor suffices. Assign duties from actual dependencies and evidence needs. |
| Permit unrestricted peer delegation | Allows local initiative but obscures scope, writers and final responsibility. Keep one coordinator accountable; workers return evidence and scope requests. |
| Restart every failed round | Simple scheduling can repeat valid work and unknown effects. Reconcile current state and rerun only work whose evidence is missing or stale. |

The [behavioral checks](checks.md) exercise these decisions. Structural validation cannot establish automatic activation, production reliability or a performance improvement.
