# OpenCode native delivery evaluation

The OpenCode adapter delivers to an existing server/session. SQLite remains the source of identity, messages, delivery attempts and recipient acknowledgements. A successful HTTP response establishes submission; server events and model completion do not replace the recipient's `ack`.

## Reproduce

Build the communication skill, then supply an OpenCode executable. The harness creates two isolated OpenCode servers, two identity-bound MCP connections, a deterministic raw recipient and a controller. It copies the production Rust binary before starting and records its hash. No sender or proxy model routes messages.

```sh
npm install --prefix /tmp/octocode-opencode-probe --no-audit --no-fund opencode-ai@1.18.32
node packages/octocode-agents-communication/scripts/build-skill.mjs
COMMUNICATION_OPENCODE_BINARY=/tmp/octocode-opencode-probe/node_modules/.bin/opencode \
  node packages/octocode-agents-communication/scripts/opencode-poc.mjs
```

Optional environment:

| Variable | Meaning |
| --- | --- |
| `COMMUNICATION_OPENCODE_MODEL` | Provider/model; defaults to `opencode/mimo-v2.6-flash-free`. Availability is provider-controlled. |
| `COMMUNICATION_BINARY` | Built Rust executable to freeze for this run. |
| `COMMUNICATION_OUTPUT` | Result directory; default is a timestamped `.octocode/benchmarks/communication-opencode/results/` folder. |
| `COMMUNICATION_OPENCODE_AUTH=0` | Explicitly test unauthenticated loopback servers; authentication is enabled by default. |

Each server gets a generated password. Its delivery listener receives `OPENCODE_SERVER_PASSWORD`, `OPENCODE_SERVER_USERNAME` and an exact `OCTOCODE_OPENCODE_AUTH_ENDPOINT` binding. Credentials are not written into the communication DB or result report. Isolated HOME/XDG directories keep the servers from loading personal OpenCode configuration or credentials. The default free provider needs no credentials; using a paid provider requires explicitly configuring that isolated environment.

The skill and bounded collaboration assignment enter each recipient once during startup. The test preserves OpenCode's native build-agent prompt and tools. In this environment, replacing the system prompt or default configuration caused the free provider to reject requests with HTTP 403. That rejection is retained as failed development evidence; the harness does not bypass it.

## Acceptance checks

- Two real OpenCode recipients and a deterministic raw participant exchange six directed questions and six exactly correlated replies, with no reply loops.
- Both native recipients discover collaborators and read the shared document through their bound MCP tools. Raw uses the production DB-backed hook/ack path.
- Passive insertion completes while assistant-message counts remain unchanged for the observation window. Subsequent action delivery wakes handling without another host prompt.
- Repeated keyed sends return the same receipt; continuing listeners do not duplicate replies.
- One actionable `notify_all` reaches all three participants. Every delivery is acknowledged; every dispatch has a submission receipt.
- Per-request native token observations are stored through `record_usage`. Owned servers/listeners are terminated and reaped; the audit database is exported.

This is a functional integration test, not a coding-quality benchmark, a reliability percentage or a controlled before/after performance comparison. The initial questions are submitted by the harness using each registered identity; replies are made by the actual OpenCode agents through MCP. The raw participant is deterministic code, not another model.

## Verified run

[Result](../../../.octocode/benchmarks/communication-opencode/results/2026-09-25T15-10-17.798Z/result.json) · [frozen harness](../../../.octocode/benchmarks/communication-opencode/results/2026-09-25T15-10-17.798Z/harness.mjs) · [audit snapshot](../../../.octocode/benchmarks/communication-opencode/results/2026-09-25T15-10-17.798Z/audit.sqlite)

OpenCode **1.18.32**, model **opencode/mimo-v2.6-flash-free**, authenticated loopback API, production binary SHA-256 `15d3391e705b5d908c68144fefaca16e6e154121d954be93486f6a170ed66241`.

| Observation | Result |
| --- | --- |
| Questions / correlated replies | 6 / 6 |
| Messages / broadcast recipients | 16 / 3 |
| Unacknowledged deliveries / duplicate replies | 0 / 0 |
| Passive message started inference | No |
| Host prompts after startup / routing-model calls | 0 / 0 |
| Native recipient MCP tool calls | 21 |
| Question handling round | 22.73 seconds |
| Owned child processes reaped | All |

