# Runtime state and initialization

> Scope: supported Pi adapter and temporary parity oracle. The native `octocode-agent`
> runtime doesn't use this state. The final cutover deletes it with the Pi package after
> all gates pass.

The Pi extension has one ephemeral session/view store, created with `zustand/vanilla` at
every `session_start`. It is not a second configuration database. SQLite remains canonical
for durable MCP server/tool enablement, server definition files remain canonical for
transport configuration, and `catalog.json` remains the exact derived inventory. The
agent prompt receives a deterministic bounded routing index with enabled server instructions,
tool names, and descriptions. Exact input schemas stay out of the prompt; `MCPTool
action:"describe"` loads one and activates a schema-bound Pi proxy when the host permits it.

## Ownership

`runtime-store.ts` owns initialization phase, task receipts, MCP loading projection,
managed status slots, foreground activity, redacted background-job observations, footer
metrics, and user notices.
`runtime-renderer.ts` subscribes once per session context, diffs rendered values, and is
the only implementation that mutates Pi status or working APIs. Foreground activity text
is projected only by the footer; the renderer derives Pi's motion visibility from activity
without repeating its label. Tool and resource modules publish state through
`setManagedStatus`, `setManagedActivity`, or `publishMcpRuntimeState`.

Foreground work is a separate discriminated Zustand slice: `idle`, `thinking`,
`researching`, `awaiting_input`, `planning`, `reviewing`, `awaiting_start`, `working`,
`verifying`, `blocked`, `complete`, or `failed`. Durable plan state remains authoritative;
the runtime slice is rebuilt from it. Generic turn-level `thinking` is only a fallback and
cannot overwrite a plan lifecycle state. `awaiting_input` hides motion so the decision card
is the sole focus owner.

Resource lifetime stays local to the owning manager. MCP clients, Chrome connections,
worker processes, file queues, timers, schema validators, and filesystem watchers do not
belong in Zustand. Their observable state may be projected into the runtime store.
`SessionRuntime` owns the session abort signal and a last-in, first-out cleanup stack.
Cleanup is idempotent, continues after individual failures, and immediately disposes a
resource registered after shutdown starts. Background Bash publishes only its stable ID,
sanitized title, state, timing, and exit code; commands, cwd values, process handles, and
log paths remain private to its manager.

The footer keeps one register-once Pi component factory because Pi requires `setFooter`
once followed by `requestRender`. It subscribes to runtime changes and disposes branch and
store listeners with the session. No persistent below-editor plan/worker widget is registered.

## Initialization order

`initializeOctocodeSession()` is the sole `session_start` initializer:

1. Dispose a previous renderer binding without touching a stale replacement context.
2. Create and bind the new runtime store.
3. Resolve project trust and propagate Octocode environment configuration.
4. Reset/restore session-scoped policy, plans, metrics, and UI components.
5. Restore the MCP catalog snapshot and start live schema discovery.
6. Start independent background receipts: checkpoints, GitHub auth, update check,
   discovery inventory, Awareness registration, and MCP refresh.
7. Mark the interactive session ready. Background task/MCP state remains visible without
   blocking normal agent work.

Environment propagation precedes MCP configuration and process startup. Non-critical
background failures become degraded task receipts; they do not reject session startup.

## MCP prompt readiness

MCP has separate promises for prompt readiness and live refresh completion. A matching
`catalog.json` supplies the exact inventory used to render the schema-free
`<mcp_catalog_index>`. Exact schemas remain available for validation and
`MCPTool action:"describe"`; selected schemas are exposed through dynamically activated Pi tool
definitions rather than copied into the system prompt. A cold or changed
configuration waits up to 35 seconds for stable first-turn prompt bytes. If the deadline
wins, the late refresh is persisted for the next session.

The renderer shows cache checking, discovery, catalog counts, cached state, and degraded
completion. Initialization emits one aggregate ready notice; stage changes
use managed status and the unified footer instead of notification spam.

## Disposal

`session_shutdown` marks the runtime disposing, suppresses late footer/inbox callbacks,
drains registered managers in reverse acquisition order, then disposes the renderer and store. Replacement-session
shutdown skips UI clears because Pi may already have invalidated that context; quit clears
the live UI. Generation checks prevent late GitHub/MCP results from changing a new session.
