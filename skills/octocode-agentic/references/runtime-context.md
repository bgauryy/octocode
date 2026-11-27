Load when a prompt is assembled at runtime and an edit has no effect, or root and worker see different instructions.

# Runtime context

- Each source proves only its step: manifest = intent, lockfile = selection, installed build = bytes, runtime trace = what this run loaded, serialized request = what crossed the boundary. Re-resolve after any install, build, entrypoint, environment, adapter, or host-config change.
- Untrusted package: read its version from docs; never execute it.
- Trace `source → load → composition order → hooks/middleware/nodes → state/checkpoint/store → compaction → serialization → visible input → retained effect`; inspect ownership, visibility, lifetime or accounting where it can explain the failure; avoid an inventory of unaffected edges.
- Runtime context is model-visible only once a projecting code path is proven.
- Inspect the host's own loader: skill discovery precedence, Pi `before_agent_start` and worker grants, LangChain middleware order, LangGraph reducers and checkpointer `thread_id`, MCP negotiated version and client filtering.
- Symptom → likely boundary (observe the last model or tool boundary first):
  - Edit has no effect → wrong copy, stale build, loader precedence, hook replacement, state path, serializer, cache.
  - Wrong after continuation → retained messages, reducer, checkpoint, summary, compaction.
  - Root and worker differ → grant, worker packet, base prompt, model overlay.
  - Tool sees a value the model does not → runtime context never projected.
  - Resume repeats work → missing checkpoint, wrong thread, lossy handoff.
- Fix: capture the redacted effective boundary, change the smallest owning layer, rerun the same trace (cold call, next turn, failure, resume, compaction, root and worker). Never bloat the base prompt to hide a loader bug.
- Record: running package, version, entrypoint; flow; visibility, lifetime; observed input (redacted, or why unavailable); owner to change.
