# Communication service evaluation — 2026-09-25

This records the **historical schema-v5 artifact**. For the subsequent Grok,
Codex and Pi automatic-wake implementation and eight-native-agent validation,
see [GROK_INTEGRATION.md](GROK_INTEGRATION.md).

**Result: local integration passes; optimization KEEP, exploratory.** Existing
Claude, Codex and Pi agents exchanged DB-audited messages without a sender/relay
model. A raw participant exercised the generic path. This is an adaptive local
development result, not a held-out reliability or coding-quality estimate.

## Selected artifact

| Item | Value |
| --- | --- |
| Platform built/run | macOS ARM64 |
| Binary SHA-256 | `5faf7653765d103e438723544b4d298943286acf51bd063de7623d1e5c0e5e88` |
| Served skill SHA-256 | `7d793fce49f4c05ed2e7a52ef10d1802019e0c7ce7af59131e2ffc1d1260f7f0` |
| Skill size | 6,465 bytes / 35 lines; baseline 7,632 / 43: **15.29% fewer bytes** |
| Deterministic verification | 118 JavaScript tests, zero skips with Python 3.14; 20 Rust tests; formatting and strict Clippy pass |

Byte reduction does not establish token or task-cost savings. Other platform
selectors exist, but these results do not validate their binaries.

## Communication and coordination

The final mesh used two Claude Haiku, two Codex Luna and two Pi Haiku recipients,
plus a raw peer and a test host. Native tools and the shared DB protocol carried
requests, replies and acknowledgements; the host did not invent agent answers.

| Guard | Observed result |
| --- | --- |
| Directed collaboration | 42 requests and exactly 42 correlated replies |
| Workspace broadcast | Seven recipients |
| Stored messages / deliveries | 85 messages; 91 submitted and acknowledged deliveries |
| Document evidence | All six model agents successfully read the required document |
| Duplicate replies / pending messages | Zero / zero |
| Recipient tool calls | 173 |
| Routing model calls | Zero; recipient model-call total **unknown** |
| Cleanup | All owned children reaped |

Evidence: [final mesh result](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T13-33-48.378Z/result.json).
The same directory preserves `audit.sqlite`, `workspace-documents/`, export
manifest, `measurements.json`, harness and vendor traces. Snapshot exports include
all DB entities; documents were preserved separately. See [protocol](SERVICE_PROTOCOL.md).

OpenCode 1.18.32 was tested separately: a passive native insertion produced one
history receipt and zero observed tokens. Action dispatch is fixture-tested only.
Claude proves socket submission, not native inbound-policy acceptance. Codex
injection does not start a turn; the owning host starts authorized recipient work.
Pi uses its extension API and durable receipt reconciliation. Generic hosts still
need a context event or explicit inbox consumption; the DB cannot wake any agent.

A separate natural crash probe recovered the lease after **60.071 seconds** and
rejected stale renewal. It used the preceding build; subsequent prompt/docs
changes did not change leases. Cooperative leases do not fence arbitrary OS writes.

## Performance of the complete version change

Sixty samples per mode after five warmups, identical executable path, fresh stores,
alternating CLI/MCP order and whole-run AB/BA. Warm MCP excludes startup; CLI
includes it. This compares complete versions, not isolated attribution to caching.

| Pair | Warm MCP send p50: baseline → candidate | Reduction | CLI send p95: baseline → candidate |
| --- | ---: | ---: | ---: |
| AB | 4.565 → 0.249 ms | 94.55% | 14.904 → 11.556 ms |
| BA | 4.536 → 0.277 ms | 93.89% | 13.692 → 11.658 ms |

Both satisfy primary improvement >=20% and CLI p95 <=baseline ×1.25 +5 ms.
All runs report zero models, loss and duplicates, with all children reaped.
[Complete comparison](../../../.octocode/benchmarks/communication-service/results/2026-09-25-complete-comparison.json)
contains the selected hashes and raw run paths. Reproduction and the separate
access-mode comparison are in the [optimization plan](OPTIMIZATION_PLAN.md).
Provider latency, cache performance and statistical significance are not measured
by this local service microbenchmark.

## Context and handling latency

The mesh retained 23 usage observations: 15 Pi requests, six Claude turns and two
Codex cumulative snapshots. Scopes and vendor input semantics differ; do not sum
them as one context size or compare vendors from this workload. Codex snapshots
recorded input/cache-read totals of 81,822/67,840 and 78,831/66,816; these are
cumulative token counters, not occupied context windows. Unknown counts remain null.

`measurements.json` derives send-to-ack times from local DB wall-clock timestamps.
Those include host batching, recipient inference and tool work; they are neither
native transport latency nor a provider comparison. The measured message bodies
totaled 10,540 bytes; this is not a model-token measurement.

## Failed attempts and remaining work

All four failed development runs remain exported and preserved: missing `.md`
document reads (13:09), Codex making no tool calls despite ready MCP with cause
uncertain (13:17), mistyped recipient UUID followed by acknowledgement of a failed
send (13:23), and an extra answer-to-answer message (13:30). Repairs introduced
bounded document hints, transactional `replyTo` routing, reply-success-before-ack
and an explicit distinction between wake scheduling and body-requested replies.
The [failure ledger](OPTIMIZATION_PLAN.md#live-integration-and-prompt-status)
records exact artifact directories. The final pass does not erase these failures.

Remaining backlog: event-driven wake hints, destructive retention with tested
restoration, authenticated OpenCode endpoints, live OpenCode actionable dispatch,
broader platforms and held-out agent tasks. Native errors stay inspectable;
there is no automatic fallback/replay after an ambiguous send. Generic skill
reviewer conventions also remain a documented mismatch for this standalone CLI.
