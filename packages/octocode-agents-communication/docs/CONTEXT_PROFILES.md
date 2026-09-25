# Context profiles and bounded recovery

September 26, 2026. SQLite remains the shared authority; native APIs deliver to
existing recipients. No sender or relay model was added. No schema migration or
runtime dependency was introduced by these changes.

## Implemented

- `skill --vendor <host>` derives a smaller setup from the canonical 38-line
  skill. Common coordination, locks, recovery and raw fallback stay intact.
  Hosts explicitly choose this once. Plain `skill` and managed workers remain
  complete; automatic managed-worker scoping was discarded after the trial.
- `inbox {"message":ID}` retrieves one unacknowledged received message, rather
  than rereading the whole inbox. It excludes other recipients and expired mail.
- Optional Claude `completion-check` reports pending submitted IDs on Stop, with
  one recovery continuation. It never replays bodies or acknowledges work itself.
  See [host setup and limits](HOST_HOOKS.md#claude-bounded-completion-check-alongside-native-delivery).
- Grok checks resident session ID and canonical workspace before staging; empty
  or mismatched metadata is rejected without prompting or reloading the session.
- Identity errors direct agents to refresh peers and copy the exact ID. The
  skill permits this recovery instead of requiring an outdated discovery result.

### Instruction size

The frozen experiment CLI reports 8,352 UTF-8 bytes for the complete skill. Derived profiles:

| Host | Bytes | Reduction versus complete skill |
| --- | ---: | ---: |
| Claude | 7,005 | 16.1% |
| Codex | 6,768 | 19.0% |
| Grok | 7,076 | 15.3% |
| Pi | 6,903 | 17.3% |
| OpenCode | 6,944 | 16.9% |
| Cursor | 6,910 | 17.3% |
| Generic | 6,586 | 21.1% |

These are instruction bytes, not token savings. The smaller setup retains common
capabilities because the host may use them later. Tool selection remains a
separate, fixed host decision. Nothing here resets vendor conversation history.

## Native collaboration and failure history

The first v2 six-agent run failed: Codex mistyped Claude's recipient ID twice and
left START pending. Five peers originated five questions each; Codex originated
four. The Stop check recovered overlooked Claude IDs without whole-inbox replay.
The failed run and audit remain under `communication-context-v2/six-agent`.

After replacing the rigid discovery-once instruction with error-triggered
refresh, the [six-agent recovery run](../../../.octocode/benchmarks/communication-context-v2/six-agent-recovery/result.json)
passed with two Claude Haiku, two Codex Luna and two Grok fast recipients:

- 30 agent-authored questions and 30 correlated replies.
- Six contribution documents; every peer read the other five.
- Six final broadcast recipients; 73 messages and 78 deliveries in the audit.
- Zero pending deliveries, duplicate messages, or routing model calls.
- 136 recipient tool calls; question exchange completed in 81.17 seconds.
- All owned processes reaped. No manual host prompt or delivery retry.

This is interoperability evidence, not a comparison with an equivalent baseline.
Its usage totals are incomplete: the last DB ACK preceded some native final
results. Do not use that run for a token reduction claim.

## Matched context experiment

`node scripts/context-profile-benchmark.mjs` freezes binary, harness, turn-observer
and driver hashes. It uses one Claude, one Codex and one Grok per trial, two task
families, two full/scoped pairs per family in AB/BA order, eight sequential trials.
The protocol task and tools remain identical within a pair. Provider caches and
host load are uncontrolled; this is exploratory evidence, not general coding
quality or a stable production percentile.

The required gate is at least 20% median reduction in total observed input for
each vendor, all task outcomes complete, no duplicate or pending delivery, and
shared handling-time p95 at most 1.2 times baseline. With four samples per arm,
p95 is a coarse maximum guard. The clock includes final broadcast handling.

Claude totals add ordinary input, cache reads and cache writes. Codex/Grok input
already includes cached input; do not add it twice. Missing metrics stay unknown.
The observer waits for final Claude results and every started Codex turn after
ACK, rather than treating DB handling as a usage receipt. Failed trials are
retained and stop the schedule; they cannot establish savings.

The first profile trial was deliberately stopped after discovering the usage
race, before accepting any comparison. It is preserved under
`communication-context-v2/profiles`; all children were reaped. The repaired
experiment is separately frozen under `communication-context-v2/profiles-settled`.
Results and keep/discard decisions belong to its final report, not inferred
from instruction size or cache hit counts.

### Result: narrower context, cost target missed

All eight [frozen trials](../../../.octocode/benchmarks/communication-context-v2/profiles-settled/result.json)
passed the protocol task and reaped their processes: 24 native participants,
48 directed questions, 48 replies and 24 final broadcast deliveries. Four matched
pairs per vendor used the same models, binary, tools and hook configuration.

| Vendor | Median paired change in total input | 20% reduction gate |
| --- | ---: | --- |
| Claude | 7.7% reduction | Failed |
| Codex | 6.0% increase | Failed |
| Grok | 7.9% reduction | Failed |

Shared handling p95 was 108.72 seconds with full instructions and 143.08 seconds
with scoped instructions, a 31.6% regression; the 20% latency guard also failed.
Tool-round and host scheduling variation is material in this small sample; this
does not establish that scoping causes higher costs. It does reject the claimed
20% cross-vendor saving for this experiment.

Keep the optional `skill --vendor` interface for measured instruction-byte
reduction. Discard its automatic rollout to managed workers; `run` retains the
full skill. Keep bounded completion recovery and identity preflight based on
their independent correctness evidence. No total-token saving or faster-agent
claim is accepted. The subsequent unified-adapter refactor changes internal
ownership, not this frozen experiment's subject.

## Validation

The release artifact `315cc2975b519736d5c58447a1c9b22809151c2ae6cd387fd87ccd0dadcdba39`
passed 255 JavaScript tests with SQL fallback enabled, 31 Rust tests, formatting
and Clippy. Additional observer tests cover final-ACK/usage ordering. Skill
validation passes and the canonical file remains 38 lines. The extracted package
smoke passed CLI/MCP messages, lease handoff, audit, and v1–v5 upgrades to v6.
This successful local macOS smoke does not erase prior intermittent startup
failures or replace the [release gates](RELEASE_CHECKLIST.md).
