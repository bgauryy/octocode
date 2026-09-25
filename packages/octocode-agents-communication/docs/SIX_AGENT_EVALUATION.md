# Six native agents: frozen live protocol

For the later batch/context candidate, see [context profiles and bounded recovery](CONTEXT_PROFILES.md).
That report preserves failed trials and the subsequent six-agent pass separately;
the original results below are not a token-cost baseline for a changed workload.

This trial uses two Codex Luna, two Claude Haiku, and two Grok fast sessions. The separate controller is a DB identity and deterministic test process, not a seventh model. Pi, OpenCode and the scripted raw peer are excluded by explicit selection. Existing default matrix behavior remains unchanged.

Before the run, the task and pass criteria are fixed:

- Every recipient discovers all six peer identities with a successful native `peers` tool call and reads the shared brief.
- Each publishes its own short coordination document and originates five requests through its own bound native `send_message` tool. The controller only sends START, passive FYI and final broadcast; it never submits these thirty requests under peer identities.
- Every directed pair has exactly one correlated question and final answer. Answers acknowledge the question atomically. Every recipient successfully reads all five peer documents before answering. Native successful send receipts must match all thirty authored DB request IDs.
- Informational delivery causes no recipient model turn. The final broadcast reaches all six, every message is acknowledged, adapter submission receipts complete, and no extra messages remain. Expected totals: 30 questions, 30 answers, 6 START messages, 6 passive notices, 1 broadcast message; 73 stored messages and 78 recipient deliveries.
- The installed skill and copied runtime are hashed before use. Routing uses production delivery adapters and starts no sender/relay model. Host settings are isolated; only six owned native sessions receive the task. Owned listeners/hosts are stopped and reaped, the DB is exported, and synthetic workspace documents and native event receipts are preserved.

The compact tool set is `peers,send_message,ack,read_document,share_document`. The behavioral task includes document review and a coordination risk/mitigation, rather than only transport acknowledgements. It does not evaluate coding quality or filesystem lease compliance. Native transcript retention and provider caches still exist; no claim of zero receiver context is made.

Run from the package directory after a reviewed skill/runtime freeze:

```sh
COMMUNICATION_VENDORS=claude,codex,grok \
COMMUNICATION_AGENT_ORIGINATED=1 \
COMMUNICATION_EXPECTED_BINARY_SHA256=APPROVED_SHA256 \
node scripts/service-mesh.mjs
```

`--plan` prints the selection without starting any hosts or requiring Pi credentials. A read-only, five-second-bounded Grok `_x.ai/session/info` probe records native session metadata separately; it is not a production preflight or a required model task.

## Results

