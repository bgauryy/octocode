# ACP feasibility and evaluation

ACP is useful as an optional recipient-control transport. Keep OpenCode's HTTP
adapter as the production path: it addresses an existing recipient and supports
passive insertion without inference. A generic ACP adapter must retain the same
DB identity, staging, acknowledgement and uncertain-delivery rules.

The [maintainer probe](../scripts/acp-probe.mjs) is implemented and tested, but is
**not registered as a production transport** and does not deliver DB messages.
It launches an ACP executable, initializes protocol version 1, and optionally
resumes an explicitly supplied existing session. It never creates a session,
loads history, starts a sender model, or retries a timed-out prompt.

## Contract and decision

| Capability | ACP probe | Production requirement |
| --- | --- | --- |
| Initialization | Negotiates v1 and records advertised capabilities | Fail closed on incompatible versions |
| Existing session | Requires advertised `sessionCapabilities.resume` | Verify the vendor can safely attach to the intended owner |
| Configuration | Requires reviewed absolute cwd and complete MCP list | Do not invent an empty list or silently change recipient tools |
| Action message | Explicit `allowModelTurn:true`; one `session/prompt` | Stage in DB before I/O; receipt does not acknowledge handling |
| Passive message | Rejected before any prompt | Use a native passive API or explicit raw delivery |
| Streaming | Counts updates without retaining conversation content | Bound output and associate updates with the correct session |
| Cancellation | Sends `session/cancel`; waits for prompt result | Cancellation does not imply no message or side effects occurred |
| Client tools | No filesystem/terminal capabilities; permission requests denied | Preserve host authority and recipient permission policy |
| Recovery | Timeout/EOF/malformed frames fail without retry | Inspect uncertain outcomes before any alternate delivery |

The [ACP session specification](https://agentclientprotocol.com/protocol/v1/session-setup)
distinguishes resume from load: load replays history; resume restores context
without replay and reconnects the MCP servers supplied by the client. This is
not a promise that arbitrary existing configuration remains unchanged. The probe
therefore refuses automatic load fallback and requires explicit configuration.

[ACP stdio](https://agentclientprotocol.com/protocol/v1/transports) starts an agent
executable as a subprocess. That can reconnect to an existing conversation, but
does not establish attachment to the same running desktop/CLI process. OpenCode
advertises [ACP support](https://opencode.ai/docs/acp/); its [HTTP server API](https://opencode.ai/docs/server/)
remains the simpler route for already-running OpenCode recipients.

## Run the probe

Create a configuration file for an executable already installed on the machine:

```json
{
  "command": ["/absolute/path/to/opencode", "acp"],
  "cwd": "/absolute/path/to/disposable-workspace",
  "timeoutMs": 10000
}
```

```sh
node scripts/acp-probe.mjs /absolute/path/to/config.json
node --test tests/acp-probe.test.mjs
```

This default only initializes capabilities; it performs no model turn. To test
resume, add `sessionId`, the complete `mcpServers` array and
`configurationReviewed:true`. Each stdio MCP entry has `name`, `command`, `args`
and `env`; use the recipient's actual configuration. `mcpServers:[]` is appropriate
only for a recipient intentionally configured without MCP servers. To test one
action, also add `prompt` and `allowModelTurn:true`. The `passive:true` option
refuses the prompt. Use a disposable session for evaluation.

The imported `AcpProbeClient` supports `initialize`, `resume`, `prompt`, `cancel`
and `close` for harnesses. Always call `close` in `finally`. `env`, when supplied,
is the complete subprocess environment; configuration files may contain secrets
and must not be checked in. Reports omit prompt bodies, MCP credentials, stderr
contents and vendor error bodies. The probe is not an OS sandbox: lack of client
tool capabilities does not disable an agent's own built-in tools.

## Results, 2026-09-25

The deterministic suite has **21 passing tests**. Cases cover capability gates,
unchanged supplied configuration, explicit action opt-in, passive refusal,
permission/client-tool denial, cancellation, concurrent-prompt refusal, wrong
session updates, unexpected history replay, malformed UTF-8/JSON/envelopes,
unsolicited responses, frame/output budgets, EOF, timeout, process spawn failure,
termination escalation, and detached descendants retaining inherited pipes. These
are protocol fixtures, not model-quality tests.

Live OpenCode **1.18.32**, model `opencode/mimo-v2.6-flash-free`, passed a bounded
existing-session experiment with a nonempty MCP configuration:

| Check | Observed |
| --- | --- |
| Existing session created through HTTP, with one passive sentinel | Preserved |
| Resume with identical reviewed MCP configuration | Passed |
| Persisted history before/after resume | Identical; one message |
| Conversation chunks replayed during resume | 0 |
| MCP marker before/after resume | Connected; initialization/tool-list receipts recorded |
| Separate ACP reconnect followed by one actionable prompt | Completed with `end_turn` |
| Recipient response | Exact sentinel retrieved from existing history |
| Sender model calls / recipient prompt requests | 0 / 1 |
| Resume including subprocess startup | 1,093 ms |
| Reconnect and action including startup/provider latency | 9,643 ms |

[Successful raw result](../../../.octocode/benchmarks/communication-acp/results/2026-09-25T15-12-31.549Z/result.json).
The owned MCP processes were checked after teardown; all had exited. Connection
initialization occurred more than once, so this proves equivalent supplied tool
configuration, not preservation of the original in-memory MCP connections.

An [earlier run](../../../.octocode/benchmarks/communication-acp/results/2026-09-25T15-10-18.083Z/result.json)
passed empty-MCP resume but its action failed with provider HTTP 403. The free
provider rejected the modified permission policy. The successful development
rerun retained OpenCode's default permissions/tools. Both outcomes are retained;
this is a two-attempt feasibility evaluation, not a sealed benchmark or evidence
of improved model speed/token usage.

## Limits before production

- The probe defaults to 1 MiB per frame, 8 MiB each for stdout/stderr/sent data,
  eight pending requests and bounded request deadlines. Notification types are
  counted in at most 65 buckets; conversation text is discarded.
- Teardown signals the owned process group, then escalates to force termination.
  Restricted hosts can reject group signals; `processGroupSignalFallback:true`
  reports fallback to the direct child. Windows/direct-child fallback does not
  establish that every descendant was reaped. After bounded termination/drain
  waits, `forcedStdioCleanup:true` reports closing the client's pipe handles when
  escaped descendants keep them open. Cleanup does not wait indefinitely for
  those descendants; their termination is not claimed.
- One OpenCode session fixture does not prove other ACP vendors' resume behavior,
  concurrent-owner safety, crash reconciliation, permission preservation, or
  portability. Grok's existing private ACP envelope still needs its vendor adapter.
- No cached-token reduction, at-most-once external effect, or native passive ACP
  delivery is claimed. Recipients retain their history for inference.
- Production acceptance requires vendor-specific attachment/configuration tests,
  DB audit integration, safe connection ownership and reconnect handling. The
  same recipient must never be offered a second API/hook path after uncertain I/O.

Decision: keep the bounded ACP probe for future adapter evaluation; use native
OpenCode HTTP for production delivery now.
