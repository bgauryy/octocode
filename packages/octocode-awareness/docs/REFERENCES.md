# Design References And Boundaries

This is the evidence owner for Octocode Awareness. It maps sources to design
choices without implying that prior work proves this implementation correct.
Runtime behavior is defined by source, schemas, tests, and the other package docs.

## Evidence Classes

| Class | Meaning |
|---|---|
| Implemented invariant | Source or specification informed shipped behavior that is covered by local tests. |
| Adjacent prior art | A useful comparison or vocabulary; Awareness does not claim compatibility or equivalent results. |
| Follow-on hypothesis | Research worth testing later; it is not production justification. |

## Local Store And Concurrency

- **Implemented invariant:** SQLite is the canonical local store; write lifecycles
  use transactions and explicit conflict handling. SQLite documents transaction
  behavior in [Transaction](https://sqlite.org/lang_transaction.html).
- **Implemented invariant:** WAL is enabled only on classified-safe embedded SQLite
  versions; affected versions use rollback journaling. SQLite documents WAL
  concurrency, same-host limits, checkpointing, and the WAL-reset bug in
  [Write-Ahead Logging](https://sqlite.org/wal.html).
- **Implemented invariant:** the package requires Node.js 22.13.0+, where
  `node:sqlite` is available without the experimental flag. See the
  [Node SQLite API history](https://nodejs.org/download/release/latest-v22.x/docs/api/sqlite.html).

These sources do not select Awareness's schema, task lifecycle, or lock policy;
those are locally tested product decisions.

## Agent Coordination

- **Adjacent prior art:** the [A2A specification](https://a2a-protocol.org/latest/specification/)
  distinguishes stateful tasks, messages, status, and artifacts across opaque
  agents. Awareness similarly separates Tasks, Signals, Runs, and plan documents,
  but it is a same-workspace local runtime and does not claim A2A compatibility.
- **Adjacent prior art:** Anthropic's
  [Building effective agents](https://www.anthropic.com/engineering/building-effective-agents)
  recommends simple composable workflows, selective parallelism, and clear tool
  interfaces. Awareness applies those ideas through explicit lifecycle commands and
  independent review, while its exact coordination policy remains local.

Mandatory advisory file presence, optional sensitive-file exclusivity, one durable
Task queue, and authored plan documents are Octocode design choices—not claims
copied from either source.

## Progressive Disclosure And Token Cost

- **Implemented invariant:** the [Agent Skills specification](https://agentskills.io/specification)
  describes progressive loading of `SKILL.md`, focused references, scripts, and
  assets. Awareness keeps its lobby bounded and routes conditional detail to one
  reference.
- **Implemented invariant:** Anthropic's
  [context-engineering guidance](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
  and [tool-design guidance](https://www.anthropic.com/engineering/writing-tools-for-agents)
  motivate next-decision context, filtering, pagination, and measured tool output.
  Awareness therefore makes `attend --compact` byte-budgeted and keeps bulk data in
  targeted queries, CSV, or HTML.
- **Adjacent prior art:** [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
  supports stable-prefix cost reduction. Awareness's stable skill/schema layer can
  benefit when a host caches it, but the CLI neither controls nor guarantees cache
  hits.
- **Adjacent prior art:** the [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
  motivates explicit schemas and structured errors. Awareness exposes its own CLI
  schemas; this does not make the CLI an MCP server.

## Memory, Reflection, And Provenance

- **Adjacent prior art:** [Reflexion](https://arxiv.org/abs/2303.11366) and
  [Self-Refine](https://arxiv.org/abs/2303.17651) show that linguistic feedback can
  improve later attempts. Awareness stores only verified, reusable synthesis and
  keeps reflection separate from success verification.
- **Follow-on hypothesis:** [NapMem](https://arxiv.org/abs/2607.05794) studies
  structured, provenance-linked memory navigation. It supports testing drill-down
  and multi-granularity retrieval, but its user-memory results do not validate
  coding-workspace memory or learned routing here.
- **Follow-on hypothesis:** [ACE](https://arxiv.org/abs/2510.04618) studies
  incremental context evolution and warns about context collapse. It motivates
  preservation and held-out evaluation, not automatic mutation of Awareness
  instructions.
- **Follow-on hypothesis:** [HOLA](https://arxiv.org/abs/2607.02303) combines a
  compressed recurrent state with a bounded exact cache. This is an analogy for
  retaining exact provenance beside summaries; HOLA is a model architecture, not
  evidence for an agent-memory ranking policy.

Generated wiki files remain leads because none of these papers makes retrieved or
reflected text authoritative. Current user instructions, source, and tests win.

## Skill And Harness Improvement

- **Implemented invariant:** [SkillOpt](https://arxiv.org/abs/2605.23904) treats a
  skill as external agent state and accepts bounded textual edits only after
  held-out improvement. Awareness uses bounded edits, skill review, and held-out
  checks; human authorization remains an additional local safety boundary.
- **Adjacent prior art:** Anthropic's
  [agent-evaluation guidance](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)
  motivates realistic multi-turn scenarios, verifiable outcomes, and cost/tool-call
  metrics. Local tests and smoke flows—not prose quality—decide whether a change is
  accepted.
- **Not production evidence:** [Self-Rewarding Language Models](https://arxiv.org/abs/2401.10020)
  and [SPIN](https://arxiv.org/abs/2401.01335) concern model training. They do not
  justify agents applying their own code, skill, or instruction changes. Awareness
  keeps proposals human-gated and separately verified.

## Interpretation Rule

References support a design question, limitation, or test hypothesis. They never
replace repository evidence, authorize writes, prove safety, or transfer benchmark
results to Awareness. New sources should state their evidence class and the exact
boundary of the claim they support.