This records the original passing artifact. A later context-batching candidate
[failed its native handling gate](OPTIMIZATION_PLAN.md#native-acceptance-failed-development-candidate-only);
do not transfer this pass to that changed artifact or infer a token-cost win.

The single bounded trial passed on 2026-09-25, 21:27:50–21:30:04 UTC. [Frozen result](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T21-27-50.303Z/result.json), alongside the copied harness/runtime, native event receipts, exported audit DB and workspace documents. The initial selection tests passed 4/4. No trial retry or host nudge was needed.

| Gate | Observed |
| --- | --- |
| Native peers | Exactly 2 Claude + 2 Codex + 2 Grok; controller separate |
| Agent-originated requests | 30/30; five successful native send receipts per agent matched to DB IDs |
| Directed correlated answers | 30/30; one final answer per question, correct target and conversation |
| Documents | Six contributor documents; every agent successfully read all five other contributions and the shared brief |
| Awareness | Six successful native `peers` receipts containing all six peers |
| Broadcast | All six recipients handled it |
| Storage/delivery | 73 messages, 78 submitted deliveries, zero pending ACKs or extra messages |
| Passive scheduling | No observed model turn triggered by passive notices |
| Tools/routing | 156 native tool calls; five communication tools exposed; zero routing model calls |
| Cleanup | All owned children reaped; audit export integrity `ok` |

The question phase took 95.709 seconds; setup, work and cleanup took 134.070 seconds. Audited question creation→answer creation latency was 21.548 seconds p50 and 61.891 seconds p95 (30 requests; maximum 64.566 seconds). These include recipient queues, document reads and inference; they are not transport latency measurements.

Exact runtime: `f180094c44e18ef6e926b16b63ba43ae1a687aa3733bc46a62a5ad4aa1e70a07`. Frozen harness: `5c0b2ce9de1b9d0e0bf5562e7302f01936761e756dbdb64cd4d4c7044f6ff0c4`. Skill: `9690f80f4afa7dfc42ba42f5c0211357cefef0199f98ab5866f2ab00a7d6862a` (7,849 bytes). Five tool schemas occupied 5,028 JSON bytes.

| Vendor CLI | Requested model |
| --- | --- |
| Claude Code 2.1.282 | `haiku` |
| Codex 0.155.0-alpha.9.2 | `gpt-6-luna` |
| Grok 1.0.41, build `4220f3b224a6` | `grok-4.7-build-fast` |

### Context observations

The providers still retain receiver conversation state and may make several inference requests within a turn. The following are provider counters, not a normalized cost comparison:

| Vendor / recorded scope | Records | Reported input | Cached input | Cache writes | Output |
| --- | ---: | ---: | ---: | ---: | ---: |
| Claude / turn | 10 | 284 | 425,816 | 39,805 | 15,290 |
| Codex / final cumulative per recipient | 2 | 611,471 | 557,312 | not supplied | 5,558 |
| Grok / turn | 9 | 695,627 | 590,976 | 0 | 20,217 |

Claude separates ordinary input, cache reads and cache creation; other providers may include cached tokens in input totals. Do not add cached input to inclusive input totals or compare the raw input columns as equivalent. These totals are substantial: this trial proves cooperative behavior and cache observations, not reduced tokens, low cost or a performance improvement. Message bodies occupied 12,296 stored UTF-8 bytes; contributor documents occupied 1,458 bytes in total, plus the 176-byte shared brief. Routing added no sender/relay model or replay of whole sender histories. It did not erase recipients' native history.

### Grok metadata probe correction

The frozen mesh sent the literal method `x.ai/session/info`; both sessions returned `-32601`. That result does **not** establish that metadata inspection is unsupported: [ACP extension methods](https://agentclientprotocol.com/protocol/v1/extensibility) require the leading underscore. The working harness now uses the corrected method; the executed copy and its hash remain unchanged in the evidence directory.

A [separate model-free probe](../../../.octocode/benchmarks/communication-grok-metadata/2026-09-25T21-32-14.886Z/result.json) used `_x.ai/session/info` on one owned Grok session. It returned the correct `sessionId` and canonical `cwd` inside `result.result`; no model prompt or agent update was sent. An unknown UUID returned an empty `result.result` rather than an explicit error. The probe's conservative “must error” assertion therefore failed and is preserved. A production consumer must require the expected identity and workspace and reject the empty response. The owned process was reaped. No production adapter was changed by this probe.

The metadata-only session also exposed default host context and tool definitions despite requested profile restrictions. Its configuration differs from the mesh's explicit system prompt, so it cannot quantify mesh overhead; it does show that “clean agent” isolation should be checked from actual host metadata, not inferred from launch flags.

This remains one synthetic collaboration scenario on one Mac. It does not prove coding quality, lease compliance, absence of all possible prompt injection, every vendor configuration, or universal zero context duplication. Native receipt evidence, exact message counts, no retries and complete ACKs support the narrower once-per-ID routing claim for this run.

## Packaging check

After the live trial, the extracted-package release smoke failed before its functional checks: the copied macOS ARM64 executable exceeded the read-only startup timeout. The built runtime completed the six-agent trial, but this does not establish reliable startup from a fresh extraction. The failed [smoke evidence](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T21-27-50.303Z/release-smoke.json) is retained; the package remains blocked for release pending the [macOS startup investigation](MACOS_STARTUP.md). No retry was used to replace this failure.
