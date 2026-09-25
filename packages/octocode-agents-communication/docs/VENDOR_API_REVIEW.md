# Vendor messaging API review

Reviewed September 26, 2026. This is a source and architecture review of the
current adapters, informed by the linked, versioned live evaluations. It does not
claim a new live test of every vendor or certify untested vendor upgrades.

The existing-session design is sound: persist identity and messages in SQLite,
then deliver through the recipient's own host. Routing needs no sender model.
Keep the vendor-specific paths: their ownership, wake and receipt contracts are
different. A provider's inference API does not address a conversation already
running in its coding CLI or editor.

## Ratings and scope

Each dimension is 0–5; the overall score is their sum divided by two. These are
engineering assessments, not benchmark measurements or scores for the vendors.
5 means the reviewed implementation fully meets the dimension for its stated
scope; 4 means a good fit with a material qualification; 3 means partial evidence
or a meaningful gap; 2 means substantial restrictions; 1 means largely missing;
0 means absent or contradicted. None implies access to arbitrary host sessions.

| Current adapter | Existing-session fit | Context and wake | Receipt and recovery | API stability | Overall |
| --- | ---: | ---: | ---: | ---: | ---: |
| Codex owning app-server | 4 | 5 | 4 | 4 | **8.5/10** |
| Pi installed extension | 4 | 5 | 4 | 4 | **8.5/10** |
| OpenCode existing HTTP server | 4 | 5 | 4 | 4 | **8.5/10** |
| Claude existing inbox socket | 4 | 4 | 3 | 3 | **7/10** |
| Grok resident leader session | 4 | 4 | 4 | 3 | **7.5/10** |
| Cursor local host hooks | 3 | 2 | 3 | 4 | **6/10** |

Fit includes recipient identity and workspace verification. Context/wake measures
passive versus actionable delivery without replaying history. Recovery measures
what is observable after crashes or lost responses. Stability considers
documented contracts versus implementation-derived framing. Cursor's score is
for the implemented hooks, not its separate SDK or cloud API.

## Claude: retain direct socket delivery, qualify its receipt