The saved native conversations contain each delivered DB message ID once: six delivered IDs in four injected batches per OpenCode recipient. That prevents repeated delivery payloads; subsequent model requests still consume their retained conversation context.

The skill occupied 7,525 UTF-8 bytes in this frozen build. The current skill may differ; the result records the precise hash. The question-round duration starts after request submission and includes recipient inference/tool work. It does not measure transport latency alone.

| Recipient | Request observations | Reported input tokens | Cache-read tokens | Output tokens |
| --- | ---: | ---: | ---: | ---: |
| OpenCode 1 | 10 | 17,166 | 131,264 | 464 |
| OpenCode 2 | 11 | 3,356 | 147,520 | 512 |

These are sums of OpenCode's per-request counters, including startup, native base context, tools and subsequent handling. Reused context is counted on each request; these totals are not unique conversation tokens. Cached input is separate from the native input counter. They demonstrate observable cache reuse, not a controlled caching improvement. Provider-reported cost was zero for this free model; that observation applies only to this recorded run.

The run retains only synthetic task/message content, native events, usage and audit evidence. It does not contain user project source or provider credentials. Generated workspaces remain in `/tmp` for failure diagnosis; processes are stopped.

## Cross-vendor matrix

The existing service-mesh harness can include two OpenCode recipients in addition to two each of Claude, Codex, Grok and Pi plus raw:

```sh
COMMUNICATION_PI_MODEL='<authenticated-provider/model>' \
COMMUNICATION_OPENCODE_COMMAND=/tmp/octocode-opencode-probe/node_modules/.bin/opencode \
  node packages/octocode-agents-communication/scripts/service-mesh.mjs
```

`COMMUNICATION_OPENCODE_MODEL`, `COMMUNICATION_BINARY` and `COMMUNICATION_OUTPUT` also apply. Without `COMMUNICATION_OPENCODE_COMMAND`, the existing eight-native-plus-raw matrix is unchanged. The expanded matrix expects 110 questions, 110 correlated replies and an eleven-recipient broadcast. It uses installed vendor authentication; a missing credential or model failure is a failed run, not replaced with a simulated success.

The expanded matrix **passed** on the final release build: [result](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T15-17-19.777Z/result.json), [frozen harness](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T15-17-19.777Z/harness.mjs), [audit](../../../.octocode/benchmarks/communication-service-mesh/results/2026-09-25T15-17-19.777Z/audit.sqlite).

- Two each of Claude, Codex, Grok, Pi and OpenCode, plus raw: **110 questions, 110 correlated replies**.
- **232 messages, 242 submitted dispatches, 11 broadcast recipients, zero pending deliveries or duplicate replies.**
- Both OpenCode recipients received **22 distinct message IDs in eight batches each**; inspection of the saved native conversations found no repeated ID. Every native recipient read the shared document using its actual tools.
- Passive delivery started no model turns; routing used no model; no host prompt was sent after initialization. All owned child processes were reaped.
- Question handling round: **80.35 seconds**. Recipient tool calls: **341**. These include all vendors and are not transport-only measurements.
- Final binary SHA-256: `e435835e58547823bc3d89cfed4aa6e8ba13a87e59786111c9a5d0a926f9dc46`; embedded skill: **7,605 bytes**. This build includes authenticated delivery, canonical workspace query scoping and busy-session preflight.

OpenCode contributed 40 request-scope usage observations: 25,705 native input tokens, 697,088 cache-read tokens and 4,494 output tokens. The complete run contains 106 observations across vendors; their request, turn and cumulative scopes differ, so they must not be summed as one comparable total.

An earlier expanded-matrix development attempt timed out waiting for the second OpenCode server's health endpoint before any messages were exchanged. Its failed result is retained separately (`2026-09-25T15-13-45.983Z`); all its child processes were reaped. The final run used a fresh workspace and frozen release binary. One successful run establishes the tested behavior, not an uptime or reliability guarantee.
