# Communication service optimization plan

Decision baseline: 2026-09-25. This is an implementation and measurement contract,
not a claim that every proposed optimization is shipped. The
[service protocol](SERVICE_PROTOCOL.md) defines semantics; [DB.md](DB.md) owns
transactions; [BENCHMARKS.md](BENCHMARKS.md) owns measured historical results. The
[final service evaluation](SERVICE_EVALUATION.md) records this iteration's selected artifact, passing mesh and remaining limits.

## Receiver context plan — September 26, 2026

**Follow-up:** [context profiles and bounded recovery](CONTEXT_PROFILES.md) records
the implemented vendor-scoped skill, selective inbox recovery, Claude Stop check,
Grok workspace preflight, subsequent six-agent pass, and matched token experiment.
The historical failed acceptance below remains evidence for its frozen artifact.

The [six-agent trial](SIX_AGENT_EVALUATION.md) proved cooperation, not lower
receiver cost. Native history, tool loops, and the deliberately dense 30-question
exercise contributed to input usage. High cache reads do not eliminate context
occupancy or repeated inference. Keep the DB and native adapters; replacing them
with ACP/A2A or another mailbox does not remove that host-owned work. See the
[landscape review](COMMUNICATION_LANDSCAPE.md) for scope and alternatives.

| Order | Change | Status / acceptance |
| --- | --- | --- |
| 1 | Drain already-ready messages in bounded groups of 16 instead of 4, retaining the 16 KiB item budget and first-oversized-item progress | Implemented; no waiting timer or scheduler policy change |
| 2 | Atomic batch ACK through the existing tool, keeping single-ID compatibility | Implemented; 1–100 unique received IDs, all-or-nothing with per-message audit |
| 3 | Teach batch handling and combining related points in the skill | Implemented; no new tool, schema is stable for each running host |
| 4 | Run frozen baseline/candidate CLI replay and native collaboration regression | Exact wire/call gates pass; [native handling gate failed](#native-acceptance-failed-development-candidate-only) |
| 5 | Compare realistic collaboration topologies with matched tasks | Next experiment: discover peers once, ask relevant owners, publish shared evidence once; do not change the all-pairs interoperability test into a cheaper task and call it an optimization |
| 6 | Measure native request-level context and cache behavior | Next experiment: same vendor/model, fixed tool set and task, fresh sessions, AB/BA order; preserve cold/warm observations and missing counters |

The frozen [contract](../../../.octocode/benchmarks/communication-context-batching/contract.json)
requires at least 50% fewer delivery envelopes for queued bursts of 6, 16, and 33
short messages, at least 90% fewer explicit ACK calls for 16 handled messages,
and no ID/content/audit loss, passive wake, or partial batch ACK. These are exact
protocol metrics, not billed-token estimates. One candidate and one native
regression are budgeted; no live retries to hide failure.

For a subsequent token-cost acceptance decision, freeze two task families (shared
document review and a dependency handoff), two pairs per vendor in AB/BA order,
and a 20% median reduction in total input tokens per successful task for each
vendor. All required outcomes must pass; no duplicate delivery or missing ACK is
allowed, and completion p95 must not exceed baseline by more than 20%. Report
cached reads, writes, ordinary input, output, tool calls, and model requests
separately. Missing comparable telemetry makes that vendor inconclusive. This
future experiment is not yet run and is not a release claim.

### Exact replay results

[Baseline/candidate replay](../../../.octocode/benchmarks/communication-context-batching/replay-final.json)
uses the real release CLI and fresh DBs with identical queued message bodies,
reasoning, and conversation IDs. It compares the frozen prior binary with
candidate `55348a30cd2d11722e0f1c982d642e309dfa87970821df9765eb0ea7df53e942`.
The same harness was rerun after rebuilding embedded DB documentation; the
[initial replay](../../../.octocode/benchmarks/communication-context-batching/replay.json)
remains preserved. Production behavior was unchanged between these two candidates.

| Queued messages | Delivery envelopes, before → after | Explicit ACK calls, before → after | Injected UTF-8 bytes, before → after |
| ---: | ---: | ---: | ---: |
| 1 | 1 → 1 | 1 → 1 | 572 → 572 |
| 6 | 2 → 1 | 6 → 1 | 1,960 → 1,592 |
| 16 | 4 → 1 | 16 → 1 | 4,749 → 3,645 |
| 33 | 9 → 3 | 33 → 3 | 10,091 → 7,883 |

Every case retained all IDs and content, one ACK audit event per delivery, no
pending deliveries, and empty subsequent hooks. New regression cases check
recipient isolation, rollback of partial updates/audit, invalid batches, repeated
ACKs, the byte ceiling, and oversized-first-item progress. The Rust starvation and Claude transport backlog
tests were expanded to exceed the new cap; their original four-message expectations
failed before those fixture updates. No production failure was hidden by the change.

The skill remains 38 lines but grows from 7,849 to 7,997 bytes. The five selected
tool schemas grow from 5,028 to 5,297 JSON bytes. These stable-prefix increases
are deliberate costs of the new action; they are not token counts. Fewer API
calls do not guarantee fewer model requests: hosts can execute parallel tools or
make further inference requests within a turn. Count actual native requests and
usage before claiming total-token savings. The dispatcher batches only messages
already eligible at a delivery boundary; sparse arrivals can still be singletons.

### Native acceptance: failed, development candidate only

The [single candidate six-agent run](../../../.octocode/benchmarks/communication-context-batching/live/result.json)
used two Claude Haiku, two Codex Luna, and two Grok fast recipients with the same
all-pairs task. All 30 agent-authored questions were stored, but only 29 received
answers. Question 40 to Claude-1 and passive notice 2 to Claude-2 remained pending;
all 71 created deliveries were submitted. The run stopped at its bounded timeout
before the final broadcast, exported the intact audit DB, and reaped every owned
process. No host nudge or retry was used. Passive notices caused no initial model
turn. This is a failed native handling gate, not a successful cheaper workload.

Claude-1 successfully used `ack({messages:[1,7,43,47,55]})`; Claude-2 used
`ack({messages:[45,50]})`. Their native tool results contain no tool errors.
Their listener receipts show only batches of one or two messages, so this run
does not attribute failure to the larger delivery cap. Socket submission cannot
establish whether the host accepted the omitted context or the model overlooked
it. The prior passing trial does not establish reliability or isolate this cause.
Do not compare token totals from incomplete work with the completed baseline.

**Decision:** retain the implementation as a development candidate for its exact
protocol savings; native acceptance and total-token improvement remain failed or
unproven. Do not promote it as a production efficiency improvement. First resolve
Claude context acceptance/handling observability with a bounded native fixture;
then start a new frozen comparison. Do not automatically replay submitted mail,
ACK on behalf of recipients, or serialize on peer replies in a way that can
deadlock collaborating agents. The independent macOS extraction/startup release
blocker also remains unresolved.

Validation: release build and Clippy pass; 31 Rust tests pass. The full JavaScript
suite passed 245/246 with no skips; its only failure was the old four-message
fixture expectation. After expanding that fixture, all 28 transport tests pass.
The standard skill validator passes and the skill remains 38 lines. Logs and the
failed native artifact are retained beside the replay contract.

The remaining sections preserve the September 25 plan and its historical gates;
the receiver-context contract above governs the September 26 changes.

## Outcome and constraints

Let already-running agents discover peers, ask questions, coordinate edits and
exchange results across vendors, using native message APIs when available and the
same durable database otherwise. Routing, presence, fanout and locks make zero
model calls. No message or identity bypasses the database for convenience.

Preserve: required short intent, explicit handling acknowledgements, bounded
outputs, immutable message bodies, atomic multi-path leases, no automatic replay
of ambiguous external delivery, and one-user/local-filesystem scope. Keep the
skill standalone and under 50 lines. Avoid transcript forwarding and unsolicited
readiness/status chatter. Existing recipient context remains owned by its host.

## Options considered

| Option | Value | Cost / failure mode | Decision |
| --- | --- | --- | --- |
| An LLM proxy for every route | Can interpret free text | Extra context, latency, authority and duplicate-response risk | Reject for routing; retain explicit worker creation only |
| Direct vendor API only | Native receipt/context semantics | Excludes generic agents; fragmented audit and locks | Use as delivery adapter behind the DB |
| SQLite plus CLI/hooks only | Portable and auditable | Host event needed to inject; per-call startup | Universal baseline and recovery path |
| Persistent MCP access | Reuses connection and process; existing transport | Host integration/lifetime management | Measure before moving more callers |
| A new shared daemon or network broker | Multiplexing and event wakeups | Deployment, authentication, ownership and crash complexity | Defer until measured polling/startup cost warrants it |
| Full task-orchestration protocol | Rich lifecycle | More mandatory fields and agent ceremony | Add only optional conversation/reply correlation now |

Architectural check: DB state remains authoritative across all adapters. Product
check: a generic agent can still participate with the built skill. Failure check:
losing a process must not hold a lease forever or silently replay an uncertain
message. These checks favor the smaller deterministic layer.

## Priorities and acceptance

| Priority | Change | Acceptance sensor | Rollback / stop condition |
| --- | --- | --- | --- |
| P0 | Preserve native API delivery without sender inference | Attached live probe plus DB audit and vendor traces | Any routing-created model turn or missing audit |
| P0 | Optional `replyTo` / `conversationId` across CLI, tools, DB and Python | Correlation and migration contract tests; cross-vendor reply IDs | Inaccessible cross-workspace parent, altered retry receipt or schema mismatch |
| P0 | Add OpenCode existing-session transport | Local protocol fixture; installed real server/API probe when available | Unproven endpoint/wake behavior must be labelled unverified, not silently downgraded |
| P0 | Compress the executing skill | Built `skill` output equals source; <=50 lines and <=6487 UTF-8 bytes | Lost coordination/recovery branches or mismatched embedded instructions |
| P1 | Optimize warm persistent MCP; report CLI separately | Candidate MCP versus baseline MCP, matched artifact path and AB/BA runs | Correctness failure; no causal claim from unmatched paths |
| P1 | Specify receipt capabilities | Native fixture errors and actual API responses map to documented outcomes | Never label write success as handled or invent native receipts |
| P1 | Durable export before retention changes (implemented) | Consistent snapshot, integrity check and digest; preserve pending state | No destructive purge until archive/recovery contract is tested |
| P2 | Event-driven wake hint with polling fallback | Idle CPU/wakeups and delivery p95 versus polling | Lost event causes undelivered eligible message or adds unbounded queues |
| P2 | Retention/compaction for completed history | Restore exported history and document references; workload size series | Any pending-message/document loss or broken audit chain |
| P2 | First-class capability discovery | Only fields affecting delivery decisions; version-skew tests | Avoid duplicating vendor configuration or promising unsupported wake |

P0/P1 rows are the current iteration's scope where their implementation and
validation are completed. P2 rows are a backlog, not a feature announcement.
Individual run records must state which rows passed, failed or remain unmeasured.

## KPI contract

Correctness guardrails are absolute for the exercised cases. Performance targets
below are prospective gates, not previously measured guarantees. Compare an
unchanged baseline and candidate under the same host, release profile, payload,
database size and concurrency. Save version and subject hashes before each run.

| KPI | Sensor / formula | Baseline evidence | Candidate gate |
| --- | --- | --- | --- |
| Audit completeness | Stored messages and expected recipient deliveries / successful sends | Existing CLI/mesh checks | 100%; no bypass route |
| Routing inference | Vendor model starts attributable only to routing/polling | Existing raw/native probes: zero sender calls | Zero |
| Retry identity | Repeated identical key returns same ID and recipient snapshot | 40/40 runtime retries | 100%; changed content rejected |
| Repeat context | Per-recipient message IDs submitted twice without explicit retry | No repeats in previous bounded runs | Zero in normal path; disclose manual-retry risk |
| Reply integrity | Valid parent visibility, inherited conversation and immutable retry fields | Not measured before v5 | All valid/invalid cases pass |
| CLI message latency | Monotonic elapsed time including launcher, p50/p95 | 28.76 / 37.32 ms, 40 samples | Report both; paired regression >20% requires investigation |
| Warm service optimization (primary) | 1 − candidate warm MCP send p50 / baseline warm MCP send p50 | Frozen baseline binary; same executable path and harness | >=20% lower p50; correctness equal and CLI guard passes |
| Access-mode comparison (secondary) | Same-artifact warm MCP versus CLI send p50; MCP startup shown separately | Both modes recorded within each service run | Descriptive only; separate from the optimization acceptance gate |
| Idle output | Bytes emitted by an empty raw hook | Zero in 40 cases | Zero |
| Passive scheduling | Model turns before an actionable message | Zero across six previous candidates | Zero for adapters declaring passive support |
| Task input cost | Startup-adjusted input/cache metrics with vendor normalization | Exploratory 40.2–60.0% reduction; tiny prior task | New comparisons need matched cases; no cross-run causal claim |
| Skill size | UTF-8 bytes and lines of built `skill` output | 7632 bytes / 43 lines | <=6487 bytes / <=50 lines; byte reduction is not token savings |
| Lock exclusion | Opposing atomic multi-path acquisitions | 20/20 one winner; no partial leases | Exactly one full winner; no partial loser |
| Crash recovery | Time to acquisition after owner kill, stale renew result | 60.064 s with 60 s presence | <= configured remaining TTL + 2 s test tolerance; stale renewal rejected |
| Delivery-to-handling | DB send through matching recipient ack; monotonic harness timestamps | Provider/workload dependent | Report p50/p95 separately by vendor; no pooled speed claim |
| Storage growth | DB/WAL/document bytes at 0/1k/10k messages and query p95 | Unmeasured at plan freeze | Publish sizes before choosing retention defaults |
| Provider telemetry coverage | Available required samples / expected samples | 24/24 in prior scheduling run | Missing values stay unknown; fail a claim requiring absent telemetry |

The service harness freezes a more explicit CLI guard: candidate p95 <= baseline p95 × 1.25 + 5 ms. The 20% threshold above remains an investigation signal, not a redefined executable gate.

The previous scheduling comparison increased all completion medians. Its
exploratory gate was candidate <=2× baseline +2 seconds, not improved speed.
Do not reuse that loose bound as a general service latency objective.

## Reproduction and evidence

Run from the monorepo root. Build the served artifact after changing prompts,
protocol docs, catalog or Rust; the binary embeds these sources.

```sh
yarn workspace @octocodeai/octocode-agents-communication build:release
COMMUNICATION_PYTHON=/absolute/python3.14-with-recent-sqlite \
  yarn workspace @octocodeai/octocode-agents-communication verify
yarn workspace @octocodeai/octocode-agents-communication benchmark:runtime
COMMUNICATION_PI_MODEL=PROVIDER/MODEL \
  yarn workspace @octocodeai/octocode-agents-communication benchmark:scheduling
COMMUNICATION_WORKERS_PER_VENDOR=2 COMMUNICATION_PI_MODEL=PROVIDER/MODEL \
  yarn workspace @octocodeai/octocode-agents-communication poc:mesh
yarn workspace @octocodeai/octocode-agents-communication poc:attached
yarn workspace @octocodeai/octocode-agents-communication poc:crash
node skills/octocode-skills/scripts/skill-review.mjs \
  packages/octocode-agents-communication/skills/octocode-agents-communication
```

Set `COMMUNICATION_PYTHON` for DB-only conformance rather than accepting its
explicit skip. An unavailable vendor fails the selected live matrix; do not omit
it and report “all agents.” Exact adapter-specific environment setup belongs in
the respective probe's help and [README](../README.md).

Record: platform, vendor/model versions, binary/skill/catalog hashes, test command,
payload bytes, database size, sample count, warmup, order, concurrency, startup
time, p50/p95, failures, unknown counters and owned-process exit status. Native API
fixtures prove wire handling; only actual hosts prove supported live behavior.
The mesh proves cooperative message/lease flows, not authorization against a
malicious same-user process or correctness on every coding task.

For model comparisons, balance baseline/candidate order, use fresh authorized
workers, freeze identical legitimate tasks, retain failed runs and raw receipts,
and separate design trials from confirmation cases. A changed evaluator restarts
both arms. Use deterministic IDs, acknowledgements, documents and lease results as
the grader; an agent saying “done” is insufficient. Record cache reads/writes and
provider totals separately. No pooled monetary claim without a dated price basis.

## Rollout and next decision

1. Preserve/export existing data and stop writers before a version migration.
2. Build, run deterministic migration/retry/expiry/visibility/transport tests, then
   use the real standalone skill and persistent tool path.
3. Run native attached delivery and the chosen Codex/Claude/Pi matrix; inspect DB
   acknowledgements, context duplication, correlated replies and process cleanup.
4. Publish measured outcomes beside unmeasured/backlog rows. Keep schema-compatible
   artifacts together; never mix old clients with the new schema.
5. Move callers to persistent access only after its measured win. Introduce a
   workspace daemon, richer lifecycle or destructive retention only when the
   corresponding unmeasured workload identifies a concrete need.

Rollback is artifact restoration plus a separately preserved old database, with
all workers stopped. No in-place schema downgrade is promised. Never erase a live
store to make a test or client pass.


## Current iteration evidence ledger

The selected final artifact passed the service comparison and live mesh. The
intermediate measurements below remain preserved; export and document-error
repairs followed them. [SERVICE_EVALUATION.md](SERVICE_EVALUATION.md) records the
final binary/skill hashes and repeated same-path comparison for that exact binary.

### Intermediate persistent service and CLI

`service-benchmark.mjs` records 60 sends per mode after five warmups, alternates
CLI/MCP ordering within a run, and reports persistent startup separately. Both
artifacts must execute at the **same absolute executable path** in separate runs;
copy the selected frozen binary there only after the preceding run has exited.
Use unique output directories, retain manifests/hashes, and run whole-run AB/BA
order (baseline-a, candidate-a, candidate-b, baseline-b).

```sh
COMMUNICATION_BINARY=/absolute/lab/benchmark-executable \
COMMUNICATION_SAMPLES=60 \
COMMUNICATION_OUTPUT=/absolute/results/run-name/result.json \
node packages/octocode-agents-communication/scripts/service-benchmark.mjs

COMMUNICATION_BINARY=/absolute/lab/benchmark-executable \
COMMUNICATION_OUTPUT=/absolute/results/storage-run/result.json \
node packages/octocode-agents-communication/scripts/storage-benchmark.mjs
```

The earlier unmatched-path baseline/candidate run failed the CLI latency guard.
Its artifacts are retained; executable location confounded process-launch timing,
so it does not isolate the optimization's effect. The corrected matched-path
runs use `/tmp/octocode-service-work/benchmark-executable` in both arms.

| Pair | MCP send p50: baseline → candidate | Reduction | CLI send p50: baseline → candidate | CLI p95: baseline → candidate | Gates |
| --- | ---: | ---: | ---: | ---: | --- |
| A (AB) | 4.574 → 0.284 ms | 93.80% | 13.402 → 11.506 ms | 14.573 → 12.506 ms | Pass |
| B (BA) | 4.589 → 0.242 ms | 94.72% | 13.391 → 11.190 ms | 14.783 → 11.406 ms | Pass |

Both pairs pass candidate warm MCP versus baseline warm MCP's predeclared >=20%
improvement and CLI p95 guard. Separately, within the candidate, warm MCP send p50
was 0.284 ms versus CLI 11.506 ms in A, and 0.242 versus 11.190 ms in B. That
access-mode comparison excludes MCP's one-time startup; it is not the primary
before/after optimization claim.
All four runs report zero model calls, loss or duplicates; all deliveries were
acknowledged and owned MCP processes exited with code 0. These are local
microbenchmarks of storing/reading messages, not recipient inference or agent
quality. Do not compare their CLI times causally with the historical launcher
benchmark above: executable path, artifact and harness differ.

Artifacts under `.octocode/benchmarks/communication-service/results/`:
`2026-09-25-matched-baseline-a/`, `2026-09-25-matched-candidate-a/`,
`2026-09-25-matched-candidate-b/`, `2026-09-25-matched-baseline-b/`;
each contains `manifest.json` and `result.json` with subject/harness hashes.

### Retained storage series

One model-free candidate run retained unacknowledged self-deliveries with 1,024-byte
bodies, including audit. Thirty first-inbox-page queries were sampled per point.
Values are observed file sizes, not compressed payload estimates or whole-inbox
scan times. WAL size varies with checkpoints and concurrent reader lifetime.

| Messages | Audit rows | DB bytes | WAL bytes | First-page p50 / p95 |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 118,784 | 0 | 0.081 / 0.223 ms |
| 1,000 | 2,001 | 2,166,784 | 4,177,712 | 1.312 / 1.383 ms |
| 10,000 | 20,001 | 21,946,368 | 4,210,672 | 5.045 / 5.282 ms |

The run passed and its MCP process exited with code 0. Evidence:
`.octocode/benchmarks/communication-storage/results/2026-09-25T13-13-51.565Z/result.json`.
This is a descriptive single run; it supports measuring retention needs and does
not establish unlimited scalability or justify deleting history. Export is
implemented; destructive compaction and document retention remain backlog.

### Live integration and prompt status

Four development mesh runs failed and remain part of the evidence. Each failure
changed the next implementation or investigation step; none can be relabelled as
an overall pass because some transport checks succeeded.

| Run on 2026-09-25 (UTC) | Observed failure | Response / interpretation |
| --- | --- | --- |
| `13-09-05.953Z` | Pi omitted `.md` while reading a document and answered without the required document evidence | Bounded missing-document suggestions, exact filename guidance and a successful native-read grader guard |
| `13-17-49.178Z` | Cross-vendor acknowledgement timeout; Codex's MCP was ready but it made no tool calls | Cause uncertain; retain traces and investigate host/tool use, without attributing it to the DB |
| `13-23-56.840Z` | Pi mistyped the recipient UUID, then acknowledged the request after its send failed; one required answer was absent | Transactional `replyTo`-only routing and required-reply-success before ack |
| `13-30-30.101Z` | All 42 required answers and six document reads passed, broadcast completed and no messages were pending; Codex additionally answered raw `ANSWER` message 45, producing 86 messages instead of 85 | Clarify that `wake:action` schedules handling; the body determines whether a reply is requested. Apply the rule consistently to native/Pi envelopes and the skill |

Artifacts live under `.octocode/benchmarks/communication-service-mesh/results/`
with directory names `2026-09-25T` plus the timestamps above. Their `result.json`
files retain failing assertions; traces and saved harnesses identify the exercised
candidate. The fourth run failed the no-reply-loop guard despite completing every
required exchange. This separates transport completeness from conversational
correctness.

The selected `2026-09-25T13-33-48.378Z` run passed: 42 requests/replies, seven
broadcast recipients, 85 messages, 91 submitted/acknowledged deliveries, all six
required document reads, no pending/extra messages and all children reaped.
All four failed runs and the passing run retain exported DB snapshots, document
files, manifests and traces. These exposed cases are development data: this final
pass is a bounded regression result, not an independent held-out reliability
estimate or a promise about arbitrary agent tasks.

The final served skill is 6,465 bytes / 35 lines versus the 7,632-byte / 43-line
baseline (15.29% fewer bytes). It clarifies reply-before-ack and body intent versus
wake scheduling. The byte/line gate passes; byte reduction is not a token-saving
measurement. Final binary hash, behavioral evidence, complete-version performance
comparison and scoped usage observations are in [SERVICE_EVALUATION.md](SERVICE_EVALUATION.md).

## Follow-up implementation (2026-09-25)

The tables above retain their historical acceptance gates. This follow-up adds:

- Read-only idle hooks, preserving mutation and native-owner checks; see [measured contention results](HOOK_EVALUATION.md).
- Fixed optional tool exposure across MCP, managed workers and native Pi; four communication descriptors use 3,986 serialized bytes versus 12,625 for the full 13-tool catalog. This measures exposed JSON, not cache savings.
- Bounded retention diagnostics and explicit SQLite compaction preserving every protocol record; destructive age-based retention remains unsupported. See [storage results](RETENTION.md).
- [150-cycle recovery evaluation](RECOVERY_EVALUATION.md), including a real presence-window crossing, plus repeated natural lease-expiry checks.
- [Actual shared-file collaboration](CONTEXT_OPTIMIZATION.md), grading unleased writes independently from functional correctness. The skill remains under 50 lines but exceeds this plan's historical 6,487-byte target; preserve that missed size gate rather than removing recovery/authority rules to claim a pass.

Production rollout still needs longer operational runs and platform/vendor-version coverage. A short successful fixture is not a durability guarantee.
