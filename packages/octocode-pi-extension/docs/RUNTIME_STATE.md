# Runtime state and initialization

> Scope: supported Pi adapter and temporary parity oracle. The native `octocode-agent`
> runtime doesn't use this state. The final cutover deletes it with the Pi package after
> all gates pass.

The Pi extension has one ephemeral session/view store, created with `zustand/vanilla` at
every `session_start`. It is not a second configuration database. SQLite remains canonical
for durable MCP server/tool enablement, server definition files remain canonical for
transport configuration, `catalog.json` remains the exact derived inventory, and
`mcp.md` is the default derived prompt cache. Set `OCTOCODE_COMPACT_MCP=0` only
to debug with the exact catalog in provider context.

## Ownership

`runtime-store.ts` owns initialization phase, task receipts, MCP loading projection,
managed status slots, working visibility/message, and user notices. `runtime-renderer.ts`
subscribes once per session context, diffs rendered values, and is the only implementation
that mutates Pi status or working-message APIs. Tool and resource modules publish state
through `setManagedStatus`, `setManagedWorking`, or `publishMcpRuntimeState`.

Foreground work is a separate discriminated Zustand slice: `idle`, `thinking`,
`researching`, `awaiting_input`, `planning`, `reviewing`, `awaiting_start`, `working`,
`verifying`, `blocked`, `complete`, or `failed`. Durable plan state remains authoritative;
the runtime slice is rebuilt from it. Generic turn-level `thinking` is only a fallback and
cannot overwrite a plan lifecycle state. `awaiting_input` hides motion so the decision card
is the sole focus owner.

Resource lifetime stays local to the owning manager. MCP clients, Chrome connections,
worker processes, file queues, timers, schema validators, and filesystem watchers do not
belong in Zustand. Their observable state may be projected into the runtime store.

The footer and below-editor panel keep register-once Pi component factories because Pi
requires `setFooter`/`setWidget` once followed by `requestRender`. Their lifecycle is
started and disposed by the same session hooks; their component-specific caches remain
local.

## Initialization order

`initializeOctocodeSession()` is the sole `session_start` initializer:

1. Dispose a previous renderer binding without touching a stale replacement context.
2. Create and bind the new runtime store.
3. Resolve project trust and propagate Octocode environment configuration.
4. Reset/restore session-scoped policy, plans, metrics, panels, and UI components.
5. Restore MCP prompt cache and start live schema discovery.
6. Start independent background receipts: checkpoints, GitHub auth, update check,
   discovery inventory, Awareness registration, and MCP refresh.
7. Mark the interactive session ready. Background task/MCP state remains visible without
   blocking normal agent work.

Environment propagation precedes MCP configuration and process startup. Non-critical
background failures become degraded task receipts; they do not reject session startup.

## MCP prompt readiness

MCP has separate promises for prompt readiness and live refresh completion. By default,
prompt readiness consumes matching `catalog.json` + `mcp.md`; exact schemas remain
private for invocation validation. With `OCTOCODE_COMPACT_MCP=0`, a matching persisted
`catalog.json` supplies `<mcp_catalog>` with enabled descriptions and schemas. Exact schemas refresh
privately for execution and the next session. A cold or changed configuration waits up to
35 seconds for stable first-turn prompt bytes. The compact guide is deterministic by
default; `OCTOCODE_MCP_AI_GUIDE=1` opts into model-authored guide generation.
If the deadline wins, the late refresh is persisted for the next session.

The renderer shows cache checking, discovery, guide optimization, counts, cached state,
and degraded completion. Initialization emits one aggregate ready notice; stage changes
use the persistent status/working surfaces instead of notification spam.

## Disposal

`session_shutdown` marks the runtime disposing, suppresses late panel/inbox callbacks,
stops resource managers, then disposes the renderer and store. Replacement-session
shutdown skips UI clears because Pi may already have invalidated that context; quit clears
the live UI. Generation checks prevent late GitHub/MCP results from changing a new session.
