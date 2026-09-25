# Cross-vendor communication landscape

Reviewed September 26, 2026. This is a bounded source/document review, not a runtime
benchmark or an endorsement of upstream reliability. No downloaded project was
installed or executed; stars were not used as proof. Five relevant families were
selected for different layers of the problem.

## Decision

Keep Octocode's local Rust/SQLite coordination authority and its existing-session
adapters. Borrow protocol ideas, not another complete orchestration stack. Durable
mail storage, recipient context admission, model wake, and semantic acknowledgement
are separate responsibilities. A standard wire protocol does not erase those
boundaries or make an arbitrary existing CLI session addressable.

| Project | Actual layer and delivery | Storage, audit, and ownership | Wake/context implication | Fit for Octocode |
| --- | --- | --- | --- | --- |
| [MCP Agent Mail](https://github.com/Dicklesworthstone/mcp_agent_mail) | Shared mail server exposed through MCP; names, inboxes, threads | SQLite plus Git-backed artifacts; advisory path reservations | Supplied post-tool hook announces unread mail; this does not itself prove idle-session wake | Closest feature comparison; retain Octocode’s stricter conflict admission and direct recipient adapters |
| [AgentBus](https://github.com/jahwag/agentbus) | Go mailbox service over HTTP/Streamable HTTP MCP | SQLite messages, per-mailbox receipts, stable delivery batches, audit; examined ownership is delivery state, not file-path authority | Consumer waits and ACKs; outstanding batches can be offered again | Best compact receipt/state-machine reference; redelivery needs context dedup above it |
| [Beads](https://github.com/gastownhall/beads) | Dependency-aware task/memory graph; mail command delegates externally | Current documented backend is Dolt; atomic task claims and task audit | Task context/compaction is useful, but mail/wake belongs to the configured provider | Complement with task references; do not turn communication storage into an issue tracker |
| [Agent Client Protocol](https://github.com/agentclientprotocol/agent-client-protocol) | Host/editor ↔ coding-agent session protocol | Session persistence belongs to the agent implementation; not a shared path-lease database | `session/prompt` runs a turn; optional resume avoids replay to the client | Use only where capability, ownership, and configuration preservation are proven |
| [A2A](https://github.com/a2aproject/A2A) | Network protocol between agentic applications | Tasks, messages, artifacts, and capability discovery; storage policy remains implementation-specific | Streams/webhooks carry task updates; not a universal desktop-session injection API | Future federation boundary, not required local machinery |

## Findings that change design choices

### Mailbox reservations are not necessarily exclusion

Agent Mail documents that `file_reservation_paths` can return both grants and
conflicts: overlapping exclusive requests are still granted. Its pre-commit guard
has a different enforcement boundary from admission before an individual edit.
Its stale-reservation recovery also considers activity heuristics. Preserve
Octocode's atomic conflict rejection and explicit lease/presence expiry; do not
interpret another server's successful reservation response as exclusive permission.
[Reservation contract](https://github.com/Dicklesworthstone/mcp_agent_mail/blob/4b11f26277f611e60bcfdb3858c305d1af2fcc53/README.md#L1663-L1672).

The supplied hook requests unread headers without bodies, rate-limits checks, and
emits Claude `PostToolUse.additionalContext`. That is useful evidence for quiet,
bounded hook work. It is a reminder channel, not proof that an idle recipient can
run. [Hook implementation](https://github.com/Dicklesworthstone/mcp_agent_mail/blob/4b11f26277f611e60bcfdb3858c305d1af2fcc53/scripts/hooks/check_inbox.sh#L90-L173).

The reviewed license includes a restrictive named-party rider and is not ordinary
MIT. No source vendoring is proposed. The [Rust sibling](https://github.com/Dicklesworthstone/mcp_agent_mail_rust)
was discovered but not audited here; neither feature parity nor identical licensing
is assumed. [Reviewed license](https://github.com/Dicklesworthstone/mcp_agent_mail/blob/4b11f26277f611e60bcfdb3858c305d1af2fcc53/LICENSE).

### Receipt semantics matter more than the word “lease”

AgentBus provides sender idempotency keys, one outstanding batch per mailbox and
per-message receipt state. Its source immediately returns an outstanding batch with
`redelivery:true` on the next delivery request. The README's visibility-timeout
wording conflicts with ADR 0001, which rejects that mechanism, and with the examined
schema/implementation. This comparison follows the code, not the README timer claim.
[Schema](https://github.com/jahwag/agentbus/blob/50add675991526cee85385388907013ca1ebf09e/internal/bus/bus.go#L117-L155),
[NextDelivery](https://github.com/jahwag/agentbus/blob/50add675991526cee85385388907013ca1ebf09e/internal/bus/bus.go#L935-L1020),
[ADR](https://github.com/jahwag/agentbus/blob/50add675991526cee85385388907013ca1ebf09e/docs/adr/0001-materialized-mailbox-receipts.md).

Adopt stable IDs and explicit recipient obligations. Do not automatically feed an
outstanding delivery back into a model: a missing ACK can mean unfinished work,
a lost ACK, or an already-visible message. Preserve separate context receipt and
handled-ACK records. AgentBus uses [MIT](https://github.com/jahwag/agentbus/blob/50add675991526cee85385388907013ca1ebf09e/LICENSE);
that makes it a simpler potential reuse candidate, not proof of suitability.

### Beads mail is a delegation seam

The repository now resolves to `gastownhall/beads`. Its documented task storage is
Dolt, so older “SQLite plus JSONL” descriptions should not be reused as current
facts. [Current overview](https://github.com/gastownhall/beads/blob/a1bc167b54922b7a3953b23d4427aa7fb46409fc/README.md).

`bd mail` looks up `BEADS_MAIL_DELEGATE`, `BD_MAIL_DELEGATE` or `mail.delegate`, then
executes the configured program. It does not itself establish native Claude/Codex
wake. A future integration should exchange task IDs and document references with
Beads rather than duplicate its task graph or assume its mail CLI speaks Octocode's
arguments. [Exact delegation](https://github.com/gastownhall/beads/blob/a1bc167b54922b7a3953b23d4427aa7fb46409fc/cmd/bd/mail.go#L12-L108).

### ACP resume is not permission to take over a session

ACP negotiates optional capabilities. `session/load` replays history to the client;
`session/resume` avoids that replay but reconnects requested MCP servers and restores
session context. Therefore resume is not a context reset, nor proof that a second
client can attach without changing a live owner's tools. Require owner/vendor tests
before enabling it in a production adapter. [Session setup](https://agentclientprotocol.com/protocol/v1/session-setup).

`session/prompt` starts processing that can include several model/tool exchanges.
Do not label it passive message insertion. Usage updates include current context
and cumulative cost; preserve those scopes instead of summing snapshots as request
usage. [Prompt lifecycle](https://agentclientprotocol.com/protocol/v1/prompt-turn).

### A2A belongs at an explicit network boundary

A2A capability-negotiates streaming and push notifications, supports task/artifact
updates, and offers `historyLength:0` for history-free responses. Those are useful
patterns for compact clients. A webhook reports a task update to its registered
consumer; a host adapter still decides whether/how to wake a local coding session.
A future bridge needs authentication, task-to-conversation mapping, receipt
policy and deduplication. [Specification](https://a2a-protocol.org/latest/specification/).

## Concrete integration recommendation

```text
sender CLI / MCP / SQL
          │
          ▼
SQLite: identity + immutable message + recipient obligation + audit
          │
          ▼
Rust delivery owner: capability check → bounded batch → durable attempt
          │
          ├─ existing native recipient API
          ├─ supported host hook / plugin
          └─ raw inbox consumer
                    │
                    ▼
           context receipt → work → handled ACK
```

1. Keep routing, fanout, presence, and path checks deterministic: no proxy-model turn.
2. Prefer an existing recipient API only when workspace, session ownership, tool
   configuration and receipt semantics are verified. “Supports MCP/ACP” is insufficient.
3. Keep ambiguous attempts inspectable; require receipt reconciliation or explicit
   retry. An ACK means handled, not merely socket-written or context-visible.
4. Make host hooks quiet when there is no actionable delta. Preserve stable message
   IDs; use immutable document references for large context instead of repeated bodies.
5. Retain atomic path reservations with bounded expiry and explicit reasons. Host
   structured-tool admission remains narrower than arbitrary filesystem fencing.
6. Add ACP/A2A/Beads bridges only for a concrete recipient requirement. Keep them
   adapters around the existing entities, not competing authorities.

## Evaluation gates for any adopted adapter

| Gate | Required evidence |
| --- | --- |
| Identity | Wrong workspace/native session rejected; no implicit new agent |
| Context | Zero repeated bodies during ordinary reconnect; explicit retries counted separately |
| Wake | Action starts handling; passive creates no model turn, or remains held if unsupported |
| Audit | Every fanout recipient and uncertain attempt stays inspectable; submitted is not ACK |
| Cost | Routing model calls = 0; record bytes, transport latency, and provider usage separately |
| Ownership | Existing recipient tools/settings preserved; stale presence cannot authorize edits |
| Recovery | Busy/disconnect/restart tests; no automatic replay after uncertain acceptance |

These are proposed acceptance gates, not measured upstream scores. This review did
not execute upstream tests, inspect every issue/release, or establish universal absence
of features. GitHub discovery and exact reads used the local Octocode CLI; official
web specifications supplied protocol details. One combined-read continuation reported
a changed snapshot, so deciding evidence was reread as narrower queries and pinned
where a commit was available. Generic “Agent Message Bus” searches found several
unrelated projects; this report uses the explicit `jahwag/agentbus` identity.
