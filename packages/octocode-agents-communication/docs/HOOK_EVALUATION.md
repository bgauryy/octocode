# Hook evaluation

The hook adapter now checks established, fresh sessions through a read-only SQLite
connection. Empty routine events do not acquire the writer lock. Creation, presence
renewal, missing attachment/context, pending messages, and session end use the
existing mutation path. There is no process-local identity cache: every invocation
checks the database, workspace, receiving transport, and presence deadline.

## Measured change

Release CLI comparison on macOS arm64, 2026-09-25; 30 routine events per vendor per
binary. No vendor process or model was invoked. Both sides initialized an isolated
database and emitted their initial identity before measuring repeated post-tool
events. A second connection observed SQLite `data_version` after each invocation.

| Measurement | Cursor baseline → candidate | Grok baseline → candidate |
| --- | --- | --- |
| Events with a committed database change | 30 → 0 | 30 → 0 |
| Idle median, including CLI process startup | 10.85 → 10.22 ms | 10.42 → 10.19 ms |
| Idle p95 | 11.45 → 10.71 ms | 11.14 → 10.35 ms |
| Invocation during a 300 ms writer hold | 425.72 → 10.54 ms | 418.68 → 10.45 ms |
| Routine output, all 30 events | 90 → 90 bytes | 90 → 90 bytes |
| Repeated identity/message context | 0 → 0 bytes | 0 → 0 bytes |

The keep criterion is unchanged envelopes and once-only context with no writer
dependency on fresh, empty events. It passed. Small uncontended timing differences
are informational, not evidence of a general speedup. `data_version` detects a
committed change between polls; it is not an exact SQL statement or disk-write
counter. No provider token or cache-hit measurement is inferred from output bytes.

Baseline binary SHA-256:
`e435835e58547823bc3d89cfed4aa6e8ba13a87e59786111c9a5d0a926f9dc46`.
Candidate binary SHA-256:
`da1bf1e35d7e785752e1937583a3543e149b1fb19aaebe3c76321cfcc0313ef1`.
Local report: `.octocode/benchmarks/communication-hooks/comparison.json` at the
repository root. The baseline binary and hook source were frozen before editing.

## Safety and trigger behavior

- Fresh presence means at least 45 seconds remain. The existing renewal policy is
  unchanged; expired identities resume through the normal path, releasing old leases.
- Concurrent first events retain transactional identity creation and unique delivery
  attempts. The read-only path never creates, stages, acknowledges, or retries mail.
- Native-bound identities receive no competing hook context, including identities
  registered with a generic vendor label. No native error enables raw delivery.
- Prompt hooks preserve host-specific empty/continue envelopes. Grok receives context
  at supported post-tool events; Cursor also supports initial session context.
- A new explicit `context_generation` takes the existing context-registration path.
  Existing audit markers remain authoritative; no conversation history is reloaded.
- A message arriving just after an empty read waits for the next host event, as with
  the existing empty-poll behavior. Hooks do not wake idle hosts. Use a native receiving
  transport where available, and `listen` when continuous presence is needed.

## Reproduce and guardrails

```
node scripts/hook-benchmark.mjs /path/to/frozen-release /path/to/candidate-release report.json
COMMUNICATION_BINARY=/path/to/candidate-release node --test tests/host-hooks.test.mjs
```

The harness uses temporary workspaces and databases, checks unchanged empty output,
records executable hashes, and removes fixtures. Thirteen host tests cover concurrent
first registration/delivery, native binding, once-per-generation context, bounded
message references, ignored/foreign events, teardown, near-expiry renewal, expired
lease cleanup, read-only idle events, and an unrelated held WAL writer. These are
deterministic CLI regressions, not a fresh live-vendor integration claim. Mutation
events intentionally still wait for SQLite's writer; this change does not remove
contention from message delivery or registration.
