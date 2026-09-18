# Runtime context flow: surface first

Load when a host, framework, skill loader, middleware, graph, or dependency may change what reaches a model or tool. Why: editing declared prompt text is a no-op when another runtime copy, hook, state path, or serializer owns the effective context.

**Surface first.** Identify the executing runtime and observe its last available model/tool boundary before counting tokens, changing prose, or blaming model behavior.

This reference owns runtime discovery and context provenance. `../context/context-budget.md` owns capacity and occupancy, `../context/prompt-caching.md` owns cache behavior, `../context/compaction.md` owns cut/keep decisions, and `../agents/agent-prompt-integrity.md` owns frozen-base verification.

## Resolve what is actually running

Keep these states distinct:

| Evidence | What it proves | What it does not prove |
|---|---|---|
| Manifest or project declaration | intended dependency range or source | installed or active version |
| Lockfile/resolution | selected artifact for that install | process loaded that artifact |
| Installed module or build output | bytes available to load | active entrypoint, config, or process identity |
| Runtime resolution, process metadata, or trace | package, version, entrypoint, adapter, and configuration used by this run | exact model-visible request unless the boundary is captured |
| Serialized request or supported effective-prompt trace | what crossed the observable boundary | provider-private transformations after that boundary |

Record the runtime-resolved host/framework and version, language/runtime, provider adapter and model, entrypoint, active configuration, environment/process identity, and serializer. Re-resolve when a lockfile, package install, build artifact, entrypoint, environment, provider adapter, or host configuration changes. Do not execute an untrusted package merely to discover its version; prefer supported introspection, traces, loaded-module metadata, or exact build provenance.

## Map the whole context path

Trace:

```text
source → discovery/load → composition order → hooks/middleware/nodes
       → state/checkpoint/store → pruning/compaction
       → provider/tool serialization → visible input → retained effect
```

At each edge record:

- **Owner** — the source or component authorized to change the value.
- **Visibility** — model-visible, tool-only, runtime-only, or human-only.
- **Lifetime** — one call, one run, one thread/session, or cross-thread.
- **Mutation** — immutable, replaced, appended, reduced, summarized, or externally updated.
- **Authority/trust** — instruction, configuration, state, retrieved data, or untrusted content.
- **Accounting** — token-bearing, cache-key-bearing, persisted but not serialized, or out of band.

Capture the last observable input before the provider or tool call, with secrets and private data redacted. Diff source versus effective context and name any unobservable downstream boundary. Runtime context or dependency injection is not automatically model-visible: prove the code path that projects it into instructions, messages, tools, or response format.

## Surface checks

| Surface | Inspect before optimizing |
|---|---|
| Agent Skill | Active host and skill copy; discovery roots and precedence; metadata catalog; activation path; loaded `SKILL.md`; references/scripts loaded on demand; host-specific deviations from the Agent Skills specification. Metadata, full instructions, and resources have different load times. |
| Pi or a Pi-derived host | Exact package/fork and version; built/custom/append system prompt; selected tool definitions; project context files; skill catalog and activation; extension hooks such as `before_agent_start`; session branch; compaction; provider/model selection; root-versus-worker grants and overlays. Use the host's supported effective-prompt/session inspection when available. |
| LangChain agent | Language and package versions; `create_agent` inputs; model request; middleware order; dynamic prompt/messages/tools/model/response format; runtime `context_schema`; state; store; summarization and guardrails. Separate transient model context from persistent tool/lifecycle writes. |
| LangGraph graph | Compiled graph identity; state/input/output/private schemas; reducers; nodes and conditional edges; runtime `context_schema`; checkpointer and `thread_id`; store; interrupts, pending writes, resume/replay path, and the node that makes each model call. Test migrated graphs against existing interrupted threads and saved state. |
| Direct provider, MCP, or tool client | Developer/system/user messages; tool definitions; response schema; provider settings; negotiated MCP version; server instructions and discovery lifecycle; client-side filtering; serialization; cache controls and usage telemetry. |

These are inspection prompts, not universal implementation claims. Frameworks and forks change. Resolve the active version, then use documentation and source for that version instead of hard-coding today's API into a generic prompt.

## Failure localization

| Symptom | Likely boundary | Proof to seek |
|---|---|---|
| Source edit has no effect | wrong copy, stale build, loader precedence, hook replacement, or cached artifact | active path/build digest and source-to-effective diff |
| Correct on the first turn, wrong after continuation | retained messages, state reducer, checkpoint, summary, or compaction | next-turn and resumed serialized inputs |
| Root and worker behave differently | capability grant, worker packet, base prompt, or provider/model overlay | effective inputs and grants for both agents |
| Tool can access a value but model cannot | runtime/tool context was never projected into model context | middleware/node/tool path and actual model request |
| Context or cost differs across equivalent runs | model/provider adapter, serializer, tools, history, cache policy, or retry path | matched cold/warm traces and usage buckets |
| Resume repeats completed work | missing checkpoint, wrong thread identity, pending-write semantics, or lossy handoff | saved state, resume target, and execution attempt identity |

## Optimize and validate

1. Classify the surface and resolve the running dependency.
2. Draw the context path and mark visibility, lifetime, mutation, and authority.
3. Capture and redact the effective boundary; state evidence limits when capture is unavailable.
4. Change the smallest owning layer. Do not compensate for a loader, reducer, or serializer bug by bloating the base prompt.
5. Compare a cold first call, next turn, and failure/retry path. Add resume/checkpoint, compaction, root/worker, or warm-cache cases only when the surface supports them.
6. Re-run the same trace after the change and verify the intended delta appears once, at the right layer, with frozen bytes and unrelated state unchanged.

Use this evidence block in reviews:

```markdown
Surface: <skill/Pi/LangChain/LangGraph/direct API/MCP/other>
Running dependency: <package/fork + resolved version + entrypoint/config>
Context flow: <source -> ... -> effective boundary -> retained effect>
Visibility/lifetime: <model/tool/runtime + call/run/thread/cross-thread>
Observed input: <trace/artifact, redacted; or unavailable + reason>
Owner to change: <smallest authoritative layer>
Unresolved boundary: <none or explicit evidence gap>
```

## Sources

- Agent Skills, [Specification](https://agentskills.io/specification) — metadata, activated `SKILL.md`, and on-demand resources are progressively disclosed; host support can vary.
- Pi, [system prompt construction](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/system-prompt.ts), [extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md), and [compaction](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/compaction.md) — prompt composition, agent-start hooks, session context, and compaction are distinct runtime layers.
- LangChain, [Context engineering](https://docs.langchain.com/oss/python/langchain/context-engineering) and [Runtime](https://docs.langchain.com/oss/python/langchain/runtime) — model, tool, lifecycle, runtime, state, and store context have different visibility and lifetimes.
- LangGraph, [Graph API](https://docs.langchain.com/oss/python/langgraph/graph-api) and [Persistence](https://docs.langchain.com/oss/python/langgraph/persistence) — reducers, runtime context, checkpoints, threads, pending writes, and stores shape execution and resumption.

Next: after the effective request is known, use `../context/context-budget.md` for occupancy, `../context/prompt-caching.md` for repeated prefixes, or `../context/compaction.md` for retention. For tool/MCP definitions load `../tools/tool-contracts.md`; for frozen bases or root/worker overlays load `../agents/agent-prompt-integrity.md`; for untrusted projected values load `../context/untrusted-content.md`.
