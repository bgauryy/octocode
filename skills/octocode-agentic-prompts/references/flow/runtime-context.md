# Runtime context flow: surface first

Load when a host, framework, skill loader, middleware, graph, or dependency may change what reaches a model or tool. Editing prompt text is a no-op when another copy, hook, state path, or serializer owns the effective context: observe the last model or tool boundary first.

## Resolve what is running

| Evidence | Proves | Does not prove |
|---|---|---|
| Manifest | intended range or source | installed or active version |
| Lockfile | selected artifact | that the process loaded it |
| Installed module or build | bytes available | active entrypoint, config, process |
| Runtime resolution or trace | package, version, entrypoint, adapter, config of this run | model-visible request |
| Serialized request or effective-prompt trace | what crossed the boundary | provider-private transforms |

- Re-resolve after any install, build, entrypoint, environment, adapter, or host-config change.
- Never execute an untrusted package to learn its version. Read the active version's docs.

## Map the path

`source → load → composition order → hooks/middleware/nodes → state/checkpoint/store → compaction → serialization → visible input → retained effect`

At each edge record owner, visibility (model/tool/runtime/human), lifetime (call/run/thread/cross-thread), mutation, trust, and accounting (token-bearing, cache-key-bearing, persisted only). Runtime context is not model-visible until a projecting code path is proven.

| Surface | Inspect |
|---|---|
| Agent Skill | active host and copy, discovery precedence, catalog, loaded `SKILL.md`, on-demand references |
| Pi or a Pi-derived host | package or fork and version, system/append prompt, tools, context files, skills, hooks such as `before_agent_start`, session branch, compaction, root vs. worker grants |
| LangChain | `create_agent` inputs, middleware order, dynamic prompt/tools/model, `context_schema`, state, store, summarization |
| LangGraph | compiled graph, schemas, reducers, nodes, `context_schema`, checkpointer and `thread_id`, interrupts, resume path |
| Direct API or MCP client | messages, tool definitions, response schema, negotiated MCP version, client filtering, cache controls |

| Symptom | Likely boundary |
|---|---|
| Source edit has no effect | wrong copy, stale build, loader precedence, hook replacement, cache |
| Wrong after continuation | retained messages, reducer, checkpoint, summary, compaction |
| Root and worker differ | grant, worker packet, base prompt, model overlay |
| Tool sees a value, model does not | runtime context never projected |
| Resume repeats work | missing checkpoint, wrong thread, lossy handoff |

## Optimize

Capture the redacted effective boundary, change the smallest owning layer, then rerun the same trace (cold call, next turn, failure path; resume, compaction, root/worker). Never bloat the base prompt to hide a loader, reducer, or serializer bug. Record: `Surface | Running (package + version + entrypoint/config) | Flow | Visibility/lifetime | Observed input (redacted, or unavailable + reason) | Owner to change`.

Next: occupancy `../context/context-budget.md`; tool definitions `../tools/tool-contracts.md`.