The official [cross-session messaging documentation](https://code.claude.com/docs/en/cross-session-messaging)
describes the per-session socket, exported authentication token and inbound
accept/hold/refuse controls. Bare mode has no inbox. Unix authentication can use
an initial auth line; Windows requires it. An unrelated sender cannot assume
the recipient's own-child permission exception. A successful write may still
lead to a held or rejected message.

The [socket adapter](../rust/transport.rs) validates a same-user Unix socket,
uses only the owning environment's matching token, and writes one attributed
user message. It does not create a relay agent. Passive-only mail stays in the
DB. The result correctly advertises **socket-write-only**, not confirmed host
acceptance or reading. Windows named pipes are not implemented.

The exact `session_id`/`msg_id`/`uuid` message envelope is not specified by the
linked public page. Treat it as version-tested interoperability, not a stable
published wire schema. [Recorded probes](VENDOR_MESSAGES.md) cover Claude Code
2.1.281; the bound recipient must retain its inbound policy and permissions.

Alternative: [MCP channels](https://code.claude.com/docs/en/channels-reference)
provide a documented push notification mechanism and reply tools. Custom channels
require explicit enablement and preview allowlist/organization policy
support. They are worth an opt-in adapter for deliberately configured sessions,
not an automatic replacement for arbitrary existing inboxes. Do not enable
permission-relay capability merely to deliver peer text.

The optional [Stop completion check](HOST_HOOKS.md#claude-bounded-completion-check-alongside-native-delivery)
now reports pending IDs once and permits explicit single-body recovery. It was
observed recovering overlooked work in Claude Code 2.1.283; it does not strengthen
the socket receipt. The channels reference also explicitly disclaims
acknowledgement: a written notification can be silently dropped by host policy.
Do not add a channels adapter merely to claim stronger receipt guarantees.

**Next improvement:** pin socket compatibility across supported versions and
test hold/refuse policy explicitly. A sender model calling `SendMessage` adds
inference without fixing the socket's observability gap.

## Codex: the app-server is the right control boundary

The [official app-server reference](https://learn.chatgpt.com/docs/app-server)
documents `thread/inject_items` for persisted passive context, `turn/start` for
new actionable input and `turn/steer` for an active turn guarded by its expected
ID. These are different operations; injecting and then starting with the same
text duplicates context. Thread metadata can be read without its turns.

The [Codex client](../rust/transport.rs) verifies thread ID, canonical workspace
and loaded-idle status before [staging delivery](../rust/dispatch.rs). It sends
either injection or start, preserves the recipient's settings, bounds frames and
requests, and rejects approval requests rather than granting permissions. The
listener reuses the connection. The JSON-RPC response is a transport receipt;
only the recipient's explicit DB acknowledgement means it handled the message.

This requires the **owning app-server**. A known thread ID does not grant access
to another desktop or CLI process. [Recorded verification](VENDOR_MESSAGES.md)
includes Codex 0.155.0-alpha.9.2 and passive insertion without a model turn.

**Next improvement:** check the installed app-server schema in compatibility
tests, including response shapes and available capabilities. The client opts into
experimental APIs; do not infer every called method is experimental. Consider
explicit urgent steering only with a known active turn and user policy; ordinary
mail should continue to wait. The idle check is a preflight, not an atomic lock
on other app-server clients. An SDK that starts another process is not a better
attachment mechanism for this recipient.

## Pi: extension injection is better than another RPC process

Pi's [upstream extension interface](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts)
exposes `sendMessage` with `triggerTurn` and steering/follow-up/next-turn options.
It injects into the extension's own session, not any process by session ID.
Its [RPC interface](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc-commands.md)
offers prompt, steer and follow-up commands; a queued response does not mean the
agent handled the message. Creating a second Pi process does not attach to an
already running extension context.

The [Pi bridge](../skills/octocode-agents-communication/scripts/pi-inbox.mjs)
uses the host API, canonical Rust-rendered context and explicit sender metadata.
It waits at busy/startup boundaries, wakes only for actionable mail, and confirms
delivery from the recipient's on-disk session record. Memory-only entries remain
pending. Recovery checks the complete receipt ledger before allowing a retry.
[Pi 0.87.1 evaluation](PI_MESSAGES.md) explains the first-message persistence
edge case and why UI notification or hidden rendering is not a context boundary.

**Next improvement:** measure changed-ledger scan cost before adding incremental
parsing. Preserve session identity, truncation/rotation checks and the 64 MiB
fail-closed limit. Prefer a future supported durable append receipt over private
flush methods. The existing extension is a strong fit for an already running Pi;
owned SDK sessions could use `sendCustomMessage` without changing the DB protocol.

## OpenCode: keep HTTP for native delivery; ACP remains optional

The [official server API](https://opencode.ai/docs/server/) exposes session
metadata/status, message submission with `noReply`, asynchronous prompts, SSE
events and an OpenAPI description. These offer the required distinction between
passive insertion and recipient execution without starting another agent.

The [HTTP adapter](../rust/transport/opencode.rs) validates the session and
canonical directory before staging, uses literal loopback endpoints, disables
redirects/proxies and scopes credentials to the exact configured endpoint.
Passive delivery checks the returned user message and exact text. Actionable
delivery accepts HTTP 204 from `prompt_async`; it does not call that completion.
It preserves model, agent, system, tool and MCP configuration. Native IDs are
allocated by OpenCode rather than forged for retry deduplication.

[OpenCode 1.18.32 evaluation](OPENCODE_EVALUATION.md) records real two-recipient
HTTP tests. The [bounded ACP evaluation](ACP_EVALUATION.md) separately proved
resume and action with preserved nonempty MCP configuration. ACP adds lifecycle
and capability negotiation but is not a demonstrated improvement over the
existing HTTP endpoint for passive delivery.

**Next improvement:** capability/version checks at explicit attachment, then
optional bounded event observation for completion and usage. Keep event receipts
separate from DB acknowledgements. Status preflight and POST are not atomic;
ambiguous submissions must remain uncertain. A process-global credential binding
limits listeners targeting differently authenticated servers; explicit
per-binding credential references needs a separate secret-storage design.

## Cursor: hooks fit editor sessions; an SDK route is now worth evaluating

Cursor's [hooks contract](https://cursor.com/docs/hooks) explicitly supports
`additional_context` after tool execution. The [host hook](../rust/host_hooks.rs)
joins the DB identity, includes identity context once, and stages peer content
only on supported context-producing events. It uses full-message references
when the context budget is exceeded. An existing native binding suppresses the
fallback, so the same message is not intentionally delivered by both paths.

`beforeSubmitPrompt` is not treated as a message injection response. Hook stdout
success is not proof that the host durably consumed it. Hooks need a host event;
they cannot independently wake a stopped or idle editor. Current evidence is
[host-contract fixtures](HOST_HOOKS.md), not a live Cursor editor/API matrix.

The current [TypeScript SDK](https://cursor.com/docs/sdk/typescript) supports
durable local/cloud agents, `Agent.resume`, `send`, run status/cancellation and
usage. Local active runs also expose steering outcomes that distinguish accepted
input from a required follow-up. Local SDK state survives process restarts;
inline MCP configuration must be supplied again on resume. This is promising
for explicitly SDK-managed recipients. It does not establish attachment to an
arbitrary existing IDE chat. No SDK adapter was implemented or live-tested here.

The [Cloud Agents API v1](https://prod.cursor.com/docs/cloud-agent/api/endpoints)
can submit a run to an existing cloud agent with its current workspace and
conversation. It rejects concurrent runs with `409 agent_busy` and exposes run
status. That is a separate remote execution/authentication boundary, not local
editor injection or a way to share this local SQLite file automatically.

**Next improvement:** retain honest hook capability reporting, obtain a live
Cursor hook check, and evaluate an opt-in SDK-owned recipient with explicit
credentials, sandbox/approval policy, checkpoint ownership and MCP preservation.
Do not use one-shot `Agent.prompt` per message or silently switch a local recipient
to cloud execution. Do not claim Cursor has no messaging APIs.

## Grok: resident-session path with workspace verification

The [official agent-mode guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/15-agent-mode.md)
documents ACP prompting, updates, permission requests and an authenticated
WebSocket server. The implementation's existing leader IPC is a narrower,
version-dependent transport rather than a generally promised public attachment
API. A documented server is worth supporting when the recipient owner explicitly
starts and exposes one; it does not prove access to an unrelated resident CLI.

The [Grok adapter](../rust/transport/grok.rs) validates same-user Unix socket and
session UUID, negotiates protocol versions and prompts the existing session.
It does not load/resume it, avoiding the observed MCP replacement hazard.
Passive-only mail waits in SQLite. [Live evidence](GROK_INTEGRATION.md) covers
Grok Build 1.0.41; direct prompt preserved the owner's MCP receipt tool.

The adapter now checks `_x.ai/session/info` on connection before staging. Pinned
upstream [session-info handling](https://github.com/xai-org/grok-build/blob/f0e3be1100ef5252488e3be8bb0e91cf68d8c305/crates/codegen/xai-grok-shell/src/agent/handlers/session.rs#L60)
returns resident session ID and `cwd` inside `result.result` without loading it.
Missing metadata, a wrong ID, or a different canonical workspace fails closed.
The reusable connection avoids another metadata request on every turn. Tests
cover all three rejection paths; native Grok 1.0.41 collaboration exercised the
implemented positive path. This is a preflight, not a lock on later host changes.
The [six-agent evaluation](SIX_AGENT_EVALUATION.md) retains earlier probe history.

## Cross-vendor decisions

1. Keep one DB identity and one delivery owner per recipient. Bind the exact
   existing endpoint/session/workspace; do not silently discover arbitrary hosts.
2. Keep routing deterministic and model-free. Send new content once; retain long
   material as workspace documents. Recipient conversation history remains the
   vendor's responsibility and still consumes context/cache capacity.
3. Preserve separate states for DB persistence, transport submission, durable
   context evidence and handled acknowledgement. Lost responses are uncertain,
   not invitations to resend through another adapter.
4. Keep passive and action modes explicit. APIs that only prompt must hold passive
   mail; hooks only inject on events they support. DB-only clients remain
   useful through host-driven or manual inbox reads even when no automatic host
   wake exists; do not spend model turns on a polling loop.
5. Add compatibility evidence before expanding claims: Grok metadata versions,
   Claude envelope/inbound behavior, Cursor live hooks/SDK ownership, and native
   version drift. Broadening transports should not multiply message histories or
   override a recipient's tools, permissions, MCP configuration or working tree.

The current choices are well suited to their stated existing-host scope.
The material opportunities are stronger host receipts and capability checks,
Grok workspace validation, and a separately scoped Cursor SDK integration. ACP
does not remove those ownership and delivery constraints. The
[service protocol](SERVICE_PROTOCOL.md) remains authoritative for DB semantics.
