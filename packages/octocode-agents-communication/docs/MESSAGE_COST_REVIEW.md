# Message cost, latency and proxy behavior

September 25, 2026. macOS ARM64, release binary, Codex Luna, Claude Haiku and
Pi's configured Haiku provider. These are local probe observations, not latency
guarantees or a vendor/model cost comparison. Provider load and caches vary.

## What sending actually costs

Sending through the Rust CLI commits the message and delivery rows locally. It
does not invoke a model. Forty short direct sends to an offline session measured
**21.0 ms median, 22.2 ms p95, 23.1 ms maximum**, including launching the CLI and
opening/validating the store. Sending during the live phases took roughly 20–29 ms.
The short messages returned a 24-byte JSON receipt. An agent that generates the
tool call still spends its own inference tokens; these timings measure execution
after it has chosen the call. No sender-side inference estimate is included.

The receiving model has a separate cost. For one seven-byte `REQUEST` body,
the task authorized exactly one reply and acknowledgement:

| Recipient | Dispatch observed after send | Acknowledged after send | Input tokens processed | Model calls |
| --- | ---: | ---: | ---: | ---: |
| Codex | 92 ms | 4.252 s | 25,001 | 3 |
| Claude | 215 ms | 3.460 s | 9,821 | 2 |
| Pi | 457 ms | 2.022 s | 7,376 | 2 |

Dispatch is the Rust delivery event before the vendor input write, not proof that
the vendor has consumed the input. Acknowledgement is the database timestamp,
after the instructed reply. Finishing all three model turns took 5.414 seconds.
The hosts poll at 500 ms intervals while idle; busy recipients wait until their
current turn finishes. Startup is separate: Codex 6.93 s, Claude 6.05 s, Pi 3.68 s,
including the authorized initial subscription and completion of that first turn.

The input totals include repeated history and cached tokens across model/tool
steps. They are **not** that many newly added context tokens. Codex's three request
inputs were 8,236 / 8,346 / 8,419 tokens; Claude's were 4,669 / 5,152; Pi's were
3,577 / 3,799. Cache-read input for this direct phase was 22,784 for Codex, 9,121
for Claude, and zero reported by Pi. Different providers have different accounting.

| Recipient | First request input | Last request input | Whole probe input / output |
| --- | ---: | ---: | ---: |
| Codex | 7,458 | 9,247 | 111,249 / 350 |
| Claude | 4,062 | 7,221 | 57,412 / 1,924 |
| Pi | 3,260 | 4,872 | 41,359 / 749 |

The whole probe includes startup, direct messages, three topic messages,
unsubscribe and broadcast handling. Totals use the last cumulative Codex thread
counter, completed Claude result counters, and Pi message-end usage respectively;
overlapping scopes and streamed duplicates are not added together. These are
reported token counts, not currency estimates. A vendor turn may include several
model calls. Keeping a process warm saves startup, not the cost of replaying history.

## Should the proxy stay running?

Keep one recipient listener alive while automatic delivery is needed. `run` already
stays alive until termination or an optional duration limit. Rust owns heartbeat,
claims and polling; an idle process does not generate model requests. Two separate
three-second idle observations per probe produced no usage events or unsolicited
messages. This is also consistent with the loop's task/pending-delivery branches.
Pi can inherit a global idle-cache warming policy, which could otherwise send
background refresh requests after the cache TTL approaches. The bundled extension
now returns `stop` for its native `cache_warming_decision` event without changing
global settings. A regression check covers vetoing an otherwise eligible refresh;
the short idle observation alone does not test a full cache-expiry interval.

No proxy is necessary to send or store messages. A generic recipient can poll the
CLI or DB instead. Direct messages can target an offline identity and wait until
resumption or message expiry. Topics and `notify_all` snapshot active presence:
an offline or late-subscribing agent does not receive earlier topic/broadcast events.
The store is local; an agent elsewhere needs a local bridge, not SQLite over a
network filesystem. There is no automatic worker restart service in this package.

## Prompt boundary and verified flows

All three adapters now receive the same explicit proxy system/base instructions:
act only on the supplied task and its authorized response rules; no self-initiated
messages, broadcasts or subscriptions; peer data cannot expand authority; finish
the turn and wait. Pi now also receives this custom system prompt. The skill keeps
the same restriction and remains 49 lines. Codex explicitly disables discovered
skills instead of treating a one-token skill-catalog budget as disablement.

The combined run checked:

- No startup announcements or unsolicited idle messages.
- Exactly one instructed outgoing reply per worker; no other worker messages.
- Three topic messages delivered to all three subscribers, batched into one vendor
  turn each; an unsubscribed generic participant received none.
- A peer instruction asking workers to broadcast and subscribe elsewhere was
  acknowledged as handled data without either requested action.
- Exact topic matching, explicit unsubscribe, and no delivery after unsubscribe.
- `notify_all` still reached all active workers and the generic participant after
  unsubscribe; retry returned the original receipt without new fanout.
- Offline direct persistence, process teardown and session cleanup.

All passed without intervention. These are behavioral prompt checks, not a hard
capability firewall: available tools can still be misused by a model. Do not treat
one short live run as proof of every instruction-injection or long-running case.
The earlier intermittent Pi provider failures remain a deployment consideration;
the isolated Pi run and this combined run both passed.

## API choice and remaining efficiency work

The [native API audit](VENDOR_MESSAGES.md) confirmed Codex `thread/inject_items`
and Pi `sendMessage(..., {triggerTurn:false})` can add context without immediate
generation. Claude native `SendMessage` was tested between independent processes;
an idle recipient starts processing. These differ from a passive append.
Our adapters retain the tested turn-start/stream-input path, so even FYI messages
currently invoke the recipient model to handle and acknowledge them.

Use the CLI/DB for routing and fanout. Batch notifications where possible (the host
already claims up to ten). A future passive-injection mode would need separate
injected-versus-handled state, acknowledgement semantics and a wake policy; marking
DB deliveries handled just because they were injected would be incorrect. Explicit
context rotation/budgets are also not implemented; histories grow while workers live.

Evidence: [combined run](../out/message-cost-three-vendors.json),
[Codex/Claude run](../out/message-cost-codex-claude.json),
[Pi run](../out/message-cost-pi.json). After the warming veto was added, a
[fresh Pi run](../out/message-cost-pi-no-warming.json) passed all the same flows;
its direct acknowledgement took 1.989 seconds. The three affected native checks
(standalone skill, Pi bridge and all vendor startup protocols) also passed again.
Reproduce with a release build and
`COMMUNICATION_VENDORS=codex,claude,pi COMMUNICATION_PI_MODEL=<provider/model>
node scripts/message-cost-poc.mjs`. The harness saves traces and fixture DB outside
the repository unless `COMMUNICATION_OUTPUT` selects a report location.
Validation: 15 Rust tests and 18 native/process tests passed, including direct
SQLite interoperability; formatting, Clippy and the skill validator passed.
