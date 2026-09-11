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

## Homeostasis And Collective Memory

- **Homeostasis — adjacent prior art:** physiological homeostasis is dynamic regulation around
  viable ranges, not a frozen set point. Billman's
  [Homeostasis: The Underappreciated and Far Too Often Ignored Central Organizing Principle of Physiology](https://pubmed.ncbi.nlm.nih.gov/32210840/)
  informs the sensor/target/actuator/guard vocabulary. Awareness applies that
  vocabulary only to measurable software state.
- **Adjacent prior art:** allostasis emphasizes anticipatory adjustment under
  changing demand; see
  [Allostasis: a model of predictive regulation](https://pubmed.ncbi.nlm.nih.gov/31488322/).
  Awareness does not infer biological drives or let predictions authorize edits.
- **Adjacent prior art:** stigmergy explains indirect coordination through a shared
  environment; see
  [A Brief History of Stigmergy](https://direct.mit.edu/artl/article/5/2/97/2318/A-Brief-History-of-Stigmergy).
  SQLite rows and file presence are an engineered coordination medium, not proof of
  collective intelligence.
- **Adjacent prior art:** transactive-memory research studies knowing where group
  knowledge resides; see
  [Transactive memory systems, learning, and learning transfer](https://pubmed.ncbi.nlm.nih.gov/12940401/).
  Awareness's `transactive_map` reports current shared-state IDs and freshness; it
  is not an expertise model.
- **Adjacent prior art:** sleep-related consolidation and adaptive forgetting
  motivate preserving verified knowledge while removing obsolete state; see
  [Memory consolidation during sleep](https://www.nature.com/articles/s41593-019-0467-3)
  and [Adaptive Forgetting in Humans](https://pubmed.ncbi.nlm.nih.gov/28641107/).
  Maintenance remains explicit, preview-first, and supervised.

These analogies support the bounded controller in [THESIS.md](THESIS.md). They do
not make a repository alive, sentient, self-authorizing, or self-governing.

## Local Store And Concurrency

- **Implemented invariant:** SQLite is the canonical local store; write lifecycles
  use transactions and explicit conflict handling. SQLite documents transaction
  behavior in [Transaction](https://sqlite.org/lang_transaction.html).
- **Implemented invariant:** WAL is enabled only on classified-safe embedded SQLite
  versions; affected versions use rollback journaling. SQLite documents WAL
  concurrency, same-host limits, checkpointing, and the WAL-reset bug in
  [Write-Ahead Logging](https://sqlite.org/wal.html).
- **Implemented invariant:** the package requires Node.js
  `^24.15.0`, where `node:sqlite` is available without
  the experimental flag. See the
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
- **Adjacent prior art:** [Beads](https://github.com/gastownhall/beads) uses a
  dependency-aware issue graph for coding-agent work. It is useful task/claim
  vocabulary, not validation of Awareness's SQLite schema, advisory file presence,
  or verification lifecycle.

Mandatory advisory file presence, optional sensitive-file exclusivity, one durable
Task queue, and authored plan documents are Octocode design choices—not claims
copied from either source.

## Concurrent Agents As A Distributed System

Parallel agents over one workspace are multiple writers without a shared clock, so
they inherit classical coordination failures. These sources supply vocabulary and
failure classes; none of them validates Awareness's lifecycle.

- **Adjacent prior art:** a practitioner account of running parallel Claude Code
  instances reports a lost update between linked worktrees — two agents created the
  same migration filename with different schemas and the second silently overwrote
  the first — and argues that shared artifacts in current multi-agent frameworks
  carry "no formal concurrency control. No fault model." See
  [Multi-Agent Systems Have a Distributed Systems Problem](https://christophermeiklejohn.com/ai/agents/distributed/zabriskie/2026/03/30/multi-agent-systems-have-a-distributed-systems-problem.html).
  It motivates advisory presence, exclusive leases, and one canonical ledger. It does
  not establish that Awareness detects divergence or merges conflicting writes.
- **Adjacent prior art:** a reported defect in another coding-agent product records the
  same failure without any parallelism: agents editing one file in sequence, where a
  later agent "can silently overwrite the file with a stale version," and argues a
  write should never be able to clobber content modified after the writer's initial
  read. See [claude-code issue #43138](https://github.com/anthropics/claude-code/issues/43138).
  It shows that a lease covering only the write window is insufficient, which is why
  [GIT_COORDINATION.md](GIT_COORDINATION.md) requires re-reading immediately before a
  write. No local test proves Awareness detects a stale read taken before a lease.
- **Adjacent prior art:** [Time, Clocks, and the Ordering of Events](https://dl.acm.org/doi/10.1145/359545.359563)
  defines happened-before ordering without a shared clock. Awareness orders writes
  through SQLite transactions and records observed state per agent; it does not
  implement vector clocks and cannot prove that one agent had seen a peer's earlier
  change before making its own.
- **Adjacent prior art:** [Conflict-Free Replicated Data Types](https://hal.inria.fr/inria-00555588)
  describe structures that merge without coordination. Awareness coordinates instead
  of converging: it prevents overlapping edits by lease and message rather than
  merging divergent copies, so CRDT guarantees do not transfer to file contents.
- **Adjacent prior art:** [MAST](https://arxiv.org/abs/2503.13657) derives fourteen
  multi-agent failure modes across specification, inter-agent misalignment, and task
  verification, and reports self-verified success without ground truth. It supports
  keeping verification separate from the agent that did the work; its benchmark
  results describe other frameworks, not this runtime.
- **Adjacent prior art:** a coding-agent maintainer discussion of
  [entity-level merge for parallel edits](https://github.com/openai/codex/discussions/13557)
  observes that "Git's line-based merge creates false conflicts when two agents add
  different functions to the same file." Awareness's physical-file leases avoid the
  interleaving rather than resolving it; coarse leases can serialize work that was
  independent.
- **Follow-on hypothesis:** treating a peer agent as a Byzantine participant — plausible
  output that passes its own checks and is then built upon — extends the existing
  "generated text is a lead" boundary to peer results. Awareness already requires
  independent verification, but no local test proves that a wrong peer result is
  contained before a dependent agent consumes it.

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
- **Adjacent prior art:**
  [Lost in the Middle](https://aclanthology.org/2024.tacl-1.9/) finds that relevant
  information can be used less reliably when buried in long context. It supports
  progressive disclosure and next-decision packets, not a universal byte limit.
- **Adjacent prior art:** [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
  supports stable-prefix cost reduction. Awareness's stable skill/schema layer can
  benefit when a host caches it, but the CLI neither controls nor guarantees cache
  hits.
- **Adjacent prior art:** the [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
  motivates explicit schemas and structured errors. Awareness exposes its own CLI
  schemas; this does not make the CLI an MCP server.

## Memory, Reflection, And Provenance

- **Adjacent prior art:**
  [Remember When It Matters](https://arxiv.org/abs/2607.08716v1) frames
  behavioral-state decay as remembered execution state losing influence and reports
  that maintained memory plus a selective grounded reminder/silence policy is more
  balanced than passive full-bank exposure or advisor-only guidance. This motivated an earlier
  scoped memory selector. Default hooks now deliver peer messages only; memory
  recall is explicit when prior learning can change the approach. It is
  not the paper's separate LLM memory agent, fixed-interval trajectory observer, or
  trained policy, so its benchmark gains do not transfer. The paper's linked
  [code repository](https://github.com/yifannnwu/proactive-memory-agent) contained no
  implementation files when evaluated on 2026-07-10, limiting independent
  reproduction of the reported mechanism.
- **Implemented invariant (local evidence):** `maintenance.test.ts`,
  `host-hook-safety.test.ts`, and the focused hook integration suites freeze the
  deterministic selector and delivery lifecycle. Local checks hold one memory lead,
  prompt non-persistence, five UTF-8 items within 1 KiB, and the SQLite application
  identity. Strict config health does not prove host runtime execution or trust.
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

Generated output and reflected text remain leads because none of these papers makes retrieved or
reflected text authoritative. Current user instructions, source, and tests win.

- **Adjacent prior art:** a systematic study of
  [memory poisoning in LLM agents](https://arxiv.org/abs/2606.04329) treats durable
  writes and later retrieval as an attack surface. It supports the “memory is a
  lead” safety boundary, but does not establish that Awareness prevents poisoning.
- **Adjacent prior art:**
  [AgentPoison](https://proceedings.neurips.cc/paper_files/paper/2024/hash/eb113910e9c3f6242541c1652e30dfd6-Abstract-Conference.html)
  demonstrates that an agent's long-term memory or knowledge base can become an
  attack surface. This reinforces provenance, scoped recall, and verification; it
  is not evidence that local SQLite alone is safe.

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
- **Follow-on hypothesis:** [Self-Harness](https://arxiv.org/abs/2606.09498)
  studies verifier-grounded weakness mining and propose/evaluate/accept harness
  changes. Awareness can test this pattern, but does not auto-accept its own code or
  instruction mutations.
- **Adjacent prior art:** Lilian Weng's
  [Harness Engineering for Self-Improvement](https://lilianweng.github.io/posts/2026-07-04-harness/)
  is a secondary synthesis of persistent artifacts, explicit subagents, context
  control, and verifier-backed improvement. It is a map to primary work, not
  independent production evidence.
- **Not production evidence:** [Self-Rewarding Language Models](https://arxiv.org/abs/2401.10020)
  and [SPIN](https://arxiv.org/abs/2401.01335) concern model training. They do not
  justify agents applying their own code, skill, or instruction changes. Awareness
  keeps proposals human-gated and separately verified.

## Interpretation Rule

References support a design question, limitation, or test hypothesis. They never
replace repository evidence, authorize writes, prove safety, or transfer benchmark
results to Awareness. New sources should state their evidence class and the exact
boundary of the claim they support.
