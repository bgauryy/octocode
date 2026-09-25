# Recovery evaluation

`scripts/recovery-soak.mjs` exercises the real Rust CLI, SQLite state machine, and
OpenCode HTTP adapter against an owned local fixture server. It freezes a copy of
the runtime for each run, records its SHA-256 and the harness SHA-256, uses temporary
workspaces, and reaps its CLI children. It does not start a model or vendor agent.
Inherited OpenCode credentials are removed from fixture child environments.

## Contract and scenarios

Default: 30 cycles, rotating through five scenarios six times. The harness permits
5–200 cycles. Each CLI process and wait has a deadline; failed assertions stop the
run and enter cleanup. All messages include a reason for the request.

| Scenario | Required behavior |
| --- | --- |
| Busy recipient; stop/restart listener | No attempt while busy; message remains in the inbox; restarted listener submits at idle |
| Disconnect during preflight | No staged attempt or external offer; message stays inspectable and submits after reconnect |
| Disconnect after server receives POST | Attempt becomes uncertain; dispatch and listener restart do not replay it |
| Kill listener after staged POST, before receipt | Attempt remains staged and inspectable; restarted listener does not replay it |
| Reset HTTP connections and hold the DB writer | Persistent listener reconnects for a new message; a blocked sender progresses after writer release |

For uncertain and interrupted staged attempts, the fixture explicitly inspects the
state and calls `retry_delivery`. The next attempt must use a new token. The server
counts every external offer by message ID and checks that staging was committed
before that offer. Exactly one extra offer is allowed for each explicitly retried
message; any other duplication or missing offer fails the run.

Every successful submission is checked in the CLI inbox with `acknowledgedAt` still
null. Only then does the fixture simulate completed handling with an explicit `ack`.
Submission and acknowledgement are never treated as the same receipt. Messages
without a transport receipt remain visible before and after restart.

## Recorded result

The 2026-09-25 macOS arm64 run passed all 30 cycles:

- 36 stored messages and 48 external offers.
- 12 deliberate retries; each rotated its attempt token and produced one additional
  offer. Zero unexpected duplicate offers.
- All messages remained inspectable when pending and were explicitly acknowledged
  only after submission checks. No message loss assertion failed.
- Zero model calls, failed assertions, cleanup failures, or remaining CLI children.

Artifact at repository root:
`.octocode/benchmarks/communication-recovery/soak-30-final.json`.
The artifact records exact runtime, executable/harness hashes, per-cycle timings,
offer tokens, final acknowledgement counts and dispatch states. It contains only
synthetic fixture messages and identities.

A separate 150-cycle run on the same date passed in **81.77 seconds**, crossing the
60-second presence window. It exercised each scenario 30 times: 180 messages,
240 external offers, 60 explicit retries, and zero unexpected duplicate offers.
All 180 messages were explicitly acknowledged after submission checks; there were
no failed assertions, model calls, cleanup failures, or remaining CLI children.
The harness renews both participants before every cycle. Runtime heartbeats require
an already-active identity, so this run cannot silently revive an expired sender or
recipient. No production heartbeat change was needed.

Longer-run artifact:
`.octocode/benchmarks/communication-recovery/soak-150-presence-window.json`.
Frozen runtime SHA-256:
`452e094603840e01814476d1b401a2f14847632edb69bcbd54dd81ff330b616b`.
This bounded 82-second exercise is not a long-duration reliability or 24-hour soak.

## Reproduce

From the package directory after building a release runtime:

```
COMMUNICATION_BINARY=/absolute/path/to/release/octocode-agents-communication \
COMMUNICATION_CYCLES=30 \
COMMUNICATION_OUTPUT=/absolute/path/to/result.json \
node scripts/recovery-soak.mjs
```

This is a deterministic transport/database recovery check. It does not prove
provider availability, actual model handling, recovery from machine power loss,
filesystem corruption, cross-platform signal behavior, or real-vendor outages.
The server receipt proves acceptance by the fixture. Live-vendor interoperability
is evaluated separately. Explicit retry can duplicate external work and is never
performed automatically by the delivery service.
